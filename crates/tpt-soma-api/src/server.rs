use std::net::SocketAddr;
use axum::{routing::get, Router};

pub struct ApiServer {
    pub addr: SocketAddr,
}

impl ApiServer {
    pub async fn run(self) -> Result<(), crate::Error> {
        let app = Router::new().route("/health", get(|| async { "ok" }));
        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
