use arrow_array::{BooleanArray, Float64Array, Int32Array, RecordBatch, StringArray};
use arrow_flight::flight_service_server::FlightServiceServer;
use arrow_ipc::writer::{DictionaryTracker, IpcDataGenerator, IpcWriteContext, IpcWriteOptions};
use arrow_schema::{DataType, Field, Schema};
use bytes::Bytes;
use ed25519_dalek::VerifyingKey;
use std::sync::Arc;
use thiserror::Error;
use tonic::transport::Server;
use tpt_soma_capability::RevocationList;
use tpt_soma_core::connection::PgPool;
use tpt_soma_core::query::{
    get_cgm_readings_by_subject, get_clinical_observations_by_subject, get_expression_by_sample,
    get_umap_by_sample, get_variants_by_sample,
};

use crate::auth::{AuthError, authenticate_bearer};

#[derive(Debug, Error)]
pub enum FlightError {
    #[error("arrow error: {0}")]
    Arrow(String),
    #[error("database error: {0}")]
    Database(String),
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
        _ => &[],
    }
}

fn auth_error_to_status(e: AuthError) -> tonic::Status {
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
    tonic::Status::new(code, msg)
}

/// Require a valid capability token on every Flight call and enforce the
/// resource-class policy for the requested data type.
async fn authorize_flight_call(
    auth_header: Option<&str>,
    verifying_key: &VerifyingKey,
    revocation_list: &Arc<RevocationList>,
    data_type: &str,
) -> Result<(), tonic::Status> {
    let token = authenticate_bearer(auth_header, verifying_key, revocation_list)
        .await
        .map_err(auth_error_to_status)?;

    if token.action != "read" && token.action != "export" && token.action != "admin" {
        return Err(tonic::Status::permission_denied(
            "Flight only serves read access",
        ));
    }

    let allowed = flight_data_type_classes(data_type);
    if allowed.is_empty() || !allowed.contains(&token.resource_class.as_str()) {
        return Err(tonic::Status::permission_denied(
            "token not authorized for this data type",
        ));
    }

    Ok(())
}

impl TptSomaFlightService {
    async fn authorize<R>(
        &self,
        request: &tonic::Request<R>,
        data_type: &str,
    ) -> Result<(), tonic::Status> {
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

#[tonic::async_trait]
impl arrow_flight::flight_service_server::FlightService for TptSomaFlightService {
    type HandshakeStream = tokio_stream::wrappers::ReceiverStream<
        Result<arrow_flight::HandshakeResponse, tonic::Status>,
    >;
    type ListFlightsStream =
        tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::FlightInfo, tonic::Status>>;
    type DoGetStream =
        tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::FlightData, tonic::Status>>;
    type DoPutStream =
        tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::PutResult, tonic::Status>>;
    type DoActionStream =
        tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::Result, tonic::Status>>;
    type ListActionsStream =
        tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::ActionType, tonic::Status>>;
    type DoExchangeStream =
        tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::FlightData, tonic::Status>>;

    async fn handshake(
        &self,
        _request: tonic::Request<tonic::Streaming<arrow_flight::HandshakeRequest>>,
    ) -> Result<tonic::Response<Self::HandshakeStream>, tonic::Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(Ok(arrow_flight::HandshakeResponse {
            protocol_version: 0,
            payload: Bytes::new(),
        }))
        .await
        .ok();
        Ok(tonic::Response::new(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }

    async fn list_flights(
        &self,
        _request: tonic::Request<arrow_flight::Criteria>,
    ) -> Result<tonic::Response<Self::ListFlightsStream>, tonic::Status> {
        Err(tonic::Status::unimplemented("list_flights not implemented"))
    }

    async fn get_flight_info(
        &self,
        request: tonic::Request<arrow_flight::FlightDescriptor>,
    ) -> Result<tonic::Response<arrow_flight::FlightInfo>, tonic::Status> {
        let cmd = String::from_utf8_lossy(&request.get_ref().cmd).to_string();

        let parts: Vec<&str> = cmd.split(':').collect();
        if parts.len() != 2 {
            return Err(tonic::Status::invalid_argument("Invalid command format"));
        }

        let (data_type, _sample_id) = (parts[0], parts[1]);

        self.authorize(&request, data_type).await?;

        let schema = match data_type {
            "variants" => Arc::new(Schema::new(vec![
                Field::new("variant_id", DataType::Utf8, false),
                Field::new("chromosome", DataType::Utf8, false),
                Field::new("position", DataType::Int32, false),
                Field::new("reference", DataType::Utf8, false),
                Field::new("alternate", DataType::Utf8, false),
                Field::new("rsid", DataType::Utf8, true),
                Field::new("clinvar_id", DataType::Utf8, true),
                Field::new("genotype", DataType::Utf8, true),
            ])),
            "expression" => Arc::new(Schema::new(vec![
                Field::new("sample_id", DataType::Utf8, false),
                Field::new("cell_id", DataType::Utf8, false),
                Field::new("gene_id", DataType::Utf8, false),
                Field::new("count", DataType::Int32, false),
            ])),
            "umap" => Arc::new(Schema::new(vec![
                Field::new("sample_id", DataType::Utf8, false),
                Field::new("cell_id", DataType::Utf8, false),
                Field::new("umap1", DataType::Float64, false),
                Field::new("umap2", DataType::Float64, false),
                Field::new("cluster", DataType::Utf8, false),
            ])),
            "clinical_observations" => Arc::new(Schema::new(vec![
                Field::new("subject_id", DataType::Utf8, false),
                Field::new("loinc_code", DataType::Utf8, false),
                Field::new("value", DataType::Float64, false),
                Field::new("unit", DataType::Utf8, true),
                Field::new("effective_time", DataType::Utf8, false),
                Field::new("status", DataType::Utf8, false),
                Field::new("source", DataType::Utf8, false),
            ])),
            "cgm" => Arc::new(Schema::new(vec![
                Field::new("subject_id", DataType::Utf8, false),
                Field::new("ts", DataType::Utf8, false),
                Field::new("glucose_mgdl", DataType::Float64, false),
                Field::new("source", DataType::Utf8, false),
                Field::new("sensor_id", DataType::Utf8, true),
                Field::new("is_calibrated", DataType::Boolean, false),
            ])),
            _ => return Err(tonic::Status::invalid_argument("Unknown data type")),
        };

        let flight_info = arrow_flight::FlightInfo::new()
            .try_with_schema(schema.as_ref())
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .with_endpoint(
                arrow_flight::FlightEndpoint::new()
                    .with_ticket(arrow_flight::Ticket::new(cmd.clone().into_bytes())),
            )
            .with_total_records(-1)
            .with_total_bytes(-1);

        Ok(tonic::Response::new(flight_info))
    }

    async fn do_get(
        &self,
        request: tonic::Request<arrow_flight::Ticket>,
    ) -> Result<tonic::Response<Self::DoGetStream>, tonic::Status> {
        let cmd = String::from_utf8_lossy(&request.get_ref().ticket).to_string();
        let parts: Vec<&str> = cmd.split(':').collect();

        if parts.len() != 2 {
            return Err(tonic::Status::invalid_argument("Invalid ticket format"));
        }

        let data_type = parts[0].to_string();
        let sample_id = parts[1].to_string();

        self.authorize(&request, &data_type).await?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let pool = self.pool.clone();
        tokio::spawn(async move {
            // Each result is (schema FlightData, batch FlightData) per the Flight
            // wire protocol: the schema must be sent as its own message before
            // any record batches so standard clients (pyarrow.flight, arrow-rs)
            // can reassemble the stream.
            let result: Result<
                (arrow_flight::FlightData, arrow_flight::FlightData),
                tonic::Status,
            > = match data_type.as_str() {
                "variants" => match get_variants_by_sample(&pool, &sample_id).await {
                    Ok(records) => variants_to_batch(records)
                        .and_then(flight_data_from_batch)
                        .map_err(|e| tonic::Status::internal(e.to_string())),
                    Err(e) => Err(tonic::Status::internal(e.to_string())),
                },
                "expression" => match get_expression_by_sample(&pool, &sample_id).await {
                    Ok(records) => expression_to_batch(records)
                        .and_then(flight_data_from_batch)
                        .map_err(|e| tonic::Status::internal(e.to_string())),
                    Err(e) => Err(tonic::Status::internal(e.to_string())),
                },
                "umap" => match get_umap_by_sample(&pool, &sample_id).await {
                    Ok(records) => umap_to_batch(records)
                        .and_then(flight_data_from_batch)
                        .map_err(|e| tonic::Status::internal(e.to_string())),
                    Err(e) => Err(tonic::Status::internal(e.to_string())),
                },
                "clinical_observations" => {
                    match get_clinical_observations_by_subject(&pool, &sample_id).await {
                        Ok(records) => clinical_observations_to_batch(records)
                            .and_then(flight_data_from_batch)
                            .map_err(|e| tonic::Status::internal(e.to_string())),
                        Err(e) => Err(tonic::Status::internal(e.to_string())),
                    }
                }
                "cgm" => match get_cgm_readings_by_subject(&pool, &sample_id).await {
                    Ok(records) => cgm_to_batch(records)
                        .and_then(flight_data_from_batch)
                        .map_err(|e| tonic::Status::internal(e.to_string())),
                    Err(e) => Err(tonic::Status::internal(e.to_string())),
                },
                _ => Err(tonic::Status::invalid_argument("Unknown data type")),
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

        Ok(tonic::Response::new(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }

    async fn do_put(
        &self,
        _request: tonic::Request<tonic::Streaming<arrow_flight::FlightData>>,
    ) -> Result<tonic::Response<Self::DoPutStream>, tonic::Status> {
        Err(tonic::Status::unimplemented("do_put not implemented"))
    }

    async fn do_action(
        &self,
        _request: tonic::Request<arrow_flight::Action>,
    ) -> Result<tonic::Response<Self::DoActionStream>, tonic::Status> {
        Err(tonic::Status::unimplemented("do_action not implemented"))
    }

    async fn list_actions(
        &self,
        _request: tonic::Request<arrow_flight::Empty>,
    ) -> Result<tonic::Response<Self::ListActionsStream>, tonic::Status> {
        Err(tonic::Status::unimplemented("list_actions not implemented"))
    }

    async fn do_exchange(
        &self,
        _request: tonic::Request<tonic::Streaming<arrow_flight::FlightData>>,
    ) -> Result<tonic::Response<Self::DoExchangeStream>, tonic::Status> {
        Err(tonic::Status::unimplemented("do_exchange not implemented"))
    }

    async fn poll_flight_info(
        &self,
        _request: tonic::Request<arrow_flight::FlightDescriptor>,
    ) -> Result<tonic::Response<arrow_flight::PollInfo>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "poll_flight_info not implemented",
        ))
    }

    async fn get_schema(
        &self,
        _request: tonic::Request<arrow_flight::FlightDescriptor>,
    ) -> Result<tonic::Response<arrow_flight::SchemaResult>, tonic::Status> {
        Err(tonic::Status::unimplemented("get_schema not implemented"))
    }
}

fn variants_to_batch(
    records: Vec<tpt_soma_core::query::VariantRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("variant_id", DataType::Utf8, false),
        Field::new("chromosome", DataType::Utf8, false),
        Field::new("position", DataType::Int32, false),
        Field::new("reference", DataType::Utf8, false),
        Field::new("alternate", DataType::Utf8, false),
        Field::new("rsid", DataType::Utf8, true),
        Field::new("clinvar_id", DataType::Utf8, true),
        Field::new("genotype", DataType::Utf8, true),
    ]));

    let variant_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.variant_id.clone()))
            .collect::<Vec<_>>(),
    );
    let chromosomes = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.chromosome.clone()))
            .collect::<Vec<_>>(),
    );
    let positions = Int32Array::from(records.iter().map(|r| Some(r.position)).collect::<Vec<_>>());
    let references = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.reference.clone()))
            .collect::<Vec<_>>(),
    );
    let alternates = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.alternate.clone()))
            .collect::<Vec<_>>(),
    );
    let rsids = StringArray::from(records.iter().map(|r| r.rsid.clone()).collect::<Vec<_>>());
    let clinvar_ids = StringArray::from(
        records
            .iter()
            .map(|r| r.clinvar_id.clone())
            .collect::<Vec<_>>(),
    );
    let genotypes = StringArray::from(
        records
            .iter()
            .map(|r| r.genotype.clone())
            .collect::<Vec<_>>(),
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(variant_ids),
            Arc::new(chromosomes),
            Arc::new(positions),
            Arc::new(references),
            Arc::new(alternates),
            Arc::new(rsids),
            Arc::new(clinvar_ids),
            Arc::new(genotypes),
        ],
    )
    .map_err(|e| FlightError::Arrow(e.to_string()))
}

fn expression_to_batch(
    records: Vec<tpt_soma_core::query::ExpressionRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("sample_id", DataType::Utf8, false),
        Field::new("cell_id", DataType::Utf8, false),
        Field::new("gene_id", DataType::Utf8, false),
        Field::new("count", DataType::Int32, false),
    ]));

    let sample_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.sample_id.to_string()))
            .collect::<Vec<_>>(),
    );
    let cell_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.cell_id.clone()))
            .collect::<Vec<_>>(),
    );
    let gene_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.gene_id.clone()))
            .collect::<Vec<_>>(),
    );
    let counts = Int32Array::from(records.iter().map(|r| Some(r.count)).collect::<Vec<_>>());

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(sample_ids),
            Arc::new(cell_ids),
            Arc::new(gene_ids),
            Arc::new(counts),
        ],
    )
    .map_err(|e| FlightError::Arrow(e.to_string()))
}

fn umap_to_batch(
    records: Vec<tpt_soma_core::query::UmapRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("sample_id", DataType::Utf8, false),
        Field::new("cell_id", DataType::Utf8, false),
        Field::new("umap1", DataType::Float64, false),
        Field::new("umap2", DataType::Float64, false),
        Field::new("cluster", DataType::Utf8, false),
    ]));

    let sample_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.sample_id.to_string()))
            .collect::<Vec<_>>(),
    );
    let cell_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.cell_id.clone()))
            .collect::<Vec<_>>(),
    );
    let umap1 = Float64Array::from(records.iter().map(|r| Some(r.umap1)).collect::<Vec<_>>());
    let umap2 = Float64Array::from(records.iter().map(|r| Some(r.umap2)).collect::<Vec<_>>());
    let clusters = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.cluster.clone()))
            .collect::<Vec<_>>(),
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(sample_ids),
            Arc::new(cell_ids),
            Arc::new(umap1),
            Arc::new(umap2),
            Arc::new(clusters),
        ],
    )
    .map_err(|e| FlightError::Arrow(e.to_string()))
}

fn clinical_observations_to_batch(
    records: Vec<tpt_soma_core::query::ClinicalObservationRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("subject_id", DataType::Utf8, false),
        Field::new("loinc_code", DataType::Utf8, false),
        Field::new("value", DataType::Float64, false),
        Field::new("unit", DataType::Utf8, true),
        Field::new("effective_time", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, false),
        Field::new("source", DataType::Utf8, false),
    ]));

    let subject_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.subject_id.clone()))
            .collect::<Vec<_>>(),
    );
    let loinc_codes = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.loinc_code.clone()))
            .collect::<Vec<_>>(),
    );
    let values = Float64Array::from(records.iter().map(|r| Some(r.value)).collect::<Vec<_>>());
    let units = StringArray::from(records.iter().map(|r| r.unit.clone()).collect::<Vec<_>>());
    let effective_times = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.effective_time.to_rfc3339()))
            .collect::<Vec<_>>(),
    );
    let statuses = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.status.clone()))
            .collect::<Vec<_>>(),
    );
    let sources = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.source.clone()))
            .collect::<Vec<_>>(),
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(subject_ids),
            Arc::new(loinc_codes),
            Arc::new(values),
            Arc::new(units),
            Arc::new(effective_times),
            Arc::new(statuses),
            Arc::new(sources),
        ],
    )
    .map_err(|e| FlightError::Arrow(e.to_string()))
}

fn cgm_to_batch(
    records: Vec<tpt_soma_core::query::CgmReadingRecord>,
) -> Result<RecordBatch, FlightError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("subject_id", DataType::Utf8, false),
        Field::new("ts", DataType::Utf8, false),
        Field::new("glucose_mgdl", DataType::Float64, false),
        Field::new("source", DataType::Utf8, false),
        Field::new("sensor_id", DataType::Utf8, true),
        Field::new("is_calibrated", DataType::Boolean, false),
    ]));

    let subject_ids = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.subject_id.clone()))
            .collect::<Vec<_>>(),
    );
    let timestamps = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.ts.to_rfc3339()))
            .collect::<Vec<_>>(),
    );
    let glucose = Float64Array::from(
        records
            .iter()
            .map(|r| Some(r.glucose_mgdl))
            .collect::<Vec<_>>(),
    );
    let sources = StringArray::from(
        records
            .iter()
            .map(|r| Some(r.source.clone()))
            .collect::<Vec<_>>(),
    );
    let sensor_ids = StringArray::from(
        records
            .iter()
            .map(|r| r.sensor_id.clone())
            .collect::<Vec<_>>(),
    );
    let is_calibrated = BooleanArray::from(
        records
            .iter()
            .map(|r| Some(r.is_calibrated))
            .collect::<Vec<_>>(),
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(subject_ids),
            Arc::new(timestamps),
            Arc::new(glucose),
            Arc::new(sources),
            Arc::new(sensor_ids),
            Arc::new(is_calibrated),
        ],
    )
    .map_err(|e| FlightError::Arrow(e.to_string()))
}

fn flight_data_from_batch(
    batch: RecordBatch,
) -> Result<(arrow_flight::FlightData, arrow_flight::FlightData), FlightError> {
    let schema_flight_data = schema_to_flight_data(batch.schema())?;
    let batch_flight_data = batch_to_flight_data(batch)?;
    Ok((schema_flight_data, batch_flight_data))
}

fn schema_to_flight_data(schema: Arc<Schema>) -> Result<arrow_flight::FlightData, FlightError> {
    let data_gen = IpcDataGenerator::default();
    let mut tracker = DictionaryTracker::new(false);
    let encoded = data_gen.schema_to_bytes_with_dictionary_tracker(
        &schema,
        &mut tracker,
        &IpcWriteOptions::default(),
    );
    Ok(arrow_flight::FlightData {
        data_header: Bytes::from(encoded.ipc_message),
        data_body: Bytes::from(encoded.arrow_data),
        ..Default::default()
    })
}

fn batch_to_flight_data(batch: RecordBatch) -> Result<arrow_flight::FlightData, FlightError> {
    let data_gen = IpcDataGenerator::default();
    let mut tracker = DictionaryTracker::new(false);
    let mut write_context = IpcWriteContext::default();
    let (_, encoded) = data_gen
        .encode(
            &batch,
            &mut tracker,
            &IpcWriteOptions::default(),
            &mut write_context,
        )
        .map_err(|e| FlightError::Arrow(e.to_string()))?;

    Ok(arrow_flight::FlightData {
        data_header: Bytes::from(encoded.ipc_message),
        data_body: Bytes::from(encoded.arrow_data),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::RecordBatch;

    fn sample_variant_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("variant_id", DataType::Utf8, false),
            Field::new("chromosome", DataType::Utf8, false),
            Field::new("position", DataType::Int32, false),
            Field::new("reference", DataType::Utf8, false),
            Field::new("alternate", DataType::Utf8, false),
            Field::new("rsid", DataType::Utf8, true),
            Field::new("clinvar_id", DataType::Utf8, true),
            Field::new("genotype", DataType::Utf8, true),
        ]));

        let variant_ids = StringArray::from(vec!["1:100:A:T", "2:300:T:A"]);
        let chromosomes = StringArray::from(vec!["1", "2"]);
        let positions = Int32Array::from(vec![100, 300]);
        let references = StringArray::from(vec!["A", "T"]);
        let alternates = StringArray::from(vec!["T", "A"]);
        let rsids = StringArray::from(vec![Some("rs123"), None]);
        let clinvar_ids = StringArray::from(vec![Some("VCV000123"), None]);
        let genotypes = StringArray::from(vec![Some("0/1"), None]);

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(variant_ids),
                Arc::new(chromosomes),
                Arc::new(positions),
                Arc::new(references),
                Arc::new(alternates),
                Arc::new(rsids),
                Arc::new(clinvar_ids),
                Arc::new(genotypes),
            ],
        )
        .unwrap()
    }

    #[test]
    fn test_flight_data_round_trip() {
        let batch = sample_variant_batch();
        let (schema_data, batch_data) = flight_data_from_batch(batch.clone()).unwrap();

        // The schema message must be carried in the header so standard clients
        // (pyarrow.flight, arrow-flight Rust decoder) can reassemble the stream.
        assert!(!schema_data.data_header.is_empty());
        assert!(!batch_data.data_header.is_empty());

        // Decode with the standard arrow-flight decoder and compare.
        let decoded =
            arrow_flight::utils::flight_data_to_batches(&[schema_data, batch_data]).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].num_rows(), batch.num_rows());
        assert_eq!(decoded[0].schema(), batch.schema());

        let variants = decoded[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(variants.value(0), "1:100:A:T");
        assert_eq!(variants.value(1), "2:300:T:A");
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
