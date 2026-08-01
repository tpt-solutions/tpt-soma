use arrow_schema::*;

pub const SAMPLE: Schema = Schema::new(vec![
    Field::new("sample_id", DataType::Utf8, false),
    Field::new("patient_id", DataType::Utf8, true),
    Field::new("source", DataType::Utf8, false),
    Field::new("dataset_provenance", DataType::Utf8, true),
]);

pub const COHORT: Schema = Schema::new(vec![
    Field::new("cohort_id", DataType::Utf8, false),
    Field::new("name", DataType::Utf8, false),
    Field::new("description", DataType::Utf8, true),
]);

pub const VARIANT: Schema = Schema::new(vec![
    Field::new("variant_id", DataType::Utf8, false),
    Field::new("chromosome", DataType::Utf8, false),
    Field::new("position", DataType::Int32, false),
    Field::new("reference", DataType::Utf8, false),
    Field::new("alternate", DataType::Utf8, false),
    Field::new("rsid", DataType::Utf8, true),
]);
