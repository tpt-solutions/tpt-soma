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

pub async fn get_simulation_outputs(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Vec<SimulationOutput>> {
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
}
