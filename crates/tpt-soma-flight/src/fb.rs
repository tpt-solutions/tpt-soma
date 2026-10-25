// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Just enough of the FlatBuffers binary format to write and read Arrow IPC
//! metadata. The writer lays objects out front to back: a parent is written
//! first with zeroed offset slots, then each child is appended and patched in,
//! so every offset points forward. The reader bounds-checks every access,
//! because `do_put` feeds it untrusted bytes.

use crate::error::{Result, ipc};

pub(crate) enum Val {
    U8(u8),
    Bool(bool),
    I16(i16),
    I32(i32),
    I64(i64),
    /// A 4-byte offset slot, filled in later with [`Builder::patch`].
    Ref,
}

impl Val {
    fn size(&self) -> usize {
        match self {
            Val::U8(_) | Val::Bool(_) => 1,
            Val::I16(_) => 2,
            Val::I32(_) | Val::Ref => 4,
            Val::I64(_) => 8,
        }
    }
}

pub(crate) struct Table {
    pub pos: usize,
    slots: Vec<(u16, usize)>,
}

impl Table {
    /// Absolute position of the offset slot for a `Val::Ref` field.
    pub fn slot(&self, slot: u16) -> usize {
        self.slots
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, at)| *at)
            .expect("slot was declared as Val::Ref")
    }
}

pub(crate) struct Builder {
    buf: Vec<u8>,
}

impl Builder {
    pub fn new() -> Self {
        // The first four bytes hold the root table offset, patched in `finish`.
        Self { buf: vec![0; 4] }
    }

    fn pad_to(&mut self, align: usize) {
        while !self.buf.len().is_multiple_of(align) {
            self.buf.push(0);
        }
    }

    pub fn table(&mut self, fields: &[(u16, Val)]) -> Table {
        let slot_count = fields
            .iter()
            .map(|(s, _)| *s as usize + 1)
            .max()
            .unwrap_or(0);

        // Largest fields first, so natural alignment needs no padding inside.
        let mut order: Vec<usize> = (0..fields.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(fields[i].1.size()));
        let mut cursor = 4usize; // the signed vtable offset comes first
        let mut vtable_entries = vec![0u16; slot_count];
        let mut place = vec![0usize; fields.len()];
        for i in order {
            let size = fields[i].1.size();
            cursor = cursor.next_multiple_of(size);
            place[i] = cursor;
            vtable_entries[fields[i].0 as usize] = cursor as u16;
            cursor += size;
        }
        let table_size = cursor;

        self.pad_to(2);
        let vtable_pos = self.buf.len();
        self.buf
            .extend_from_slice(&((4 + 2 * slot_count) as u16).to_le_bytes());
        self.buf
            .extend_from_slice(&(table_size as u16).to_le_bytes());
        for entry in &vtable_entries {
            self.buf.extend_from_slice(&entry.to_le_bytes());
        }
        self.pad_to(8);
        let table_pos = self.buf.len();
        self.buf.resize(table_pos + table_size, 0);
        let soffset = (table_pos - vtable_pos) as i32;
        self.buf[table_pos..table_pos + 4].copy_from_slice(&soffset.to_le_bytes());

        let mut slots = Vec::new();
        for (i, (slot, val)) in fields.iter().enumerate() {
            let at = table_pos + place[i];
            match val {
                Val::U8(v) => self.buf[at] = *v,
                Val::Bool(v) => self.buf[at] = *v as u8,
                Val::I16(v) => self.buf[at..at + 2].copy_from_slice(&v.to_le_bytes()),
                Val::I32(v) => self.buf[at..at + 4].copy_from_slice(&v.to_le_bytes()),
                Val::I64(v) => self.buf[at..at + 8].copy_from_slice(&v.to_le_bytes()),
                Val::Ref => slots.push((*slot, at)),
            }
        }
        Table {
            pos: table_pos,
            slots,
        }
    }

    pub fn string(&mut self, s: &str) -> usize {
        self.pad_to(4);
        let pos = self.buf.len();
        self.buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        pos
    }

    /// A vector of `n` offsets (to tables). Returns the vector position and the
    /// absolute position of each element slot, for patching.
    pub fn offset_vector(&mut self, n: usize) -> (usize, Vec<usize>) {
        self.pad_to(4);
        let pos = self.buf.len();
        self.buf.extend_from_slice(&(n as u32).to_le_bytes());
        let slots = (0..n).map(|i| pos + 4 + 4 * i).collect();
        self.buf.resize(pos + 4 + 4 * n, 0);
        (pos, slots)
    }

    /// A vector of 16-byte structs made of two `i64` fields (`FieldNode`, `Buffer`).
    pub fn pair_vector(&mut self, items: &[(i64, i64)]) -> usize {
        while !(self.buf.len() + 4).is_multiple_of(8) {
            self.buf.push(0);
        }
        let pos = self.buf.len();
        self.buf
            .extend_from_slice(&(items.len() as u32).to_le_bytes());
        for (a, b) in items {
            self.buf.extend_from_slice(&a.to_le_bytes());
            self.buf.extend_from_slice(&b.to_le_bytes());
        }
        pos
    }

    pub fn patch(&mut self, slot_pos: usize, target: usize) {
        debug_assert!(target > slot_pos, "offsets must point forward");
        let rel = (target - slot_pos) as u32;
        self.buf[slot_pos..slot_pos + 4].copy_from_slice(&rel.to_le_bytes());
    }

    pub fn finish(mut self, root: &Table) -> Vec<u8> {
        self.patch(0, root.pos);
        self.pad_to(8);
        self.buf
    }
}

pub(crate) struct Reader<'a>(pub &'a [u8]);

impl<'a> Reader<'a> {
    fn bytes(&self, pos: usize, len: usize) -> Result<&'a [u8]> {
        pos.checked_add(len)
            .and_then(|end| self.0.get(pos..end))
            .map_or_else(|| ipc("flatbuffer read out of bounds"), Ok)
    }

    pub fn u8(&self, pos: usize) -> Result<u8> {
        Ok(self.bytes(pos, 1)?[0])
    }

    fn u16(&self, pos: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(pos, 2)?.try_into().unwrap()))
    }

    fn i16(&self, pos: usize) -> Result<i16> {
        Ok(i16::from_le_bytes(self.bytes(pos, 2)?.try_into().unwrap()))
    }

    fn i32(&self, pos: usize) -> Result<i32> {
        Ok(i32::from_le_bytes(self.bytes(pos, 4)?.try_into().unwrap()))
    }

    fn u32(&self, pos: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(pos, 4)?.try_into().unwrap()))
    }

    pub fn i64(&self, pos: usize) -> Result<i64> {
        Ok(i64::from_le_bytes(self.bytes(pos, 8)?.try_into().unwrap()))
    }

    /// Follows the unsigned offset stored at `pos`.
    fn indirect(&self, pos: usize) -> Result<usize> {
        let target = pos
            .checked_add(self.u32(pos)? as usize)
            .filter(|t| *t < self.0.len());
        target.map_or_else(|| ipc("flatbuffer offset out of bounds"), Ok)
    }

    pub fn root(&self) -> Result<usize> {
        self.indirect(0)
    }

    /// Absolute position of a table field's data, or `None` if it is absent.
    fn field_pos(&self, table: usize, slot: u16) -> Result<Option<usize>> {
        let soffset = self.i32(table)? as i64;
        let vtable = table as i64 - soffset;
        if vtable < 0 {
            return ipc("flatbuffer vtable out of bounds");
        }
        let vtable = vtable as usize;
        let vtable_len = self.u16(vtable)? as usize;
        let entry = 4 + 2 * slot as usize;
        if entry + 2 > vtable_len {
            return Ok(None);
        }
        let offset = self.u16(vtable + entry)? as usize;
        Ok((offset != 0).then_some(table + offset))
    }

    pub fn u8_field(&self, table: usize, slot: u16, default: u8) -> Result<u8> {
        self.field_pos(table, slot)?
            .map_or(Ok(default), |p| self.u8(p))
    }

    pub fn bool_field(&self, table: usize, slot: u16) -> Result<bool> {
        Ok(self.u8_field(table, slot, 0)? != 0)
    }

    pub fn i16_field(&self, table: usize, slot: u16, default: i16) -> Result<i16> {
        self.field_pos(table, slot)?
            .map_or(Ok(default), |p| self.i16(p))
    }

    pub fn i32_field(&self, table: usize, slot: u16, default: i32) -> Result<i32> {
        self.field_pos(table, slot)?
            .map_or(Ok(default), |p| self.i32(p))
    }

    pub fn i64_field(&self, table: usize, slot: u16, default: i64) -> Result<i64> {
        self.field_pos(table, slot)?
            .map_or(Ok(default), |p| self.i64(p))
    }

    pub fn has_field(&self, table: usize, slot: u16) -> Result<bool> {
        Ok(self.field_pos(table, slot)?.is_some())
    }

    pub fn table_field(&self, table: usize, slot: u16) -> Result<Option<usize>> {
        self.field_pos(table, slot)?
            .map(|p| self.indirect(p))
            .transpose()
    }

    pub fn string_field(&self, table: usize, slot: u16) -> Result<Option<&'a str>> {
        let Some(p) = self.field_pos(table, slot)? else {
            return Ok(None);
        };
        let start = self.indirect(p)?;
        let len = self.u32(start)? as usize;
        let raw = self.bytes(start + 4, len)?;
        std::str::from_utf8(raw)
            .map(Some)
            .or_else(|_| ipc("string is not valid UTF-8"))
    }

    /// `(position of first element, element count)` of a vector field. The
    /// count is checked against the buffer size so a hostile length cannot
    /// drive a huge allocation.
    pub fn vector_field(&self, table: usize, slot: u16) -> Result<Option<(usize, usize)>> {
        let Some(p) = self.field_pos(table, slot)? else {
            return Ok(None);
        };
        let start = self.indirect(p)?;
        let count = self.u32(start)? as usize;
        if count > self.0.len() {
            return ipc("flatbuffer vector length out of bounds");
        }
        Ok(Some((start + 4, count)))
    }

    /// Table position of element `i` in a vector of tables.
    pub fn vector_table(&self, first: usize, i: usize) -> Result<usize> {
        self.indirect(first + 4 * i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_round_trip() {
        let mut b = Builder::new();
        let t = b.table(&[
            (0, Val::I16(4)),
            (1, Val::U8(7)),
            (2, Val::Ref),
            (3, Val::I64(-9)),
        ]);
        let s = b.string("hello");
        b.patch(t.slot(2), s);
        let bytes = b.finish(&t);

        let r = Reader(&bytes);
        let root = r.root().unwrap();
        assert_eq!(r.i16_field(root, 0, 0).unwrap(), 4);
        assert_eq!(r.u8_field(root, 1, 0).unwrap(), 7);
        assert_eq!(r.string_field(root, 2).unwrap(), Some("hello"));
        assert_eq!(r.i64_field(root, 3, 0).unwrap(), -9);
        assert_eq!(r.i64_field(root, 9, 5).unwrap(), 5);
        assert_eq!(bytes.len() % 8, 0);
    }

    #[test]
    fn truncated_input_is_an_error_not_a_panic() {
        let mut b = Builder::new();
        let t = b.table(&[(0, Val::I64(1))]);
        let bytes = b.finish(&t);
        for cut in 0..bytes.len() {
            let r = Reader(&bytes[..cut]);
            let _ = r.root().and_then(|root| r.i64_field(root, 0, 0));
        }
    }
}
