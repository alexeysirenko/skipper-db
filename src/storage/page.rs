use crate::storage::DEFAULT_PAGE_SIZE;

pub const PAGE_SIZE: usize = DEFAULT_PAGE_SIZE as usize;

pub type SlotId = u16;

const HEADER_SIZE: usize = 4;
const SLOT_SIZE: usize = 4;

// A fixed-size page holding variable-length records. The slot directory grows
// from the front; record cells are packed from the back. The gap between them
// is free space.
pub struct SlottedPage {
    bytes: [u8; PAGE_SIZE],
}

impl SlottedPage {
    // A fresh, empty page: zero slots, free-space pointer at the end.
    pub fn new() -> Self {
        let mut page = SlottedPage {
            bytes: [0u8; PAGE_SIZE],
        };
        page.set_slot_count(0);
        page.set_free_ptr(PAGE_SIZE as u16);
        page
    }

    // Wrap a page-sized buffer read from disk.
    pub fn from_bytes(bytes: [u8; PAGE_SIZE]) -> Self {
        SlottedPage { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; PAGE_SIZE] {
        &self.bytes
    }

    // Number of slots in the directory (including any tombstoned ones).
    pub fn slot_count(&self) -> usize {
        self.read_u16(0) as usize
    }

    // Contiguous free bytes between the slot directory and the cell region.
    pub fn free_space(&self) -> usize {
        self.free_ptr() - self.slot_dir_end()
    }

    // Copy `record` into a cell, append a slot, return its id — or None if it
    // does not fit.
    pub fn insert(&mut self, record: &[u8]) -> Option<SlotId> {
        if record.len() + SLOT_SIZE > self.free_space() {
            return None;
        }

        let offset = self.free_ptr() - record.len();
        self.bytes[offset..offset + record.len()].copy_from_slice(record);

        let slot = self.slot_count();
        self.set_slot(slot, offset as u16, record.len() as u16);
        self.set_free_ptr(offset as u16);
        self.set_slot_count((slot + 1) as u16);

        Some(slot as SlotId)
    }

    // The record bytes for `slot`, or None if the slot is out of range or dead.
    pub fn get(&self, slot: SlotId) -> Option<&[u8]> {
        let slot = slot as usize;
        if slot >= self.slot_count() {
            return None;
        }
        let (offset, len) = self.slot(slot);
        if len == 0 {
            return None;
        }
        Some(&self.bytes[offset..offset + len])
    }

    fn slot_dir_end(&self) -> usize {
        HEADER_SIZE + self.slot_count() * SLOT_SIZE
    }

    fn free_ptr(&self) -> usize {
        self.read_u16(2) as usize
    }

    fn set_slot_count(&mut self, count: u16) {
        self.write_u16(0, count);
    }

    fn set_free_ptr(&mut self, ptr: u16) {
        self.write_u16(2, ptr);
    }

    fn slot(&self, index: usize) -> (usize, usize) {
        let base = HEADER_SIZE + index * SLOT_SIZE;
        (self.read_u16(base) as usize, self.read_u16(base + 2) as usize)
    }

    fn set_slot(&mut self, index: usize, offset: u16, len: u16) {
        let base = HEADER_SIZE + index * SLOT_SIZE;
        self.write_u16(base, offset);
        self.write_u16(base + 2, len);
    }

    fn read_u16(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    fn write_u16(&mut self, at: usize, value: u16) {
        self.bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
}

impl Default for SlottedPage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_get() {
        let mut page = SlottedPage::new();
        let a = page.insert(b"hello").unwrap();
        let b = page.insert(b"world!!").unwrap();

        assert_eq!(page.get(a), Some(&b"hello"[..]));
        assert_eq!(page.get(b), Some(&b"world!!"[..]));
        assert_eq!(page.slot_count(), 2);
    }

    #[test]
    fn get_out_of_range_is_none() {
        let page = SlottedPage::new();
        assert_eq!(page.get(0), None);
    }

    #[test]
    fn survives_bytes_roundtrip() {
        let mut page = SlottedPage::new();
        let slot = page.insert(b"persist me").unwrap();

        let reopened = SlottedPage::from_bytes(*page.as_bytes());
        assert_eq!(reopened.get(slot), Some(&b"persist me"[..]));
    }

    #[test]
    fn insert_returns_none_when_full() {
        let mut page = SlottedPage::new();
        let too_big = vec![0u8; PAGE_SIZE];
        assert_eq!(page.insert(&too_big), None);
    }
}
