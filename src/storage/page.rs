use crate::storage::DEFAULT_PAGE_SIZE;

pub const PAGE_SIZE: usize = DEFAULT_PAGE_SIZE as usize;

pub type SlotId = u16;

// A fixed-size page holding variable-length records. The slot directory grows
// from the front; record cells are packed from the back. The gap between them
// is free space.
pub struct SlottedPage {
    bytes: [u8; PAGE_SIZE],
}

impl SlottedPage {
    // A fresh, empty page: zero slots, free-space pointer at the end.
    pub fn new() -> Self {
        todo!()
    }

    // Wrap a page-sized buffer read from disk.
    pub fn from_bytes(bytes: [u8; PAGE_SIZE]) -> Self {
        todo!()
    }

    pub fn as_bytes(&self) -> &[u8; PAGE_SIZE] {
        &self.bytes
    }

    // Number of slots in the directory (including any tombstoned ones).
    pub fn slot_count(&self) -> usize {
        todo!()
    }

    // Bytes available for one more record plus its slot entry.
    pub fn free_space(&self) -> usize {
        todo!()
    }

    // Copy `record` into a cell, append a slot, return its id — or None if it
    // does not fit.
    pub fn insert(&mut self, record: &[u8]) -> Option<SlotId> {
        todo!()
    }

    // The record bytes for `slot`, or None if the slot is out of range or dead.
    pub fn get(&self, slot: SlotId) -> Option<&[u8]> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_get() {
        // Insert a couple of byte records into a new page and read them back by
        // slot id.
        todo!()
    }
}
