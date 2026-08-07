// End-to-end Arrow Flight RPC integration test (TM-02: Flight authentication).
//
// Requires a running PostgreSQL database at TEST_DATABASE_URL (see
// tpt-soma-core::test_helpers::test_pool for the default).

use arrow_flight::flight_service_client::FlightServiceClient;
use arrow_flight::{FlightDescriptor, Ticket};
use ed25519_dalek::SigningKey;
use std::sync::Arc;
use tpt_soma_api::flight::FlightServer;
use tpt_soma_capability::{CapabilityToken, RevocationList, signing::LocalSigningBackend};
use tpt_soma_core::test_helpers::test_pool;

fn signed_token(
    signing_key: &SigningKey,
    subject: &str,
    resource_class: &str,
    action: &str,
) -> String {
    let backend = LocalSigningBackend::new(signing_key.clone());
    let token = CapabilityToken {
        subject: subject.to_string(),
        resource_class: resource_class.to_string(),
        cohort_scope: vec!["*".to_string()],
        action: action.to_string(),
        expiry: u64::MAX,
        nonce: rand::random::<[u8; 32]>().to_vec(),
        signature: Vec::new(),
    };
    let signed = CapabilityToken::sign(&backend, token);
    serde_json::to_string(&signed).unwrap()
}

async fn spawn_flight_server() -> (FlightServiceClient<tonic::transport::Channel>, String) {
    let pool = test_pool().await.unwrap();
    let signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let verifying_key = signing_key.verifying_key();
    let revocation_list = Arc::new(RevocationList::new());

    let server = FlightServer {
        schema: Arc::new(arrow_schema::Schema::empty()),
        pool,
        verifying_key,
        revocation_list: revocation_list.clone(),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener); // release the port so the server can bind it
    tokio::spawn(async move {
        let _ = server.run(addr).await;
    });

    // Wait for the server to accept connections.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let channel = tonic::transport::Endpoint::new(format!("http://{}", addr))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let client = FlightServiceClient::new(channel);
    let token = signed_token(&signing_key, "researcher-1", "genomic_variant", "read");
    (client, token)
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_flight_do_get_rejects_unauthenticated() {
    let (mut client, _token) = spawn_flight_server().await;

    let ticket = Ticket::new("variants:00000000-0000-0000-0000-000000000000".to_string());
    let result = client.do_get(ticket).await;
    assert!(result.is_err());
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_flight_do_get_rejects_wrong_resource_class() {
    let (mut client, _token) = spawn_flight_server().await;
    let signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let clinical_token = signed_token(&signing_key, "researcher-1", "clinical_observation", "read");

    let ticket = Ticket::new("variants:00000000-0000-0000-0000-000000000000".to_string());
    let mut req = tonic::Request::new(ticket);
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", clinical_token).parse().unwrap(),
    );
    let result = client.do_get(req).await.unwrap_err();
    assert_eq!(result.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_flight_get_flight_info_requires_valid_token() {
    let (mut client, token) = spawn_flight_server().await;

    // No token header -> unauthenticated.
    let descriptor = FlightDescriptor::new_cmd("variants:00000000-0000-0000-0000-000000000000");
    let result = client.get_flight_info(descriptor.clone()).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);

    // Valid token header -> authorized, returns FlightInfo.
    let mut req = tonic::Request::new(descriptor);
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );
    let info = client.get_flight_info(req).await.unwrap().into_inner();
    assert!(!info.endpoint.is_empty());
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_flight_do_get_authorized_returns_batches() {
    let (mut client, token) = spawn_flight_server().await;
    let pool = test_pool().await.unwrap();

    // Insert a variant + sample association so the query returns a real row.
    sqlx::query("DELETE FROM sample_variants WHERE sample_id = $1")
        .bind("00000000-0000-0000-0000-0000000000aa")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM variants WHERE variant_id = $1")
        .bind("test:1:100:A:T")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM samples WHERE sample_id = $1")
        .bind("00000000-0000-0000-0000-0000000000aa")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO samples (sample_id, source) VALUES ($1, 'public')")
        .bind("00000000-0000-0000-0000-0000000000aa")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO variants (variant_id, chromosome, position, reference, alternate, rsid) VALUES ($1, '1', 100, 'A', 'T', 'rs123')",
    )
    .bind("test:1:100:A:T")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sample_variants (sample_id, variant_id, genotype) VALUES ($1, $2, '0/1')",
    )
    .bind("00000000-0000-0000-0000-0000000000aa")
    .bind("test:1:100:A:T")
    .execute(&pool)
    .await
    .unwrap();

    let mut req = tonic::Request::new(Ticket::new(
        "variants:00000000-0000-0000-0000-0000000000aa".to_string(),
    ));
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );

    let mut stream = client.do_get(req).await.unwrap().into_inner();
    let mut flights: Vec<arrow_flight::FlightData> = Vec::new();
    while let Ok(Some(data)) = stream.message().await {
        flights.push(data);
    }
    assert!(flights.len() >= 2, "expected schema + batch messages");

    let batches = arrow_flight::utils::flight_data_to_batches(&flights).unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].num_rows(), 1);

    let rsid = batches[0]
        .column(5)
        .as_any()
        .downcast_ref::<arrow_array::StringArray>()
        .unwrap();
    assert_eq!(rsid.value(0), "rs123");

    // Cleanup
    sqlx::query("DELETE FROM sample_variants WHERE sample_id = $1")
        .bind("00000000-0000-0000-0000-0000000000aa")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM variants WHERE variant_id = $1")
        .bind("test:1:100:A:T")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM samples WHERE sample_id = $1")
        .bind("00000000-0000-0000-0000-0000000000aa")
        .execute(&pool)
        .await
        .unwrap();
}
