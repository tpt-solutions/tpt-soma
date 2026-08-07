use clap::{Parser, ValueHint};
use std::net::SocketAddr;
use std::sync::Arc;
use tpt_soma_api::flight::FlightServer;
use tpt_soma_capability::RevocationList;
use tpt_soma_core::connection::create_pool;
use tracing_subscriber::prelude::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, env = "DATABASE_URL", value_hint = ValueHint::Url)]
    database_url: String,

    #[arg(long, env = "FLIGHT_LISTEN_ADDR", default_value = "0.0.0.0:8815")]
    flight_listen_addr: String,

    #[arg(
        long,
        env = "CAPABILITY_ROOT_KEY_PATH",
        value_hint = ValueHint::FilePath
    )]
    capability_root_key_path: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let pool = create_pool(&args.database_url).await?;

    let signing_key =
        tpt_soma_api::secrets::load_signing_key(args.capability_root_key_path.as_deref())?;
    let verifying_key = signing_key.verifying_key();

    let server = FlightServer {
        schema: std::sync::Arc::new(arrow_schema::Schema::empty()),
        pool,
        verifying_key,
        revocation_list: Arc::new(RevocationList::new()),
    };

    let addr: SocketAddr = args.flight_listen_addr.parse()?;
    server.run(addr).await?;
    Ok(())
}
