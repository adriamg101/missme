use std::collections::HashMap;

use crate::models::interpreter::riscv::codegen::ir::IrEntry;

/// `.text` is a known, contiguous, fully populated address range, so
/// instructions live in a dense `Vec` indexed directly by
/// `(address - text_base) / 4` — no hashing needed, nothing sparse to save.
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

/// `.data`/`.bss`, RAM, and disk are genuinely sparse — the user can
/// reserve far more than they touch — so this is a block-granular
/// `HashMap`; only blocks actually accessed exist.
pub struct DataStore {
    block_size: usize,
    blocks: HashMap<u64, Vec<u8>>,
}

impl DataStore {
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            blocks: HashMap::new(),
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

    pub fn read_byte(&self, address: u64) -> u8 {
        let block_addr = self.block_address(address);
        let offset = (address - block_addr) as usize;
        self.blocks
            .get(&block_addr)
            .map(|b| b[offset])
            .unwrap_or(0)
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
        // untouched neighbor stays zero
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
        store.write_byte(0x10000003, 0xAA); // last byte of block 0
        store.write_byte(0x10000004, 0xBB); // first byte of block 1
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
}
