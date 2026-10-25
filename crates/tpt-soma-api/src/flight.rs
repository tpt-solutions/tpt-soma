use bytes::Bytes;
use ed25519_dalek::VerifyingKey;
use std::sync::Arc;
use thiserror::Error;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};
use tpt_soma_capability::RevocationList;
use tpt_soma_core::connection::PgPool;
use tpt_soma_core::query::{
    get_cgm_readings_by_subject, get_clinical_observations_by_subject, get_expression_by_sample,
    get_umap_by_sample, get_variants_by_sample,
};
use tpt_soma_flight::data::flight_data_for;
use tpt_soma_flight::ipc::StreamDecoder;
use tpt_soma_flight::pb::{
    FlightData, FlightDescriptor, FlightEndpoint, FlightInfo, HandshakeRequest, HandshakeResponse,
    PutResult, SchemaResult, Ticket,
};
use tpt_soma_flight::service::{FlightService, FlightServiceServer};
use tpt_soma_flight::{Column, DataType, Field, RecordBatch, Schema};

use crate::auth::{AuthError, authenticate_bearer};
use tpt_soma_simulacrum::storage as sim_storage;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum FlightError {
    #[error("columnar error: {0}")]
    Columnar(String),
    #[error("database error: {0}")]
    Database(String),
}

impl From<tpt_soma_flight::Error> for FlightError {
    fn from(e: tpt_soma_flight::Error) -> Self {
        FlightError::Columnar(e.to_string())
    }
}

pub struct FlightServer {
    pub schema: Arc<Schema>,
    pub pool: PgPool,
    pub verifying_key: VerifyingKey,
    pub revocation_list: Arc<RevocationList>,
}

impl FlightServer {
    pub async fn run(
        self,
        addr: std::net::SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let svc = FlightServiceServer::new(TptSomaFlightService {
            pool: self.pool,
            verifying_key: self.verifying_key,
            revocation_list: self.revocation_list,
        });

        Server::builder().add_service(svc).serve(addr).await?;

        Ok(())
    }
}

struct TptSomaFlightService {
    pool: PgPool,
    verifying_key: VerifyingKey,
    revocation_list: Arc<RevocationList>,
}

/// Resource classes a given Flight data type may be read under (TM-02).
fn flight_data_type_classes(data_type: &str) -> &'static [&'static str] {
    match data_type {
        "variants" => &["genomic_variant"],
        "expression" | "umap" => &["transcriptomic_scrna"],
        "clinical_observations" => &["clinical_observation"],
        "cgm" => &["cgm_continuous"],
        "simulation" => &["simulation_output"],
        _ => &[],
    }
}

fn auth_error_to_status(e: AuthError) -> Status {
    let (code, msg) = match e {
        AuthError::MissingAuthHeader => {
            (tonic::Code::Unauthenticated, "missing authorization header")
        }
        AuthError::InvalidAuthHeader => {
            (tonic::Code::Unauthenticated, "invalid authorization header")
        }
        AuthError::InvalidTokenFormat => (tonic::Code::Unauthenticated, "invalid token format"),
        AuthError::InvalidSignature => (tonic::Code::Unauthenticated, "invalid token signature"),
        AuthError::TokenExpired => (tonic::Code::Unauthenticated, "token expired"),
        AuthError::TokenRevoked => (tonic::Code::Unauthenticated, "token revoked"),
        AuthError::InsufficientScope => (tonic::Code::PermissionDenied, "insufficient scope"),
    };
    Status::new(code, msg)
}

/// Require a valid capability token on every Flight call and enforce the
/// resource-class policy for the requested data type.
async fn authorize_flight_call(
    auth_header: Option<&str>,
    verifying_key: &VerifyingKey,
    revocation_list: &Arc<RevocationList>,
    data_type: &str,
) -> Result<(), Status> {
    let token = authenticate_bearer(auth_header, verifying_key, revocation_list)
        .await
        .map_err(auth_error_to_status)?;

    if token.action != "read" && token.action != "export" && token.action != "admin" {
        return Err(Status::permission_denied("Flight only serves read access"));
    }

    let allowed = flight_data_type_classes(data_type);
    if allowed.is_empty() || !allowed.contains(&token.resource_class.as_str()) {
        return Err(Status::permission_denied(
            "token not authorized for this data type",
        ));
    }

    Ok(())
}

/// Records a device-state write attempt. Used for refusals and failures, where
/// the write did not happen. A failure here is logged, not returned.
async fn record_best_effort(
    ledger: &tpt_soma_audit::AuditLedger,
    actor: &str,
    outcome: &str,
    rows: u64,
) {
    let event = crate::device_ingest::audit_event(actor, outcome, rows);
    if let Err(e) = ledger.append(event).await {
        tracing::error!(error = %e, outcome, "device_state audit write failed");
    }
}

/// Drains decoded batches into the device-state table, one batch at a time.
///
/// `first` is the opening message of the put stream, which carries the schema.
/// Each valid batch is audited before it is written. If that audit record
/// cannot be written, the batch is not written (fail closed). A bad batch
/// stops the put. Batches before it are already committed, and each batch is
/// its own transaction. Returns the rows written.
async fn ingest_batches(
    pool: &PgPool,
    ledger: &tpt_soma_audit::AuditLedger,
    actor: &str,
    first: FlightData,
    stream: &mut Streaming<FlightData>,
) -> Result<u64, Status> {
    let mut decoder = StreamDecoder::new();
    let mut total = 0u64;
    let mut message = Some(first);
    while let Some(m) = message.take() {
        let batch = decoder
            .push(&m.data_header, &m.data_body)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        if let Some(batch) = batch {
            let rows = match crate::device_ingest::observations_from_batch(&batch) {
                Ok(rows) => rows,
                Err(e) => {
                    record_best_effort(ledger, actor, "rejected", 0).await;
                    return Err(Status::invalid_argument(e.to_string()));
                }
            };
            let count = rows.len() as u64;
            ledger
                .append(crate::device_ingest::audit_event(actor, "accepted", count))
                .await
                .map_err(|e| Status::internal(format!("audit write failed: {e}")))?;
            match crate::device_ingest::insert(pool, &rows).await {
                Ok(n) => total += n,
                Err(e) => {
                    record_best_effort(ledger, actor, "failed", count).await;
                    return Err(Status::internal(e.to_string()));
                }
            }
        }
        message = stream.message().await?;
    }
    Ok(total)
}

impl TptSomaFlightService {
    async fn authorize<R>(&self, request: &Request<R>, data_type: &str) -> Result<(), Status> {
        let auth_header = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok());

        authorize_flight_call(
            auth_header,
            &self.verifying_key,
            &self.revocation_list,
            data_type,
        )
        .await
    }
}

/// Schema served for each Flight data type.
fn schema_for(data_type: &str) -> Option<Schema> {
    let utf8 = |name: &str, nullable| Field::new(name, DataType::Utf8, nullable);
    let fields = match data_type {
        "variants" => vec![
            utf8("variant_id", false),
            utf8("chromosome", false),
            Field::new("position", DataType::Int32, false),
            utf8("reference", false),
            utf8("alternate", false),
            utf8("rsid", true),
            utf8("clinvar_id", true),
            utf8("genotype", true),
        ],
        "expression" => vec![
            utf8("sample_id", false),
            utf8("cell_id", false),
            utf8("gene_id", false),
            Field::new("count", DataType::Int32, false),
        ],
        "umap" => vec![
            utf8("sample_id", false),
            utf8("cell_id", false),
            Field::new("umap1", DataType::Float64, false),
            Field::new("umap2", DataType::Float64, false),
            utf8("cluster", false),
        ],
        "clinical_observations" => vec![
            utf8("subject_id", false),
            utf8("loinc_code", false),
            Field::new("value", DataType::Float64, false),
            utf8("unit", true),
            utf8("effective_time", false),
            utf8("status", false),
            utf8("source", false),
        ],
        "cgm" => vec![
            utf8("subject_id", false),
            utf8("ts", false),
            Field::new("glucose_mgdl", DataType::Float64, false),
            utf8("source", false),
            utf8("sensor_id", true),
            Field::new("is_calibrated", DataType::Boolean, false),
        ],
        "simulation" => vec![
            utf8("run_id", false),
            utf8("ts", false),
            utf8("series_name", false),
            Field::new("value", DataType::Float64, false),
        ],
        _ => return None,
    };
    Some(Schema::new(fields))
}

/// Non-null UTF-8 column built from a record field.
fn text<R>(records: &[R], f: impl Fn(&R) -> String) -> Column {
    Column::utf8(records.iter().map(f))
}

/// Looks up the requested data and shapes it as one record batch.
async fn load_batch(pool: &PgPool, data_type: &str, id: &str) -> Result<RecordBatch, Status> {
    let internal = |e: &dyn std::fmt::Display| Status::internal(e.to_string());
    match data_type {
        "variants" => {
            let r = get_variants_by_sample(pool, id)
                .await
                .map_err(|e| internal(&e))?;
            variants_to_batch(r).map_err(|e| internal(&e))
        }
        "expression" => {
            let r = get_expression_by_sample(pool, id)
                .await
                .map_err(|e| internal(&e))?;
            expression_to_batch(r).map_err(|e| internal(&e))
        }
        "umap" => {
            let r = get_umap_by_sample(pool, id)
                .await
                .map_err(|e| internal(&e))?;
            umap_to_batch(r).map_err(|e| internal(&e))
        }
        "clinical_observations" => {
            let r = get_clinical_observations_by_subject(pool, id)
                .await
                .map_err(|e| internal(&e))?;
            clinical_observations_to_batch(r).map_err(|e| internal(&e))
        }
        "cgm" => {
            let r = get_cgm_readings_by_subject(pool, id)
                .await
                .map_err(|e| internal(&e))?;
            cgm_to_batch(r).map_err(|e| internal(&e))
        }
        "simulation" => {
            let run_uuid = Uuid::parse_str(id)
                .map_err(|e| Status::invalid_argument(format!("invalid run_id: {}", e)))?;
            let r = sim_storage::get_simulation_outputs(pool, run_uuid)
                .await
                .map_err(|e| internal(&e))?;
            simulation_to_batch(r).map_err(|e| internal(&e))
        }
        _ => Err(Status::invalid_argument("Unknown data type")),
    }
}

#[tonic::async_trait]
impl FlightService for TptSomaFlightService {
    type HandshakeStream =
        tokio_stream::wrappers::ReceiverStream<Result<HandshakeResponse, Status>>;
    type DoGetStream = tokio_stream::wrappers::ReceiverStream<Result<FlightData, Status>>;
    type DoPutStream = tokio_stream::wrappers::ReceiverStream<Result<PutResult, Status>>;

    async fn handshake(
        &self,
        _request: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(Ok(HandshakeResponse {
            protocol_version: 0,
            payload: Bytes::new(),
        }))
        .await
        .ok();
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn get_flight_info(
        &self,
        request: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        let descriptor = request.get_ref().clone();
        let cmd = String::from_utf8_lossy(&descriptor.cmd).to_string();

        let parts: Vec<&str> = cmd.split(':').collect();
        if parts.len() != 2 {
            return Err(Status::invalid_argument("Invalid command format"));
        }

        let (data_type, _sample_id) = (parts[0], parts[1]);

        self.authorize(&request, data_type).await?;

        let schema =
            schema_for(data_type).ok_or_else(|| Status::invalid_argument("Unknown data type"))?;

        let mut flight_info = FlightInfo::new()
            .with_schema(&schema)
            .with_endpoint(FlightEndpoint {
                ticket: Some(Ticket::new(cmd.into_bytes())),
                ..Default::default()
            })
            .with_total_records(-1)
            .with_total_bytes(-1);
        // Standard clients (Arrow C++, pyarrow) reject a FlightInfo without it.
        flight_info.flight_descriptor = Some(descriptor);

        Ok(Response::new(flight_info))
    }

    async fn get_schema(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status> {
        Err(Status::unimplemented("get_schema not implemented"))
    }

    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        let cmd = String::from_utf8_lossy(&request.get_ref().ticket).to_string();
        let parts: Vec<&str> = cmd.split(':').collect();

        if parts.len() != 2 {
            return Err(Status::invalid_argument("Invalid ticket format"));
        }

        let data_type = parts[0].to_string();
        let sample_id = parts[1].to_string();

        self.authorize(&request, &data_type).await?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let pool = self.pool.clone();
        tokio::spawn(async move {
            // Each result is (schema FlightData, batch FlightData) per the Flight
            // wire protocol: the schema must be sent as its own message before
            // any record batches so standard clients (pyarrow.flight) can
            // reassemble the stream.
            let result = match load_batch(&pool, &data_type, &sample_id).await {
                Ok(batch) => flight_data_for(&batch).map_err(|e| Status::internal(e.to_string())),
                Err(status) => Err(status),
            };

            match result {
                Ok((schema_flight_data, batch_flight_data)) => {
                    let _ = tx.send(Ok(schema_flight_data)).await;
                    let _ = tx.send(Ok(batch_flight_data)).await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn do_put(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        // Writes are a separate policy from reads (TM-02). Only device-state
        // ingest is accepted, and only with the `ingest` action.
        let auth_header = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let token = authenticate_bearer(
            auth_header.as_deref(),
            &self.verifying_key,
            &self.revocation_list,
        )
        .await
        .map_err(auth_error_to_status)?;
        let ledger = tpt_soma_audit::AuditLedger::new(self.pool.clone());
        if let Err(status) = crate::device_ingest::check_scope(&token.action, &token.resource_class)
        {
            record_best_effort(&ledger, &token.subject, "denied", 0).await;
            return Err(status);
        }

        let mut stream = request.into_inner();
        let first = stream
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("empty put stream"))?;
        let descriptor = first
            .flight_descriptor
            .clone()
            .ok_or_else(|| Status::invalid_argument("first message needs a descriptor"))?;
        let subject = String::from_utf8_lossy(&descriptor.cmd).to_string();
        if subject != crate::device_ingest::SUBJECT {
            return Err(Status::invalid_argument(
                "do_put only accepts the device_state subject",
            ));
        }

        let pool = self.pool.clone();
        let actor = token.subject.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let outcome = ingest_batches(&pool, &ledger, &actor, first, &mut stream).await;
            let msg = outcome.map(|rows| PutResult {
                app_metadata: Bytes::from(format!("{{\"rows\":{rows}}}")),
            });
            let _ = tx.send(msg).await;
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

fn variants_to_batch(
    records: Vec<tpt_soma_core::query::VariantRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(schema_for("variants").expect("known data type"));
    let columns = vec![
        text(&records, |r| r.variant_id.clone()),
        text(&records, |r| r.chromosome.clone()),
        Column::int32(records.iter().map(|r| r.position)),
        text(&records, |r| r.reference.clone()),
        text(&records, |r| r.alternate.clone()),
        Column::utf8_opt(records.iter().map(|r| r.rsid.clone())),
        Column::utf8_opt(records.iter().map(|r| r.clinvar_id.clone())),
        Column::utf8_opt(records.iter().map(|r| r.genotype.clone())),
    ];
    Ok(RecordBatch::try_new(schema, columns)?)
}

fn expression_to_batch(
    records: Vec<tpt_soma_core::query::ExpressionRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(schema_for("expression").expect("known data type"));
    let columns = vec![
        text(&records, |r| r.sample_id.to_string()),
        text(&records, |r| r.cell_id.clone()),
        text(&records, |r| r.gene_id.clone()),
        Column::int32(records.iter().map(|r| r.count)),
    ];
    Ok(RecordBatch::try_new(schema, columns)?)
}

fn umap_to_batch(
    records: Vec<tpt_soma_core::query::UmapRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(schema_for("umap").expect("known data type"));
    let columns = vec![
        text(&records, |r| r.sample_id.to_string()),
        text(&records, |r| r.cell_id.clone()),
        Column::float64(records.iter().map(|r| r.umap1)),
        Column::float64(records.iter().map(|r| r.umap2)),
        text(&records, |r| r.cluster.clone()),
    ];
    Ok(RecordBatch::try_new(schema, columns)?)
}

fn clinical_observations_to_batch(
    records: Vec<tpt_soma_core::query::ClinicalObservationRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(schema_for("clinical_observations").expect("known data type"));
    let columns = vec![
        text(&records, |r| r.subject_id.clone()),
        text(&records, |r| r.loinc_code.clone()),
        Column::float64(records.iter().map(|r| r.value)),
        Column::utf8_opt(records.iter().map(|r| r.unit.clone())),
        text(&records, |r| r.effective_time.to_rfc3339()),
        text(&records, |r| r.status.clone()),
        text(&records, |r| r.source.clone()),
    ];
    Ok(RecordBatch::try_new(schema, columns)?)
}

fn cgm_to_batch(
    records: Vec<tpt_soma_core::query::CgmReadingRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(schema_for("cgm").expect("known data type"));
    let columns = vec![
        text(&records, |r| r.subject_id.clone()),
        text(&records, |r| r.ts.to_rfc3339()),
        Column::float64(records.iter().map(|r| r.glucose_mgdl)),
        text(&records, |r| r.source.clone()),
        Column::utf8_opt(records.iter().map(|r| r.sensor_id.clone())),
        Column::boolean(records.iter().map(|r| r.is_calibrated)),
    ];
    Ok(RecordBatch::try_new(schema, columns)?)
}

fn simulation_to_batch(
    records: Vec<sim_storage::SimulationOutput>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(schema_for("simulation").expect("known data type"));
    let columns = vec![
        text(&records, |r| r.run_id.to_string()),
        text(&records, |r| r.ts.to_rfc3339()),
        text(&records, |r| r.series_name.clone()),
        Column::float64(records.iter().map(|r| r.value)),
    ];
    Ok(RecordBatch::try_new(schema, columns)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_soma_flight::data::flight_data_to_batches;

    fn sample_variant_batch() -> RecordBatch {
        let schema = Arc::new(schema_for("variants").unwrap());
        RecordBatch::try_new(
            schema,
            vec![
                Column::utf8(["1:100:A:T", "2:300:T:A"]),
                Column::utf8(["1", "2"]),
                Column::int32([100, 300]),
                Column::utf8(["A", "T"]),
                Column::utf8(["T", "A"]),
                Column::utf8_opt([Some("rs123"), None]),
                Column::utf8_opt([Some("VCV000123"), None]),
                Column::utf8_opt([Some("0/1"), None]),
            ],
        )
        .unwrap()
    }

    #[test]
    fn test_flight_data_round_trip() {
        let batch = sample_variant_batch();
        let (schema_data, batch_data) = flight_data_for(&batch).unwrap();

        // The schema message must be carried in the header so standard clients
        // (pyarrow.flight) can reassemble the stream.
        assert!(!schema_data.data_header.is_empty());
        assert!(!batch_data.data_header.is_empty());

        let decoded = flight_data_to_batches(&[schema_data, batch_data]).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0], batch);

        let variants = decoded[0].column(0).as_utf8().unwrap();
        assert_eq!(variants[0].as_deref(), Some("1:100:A:T"));
        assert_eq!(variants[1].as_deref(), Some("2:300:T:A"));
    }

    #[test]
    fn test_every_data_type_has_a_schema() {
        for name in [
            "variants",
            "expression",
            "umap",
            "clinical_observations",
            "cgm",
            "simulation",
        ] {
            assert!(schema_for(name).is_some(), "{name}");
        }
        assert!(schema_for("nope").is_none());
    }

    #[test]
    fn test_simulation_batch_round_trip() {
        let outputs = vec![
            sim_storage::SimulationOutput {
                id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                ts: chrono::Utc::now(),
                series_name: "igf1".to_string(),
                value: 12.5,
            },
            sim_storage::SimulationOutput {
                id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                ts: chrono::Utc::now(),
                series_name: "breast".to_string(),
                value: 3.25,
            },
        ];
        let batch = simulation_to_batch(outputs).unwrap();
        let (schema_data, batch_data) = flight_data_for(&batch).unwrap();
        let decoded = flight_data_to_batches(&[schema_data, batch_data]).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].num_rows(), 2);
        let series = decoded[0].column(2).as_utf8().unwrap();
        assert_eq!(series[0].as_deref(), Some("igf1"));
        assert_eq!(series[1].as_deref(), Some("breast"));
    }

    #[tokio::test]
    async fn test_flight_authorize_requires_token() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = key.verifying_key();
        let revocation_list = Arc::new(RevocationList::new());

        let result = authorize_flight_call(None, &verifying_key, &revocation_list, "variants")
            .await
            .unwrap_err();
        assert_eq!(result.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn test_flight_authorize_valid_token() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = key.verifying_key();
        let revocation_list = Arc::new(RevocationList::new());

        let token = crate::auth::test_helpers::signed_token(
            &key,
            "researcher-1",
            "genomic_variant",
            vec!["cohort-a".to_string()],
            "read",
            [1u8; 32],
        );

        authorize_flight_call(
            Some(&format!("Bearer {}", token)),
            &verifying_key,
            &revocation_list,
            "variants",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_flight_authorize_wrong_data_class_rejected() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = key.verifying_key();
        let revocation_list = Arc::new(RevocationList::new());

        let token = crate::auth::test_helpers::signed_token(
            &key,
            "researcher-1",
            "clinical_observation",
            vec!["cohort-a".to_string()],
            "read",
            [1u8; 32],
        );

        let result = authorize_flight_call(
            Some(&format!("Bearer {}", token)),
            &verifying_key,
            &revocation_list,
            "variants",
        )
        .await
        .unwrap_err();
        assert_eq!(result.code(), tonic::Code::PermissionDenied);
    }

    #[tokio::test]
    async fn test_flight_authorize_simulation_data_class() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = key.verifying_key();
        let revocation_list = Arc::new(RevocationList::new());

        let token = crate::auth::test_helpers::signed_token(
            &key,
            "researcher-1",
            "simulation_output",
            vec!["cohort-a".to_string()],
            "read",
            [1u8; 32],
        );

        authorize_flight_call(
            Some(&format!("Bearer {}", token)),
            &verifying_key,
            &revocation_list,
            "simulation",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_flight_authorize_write_action_rejected() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = key.verifying_key();
        let revocation_list = Arc::new(RevocationList::new());

        let token = crate::auth::test_helpers::signed_token(
            &key,
            "researcher-1",
            "genomic_variant",
            vec!["cohort-a".to_string()],
            "write",
            [1u8; 32],
        );

        let result = authorize_flight_call(
            Some(&format!("Bearer {}", token)),
            &verifying_key,
            &revocation_list,
            "variants",
        )
        .await
        .unwrap_err();
        assert_eq!(result.code(), tonic::Code::PermissionDenied);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_flight_server() {
        // Full server round-trip (do_get over the wire) is covered by the
        // end-to-end integration test in tests/e2e_flight.rs
    }
}
