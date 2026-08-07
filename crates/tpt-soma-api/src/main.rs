use clap::{Parser, ValueHint};
use prometheus::{Encoder, TextEncoder};
use std::net::SocketAddr;
use std::sync::Arc;
use tpt_soma_api::server::ApiServer;
use tpt_soma_audit::AuditLedger;
use tpt_soma_capability::{registry::DataClassRegistry, revocation::RevocationList};
use tpt_soma_core::{
    connection::{create_pool, run_migrations},
    store::ObjectStoreClient,
};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, env = "DATABASE_URL", value_hint = ValueHint::Url)]
    database_url: String,

    #[arg(long, env = "MINIO_ENDPOINT", default_value = "http://localhost:9000")]
    minio_endpoint: String,

    #[arg(long, env = "MINIO_BUCKET", default_value = "raw-omics")]
    minio_bucket: String,

    #[arg(long, env = "MINIO_ACCESS_KEY", default_value = "minioadmin")]
    minio_access_key: String,

    #[arg(long, env = "MINIO_SECRET_KEY", default_value = "minioadmin")]
    minio_secret_key: String,

    #[arg(long, env = "CAPABILITY_ROOT_KEY_PATH", value_hint = ValueHint::FilePath)]
    capability_root_key_path: Option<String>,

    #[arg(long, env = "LISTEN_ADDR", default_value = "0.0.0.0:8080")]
    listen_addr: String,

    #[arg(long, env = "DP_EPSILON", default_value = "1.0")]
    dp_epsilon: f64,

    #[arg(long, env = "METRICS_ADDR", default_value = "0.0.0.0:9090")]
    metrics_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    // Initialize structured logging with JSON format for production
    let fmt_layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_thread_ids(true)
        .with_thread_names(true);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt_layer)
        .init();

    // Initialize Prometheus metrics
    let metrics_registry = prometheus::Registry::new();
    let metrics_registry = Arc::new(metrics_registry);

    let pool = create_pool(&args.database_url).await?;
    run_migrations(&pool).await?;

    let store = ObjectStoreClient::from_env();
    let mut registry = DataClassRegistry::default();
    registry.seed_phase0();
    registry.seed_phase2();
    registry.seed_phase3();
    registry.seed_phase4();

    let signing_key =
        tpt_soma_api::secrets::load_signing_key(args.capability_root_key_path.as_deref())?;
    let verifying_key = signing_key.verifying_key();

    let revocation_list = Arc::new(RevocationList::new());
    let audit_ledger = Arc::new(AuditLedger::new(pool.clone()));

    let server = ApiServer {
        addr: args.listen_addr.parse()?,
        pool,
        verifying_key,
        revocation_list,
        audit_ledger,
        object_store: Arc::new(store),
        dp_epsilon: args.dp_epsilon,
    };

    // Start metrics server in background
    let metrics_addr = args.metrics_addr.parse::<SocketAddr>()?;
    let metrics_registry_clone = metrics_registry.clone();
    tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/metrics",
            axum::routing::get(move || {
                let registry = metrics_registry_clone.clone();
                async move {
                    let encoder = TextEncoder::new();
                    let metric_families = registry.gather();
                    let mut buffer = Vec::new();
                    if encoder.encode(&metric_families, &mut buffer).is_ok() {
                        axum::http::Response::builder()
                            .header("Content-Type", "text/plain; version=0.0.4")
                            .body(axum::body::Body::from(buffer))
                            .unwrap()
                    } else {
                        axum::http::Response::builder()
                            .status(500)
                            .body(axum::body::Body::from("failed to encode metrics"))
                            .unwrap()
                    }
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind(metrics_addr).await.unwrap();
        tracing::info!("Metrics server listening on {}", metrics_addr);
        axum::serve(listener, app).await.unwrap();
    });

    // Start scheduled audit integrity check job
    let audit_ledger_clone = server.audit_ledger.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600)); // Every hour
        loop {
            interval.tick().await;
            tracing::info!("Running scheduled audit integrity check");
            if let Err(e) = audit_ledger_clone.verify_chain().await {
                tracing::error!("Audit integrity check failed: {}", e);
                // In production, this would trigger an alert
            } else {
                tracing::info!("Audit integrity check passed");
            }
        }
    });

    tracing::info!("Starting tpt-soma API server on {}", server.addr);
    server.run().await?;
    Ok(())
}
