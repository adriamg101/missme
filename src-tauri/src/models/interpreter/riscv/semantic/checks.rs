use crate::models::interpreter::error::SemanticError;
use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
use crate::models::interpreter::riscv::parser::ast::Operand;
use crate::models::interpreter::riscv::{Base, Extension, RiscvConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperandShape {
    RRR,
    RRImm(u32),
    RRShamt,
    RegOffset,
    Branch,
    Upper,
    Jal,
    NoOperand,
    Fence,
    FRRR,
    FRRRR,
    FRR,
    FCmp,
    IntFromFp,
    FpFromInt,
    MvReg,
    BranchZero,
    JLabel,
    JrReg,
    CallLabel,
    LaLabel,
    LiImm,
}

fn shape_of(kind: InstructionKind) -> OperandShape {
    use InstructionKind::*;
    use OperandShape::*;

    match kind {
        Lui | Auipc => Upper,
        InstructionKind::Jal => OperandShape::Jal,
        Jalr => RegOffset,
        Beq | Bne | Blt | Bge | Bltu | Bgeu => Branch,
        Lb | Lh | Lw | Lbu | Lhu | Lwu | Ld | Flw | Fld | Flq => RegOffset,
        Sb | Sh | Sw | Sd | Fsw | Fsd | Fsq => RegOffset,
        Addi | Slti | Sltiu | Xori | Ori | Andi | Addiw => RRImm(12),
        Slli | Srli | Srai | Slliw | Srliw | Sraiw => RRShamt,
        Add | Sub | Sll | Slt | Sltu | Xor | Srl | Sra | Or | And | Addw | Subw | Sllw | Srlw
        | Sraw | Mul | Mulh | Mulhsu | Mulhu | Div | Divu | Rem | Remu => RRR,
        InstructionKind::Fence => OperandShape::Fence,
        Ecall | Ebreak => NoOperand,

        FaddS | FsubS | FmulS | FdivS | FsgnjS | FsgnjnS | FsgnjxS | FminS | FmaxS
        | FaddD | FsubD | FmulD | FdivD | FsgnjD | FsgnjnD | FsgnjxD | FminD | FmaxD
        | FaddQ | FsubQ | FmulQ | FdivQ | FsgnjQ | FsgnjnQ | FsgnjxQ | FminQ | FmaxQ => FRRR,

        FmaddS | FmsubS | FnmsubS | FnmaddS | FmaddD | FmsubD | FnmsubD | FnmaddD | FmaddQ
        | FmsubQ | FnmsubQ | FnmaddQ => FRRRR,

        FsqrtS | FsqrtD | FsqrtQ | FcvtSD | FcvtDS | FcvtSQ | FcvtQS | FcvtDQ | FcvtQD => FRR,

        FeqS | FltS | FleS | FeqD | FltD | FleD | FeqQ | FltQ | FleQ => FCmp,

        FcvtWS | FcvtWuS | FmvXW | FclassS | FcvtWD | FcvtWuD | FclassD | FcvtWQ | FcvtWuQ
        | FclassQ => IntFromFp,

        FcvtSW | FcvtSWu | FmvWX | FcvtDW | FcvtDWu | FcvtQW | FcvtQWu => FpFromInt,

        Nop | Ret => NoOperand,
        Li => LiImm,
        Mv | Not | Neg | Seqz | Snez | Sltz | Sgtz => MvReg,
        Beqz | Bnez | Blez | Bgez | Bltz | Bgtz => BranchZero,
        Bgt | Ble | Bgtu | Bleu => Branch,
        J => JLabel,
        Jr => JrReg,
        Call | Tail => CallLabel,
        La => LaLabel,
    }
}

fn required_extension(kind: InstructionKind) -> Option<Extension> {
    use InstructionKind::*;

    match kind {
        Mul | Mulh | Mulhsu | Mulhu | Div | Divu | Rem | Remu => Some(Extension::M),
        Flw | Fsw | FmaddS | FmsubS | FnmsubS | FnmaddS | FaddS | FsubS | FmulS | FdivS
        | FsqrtS | FsgnjS | FsgnjnS | FsgnjxS | FminS | FmaxS | FcvtWS | FcvtWuS | FmvXW
        | FeqS | FltS | FleS | FclassS | FcvtSW | FcvtSWu | FmvWX => Some(Extension::F),
        Fld | Fsd | FmaddD | FmsubD | FnmsubD | FnmaddD | FaddD | FsubD | FmulD | FdivD
        | FsqrtD | FsgnjD | FsgnjnD | FsgnjxD | FminD | FmaxD | FcvtSD | FcvtDS | FeqD | FltD
        | FleD | FclassD | FcvtWD | FcvtWuD | FcvtDW | FcvtDWu => Some(Extension::D),
        Flq | Fsq | FmaddQ | FmsubQ | FnmsubQ | FnmaddQ | FaddQ | FsubQ | FmulQ | FdivQ
        | FsqrtQ | FsgnjQ | FsgnjnQ | FsgnjxQ | FminQ | FmaxQ | FcvtSQ | FcvtQS | FcvtDQ
        | FcvtQD | FeqQ | FltQ | FleQ | FclassQ | FcvtWQ | FcvtWuQ | FcvtQW | FcvtQWu => {
            Some(Extension::Q)
        }
        _ => None,
    }
}

fn requires_rv64(kind: InstructionKind) -> bool {
    use InstructionKind::*;
    matches!(kind, Lwu | Ld | Sd | Addiw | Slliw | Srliw | Sraiw | Addw | Subw | Sllw | Srlw | Sraw)
}

fn shamt_bits(kind: InstructionKind, config: &RiscvConfig) -> u32 {
    use InstructionKind::*;
    match kind {
        Slliw | Srliw | Sraiw => 5,
        _ => {
            if config.base == Base::Rv64i {
                6
            } else {
                5
            }
        }
    }
}

fn check_signed_range(value: i64, bits: u32, mnemonic: &str, line: usize, column: usize) -> Result<(), SemanticError> {
    let min = -(1i64 << (bits - 1));
    let max = (1i64 << (bits - 1)) - 1;
    if value < min || value > max {
        return Err(SemanticError::new(
            line,
            column,
            format!("'{}' immediate {} does not fit in {} signed bits", mnemonic, value, bits),
        ));
    }
    Ok(())
}

fn check_unsigned_range(value: i64, bits: u32, mnemonic: &str, line: usize, column: usize) -> Result<(), SemanticError> {
    let max = (1i64 << bits) - 1;
    if value < 0 || value > max {
        return Err(SemanticError::new(
            line,
            column,
            format!("'{}' immediate {} does not fit in {} unsigned bits", mnemonic, value, bits),
        ));
    }
    Ok(())
}

pub(super) fn check_config(
    kind: InstructionKind,
    config: &RiscvConfig,
    line: usize,
    column: usize,
) -> Result<(), SemanticError> {
    if requires_rv64(kind) && config.base != Base::Rv64i {
        return Err(SemanticError::new(line, column, format!("'{:?}' requires RV64I base", kind)));
    }
    if let Some(ext) = required_extension(kind) {
        if !config.extensions.contains(&ext) {
            return Err(SemanticError::new(
                line,
                column,
                format!("'{:?}' requires the {:?} extension", kind, ext),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_operands(
    kind: InstructionKind,
    operands: &[Operand],
    config: &RiscvConfig,
    line: usize,
    column: usize,
) -> Result<(), SemanticError> {
    let shape = shape_of(kind);
    let mnemonic = format!("{:?}", kind);

    let expect_count = |n: usize| -> Result<(), SemanticError> {
        if operands.len() != n {
            Err(SemanticError::new(
                line,
                column,
                format!("'{}' expects {} operand(s), found {}", mnemonic, n, operands.len()),
            ))
        } else {
            Ok(())
        }
    };

    let expect_register = |op: &Operand, is_fp: bool| -> Result<(), SemanticError> {
        match op {
            Operand::Register(RegisterKind::F(_)) if is_fp => Ok(()),
            Operand::Register(RegisterKind::X(_)) if !is_fp => Ok(()),
            _ => Err(SemanticError::new(
                line,
                column,
                format!(
                    "'{}' expects a {} register operand",
                    mnemonic,
                    if is_fp { "floating-point" } else { "integer" }
                ),
            )),
        }
    };

    match shape {
        OperandShape::RRR => {
            expect_count(3)?;
            expect_register(&operands[0], false)?;
            expect_register(&operands[1], false)?;
            expect_register(&operands[2], false)?;
        }
        OperandShape::RRImm(bits) => {
            expect_count(3)?;
            expect_register(&operands[0], false)?;
            expect_register(&operands[1], false)?;
            match &operands[2] {
                Operand::Immediate(value) => check_signed_range(*value, bits, &mnemonic, line, column)?,
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects an immediate operand", mnemonic))),
            }
        }
        OperandShape::RRShamt => {
            expect_count(3)?;
            expect_register(&operands[0], false)?;
            expect_register(&operands[1], false)?;
            let bits = shamt_bits(kind, config);
            match &operands[2] {
                Operand::Immediate(value) => check_unsigned_range(*value, bits, &mnemonic, line, column)?,
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects a shift amount", mnemonic))),
            }
        }
        OperandShape::RegOffset => {
            expect_count(2)?;
            let is_fp = matches!(
                kind,
                InstructionKind::Flw
                    | InstructionKind::Fld
                    | InstructionKind::Flq
                    | InstructionKind::Fsw
                    | InstructionKind::Fsd
                    | InstructionKind::Fsq
            );
            expect_register(&operands[0], is_fp)?;
            match &operands[1] {
                Operand::Offset { imm, .. } => check_signed_range(*imm, 12, &mnemonic, line, column)?,
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects an offset operand, e.g. 4(sp)", mnemonic))),
            }
        }
        OperandShape::Branch => {
            expect_count(3)?;
            expect_register(&operands[0], false)?;
            expect_register(&operands[1], false)?;
            match &operands[2] {
                Operand::LabelRef(_) => {}
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects a label operand", mnemonic))),
            }
        }
        OperandShape::Upper => {
            expect_count(2)?;
            expect_register(&operands[0], false)?;
            match &operands[1] {
                Operand::Immediate(value) => check_unsigned_range(*value, 20, &mnemonic, line, column)?,
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects an immediate operand", mnemonic))),
            }
        }
        OperandShape::Jal => {
            expect_count(2)?;
            expect_register(&operands[0], false)?;
            match &operands[1] {
                Operand::LabelRef(_) => {}
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects a label operand", mnemonic))),
            }
        }
        OperandShape::NoOperand => {
            expect_count(0)?;
        }
        OperandShape::Fence => {
            if !operands.is_empty() && operands.len() != 2 {
                return Err(SemanticError::new(line, column, "'fence' expects 0 or 2 operands".to_string()));
            }
            for op in operands {
                if !matches!(op, Operand::LabelRef(_)) {
                    return Err(SemanticError::new(line, column, "'fence' operands must be identifiers (e.g. iorw)".to_string()));
                }
            }
        }
        OperandShape::FRRR => {
            expect_count(3)?;
            expect_register(&operands[0], true)?;
            expect_register(&operands[1], true)?;
            expect_register(&operands[2], true)?;
        }
        OperandShape::FRRRR => {
            expect_count(4)?;
            for op in operands {
                expect_register(op, true)?;
            }
        }
        OperandShape::FRR => {
            expect_count(2)?;
            expect_register(&operands[0], true)?;
            expect_register(&operands[1], true)?;
        }
        OperandShape::FCmp => {
            expect_count(3)?;
            expect_register(&operands[0], false)?;
            expect_register(&operands[1], true)?;
            expect_register(&operands[2], true)?;
        }
        OperandShape::IntFromFp => {
            expect_count(2)?;
            expect_register(&operands[0], false)?;
            expect_register(&operands[1], true)?;
        }
        OperandShape::FpFromInt => {
            expect_count(2)?;
            expect_register(&operands[0], true)?;
            expect_register(&operands[1], false)?;
        }
        OperandShape::MvReg => {
            expect_count(2)?;
            expect_register(&operands[0], false)?;
            expect_register(&operands[1], false)?;
        }
        OperandShape::BranchZero => {
            expect_count(2)?;
            expect_register(&operands[0], false)?;
            match &operands[1] {
                Operand::LabelRef(_) => {}
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects a label operand", mnemonic))),
            }
        }
        OperandShape::JLabel | OperandShape::CallLabel => {
            expect_count(1)?;
            match &operands[0] {
                Operand::LabelRef(_) => {}
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects a label operand", mnemonic))),
            }
        }
        OperandShape::JrReg => {
            expect_count(1)?;
            expect_register(&operands[0], false)?;
        }
        OperandShape::LaLabel => {
            expect_count(2)?;
            expect_register(&operands[0], false)?;
            match &operands[1] {
                Operand::LabelRef(_) => {}
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects a label operand", mnemonic))),
            }
        }
        OperandShape::LiImm => {
            expect_count(2)?;
            expect_register(&operands[0], false)?;
            match &operands[1] {
                Operand::Immediate(_) => {}
                _ => return Err(SemanticError::new(line, column, format!("'{}' expects an immediate operand", mnemonic))),
            }
        }
    }

    Ok(())
}

pub(super) fn validate_directive(
    name: &str,
    args: &[Operand],
    line: usize,
    column: usize,
) -> Result<(), SemanticError> {
    match name {
        ".text" | ".data" | ".bss" => {
            if !args.is_empty() {
                return Err(SemanticError::new(line, column, format!("'{}' does not take arguments", name)));
            }
        }
        ".global" | ".globl" => {
            if args.len() != 1 || !matches!(args[0], Operand::LabelRef(_)) {
                return Err(SemanticError::new(line, column, format!("'{}' expects a single label argument", name)));
            }
        }
        ".word" | ".byte" | ".half" => {
            if args.is_empty() {
                return Err(SemanticError::new(line, column, format!("'{}' expects at least one value", name)));
            }
            for arg in args {
                if !matches!(arg, Operand::Immediate(_) | Operand::LabelRef(_)) {
                    return Err(SemanticError::new(line, column, format!("'{}' expects immediate or label values", name)));
                }
            }
        }
        ".string" | ".asciz" | ".ascii" => {
            if args.len() != 1 || !matches!(args[0], Operand::StringLiteral(_)) {
                return Err(SemanticError::new(line, column, format!("'{}' expects a single string argument", name)));
            }
        }
        _ => {
            return Err(SemanticError::new(line, column, format!("unknown directive '{}'", name)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn rv32i() -> RiscvConfig {
        RiscvConfig { base: Base::Rv32i, extensions: HashSet::new() }
    }

    fn rv64i() -> RiscvConfig {
        RiscvConfig { base: Base::Rv64i, extensions: HashSet::new() }
    }

    fn rv32i_with(ext: Extension) -> RiscvConfig {
        RiscvConfig { base: Base::Rv32i, extensions: HashSet::from([ext]) }
    }

    #[test]
    fn rrr_instruction_with_correct_operands_is_valid() {
        let operands = vec![
            Operand::Register(RegisterKind::X(1)),
            Operand::Register(RegisterKind::X(2)),
            Operand::Register(RegisterKind::X(3)),
        ];
        assert!(validate_operands(InstructionKind::Add, &operands, &rv32i(), 1, 1).is_ok());
    }

    #[test]
    fn rrr_instruction_with_wrong_operand_count_is_an_error() {
        let operands = vec![Operand::Register(RegisterKind::X(1))];
        let err = validate_operands(InstructionKind::Add, &operands, &rv32i(), 1, 1).unwrap_err();
        assert!(err.message.contains("expects 3 operand"));
    }

    #[test]
    fn rrimm_instruction_out_of_range_is_an_error() {
        let operands = vec![
            Operand::Register(RegisterKind::X(1)),
            Operand::Register(RegisterKind::X(2)),
            Operand::Immediate(4096),
        ];
        let err = validate_operands(InstructionKind::Addi, &operands, &rv32i(), 1, 1).unwrap_err();
        assert!(err.message.contains("does not fit in 12 signed bits"));
    }

    #[test]
    fn rrimm_instruction_in_range_is_valid() {
        let operands = vec![
            Operand::Register(RegisterKind::X(1)),
            Operand::Register(RegisterKind::X(2)),
            Operand::Immediate(-2048),
        ];
        assert!(validate_operands(InstructionKind::Addi, &operands, &rv32i(), 1, 1).is_ok());
    }

    #[test]
    fn shamt_width_depends_on_base_for_slli() {
        let operands_ok_on_rv64 = vec![
            Operand::Register(RegisterKind::X(1)),
            Operand::Register(RegisterKind::X(2)),
            Operand::Immediate(40),
        ];
        assert!(validate_operands(InstructionKind::Slli, &operands_ok_on_rv64, &rv64i(), 1, 1).is_ok());
        let err = validate_operands(InstructionKind::Slli, &operands_ok_on_rv64, &rv32i(), 1, 1).unwrap_err();
        assert!(err.message.contains("does not fit in 5 unsigned bits"));
    }

    #[test]
    fn load_instruction_expects_offset_operand() {
        let operands = vec![
            Operand::Register(RegisterKind::X(5)),
            Operand::Offset { imm: 4, base: RegisterKind::X(2) },
        ];
        assert!(validate_operands(InstructionKind::Lw, &operands, &rv32i(), 1, 1).is_ok());
    }

    #[test]
    fn branch_instruction_expects_label_operand() {
        let operands = vec![
            Operand::Register(RegisterKind::X(1)),
            Operand::Register(RegisterKind::X(2)),
            Operand::LabelRef("end".into()),
        ];
        assert!(validate_operands(InstructionKind::Beq, &operands, &rv32i(), 1, 1).is_ok());
    }

    #[test]
    fn fp_instruction_requires_float_registers() {
        let operands = vec![
            Operand::Register(RegisterKind::X(1)),
            Operand::Register(RegisterKind::F(2)),
            Operand::Register(RegisterKind::F(3)),
        ];
        let err = validate_operands(InstructionKind::FaddS, &operands, &rv32i_with(Extension::F), 1, 1).unwrap_err();
        assert!(err.message.contains("floating-point"));
    }

    #[test]
    fn no_operand_instruction_rejects_operands() {
        let operands = vec![Operand::Register(RegisterKind::X(1))];
        let err = validate_operands(InstructionKind::Ecall, &operands, &rv32i(), 1, 1).unwrap_err();
        assert!(err.message.contains("expects 0 operand"));
    }

    #[test]
    fn m_extension_instruction_needs_m_enabled() {
        let operands = vec![
            Operand::Register(RegisterKind::X(1)),
            Operand::Register(RegisterKind::X(2)),
            Operand::Register(RegisterKind::X(3)),
        ];
        assert!(validate_operands(InstructionKind::Mul, &operands, &rv32i_with(Extension::M), 1, 1).is_ok());

        let err = check_config(InstructionKind::Mul, &rv32i(), 1, 1).unwrap_err();
        assert!(err.message.contains("requires the M extension"));
    }

    #[test]
    fn rv64_only_instruction_needs_rv64_base() {
        let err = check_config(InstructionKind::Addiw, &rv32i(), 1, 1).unwrap_err();
        assert!(err.message.contains("requires RV64I base"));
        assert!(check_config(InstructionKind::Addiw, &rv64i(), 1, 1).is_ok());
    }

    #[test]
    fn base_instruction_needs_no_extension() {
        assert!(check_config(InstructionKind::Add, &rv32i(), 1, 1).is_ok());
    }

    #[test]
    fn word_directive_accepts_immediates() {
        let args = vec![Operand::Immediate(1), Operand::Immediate(2)];
        assert!(validate_directive(".word", &args, 1, 1).is_ok());
    }

    #[test]
    fn word_directive_rejects_empty_args() {
        let err = validate_directive(".word", &[], 1, 1).unwrap_err();
        assert!(err.message.contains("expects at least one value"));
    }

    #[test]
    fn string_directive_expects_a_single_string() {
        let args = vec![Operand::StringLiteral("hi".into())];
        assert!(validate_directive(".string", &args, 1, 1).is_ok());
        let err = validate_directive(".string", &[], 1, 1).unwrap_err();
        assert!(err.message.contains("expects a single string argument"));
    }

    #[test]
    fn unknown_directive_is_an_error() {
        let err = validate_directive(".unknown", &[], 1, 1).unwrap_err();
        assert!(err.message.contains("unknown directive"));
    }
}
