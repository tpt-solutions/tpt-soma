// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! In-house replacement for the Apache-2.0-only `arrow` / `arrow-flight` /
//! `prost` stack, so the platform keeps a dependency chain usable under plain
//! MIT. It implements the subset of the Arrow columnar format and the Flight
//! protocol that the platform exchanges with `pyarrow.flight` and other
//! standard clients.
//!
//! - [`columnar`]: schemas and record batches for five value types.
//! - [`ipc`]: Arrow IPC messages (metadata V5), read and write.
//! - [`pb`]: protobuf encoding of the Flight messages.
//! - [`service`]: the Flight gRPC server and a small client, on `tonic`
//!   (feature `service`, on by default).

pub mod columnar;
pub mod data;
pub mod error;
pub(crate) mod fb;
pub mod ipc;
pub mod pb;
#[cfg(feature = "service")]
pub mod service;

pub use columnar::{Column, DataType, Field, RecordBatch, Schema};
pub use error::{Error, Result};
