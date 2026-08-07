//! tpt-soma-clinica: Clinical trial design & management, cohort discovery, and
//! biomarker discovery/validation statistics (Phase 4).
//!
//! Initial slice: a small, tested statistics toolkit (Welch t-test, Pearson and
//! point-biserial correlation), a cohort-discovery query builder over clinical
//! observations, and a biomarker-association pipeline that records results into
//! the `biomarker_discovery` table.

pub mod biomarker;
pub mod cohort;
pub mod error;
pub mod stats;
pub mod storage;

pub use error::{ClinicaError, Result};
