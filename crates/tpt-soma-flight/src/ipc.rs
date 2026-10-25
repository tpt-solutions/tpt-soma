// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Arrow-compatible IPC messages (metadata version V5) for the types in
//! [`crate::columnar`]. This is the wire format `pyarrow.flight` and the other
//! standard Arrow clients speak, written from the public Arrow columnar format
//! specification. Dictionaries, compression and nested types are not supported
//! and are rejected on read.

use crate::columnar::{Column, DataType, Field, RecordBatch, Schema};
use crate::error::{Error, Result, ipc};
use crate::fb::{Builder, Reader, Val};
use std::sync::Arc;

const VERSION_V5: i16 = 4;
const HEADER_SCHEMA: u8 = 1;
const HEADER_RECORD_BATCH: u8 = 3;
const TYPE_INT: u8 = 2;
const TYPE_FLOAT: u8 = 3;
const TYPE_UTF8: u8 = 5;
const TYPE_BOOL: u8 = 6;
const PRECISION_DOUBLE: i16 = 2;
const CONTINUATION: [u8; 4] = [0xFF; 4];

// ---- encoding -------------------------------------------------------------

/// The flatbuffer `Message` for a schema (no stream framing).
pub fn schema_message(schema: &Schema) -> Vec<u8> {
    let mut b = Builder::new();
    let msg = b.table(&[
        (0, Val::I16(VERSION_V5)),
        (1, Val::U8(HEADER_SCHEMA)),
        (2, Val::Ref),
        (3, Val::I64(0)),
    ]);
    let sch = b.table(&[(0, Val::I16(0)), (1, Val::Ref)]);
    b.patch(msg.slot(2), sch.pos);
    let (fields_vec, slots) = b.offset_vector(schema.fields.len());
    b.patch(sch.slot(1), fields_vec);
    for (field, slot) in schema.fields.iter().zip(slots) {
        let pos = write_field(&mut b, field);
        b.patch(slot, pos);
    }
    b.finish(&msg)
}

fn write_field(b: &mut Builder, field: &Field) -> usize {
    let type_id = match field.data_type {
        DataType::Int32 | DataType::Int64 => TYPE_INT,
        DataType::Float64 => TYPE_FLOAT,
        DataType::Utf8 => TYPE_UTF8,
        DataType::Boolean => TYPE_BOOL,
    };
    let table = b.table(&[
        (0, Val::Ref),
        (1, Val::Bool(field.nullable)),
        (2, Val::U8(type_id)),
        (3, Val::Ref),
        (5, Val::Ref),
    ]);
    let name = b.string(&field.name);
    b.patch(table.slot(0), name);
    let type_table = match field.data_type {
        DataType::Int32 => b.table(&[(0, Val::I32(32)), (1, Val::Bool(true))]),
        DataType::Int64 => b.table(&[(0, Val::I32(64)), (1, Val::Bool(true))]),
        DataType::Float64 => b.table(&[(0, Val::I16(PRECISION_DOUBLE))]),
        DataType::Utf8 | DataType::Boolean => b.table(&[]),
    };
    b.patch(table.slot(3), type_table.pos);
    // Arrow C++ requires the children vector to be present, even when empty.
    let (children, _) = b.offset_vector(0);
    b.patch(table.slot(5), children);
    table.pos
}

fn push_buffer(body: &mut Vec<u8>, buffers: &mut Vec<(i64, i64)>, data: &[u8]) {
    while !body.len().is_multiple_of(8) {
        body.push(0);
    }
    buffers.push((body.len() as i64, data.len() as i64));
    body.extend_from_slice(data);
}

fn bitmap(len: usize, bit: impl Fn(usize) -> bool) -> Vec<u8> {
    let mut out = vec![0u8; len.div_ceil(8)];
    for i in 0..len {
        if bit(i) {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    out
}

/// Encodes a batch as `(flatbuffer message, body)`, the two halves of a Flight
/// `FlightData` message.
pub fn batch_message(batch: &RecordBatch) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut body = Vec::new();
    let mut buffers: Vec<(i64, i64)> = Vec::new();
    let mut nodes: Vec<(i64, i64)> = Vec::new();

    for column in batch.columns() {
        let null_count = column.null_count();
        nodes.push((column.len() as i64, null_count as i64));
        // A column without nulls carries an empty validity buffer.
        if null_count == 0 {
            push_buffer(&mut body, &mut buffers, &[]);
        } else {
            let bits = bitmap(column.len(), |i| !column.is_null(i));
            push_buffer(&mut body, &mut buffers, &bits);
        }
        match column {
            Column::Utf8(values) => {
                let mut offsets = Vec::with_capacity(4 * (values.len() + 1));
                let mut data = Vec::new();
                offsets.extend_from_slice(&0i32.to_le_bytes());
                for v in values {
                    if let Some(s) = v {
                        data.extend_from_slice(s.as_bytes());
                    }
                    let end = i32::try_from(data.len()).map_err(|_| {
                        Error::Batch("string column exceeds 2 GiB in one batch".into())
                    })?;
                    offsets.extend_from_slice(&end.to_le_bytes());
                }
                push_buffer(&mut body, &mut buffers, &offsets);
                push_buffer(&mut body, &mut buffers, &data);
            }
            Column::Int32(values) => {
                let data: Vec<u8> = values
                    .iter()
                    .flat_map(|v| v.unwrap_or(0).to_le_bytes())
                    .collect();
                push_buffer(&mut body, &mut buffers, &data);
            }
            Column::Int64(values) => {
                let data: Vec<u8> = values
                    .iter()
                    .flat_map(|v| v.unwrap_or(0).to_le_bytes())
                    .collect();
                push_buffer(&mut body, &mut buffers, &data);
            }
            Column::Float64(values) => {
                let data: Vec<u8> = values
                    .iter()
                    .flat_map(|v| v.unwrap_or(0.0).to_le_bytes())
                    .collect();
                push_buffer(&mut body, &mut buffers, &data);
            }
            Column::Boolean(values) => {
                let bits = bitmap(values.len(), |i| values[i] == Some(true));
                push_buffer(&mut body, &mut buffers, &bits);
            }
        }
    }
    while body.len() % 8 != 0 {
        body.push(0);
    }

    let mut b = Builder::new();
    let msg = b.table(&[
        (0, Val::I16(VERSION_V5)),
        (1, Val::U8(HEADER_RECORD_BATCH)),
        (2, Val::Ref),
        (3, Val::I64(body.len() as i64)),
    ]);
    let rb = b.table(&[
        (0, Val::I64(batch.num_rows() as i64)),
        (1, Val::Ref),
        (2, Val::Ref),
    ]);
    b.patch(msg.slot(2), rb.pos);
    let nodes_vec = b.pair_vector(&nodes);
    b.patch(rb.slot(1), nodes_vec);
    let buffers_vec = b.pair_vector(&buffers);
    b.patch(rb.slot(2), buffers_vec);
    Ok((b.finish(&msg), body))
}

/// Wraps a message in the IPC stream framing (continuation marker, length,
/// 8-byte padding). `FlightInfo.schema` and `SchemaResult.schema` carry the
/// schema in this form; `FlightData.data_header` does not.
pub fn encapsulate(message: &[u8]) -> Vec<u8> {
    let padded = message.len().next_multiple_of(8);
    let mut out = Vec::with_capacity(8 + padded);
    out.extend_from_slice(&CONTINUATION);
    out.extend_from_slice(&(padded as u32).to_le_bytes());
    out.extend_from_slice(message);
    out.resize(8 + padded, 0);
    out
}

/// Inverse of [`encapsulate`]. Also accepts the pre-1.0 form with no marker.
pub fn strip_encapsulation(bytes: &[u8]) -> Result<&[u8]> {
    let rest = bytes.strip_prefix(&CONTINUATION).unwrap_or(bytes);
    let len_bytes: [u8; 4] = rest
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .map_or_else(|| ipc("truncated IPC frame"), Ok)?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    rest.get(4..)
        .and_then(|r| r.get(..len))
        .map_or_else(|| ipc("truncated IPC frame"), Ok)
}

// ---- decoding -------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Schema,
    RecordBatch,
    Other(u8),
}

pub fn message_kind(header: &[u8]) -> Result<MessageKind> {
    let r = Reader(header);
    let root = r.root()?;
    Ok(match r.u8_field(root, 1, 0)? {
        HEADER_SCHEMA => MessageKind::Schema,
        HEADER_RECORD_BATCH => MessageKind::RecordBatch,
        other => MessageKind::Other(other),
    })
}

pub fn decode_schema(header: &[u8]) -> Result<Schema> {
    let r = Reader(header);
    let root = r.root()?;
    if r.u8_field(root, 1, 0)? != HEADER_SCHEMA {
        return ipc("message is not a schema");
    }
    let schema = r
        .table_field(root, 2)?
        .map_or_else(|| ipc("schema message has no schema"), Ok)?;
    let mut fields = Vec::new();
    if let Some((first, count)) = r.vector_field(schema, 1)? {
        for i in 0..count {
            let field = r.vector_table(first, i)?;
            fields.push(read_field(&r, field)?);
        }
    }
    Ok(Schema::new(fields))
}

fn read_field(r: &Reader<'_>, field: usize) -> Result<Field> {
    let name = r.string_field(field, 0)?.unwrap_or_default().to_string();
    let nullable = r.bool_field(field, 1)?;
    if r.has_field(field, 4)? {
        return ipc(format!(
            "field `{name}`: dictionary encoding is not supported"
        ));
    }
    if let Some((_, count)) = r.vector_field(field, 5)?
        && count > 0
    {
        return ipc(format!("field `{name}`: nested types are not supported"));
    }
    let type_table = r.table_field(field, 3)?;
    let data_type = match (r.u8_field(field, 2, 0)?, type_table) {
        (TYPE_UTF8, _) => DataType::Utf8,
        (TYPE_BOOL, _) => DataType::Boolean,
        (TYPE_INT, Some(t)) => {
            let signed = r.bool_field(t, 1)?;
            match (r.i32_field(t, 0, 0)?, signed) {
                (32, true) => DataType::Int32,
                (64, true) => DataType::Int64,
                (bits, signed) => {
                    return ipc(format!(
                        "field `{name}`: unsupported integer (bits={bits}, signed={signed})"
                    ));
                }
            }
        }
        (TYPE_FLOAT, Some(t)) if r.i16_field(t, 0, 0)? == PRECISION_DOUBLE => DataType::Float64,
        (other, _) => {
            return ipc(format!("field `{name}`: unsupported type id {other}"));
        }
    };
    Ok(Field::new(name, data_type, nullable))
}

fn buffer_slice(body: &[u8], (offset, len): (i64, i64)) -> Result<&[u8]> {
    let (Ok(offset), Ok(len)) = (usize::try_from(offset), usize::try_from(len)) else {
        return ipc("negative buffer offset or length");
    };
    offset
        .checked_add(len)
        .and_then(|end| body.get(offset..end))
        .map_or_else(|| ipc("buffer lies outside the message body"), Ok)
}

fn validity(buf: &[u8], rows: usize, null_count: i64) -> Result<Option<&[u8]>> {
    if buf.is_empty() {
        if null_count > 0 && rows > 0 {
            return ipc("column reports nulls but has no validity bitmap");
        }
        return Ok(None);
    }
    if buf.len() < rows.div_ceil(8) {
        return ipc("validity bitmap is too short");
    }
    Ok(Some(buf))
}

fn is_valid(bits: Option<&[u8]>, row: usize) -> bool {
    bits.is_none_or(|b| (b[row / 8] >> (row % 8)) & 1 == 1)
}

fn fixed_width(buf: &[u8], rows: usize, width: usize) -> Result<&[u8]> {
    match rows.checked_mul(width) {
        Some(need) if buf.len() >= need => Ok(&buf[..need]),
        _ => ipc("data buffer is shorter than the row count requires"),
    }
}

fn take<'a>(body: &'a [u8], buffers: &[(i64, i64)], next: &mut usize) -> Result<&'a [u8]> {
    let buf = buffer_slice(body, buffers[*next])?;
    *next += 1;
    Ok(buf)
}

/// Decodes a record batch. `header` is the flatbuffer message, `body` the data
/// that follows it, and `schema` the schema sent earlier in the stream.
pub fn decode_batch(header: &[u8], body: &[u8], schema: &Arc<Schema>) -> Result<RecordBatch> {
    let r = Reader(header);
    let root = r.root()?;
    if r.u8_field(root, 1, 0)? != HEADER_RECORD_BATCH {
        return ipc("message is not a record batch");
    }
    if r.i64_field(root, 3, 0)? > body.len() as i64 {
        return ipc("message body is shorter than its declared length");
    }
    let rb = r
        .table_field(root, 2)?
        .map_or_else(|| ipc("record batch message has no batch"), Ok)?;
    if r.has_field(rb, 3)? {
        return ipc("compressed record batches are not supported");
    }
    let Ok(rows) = usize::try_from(r.i64_field(rb, 0, 0)?) else {
        return ipc("negative row count");
    };

    let mut nodes = Vec::new();
    if let Some((first, count)) = r.vector_field(rb, 1)? {
        for i in 0..count {
            let at = first + 16 * i;
            nodes.push((r.i64(at)?, r.i64(at + 8)?));
        }
    }
    let mut buffers = Vec::new();
    if let Some((first, count)) = r.vector_field(rb, 2)? {
        for i in 0..count {
            let at = first + 16 * i;
            buffers.push((r.i64(at)?, r.i64(at + 8)?));
        }
    }

    if nodes.len() != schema.fields.len() {
        return ipc("batch column count does not match the schema");
    }
    let expected_buffers: usize = schema
        .fields
        .iter()
        .map(|f| if f.data_type == DataType::Utf8 { 3 } else { 2 })
        .sum();
    if buffers.len() != expected_buffers {
        return ipc("batch buffer count does not match the schema");
    }

    let mut next = 0usize;
    let mut columns = Vec::with_capacity(schema.fields.len());
    for (field, &(node_len, null_count)) in schema.fields.iter().zip(&nodes) {
        if node_len != rows as i64 {
            return ipc("column length differs from the batch row count");
        }
        let bits = validity(take(body, &buffers, &mut next)?, rows, null_count)?;
        let column = match field.data_type {
            DataType::Utf8 => {
                let offsets = fixed_width(take(body, &buffers, &mut next)?, rows + 1, 4)?;
                let data = take(body, &buffers, &mut next)?;
                let at = |i: usize| -> i32 {
                    i32::from_le_bytes(offsets[4 * i..4 * i + 4].try_into().unwrap())
                };
                let mut values = Vec::with_capacity(rows);
                let mut prev = at(0);
                if prev != 0 {
                    return ipc("string offsets must start at zero");
                }
                for row in 0..rows {
                    let end = at(row + 1);
                    if end < prev || end as usize > data.len() {
                        return ipc("string offsets are not monotonic or exceed the data");
                    }
                    let bytes = &data[prev as usize..end as usize];
                    prev = end;
                    values.push(if is_valid(bits, row) {
                        Some(
                            std::str::from_utf8(bytes)
                                .map_err(|_| Error::Ipc("string is not valid UTF-8".into()))?
                                .to_string(),
                        )
                    } else {
                        None
                    });
                }
                Column::Utf8(values)
            }
            DataType::Int32 => {
                let data = fixed_width(take(body, &buffers, &mut next)?, rows, 4)?;
                Column::Int32(
                    (0..rows)
                        .map(|i| {
                            is_valid(bits, i).then(|| {
                                i32::from_le_bytes(data[4 * i..4 * i + 4].try_into().unwrap())
                            })
                        })
                        .collect(),
                )
            }
            DataType::Int64 => {
                let data = fixed_width(take(body, &buffers, &mut next)?, rows, 8)?;
                Column::Int64(
                    (0..rows)
                        .map(|i| {
                            is_valid(bits, i).then(|| {
                                i64::from_le_bytes(data[8 * i..8 * i + 8].try_into().unwrap())
                            })
                        })
                        .collect(),
                )
            }
            DataType::Float64 => {
                let data = fixed_width(take(body, &buffers, &mut next)?, rows, 8)?;
                Column::Float64(
                    (0..rows)
                        .map(|i| {
                            is_valid(bits, i).then(|| {
                                f64::from_le_bytes(data[8 * i..8 * i + 8].try_into().unwrap())
                            })
                        })
                        .collect(),
                )
            }
            DataType::Boolean => {
                let data = take(body, &buffers, &mut next)?;
                if data.len() < rows.div_ceil(8) {
                    return ipc("boolean data buffer is shorter than the row count requires");
                }
                Column::Boolean(
                    (0..rows)
                        .map(|i| is_valid(bits, i).then(|| (data[i / 8] >> (i % 8)) & 1 == 1))
                        .collect(),
                )
            }
        };
        columns.push(column);
    }
    RecordBatch::try_new_with_rows(schema.clone(), columns, rows)
}

/// Reassembles a Flight record-batch stream: the schema message first, then
/// batches. Messages with an empty header (metadata-only) are skipped.
#[derive(Debug, Default)]
pub struct StreamDecoder {
    schema: Option<Arc<Schema>>,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, header: &[u8], body: &[u8]) -> Result<Option<RecordBatch>> {
        if header.is_empty() {
            return Ok(None);
        }
        match message_kind(header)? {
            MessageKind::Schema => {
                self.schema = Some(Arc::new(decode_schema(header)?));
                Ok(None)
            }
            MessageKind::RecordBatch => {
                let Some(schema) = &self.schema else {
                    return ipc("record batch received before the schema");
                };
                decode_batch(header, body, schema).map(Some)
            }
            MessageKind::Other(id) => ipc(format!(
                "unsupported IPC message type {id} (dictionaries are not supported)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("pos", DataType::Int32, false),
            Field::new("ts", DataType::Int64, false),
            Field::new("val", DataType::Float64, true),
            Field::new("note", DataType::Utf8, true),
            Field::new("flag", DataType::Boolean, true),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Column::utf8(["a", "bb", ""]),
                Column::int32([1, -2, 3]),
                Column::int64([10, 20, i64::MAX]),
                Column::Float64(vec![Some(1.5), None, Some(f64::MIN)]),
                Column::Utf8(vec![Some("x".into()), None, Some("héllo".into())]),
                Column::Boolean(vec![Some(true), Some(false), None]),
            ],
        )
        .unwrap()
    }

    #[test]
    fn batch_round_trip() {
        let batch = sample();
        let schema_msg = schema_message(&batch.schema());
        let (header, body) = batch_message(&batch).unwrap();
        let mut dec = StreamDecoder::new();
        assert!(dec.push(&schema_msg, &[]).unwrap().is_none());
        let back = dec.push(&header, &body).unwrap().unwrap();
        assert_eq!(back, batch);
    }

    #[test]
    fn empty_batch_round_trip() {
        let batch = RecordBatch::new_empty(sample().schema());
        let (header, body) = batch_message(&batch).unwrap();
        let mut dec = StreamDecoder::new();
        dec.push(&schema_message(&batch.schema()), &[]).unwrap();
        assert_eq!(dec.push(&header, &body).unwrap().unwrap(), batch);
    }

    #[test]
    fn messages_are_eight_byte_aligned() {
        let batch = sample();
        let (header, body) = batch_message(&batch).unwrap();
        assert_eq!(header.len() % 8, 0);
        assert_eq!(body.len() % 8, 0);
        assert_eq!(schema_message(&batch.schema()).len() % 8, 0);
    }

    #[test]
    fn encapsulation_round_trip() {
        let msg = schema_message(&sample().schema());
        let framed = encapsulate(&msg);
        assert_eq!(&framed[..4], &[0xFF; 4]);
        assert_eq!(strip_encapsulation(&framed).unwrap(), &msg[..]);
    }

    #[test]
    fn corrupt_input_never_panics() {
        let batch = sample();
        let schema = batch.schema();
        let (header, body) = batch_message(&batch).unwrap();
        let schema_msg = schema_message(&schema);
        // Truncations.
        for cut in 0..header.len() {
            let _ = decode_batch(&header[..cut], &body, &schema);
        }
        for cut in 0..body.len() {
            let _ = decode_batch(&header, &body[..cut], &schema);
        }
        for cut in 0..schema_msg.len() {
            let _ = decode_schema(&schema_msg[..cut]);
        }
        // Single-byte corruptions.
        for i in 0..header.len() {
            let mut h = header.clone();
            h[i] ^= 0xFF;
            let _ = decode_batch(&h, &body, &schema);
        }
        for i in 0..schema_msg.len() {
            let mut m = schema_msg.clone();
            m[i] ^= 0xFF;
            let _ = decode_schema(&m);
        }
    }

    #[test]
    fn batch_before_schema_is_rejected() {
        let (header, body) = batch_message(&sample()).unwrap();
        assert!(StreamDecoder::new().push(&header, &body).is_err());
    }
}
