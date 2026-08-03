//! Persistence for CGM readings

use crate::cgm::CgmReading;
use crate::{ChronosError, Result};
use sqlx::PgPool;

pub async fn insert_cgm_reading(pool: &PgPool, reading: &CgmReading) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO cgm_readings
            (id, subject_id, ts, glucose_mgdl, source, sensor_id, is_calibrated, trend_arrow)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (subject_id, ts, source) DO NOTHING
        "#,
    )
    .bind(reading.id)
    .bind(&reading.subject_id)
    .bind(reading.timestamp)
    .bind(reading.glucose_mgdl)
    .bind(format!("{:?}", reading.source))
    .bind(&reading.sensor_id)
    .bind(reading.is_calibrated)
    .bind(reading.trend_arrow.as_ref().map(|t| format!("{:?}", t)))
    .execute(pool)
    .await
    .map_err(ChronosError::Database)?;
    Ok(())
}

pub async fn insert_cgm_readings(pool: &PgPool, readings: &[CgmReading]) -> Result<usize> {
    for reading in readings {
        insert_cgm_reading(pool, reading).await?;
    }
    Ok(readings.len())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_insert_cgm_reading() {
        // Would require a test database
    }
}
