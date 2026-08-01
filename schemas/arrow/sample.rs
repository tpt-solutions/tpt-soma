use crate::{SAMPLE, COHORT};

pub fn sample_batch() -> arrow::array::RecordBatch {
    let schema = SAMPLE.clone();
    let sample_ids = arrow::array::StringBuilder::new(4);
    let patient_ids = arrow::array::StringBuilder::new(4);
    let sources = arrow::array::StringBuilder::new(4);
    let provenance = arrow::array::StringBuilder::new(4);
    // This is a compile-time sanity check placeholder; real golden-file tests
    // live in crates/tpt-soma-core/tests/.
    arrow::array::RecordBatch::try_new(schema, vec![
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["sample-1"])),
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["patient-1"])),
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["public"])),
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["1000g"])),
    ]).expect("valid batch")
}
