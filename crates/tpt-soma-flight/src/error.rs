// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// A batch or schema is internally inconsistent.
    #[error("invalid batch: {0}")]
    Batch(String),
    /// Malformed or unsupported IPC (flatbuffer) data.
    #[error("ipc: {0}")]
    Ipc(String),
    /// Malformed protobuf data.
    #[error("protobuf: {0}")]
    Protobuf(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn ipc<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Ipc(msg.into()))
}

pub(crate) fn pb<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Protobuf(msg.into()))
}
