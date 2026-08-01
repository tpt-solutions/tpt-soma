use std::sync::Arc;
use tokio::sync::Mutex;
use arrow_schema::Schema;

pub struct FlightServer {
    pub schema: Arc<Schema>,
    pub data: Arc<Mutex<Vec<arrow::record_batch::RecordBatch>>>,
}

impl FlightServer {
    pub async fn run(self, addr: std::net::SocketAddr) -> Result<(), crate::Error> {
        let _ = self;
        let _ = addr;
        Ok(())
    }
}
