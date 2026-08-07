//! Persistence for simulation runs, parameter sets, calibration targets, and
//! emitted trajectories (Phase 3 schema, migration `20240101000006_*`).

use crate::{Result, SimulacrumError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRun {
    pub id: Uuid,
    pub subject_id: String,
    pub model_name: String,
    pub created_at: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSet {
    pub id: Uuid,
    pub run_id: Uuid,
    pub param_name: String,
    pub param_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationTarget {
    pub id: Uuid,
    pub run_id: Uuid,
    pub target_name: String,
    pub target_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SimulationOutput {
    pub id: Uuid,
    pub run_id: Uuid,
    pub ts: DateTime<Utc>,
    pub series_name: String,
    pub value: f64,
}

pub async fn insert_simulation_run(pool: &PgPool, run: &SimulationRun) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO simulation_runs (id, subject_id, model_name, created_at, status)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(run.id)
    .bind(&run.subject_id)
    .bind(&run.model_name)
    .bind(run.created_at)
    .bind(&run.status)
    .execute(pool)
    .await
    .map_err(SimulacrumError::Database)?;
    Ok(())
}

pub async fn insert_parameter_set(pool: &PgPool, p: &ParameterSet) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO simulation_parameter_sets (id, run_id, param_name, param_value)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(p.id)
    .bind(p.run_id)
    .bind(&p.param_name)
    .bind(p.param_value)
    .execute(pool)
    .await
    .map_err(SimulacrumError::Database)?;
    Ok(())
}

pub async fn insert_calibration_target(pool: &PgPool, t: &CalibrationTarget) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO calibration_targets (id, run_id, target_name, target_value)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(t.id)
    .bind(t.run_id)
    .bind(&t.target_name)
    .bind(t.target_value)
    .execute(pool)
    .await
    .map_err(SimulacrumError::Database)?;
    Ok(())
}

pub async fn insert_simulation_output(pool: &PgPool, o: &SimulationOutput) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO simulation_outputs (id, run_id, ts, series_name, value)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(o.id)
    .bind(o.run_id)
    .bind(o.ts)
    .bind(&o.series_name)
    .bind(o.value)
    .execute(pool)
    .await
    .map_err(SimulacrumError::Database)?;
    Ok(())
}

pub async fn get_simulation_outputs(pool: &PgPool, run_id: Uuid) -> Result<Vec<SimulationOutput>> {
    let rows = sqlx::query_as::<_, SimulationOutput>(
        r#"
        SELECT id, run_id, ts, series_name, value
        FROM simulation_outputs
        WHERE run_id = $1
        ORDER BY ts, series_name
        "#,
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(SimulacrumError::Database)?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SimulationSeriesPoint {
    pub id: Uuid,
    pub run_id: Uuid,
    pub ts: DateTime<Utc>,
    pub series_name: String,
    pub value: f64,
}

/// Mirror a run's relational `simulation_outputs` rows into the Chronos-style
/// `simulation_series` time-series table (deferred item ADR 007 §2.2). Keeps the
/// digital-twin trajectories queryable through the same longitudinal path as
/// CGM / organ-function data without changing the authoritative store.
pub async fn mirror_outputs_to_chronos(pool: &PgPool, run_id: Uuid) -> Result<usize> {
    let outputs = get_simulation_outputs(pool, run_id).await?;
    let mut mirrored = 0usize;
    for o in outputs {
        let point = SimulationSeriesPoint {
            id: Uuid::new_v4(),
            run_id: o.run_id,
            ts: o.ts,
            series_name: o.series_name,
            value: o.value,
        };
        sqlx::query(
            r#"
            INSERT INTO simulation_series (id, run_id, ts, series_name, value)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(point.id)
        .bind(point.run_id)
        .bind(point.ts)
        .bind(&point.series_name)
        .bind(point.value)
        .execute(pool)
        .await
        .map_err(SimulacrumError::Database)?;
        mirrored += 1;
    }
    Ok(mirrored)
}

/// Mirror a run into the Ontological Soma Graph by writing `cross_talk` edges
/// from the simulation run to each OSG node it touched (deferred item ADR 007
/// §2.2).
///
/// NOTE: this relies on Keystone's Plexus `create_edge` runtime function. The
/// signature below assumes `(source_id, target_id, edge_type, properties)` and
/// must be validated against the deployed Plexus extension; `tpt-soma` talks to
/// Keystone over the Postgres wire and does not pin a Plexus SDK version. The
/// call is best-effort: a missing Plexus function surfaces as a
/// `SimulacrumError::Database` that callers may choose to log-and-continue.
pub async fn mirror_run_to_plexus(
    pool: &PgPool,
    run_id: Uuid,
    touched_nodes: &[String],
) -> Result<usize> {
    let source = format!("simulation:{run_id}");
    let mut edges = 0usize;
    for node in touched_nodes {
        sqlx::query("SELECT plexus.create_edge($1::text, $2::text, 'cross_talk', $3::jsonb)")
            .bind(&source)
            .bind(node)
            .bind(serde_json::json!({"mechanism": "digital_twin", "direction": "produces"}))
            .execute(pool)
            .await
            .map_err(SimulacrumError::Database)?;
        edges += 1;
    }
    Ok(edges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_run_struct_roundtrip_fields() {
        let run = SimulationRun {
            id: Uuid::new_v4(),
            subject_id: "subject-1".to_string(),
            model_name: "InsulinGlucose".to_string(),
            created_at: Utc::now(),
            status: "completed".to_string(),
        };
        assert_eq!(run.model_name, "InsulinGlucose");
        assert_eq!(run.subject_id, "subject-1");
    }

    async fn test_pool() -> Option<PgPool> {
        match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => Some(PgPool::connect(&url).await.unwrap()),
            Err(_) => None,
        }
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_mirror_outputs_to_chronos() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let run_id = Uuid::new_v4();
        insert_simulation_output(
            &pool,
            &SimulationOutput {
                id: Uuid::new_v4(),
                run_id,
                ts: Utc::now(),
                series_name: "glucose".to_string(),
                value: 5.5,
            },
        )
        .await
        .unwrap();
        let mirrored = mirror_outputs_to_chronos(&pool, run_id).await.unwrap();
        assert_eq!(mirrored, 1);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_mirror_run_to_plexus() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let run_id = Uuid::new_v4();
        let edges = mirror_run_to_plexus(&pool, run_id, &["breast_tissue".to_string()])
            .await
            .unwrap();
        assert_eq!(edges, 1);
    }
}
