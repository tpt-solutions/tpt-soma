// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Minimal Flight server for checking wire compatibility with a real Arrow
//! client (see `scripts/flight_interop_check.py`):
//!
//!     cargo run -p tpt-soma-flight --example interop_server -- 127.0.0.1:50999

use bytes::Bytes;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tpt_soma_flight::data::{flight_data_for, flight_data_to_batches};
use tpt_soma_flight::pb::{
    FlightData, FlightDescriptor, FlightEndpoint, FlightInfo, HandshakeRequest, HandshakeResponse,
    PutResult, SchemaResult, Ticket,
};
use tpt_soma_flight::service::{FlightService, FlightServiceServer};
use tpt_soma_flight::{Column, DataType, Field, RecordBatch, Schema};

fn sample() -> RecordBatch {
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
            Column::Float64(vec![Some(1.5), None, Some(-0.25)]),
            Column::Utf8(vec![Some("x".into()), None, Some("héllo".into())]),
            Column::Boolean(vec![Some(true), Some(false), None]),
        ],
    )
    .unwrap()
}

struct Interop;

#[tonic::async_trait]
impl FlightService for Interop {
    type HandshakeStream = ReceiverStream<Result<HandshakeResponse, Status>>;
    type DoGetStream = ReceiverStream<Result<FlightData, Status>>;
    type DoPutStream = ReceiverStream<Result<PutResult, Status>>;

    async fn handshake(
        &self,
        _r: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        Err(Status::unimplemented("handshake"))
    }

    async fn get_flight_info(
        &self,
        r: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        let descriptor = r.into_inner();
        let cmd = descriptor.cmd.clone();
        let mut info = FlightInfo::new()
            .with_schema(&sample().schema())
            .with_endpoint(FlightEndpoint {
                ticket: Some(Ticket::new(cmd)),
                ..Default::default()
            })
            .with_total_records(-1)
            .with_total_bytes(-1);
        // Arrow C++ rejects a FlightInfo that has no descriptor.
        info.flight_descriptor = Some(descriptor);
        Ok(Response::new(info))
    }

    async fn get_schema(
        &self,
        _r: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status> {
        Err(Status::unimplemented("get_schema"))
    }

    async fn do_get(&self, _r: Request<Ticket>) -> Result<Response<Self::DoGetStream>, Status> {
        let (schema, batch) =
            flight_data_for(&sample()).map_err(|e| Status::internal(e.to_string()))?;
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(Ok(schema)).await.ok();
        tx.send(Ok(batch)).await.ok();
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn do_put(
        &self,
        r: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        let mut stream = r.into_inner();
        let mut messages = Vec::new();
        while let Some(m) = stream.message().await? {
            messages.push(m);
        }
        let batches = flight_data_to_batches(&messages)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        for b in &batches {
            eprintln!("received batch: {:?}", b.columns());
        }
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(Ok(PutResult {
            app_metadata: Bytes::from(format!("{{\"rows\":{rows}}}")),
        }))
        .await
        .ok();
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:50999".into())
        .parse()?;
    eprintln!("interop server listening on {addr}");
    tonic::transport::Server::builder()
        .add_service(FlightServiceServer::new(Interop))
        .serve(addr)
        .await?;
    Ok(())
}
