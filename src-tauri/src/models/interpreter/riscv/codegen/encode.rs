use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
use crate::models::interpreter::riscv::parser::ast::Operand;

fn r_type(opcode: u32, funct3: u32, funct7: u32, rd: u32, rs1: u32, rs2: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

fn i_type(opcode: u32, funct3: u32, rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xFFF) << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

fn s_type(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    (((imm >> 5) & 0x7F) << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | ((imm & 0x1F) << 7) | opcode
}

fn b_type(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    let bit12 = (imm >> 12) & 0x1;
    let bits10_5 = (imm >> 5) & 0x3F;
    let bits4_1 = (imm >> 1) & 0xF;
    let bit11 = (imm >> 11) & 0x1;
    (bit12 << 31) | (bits10_5 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (bits4_1 << 8) | (bit11 << 7) | opcode
}

fn u_type(opcode: u32, rd: u32, imm20: i32) -> u32 {
    (((imm20 as u32) & 0xFFFFF) << 12) | (rd << 7) | opcode
}

fn j_type(opcode: u32, rd: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    let bit20 = (imm >> 20) & 0x1;
    let bits10_1 = (imm >> 1) & 0x3FF;
    let bit11 = (imm >> 11) & 0x1;
    let bits19_12 = (imm >> 12) & 0xFF;
    (bit20 << 31) | (bits10_1 << 21) | (bit11 << 20) | (bits19_12 << 12) | (rd << 7) | opcode
}

fn r4_type(opcode: u32, fmt: u32, rs3: u32, rs2: u32, rs1: u32, rm: u32, rd: u32) -> u32 {
    (rs3 << 27) | (fmt << 25) | (rs2 << 20) | (rs1 << 15) | (rm << 12) | (rd << 7) | opcode
}

fn op_fp(fmt: u32, funct5: u32, rs2: u32, rs1: u32, rm: u32, rd: u32) -> u32 {
    r_type(0b1010011, rm, (funct5 << 2) | fmt, rd, rs1, rs2)
}

fn reg_num(op: &Operand) -> u32 {
    match op {
        Operand::Register(RegisterKind::X(n)) | Operand::Register(RegisterKind::F(n)) => *n as u32,
        _ => unreachable!("operand shape already validated by semantic analysis"),
    }
}

fn imm_of(op: &Operand) -> i32 {
    match op {
        Operand::Immediate(v) => *v as i32,
        _ => unreachable!("operand shape already validated by semantic analysis"),
    }
}

fn offset_parts(op: &Operand) -> (i32, u32) {
    match op {
        Operand::Offset { imm, base } => (*imm as i32, reg_num(&Operand::Register(*base))),
        _ => unreachable!("operand shape already validated by semantic analysis"),
    }
}

pub(super) fn encode(kind: InstructionKind, operands: &[Operand]) -> u32 {
    use InstructionKind::*;
    match kind {
        Lui => u_type(0b0110111, reg_num(&operands[0]), imm_of(&operands[1])),
        Auipc => u_type(0b0010111, reg_num(&operands[0]), imm_of(&operands[1])),
        Jal => j_type(0b1101111, reg_num(&operands[0]), imm_of(&operands[1])),
        Jalr => {
            let (imm, rs1) = offset_parts(&operands[1]);
            i_type(0b1100111, 0b000, reg_num(&operands[0]), rs1, imm)
        }
        Beq | Bne | Blt | Bge | Bltu | Bgeu => {
            let funct3 = match kind {
                Beq => 0b000,
                Bne => 0b001,
                Blt => 0b100,
                Bge => 0b101,
                Bltu => 0b110,
                Bgeu => 0b111,
                _ => unreachable!(),
            };
            b_type(0b1100011, funct3, reg_num(&operands[0]), reg_num(&operands[1]), imm_of(&operands[2]))
        }
        Lb | Lh | Lw | Lbu | Lhu | Lwu | Ld => {
            let funct3 = match kind {
                Lb => 0b000,
                Lh => 0b001,
                Lw => 0b010,
                Lbu => 0b100,
                Lhu => 0b101,
                Lwu => 0b110,
                Ld => 0b011,
                _ => unreachable!(),
            };
            let (imm, rs1) = offset_parts(&operands[1]);
            i_type(0b0000011, funct3, reg_num(&operands[0]), rs1, imm)
        }
        Sb | Sh | Sw | Sd => {
            let funct3 = match kind {
                Sb => 0b000,
                Sh => 0b001,
                Sw => 0b010,
                Sd => 0b011,
                _ => unreachable!(),
            };
            let (imm, rs1) = offset_parts(&operands[1]);
            s_type(0b0100011, funct3, rs1, reg_num(&operands[0]), imm)
        }
        Addi | Slti | Sltiu | Xori | Ori | Andi => {
            let funct3 = match kind {
                Addi => 0b000,
                Slti => 0b010,
                Sltiu => 0b011,
                Xori => 0b100,
                Ori => 0b110,
                Andi => 0b111,
                _ => unreachable!(),
            };
            i_type(0b0010011, funct3, reg_num(&operands[0]), reg_num(&operands[1]), imm_of(&operands[2]))
        }
        Slli | Srli | Srai => {
            let shamt = imm_of(&operands[2]) & 0x3F;
            let funct7 = if kind == Srai { 0b0100000 } else { 0b0000000 };
            let imm = (funct7 << 5) | shamt;
            let funct3 = if kind == Slli { 0b001 } else { 0b101 };
            i_type(0b0010011, funct3, reg_num(&operands[0]), reg_num(&operands[1]), imm)
        }
        Addiw => i_type(0b0011011, 0b000, reg_num(&operands[0]), reg_num(&operands[1]), imm_of(&operands[2])),
        Slliw | Srliw | Sraiw => {
            let shamt = imm_of(&operands[2]) & 0x1F;
            let funct7 = if kind == Sraiw { 0b0100000 } else { 0b0000000 };
            let imm = (funct7 << 5) | shamt;
            let funct3 = if kind == Slliw { 0b001 } else { 0b101 };
            i_type(0b0011011, funct3, reg_num(&operands[0]), reg_num(&operands[1]), imm)
        }
        Add | Sub | Sll | Slt | Sltu | Xor | Srl | Sra | Or | And => {
            let (funct3, funct7) = match kind {
                Add => (0b000, 0b0000000),
                Sub => (0b000, 0b0100000),
                Sll => (0b001, 0b0000000),
                Slt => (0b010, 0b0000000),
                Sltu => (0b011, 0b0000000),
                Xor => (0b100, 0b0000000),
                Srl => (0b101, 0b0000000),
                Sra => (0b101, 0b0100000),
                Or => (0b110, 0b0000000),
                And => (0b111, 0b0000000),
                _ => unreachable!(),
            };
            r_type(0b0110011, funct3, funct7, reg_num(&operands[0]), reg_num(&operands[1]), reg_num(&operands[2]))
        }
        Addw | Subw | Sllw | Srlw | Sraw => {
            let (funct3, funct7) = match kind {
                Addw => (0b000, 0b0000000),
                Subw => (0b000, 0b0100000),
                Sllw => (0b001, 0b0000000),
                Srlw => (0b101, 0b0000000),
                Sraw => (0b101, 0b0100000),
                _ => unreachable!(),
            };
            r_type(0b0111011, funct3, funct7, reg_num(&operands[0]), reg_num(&operands[1]), reg_num(&operands[2]))
        }
        Mul | Mulh | Mulhsu | Mulhu | Div | Divu | Rem | Remu => {
            let funct3 = match kind {
                Mul => 0b000,
                Mulh => 0b001,
                Mulhsu => 0b010,
                Mulhu => 0b011,
                Div => 0b100,
                Divu => 0b101,
                Rem => 0b110,
                Remu => 0b111,
                _ => unreachable!(),
            };
            r_type(0b0110011, funct3, 0b0000001, reg_num(&operands[0]), reg_num(&operands[1]), reg_num(&operands[2]))
        }
        Fence => i_type(0b0001111, 0b000, 0, 0, 0b1111_1111_1111u16 as i32),
        Ecall => i_type(0b1110011, 0b000, 0, 0, 0b0000_0000_0000),
        Ebreak => i_type(0b1110011, 0b000, 0, 0, 0b0000_0000_0001),

        Flw | Fld | Flq => {
            let funct3 = match kind {
                Flw => 0b010,
                Fld => 0b011,
                Flq => 0b100,
                _ => unreachable!(),
            };
            let (imm, rs1) = offset_parts(&operands[1]);
            i_type(0b0000111, funct3, reg_num(&operands[0]), rs1, imm)
        }
        Fsw | Fsd | Fsq => {
            let funct3 = match kind {
                Fsw => 0b010,
                Fsd => 0b011,
                Fsq => 0b100,
                _ => unreachable!(),
            };
            let (imm, rs1) = offset_parts(&operands[1]);
            s_type(0b0100111, funct3, rs1, reg_num(&operands[0]), imm)
        }

        FmaddS | FmaddD | FmaddQ | FmsubS | FmsubD | FmsubQ | FnmsubS | FnmsubD | FnmsubQ | FnmaddS | FnmaddD
        | FnmaddQ => {
            let opcode = match kind {
                FmaddS | FmaddD | FmaddQ => 0b1000011,
                FmsubS | FmsubD | FmsubQ => 0b1000111,
                FnmsubS | FnmsubD | FnmsubQ => 0b1001011,
                _ => 0b1001111,
            };
            let fmt = fmt_of(kind);
            r4_type(opcode, fmt, reg_num(&operands[3]), reg_num(&operands[2]), reg_num(&operands[1]), 0b000, reg_num(&operands[0]))
        }

        FaddS | FaddD | FaddQ | FsubS | FsubD | FsubQ | FmulS | FmulD | FmulQ | FdivS | FdivD | FdivQ => {
            let funct5 = match kind {
                FaddS | FaddD | FaddQ => 0b00000,
                FsubS | FsubD | FsubQ => 0b00001,
                FmulS | FmulD | FmulQ => 0b00010,
                _ => 0b00011,
            };
            op_fp(fmt_of(kind), funct5, reg_num(&operands[2]), reg_num(&operands[1]), 0b000, reg_num(&operands[0]))
        }
        FsgnjS | FsgnjD | FsgnjQ | FsgnjnS | FsgnjnD | FsgnjnQ | FsgnjxS | FsgnjxD | FsgnjxQ => {
            let rm = match kind {
                FsgnjS | FsgnjD | FsgnjQ => 0b000,
                FsgnjnS | FsgnjnD | FsgnjnQ => 0b001,
                _ => 0b010,
            };
            op_fp(fmt_of(kind), 0b00100, reg_num(&operands[2]), reg_num(&operands[1]), rm, reg_num(&operands[0]))
        }
        FminS | FminD | FminQ | FmaxS | FmaxD | FmaxQ => {
            let rm = if matches!(kind, FminS | FminD | FminQ) { 0b000 } else { 0b001 };
            op_fp(fmt_of(kind), 0b00101, reg_num(&operands[2]), reg_num(&operands[1]), rm, reg_num(&operands[0]))
        }
        FcvtSD | FcvtDS | FcvtSQ | FcvtQS | FcvtDQ | FcvtQD => {
            let (fmt, source) = match kind {
                FcvtSD => (0b00, 0b00001),
                FcvtDS => (0b01, 0b00000),
                FcvtSQ => (0b00, 0b00011),
                FcvtQS => (0b11, 0b00000),
                FcvtDQ => (0b01, 0b00011),
                FcvtQD => (0b11, 0b00001),
                _ => unreachable!(),
            };
            op_fp(fmt, 0b01000, source, reg_num(&operands[1]), 0b000, reg_num(&operands[0]))
        }
        FsqrtS | FsqrtD | FsqrtQ => {
            op_fp(fmt_of(kind), 0b01011, 0b00000, reg_num(&operands[1]), 0b000, reg_num(&operands[0]))
        }
        FleS | FleD | FleQ | FltS | FltD | FltQ | FeqS | FeqD | FeqQ => {
            let rm = match kind {
                FleS | FleD | FleQ => 0b000,
                FltS | FltD | FltQ => 0b001,
                _ => 0b010,
            };
            op_fp(fmt_of(kind), 0b10100, reg_num(&operands[2]), reg_num(&operands[1]), rm, reg_num(&operands[0]))
        }
        FclassS | FclassD | FclassQ => {
            op_fp(fmt_of(kind), 0b11100, 0b00000, reg_num(&operands[1]), 0b001, reg_num(&operands[0]))
        }
        FmvXW => op_fp(0b00, 0b11100, 0b00000, reg_num(&operands[1]), 0b000, reg_num(&operands[0])),
        FcvtWS | FcvtWuS | FcvtWD | FcvtWuD | FcvtWQ | FcvtWuQ => {
            let rs2 = if matches!(kind, FcvtWS | FcvtWD | FcvtWQ) { 0b00000 } else { 0b00001 };
            op_fp(fmt_of(kind), 0b11000, rs2, reg_num(&operands[1]), 0b000, reg_num(&operands[0]))
        }
        FcvtSW | FcvtSWu | FcvtDW | FcvtDWu | FcvtQW | FcvtQWu => {
            let rs2 = if matches!(kind, FcvtSW | FcvtDW | FcvtQW) { 0b00000 } else { 0b00001 };
            op_fp(fmt_of(kind), 0b11010, rs2, reg_num(&operands[1]), 0b000, reg_num(&operands[0]))
        }
        FmvWX => op_fp(0b00, 0b11110, 0b00000, reg_num(&operands[1]), 0b000, reg_num(&operands[0])),

        other => unreachable!("{:?} is a pseudo-instruction and should have been expanded before encoding", other),
    }
}

/// Works out the precision code used inside the encoded floating point instruction word, based on the instruction name.
fn fmt_of(kind: InstructionKind) -> u32 {
    use InstructionKind::*;
    match kind {
        FaddS | FsubS | FmulS | FdivS | FsgnjS | FsgnjnS | FsgnjxS | FminS | FmaxS | FsqrtS | FleS | FltS | FeqS
        | FclassS | FmaddS | FmsubS | FnmsubS | FnmaddS | FcvtWS | FcvtWuS | FcvtSW | FcvtSWu => 0b00,
        FaddD | FsubD | FmulD | FdivD | FsgnjD | FsgnjnD | FsgnjxD | FminD | FmaxD | FsqrtD | FleD | FltD | FeqD
        | FclassD | FmaddD | FmsubD | FnmsubD | FnmaddD | FcvtWD | FcvtWuD | FcvtDW | FcvtDWu => 0b01,
        FaddQ | FsubQ | FmulQ | FdivQ | FsgnjQ | FsgnjnQ | FsgnjxQ | FminQ | FmaxQ | FsqrtQ | FleQ | FltQ | FeqQ
        | FclassQ | FmaddQ | FmsubQ | FnmsubQ | FnmaddQ | FcvtWQ | FcvtWuQ | FcvtQW | FcvtQWu => 0b11,
        other => unreachable!("{:?} has no single precision format (cross-precision or non-fp)", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::RegisterKind;

    fn x(n: u8) -> Operand {
        Operand::Register(RegisterKind::X(n))
    }

    fn f(n: u8) -> Operand {
        Operand::Register(RegisterKind::F(n))
    }

    #[test]
    fn encodes_add_x1_x2_x3() {
        // Working out the expected bit pattern for "add x1, x2, x3" by hand,
        // which comes out to 0x003100B3.
        let ops = vec![x(1), x(2), x(3)];
        assert_eq!(encode(InstructionKind::Add, &ops), 0x003100B3);
    }

    #[test]
    fn encodes_addi_x1_x0_5() {
        // Working out the expected bit pattern for "addi x1, x0, 5" by hand,
        // which comes out to 0x00500093.
        let ops = vec![x(1), x(0), Operand::Immediate(5)];
        assert_eq!(encode(InstructionKind::Addi, &ops), 0x00500093);
    }

    #[test]
    fn encodes_sub_x1_x2_x3() {
        let ops = vec![x(1), x(2), x(3)];
        // "sub" uses a different bit pattern for its operation code than "add"
        assert_eq!(encode(InstructionKind::Sub, &ops), 0x403100B3);
    }

    #[test]
    fn encodes_lw_offset() {
        let ops = vec![x(5), Operand::Offset { imm: 4, base: RegisterKind::X(2) }];
        // Expected bit pattern for "lw x5, 4(x2)"
        assert_eq!(encode(InstructionKind::Lw, &ops), 0x00412283);
    }

    #[test]
    fn encodes_sw_offset() {
        let ops = vec![x(5), Operand::Offset { imm: 4, base: RegisterKind::X(2) }];
        // Expected bit pattern for "sw x5, 4(x2)"
        assert_eq!(encode(InstructionKind::Sw, &ops), 0x00512223);
    }

    #[test]
    fn encodes_beq() {
        let ops = vec![x(1), x(2), Operand::Immediate(8)];
        // Expected bit pattern for "beq x1, x2, 8"
        assert_eq!(encode(InstructionKind::Beq, &ops), 0x00208463);
    }

    #[test]
    fn encodes_jal() {
        let ops = vec![x(1), Operand::Immediate(16)];
        // Expected bit pattern for "jal x1, 16"
        assert_eq!(encode(InstructionKind::Jal, &ops), 0x010000EF);
    }

    #[test]
    fn encodes_lui() {
        let ops = vec![x(1), Operand::Immediate(1)];
        assert_eq!(encode(InstructionKind::Lui, &ops), 0x000010B7);
    }

    #[test]
    fn encodes_mul() {
        let ops = vec![x(1), x(2), x(3)];
        // "mul" uses a different bit pattern for its operation code than "add"
        assert_eq!(encode(InstructionKind::Mul, &ops), 0x023100B3);
    }

    #[test]
    fn encodes_addw() {
        let ops = vec![x(1), x(2), x(3)];
        assert_eq!(encode(InstructionKind::Addw, &ops), 0x003100BB);
    }

    #[test]
    fn encodes_slli_rv32() {
        let ops = vec![x(1), x(2), Operand::Immediate(3)];
        assert_eq!(encode(InstructionKind::Slli, &ops), 0x00311093);
    }

    #[test]
    fn encodes_ecall_and_ebreak() {
        assert_eq!(encode(InstructionKind::Ecall, &[]), 0x00000073);
        assert_eq!(encode(InstructionKind::Ebreak, &[]), 0x00100073);
    }

    #[test]
    fn encodes_fadd_s_f1_f2_f3() {
        // Working out the expected bit pattern for "fadd.s f1, f2, f3" by
        // hand, which comes out to 0x003100D3.
        let ops = vec![f(1), f(2), f(3)];
        assert_eq!(encode(InstructionKind::FaddS, &ops), 0x003100D3);
    }

    #[test]
    fn encodes_fsqrt_s() {
        // Checks the exact bit values this instruction family should produce.
        let ops = vec![f(1), f(2)];
        assert_eq!(encode(InstructionKind::FsqrtS, &ops), op_fp(0b00, 0b01011, 0, 2, 0b000, 1));
    }

    #[test]
    fn encodes_feq_s() {
        // The specific bit code chosen here picks the "equal" comparison out
        // of this family of compare instructions.
        let ops = vec![x(1), f(2), f(3)];
        assert_eq!(encode(InstructionKind::FeqS, &ops), op_fp(0b00, 0b10100, 3, 2, 0b010, 1));
    }

    #[test]
    fn encodes_fclass_s() {
        // Shares the same base code as FMV.X.W but is told apart by a
        // different mode bit.
        let ops = vec![x(1), f(2)];
        assert_eq!(encode(InstructionKind::FclassS, &ops), op_fp(0b00, 0b11100, 0, 2, 0b001, 1));
    }

    #[test]
    fn encodes_fcvt_w_s() {
        // The chosen bit code selects converting to a signed integer, a different code would select unsigned.
        let ops = vec![x(1), f(2)];
        assert_eq!(encode(InstructionKind::FcvtWS, &ops), op_fp(0b00, 0b11000, 0, 2, 0b000, 1));
    }

    #[test]
    fn encodes_fcvt_s_w() {
        // The chosen bit code selects converting from a signed integer.
        let ops = vec![f(1), x(2)];
        assert_eq!(encode(InstructionKind::FcvtSW, &ops), op_fp(0b00, 0b11010, 0, 2, 0b000, 1));
    }

    #[test]
    fn encodes_fmv_x_w() {
        // Shares the same base code as FCLASS.S but is told apart by a
        // different mode bit.
        let ops = vec![x(1), f(2)];
        assert_eq!(encode(InstructionKind::FmvXW, &ops), op_fp(0b00, 0b11100, 0, 2, 0b000, 1));
    }

    #[test]
    fn encodes_fmv_w_x() {
        let ops = vec![f(1), x(2)];
        assert_eq!(encode(InstructionKind::FmvWX, &ops), op_fp(0b00, 0b11110, 0, 2, 0b000, 1));
    }

    #[test]
    fn encodes_fmadd_s() {
        // Checks the fields land in the right spots for a four-operand
        // floating-point instruction like "fmadd.s".
        let ops = vec![f(1), f(2), f(3), f(4)];
        assert_eq!(encode(InstructionKind::FmaddS, &ops), r4_type(0b1000011, 0b00, 4, 3, 2, 0b000, 1));
    }

    #[test]
    fn encodes_fadd_d_and_fadd_q() {
        let ops = vec![f(1), f(2), f(3)];
        assert_ne!(encode(InstructionKind::FaddD, &ops), encode(InstructionKind::FaddS, &ops));
        assert_ne!(encode(InstructionKind::FaddQ, &ops), encode(InstructionKind::FaddD, &ops));
    }

    #[test]
    fn encodes_fcvt_s_d() {
        let ops = vec![f(1), f(2)];
        let word = encode(InstructionKind::FcvtSD, &ops);
        // Converting from double to single precision, target format is single, source format is double.
        let expected = op_fp(0b00, 0b01000, 0b00001, 2, 0b000, 1);
        assert_eq!(word, expected);
    }
}
