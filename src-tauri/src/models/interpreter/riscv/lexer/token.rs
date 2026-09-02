use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Identifier,
    Directive,
    Number,
    StringLiteral,
    Comma,
    Colon,
    LParen,
    RParen,
    Newline,
    Minus,
    Plus,
    Instruction(InstructionKind),
    Register(RegisterKind),
}

/// All the RISC-V instruction names this app recognizes, grouped by which
/// base instruction set or extension defines them. Every name here is
/// recognized no matter which extensions are actually turned on for the
/// current program; checking whether an instruction is actually allowed
/// happens in a later step, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionKind {
    // Base 32-bit instructions
    Lui,
    Auipc,
    Jal,
    Jalr,
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Lb,
    Lh,
    Lw,
    Lbu,
    Lhu,
    Sb,
    Sh,
    Sw,
    Addi,
    Slti,
    Sltiu,
    Xori,
    Ori,
    Andi,
    Slli,
    Srli,
    Srai,
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Fence,
    Ecall,
    Ebreak,

    // Extra instructions added for 64-bit support
    Lwu,
    Ld,
    Sd,
    Addiw,
    Slliw,
    Srliw,
    Sraiw,
    Addw,
    Subw,
    Sllw,
    Srlw,
    Sraw,

    // Multiplication and division instructions
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,

    // Single-precision floating point instructions
    Flw,
    Fsw,
    FmaddS,
    FmsubS,
    FnmsubS,
    FnmaddS,
    FaddS,
    FsubS,
    FmulS,
    FdivS,
    FsqrtS,
    FsgnjS,
    FsgnjnS,
    FsgnjxS,
    FminS,
    FmaxS,
    FcvtWS,
    FcvtWuS,
    FmvXW,
    FeqS,
    FltS,
    FleS,
    FclassS,
    FcvtSW,
    FcvtSWu,
    FmvWX,

    // Double-precision floating point instructions
    Fld,
    Fsd,
    FmaddD,
    FmsubD,
    FnmsubD,
    FnmaddD,
    FaddD,
    FsubD,
    FmulD,
    FdivD,
    FsqrtD,
    FsgnjD,
    FsgnjnD,
    FsgnjxD,
    FminD,
    FmaxD,
    FcvtSD,
    FcvtDS,
    FeqD,
    FltD,
    FleD,
    FclassD,
    FcvtWD,
    FcvtWuD,
    FcvtDW,
    FcvtDWu,

    // Quad-precision floating point instructions
    Flq,
    Fsq,
    FmaddQ,
    FmsubQ,
    FnmsubQ,
    FnmaddQ,
    FaddQ,
    FsubQ,
    FmulQ,
    FdivQ,
    FsqrtQ,
    FsgnjQ,
    FsgnjnQ,
    FsgnjxQ,
    FminQ,
    FmaxQ,
    FcvtSQ,
    FcvtQS,
    FcvtDQ,
    FcvtQD,
    FeqQ,
    FltQ,
    FleQ,
    FclassQ,
    FcvtWQ,
    FcvtWuQ,
    FcvtQW,
    FcvtQWu,

    // Shorthand instructions that stand in for a real instruction
    Nop,
    Li,
    Mv,
    Not,
    Neg,
    Seqz,
    Snez,
    Sltz,
    Sgtz,
    Beqz,
    Bnez,
    Blez,
    Bgez,
    Bltz,
    Bgtz,
    Bgt,
    Ble,
    Bgtu,
    Bleu,
    J,
    Jr,
    Ret,
    Call,
    Tail,
    La,
}

/// Integer (`x0`-`x31`) and floating-point (`f0`-`f31`) registers. Common
/// name aliases (`sp`, `a0`, `fa0`, ...) point to the same register as their
/// `xN`/`fN` form. The exact spelling the user typed is kept separately in
/// `Token::lexeme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterKind {
    X(u8),
    F(u8),
}

/// Looks up a word as a RISC-V instruction name (must be lowercase, as
/// assembly code expects). Returns `None` if the word isn't a known
/// instruction name.
pub fn lookup_instruction(word: &str) -> Option<InstructionKind> {
    use InstructionKind::*;
    Some(match word {
        // Base 32-bit instructions
        "lui" => Lui,
        "auipc" => Auipc,
        "jal" => Jal,
        "jalr" => Jalr,
        "beq" => Beq,
        "bne" => Bne,
        "blt" => Blt,
        "bge" => Bge,
        "bltu" => Bltu,
        "bgeu" => Bgeu,
        "lb" => Lb,
        "lh" => Lh,
        "lw" => Lw,
        "lbu" => Lbu,
        "lhu" => Lhu,
        "sb" => Sb,
        "sh" => Sh,
        "sw" => Sw,
        "addi" => Addi,
        "slti" => Slti,
        "sltiu" => Sltiu,
        "xori" => Xori,
        "ori" => Ori,
        "andi" => Andi,
        "slli" => Slli,
        "srli" => Srli,
        "srai" => Srai,
        "add" => Add,
        "sub" => Sub,
        "sll" => Sll,
        "slt" => Slt,
        "sltu" => Sltu,
        "xor" => Xor,
        "srl" => Srl,
        "sra" => Sra,
        "or" => Or,
        "and" => And,
        "fence" => Fence,
        "ecall" => Ecall,
        "ebreak" => Ebreak,

        // Extra instructions added for 64-bit support
        "lwu" => Lwu,
        "ld" => Ld,
        "sd" => Sd,
        "addiw" => Addiw,
        "slliw" => Slliw,
        "srliw" => Srliw,
        "sraiw" => Sraiw,
        "addw" => Addw,
        "subw" => Subw,
        "sllw" => Sllw,
        "srlw" => Srlw,
        "sraw" => Sraw,

        // Multiplication and division instructions
        "mul" => Mul,
        "mulh" => Mulh,
        "mulhsu" => Mulhsu,
        "mulhu" => Mulhu,
        "div" => Div,
        "divu" => Divu,
        "rem" => Rem,
        "remu" => Remu,

        // Single-precision floating point instructions
        "flw" => Flw,
        "fsw" => Fsw,
        "fmadd.s" => FmaddS,
        "fmsub.s" => FmsubS,
        "fnmsub.s" => FnmsubS,
        "fnmadd.s" => FnmaddS,
        "fadd.s" => FaddS,
        "fsub.s" => FsubS,
        "fmul.s" => FmulS,
        "fdiv.s" => FdivS,
        "fsqrt.s" => FsqrtS,
        "fsgnj.s" => FsgnjS,
        "fsgnjn.s" => FsgnjnS,
        "fsgnjx.s" => FsgnjxS,
        "fmin.s" => FminS,
        "fmax.s" => FmaxS,
        "fcvt.w.s" => FcvtWS,
        "fcvt.wu.s" => FcvtWuS,
        "fmv.x.w" => FmvXW,
        "feq.s" => FeqS,
        "flt.s" => FltS,
        "fle.s" => FleS,
        "fclass.s" => FclassS,
        "fcvt.s.w" => FcvtSW,
        "fcvt.s.wu" => FcvtSWu,
        "fmv.w.x" => FmvWX,

        // Double-precision floating point instructions
        "fld" => Fld,
        "fsd" => Fsd,
        "fmadd.d" => FmaddD,
        "fmsub.d" => FmsubD,
        "fnmsub.d" => FnmsubD,
        "fnmadd.d" => FnmaddD,
        "fadd.d" => FaddD,
        "fsub.d" => FsubD,
        "fmul.d" => FmulD,
        "fdiv.d" => FdivD,
        "fsqrt.d" => FsqrtD,
        "fsgnj.d" => FsgnjD,
        "fsgnjn.d" => FsgnjnD,
        "fsgnjx.d" => FsgnjxD,
        "fmin.d" => FminD,
        "fmax.d" => FmaxD,
        "fcvt.s.d" => FcvtSD,
        "fcvt.d.s" => FcvtDS,
        "feq.d" => FeqD,
        "flt.d" => FltD,
        "fle.d" => FleD,
        "fclass.d" => FclassD,
        "fcvt.w.d" => FcvtWD,
        "fcvt.wu.d" => FcvtWuD,
        "fcvt.d.w" => FcvtDW,
        "fcvt.d.wu" => FcvtDWu,

        // Quad-precision floating point instructions
        "flq" => Flq,
        "fsq" => Fsq,
        "fmadd.q" => FmaddQ,
        "fmsub.q" => FmsubQ,
        "fnmsub.q" => FnmsubQ,
        "fnmadd.q" => FnmaddQ,
        "fadd.q" => FaddQ,
        "fsub.q" => FsubQ,
        "fmul.q" => FmulQ,
        "fdiv.q" => FdivQ,
        "fsqrt.q" => FsqrtQ,
        "fsgnj.q" => FsgnjQ,
        "fsgnjn.q" => FsgnjnQ,
        "fsgnjx.q" => FsgnjxQ,
        "fmin.q" => FminQ,
        "fmax.q" => FmaxQ,
        "fcvt.s.q" => FcvtSQ,
        "fcvt.q.s" => FcvtQS,
        "fcvt.d.q" => FcvtDQ,
        "fcvt.q.d" => FcvtQD,
        "feq.q" => FeqQ,
        "flt.q" => FltQ,
        "fle.q" => FleQ,
        "fclass.q" => FclassQ,
        "fcvt.w.q" => FcvtWQ,
        "fcvt.wu.q" => FcvtWuQ,
        "fcvt.q.w" => FcvtQW,
        "fcvt.q.wu" => FcvtQWu,

        // Shorthand instructions that stand in for a real instruction
        "nop" => Nop,
        "li" => Li,
        "mv" => Mv,
        "not" => Not,
        "neg" => Neg,
        "seqz" => Seqz,
        "snez" => Snez,
        "sltz" => Sltz,
        "sgtz" => Sgtz,
        "beqz" => Beqz,
        "bnez" => Bnez,
        "blez" => Blez,
        "bgez" => Bgez,
        "bltz" => Bltz,
        "bgtz" => Bgtz,
        "bgt" => Bgt,
        "ble" => Ble,
        "bgtu" => Bgtu,
        "bleu" => Bleu,
        "j" => J,
        "jr" => Jr,
        "ret" => Ret,
        "call" => Call,
        "tail" => Tail,
        "la" => La,

        _ => return None,
    })
}

/// Looks up a word as an integer or floating-point register name, either
/// the numbered form (`x5`, `f12`) or a common alias (`sp`, `a0`, `fa0`, ...).
/// Returns `None` if the word isn't a recognized register name.
pub fn lookup_register(word: &str) -> Option<RegisterKind> {
    use RegisterKind::*;

    if let Some(n) = word.strip_prefix('x').and_then(|n| n.parse::<u8>().ok()) {
        if n <= 31 {
            return Some(X(n));
        }
    }
    if let Some(n) = word.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        if n <= 31 {
            return Some(F(n));
        }
    }

    Some(match word {
        "zero" => X(0),
        "ra" => X(1),
        "sp" => X(2),
        "gp" => X(3),
        "tp" => X(4),
        "t0" => X(5),
        "t1" => X(6),
        "t2" => X(7),
        "s0" | "fp" => X(8),
        "s1" => X(9),
        "a0" => X(10),
        "a1" => X(11),
        "a2" => X(12),
        "a3" => X(13),
        "a4" => X(14),
        "a5" => X(15),
        "a6" => X(16),
        "a7" => X(17),
        "s2" => X(18),
        "s3" => X(19),
        "s4" => X(20),
        "s5" => X(21),
        "s6" => X(22),
        "s7" => X(23),
        "s8" => X(24),
        "s9" => X(25),
        "s10" => X(26),
        "s11" => X(27),
        "t3" => X(28),
        "t4" => X(29),
        "t5" => X(30),
        "t6" => X(31),

        "ft0" => F(0),
        "ft1" => F(1),
        "ft2" => F(2),
        "ft3" => F(3),
        "ft4" => F(4),
        "ft5" => F(5),
        "ft6" => F(6),
        "ft7" => F(7),
        "fs0" => F(8),
        "fs1" => F(9),
        "fa0" => F(10),
        "fa1" => F(11),
        "fa2" => F(12),
        "fa3" => F(13),
        "fa4" => F(14),
        "fa5" => F(15),
        "fa6" => F(16),
        "fa7" => F(17),
        "fs2" => F(18),
        "fs3" => F(19),
        "fs4" => F(20),
        "fs5" => F(21),
        "fs6" => F(22),
        "fs7" => F(23),
        "fs8" => F(24),
        "fs9" => F(25),
        "fs10" => F(26),
        "fs11" => F(27),
        "ft8" => F(28),
        "ft9" => F(29),
        "ft10" => F(30),
        "ft11" => F(31),

        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, lexeme: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            kind,
            lexeme: lexeme.into(),
            line,
            column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_expected_json_line() {
        let token = Token::new(TokenKind::Identifier, "add", 1, 1);
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"identifier","lexeme":"add","line":1,"column":1}"#
        );
    }

    #[test]
    fn looks_up_rv32i_instruction() {
        assert_eq!(lookup_instruction("add"), Some(InstructionKind::Add));
        assert_eq!(lookup_instruction("beq"), Some(InstructionKind::Beq));
    }

    #[test]
    fn looks_up_extension_instructions() {
        assert_eq!(lookup_instruction("mul"), Some(InstructionKind::Mul));
        assert_eq!(lookup_instruction("fadd.s"), Some(InstructionKind::FaddS));
        assert_eq!(lookup_instruction("fld"), Some(InstructionKind::Fld));
        assert_eq!(lookup_instruction("flq"), Some(InstructionKind::Flq));
        assert_eq!(lookup_instruction("addw"), Some(InstructionKind::Addw));
    }

    #[test]
    fn looks_up_pseudo_instructions() {
        assert_eq!(lookup_instruction("li"), Some(InstructionKind::Li));
        assert_eq!(lookup_instruction("ret"), Some(InstructionKind::Ret));
    }

    #[test]
    fn unknown_word_is_not_an_instruction() {
        assert_eq!(lookup_instruction("loop"), None);
    }

    #[test]
    fn looks_up_numbered_and_abi_registers() {
        assert_eq!(lookup_register("x0"), Some(RegisterKind::X(0)));
        assert_eq!(lookup_register("x31"), Some(RegisterKind::X(31)));
        assert_eq!(lookup_register("sp"), Some(RegisterKind::X(2)));
        assert_eq!(lookup_register("a0"), Some(RegisterKind::X(10)));
        assert_eq!(lookup_register("fp"), Some(RegisterKind::X(8)));
        assert_eq!(lookup_register("f12"), Some(RegisterKind::F(12)));
        assert_eq!(lookup_register("fa0"), Some(RegisterKind::F(10)));
    }

    #[test]
    fn out_of_range_numbered_register_is_not_a_register() {
        assert_eq!(lookup_register("x32"), None);
        assert_eq!(lookup_register("f99"), None);
    }

    #[test]
    fn unknown_word_is_not_a_register() {
        assert_eq!(lookup_register("loop"), None);
    }
}
