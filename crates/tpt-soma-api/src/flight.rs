use arrow_array::{Float64Array, Int32Array, RecordBatch, StringArray};
use arrow_flight::flight_service_server::FlightServiceServer;
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema};
use bytes::Bytes;
use std::sync::Arc;
use thiserror::Error;
use tonic::transport::Server;
use tpt_soma_core::connection::PgPool;
use tpt_soma_core::query::{get_expression_by_sample, get_umap_by_sample, get_variants_by_sample};

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
}

impl FlightServer {
    pub async fn run(
        self,
        addr: std::net::SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let svc = FlightServiceServer::new(TptSomaFlightService { pool: self.pool });

        Server::builder().add_service(svc).serve(addr).await?;

        Ok(())
    }
}

struct TptSomaFlightService {
    pool: PgPool,
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
        let descriptor = request.into_inner();
        let cmd = String::from_utf8_lossy(&descriptor.cmd).to_string();

        let parts: Vec<&str> = cmd.split(':').collect();
        if parts.len() != 2 {
            return Err(tonic::Status::invalid_argument("Invalid command format"));
        }

        let (data_type, _sample_id) = (parts[0], parts[1]);

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
        let ticket = request.into_inner();
        let cmd = String::from_utf8_lossy(&ticket.ticket).to_string();
        let parts: Vec<&str> = cmd.split(':').collect();

        if parts.len() != 2 {
            return Err(tonic::Status::invalid_argument("Invalid ticket format"));
        }

        let data_type = parts[0].to_string();
        let sample_id = parts[1].to_string();

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let pool = self.pool.clone();
        tokio::spawn(async move {
            let result: Result<arrow_flight::FlightData, tonic::Status> = match data_type.as_str() {
                "variants" => match get_variants_by_sample(&pool, &sample_id).await {
                    Ok(records) => match variants_to_batch(records) {
                        Ok(batch) => match batch_to_flight_data(batch) {
                            Ok(flight_data) => Ok(flight_data),
                            Err(e) => Err(tonic::Status::internal(e.to_string())),
                        },
                        Err(e) => Err(tonic::Status::internal(e.to_string())),
                    },
                    Err(e) => Err(tonic::Status::internal(e.to_string())),
                },
                "expression" => match get_expression_by_sample(&pool, &sample_id).await {
                    Ok(records) => match expression_to_batch(records) {
                        Ok(batch) => match batch_to_flight_data(batch) {
                            Ok(flight_data) => Ok(flight_data),
                            Err(e) => Err(tonic::Status::internal(e.to_string())),
                        },
                        Err(e) => Err(tonic::Status::internal(e.to_string())),
                    },
                    Err(e) => Err(tonic::Status::internal(e.to_string())),
                },
                "umap" => match get_umap_by_sample(&pool, &sample_id).await {
                    Ok(records) => match umap_to_batch(records) {
                        Ok(batch) => match batch_to_flight_data(batch) {
                            Ok(flight_data) => Ok(flight_data),
                            Err(e) => Err(tonic::Status::internal(e.to_string())),
                        },
                        Err(e) => Err(tonic::Status::internal(e.to_string())),
                    },
                    Err(e) => Err(tonic::Status::internal(e.to_string())),
                },
                _ => Err(tonic::Status::invalid_argument("Unknown data type")),
            };

            let _ = tx.send(result).await;
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

fn batch_to_flight_data(batch: RecordBatch) -> Result<arrow_flight::FlightData, FlightError> {
    let mut buffer = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, batch.schema().as_ref())
            .map_err(|e| FlightError::Arrow(e.to_string()))?;
        writer
            .write(&batch)
            .map_err(|e| FlightError::Arrow(e.to_string()))?;
        writer
            .finish()
            .map_err(|e| FlightError::Arrow(e.to_string()))?;
    }

    Ok(arrow_flight::FlightData {
        data_body: Bytes::from(buffer),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires flight server"]
    async fn test_flight_server() {
        // Integration test would go here
    }
}
