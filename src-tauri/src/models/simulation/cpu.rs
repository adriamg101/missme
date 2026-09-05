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
    #[allow(dead_code)] // kept alongside the other access details for consistency, nothing reads it yet but something probably will soon
    size: u8,
    sign_extend: bool,
    is_fp: bool,
}

pub struct RiscvCpuSimulator {
    base: Base,
    registers: [u64; 32],
    f_registers: [f64; 32],
    pc: u64,
    instructions: InstructionStore,
    memory: MemoryHierarchySimulator,
    pending: Option<PendingMemoryOp>,
    // Source line of the instruction being fetched right now, while it makes its way through the memory hierarchy.
    // Works like pending does for loads and stores, but a fetch has no register to write back.
    fetch_pending: Option<usize>,
}

impl RiscvCpuSimulator {
    pub fn new(base: Base, codegen: CodegenOutput, mut memory: MemoryHierarchySimulator) -> Self {
        let text_base = codegen.layout.text_base as u64;
        let data_base = codegen.layout.data_base as u64;
        memory.preload_ram(data_base, &codegen.layout.initial_data);
        // The instruction bytes are also loaded into RAM in program order, so a fetch has real bytes to read.
        // The simulator never reads them back to decode instructions though, it always uses the real source below.
        let text_bytes: Vec<u8> = codegen.entries.iter().flat_map(|entry| entry.bytecode.to_le_bytes()).collect();
        memory.preload_ram(text_base, &text_bytes);
        Self {
            base,
            registers: [0; 32],
            f_registers: [0.0; 32],
            pc: text_base,
            instructions: InstructionStore::new(text_base, codegen.entries),
            memory,
            pending: None,
            fetch_pending: None,
        }
    }

    /// Trims a value down to this CPU's word size, 32 bits for Rv32i, the full 64 bits for Rv64i.
    fn mask(&self, value: u64) -> u64 {
        match self.base {
            Base::Rv32i => value & 0xFFFF_FFFF,
            Base::Rv64i => value,
        }
    }

    /// Turns a value into a signed number, based on this CPU's word size.
    /// For Rv32i only the lower 32 bits count, with the sign extended from there, Rv64i uses the full 64 bits.
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

    fn read_fp_register(&self, id: RegisterId) -> f64 {
        self.f_registers[id.0 as usize]
    }

    fn write_fp_register(&mut self, id: RegisterId, value: f64) {
        self.f_registers[id.0 as usize] = value;
    }

    fn fp_register_id(operand: &Operand) -> RegisterId {
        match operand {
            Operand::Register(RegisterKind::F(n)) => RegisterId(*n),
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
            // A full 64 bit read already fills the whole value, so there is nothing left to extend.
            // Shifting by a full 64 bits would crash in a debug build, this guard just protects against that.
            if bit_width < 64 && value & sign_bit != 0 {
                value |= !0u64 << bit_width;
            }
        }
        value
    }

    fn fclass_bits_f32(v: f32) -> u64 {
        if v.is_nan() {
            let mantissa = v.to_bits() & 0x007F_FFFF;
            return if mantissa & 0x0040_0000 != 0 { 1 << 9 } else { 1 << 8 };
        }
        if v.is_infinite() {
            return if v.is_sign_negative() { 1 << 0 } else { 1 << 7 };
        }
        if v == 0.0 {
            return if v.is_sign_negative() { 1 << 3 } else { 1 << 4 };
        }
        if v.is_normal() {
            return if v.is_sign_negative() { 1 << 1 } else { 1 << 6 };
        }
        if v.is_sign_negative() { 1 << 2 } else { 1 << 5 }
    }

    fn fclass_bits_f64(v: f64) -> u64 {
        if v.is_nan() {
            let mantissa = v.to_bits() & 0x000F_FFFF_FFFF_FFFF;
            return if mantissa & 0x0008_0000_0000_0000 != 0 { 1 << 9 } else { 1 << 8 };
        }
        if v.is_infinite() {
            return if v.is_sign_negative() { 1 << 0 } else { 1 << 7 };
        }
        if v == 0.0 {
            return if v.is_sign_negative() { 1 << 3 } else { 1 << 4 };
        }
        if v.is_normal() {
            return if v.is_sign_negative() { 1 << 1 } else { 1 << 6 };
        }
        if v.is_sign_negative() { 1 << 2 } else { 1 << 5 }
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
                // The value comes in unsigned, and shifting it left can set the top bit of the 32 bit result.
                // Under Rv64i that top bit needs to carry through the full 64 bits, hence the signed conversion below.
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
                // Same shifting logic as Lui above, then add the program counter.
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
                self.pending = Some(PendingMemoryOp { source_line: entry.source_line, register: rd, size, sign_extend, is_fp: false });
                return Action { kind: ActionKind::MemoryLevel(level_result), source_line: entry.source_line, instruction_complete: false };
            }
            InstructionKind::Flw | InstructionKind::Fld => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let (imm, base) = match &entry.operands[1] {
                    Operand::Offset { imm, base } => (*imm, Self::register_id(&Operand::Register(*base))),
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let address = (self.read_register(base) as i64 + imm) as u64;
                let size: u8 = if entry.kind == InstructionKind::Flw { 4 } else { 8 };
                let access = super::memory::hierarchy::MemoryAccess {
                    kind: super::memory::hierarchy::AccessKind::Load,
                    address,
                    size,
                    store_bytes: None,
                };
                let level_result = self.memory.begin_access(access);
                self.pending = Some(PendingMemoryOp { source_line: entry.source_line, register: rd, size, sign_extend: false, is_fp: true });
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
                self.pending = Some(PendingMemoryOp { source_line: entry.source_line, register: RegisterId(0), size, sign_extend: false, is_fp: false });
                return Action { kind: ActionKind::MemoryLevel(level_result), source_line: entry.source_line, instruction_complete: false };
            }
            InstructionKind::Fsw | InstructionKind::Fsd => {
                let rs2 = Self::fp_register_id(&entry.operands[0]);
                let (imm, base) = match &entry.operands[1] {
                    Operand::Offset { imm, base } => (*imm, Self::register_id(&Operand::Register(*base))),
                    _ => unreachable!("operand shape already validated by semantic analysis"),
                };
                let address = (self.read_register(base) as i64 + imm) as u64;
                let value = self.read_fp_register(rs2);
                let bytes: Vec<u8> = if entry.kind == InstructionKind::Fsw {
                    (value as f32).to_bits().to_le_bytes().to_vec()
                } else {
                    value.to_bits().to_le_bytes().to_vec()
                };
                let size = bytes.len() as u8;
                let access = super::memory::hierarchy::MemoryAccess {
                    kind: super::memory::hierarchy::AccessKind::Store,
                    address,
                    size,
                    store_bytes: Some(bytes),
                };
                let level_result = self.memory.begin_access(access);
                self.pending = Some(PendingMemoryOp { source_line: entry.source_line, register: RegisterId(0), size, sign_extend: false, is_fp: false });
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
            InstructionKind::FaddS | InstructionKind::FsubS | InstructionKind::FmulS | InstructionKind::FdivS => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1) as f32;
                let b = self.read_fp_register(rs2) as f32;
                let result = match entry.kind {
                    InstructionKind::FaddS => a + b,
                    InstructionKind::FsubS => a - b,
                    InstructionKind::FmulS => a * b,
                    InstructionKind::FdivS => a / b,
                    _ => unreachable!(),
                };
                self.write_fp_register(rd, result as f64);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FaddD | InstructionKind::FsubD | InstructionKind::FmulD | InstructionKind::FdivD => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1);
                let b = self.read_fp_register(rs2);
                let result = match entry.kind {
                    InstructionKind::FaddD => a + b,
                    InstructionKind::FsubD => a - b,
                    InstructionKind::FmulD => a * b,
                    InstructionKind::FdivD => a / b,
                    _ => unreachable!(),
                };
                self.write_fp_register(rd, result);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FsqrtS => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let a = self.read_fp_register(rs1) as f32;
                self.write_fp_register(rd, a.sqrt() as f64);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FsqrtD => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let a = self.read_fp_register(rs1);
                self.write_fp_register(rd, a.sqrt());
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FsgnjS | InstructionKind::FsgnjnS | InstructionKind::FsgnjxS => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1) as f32;
                let b = self.read_fp_register(rs2) as f32;
                let result = match entry.kind {
                    InstructionKind::FsgnjS => a.copysign(b),
                    InstructionKind::FsgnjnS => a.copysign(-b),
                    InstructionKind::FsgnjxS => {
                        let sign: f32 = if a.is_sign_negative() ^ b.is_sign_negative() { -1.0 } else { 1.0 };
                        a.abs().copysign(sign)
                    }
                    _ => unreachable!(),
                };
                self.write_fp_register(rd, result as f64);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FsgnjD | InstructionKind::FsgnjnD | InstructionKind::FsgnjxD => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1);
                let b = self.read_fp_register(rs2);
                let result = match entry.kind {
                    InstructionKind::FsgnjD => a.copysign(b),
                    InstructionKind::FsgnjnD => a.copysign(-b),
                    InstructionKind::FsgnjxD => {
                        let sign: f64 = if a.is_sign_negative() ^ b.is_sign_negative() { -1.0 } else { 1.0 };
                        a.abs().copysign(sign)
                    }
                    _ => unreachable!(),
                };
                self.write_fp_register(rd, result);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            // RISC-V says the sign of zero matters for fmin and fmax, and Rust does not officially guarantee the same behavior.
            // The current Rust version happens to give the right answer, worth checking again after a Rust update.
            InstructionKind::FminS | InstructionKind::FmaxS => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1) as f32;
                let b = self.read_fp_register(rs2) as f32;
                let result = if entry.kind == InstructionKind::FminS { a.min(b) } else { a.max(b) };
                self.write_fp_register(rd, result as f64);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            // Same situation as FminS and FmaxS above.
            InstructionKind::FminD | InstructionKind::FmaxD => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1);
                let b = self.read_fp_register(rs2);
                let result = if entry.kind == InstructionKind::FminD { a.min(b) } else { a.max(b) };
                self.write_fp_register(rd, result);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FmaddS | InstructionKind::FmsubS | InstructionKind::FnmsubS | InstructionKind::FnmaddS => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let rs3 = Self::fp_register_id(&entry.operands[3]);
                let a = self.read_fp_register(rs1) as f32;
                let b = self.read_fp_register(rs2) as f32;
                let c = self.read_fp_register(rs3) as f32;
                let result = match entry.kind {
                    InstructionKind::FmaddS => a.mul_add(b, c),
                    InstructionKind::FmsubS => a.mul_add(b, -c),
                    InstructionKind::FnmsubS => (-a).mul_add(b, c),
                    InstructionKind::FnmaddS => (-a).mul_add(b, -c),
                    _ => unreachable!(),
                };
                self.write_fp_register(rd, result as f64);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FmaddD | InstructionKind::FmsubD | InstructionKind::FnmsubD | InstructionKind::FnmaddD => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let rs3 = Self::fp_register_id(&entry.operands[3]);
                let a = self.read_fp_register(rs1);
                let b = self.read_fp_register(rs2);
                let c = self.read_fp_register(rs3);
                let result = match entry.kind {
                    InstructionKind::FmaddD => a.mul_add(b, c),
                    InstructionKind::FmsubD => a.mul_add(b, -c),
                    InstructionKind::FnmsubD => (-a).mul_add(b, c),
                    InstructionKind::FnmaddD => (-a).mul_add(b, -c),
                    _ => unreachable!(),
                };
                self.write_fp_register(rd, result);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FeqS | InstructionKind::FltS | InstructionKind::FleS => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1) as f32;
                let b = self.read_fp_register(rs2) as f32;
                let result = match entry.kind {
                    InstructionKind::FeqS => a == b,
                    InstructionKind::FltS => a < b,
                    InstructionKind::FleS => a <= b,
                    _ => unreachable!(),
                };
                self.write_register(rd, result as u64);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FeqD | InstructionKind::FltD | InstructionKind::FleD => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let rs2 = Self::fp_register_id(&entry.operands[2]);
                let a = self.read_fp_register(rs1);
                let b = self.read_fp_register(rs2);
                let result = match entry.kind {
                    InstructionKind::FeqD => a == b,
                    InstructionKind::FltD => a < b,
                    InstructionKind::FleD => a <= b,
                    _ => unreachable!(),
                };
                self.write_register(rd, result as u64);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FclassS | InstructionKind::FclassD => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let value = self.read_fp_register(rs1);
                let bits = if entry.kind == InstructionKind::FclassS {
                    Self::fclass_bits_f32(value as f32)
                } else {
                    Self::fclass_bits_f64(value)
                };
                self.write_register(rd, bits);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FcvtWS | InstructionKind::FcvtWuS | InstructionKind::FcvtWD | InstructionKind::FcvtWuD => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let value = self.read_fp_register(rs1);
                let is_single = matches!(entry.kind, InstructionKind::FcvtWS | InstructionKind::FcvtWuS);
                let is_unsigned = matches!(entry.kind, InstructionKind::FcvtWuS | InstructionKind::FcvtWuD);
                // A normal too large number already clamps correctly to the smallest or largest possible value.
                // Converting NaN this way would give 0, but RISC-V requires NaN to convert to the largest possible value.
                let is_nan = if is_single { (value as f32).is_nan() } else { value.is_nan() };
                let result: u64 = if is_unsigned {
                    let v: u32 = if is_nan {
                        u32::MAX
                    } else if is_single {
                        (value as f32) as u32
                    } else {
                        value as u32
                    };
                    // The unsigned result extends to the full register width the same way the signed version does, carrying the sign bit.
                    // This is unusual but it is what RISC-V requires, and on Rv32i the masking below makes the difference invisible anyway.
                    self.mask(v as i32 as i64 as u64)
                } else {
                    let v: i32 = if is_nan {
                        i32::MAX
                    } else if is_single {
                        (value as f32) as i32
                    } else {
                        value as i32
                    };
                    self.mask(v as i64 as u64)
                };
                self.write_register(rd, result);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FcvtSW | InstructionKind::FcvtSWu | InstructionKind::FcvtDW | InstructionKind::FcvtDWu => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let is_single = matches!(entry.kind, InstructionKind::FcvtSW | InstructionKind::FcvtSWu);
                let is_unsigned = matches!(entry.kind, InstructionKind::FcvtSWu | InstructionKind::FcvtDWu);
                let raw = self.read_register(rs1);
                // These instructions convert a 32 bit integer, always keeping only the lower 32 bits first, no matter the word size.
                // On Rv64i the full 64 bit register value must not be used directly, since the upper bits could hold unrelated data.
                let as_f64 = if is_unsigned {
                    (raw as u32) as f64
                } else {
                    (raw as u32 as i32) as f64
                };
                let result = if is_single { as_f64 as f32 as f64 } else { as_f64 };
                self.write_fp_register(rd, result);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            // Floating point registers always store 64 bit values here, even for 32 bit numbers, as a deliberate simplification.
            // Moving a 32 bit NaN out and back through FmvWX and FmvXW can lose its exact bit pattern, an expected side effect.
            InstructionKind::FmvXW => {
                let rd = Self::register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let bits = (self.read_fp_register(rs1) as f32).to_bits();
                let value = self.mask(bits as i32 as i64 as u64);
                self.write_register(rd, value);
                self.pc += 4;
                Action {
                    kind: ActionKind::RegisterWrite { register: rd, value: self.read_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FmvWX => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::register_id(&entry.operands[1]);
                let bits = self.read_register(rs1) as u32;
                self.write_fp_register(rd, f32::from_bits(bits) as f64);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
                    source_line: entry.source_line,
                    instruction_complete: true,
                }
            }
            InstructionKind::FcvtSD | InstructionKind::FcvtDS => {
                let rd = Self::fp_register_id(&entry.operands[0]);
                let rs1 = Self::fp_register_id(&entry.operands[1]);
                let a = self.read_fp_register(rs1);
                let result = (a as f32) as f64;
                self.write_fp_register(rd, result);
                self.pc += 4;
                Action {
                    kind: ActionKind::FpRegisterWrite { register: rd, value: self.read_fp_register(rd) },
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
            other => panic!("RiscvCpuSimulator does not implement {:?} (the Q extension is out of scope for this branch's floating-point simulation work)", other),
        }
    }
}

impl CpuSimulator for RiscvCpuSimulator {
    fn step_action(&mut self) -> Action {
        assert!(!self.is_finished(), "step_action called after the program finished");

        if let Some(pending) = &self.pending {
            let source_line = pending.source_line;
            // The memory access may have already finished as soon as it started, for example with no cache levels.
            // In that case there is no extra step to report, so we go straight to finishing up below.
            if self.memory.is_access_finished() {
                let register = pending.register;
                let sign_extend = pending.sign_extend;
                let is_fp = pending.is_fp;
                self.pending = None;
                let result = self.memory.take_result();
                self.pc += 4;
                if let Some(bytes) = result.load_bytes {
                    if is_fp {
                        let value = if bytes.len() == 4 {
                            f32::from_bits(u32::from_le_bytes(bytes.try_into().unwrap())) as f64
                        } else {
                            f64::from_bits(u64::from_le_bytes(bytes.try_into().unwrap()))
                        };
                        self.write_fp_register(register, value);
                        return Action {
                            kind: ActionKind::FpRegisterWrite { register, value: self.read_fp_register(register) },
                            source_line,
                            instruction_complete: true,
                        };
                    }
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
            // Otherwise the memory access is still going, take one more step and report what happened.
            // Finishing up is left for the next call, which sees the access is done and takes the branch above.
            let level_result = self.memory.step();
            return Action { kind: ActionKind::MemoryLevel(level_result), source_line, instruction_complete: false };
        }

        if let Some(source_line) = self.fetch_pending {
            // Works the same way as the pending branch above, reporting one step per call while the fetch is going.
            // Once it finishes, the fetched bytes are not needed since decoding uses the real source, so run the instruction right away.
            if self.memory.is_access_finished() {
                self.fetch_pending = None;
                self.memory.take_result();
                return self.execute_current_instruction();
            }
            let level_result = self.memory.step();
            return Action { kind: ActionKind::MemoryLevel(level_result), source_line, instruction_complete: false };
        }

        // Start fetching the instruction at the current program counter through the memory hierarchy.
        // This only simulates cache, TLB, and RAM hits and misses for the memory views, decoding uses the real source directly.
        let entry = self
            .instructions
            .fetch(self.pc)
            .expect("step_action called while finished");
        let source_line = entry.source_line;
        let access = super::memory::hierarchy::MemoryAccess {
            kind: super::memory::hierarchy::AccessKind::Fetch,
            address: self.pc,
            size: 4,
            store_bytes: None,
        };
        let level_result = self.memory.begin_access(access);
        self.fetch_pending = Some(source_line);
        Action { kind: ActionKind::MemoryLevel(level_result), source_line, instruction_complete: false }
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

    fn fp_register(&self, id: RegisterId) -> f64 {
        self.read_fp_register(id)
    }

    fn memory(&self) -> &MemoryHierarchySimulator {
        &self.memory
    }

    fn current_source_line(&self) -> Option<usize> {
        self.instructions.fetch(self.pc).map(|entry| entry.source_line)
    }
}

#[cfg(test)]
mod tests {

    fn run_to_finished_action(sim: &mut RiscvCpuSimulator) -> Action {
        loop {
            let action = sim.step_action();
            if action.instruction_complete {
                return action;
            }
        }
    }

    use super::*;
    use crate::models::interpreter::riscv::codegen::ir::{CodegenOutput, MemoryLayout};
    use crate::models::interpreter::riscv::lexer::token::RegisterKind;
    use crate::models::interpreter::riscv::parser::ast::Operand;
    use crate::models::simulation::memory::backing_store::DataStore;
    use crate::models::simulation::memory::config::{
        CacheConfig, CacheEntry, DiskConfig, FrameAllocationPolicy, HierarchyConfig, InclusionPolicy, Level, RamConfig, ReplacementPolicy, TlbConfig,
        TlbReplacementPolicy, WritePolicy,
    };
    use crate::models::simulation::memory::hierarchy::{AccessKind, LevelId, LevelOutcome, LevelResult, MemoryHierarchySimulator};
    use std::collections::HashMap;

    fn no_cache_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            page_size: 4096,
            hierarchy: vec![],
            tlb: TlbConfig {
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
                latency: 1,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    fn mem() -> MemoryHierarchySimulator {
        MemoryHierarchySimulator::new(no_cache_hierarchy_config(), DataStore::new(64), DataStore::new(64), 0)
    }

    /// A memory setup with one cache level between the CPU and RAM.
    /// A brand new access here misses the cache first and needs another step to reach RAM.
    fn one_cache_level_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            page_size: 4096,
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
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
                latency: 1,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    fn mem_with_one_cache_level() -> MemoryHierarchySimulator {
        MemoryHierarchySimulator::new(one_cache_level_hierarchy_config(), DataStore::new(4), DataStore::new(4), 0)
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

        let action = run_to_finished_action(&mut sim);

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
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(0)), 0);
    }

    #[test]
    fn instruction_bytecode_is_preloaded_into_ram_at_text_base() {
        // The memory simulator has no separate place to store instructions, so a fetch needs real bytes to read.
        // The compiled instructions are loaded into RAM in program order, so fetches have something real to find.
        let entry_a = IrEntry {
            address: 0x1000,
            kind: InstructionKind::Add,
            operands: vec![
                Operand::Register(RegisterKind::X(1)),
                Operand::Register(RegisterKind::X(2)),
                Operand::Register(RegisterKind::X(3)),
            ],
            bytecode: 0x003100B3,
            source_line: 1,
            source_column: 1,
        };
        let entry_b = IrEntry { address: 0x1004, bytecode: 0xDEADBEEF, ..entry_a.clone() };
        let sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry_a, entry_b]), mem());

        let blocks = sim.memory().ram_blocks(0x1000, 1);
        assert!(blocks[0].populated);
        assert_eq!(blocks[0].words[0], 0x003100B3);
        assert_eq!(blocks[0].words[1], 0xDEADBEEF);
    }

    #[test]
    fn program_is_finished_after_its_only_instruction_executes() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![add_entry(0x1000, 1, 2, 3)]), mem());
        run_to_finished_action(&mut sim);
        assert!(sim.is_finished());
    }

    #[test]
    fn addi_adds_a_sign_extended_immediate() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Addi, 0x1000, 1, 2, -5)]), mem());
        sim.registers[2] = 10;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 5);
    }

    #[test]
    fn sub_subtracts_with_wraparound() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Sub, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0;
        sim.registers[3] = 1;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF);
    }

    #[test]
    fn slt_reports_signed_less_than() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Slt, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF; // -1 signed
        sim.registers[3] = 1;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1);
    }

    #[test]
    fn sltu_reports_unsigned_less_than() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Sltu, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF;
        sim.registers[3] = 1;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    #[test]
    fn srai_preserves_sign() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Srai, 0x1000, 1, 2, 1)]), mem());
        sim.registers[2] = 0xFFFF_FFFE; // -2
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF); // -1
    }

    #[test]
    fn srli_is_logical() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Srli, 0x1000, 1, 2, 1)]), mem());
        sim.registers[2] = 0xFFFF_FFFE;
        run_to_finished_action(&mut sim);
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
        run_to_finished_action(&mut sim);
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
        run_to_finished_action(&mut sim);
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
        run_to_finished_action(&mut sim);
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
        run_to_finished_action(&mut sim);
        assert_eq!(sim.pc(), 0x2000); // 0x2000 + 1, low bit cleared
    }

    #[test]
    fn beq_branches_when_operands_are_equal() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Beq, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 5;
        sim.registers[2] = 5;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.pc(), 0x1020);
    }

    #[test]
    fn beq_falls_through_when_operands_differ() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Beq, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 5;
        sim.registers[2] = 6;
        let action = run_to_finished_action(&mut sim);
        assert_eq!(action.kind, ActionKind::NoOp);
        assert_eq!(sim.pc(), 0x1004);
    }

    #[test]
    fn bltu_compares_unsigned() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Bltu, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 0xFFFF_FFFF; // huge unsigned, would be -1 signed
        sim.registers[2] = 1;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.pc(), 0x1004); // not taken, 0xFFFFFFFF is NOT < 1 unsigned
    }

    #[test]
    fn blt_treats_register_as_signed() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Blt, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 0xFFFF_FFFF; // -1 signed
        sim.registers[2] = 1;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.pc(), 0x1020); // taken, -1 < 1 signed
    }

    #[test]
    fn bge_treats_register_as_signed() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![branch_entry(InstructionKind::Bge, 0x1000, 1, 2, 0x20)]), mem());
        sim.registers[1] = 0xFFFF_FFFF; // -1 signed
        sim.registers[2] = 1;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.pc(), 0x1004); // not taken, -1 >= 1 is false
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

    #[test]
    fn lw_reads_a_previously_stored_word() {
        let mut mem = mem();
        mem.preload_ram(0x10000000, &[0xEF, 0xBE, 0xAD, 0xDE]); // little endian 0xDEADBEEF
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lw, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        // Every instruction starts with its own fetch step, and with no cache levels that goes straight to RAM and finishes.
        // This throwaway call finishes the fetch and immediately starts the load's own access, which also finishes straight away.
        sim.step_action();

        // The load's own access also goes straight to RAM, so it needs no extra step.
        // The next call must find the access already finished and complete right away.
        let first = sim.step_action();
        assert_eq!(
            first.kind,
            ActionKind::MemoryLevel(LevelResult {
                level: LevelId::Ram,
                outcome: LevelOutcome::Hit,
                address: 0x10000000,
                access_kind: AccessKind::Load,
                page_translation_outcome: None,
            })
        );
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
        // Regression test, with one cache level a fresh load misses the cache first, then a later step reaches RAM.
        // That RAM step must be reported as its own action, not silently combined with the final register write.
        let mut mem = mem_with_one_cache_level();
        mem.preload_ram(0x10000000, &[0xEF, 0xBE, 0xAD, 0xDE]); // little endian 0xDEADBEEF
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![load_entry(InstructionKind::Lw, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        // Every instruction starts with its own fetch step, which for this address misses the cache first, then hits RAM.
        // That finishes the fetch and immediately starts the load's own access, whose result is what first below reads.
        sim.step_action();
        sim.step_action();

        // The load's own access first misses the cache.
        let first = sim.step_action();
        assert_eq!(
            first.kind,
            ActionKind::MemoryLevel(LevelResult {
                level: LevelId::Cache(0),
                outcome: LevelOutcome::Miss,
                address: 0x10000000,
                access_kind: AccessKind::Load,
                page_translation_outcome: None,
            })
        );
        assert!(!first.instruction_complete);
        assert_eq!(sim.pc(), 0x1000, "pc must not advance until the instruction actually completes");

        // This step reaches RAM and finishes the access, actually resolving the load.
        // It must be reported here as its own action, not skipped.
        let second = sim.step_action();
        assert_eq!(
            second.kind,
            ActionKind::MemoryLevel(LevelResult {
                level: LevelId::Ram,
                outcome: LevelOutcome::Hit,
                address: 0x10000000,
                access_kind: AccessKind::Load,
                page_translation_outcome: None,
            })
        );
        assert!(!second.instruction_complete);
        assert_eq!(sim.pc(), 0x1000, "pc must not advance on the resolving step either — only on the completion action");
        assert_eq!(sim.register(RegisterId(1)), 0, "the register write must not happen until the dedicated completion call");

        // Only now does finishing up happen, the register write and moving the program counter forward, just like the no cache case.
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
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn addw_operates_on_the_low_32_bits_only() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![r_type_entry(InstructionKind::Addw, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF_0000_0001;
        sim.registers[3] = 1;
        run_to_finished_action(&mut sim);
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
        // Regression test, Lw loads 32 bits and must extend the sign to fill the full register under Rv64i.
        // Lwu instead fills the rest with zeros, this test tells the two apart.
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
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0); // 0x1_0000 * 0x1_0000 = 0x1_0000_0000, wraps to 0 in 32 bits
    }

    #[test]
    fn mulhu_returns_the_upper_half_of_an_unsigned_product() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Mulhu, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 0xFFFF_FFFF;
        sim.registers[3] = 0xFFFF_FFFF;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFE);
    }

    #[test]
    fn div_by_zero_yields_all_ones() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Div, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 42;
        sim.registers[3] = 0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF);
    }

    #[test]
    fn rem_by_zero_yields_the_dividend() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Rem, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 42;
        sim.registers[3] = 0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 42);
    }

    #[test]
    fn divu_computes_unsigned_division() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![r_type_entry(InstructionKind::Divu, 0x1000, 1, 2, 3)]), mem());
        sim.registers[2] = 10;
        sim.registers[3] = 3;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 3);
    }

    #[test]
    fn fence_is_a_no_op_that_advances_pc() {
        let entry = IrEntry { address: 0x1000, kind: InstructionKind::Fence, operands: vec![], bytecode: 0, source_line: 1, source_column: 1 };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        let action = run_to_finished_action(&mut sim);
        assert_eq!(action.kind, ActionKind::NoOp);
        assert_eq!(sim.pc(), 0x1004);
    }

    #[test]
    fn ecall_advances_past_itself_and_the_program_can_finish() {
        let entry = IrEntry { address: 0x1000, kind: InstructionKind::Ecall, operands: vec![], bytecode: 0, source_line: 1, source_column: 1 };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        run_to_finished_action(&mut sim);
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
        // Regression test, the value comes in as an unsigned number, and shifting it left sets the top bit of the result.
        // That top bit must extend through the full register under Rv64i.
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![u_type_entry(InstructionKind::Lui, 0x1000, 1, 0xFFFFF)]), mem());
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF_FFFF_F000);
    }

    #[test]
    fn rv32i_lui_is_unaffected_by_the_sign_extension_fix() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![u_type_entry(InstructionKind::Lui, 0x1000, 1, 0xFFFFF)]), mem());
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_F000);
    }

    #[test]
    fn rv64i_auipc_sign_extends_bit_31_of_its_result() {
        // Regression test, adding these two values must equal zero.
        // Before this was fixed, the value was not properly sign extended and the result came out wrong.
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![u_type_entry(InstructionKind::Auipc, 0x1000, 1, 0xFFFFF)]), mem());
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    #[test]
    fn rv32i_sltiu_compares_against_the_sign_extended_then_masked_immediate() {
        // Regression test, comparing a register to its own largest value must give zero under Rv32i.
        // The old code converted the value the wrong way, producing a huge number that made the comparison always come out true.
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![i_type_entry(InstructionKind::Sltiu, 0x1000, 1, 2, -1)]), mem());
        sim.registers[2] = 0xFFFF_FFFF;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    fn fp_load_entry(kind: InstructionKind, address: u32, rd: u8, base: u8, imm: i64) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::F(rd)),
                Operand::Offset { imm, base: RegisterKind::X(base) },
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    fn fp_store_entry(kind: InstructionKind, address: u32, rs2: u8, base: u8, imm: i64) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::F(rs2)),
                Operand::Offset { imm, base: RegisterKind::X(base) },
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    fn fp_r3_entry(kind: InstructionKind, address: u32, rd: u8, rs1: u8, rs2: u8) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::F(rd)),
                Operand::Register(RegisterKind::F(rs1)),
                Operand::Register(RegisterKind::F(rs2)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn flw_reads_a_previously_stored_f32_bit_pattern() {
        let mut mem = mem();
        let bits: u32 = 1.5f32.to_bits();
        mem.preload_ram(0x10000000, &bits.to_le_bytes());
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_load_entry(InstructionKind::Flw, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        let action = run_to_finished_action(&mut sim);

        assert_eq!(action.kind, ActionKind::FpRegisterWrite { register: RegisterId(1), value: 1.5 });
        assert_eq!(sim.fp_register(RegisterId(1)), 1.5);
    }

    #[test]
    fn fld_reads_a_previously_stored_f64_bit_pattern() {
        let mut mem = mem();
        let bits: u64 = 2.5f64.to_bits();
        mem.preload_ram(0x10000000, &bits.to_le_bytes());
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_load_entry(InstructionKind::Fld, 0x1000, 1, 2, 0)]), mem);
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);

        assert_eq!(sim.fp_register(RegisterId(1)), 2.5);
    }

    #[test]
    fn fsw_then_flw_round_trips_an_f32_value() {
        let mut sim = RiscvCpuSimulator::new(
            Base::Rv32i,
            codegen(vec![fp_store_entry(InstructionKind::Fsw, 0x1000, 1, 2, 0), fp_load_entry(InstructionKind::Flw, 0x1004, 3, 2, 0)]),
            mem(),
        );
        sim.f_registers[1] = 3.25;
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim); // fsw
        run_to_finished_action(&mut sim); // flw

        assert_eq!(sim.fp_register(RegisterId(3)), 3.25);
    }

    #[test]
    fn fsd_then_fld_round_trips_an_f64_value() {
        let mut sim = RiscvCpuSimulator::new(
            Base::Rv32i,
            codegen(vec![fp_store_entry(InstructionKind::Fsd, 0x1000, 1, 2, 0), fp_load_entry(InstructionKind::Fld, 0x1004, 3, 2, 0)]),
            mem(),
        );
        sim.f_registers[1] = 6.75;
        sim.registers[2] = 0x10000000;

        run_to_finished_action(&mut sim);
        run_to_finished_action(&mut sim);

        assert_eq!(sim.fp_register(RegisterId(3)), 6.75);
    }

    #[test]
    fn f0_is_not_hardwired_to_zero() {
        let mut sim = RiscvCpuSimulator::new(
            Base::Rv32i,
            codegen(vec![fp_store_entry(InstructionKind::Fsw, 0x1000, 0, 2, 0), fp_load_entry(InstructionKind::Flw, 0x1004, 3, 2, 0)]),
            mem(),
        );
        sim.f_registers[0] = 9.0;
        sim.registers[2] = 0x10000000;

        // If the source register were wrongly stuck at zero, the store would write the wrong bits.
        // Reloading the stored bytes into a different register actually proves what landed in memory, rather than reading the same register again.
        run_to_finished_action(&mut sim);
        run_to_finished_action(&mut sim);

        assert_eq!(sim.fp_register(RegisterId(3)), 9.0);
    }

    #[test]
    fn fadd_s_computes_in_single_precision() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FaddS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 1.1;
        sim.f_registers[3] = 2.2;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), (1.1f32 + 2.2f32) as f64);
    }

    #[test]
    fn fadd_d_computes_in_double_precision() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FaddD, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 1.1;
        sim.f_registers[3] = 2.2;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 1.1f64 + 2.2f64);
    }

    #[test]
    fn fsub_s_subtracts() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FsubS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = 2.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 3.0);
    }

    #[test]
    fn fmul_d_multiplies() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FmulD, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 3.0;
        sim.f_registers[3] = 4.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 12.0);
    }

    #[test]
    fn fdiv_s_divides() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FdivS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 10.0;
        sim.f_registers[3] = 4.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 2.5);
    }

    fn fp_r2_entry(kind: InstructionKind, address: u32, rd: u8, rs1: u8) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![Operand::Register(RegisterKind::F(rd)), Operand::Register(RegisterKind::F(rs1))],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn fsqrt_d_computes_the_square_root() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r2_entry(InstructionKind::FsqrtD, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 16.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 4.0);
    }

    #[test]
    fn fsgnj_s_takes_the_sign_of_rs2() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FsgnjS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = -1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -5.0);
    }

    #[test]
    fn fsgnjn_s_negates_the_sign_of_rs2() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FsgnjnS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = -1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 5.0);
    }

    #[test]
    fn fsgnjx_s_xors_the_signs() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FsgnjxS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = -5.0;
        sim.f_registers[3] = -1.0;
        run_to_finished_action(&mut sim);
        // Both signs are negative, so they cancel out, giving a positive value.
        assert_eq!(sim.fp_register(RegisterId(1)), 5.0);
    }

    #[test]
    fn fsgnjx_s_xors_the_signs_discriminating_test() {
        // Confirms the sign logic really combines two different signs into a negative result.
        // A wrong implementation that just copies the second value's negated sign would give a positive result instead.
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FsgnjxS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = -1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -5.0);
    }

    #[test]
    fn fsqrt_s_computes_the_square_root() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r2_entry(InstructionKind::FsqrtS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 9.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 3.0);
    }

    #[test]
    fn fsgnj_d_takes_the_sign_of_rs2() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FsgnjD, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = -1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -5.0);
    }

    #[test]
    fn bytes_to_value_does_not_panic_on_a_full_8_byte_sign_extending_read() {
        // Regression test, a full 8 byte read already fills the value, so sign extension should do nothing.
        // A naive implementation could crash trying to shift by a full 64 bits, this guard protects against that.
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x80];
        let value = RiscvCpuSimulator::bytes_to_value(&bytes, true);
        assert_eq!(value, 0x80FF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn fmin_s_returns_the_smaller_value() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FminS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = 2.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 2.0);
    }

    #[test]
    fn fmax_d_returns_the_larger_value() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FmaxD, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = 2.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 5.0);
    }

    #[test]
    fn fmin_s_propagates_the_non_nan_operand() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r3_entry(InstructionKind::FminS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = f64::NAN;
        sim.f_registers[3] = 2.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 2.0);
    }

    fn fp_r4_entry(kind: InstructionKind, address: u32, rd: u8, rs1: u8, rs2: u8, rs3: u8) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::F(rd)),
                Operand::Register(RegisterKind::F(rs1)),
                Operand::Register(RegisterKind::F(rs2)),
                Operand::Register(RegisterKind::F(rs3)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn fmadd_d_computes_rs1_times_rs2_plus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FmaddD, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 7.0); // 2*3+1
    }

    #[test]
    fn fnmadd_d_computes_negative_rs1_times_rs2_minus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FnmaddD, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -7.0); // -(2*3)-1
    }

    #[test]
    fn fmsub_s_computes_rs1_times_rs2_minus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FmsubS, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 5.0); // 2*3-1
    }

    #[test]
    fn fmadd_s_computes_rs1_times_rs2_plus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FmaddS, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 7.0); // 2*3+1
    }

    #[test]
    fn fnmsub_s_computes_negative_rs1_times_rs2_plus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FnmsubS, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -5.0); // -(2*3)+1
    }

    #[test]
    fn fnmadd_s_computes_negative_rs1_times_rs2_minus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FnmaddS, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -7.0); // -(2*3)-1
    }

    #[test]
    fn fmsub_d_computes_rs1_times_rs2_minus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FmsubD, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 5.0); // 2*3-1
    }

    #[test]
    fn fnmsub_d_computes_negative_rs1_times_rs2_plus_rs3() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r4_entry(InstructionKind::FnmsubD, 0x1000, 1, 2, 3, 4)]), mem());
        sim.f_registers[2] = 2.0;
        sim.f_registers[3] = 3.0;
        sim.f_registers[4] = 1.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -5.0); // -(2*3)+1
    }

    fn fp_cmp_entry(kind: InstructionKind, address: u32, rd: u8, rs1: u8, rs2: u8) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![
                Operand::Register(RegisterKind::X(rd)),
                Operand::Register(RegisterKind::F(rs1)),
                Operand::Register(RegisterKind::F(rs2)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn feq_s_writes_1_to_an_integer_register_when_equal() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_cmp_entry(InstructionKind::FeqS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 3.0;
        sim.f_registers[3] = 3.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1);
    }

    #[test]
    fn flt_d_writes_0_when_not_less_than() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_cmp_entry(InstructionKind::FltD, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 5.0;
        sim.f_registers[3] = 3.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    #[test]
    fn feq_s_is_false_when_either_operand_is_nan() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_cmp_entry(InstructionKind::FeqS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = f64::NAN;
        sim.f_registers[3] = f64::NAN;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0);
    }

    #[test]
    fn fle_s_writes_1_when_less_or_equal() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_cmp_entry(InstructionKind::FleS, 0x1000, 1, 2, 3)]), mem());
        sim.f_registers[2] = 3.0;
        sim.f_registers[3] = 3.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1);
    }

    fn fclass_entry(kind: InstructionKind, address: u32, rd: u8, rs1: u8) -> IrEntry {
        IrEntry {
            address,
            kind,
            operands: vec![Operand::Register(RegisterKind::X(rd)), Operand::Register(RegisterKind::F(rs1))],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }
    }

    #[test]
    fn fclass_s_identifies_positive_normal() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FclassS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 3.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1 << 6);
    }

    #[test]
    fn fclass_s_identifies_negative_infinity() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FclassS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = f64::NEG_INFINITY;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1 << 0);
    }

    #[test]
    fn fclass_d_identifies_positive_zero() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FclassD, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 0.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1 << 4);
    }

    #[test]
    fn fclass_d_identifies_negative_zero() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FclassD, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = -0.0;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1 << 3);
    }

    #[test]
    fn fclass_d_identifies_a_quiet_nan() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FclassD, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = f64::NAN; // this is the "quiet" kind of NaN, not the "signaling" kind
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1 << 9);
    }

    #[test]
    fn fcvt_w_s_truncates_toward_zero() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FcvtWS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 3.9;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 3);
    }

    #[test]
    fn fcvt_w_d_converts_a_negative_value() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FcvtWD, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = -3.9;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFD); // -3, with the sign extended within 32 bits and then trimmed
    }

    #[test]
    fn fcvt_s_w_converts_a_signed_integer_to_float() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::FcvtSW,
            operands: vec![Operand::Register(RegisterKind::F(1)), Operand::Register(RegisterKind::X(2))],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.registers[2] = 0xFFFF_FFFF; // -1 under Rv32i
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), -1.0);
    }

    #[test]
    fn fcvt_d_wu_converts_an_unsigned_integer_to_float() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::FcvtDWu,
            operands: vec![Operand::Register(RegisterKind::F(1)), Operand::Register(RegisterKind::X(2))],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.registers[2] = 0xFFFF_FFFF; // 4294967295 unsigned under Rv32i
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 4294967295.0);
    }

    #[test]
    fn fcvt_s_w_on_rv64_truncates_to_the_low_32_bits_before_sign_extending() {
        // Regression test, on Rv64i this conversion must use only the lower 32 bits of the register, not the full value.
        // A register with extra bits set higher up must not affect the converted result.
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::FcvtSW,
            operands: vec![Operand::Register(RegisterKind::F(1)), Operand::Register(RegisterKind::X(2))],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![entry]), mem());
        sim.registers[2] = 0x0000_0001_0000_0000;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 0.0);
    }

    #[test]
    fn fcvt_s_wu_on_rv64_truncates_to_the_low_32_bits_before_zero_extending() {
        // Regression test, the unsigned version, must use only the lower 32 bits, filled with zeros above.
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::FcvtSWu,
            operands: vec![Operand::Register(RegisterKind::F(1)), Operand::Register(RegisterKind::X(2))],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![entry]), mem());
        sim.registers[2] = 0xFFFF_FFFF_0000_0005;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 5.0);
    }

    #[test]
    fn fcvt_wu_s_on_rv64_sign_extends_the_32_bit_result() {
        // Regression test, this conversion extends its 32 bit result using the sign bit, like the signed version does.
        // A result with the top bit set comes back with all the upper bits set to one as well.
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![fclass_entry(InstructionKind::FcvtWuS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 4294967295.0; // 2^32 minus 1, the top bit of the 32 bit result is set
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn fcvt_w_s_maps_nan_to_the_signed_saturated_maximum() {
        // Regression test, RISC-V requires that converting a NaN value gives the largest possible result, not zero.
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![fclass_entry(InstructionKind::FcvtWS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = f64::NAN;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0x7FFF_FFFF);
    }

    #[test]
    fn fcvt_wu_s_maps_nan_to_the_unsigned_saturated_maximum() {
        // Regression test, the unsigned version, NaN must convert to the largest unsigned value with the upper bits extended too.
        let mut sim = RiscvCpuSimulator::new(Base::Rv64i, codegen(vec![fclass_entry(InstructionKind::FcvtWuS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 0.0 / 0.0; // NaN
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn fmv_x_w_reinterprets_bits_not_the_numeric_value() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fclass_entry(InstructionKind::FmvXW, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 1.5;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.register(RegisterId(1)), 1.5f32.to_bits() as u64);
    }

    #[test]
    fn fmv_w_x_reinterprets_bits_not_the_numeric_value() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::FmvWX,
            operands: vec![Operand::Register(RegisterKind::F(1)), Operand::Register(RegisterKind::X(2))],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        };
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        sim.registers[2] = 1.5f32.to_bits() as u64;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 1.5);
    }

    #[test]
    fn fcvt_s_d_narrows_to_single_precision() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r2_entry(InstructionKind::FcvtSD, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 1.0 / 3.0; // a value that differs between f32 and f64 precision
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), (1.0f64 / 3.0) as f32 as f64);
    }

    #[test]
    fn fcvt_d_s_widens_from_single_precision() {
        let mut sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![fp_r2_entry(InstructionKind::FcvtDS, 0x1000, 1, 2)]), mem());
        sim.f_registers[2] = 2.5;
        run_to_finished_action(&mut sim);
        assert_eq!(sim.fp_register(RegisterId(1)), 2.5);
    }

    #[test]
    fn current_source_line_reflects_the_next_instruction_to_run() {
        let entry = IrEntry {
            address: 0x1000,
            kind: InstructionKind::Add,
            operands: vec![
                Operand::Register(RegisterKind::X(1)),
                Operand::Register(RegisterKind::X(2)),
                Operand::Register(RegisterKind::X(3)),
            ],
            bytecode: 0,
            source_line: 42,
            source_column: 1,
        };
        let sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![entry]), mem());
        assert_eq!(sim.current_source_line(), Some(42));
    }

    #[test]
    fn current_source_line_is_none_when_finished() {
        let sim = RiscvCpuSimulator::new(Base::Rv32i, codegen(vec![]), mem());
        assert_eq!(sim.current_source_line(), None);
    }
}
