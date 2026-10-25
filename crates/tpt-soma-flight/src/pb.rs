// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Protobuf encoding for the Arrow Flight messages this service speaks, with
//! field numbers from the public `Flight.proto`. Proto3 rules: default values
//! are omitted on write, unknown fields are skipped on read.

use crate::columnar::Schema;
use crate::error::{Result, pb};
use crate::ipc;
use bytes::Bytes;

pub trait Message: Default + Send + 'static {
    fn encode_raw(&self, out: &mut Vec<u8>);
    fn decode_raw(buf: &[u8]) -> Result<Self>;

    fn encode_to_vec(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_raw(&mut out);
        out
    }
}

// ---- wire primitives ------------------------------------------------------

const WIRE_VARINT: u8 = 0;
const WIRE_FIXED64: u8 = 1;
const WIRE_LEN: u8 = 2;
const WIRE_FIXED32: u8 = 5;

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_tag(out: &mut Vec<u8>, field: u32, wire: u8) {
    put_varint(out, ((field as u64) << 3) | wire as u64);
}

fn put_u64(out: &mut Vec<u8>, field: u32, v: u64) {
    if v != 0 {
        put_tag(out, field, WIRE_VARINT);
        put_varint(out, v);
    }
}

fn put_i64(out: &mut Vec<u8>, field: u32, v: i64) {
    put_u64(out, field, v as u64);
}

fn put_bool(out: &mut Vec<u8>, field: u32, v: bool) {
    put_u64(out, field, v as u64);
}

fn put_bytes(out: &mut Vec<u8>, field: u32, v: &[u8]) {
    if !v.is_empty() {
        put_tag(out, field, WIRE_LEN);
        put_varint(out, v.len() as u64);
        out.extend_from_slice(v);
    }
}

/// Writes a nested message, even when it encodes to nothing (presence matters).
fn put_message(out: &mut Vec<u8>, field: u32, m: &impl Message) {
    let inner = m.encode_to_vec();
    put_tag(out, field, WIRE_LEN);
    put_varint(out, inner.len() as u64);
    out.extend_from_slice(&inner);
}

enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Skipped,
}

struct Fields<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Fields<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn varint(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let Some(&byte) = self.buf.get(self.pos) else {
                return pb("truncated varint");
            };
            self.pos += 1;
            if shift == 63 && byte > 1 {
                return pb("varint overflows 64 bits");
            }
            value |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        pb("varint too long")
    }

    fn take(&mut self, len: u64) -> Result<&'a [u8]> {
        let end = usize::try_from(len)
            .ok()
            .and_then(|len| self.pos.checked_add(len))
            .filter(|end| *end <= self.buf.len());
        let Some(end) = end else {
            return pb("truncated field");
        };
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn next(&mut self) -> Result<Option<(u32, Value<'a>)>> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        let value = match (key & 7) as u8 {
            WIRE_VARINT => Value::Varint(self.varint()?),
            WIRE_LEN => {
                let len = self.varint()?;
                Value::Bytes(self.take(len)?)
            }
            WIRE_FIXED64 => {
                self.take(8)?;
                Value::Skipped
            }
            WIRE_FIXED32 => {
                self.take(4)?;
                Value::Skipped
            }
            other => return pb(format!("unsupported wire type {other}")),
        };
        Ok(Some((field, value)))
    }
}

fn utf8(bytes: &[u8]) -> Result<String> {
    std::str::from_utf8(bytes)
        .map(str::to_string)
        .or_else(|_| pb("string field is not valid UTF-8"))
}

// ---- messages -------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandshakeRequest {
    pub protocol_version: u64,
    pub payload: Bytes,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandshakeResponse {
    pub protocol_version: u64,
    pub payload: Bytes,
}

macro_rules! handshake_message {
    ($t:ty) => {
        impl Message for $t {
            fn encode_raw(&self, out: &mut Vec<u8>) {
                put_u64(out, 1, self.protocol_version);
                put_bytes(out, 2, &self.payload);
            }

            fn decode_raw(buf: &[u8]) -> Result<Self> {
                let mut m = Self::default();
                let mut f = Fields::new(buf);
                while let Some((n, v)) = f.next()? {
                    match (n, v) {
                        (1, Value::Varint(v)) => m.protocol_version = v,
                        (2, Value::Bytes(b)) => m.payload = Bytes::copy_from_slice(b),
                        _ => {}
                    }
                }
                Ok(m)
            }
        }
    };
}
handshake_message!(HandshakeRequest);
handshake_message!(HandshakeResponse);

/// `FlightDescriptor.type`: 1 = PATH, 2 = CMD.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlightDescriptor {
    pub kind: i32,
    pub cmd: Bytes,
    pub path: Vec<String>,
}

impl FlightDescriptor {
    pub fn new_cmd(cmd: impl Into<Bytes>) -> Self {
        Self {
            kind: 2,
            cmd: cmd.into(),
            path: Vec::new(),
        }
    }

    pub fn new_path(path: Vec<String>) -> Self {
        Self {
            kind: 1,
            cmd: Bytes::new(),
            path,
        }
    }
}

impl Message for FlightDescriptor {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        put_i64(out, 1, self.kind as i64);
        put_bytes(out, 2, &self.cmd);
        for p in &self.path {
            put_tag(out, 3, WIRE_LEN);
            put_varint(out, p.len() as u64);
            out.extend_from_slice(p.as_bytes());
        }
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            match (n, v) {
                (1, Value::Varint(v)) => m.kind = v as i32,
                (2, Value::Bytes(b)) => m.cmd = Bytes::copy_from_slice(b),
                (3, Value::Bytes(b)) => m.path.push(utf8(b)?),
                _ => {}
            }
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ticket {
    pub ticket: Bytes,
}

impl Ticket {
    pub fn new(ticket: impl Into<Bytes>) -> Self {
        Self {
            ticket: ticket.into(),
        }
    }
}

impl Message for Ticket {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        put_bytes(out, 1, &self.ticket);
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            if let (1, Value::Bytes(b)) = (n, v) {
                m.ticket = Bytes::copy_from_slice(b);
            }
        }
        Ok(m)
    }
}

/// A `Location` message: one URI.
#[derive(Debug, Clone, Default, PartialEq)]
struct Location {
    uri: String,
}

impl Message for Location {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        put_bytes(out, 1, self.uri.as_bytes());
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            if let (1, Value::Bytes(b)) = (n, v) {
                m.uri = utf8(b)?;
            }
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlightEndpoint {
    pub ticket: Option<Ticket>,
    /// Location URIs. Empty means "the server this info came from".
    pub locations: Vec<String>,
    pub app_metadata: Bytes,
}

impl Message for FlightEndpoint {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        if let Some(t) = &self.ticket {
            put_message(out, 1, t);
        }
        for uri in &self.locations {
            put_message(out, 2, &Location { uri: uri.clone() });
        }
        put_bytes(out, 4, &self.app_metadata);
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            match (n, v) {
                (1, Value::Bytes(b)) => m.ticket = Some(Ticket::decode_raw(b)?),
                (2, Value::Bytes(b)) => m.locations.push(Location::decode_raw(b)?.uri),
                (4, Value::Bytes(b)) => m.app_metadata = Bytes::copy_from_slice(b),
                _ => {}
            }
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlightInfo {
    /// IPC-encapsulated schema message; see [`ipc::encapsulate`].
    pub schema: Bytes,
    pub flight_descriptor: Option<FlightDescriptor>,
    pub endpoint: Vec<FlightEndpoint>,
    pub total_records: i64,
    pub total_bytes: i64,
    pub ordered: bool,
    pub app_metadata: Bytes,
}

impl FlightInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_schema(mut self, schema: &Schema) -> Self {
        self.schema = Bytes::from(ipc::encapsulate(&ipc::schema_message(schema)));
        self
    }

    pub fn with_endpoint(mut self, endpoint: FlightEndpoint) -> Self {
        self.endpoint.push(endpoint);
        self
    }

    pub fn with_total_records(mut self, n: i64) -> Self {
        self.total_records = n;
        self
    }

    pub fn with_total_bytes(mut self, n: i64) -> Self {
        self.total_bytes = n;
        self
    }

    /// Decodes the schema carried in this message.
    pub fn decode_schema(&self) -> Result<Schema> {
        ipc::decode_schema(ipc::strip_encapsulation(&self.schema)?)
    }
}

impl Message for FlightInfo {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        put_bytes(out, 1, &self.schema);
        if let Some(d) = &self.flight_descriptor {
            put_message(out, 2, d);
        }
        for e in &self.endpoint {
            put_message(out, 3, e);
        }
        put_i64(out, 4, self.total_records);
        put_i64(out, 5, self.total_bytes);
        put_bool(out, 6, self.ordered);
        put_bytes(out, 7, &self.app_metadata);
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            match (n, v) {
                (1, Value::Bytes(b)) => m.schema = Bytes::copy_from_slice(b),
                (2, Value::Bytes(b)) => {
                    m.flight_descriptor = Some(FlightDescriptor::decode_raw(b)?)
                }
                (3, Value::Bytes(b)) => m.endpoint.push(FlightEndpoint::decode_raw(b)?),
                (4, Value::Varint(v)) => m.total_records = v as i64,
                (5, Value::Varint(v)) => m.total_bytes = v as i64,
                (6, Value::Varint(v)) => m.ordered = v != 0,
                (7, Value::Bytes(b)) => m.app_metadata = Bytes::copy_from_slice(b),
                _ => {}
            }
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SchemaResult {
    pub schema: Bytes,
}

impl Message for SchemaResult {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        put_bytes(out, 1, &self.schema);
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            if let (1, Value::Bytes(b)) = (n, v) {
                m.schema = Bytes::copy_from_slice(b);
            }
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlightData {
    pub flight_descriptor: Option<FlightDescriptor>,
    pub data_header: Bytes,
    pub app_metadata: Bytes,
    pub data_body: Bytes,
}

impl Message for FlightData {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        if let Some(d) = &self.flight_descriptor {
            put_message(out, 1, d);
        }
        put_bytes(out, 2, &self.data_header);
        put_bytes(out, 3, &self.app_metadata);
        put_bytes(out, 1000, &self.data_body);
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            match (n, v) {
                (1, Value::Bytes(b)) => {
                    m.flight_descriptor = Some(FlightDescriptor::decode_raw(b)?)
                }
                (2, Value::Bytes(b)) => m.data_header = Bytes::copy_from_slice(b),
                (3, Value::Bytes(b)) => m.app_metadata = Bytes::copy_from_slice(b),
                (1000, Value::Bytes(b)) => m.data_body = Bytes::copy_from_slice(b),
                _ => {}
            }
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PutResult {
    pub app_metadata: Bytes,
}

impl Message for PutResult {
    fn encode_raw(&self, out: &mut Vec<u8>) {
        put_bytes(out, 1, &self.app_metadata);
    }

    fn decode_raw(buf: &[u8]) -> Result<Self> {
        let mut m = Self::default();
        let mut f = Fields::new(buf);
        while let Some((n, v)) = f.next()? {
            if let (1, Value::Bytes(b)) = (n, v) {
                m.app_metadata = Bytes::copy_from_slice(b);
            }
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columnar::{DataType, Field};

    fn round_trip<M: Message + PartialEq + std::fmt::Debug>(m: M) {
        let bytes = m.encode_to_vec();
        assert_eq!(M::decode_raw(&bytes).unwrap(), m);
    }

    #[test]
    fn messages_round_trip() {
        round_trip(HandshakeRequest {
            protocol_version: 7,
            payload: Bytes::from_static(b"hi"),
        });
        round_trip(FlightDescriptor::new_cmd("variants:abc"));
        round_trip(FlightDescriptor::new_path(vec!["a".into(), "b".into()]));
        round_trip(Ticket::new("variants:abc"));
        round_trip(FlightData {
            flight_descriptor: Some(FlightDescriptor::new_cmd("device_state")),
            data_header: Bytes::from_static(b"\x01\x02"),
            app_metadata: Bytes::from_static(b"m"),
            data_body: Bytes::from(vec![9u8; 300]),
        });
        round_trip(PutResult {
            app_metadata: Bytes::from_static(b"{\"rows\":3}"),
        });
        let info = FlightInfo::new()
            .with_schema(&Schema::new(vec![Field::new("a", DataType::Utf8, false)]))
            .with_endpoint(FlightEndpoint {
                ticket: Some(Ticket::new("t")),
                ..Default::default()
            })
            .with_total_records(-1)
            .with_total_bytes(-1);
        let bytes = info.encode_to_vec();
        let back = FlightInfo::decode_raw(&bytes).unwrap();
        assert_eq!(back, info);
        assert_eq!(back.total_records, -1);
        assert_eq!(back.decode_schema().unwrap().fields[0].name, "a");
    }

    #[test]
    fn known_wire_bytes() {
        // Ticket { ticket: "ab" } is field 1, length-delimited.
        assert_eq!(Ticket::new("ab").encode_to_vec(), [0x0A, 0x02, b'a', b'b']);
        // Defaults are omitted entirely.
        assert!(PutResult::default().encode_to_vec().is_empty());
        // -1 encodes as a 10-byte varint (two's complement), field 4.
        let info = FlightInfo::new().with_total_records(-1);
        let bytes = info.encode_to_vec();
        assert_eq!(bytes[0], 0x20);
        assert_eq!(bytes.len(), 11);
    }

    #[test]
    fn unknown_fields_are_skipped_and_garbage_is_an_error() {
        // Unknown varint field 15 followed by a real ticket.
        let mut buf = vec![0x78, 0x05];
        buf.extend(Ticket::new("x").encode_to_vec());
        assert_eq!(Ticket::decode_raw(&buf).unwrap(), Ticket::new("x"));

        assert!(Ticket::decode_raw(&[0x0A, 0x05, b'a']).is_err()); // truncated
        assert!(Ticket::decode_raw(&[0x80]).is_err()); // truncated varint
        assert!(Ticket::decode_raw(&[0x0B]).is_err()); // group wire type
    }
}
