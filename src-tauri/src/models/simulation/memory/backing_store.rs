use std::collections::HashMap;

use super::seeded_random;
use crate::models::interpreter::riscv::codegen::ir::IrEntry;

/// The instruction section is a known, unbroken, fully filled range, so instructions live in a plain list by position.
/// No lookup table is needed since there is nothing sparse to save space on.
pub struct InstructionStore {
    text_base: u64,
    entries: Vec<IrEntry>,
}

impl InstructionStore {
    pub fn new(text_base: u64, entries: Vec<IrEntry>) -> Self {
        Self { text_base, entries }
    }

    pub fn fetch(&self, address: u64) -> Option<&IrEntry> {
        if address < self.text_base {
            return None;
        }
        let offset = address - self.text_base;
        if offset % 4 != 0 {
            return None;
        }
        self.entries.get((offset / 4) as usize)
    }
}

/// The data section, RAM, and disk are genuinely sparse since the user can reserve far more space than they use.
/// This stores data in blocks, and only blocks that were actually touched exist in the table.
pub struct DataStore {
    block_size: usize,
    blocks: HashMap<u64, Vec<u8>>,
    dirty_percent: u8,
    seed: u64,
}

impl DataStore {
    pub fn new(block_size: usize) -> Self {
        Self::with_dirty_percent(block_size, 0)
    }

    /// Some blocks that are never explicitly written still read as pre-existing garbage, the same way real memory behaves.
    /// This is worked out fresh each time from a seed, and the user never picks one directly, a random one is drawn per session.
    pub fn with_dirty_percent(block_size: usize, dirty_percent: u8) -> Self {
        Self::with_dirty_percent_seeded(block_size, dirty_percent, seeded_random::generate_seed())
    }

    /// Same as with_dirty_percent, but the seed is given directly for tests that need a specific garbage pattern.
    pub fn with_dirty_percent_seeded(block_size: usize, dirty_percent: u8, seed: u64) -> Self {
        Self {
            block_size,
            blocks: HashMap::new(),
            dirty_percent,
            seed,
        }
    }

    pub fn block_address(&self, address: u64) -> u64 {
        address - (address % self.block_size as u64)
    }

    pub fn preload(&mut self, base_address: u64, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.write_byte(base_address + i as u64, byte);
        }
    }

    /// A read that does not count as a real access, so the inspection views can look at a cell without revealing untouched garbage.
    /// Use access_byte instead for a real simulated Load or Fetch.
    pub fn read_byte(&self, address: u64) -> u8 {
        let block_addr = self.block_address(address);
        let offset = (address - block_addr) as usize;
        self.blocks.get(&block_addr).map(|b| b[offset]).unwrap_or(0)
    }

    /// A real access. If this block was still untouched garbage, its content gets generated and saved into the store for good.
    /// From then on it reads and displays like any other filled block.
    pub fn access_byte(&mut self, address: u64) -> u8 {
        let block_addr = self.block_address(address);
        if !self.blocks.contains_key(&block_addr) && self.is_dirty_seeded(block_addr) {
            self.materialize_dirty_block(block_addr);
        }
        let offset = (address - block_addr) as usize;
        self.blocks.get(&block_addr).map(|b| b[offset]).unwrap_or(0)
    }

    fn materialize_dirty_block(&mut self, block_addr: u64) {
        let seed = self.seed;
        let bytes: Vec<u8> = (0..self.block_size as u64).map(|i| seeded_random::byte(seed, &[2, block_addr + i])).collect();
        self.blocks.insert(block_addr, bytes);
    }

    fn is_dirty_seeded(&self, block_address: u64) -> bool {
        self.dirty_percent > 0 && seeded_random::percent_roll(self.seed, &[1, block_address]) < self.dirty_percent
    }

    pub fn write_byte(&mut self, address: u64, value: u8) {
        let block_addr = self.block_address(address);
        let offset = (address - block_addr) as usize;
        let block_size = self.block_size;
        let block = self
            .blocks
            .entry(block_addr)
            .or_insert_with(|| vec![0u8; block_size]);
        block[offset] = value;
    }

    pub fn read_block(&self, address: u64, len: usize) -> Vec<u8> {
        (0..len as u64).map(|i| self.read_byte(address + i)).collect()
    }

    pub fn is_populated(&self, block_address: u64) -> bool {
        self.blocks.contains_key(&block_address)
    }

    /// True for a block that was never written but still reads as garbage because it rolled under the dirty percent.
    /// The display uses this to tell a truly empty cell apart from one that should show as dirty.
    pub fn is_dirty(&self, block_address: u64) -> bool {
        !self.is_populated(block_address) && self.is_dirty_seeded(block_address)
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::InstructionKind;
    use crate::models::interpreter::riscv::parser::ast::Operand;
    use crate::models::interpreter::riscv::lexer::token::RegisterKind;

    fn entry(address: u32) -> IrEntry {
        IrEntry {
            address,
            kind: InstructionKind::Add,
            operands: vec![
                Operand::Register(RegisterKind::X(1)),
                Operand::Register(RegisterKind::X(2)),
                Operand::Register(RegisterKind::X(3)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn fetches_the_instruction_at_the_given_address() {
        let store = InstructionStore::new(0x1000, vec![entry(0x1000), entry(0x1004)]);
        assert_eq!(store.fetch(0x1000).unwrap().address, 0x1000);
        assert_eq!(store.fetch(0x1004).unwrap().address, 0x1004);
    }

    #[test]
    fn fetch_out_of_range_returns_none() {
        let store = InstructionStore::new(0x1000, vec![entry(0x1000)]);
        assert!(store.fetch(0x1008).is_none());
        assert!(store.fetch(0x0FFC).is_none());
    }

    #[test]
    fn fetch_on_a_misaligned_address_returns_none() {
        let store = InstructionStore::new(0x1000, vec![entry(0x1000)]);
        assert!(store.fetch(0x1001).is_none());
    }

    #[test]
    fn data_store_read_before_any_write_returns_zero() {
        let store = DataStore::new(64);
        assert_eq!(store.read_byte(0x10000000), 0);
    }

    #[test]
    fn data_store_round_trips_a_written_byte() {
        let mut store = DataStore::new(64);
        store.write_byte(0x10000005, 0xAB);
        assert_eq!(store.read_byte(0x10000005), 0xAB);
        // the untouched byte next to it stays zero
        assert_eq!(store.read_byte(0x10000006), 0);
    }

    #[test]
    fn data_store_preload_writes_a_contiguous_byte_range() {
        let mut store = DataStore::new(64);
        store.preload(0x10000000, &[1, 2, 3]);
        assert_eq!(store.read_byte(0x10000000), 1);
        assert_eq!(store.read_byte(0x10000001), 2);
        assert_eq!(store.read_byte(0x10000002), 3);
    }

    #[test]
    fn data_store_writes_spanning_two_blocks_land_in_both() {
        let mut store = DataStore::new(4);
        store.write_byte(0x10000003, 0xAA); // the last byte of block 0
        store.write_byte(0x10000004, 0xBB); // the first byte of block 1
        assert_eq!(store.read_byte(0x10000003), 0xAA);
        assert_eq!(store.read_byte(0x10000004), 0xBB);
    }

    #[test]
    fn block_address_rounds_down_to_the_block_boundary() {
        let store = DataStore::new(64);
        assert_eq!(store.block_address(0x100), 0x100);
        assert_eq!(store.block_address(0x105), 0x100);
        assert_eq!(store.block_address(0x13F), 0x100);
        assert_eq!(store.block_address(0x140), 0x140);
    }

    #[test]
    fn read_block_returns_len_bytes_including_unwritten_zeros() {
        let mut store = DataStore::new(8);
        store.write_byte(0x100, 0xAA);
        store.write_byte(0x102, 0xBB);
        let block = store.read_block(0x100, 4);
        assert_eq!(block, vec![0xAA, 0x00, 0xBB, 0x00]);
    }

    #[test]
    fn is_populated_reflects_whether_the_block_has_been_written() {
        let mut store = DataStore::new(8);
        assert!(!store.is_populated(0x100));
        store.write_byte(0x100, 1);
        assert!(store.is_populated(0x100));
        assert!(!store.is_populated(0x108)); // a different block that's still untouched
    }

    #[test]
    fn block_size_returns_the_configured_value() {
        let store = DataStore::new(64);
        assert_eq!(store.block_size(), 64);
    }

    #[test]
    fn zero_dirty_percent_never_seeds_garbage() {
        let store = DataStore::with_dirty_percent(64, 0);
        assert_eq!(store.read_byte(0x10000000), 0);
        assert!(!store.is_dirty(0x10000000));
    }

    #[test]
    fn a_passive_read_of_a_dirty_seeded_block_never_reveals_garbage() {
        // The inspection views read without counting as a real access.
        // An untouched cell must never leak its garbage value, or become real data just from being looked at.
        let store = DataStore::with_dirty_percent(64, 100);
        assert!(store.is_dirty(0x10000000));
        assert_eq!(store.read_byte(0x10000005), 0);
        assert!(!store.is_populated(0x10000000));
        assert!(store.is_dirty(0x10000000)); // still hidden, the read didn't change anything
    }

    #[test]
    fn a_real_access_materializes_a_dirty_seeded_block_reproducibly() {
        let mut store = DataStore::with_dirty_percent(64, 100);
        assert!(store.is_dirty(0x10000000));
        let first = store.access_byte(0x10000005);
        // Now really filled in, from here on it reads like any other stored byte and is no longer reported as hidden.
        assert!(store.is_populated(0x10000000));
        assert!(!store.is_dirty(0x10000000));
        let second = store.access_byte(0x10000005);
        assert_eq!(first, second);
        assert_eq!(store.read_byte(0x10000005), first);
    }

    #[test]
    fn writing_into_a_dirty_seeded_block_makes_it_populated_not_dirty() {
        let mut store = DataStore::with_dirty_percent(64, 100);
        assert!(store.is_dirty(0x10000000));
        store.write_byte(0x10000000, 0xAB);
        assert!(store.is_populated(0x10000000));
        assert!(!store.is_dirty(0x10000000));
        assert_eq!(store.read_byte(0x10000000), 0xAB);
    }

    #[test]
    fn dirty_percent_zero_is_the_default_via_new() {
        let store = DataStore::new(64);
        assert!(!store.is_dirty(0x10000000));
    }
}
