use crate::VARIANT;

pub fn variant_batch() -> arrow::array::RecordBatch {
    let schema = VARIANT.clone();
    arrow::array::RecordBatch::try_new(schema, vec![
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["var-1"])),
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["1"])),
        std::sync::Arc::new(arrow::array::Int32Array::from(vec![1000])),
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["A"])),
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["T"])),
        std::sync::Arc::new(arrow::array::StringArray::from(vec!["rs12345"])),
    ]).expect("valid batch")
}
