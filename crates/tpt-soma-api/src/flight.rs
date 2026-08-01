use std::sync::Arc;
use arrow_schema::Schema;
use arrow_flight::flight_service_server::FlightServiceServer;
use tonic::transport::Server;
use tpt_soma_core::connection::PgPool;
use tpt_soma_core::query::{get_variants_by_sample, get_expression_by_sample, get_umap_by_sample};
use bytes::Bytes;

pub struct FlightServer {
    pub schema: Arc<Schema>,
    pub pool: PgPool,
}

impl FlightServer {
    pub async fn run(self, addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let svc = FlightServiceServer::new(TptSomaFlightService {
            pool: self.pool,
        });

        Server::builder()
            .add_service(svc)
            .serve(addr)
            .await?;

        Ok(())
    }
}

struct TptSomaFlightService {
    pool: PgPool,
}

#[tonic::async_trait]
impl arrow_flight::flight_service_server::FlightService for TptSomaFlightService {
    type HandshakeStream = tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::HandshakeResponse, tonic::Status>>;
    type ListFlightsStream = tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::FlightInfo, tonic::Status>>;
    type DoGetStream = tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::FlightData, tonic::Status>>;
    type DoPutStream = tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::PutResult, tonic::Status>>;
    type DoActionStream = tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::Result, tonic::Status>>;
    type ListActionsStream = tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::ActionType, tonic::Status>>;
    type DoExchangeStream = tokio_stream::wrappers::ReceiverStream<Result<arrow_flight::FlightData, tonic::Status>>;

    async fn handshake(
        &self,
        _request: tonic::Request<tonic::Streaming<arrow_flight::HandshakeRequest>>,
    ) -> Result<tonic::Response<Self::HandshakeStream>, tonic::Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(Ok(arrow_flight::HandshakeResponse {
            protocol_version: 0,
            payload: Bytes::new(),
        })).await.ok();
        Ok(tonic::Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
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
        
        // Parse command to determine what data to return
        // Format: "variants:{sample_id}" or "expression:{sample_id}" or "umap:{sample_id}"
        let parts: Vec<&str> = cmd.split(':').collect();
        if parts.len() != 2 {
            return Err(tonic::Status::invalid_argument("Invalid command format"));
        }

        let (data_type, sample_id) = (parts[0], parts[1]);
        
        let schema = match data_type {
            "variants" => arrow_schema::Schema::new(vec![
                arrow_schema::Field::new("variant_id", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("chromosome", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("position", arrow_schema::DataType::Int32, false),
                arrow_schema::Field::new("reference", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("alternate", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("rsid", arrow_schema::DataType::Utf8, true),
                arrow_schema::Field::new("clinvar_id", arrow_schema::DataType::Utf8, true),
                arrow_schema::Field::new("genotype", arrow_schema::DataType::Utf8, true),
            ]),
            "expression" => arrow_schema::Schema::new(vec![
                arrow_schema::Field::new("sample_id", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("cell_id", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("gene_id", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("count", arrow_schema::DataType::Int32, false),
            ]),
            "umap" => arrow_schema::Schema::new(vec![
                arrow_schema::Field::new("sample_id", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("cell_id", arrow_schema::DataType::Utf8, false),
                arrow_schema::Field::new("umap1", arrow_schema::DataType::Float64, false),
                arrow_schema::Field::new("umap2", arrow_schema::DataType::Float64, false),
                arrow_schema::Field::new("cluster", arrow_schema::DataType::Utf8, false),
            ]),
            _ => return Err(tonic::Status::invalid_argument("Unknown data type")),
        };

        let flight_info = arrow_flight::FlightInfo::new()
            .try_with_schema(&schema)
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .with_endpoint(arrow_flight::FlightEndpoint::new().with_ticket(arrow_flight::Ticket::new(cmd.clone().into_bytes())))
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

        let (data_type, sample_id) = (parts[0].to_string(), parts[1].to_string());
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let result = match data_type.as_str() {
                "variants" => {
                    let records = get_variants_by_sample(&pool, &sample_id).await;
                    match records {
                        Ok(records) => {
                            let _ = records;
                            Ok(arrow_flight::FlightData::default())
                        }
                        Err(e) => Err(tonic::Status::internal(e.to_string())),
                    }
                }
                "expression" => {
                    let records = get_expression_by_sample(&pool, &sample_id).await;
                    match records {
                        Ok(records) => {
                            let _ = records;
                            Ok(arrow_flight::FlightData::default())
                        }
                        Err(e) => Err(tonic::Status::internal(e.to_string())),
                    }
                }
                "umap" => {
                    let records = get_umap_by_sample(&pool, &sample_id).await;
                    match records {
                        Ok(records) => {
                            let _ = records;
                            Ok(arrow_flight::FlightData::default())
                        }
                        Err(e) => Err(tonic::Status::internal(e.to_string())),
                    }
                }
                _ => Err(tonic::Status::invalid_argument("Unknown data type")),
            };
            
            let _ = tx.send(result).await;
        });

        Ok(tonic::Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
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
        Err(tonic::Status::unimplemented("poll_flight_info not implemented"))
    }

    async fn get_schema(
        &self,
        _request: tonic::Request<arrow_flight::FlightDescriptor>,
    ) -> Result<tonic::Response<arrow_flight::SchemaResult>, tonic::Status> {
        Err(tonic::Status::unimplemented("get_schema not implemented"))
    }
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