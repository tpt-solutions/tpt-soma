// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Minimal columnar batches: the five value types the platform actually
//! exchanges over Flight. Nullable values are `Option<T>`.

use crate::error::{Error, Result};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Utf8,
    Int32,
    Int64,
    Float64,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}

impl Field {
    pub fn new(name: impl Into<String>, data_type: DataType, nullable: bool) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Schema {
    pub fields: Vec<Field>,
}

impl Schema {
    pub fn new(fields: Vec<Field>) -> Self {
        Self { fields }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Column {
    Utf8(Vec<Option<String>>),
    Int32(Vec<Option<i32>>),
    Int64(Vec<Option<i64>>),
    Float64(Vec<Option<f64>>),
    Boolean(Vec<Option<bool>>),
}

impl Column {
    pub fn utf8<S: Into<String>>(values: impl IntoIterator<Item = S>) -> Self {
        Column::Utf8(values.into_iter().map(|v| Some(v.into())).collect())
    }

    pub fn utf8_opt<S: Into<String>>(values: impl IntoIterator<Item = Option<S>>) -> Self {
        Column::Utf8(values.into_iter().map(|v| v.map(Into::into)).collect())
    }

    pub fn int32(values: impl IntoIterator<Item = i32>) -> Self {
        Column::Int32(values.into_iter().map(Some).collect())
    }

    pub fn int64(values: impl IntoIterator<Item = i64>) -> Self {
        Column::Int64(values.into_iter().map(Some).collect())
    }

    pub fn float64(values: impl IntoIterator<Item = f64>) -> Self {
        Column::Float64(values.into_iter().map(Some).collect())
    }

    pub fn boolean(values: impl IntoIterator<Item = bool>) -> Self {
        Column::Boolean(values.into_iter().map(Some).collect())
    }

    pub fn data_type(&self) -> DataType {
        match self {
            Column::Utf8(_) => DataType::Utf8,
            Column::Int32(_) => DataType::Int32,
            Column::Int64(_) => DataType::Int64,
            Column::Float64(_) => DataType::Float64,
            Column::Boolean(_) => DataType::Boolean,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Column::Utf8(v) => v.len(),
            Column::Int32(v) => v.len(),
            Column::Int64(v) => v.len(),
            Column::Float64(v) => v.len(),
            Column::Boolean(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_null(&self, row: usize) -> bool {
        match self {
            Column::Utf8(v) => v[row].is_none(),
            Column::Int32(v) => v[row].is_none(),
            Column::Int64(v) => v[row].is_none(),
            Column::Float64(v) => v[row].is_none(),
            Column::Boolean(v) => v[row].is_none(),
        }
    }

    pub fn null_count(&self) -> usize {
        (0..self.len()).filter(|&i| self.is_null(i)).count()
    }

    pub fn as_utf8(&self) -> Option<&[Option<String>]> {
        match self {
            Column::Utf8(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_int32(&self) -> Option<&[Option<i32>]> {
        match self {
            Column::Int32(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_int64(&self) -> Option<&[Option<i64>]> {
        match self {
            Column::Int64(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_float64(&self) -> Option<&[Option<f64>]> {
        match self {
            Column::Float64(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_boolean(&self) -> Option<&[Option<bool>]> {
        match self {
            Column::Boolean(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordBatch {
    schema: Arc<Schema>,
    columns: Vec<Column>,
    num_rows: usize,
}

impl RecordBatch {
    /// Builds a batch, checking column count, types, lengths and nullability.
    pub fn try_new(schema: Arc<Schema>, columns: Vec<Column>) -> Result<Self> {
        let num_rows = columns.first().map_or(0, Column::len);
        Self::try_new_with_rows(schema, columns, num_rows)
    }

    /// Like [`RecordBatch::try_new`], with an explicit row count (needed only
    /// for batches that have no columns).
    pub fn try_new_with_rows(
        schema: Arc<Schema>,
        columns: Vec<Column>,
        num_rows: usize,
    ) -> Result<Self> {
        if schema.fields.len() != columns.len() {
            return Err(Error::Batch(format!(
                "schema has {} fields but {} columns were given",
                schema.fields.len(),
                columns.len()
            )));
        }
        for (field, col) in schema.fields.iter().zip(&columns) {
            if field.data_type != col.data_type() {
                return Err(Error::Batch(format!(
                    "column `{}` does not match its declared type",
                    field.name
                )));
            }
            if col.len() != num_rows {
                return Err(Error::Batch(format!(
                    "column `{}` has {} rows, expected {}",
                    field.name,
                    col.len(),
                    num_rows
                )));
            }
            if !field.nullable && col.null_count() > 0 {
                return Err(Error::Batch(format!(
                    "column `{}` is not nullable but contains a null",
                    field.name
                )));
            }
        }
        Ok(Self {
            schema,
            columns,
            num_rows,
        })
    }

    pub fn new_empty(schema: Arc<Schema>) -> Self {
        let columns = schema
            .fields
            .iter()
            .map(|f| match f.data_type {
                DataType::Utf8 => Column::Utf8(Vec::new()),
                DataType::Int32 => Column::Int32(Vec::new()),
                DataType::Int64 => Column::Int64(Vec::new()),
                DataType::Float64 => Column::Float64(Vec::new()),
                DataType::Boolean => Column::Boolean(Vec::new()),
            })
            .collect();
        Self {
            schema,
            columns,
            num_rows: 0,
        }
    }

    pub fn schema(&self) -> Arc<Schema> {
        self.schema.clone()
    }

    pub fn num_rows(&self) -> usize {
        self.num_rows
    }

    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn column(&self, index: usize) -> &Column {
        &self.columns[index]
    }

    pub fn column_by_name(&self, name: &str) -> Option<&Column> {
        self.schema.index_of(name).map(|i| &self.columns[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_null_in_non_nullable_column() {
        let schema = Arc::new(Schema::new(vec![Field::new("a", DataType::Utf8, false)]));
        let err = RecordBatch::try_new(schema, vec![Column::Utf8(vec![None])]).unwrap_err();
        assert!(matches!(err, Error::Batch(_)));
    }

    #[test]
    fn rejects_type_and_length_mismatch() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("a", DataType::Int32, false),
            Field::new("b", DataType::Utf8, false),
        ]));
        assert!(
            RecordBatch::try_new(schema.clone(), vec![Column::int32([1]), Column::int32([1])])
                .is_err()
        );
        assert!(
            RecordBatch::try_new(schema, vec![Column::int32([1, 2]), Column::utf8(["x"])]).is_err()
        );
    }
}
