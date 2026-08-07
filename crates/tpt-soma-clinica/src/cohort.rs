//! Cohort discovery over clinical observations (Phase 4, `tpt-soma-clinica`).

use crate::{ClinicaError, Result};
use sqlx::PgPool;
use sqlx::Row;

/// Filter selecting subjects that have an observation with a given LOINC code
/// whose numeric value falls in `[min_value, max_value]`.
#[derive(Debug, Clone)]
pub struct BiomarkerCohortFilter {
    pub loinc_code: String,
    pub min_value: f64,
    pub max_value: f64,
}

/// A typed query parameter (kept small; only the types we bind).
#[derive(Debug, Clone)]
pub enum Param {
    Text(String),
    F64(f64),
    I32(i64),
}

/// Build a parameterized cohort-discovery query (no string interpolation of
/// user values — safe against injection).
pub fn build_cohort_query(f: &BiomarkerCohortFilter) -> (String, Vec<Param>) {
    let sql = "SELECT DISTINCT subject_id FROM organ_function_observations \
               WHERE loinc_code = $1 AND value >= $2 AND value <= $3"
        .to_string();
    (
        sql,
        vec![
            Param::Text(f.loinc_code.clone()),
            Param::F64(f.min_value),
            Param::F64(f.max_value),
        ],
    )
}

/// Run the cohort-discovery query and return matching `subject_id`s.
pub async fn discover_cohort(pool: &PgPool, f: &BiomarkerCohortFilter) -> Result<Vec<String>> {
    let (sql, params) = build_cohort_query(f);
    let mut q = sqlx::query(&sql);
    for p in &params {
        q = match p {
            Param::Text(s) => q.bind(s.clone()),
            Param::F64(v) => q.bind(*v),
            Param::I32(v) => q.bind(*v),
        };
    }
    let rows = q.fetch_all(pool).await.map_err(ClinicaError::Database)?;
    Ok(rows
        .iter()
        .map(|r| r.get::<String, _>("subject_id"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cohort_query_shape() {
        let f = BiomarkerCohortFilter {
            loinc_code: "2093-3".to_string(),
            min_value: 100.0,
            max_value: 200.0,
        };
        let (sql, params) = build_cohort_query(&f);
        assert!(sql.contains("organ_function_observations"));
        assert!(sql.contains("$1") && sql.contains("$2") && sql.contains("$3"));
        assert_eq!(params.len(), 3);
        assert!(matches!(params[0], Param::Text(ref s) if s == "2093-3"));
    }
}
