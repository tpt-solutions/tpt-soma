// Full-stack real-PHI pilot end-to-end test.
//
// Drives the real `ApiServer` over HTTP: capability enforcement (TM-01),
// audit ledger chaining, and differential-privacy budget enforcement, using
// Phase 2 real-PHI-like data (FHIR clinical observations + CGM readings) for
// one subject.
//
// Requires a running PostgreSQL database at TEST_DATABASE_URL (see
// tpt-soma-core::test_helpers::test_pool for the default). MinIO is not
// exercised (no object-store ingest routes are hit).

use ed25519_dalek::SigningKey;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tpt_soma_api::server::ApiServer;
use tpt_soma_audit::AuditLedger;
use tpt_soma_capability::{CapabilityToken, RevocationList, signing::LocalSigningBackend};
use tpt_soma_core::{
    connection::run_migrations, store::ObjectStoreClient, test_helpers::test_pool,
};

const SUBJECT: &str = "aaaaaaaa-1111-2222-3333-0000000000aa";
const SAMPLE_ID: &str = "aaaaaaaa-1111-2222-3333-0000000000bb";
const COHORT_ID: &str = "aaaaaaaa-1111-2222-3333-0000000000cc";

fn signed_token(
    signing_key: &SigningKey,
    subject: &str,
    resource_class: &str,
    cohort_scope: Vec<String>,
    action: &str,
) -> String {
    let backend = LocalSigningBackend::new(signing_key.clone());
    let token = CapabilityToken {
        subject: subject.to_string(),
        resource_class: resource_class.to_string(),
        cohort_scope,
        action: action.to_string(),
        expiry: u64::MAX,
        nonce: rand::random::<[u8; 32]>().to_vec(),
        signature: Vec::new(),
    };
    let signed = CapabilityToken::sign(&backend, token);
    serde_json::to_string(&signed).unwrap()
}

fn tokens(signing_key: &SigningKey, cohort: &str) -> (String, String, String, String, String) {
    let read = signed_token(
        signing_key,
        "researcher-real-phi",
        "clinical_observation",
        vec![cohort.to_string()],
        "read",
    );
    let cgm_read = signed_token(
        signing_key,
        "researcher-real-phi",
        "cgm_continuous",
        vec![cohort.to_string()],
        "read",
    );
    let wrong_cohort_read = signed_token(
        signing_key,
        "researcher-real-phi",
        "clinical_observation",
        vec!["other-cohort".to_string()],
        "read",
    );
    let write = signed_token(
        signing_key,
        "researcher-real-phi",
        "clinical_observation",
        vec![cohort.to_string()],
        "write",
    );
    let export = signed_token(
        signing_key,
        "researcher-real-phi",
        "clinical_observation",
        vec![cohort.to_string()],
        "export",
    );
    (read, cgm_read, wrong_cohort_read, write, export)
}

async fn spawn_api_server() -> (SocketAddr, SigningKey) {
    let pool = test_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();

    let signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let verifying_key = signing_key.verifying_key();
    let revocation_list = Arc::new(RevocationList::new());
    let audit_ledger = Arc::new(AuditLedger::new(pool.clone()));
    let object_store = Arc::new(ObjectStoreClient::from_env());

    let server = ApiServer {
        addr: "127.0.0.1:0".parse().unwrap(),
        pool,
        verifying_key,
        revocation_list,
        audit_ledger,
        object_store,
        dp_epsilon: 1.0,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener); // release the port so the server can bind it

    let mut server = server;
    server.addr = addr;
    tokio::spawn(async move {
        let _ = server.run().await;
    });

    // Wait for the server to accept connections.
    tokio::time::sleep(Duration::from_millis(150)).await;
    (addr, signing_key)
}

async fn cleanup(pool: &sqlx::PgPool) {
    for table in [
        "cgm_readings",
        "organ_function_observations",
        "fhir_resource_payloads",
    ] {
        sqlx::query(&format!("DELETE FROM {} WHERE subject_id = $1", table))
            .bind(SUBJECT)
            .execute(pool)
            .await
            .ok();
        sqlx::query(&format!("DELETE FROM {} WHERE resource_id LIKE $1", table))
            .bind(format!("{}%", SUBJECT))
            .execute(pool)
            .await
            .ok();
    }
    sqlx::query("DELETE FROM cohort_membership WHERE cohort_id = $1")
        .bind(COHORT_ID.parse::<uuid::Uuid>().unwrap())
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM samples WHERE sample_id = $1")
        .bind(SAMPLE_ID.parse::<uuid::Uuid>().unwrap())
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM cohorts WHERE cohort_id = $1")
        .bind(COHORT_ID.parse::<uuid::Uuid>().unwrap())
        .execute(pool)
        .await
        .ok();
}

async fn seed(pool: &sqlx::PgPool) {
    sqlx::query("INSERT INTO samples (sample_id, patient_id, source) VALUES ($1, $2, 'patient')")
        .bind(SAMPLE_ID.parse::<uuid::Uuid>().unwrap())
        .bind(SUBJECT.parse::<uuid::Uuid>().unwrap())
        .execute(pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO cohorts (cohort_id, name) VALUES ($1, 'RealPHI Pilot Cohort')")
        .bind(COHORT_ID.parse::<uuid::Uuid>().unwrap())
        .execute(pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO cohort_membership (cohort_id, sample_id) VALUES ($1, $2)")
        .bind(COHORT_ID.parse::<uuid::Uuid>().unwrap())
        .bind(SAMPLE_ID.parse::<uuid::Uuid>().unwrap())
        .execute(pool)
        .await
        .unwrap();

    // Phase 2: clinical observations (renal panel: creatinine + eGFR)
    for (loinc, value) in [("2160-0", 1.1_f64), ("62238-1", 88.0)] {
        sqlx::query(
            "INSERT INTO organ_function_observations (subject_id, loinc_code, value, unit, effective_time, source) \
             VALUES ($1, $2, $3, $4, now(), 'fhir')",
        )
        .bind(SUBJECT)
        .bind(loinc)
        .bind(value)
        .bind("mg/dL")
        .execute(pool)
        .await
        .unwrap();
    }

    // Phase 2: CGM readings (3 readings so glycemic variability can be computed)
    for (i, glucose) in [95.0_f64, 142.0, 108.0].into_iter().enumerate() {
        let ts = chrono::Utc::now() - chrono::Duration::hours(2 - i as i64);
        sqlx::query(
            "INSERT INTO cgm_readings (subject_id, ts, glucose_mgdl, source) VALUES ($1, $2, $3, 'DexcomG6')",
        )
        .bind(SUBJECT)
        .bind(ts)
        .bind(glucose)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn wait_for_audit_rows(pool: &sqlx::PgPool, min_rows: i64, timeout: Duration) -> i64 {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_ledger WHERE actor = $1")
            .bind("researcher-real-phi")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        if count >= min_rows || std::time::Instant::now() > deadline {
            return count;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_real_phi_pilot_full_stack() {
    let (addr, signing_key) = spawn_api_server().await;
    let pool = test_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();

    cleanup(&pool).await;
    seed(&pool).await;

    let base = format!("http://{}", addr);
    let client = reqwest::Client::new();
    let (read_tok, cgm_tok, wrong_cohort_tok, write_tok, export_tok) =
        tokens(&signing_key, COHORT_ID);

    // 1. No token -> 401
    let resp = client
        .get(format!("{}/api/v1/clinical-observations/{}", base, SUBJECT))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "missing token must be rejected");

    // 2. Valid read token -> 200 with the seeded observations
    let resp = client
        .get(format!("{}/api/v1/clinical-observations/{}", base, SUBJECT))
        .bearer_auth(&read_tok)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let observations: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(observations.as_array().unwrap().len(), 2);

    // 3. Wrong resource class -> 403 (TM-01)
    let resp = client
        .get(format!("{}/api/v1/clinical-observations/{}", base, SUBJECT))
        .bearer_auth(&cgm_tok)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        403,
        "cgm token on clinical route must be rejected"
    );

    // 4. Out-of-cohort token -> 403 (TM-01)
    let resp = client
        .get(format!("{}/api/v1/clinical-observations/{}", base, SUBJECT))
        .bearer_auth(&wrong_cohort_tok)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "out-of-cohort token must be rejected");

    // 5. Write token implies read -> 200
    let resp = client
        .get(format!("{}/api/v1/clinical-observations/{}", base, SUBJECT))
        .bearer_auth(&write_tok)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "write token must imply read");

    // 6. Ingest a FHIR observation with a write token -> 200
    let fhir_id = format!("{}-obs-1", SUBJECT);
    let fhir = serde_json::json!({
        "id": fhir_id,
        "status": "final",
        "category": [{ "coding": [{"system": "http://terminology.hl7.org/CodeSystem/observation-category", "code": "laboratory"}] }],
        "code": { "coding": [{"system": "http://loinc.org", "code": "1920-8"}], "text": "AST" },
        "subject": { "reference": format!("Patient/{}", SUBJECT) },
        "effective": "2026-08-01T10:00:00Z",
        "value": { "value": 34.0, "unit": "U/L", "system": "http://unitsofmeasure.org", "code": "U/L" },
        "interpretation": [],
        "reference_range": []
    });
    let resp = client
        .post(format!("{}/api/v1/ingest/fhir-observation", base))
        .bearer_auth(&write_tok)
        .json(&fhir)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "FHIR ingest failed: {}",
        resp.text().await.unwrap()
    );

    // 7. Read back includes the ingested observation (3 total)
    let resp = client
        .get(format!("{}/api/v1/clinical-observations/{}", base, SUBJECT))
        .bearer_auth(&read_tok)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let observations: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(observations.as_array().unwrap().len(), 3);

    // 8. Cross-phase summary (Phase 1 sample + Phase 2 records) -> 200
    let resp = client
        .get(format!(
            "{}/api/v1/subjects/{}/cross-phase-summary",
            base, SUBJECT
        ))
        .bearer_auth(&read_tok)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let summary: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(summary["phase1_sample_count"], 1);
    assert_eq!(summary["phase2_observation_count"], 3);
    assert_eq!(summary["phase2_cgm_reading_count"], 3);

    // 9. CGM variability with a cgm_continuous token -> 200
    let start = chrono::Utc::now() - chrono::Duration::hours(3);
    let end = chrono::Utc::now() + chrono::Duration::hours(1);
    let resp = client
        .get(format!(
            "{}/api/v1/cgm/{}/variability?start={}&end={}",
            base,
            SUBJECT,
            start.to_rfc3339(),
            end.to_rfc3339()
        ))
        .bearer_auth(&cgm_tok)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "cgm variability failed: {}",
        resp.text().await.unwrap()
    );

    // 10. Aggregate count requires export (read -> 403)
    let resp = client
        .post(format!(
            "{}/api/v1/cohorts/{}/aggregate/count",
            base, COHORT_ID
        ))
        .bearer_auth(&read_tok)
        .json(&serde_json::json!({ "sensitivity": 0.5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "read token must not allow DP export");

    // 11. Export with sensitivity 0.5 -> 200; the true count must NOT leak.
    let resp = client
        .post(format!(
            "{}/api/v1/cohorts/{}/aggregate/count",
            base, COHORT_ID
        ))
        .bearer_auth(&export_tok)
        .json(&serde_json::json!({ "sensitivity": 0.5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body.get("noisy_count").is_some(),
        "response must include noisy_count"
    );
    assert!(
        body.get("true_count").is_none(),
        "DP must not leak the true count in the response"
    );
    assert_eq!(body["epsilon_spent"], 0.5);

    // 12. Budget exhaustion: remaining budget is 0.5 (epsilon=1.0) -> 403.
    let resp = client
        .post(format!(
            "{}/api/v1/cohorts/{}/aggregate/count",
            base, COHORT_ID
        ))
        .bearer_auth(&export_tok)
        .json(&serde_json::json!({ "sensitivity": 0.6 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "DP budget exhaustion must block export");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("differential privacy"),
        "budget error should be surfaced: {body}"
    );

    // 13. Audit ledger: wait for the middleware (fire-and-forget) writes, then
    //     assert read/write/export events landed and the chain still verifies.
    let audit_rows = wait_for_audit_rows(&pool, 8, Duration::from_secs(5)).await;
    assert!(audit_rows >= 8, "expected audit events, got {audit_rows}");

    let read_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_ledger WHERE actor = $1 AND action = 'read'",
    )
    .bind("researcher-real-phi")
    .fetch_one(&pool)
    .await
    .unwrap();
    let write_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_ledger WHERE actor = $1 AND action = 'write'",
    )
    .bind("researcher-real-phi")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(read_events >= 3, "expected read events, got {read_events}");
    assert!(
        write_events >= 1,
        "expected write event, got {write_events}"
    );

    // DP budget spend must also be recorded (audit hook).
    let dp_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_ledger WHERE resource_class = 'dp_budget'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        dp_events >= 1,
        "expected dp_budget spend event, got {dp_events}"
    );

    let ledger = AuditLedger::new(pool.clone());
    let report = ledger.verify_chain().await.unwrap();
    assert!(report.valid, "audit chain must verify");
    assert!(report.events_checked > 0, "chain must have been checked");

    cleanup(&pool).await;
}
