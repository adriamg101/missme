use crate::models::interpreter::riscv::codegen::ir::{CodegenOutput, IrEntry};
use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
use crate::models::interpreter::riscv::parser::ast::Operand;
use crate::models::interpreter::riscv::Base;

use super::memory::backing_store::InstructionStore;
use super::memory::hierarchy::MemoryHierarchySimulator;
use super::{Action, ActionKind, CpuSimulator, RegisterId};

struct PendingMemoryOp {
    source_line: usize,
    register: RegisterId,
    #[allow(dead_code)] // constructed for symmetry with the access request; not yet consulted after completion, kept for a near-future consumer (e.g. event payloads)
    size: u8,
    sign_extend: bool,
}

pub struct RiscvCpuSimulator {
    base: Base,
    registers: [u64; 32],
    pc: u64,
    instructions: InstructionStore,
    memory: MemoryHierarchySimulator,
    pending: Option<PendingMemoryOp>,
}

impl RiscvCpuSimulator {
    pub fn new(base: Base, codegen: CodegenOutput, mut memory: MemoryHierarchySimulator) -> Self {
        let text_base = codegen.layout.text_base as u64;
        let data_base = codegen.layout.data_base as u64;
        memory.preload_ram(data_base, &codegen.layout.initial_data);
        Self {
            base,
            registers: [0; 32],
            pc: text_base,
            instructions: InstructionStore::new(text_base, codegen.entries),
            memory,
            pending: None,
        }
    }

    /// Masks a value to this CPU's configured word width — 32 bits for
    /// Rv32i, the full 64 bits for Rv64i.
    fn mask(&self, value: u64) -> u64 {
        match self.base {
            Base::Rv32i => value & 0xFFFF_FFFF,
            Base::Rv64i => value,
        }
    }

    /// Sign-extends a u64 value to i64 based on this CPU's configured word width.
    /// For Rv32i, treats the lower 32 bits as a signed 32-bit value and extends the sign.
    /// For Rv64i, treats the full 64 bits as a signed 64-bit value.
    fn sign_extend(&self, value: u64) -> i64 {
        match self.base {
            Base::Rv32i => ((value as u32) as i32) as i64,
            Base::Rv64i => value as i64,
        }
    }

    fn read_register(&self, id: RegisterId) -> u64 {
        if id.0 == 0 { 0 } else { self.registers[id.0 as usize] }
    }

    fn write_register(&mut self, id: RegisterId, value: u64) {
        if id.0 != 0 {
            self.registers[id.0 as usize] = self.mask(value);
        }
    }

    fn register_id(operand: &Operand) -> RegisterId {
        match operand {
            Operand::Register(RegisterKind::X(n)) => RegisterId(*n),
            _ => unreachable!("operand shape already validated by semantic analysis"),
        }
    }

    fn bytes_to_value(bytes: &[u8], sign_extend: bool) -> u64 {
        let mut value: u64 = 0;
        for (i, &b) in bytes.iter().enumerate() {
            value |= (b as u64) << (i * 8);
        }
        if sign_extend {
            let bit_width = bytes.len() * 8;
            let sign_bit = 1u64 << (bit_width - 1);
            // A full 64-bit read already fills the type, so it needs no
            // extension — and `!0u64 << 64` would panic (shift amount
            // must be < 64) in a debug build. Unreachable with today's
            // callers (no instruction loads 8 sign-extended bytes), but
            // guarded defensively since this is easy to get wrong later.
            if bit_width < 64 && value & sign_bit != 0 {
                value |= !0u64 << bit_width;
            }
        }
        value
    }

    fn execute_current_instruction(&mut self) -> Action {
        let entry: IrEntry = self
            .instructions
            .fetch(self.pc)
            .cloned()
            .expect("execute_current_instruction called while finished");

        match entry.kind {
            InstructionKind::Add => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let rs2 = Self::register_id(&entry.operands[2]);
                let value = self.mask(self.read_register(rs1).wrapping_add(self.read_register(rs2)));
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Lui => {
                let rd = Self::register_id(&entry.operands[0]);
                let imm = match &entry.operands[1] {
                    Operand::Immediate(v) => *v,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                // `imm` arrives as an unsigned 20-bit value (validated
                // upstream). Shifting it into bit 31 can set that bit,
                // which under RV64I must be sign-extended through the
                // full 64-bit result — hence the signed round-trip below.
                let shifted = ((imm as u32) << 12) as i32 as i64 as u64;
                let value = self.mask(shifted);
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Auipc => {
                let rd = Self::register_id(&entry.operands[0]);
                let imm = match &entry.operands[1] {
                    Operand::Immediate(v) => *v,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                // Same signed-shift reasoning as `Lui`, then add the pc.
                let shifted = ((imm as u32) << 12) as i32 as i64 as u64;
                let value = self.mask((entry.address as u64).wrapping_add(shifted));
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Addi | InstructionKind::Slti | InstructionKind::Sltiu | InstructionKind::Xori | InstructionKind::Ori | InstructionKind::Andi => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let imm = match &entry.operands[2] {
                    Operand::Immediate(v) => *v,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let a = self.read_register(rs1);
                let value = self.mask(match entry.kind {
                    InstructionKind::Addi => a.wrapping_add(imm as u64),
                    InstructionKind::Slti => {
                        (self.sign_extend(a) < imm) as u64
                    }
                    InstructionKind::Sltiu => (a < self.mask(imm as u64)) as u64,
                    InstructionKind::Xori => a ^ (imm as u64),
                    InstructionKind::Ori => a | (imm as u64),
                    InstructionKind::Andi => a & (imm as u64),
                    _ => unreachable!(),
                });
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Slli | InstructionKind::Srli | InstructionKind::Srai => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let shamt_mask: u64 = if self.base == Base::Rv32i { 0x1F } else { 0x3F };
                let shamt = match &entry.operands[2] {
                    Operand::Immediate(v) => (*v as u64) & shamt_mask,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let a = self.read_register(rs1);
                let value = self.mask(match entry.kind {
                    InstructionKind::Slli => a << shamt,
                    InstructionKind::Srli => a >> shamt,
                    InstructionKind::Srai => {
                        (self.sign_extend(a) >> shamt) as u64
                    }
                    _ => unreachable!(),
                });
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Sub | InstructionKind::Sll | InstructionKind::Slt | InstructionKind::Sltu | InstructionKind::Xor | InstructionKind::Srl | InstructionKind::Sra | InstructionKind::Or | InstructionKind::And => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let rs2 = Self::register_id(&entry.operands[2]);
                let a = self.read_register(rs1);
                let b = self.read_register(rs2);
                let shamt_mask: u64 = if self.base == Base::Rv32i { 0x1F } else { 0x3F };
                let value = self.mask(match entry.kind {
                    InstructionKind::Sub => a.wrapping_sub(b),
                    InstructionKind::Sll => a << (b & shamt_mask),
                    InstructionKind::Slt => {
                        (self.sign_extend(a) < self.sign_extend(b)) as u64
                    }
                    InstructionKind::Sltu => (a < b) as u64,
                    InstructionKind::Xor => a ^ b,
                    InstructionKind::Srl => a >> (b & shamt_mask),
                    InstructionKind::Sra => {
                        (self.sign_extend(a) >> (b & shamt_mask)) as u64
                    }
                    InstructionKind::Or => a | b,
                    InstructionKind::And => a & b,
                    _ => unreachable!(),
                });
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Jal => {
                let rd = Self::register_id(&entry.operands[0]);
                let imm = match &entry.operands[1] {
                    Operand::Immediate(v) => *v,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let return_address = entry.address as u64 + 4;
                self.write_register(rd, return_address);
                self.pc = (entry.address as i64 + imm) as u64;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Jalr => {
                let rd = Self::register_id(&entry.operands[0]);
                let (imm, base) = match &entry.operands[1] {
                    Operand::Offset { imm, base } => (*imm, Self::register_id(&Operand::Register(*base))),
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let target = (self.read_register(base) as i64 + imm) as u64 & !1;
                let return_address = entry.address as u64 + 4;
                self.write_register(rd, return_address);
                self.pc = target;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Beq | InstructionKind::Bne | InstructionKind::Blt | InstructionKind::Bge | InstructionKind::Bltu | InstructionKind::Bgeu => {
                let rs1 = Self::register_id(&entry.operands[0]);
                let rs2 = Self::register_id(&entry.operands[1]);
                let imm = match &entry.operands[2] {
                    Operand::Immediate(v) => *v,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let a = self.read_register(rs1);
                let b = self.read_register(rs2);
                let taken = match entry.kind {
                    InstructionKind::Beq => a == b,
                    InstructionKind::Bne => a != b,
                    InstructionKind::Blt => self.sign_extend(a) < self.sign_extend(b),
                    InstructionKind::Bge => self.sign_extend(a) >= self.sign_extend(b),
                    InstructionKind::Bltu => a < b,
                    InstructionKind::Bgeu => a >= b,
                    _ => unreachable!(),
                };
                self.pc = if taken { (entry.address as i64 + imm) as u64 } else { entry.address as u64 + 4 };
                Action {
                    kind: ActionKind::NoOp,
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Lb | InstructionKind::Lh | InstructionKind::Lw | InstructionKind::Lbu | InstructionKind::Lhu | InstructionKind::Lwu | InstructionKind::Ld => {
                let rd = Self::register_id(&entry.operands[0]);
                let (imm, base) = match &entry.operands[1] {
                    Operand::Offset { imm, base } => (*imm, Self::register_id(&Operand::Register(*base))),
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let address = (self.read_register(base) as i64 + imm) as u64;
                let (size, sign_extend) = match entry.kind {
                    InstructionKind::Lb => (1, true),
                    InstructionKind::Lh => (2, true),
                    InstructionKind::Lw => (4, true),
                    InstructionKind::Lbu => (1, false),
                    InstructionKind::Lhu => (2, false),
                    InstructionKind::Lwu => (4, false),
                    InstructionKind::Ld => (8, false),
                    _ => unreachable!(),
                };
                let access = super::memory::hierarchy::MemoryAccess {
                    kind: super::memory::hierarchy::AccessKind::Load,
                    address,
                    size,
                    store_bytes: None,
                };
                let level_result = self.memory.begin_access(access);
                self.pending = Some(PendingMemoryOp { source_line: entry.source_line, register: rd, size, sign_extend });
                return Action { kind: ActionKind::MemoryLevel(level_result), source_line: entry.source_line, instruction_complete: false };
            }
            InstructionKind::Sb | InstructionKind::Sh | InstructionKind::Sw | InstructionKind::Sd => {
                let rs2 = Self::register_id(&entry.operands[0]);
                let (imm, base) = match &entry.operands[1] {
                    Operand::Offset { imm, base } => (*imm, Self::register_id(&Operand::Register(*base))),
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let address = (self.read_register(base) as i64 + imm) as u64;
                let size: u8 = match entry.kind {
                    InstructionKind::Sb => 1,
                    InstructionKind::Sh => 2,
                    InstructionKind::Sw => 4,
                    InstructionKind::Sd => 8,
                    _ => unreachable!(),
                };
                let value = self.read_register(rs2);
                let bytes: Vec<u8> = (0..size).map(|i| ((value >> (i * 8)) & 0xFF) as u8).collect();
                let access = super::memory::hierarchy::MemoryAccess {
                    kind: super::memory::hierarchy::AccessKind::Store,
                    address,
                    size,
                    store_bytes: Some(bytes),
                };
                let level_result = self.memory.begin_access(access);
                self.pending = Some(PendingMemoryOp { source_line: entry.source_line, register: RegisterId(0), size, sign_extend: false });
                return Action { kind: ActionKind::MemoryLevel(level_result), source_line: entry.source_line, instruction_complete: false };
            }
            InstructionKind::Addiw => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let imm = match &entry.operands[2] {
                    Operand::Immediate(v) => *v,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let result32 = (self.read_register(rs1) as i32).wrapping_add(imm as i32);
                let value = self.mask(result32 as i64 as u64);
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Slliw | InstructionKind::Srliw | InstructionKind::Sraiw => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let shamt = match &entry.operands[2] {
                    Operand::Immediate(v) => (*v as u32) & 0x1F,
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let a32 = self.read_register(rs1) as u32;
                let result32 = match entry.kind {
                    InstructionKind::Slliw => a32 << shamt,
                    InstructionKind::Srliw => a32 >> shamt,
                    InstructionKind::Sraiw => ((a32 as i32) >> shamt) as u32,
                    _ => unreachable!(),
                };
                let value = self.mask(result32 as i32 as i64 as u64);
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Addw | InstructionKind::Subw | InstructionKind::Sllw | InstructionKind::Srlw | InstructionKind::Sraw => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let rs2 = Self::register_id(&entry.operands[2]);
                let a32 = self.read_register(rs1) as u32;
                let b32 = self.read_register(rs2) as u32;
                let result32 = match entry.kind {
                    InstructionKind::Addw => a32.wrapping_add(b32),
                    InstructionKind::Subw => a32.wrapping_sub(b32),
                    InstructionKind::Sllw => a32 << (b32 & 0x1F),
                    InstructionKind::Srlw => a32 >> (b32 & 0x1F),
                    InstructionKind::Sraw => ((a32 as i32) >> (b32 & 0x1F)) as u32,
                    _ => unreachable!(),
                };
                let value = self.mask(result32 as i32 as i64 as u64);
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Mul | InstructionKind::Mulh | InstructionKind::Mulhsu | InstructionKind::Mulhu | InstructionKind::Div | InstructionKind::Divu | InstructionKind::Rem | InstructionKind::Remu => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let rs2 = Self::register_id(&entry.operands[2]);
                let a = self.read_register(rs1);
                let b = self.read_register(rs2);
                let word_bits: u32 = if self.base == Base::Rv32i { 32 } else { 64 };
                let a_signed = self.sign_extend(a);
                let b_signed = self.sign_extend(b);

                let value = match entry.kind {
                    InstructionKind::Mul => a.wrapping_mul(b),
                    InstructionKind::Mulh => (((a_signed as i128) * (b_signed as i128)) >> word_bits) as u64,
                    InstructionKind::Mulhsu => (((a_signed as i128) * (b as u128) as i128) >> word_bits) as u64,
                    InstructionKind::Mulhu => (((a as u128) * (b as u128)) >> word_bits) as u64,
                    InstructionKind::Div => {
                        if b_signed == 0 {
                            u64::MAX
                        } else if a_signed == i64::MIN && b_signed == -1 {
                            a
                        } else {
                            a_signed.wrapping_div(b_signed) as u64
                        }
                    }
                    InstructionKind::Divu => {
                        if b == 0 { u64::MAX } else { a.wrapping_div(b) }
                    }
                    InstructionKind::Rem => {
                        if b_signed == 0 {
                            a
                        } else if a_signed == i64::MIN && b_signed == -1 {
                            0
                        } else {
                            a_signed.wrapping_rem(b_signed) as u64
                        }
                    }
                    InstructionKind::Remu => {
                        if b == 0 { a } else { a.wrapping_rem(b) }
                    }
                    _ => unreachable!(),
                };
                self.write_register(rd, self.mask(value));
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::Fence | InstructionKind::Ecall | InstructionKind::Ebreak => {
                self.pc += 4;
                Action {
                    kind: ActionKind::NoOp,
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            other => panic!("RiscvCpuSimulator does not yet implement {:?} (base/M ISA coverage lands across Tasks 6-11 of TT4.2)", other),
        }
    }
}

impl CpuSimulator for RiscvCpuSimulator {
    fn step_action(&mut self) -> Action {
        assert!(!self.is_finished(), "step_action called after the program finished");

        if let Some(pending) = &self.pending {
            let source_line = pending.source_line;
            // The access may already have finished during `begin_access`
            // itself (e.g. a hierarchy with no cache levels resolves
            // straight to RAM in one call) — in that case there is no
            // further `LevelResult` to report and we go straight to the
            // completion action below.
            if self.memory.is_access_finished() {
                let register = pending.register;
                let sign_extend = pending.sign_extend;
                self.pending = None;
                let result = self.memory.take_result();
                self.pc += 4;
                if let Some(bytes) = result.load_bytes {
                    let value = self.mask(Self::bytes_to_value(&bytes, sign_extend));
                    self.write_register(register, value);
                    return Action {
                        kind: ActionKind::RegisterWrite { register, value: self.read_register(register) },
                        source_line,
                        instruction_complete: true,
                    };
                }
                return Action { kind: ActionKind::NoOp, source_line, instruction_complete: true };
            }
            // Otherwise the access is still in progress: take one step and
            // report its own `LevelResult` on THIS call, even if this very
            // step is the one that finishes the access. The completion
            // action (register write / NoOp / pc advance) is deliberately
            // deferred to the NEXT `step_action()` call, which will see
            // `is_access_finished()` already true and take the branch
            // above — keeping "the level that resolved the access" and
            // "the completion action" as two separate actions, exactly
            // like a step that finishes on a later call already does.
            let level_result = self.memory.step();
            return Action { kind: ActionKind::MemoryLevel(level_result), source_line, instruction_complete: false };
        }

        self.execute_current_instruction()
    }

    fn is_finished(&self) -> bool {
        self.instructions.fetch(self.pc).is_none()
    }

    fn pc(&self) -> u64 {
        self.pc
    }

    fn register(&self, id: RegisterId) -> u64 {
        self.read_register(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::codegen::ir::{CodegenOutput, MemoryLayout};
    use crate::models::interpreter::riscv::lexer::token::RegisterKind;
    use crate::models::interpreter::riscv::parser::ast::Operand;
    use crate::models::simulation::memory::backing_store::DataStore;
    use crate::models::simulation::memory::config::{
        CacheConfig, CacheEntry, DiskConfig, HierarchyConfig, InclusionPolicy, Level, RamConfig, ReplacementPolicy, TlbConfig,
        TlbReplacementPolicy, WritePolicy,
    };
    use crate::models::simulation::memory::hierarchy::{LevelId, LevelOutcome, LevelResult, MemoryHierarchySimulator};
    use std::collections::HashMap;

    fn no_cache_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            hierarchy: vec![],
            tlb: TlbConfig {
                page_size: 4096,
                page_table_levels: 1,
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
            disk: DiskConfig { latency: 10000 },
        }
    }

    fn mem() -> MemoryHierarchySimulator {
        MemoryHierarchySimulator::new(no_cache_hierarchy_config(), DataStore::new(64), DataStore::new(64))
    }

    /// A hierarchy with a single L1 cache level between the CPU and RAM —
    /// unlike `no_cache_hierarchy_config`, a fresh (cold) access here
    /// misses at the cache and needs a further `step()` to reach RAM, so
    /// `step_action` has to take more than one call to resolve it.
    fn one_cache_level_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            hierarchy: vec![Level {
                id: "l1".into(),
                harvard: false,
                unified: Some(CacheEntry {
                    exp: 1,
                    config: CacheConfig {
                        block_size: 4,
                        associativity: 1,
                        replacement: ReplacementPolicy::Lru,
                        write_policy: WritePolicy::WriteBack,
                        write_allocate: true,
                        hit_time: 1,
                        dirty_percent: 0,
                    },
                }),
                i: None,
                d: None,
                inclusion: InclusionPolicy::Nine,
                bus_width: 32,
            }],
            tlb: TlbConfig {
                page_size: 4096,
                page_table_levels: 1,
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
            disk: DiskConfig { latency: 10000 },
        }
    }

    fn mem_with_one_cache_level() -> MemoryHierarchySimulator {
        MemoryHierarchySimulator::new(one_cache_level_hierarchy_config(), DataStore::new(4), DataStore::new(4))
    }

    fn add_entry(address: u32, rd: u8, rs1: u8, rs2: u8) -> IrEntry {
        IrEntry {
            address,
            kind: InstructionKind::Add,
            operands: vec![
                Operand::Register(RegisterKind::X(rd)),
                Operand::Register(RegisterKind::X(rs1)),
                Operand::Register(RegisterKind::X(rs2)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    fn codegen(entries: Vec<IrEntry>) -> CodegenOutput {
        CodegenOutput {
            layout: MemoryLayout {
                text_base: 0x1000,
                data_base: 0x2000,
                labels: HashMap::new(),
                initial_data: vec![],
            },
            entries,
        }
    }

    fn i_type_entry(kind: InstructionKind, address: u32, rd: u8, rs1: u8, imm: i64) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::X(rd)),
                Operand::Register(RegisterKind::X(rs1)),
                Operand::Immediate(imm),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    fn r_type_entry(kind: InstructionKind, address: u32, rd: u8, rs1: u8, rs2: u8) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::X(rd)),
                Operand::Register(RegisterKind::X(rs1)),
                Operand::Register(RegisterKind::X(rs2)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn starts_at_the_program_s_text_base() {
        let sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![]), mem());
        assert_eq!(sim.pc(), 0x1000);
    }

    #[test]
    fn is_finished_when_there_is_no_instruction_at_pc() {
        let sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![]), mem());
        assert!(sim.is_finished());
    }

    #[test]
    fn is_not_finished_while_instructions_remain() {
        let sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![add_entry(0x1000, 1, 2, 3)]), mem());
        assert!(!sim.is_finished());
    }

    #[test]
    fn executing_add_writes_the_destination_register_and_advances_pc() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![add_entry(0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 10;
        sim.registers[3] = 32;

        let action = sim.step_action();

        assert_eq!(action.kind, ActionKind::RegisterWrite { register: RegisterId(1), value: 42 });
        assert_eq!(action.source_line, 1);
        assert!(action.instruction_complete);
        assert_eq!(sim.register(RegisterId(1)), 42);
        assert_eq!(sim.pc(), 0x1004);
    }

    #[test]
    fn x0_always_reads_as_zero_even_after_being_written() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![add_entry(0x1000, 0, 1, 1)]), mem());
        sim.registers[1] = 5;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(0)), 0);
    }

    #[test]
    fn program_is_finished_after_its_only_instruction_executes() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![add_entry(0x1000, 1, 2, 3)]), mem());
        sim.step_action();
        assert!(sim.is_finished());
    }

    #[test]
    fn addi_adds_a_sign_extended_immediate() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Addi, 0x1000, 1, 2, -5)]), mem());
        sim.registers[2] = 10;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 5);
    }

    #[test]
    fn sub_subtracts_with_wraparound() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Sub, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0;
        sim.registers[3] = 1;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF);
    }

    #[test]
    fn slt_reports_signed_less_than() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Slt, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF; // -1 signed
        sim.registers[3] = 1;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 1);
    }

    #[test]
    fn sltu_reports_unsigned_less_than() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Sltu, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF;
        sim.registers[3] = 1;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    #[test]
    fn srai_preserves_sign() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Srai, 0x1000, 1, 2, 1)]), mem());
        sim.registers[2] = 0xFFFF_FFFE; // -2
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF); // -1
    }

    #[test]
    fn srli_is_logical() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Srli, 0x1000, 1, 2, 1)]), mem());
        sim.registers[2] = 0xFFFF_FFFE;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0x7FFF_FFFF);
    }

    #[test]
    fn lui_places_the_immediate_in_the_upper_bits() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::Lui,
            operands: vec![Operand::Register(RegisterKind::X(1)), Operand::Immediate(0x12345)],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0x1234_5000);
    }

    #[test]
    fn auipc_adds_the_instruction_s_own_address() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::Auipc,
            operands: vec![Operand::Register(RegisterKind::X(1)), Operand::Immediate(1)],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0x1000 + 0x1000);
    }

    fn branch_entry(kind: InstructionKind, address: u32, rs1: u8, rs2: u8, imm: i64) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::X(rs1)),
                Operand::Register(RegisterKind::X(rs2)),
                Operand::Immediate(imm),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn jal_writes_the_return_address_and_jumps() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::Jal,
            operands: vec![Operand::Register(RegisterKind::X(1)), Operand::Immediate(0x100)],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0x1004);
        assert_eq!(sim.pc(), 0x1100);
    }

    #[test]
    fn jalr_clears_the_low_bit_of_the_target() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::Jalr,
            operands: vec![
                Operand::Register(RegisterKind::X(1)),
                Operand::Offset { imm: 1, base: RegisterKind::X(2) },
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.registers[2] = 0x2000;
        sim.step_action();
        assert_eq!(sim.pc(), 0x2000); // 0x2000 + 1, low bit cleared
    }

    #[test]
    fn beq_branches_when_operands_are_equal() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Beq, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 5;
        sim.registers[2] = 5;
        sim.step_action();
        assert_eq!(sim.pc(), 0x1020);
    }

    #[test]
    fn beq_falls_through_when_operands_differ() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Beq, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 5;
        sim.registers[2] = 6;
        let action = sim.step_action();
        assert_eq!(action.kind, ActionKind::NoOp);
        assert_eq!(sim.pc(), 0x1004);
    }

    #[test]
    fn bltu_compares_unsigned() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Bltu, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 0xFFFF_FFFF; // huge unsigned, would be -1 signed
        sim.registers[2] = 1;
        sim.step_action();
        assert_eq!(sim.pc(), 0x1004); // not taken: 0xFFFFFFFF is NOT < 1 unsigned
    }

    #[test]
    fn blt_treats_register_as_signed() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Blt, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 0xFFFF_FFFF; // -1 signed
        sim.registers[2] = 1;
        sim.step_action();
        assert_eq!(sim.pc(), 0x1020); // taken: -1 < 1 signed
    }

    #[test]
    fn bge_treats_register_as_signed() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Bge, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 0xFFFF_FFFF; // -1 signed
        sim.registers[2] = 1;
        sim.step_action();
        assert_eq!(sim.pc(), 0x1004); // not taken: -1 >= 1 is false
    }

    fn load_entry(kind: InstructionKind, address: u32, rd: u8, base: u8, imm: i64) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::X(rd)),
                Operand::Offset { imm, base: RegisterKind::X(base) },
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    fn run_to_finished_action(sim: &mut RiscvCpuSimulator) -> Action {
        loop {
            let action = sim.step_action();
            if action.instruction_complete {
                return action;
            }
        }
    }

    #[test]
    fn lw_reads_a_previously_stored_word() {
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[0xEF, 0xBE, 0xAD, 0xDE]); // little-endian 0xDEADBEEF
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lw, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        // With a zero-level hierarchy, `begin_access` (inside the first
        // `step_action` call) resolves straight to RAM, so no `step()` is
        // ever taken — the second `step_action` call must find the access
        // already finished and complete right away, with no spurious
        // extra step introduced by the finding #1 fix.
        let first = sim.step_action();
        assert_eq!(first.kind, ActionKind::MemoryLevel(LevelResult { level: LevelId::Ram, outcome: LevelOutcome::Hit }));
        assert!(!first.instruction_complete);

        let second = sim.step_action();
        assert_eq!(second.kind, ActionKind::RegisterWrite { register: RegisterId(1), value: 0xDEAD_BEEF });
        assert!(second.instruction_complete);
        assert_eq!(sim.pc(), 0x1004);
    }

    #[test]
    fn lb_sign_extends_a_negative_byte() {
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[0xFF]);
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lb, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);

        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF);
    }

    #[test]
    fn lbu_zero_extends() {
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[0xFF]);
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lbu, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);

        assert_eq!(sim.register(RegisterId(1)), 0xFF);
    }

    #[test]
    fn load_produces_a_memory_level_action_before_the_final_register_write() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lw, 0x1000, 1, 2, 0)]), mem());
        sim.registers[2] = 0x10000000;

        let first = sim.step_action();
        assert!(matches!(first.kind, ActionKind::MemoryLevel(_)));
        assert!(!first.instruction_complete);
    }

    #[test]
    fn the_level_that_resolves_a_load_is_reported_before_the_completion_action() {
        // Regression test for finding #1: with an L1 cache level between
        // the CPU and RAM, a cold `lw` misses at L1 (reported by
        // `begin_access`) and then the `step()` that reaches RAM is the
        // one that actually finishes the access. That RAM `LevelResult`
        // must be surfaced as its own `MemoryLevel` action — not silently
        // folded into the completion `RegisterWrite` on the same call.
        let mut mem = mem_with_one_cache_level();
        mem.preload_ram(0x10000000, &[0xEF, 0xBE, 0xAD, 0xDE]); // little-endian 0xDEADBEEF
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lw, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        // Call 1: begin_access misses at L1.
        let first = sim.step_action();
        assert_eq!(first.kind, ActionKind::MemoryLevel(LevelResult { level: LevelId::Cache(0), outcome: LevelOutcome::Miss }));
        assert!(!first.instruction_complete);
        assert_eq!(sim.pc(), 0x1000, "pc must not advance until the instruction actually completes");

        // Call 2: step() reaches RAM and finishes the access. This is the
        // level that actually resolved the load — it must be reported
        // here, as its own action, not dropped.
        let second = sim.step_action();
        assert_eq!(second.kind, ActionKind::MemoryLevel(LevelResult { level: LevelId::Ram, outcome: LevelOutcome::Hit }));
        assert!(!second.instruction_complete);
        assert_eq!(sim.pc(), 0x1000, "pc must not advance on the resolving step either — only on the completion action");
        assert_eq!(sim.register(RegisterId(1)), 0, "the register write must not happen until the dedicated completion call");

        // Call 3: only now does the completion (register write + pc
        // advance) happen, exactly like the zero-step / no-cache path.
        let third = sim.step_action();
        assert_eq!(third.kind, ActionKind::RegisterWrite { register: RegisterId(1), value: 0xDEAD_BEEF });
        assert!(third.instruction_complete);
        assert_eq!(sim.pc(), 0x1004);
        assert_eq!(sim.register(RegisterId(1)), 0xDEAD_BEEF);
    }

    #[test]
    fn load_honors_the_offset_immediate() {
        let mut mem = mem();
        mem.preload_ram(0x10000010, &[7, 0, 0, 0]);
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lw, 0x1000, 1, 2, 0x10)]), mem);
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);

        assert_eq!(sim.register(RegisterId(1)), 7);
    }

    fn store_entry(kind: InstructionKind, address: u32, rs2: u8, base: u8, imm: i64) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::X(rs2)),
                Operand::Offset { imm, base: RegisterKind::X(base) },
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn sw_then_lw_round_trips_a_word() {
        let mut sim = RiscvCpuSimulator::new(
            Base::Rv32i,
            codegen(vec![store_entry(InstructionKind::Sw, 0x1000, 1, 2, 0), load_entry(InstructionKind::Lw, 0x1004, 3, 2, 0)]),
            mem(),
        );
        sim.registers[1] = 0x1234_5678;
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim); // sw
        run_to_finished_action(&mut sim); // lw

        assert_eq!(sim.register(RegisterId(3)), 0x1234_5678);
    }

    #[test]
    fn sb_only_writes_the_low_byte() {
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[0, 0, 0, 0]);
        let mut sim = RiscvCpuSimulator::new(
            Base::Rv32i,
            codegen(vec![store_entry(InstructionKind::Sb, 0x1000, 1, 2, 0), load_entry(InstructionKind::Lw, 0x1004, 3, 2, 0)]),
            mem,
        );
        sim.registers[1] = 0xFFFF_FFAB;
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim); // sb
        run_to_finished_action(&mut sim); // lw

        assert_eq!(sim.register(RegisterId(3)), 0x0000_00AB);
    }

    #[test]
    fn store_does_not_advance_past_pc_until_finished() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![store_entry(InstructionKind::Sw, 0x1000, 1, 2, 0)]), mem());
        sim.registers[2] = 0x10000000;
        sim.step_action(); // first memory level only
        assert_eq!(sim.pc(), 0x1000);
    }

    #[test]
    fn addiw_sign_extends_the_32_bit_result() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::Addiw,
            operands: vec![
                Operand::Register(RegisterKind::X(1)),
                Operand::Register(RegisterKind::X(2)),
                Operand::Immediate(-1),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![entry]), mem());
        sim.registers[2] = 0;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn addw_operates_on_the_low_32_bits_only() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![r_type_entry(InstructionKind::Addw, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF_0000_0001;
        sim.registers[3] = 1;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 2);
    }

    #[test]
    fn ld_reads_a_full_64_bit_doubleword() {
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[1, 0, 0, 0, 0, 0, 0, 0x80]);
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![load_entry(InstructionKind::Ld, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);

        assert_eq!(sim.register(RegisterId(1)), 0x8000_0000_0000_0001);
    }

    #[test]
    fn rv64i_lw_sign_extends_unlike_lwu() {
        // Regression test for finding #2: `Lw` loads 32 bits and must
        // sign-extend to fill the 64-bit register under Rv64i, unlike
        // `Lwu` which zero-extends. This test distinguishes the two.
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[0xFF, 0xFF, 0xFF, 0xFF]);
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![load_entry(InstructionKind::Lw, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);

        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn rv64i_lwu_zero_extends_unlike_lw() {
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[0xFF, 0xFF, 0xFF, 0xFF]);
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![load_entry(InstructionKind::Lwu, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);

        assert_eq!(sim.register(RegisterId(1)), 0x0000_0000_FFFF_FFFF);
    }

    #[test]
    fn mul_wraps_within_word_width() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Mul, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0x1_0000;
        sim.registers[3] = 0x1_0000;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0); // 0x1_0000 * 0x1_0000 = 0x1_0000_0000, wraps to 0 in 32 bits
    }

    #[test]
    fn mulhu_returns_the_upper_half_of_an_unsigned_product() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Mulhu, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF;
        sim.registers[3] = 0xFFFF_FFFF;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFE);
    }

    #[test]
    fn div_by_zero_yields_all_ones() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Div, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 42;
        sim.registers[3] = 0;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF);
    }

    #[test]
    fn rem_by_zero_yields_the_dividend() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Rem, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 42;
        sim.registers[3] = 0;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 42);
    }

    #[test]
    fn divu_computes_unsigned_division() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Divu, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 10;
        sim.registers[3] = 3;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 3);
    }

    #[test]
    fn fence_is_a_no_op_that_advances_pc() {
        let entry = IrEntry { address: 0x1000, kind: InstructionKind::Fence, operands: vec![], bytecode: 0, source_line: 1, source_column: 1 };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        let action = sim.step_action();
        assert_eq!(action.kind, ActionKind::NoOp);
        assert_eq!(sim.pc(), 0x1004);
    }

    #[test]
    fn ecall_advances_past_itself_and_the_program_can_finish() {
        let entry = IrEntry { address: 0x1000, kind: InstructionKind::Ecall, operands: vec![], bytecode: 0, source_line: 1, source_column: 1 };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.step_action();
        assert!(sim.is_finished());
    }

    fn u_type_entry(kind: InstructionKind, address: u32, rd: u8, imm: i64) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![Operand::Register(RegisterKind::X(rd)), Operand::Immediate(imm)],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn rv64i_lui_sign_extends_bit_31_of_its_result() {
        // Regression test for finding #3. The immediate arrives as the
        // unsigned 20-bit value 0xFFFFF (bit 19 set), which after `<< 12`
        // sets bit 31 of the 32-bit result — that must sign-extend through
        // the full 64-bit register under Rv64i.
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![u_type_entry(InstructionKind::Lui, 0x1000, 1, 0xFFFFF)]), mem());
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF_FFFF_F000);
    }

    #[test]
    fn rv32i_lui_is_unaffected_by_the_sign_extension_fix() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![u_type_entry(InstructionKind::Lui, 0x1000, 1, 0xFFFFF)]), mem());
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_F000);
    }

    #[test]
    fn rv64i_auipc_sign_extends_bit_31_of_its_result() {
        // Regression test for finding #3. `0x1000 + (-4096)` must be `0`;
        // before the fix the un-sign-extended immediate made this
        // `0x1_0000_0000` instead.
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![u_type_entry(InstructionKind::Auipc, 0x1000, 1, 0xFFFFF)]), mem());
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    #[test]
    fn rv32i_sltiu_compares_against_the_sign_extended_then_masked_immediate() {
        // Regression test for finding #4: `sltiu x1, x2, -1` with
        // `x2 = 0xFFFF_FFFF` must yield 0 under Rv32i — comparing the
        // register's max 32-bit value against itself is never "less
        // than". The old code cast the immediate straight `as u64`,
        // producing 0xFFFF_FFFF_FFFF_FFFF, which made the comparison
        // always true.
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Sltiu, 0x1000, 1, 2, -1)]), mem());
        sim.registers[2] = 0xFFFF_FFFF;
        sim.step_action();
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    #[test]
    fn bytes_to_value_does_not_panic_on_a_full_8_byte_sign_extending_read() {
        // Regression test for finding #6: an 8-byte read is already the
        // full width of the `u64` accumulator, so sign-extension is a
        // no-op — but the naive `!0u64 << 64` would panic in a debug
        // build. Unreachable via any current instruction, but cheap to
        // guard directly.
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x80];
        let value = RiscvCpuSimulator::bytes_to_value(&bytes, true);
        assert_eq!(value, 0x80FF_FFFF_FFFF_FFFF);
    }
}
