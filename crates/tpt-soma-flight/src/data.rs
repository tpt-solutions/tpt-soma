// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Glue between record batches and Flight `FlightData` messages.

use crate::columnar::{RecordBatch, Schema};
use crate::error::Result;
use crate::ipc::{self, StreamDecoder};
use crate::pb::FlightData;
use bytes::Bytes;

/// The schema message that opens every record-batch stream.
pub fn schema_flight_data(schema: &Schema) -> FlightData {
    FlightData {
        data_header: Bytes::from(ipc::schema_message(schema)),
        ..Default::default()
    }
}

pub fn batch_flight_data(batch: &RecordBatch) -> Result<FlightData> {
    let (header, body) = ipc::batch_message(batch)?;
    Ok(FlightData {
        data_header: Bytes::from(header),
        data_body: Bytes::from(body),
        ..Default::default()
    })
}

/// `(schema message, batch message)`: the two `FlightData` messages a client
/// needs to reassemble one batch.
pub fn flight_data_for(batch: &RecordBatch) -> Result<(FlightData, FlightData)> {
    Ok((
        schema_flight_data(&batch.schema()),
        batch_flight_data(batch)?,
    ))
}

/// Decodes a complete stream of `FlightData` into its record batches.
pub fn flight_data_to_batches(messages: &[FlightData]) -> Result<Vec<RecordBatch>> {
    let mut decoder = StreamDecoder::new();
    let mut batches = Vec::new();
    for m in messages {
        if let Some(batch) = decoder.push(&m.data_header, &m.data_body)? {
            batches.push(batch);
        }
    }
    Ok(batches)
}
