// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Device-state ingest: the write path behind Arrow Flight `do_put`.
//!
//! Flight is read-only everywhere else (threat model TM-02). This module is the
//! one exception. A write needs a capability token with the `ingest` action (or
//! `admin`) for the `device_state` resource class. A `read` token is refused.
//!
//! Rows describe devices, not people. `tool_id` names an end-effector. Nothing
//! here takes a patient identifier.

use arrow_array::{Array, Float64Array, Int32Array, Int64Array, RecordBatch, StringArray};
use tpt_soma_core::connection::PgPool;

/// Flight subject name for device-state writes. Used as the `do_put` descriptor.
pub const SUBJECT: &str = "device_state";
/// Capability resource class that allows device-state writes and reads.
pub const RESOURCE_CLASS: &str = "device_state";
/// Capability action that allows a write. `admin` also passes.
pub const INGEST_ACTION: &str = "ingest";
/// Largest number of rows accepted in one record batch.
pub const MAX_ROWS_PER_BATCH: usize = 10_000;

const PATHWAY_STATES: [&str; 3] = ["intact", "severed", "hypertrophied"];

/// One pathway's state at one telemetry sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub tool_id: String,
    pub timestamp_us: i64,
    pub structural_integrity: f64,
    pub pathway_id: i32,
    pub pathway_state: String,
    pub healing_progress: f64,
    pub resistance_ohms: f64,
    pub max_bandwidth_mbps: f64,
}

/// Why a batch was refused. Any refusal rejects the whole batch.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IngestError {
    #[error("missing column `{0}`")]
    MissingColumn(&'static str),
    #[error("column `{0}` has the wrong type")]
    WrongType(&'static str),
    #[error("row {row}: column `{column}` is null")]
    Null { row: usize, column: &'static str },
    #[error("row {row}: tool_id is empty")]
    EmptyToolId { row: usize },
    #[error("row {row}: timestamp_us is negative")]
    NegativeTimestamp { row: usize },
    #[error("row {row}: pathway_state `{value}` is not intact, severed or hypertrophied")]
    BadPathwayState { row: usize, value: String },
    #[error("row {row}: column `{column}` is not finite")]
    NonFinite { row: usize, column: &'static str },
    #[error("batch has {0} rows, the limit is {MAX_ROWS_PER_BATCH}")]
    TooManyRows(usize),
}

/// Checks the capability for a device-state write.
///
/// Returns `PermissionDenied` for any token that is not `ingest` or `admin`, or
/// that is scoped to another resource class.
pub fn check_scope(action: &str, resource_class: &str) -> Result<(), tonic::Status> {
    if resource_class != RESOURCE_CLASS {
        return Err(tonic::Status::permission_denied(
            "token not authorized for device_state",
        ));
    }
    if action != INGEST_ACTION && action != "admin" {
        return Err(tonic::Status::permission_denied(
            "device_state writes need the ingest action",
        ));
    }
    Ok(())
}

fn column<'a, T: 'static>(
    batch: &'a RecordBatch,
    name: &'static str,
) -> Result<&'a T, IngestError> {
    let col = batch
        .column_by_name(name)
        .ok_or(IngestError::MissingColumn(name))?;
    col.as_any()
        .downcast_ref::<T>()
        .ok_or(IngestError::WrongType(name))
}

fn check_null(arr: &dyn Array, row: usize, column: &'static str) -> Result<(), IngestError> {
    if arr.is_null(row) {
        Err(IngestError::Null { row, column })
    } else {
        Ok(())
    }
}

fn finite(value: f64, row: usize, column: &'static str) -> Result<f64, IngestError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(IngestError::NonFinite { row, column })
    }
}

/// Converts one record batch into observations, checking every row.
///
/// The batch must have these columns, with these types:
/// `tool_id` Utf8, `timestamp_us` Int64, `structural_integrity` Float64,
/// `pathway_id` Int32, `pathway_state` Utf8, `healing_progress` Float64,
/// `resistance_ohms` Float64, `max_bandwidth_mbps` Float64. No nulls.
///
/// ```
/// use tpt_soma_api::device_ingest::{observations_from_batch, IngestError};
/// use arrow_array::RecordBatch;
/// use arrow_schema::{DataType, Field, Schema};
/// use std::sync::Arc;
///
/// let schema = Arc::new(Schema::new(vec![Field::new("tool_id", DataType::Utf8, false)]));
/// let batch = RecordBatch::new_empty(schema);
/// assert!(matches!(
///     observations_from_batch(&batch),
///     Err(IngestError::MissingColumn("timestamp_us"))
/// ));
/// ```
pub fn observations_from_batch(batch: &RecordBatch) -> Result<Vec<Observation>, IngestError> {
    if batch.num_rows() > MAX_ROWS_PER_BATCH {
        return Err(IngestError::TooManyRows(batch.num_rows()));
    }

    let tool_id = column::<StringArray>(batch, "tool_id")?;
    let timestamp = column::<Int64Array>(batch, "timestamp_us")?;
    let integrity = column::<Float64Array>(batch, "structural_integrity")?;
    let pathway_id = column::<Int32Array>(batch, "pathway_id")?;
    let state = column::<StringArray>(batch, "pathway_state")?;
    let healing = column::<Float64Array>(batch, "healing_progress")?;
    let resistance = column::<Float64Array>(batch, "resistance_ohms")?;
    let bandwidth = column::<Float64Array>(batch, "max_bandwidth_mbps")?;

    let mut rows = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        check_null(tool_id, row, "tool_id")?;
        check_null(timestamp, row, "timestamp_us")?;
        check_null(integrity, row, "structural_integrity")?;
        check_null(pathway_id, row, "pathway_id")?;
        check_null(state, row, "pathway_state")?;
        check_null(healing, row, "healing_progress")?;
        check_null(resistance, row, "resistance_ohms")?;
        check_null(bandwidth, row, "max_bandwidth_mbps")?;

        let id = tool_id.value(row);
        if id.is_empty() {
            return Err(IngestError::EmptyToolId { row });
        }
        let ts = timestamp.value(row);
        if ts < 0 {
            return Err(IngestError::NegativeTimestamp { row });
        }
        let st = state.value(row);
        if !PATHWAY_STATES.contains(&st) {
            return Err(IngestError::BadPathwayState {
                row,
                value: st.to_string(),
            });
        }

        rows.push(Observation {
            tool_id: id.to_string(),
            timestamp_us: ts,
            structural_integrity: finite(integrity.value(row), row, "structural_integrity")?,
            pathway_id: pathway_id.value(row),
            pathway_state: st.to_string(),
            healing_progress: finite(healing.value(row), row, "healing_progress")?,
            resistance_ohms: finite(resistance.value(row), row, "resistance_ohms")?,
            max_bandwidth_mbps: finite(bandwidth.value(row), row, "max_bandwidth_mbps")?,
        });
    }
    Ok(rows)
}

/// Writes observations in one transaction. Returns the number of rows written.
pub async fn insert(pool: &PgPool, rows: &[Observation]) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    for r in rows {
        sqlx::query(
            "INSERT INTO device_state_observations \
             (tool_id, timestamp_us, structural_integrity, pathway_id, pathway_state, \
              healing_progress, resistance_ohms, max_bandwidth_mbps) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&r.tool_id)
        .bind(r.timestamp_us)
        .bind(r.structural_integrity)
        .bind(r.pathway_id)
        .bind(&r.pathway_state)
        .bind(r.healing_progress)
        .bind(r.resistance_ohms)
        .bind(r.max_bandwidth_mbps)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(rows.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{ArrayRef, Float64Array, Int32Array, Int64Array, RecordBatch, StringArray};
    use arrow_schema::{DataType, Field, Schema};
    use std::sync::Arc;

    // ---- scope ------------------------------------------------------------

    #[test]
    fn read_token_cannot_write() {
        // Checklist item A7: a read token on a write route must be refused.
        let status = check_scope("read", RESOURCE_CLASS).unwrap_err();
        assert_eq!(status.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn export_token_cannot_write() {
        assert!(check_scope("export", RESOURCE_CLASS).is_err());
    }

    #[test]
    fn ingest_and_admin_can_write() {
        assert!(check_scope("ingest", RESOURCE_CLASS).is_ok());
        assert!(check_scope("admin", RESOURCE_CLASS).is_ok());
    }

    #[test]
    fn ingest_for_another_class_is_refused() {
        // A genomic token must not write device state.
        let status = check_scope("ingest", "genomic_variant").unwrap_err();
        assert_eq!(status.code(), tonic::Code::PermissionDenied);
    }

    // ---- batch validation -------------------------------------------------

    struct Cols {
        tool_id: Vec<Option<&'static str>>,
        timestamp: Vec<Option<i64>>,
        integrity: Vec<Option<f64>>,
        pathway_id: Vec<Option<i32>>,
        state: Vec<Option<&'static str>>,
        healing: Vec<Option<f64>>,
        resistance: Vec<Option<f64>>,
        bandwidth: Vec<Option<f64>>,
    }

    fn one_good_row() -> Cols {
        Cols {
            tool_id: vec![Some("hand-0")],
            timestamp: vec![Some(1_000)],
            integrity: vec![Some(0.5)],
            pathway_id: vec![Some(0)],
            state: vec![Some("severed")],
            healing: vec![Some(0.25)],
            resistance: vec![Some(480.0)],
            bandwidth: vec![Some(100.0)],
        }
    }

    fn batch_of(c: Cols) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("tool_id", DataType::Utf8, true),
            Field::new("timestamp_us", DataType::Int64, true),
            Field::new("structural_integrity", DataType::Float64, true),
            Field::new("pathway_id", DataType::Int32, true),
            Field::new("pathway_state", DataType::Utf8, true),
            Field::new("healing_progress", DataType::Float64, true),
            Field::new("resistance_ohms", DataType::Float64, true),
            Field::new("max_bandwidth_mbps", DataType::Float64, true),
        ]));
        let cols: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(c.tool_id)),
            Arc::new(Int64Array::from(c.timestamp)),
            Arc::new(Float64Array::from(c.integrity)),
            Arc::new(Int32Array::from(c.pathway_id)),
            Arc::new(StringArray::from(c.state)),
            Arc::new(Float64Array::from(c.healing)),
            Arc::new(Float64Array::from(c.resistance)),
            Arc::new(Float64Array::from(c.bandwidth)),
        ];
        RecordBatch::try_new(schema, cols).unwrap()
    }

    #[test]
    fn valid_row_converts() {
        let rows = observations_from_batch(&batch_of(one_good_row())).unwrap();
        assert_eq!(
            rows,
            vec![Observation {
                tool_id: "hand-0".into(),
                timestamp_us: 1_000,
                structural_integrity: 0.5,
                pathway_id: 0,
                pathway_state: "severed".into(),
                healing_progress: 0.25,
                resistance_ohms: 480.0,
                max_bandwidth_mbps: 100.0,
            }]
        );
    }

    #[test]
    fn null_is_refused() {
        let mut c = one_good_row();
        c.resistance = vec![None];
        assert_eq!(
            observations_from_batch(&batch_of(c)),
            Err(IngestError::Null {
                row: 0,
                column: "resistance_ohms"
            })
        );
    }

    #[test]
    fn empty_tool_id_is_refused() {
        let mut c = one_good_row();
        c.tool_id = vec![Some("")];
        assert_eq!(
            observations_from_batch(&batch_of(c)),
            Err(IngestError::EmptyToolId { row: 0 })
        );
    }

    #[test]
    fn negative_timestamp_is_refused() {
        let mut c = one_good_row();
        c.timestamp = vec![Some(-1)];
        assert_eq!(
            observations_from_batch(&batch_of(c)),
            Err(IngestError::NegativeTimestamp { row: 0 })
        );
    }

    #[test]
    fn unknown_pathway_state_is_refused() {
        let mut c = one_good_row();
        c.state = vec![Some("torn")];
        assert!(matches!(
            observations_from_batch(&batch_of(c)),
            Err(IngestError::BadPathwayState { row: 0, .. })
        ));
    }

    #[test]
    fn nan_is_refused() {
        let mut c = one_good_row();
        c.integrity = vec![Some(f64::NAN)];
        assert_eq!(
            observations_from_batch(&batch_of(c)),
            Err(IngestError::NonFinite {
                row: 0,
                column: "structural_integrity"
            })
        );
    }

    #[test]
    fn wrong_column_type_is_refused() {
        // pathway_id sent as Int64 instead of Int32.
        let schema = Arc::new(Schema::new(vec![
            Field::new("tool_id", DataType::Utf8, true),
            Field::new("timestamp_us", DataType::Int64, true),
            Field::new("structural_integrity", DataType::Float64, true),
            Field::new("pathway_id", DataType::Int64, true),
            Field::new("pathway_state", DataType::Utf8, true),
            Field::new("healing_progress", DataType::Float64, true),
            Field::new("resistance_ohms", DataType::Float64, true),
            Field::new("max_bandwidth_mbps", DataType::Float64, true),
        ]));
        let cols: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(vec!["hand-0"])),
            Arc::new(Int64Array::from(vec![1i64])),
            Arc::new(Float64Array::from(vec![0.5])),
            Arc::new(Int64Array::from(vec![0i64])),
            Arc::new(StringArray::from(vec!["intact"])),
            Arc::new(Float64Array::from(vec![0.0])),
            Arc::new(Float64Array::from(vec![10.0])),
            Arc::new(Float64Array::from(vec![100.0])),
        ];
        let batch = RecordBatch::try_new(schema, cols).unwrap();
        assert_eq!(
            observations_from_batch(&batch),
            Err(IngestError::WrongType("pathway_id"))
        );
    }

    #[test]
    fn missing_column_is_refused() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "tool_id",
            DataType::Utf8,
            true,
        )]));
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(StringArray::from(vec!["hand-0"])) as ArrayRef],
        )
        .unwrap();
        assert_eq!(
            observations_from_batch(&batch),
            Err(IngestError::MissingColumn("timestamp_us"))
        );
    }

    #[test]
    fn oversize_batch_is_refused() {
        let n = MAX_ROWS_PER_BATCH + 1;
        let c = Cols {
            tool_id: vec![Some("hand-0"); n],
            timestamp: vec![Some(1); n],
            integrity: vec![Some(0.5); n],
            pathway_id: vec![Some(0); n],
            state: vec![Some("intact"); n],
            healing: vec![Some(0.0); n],
            resistance: vec![Some(10.0); n],
            bandwidth: vec![Some(100.0); n],
        };
        assert_eq!(
            observations_from_batch(&batch_of(c)),
            Err(IngestError::TooManyRows(n))
        );
    }

    // ---- database (needs TEST_DATABASE_URL) -------------------------------

    #[tokio::test]
    #[ignore = "requires a running PostgreSQL database at TEST_DATABASE_URL"]
    async fn insert_writes_rows() {
        let pool = tpt_soma_core::test_helpers::test_pool().await.unwrap();
        let rows = observations_from_batch(&batch_of(one_good_row())).unwrap();
        let written = insert(&pool, &rows).await.unwrap();
        assert_eq!(written, 1);
    }
}
