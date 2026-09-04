use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
use crate::models::interpreter::riscv::parser::ast::{Node, Operand};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ExpandedInstruction {
    pub kind: InstructionKind,
    pub operands: Vec<Operand>,
    pub line: usize,
    pub column: usize,
}

pub(super) fn expand(node: &Node) -> Vec<ExpandedInstruction> {
    let (kind, operands, line, column) = match node {
        Node::Instruction { kind, operands, line, column } => (*kind, operands, *line, *column),
        _ => unreachable!("expand() is only called on Instruction nodes"),
    };

    use InstructionKind::*;
    let real = |k: InstructionKind, ops: Vec<Operand>| ExpandedInstruction { kind: k, operands: ops, line, column };
    let x0 = Operand::Register(RegisterKind::X(0));
    let x1 = Operand::Register(RegisterKind::X(1));

    match kind {
        Nop => vec![real(Addi, vec![x0.clone(), x0, Operand::Immediate(0)])],
        Li => {
            let rd = operands[0].clone();
            let imm = match &operands[1] {
                Operand::Immediate(v) => *v,
                _ => unreachable!("semantic analysis already validated li's operand shape"),
            };
            if (-2048..=2047).contains(&imm) {
                vec![real(Addi, vec![rd, x0, Operand::Immediate(imm)])]
            } else {
                let lo = (imm & 0xFFF) as i32;
                let lo_signed = if lo >= 2048 { lo - 4096 } else { lo };
                let hi = (imm - lo_signed as i64) >> 12;
                vec![
                    real(Lui, vec![rd.clone(), Operand::Immediate(hi)]),
                    real(Addi, vec![rd.clone(), rd, Operand::Immediate(lo_signed as i64)]),
                ]
            }
        }
        Mv => vec![real(Addi, vec![operands[0].clone(), operands[1].clone(), Operand::Immediate(0)])],
        Not => vec![real(Xori, vec![operands[0].clone(), operands[1].clone(), Operand::Immediate(-1)])],
        Neg => vec![real(Sub, vec![operands[0].clone(), x0, operands[1].clone()])],
        Seqz => vec![real(Sltiu, vec![operands[0].clone(), operands[1].clone(), Operand::Immediate(1)])],
        Snez => vec![real(Sltu, vec![operands[0].clone(), x0, operands[1].clone()])],
        Sltz => vec![real(Slt, vec![operands[0].clone(), operands[1].clone(), x0])],
        Sgtz => vec![real(Slt, vec![operands[0].clone(), x0, operands[1].clone()])],
        Beqz => vec![real(Beq, vec![operands[0].clone(), x0, operands[1].clone()])],
        Bnez => vec![real(Bne, vec![operands[0].clone(), x0, operands[1].clone()])],
        Blez => vec![real(Bge, vec![x0, operands[0].clone(), operands[1].clone()])],
        Bgez => vec![real(Bge, vec![operands[0].clone(), x0, operands[1].clone()])],
        Bltz => vec![real(Blt, vec![operands[0].clone(), x0, operands[1].clone()])],
        Bgtz => vec![real(Blt, vec![x0, operands[0].clone(), operands[1].clone()])],
        Bgt => vec![real(Blt, vec![operands[1].clone(), operands[0].clone(), operands[2].clone()])],
        Ble => vec![real(Bge, vec![operands[1].clone(), operands[0].clone(), operands[2].clone()])],
        Bgtu => vec![real(Bltu, vec![operands[1].clone(), operands[0].clone(), operands[2].clone()])],
        Bleu => vec![real(Bgeu, vec![operands[1].clone(), operands[0].clone(), operands[2].clone()])],
        J => vec![real(Jal, vec![x0, operands[0].clone()])],
        Jr => vec![real(Jalr, vec![x0, Operand::Offset { imm: 0, base: reg_of(&operands[0]) }])],
        Ret => vec![real(Jalr, vec![x0, Operand::Offset { imm: 0, base: RegisterKind::X(1) }])],
        Call => vec![real(Jal, vec![x1, operands[0].clone()])],
        Tail => vec![real(Jal, vec![x0, operands[0].clone()])],
        La => {
            let label = operands[1].clone();
            vec![
                real(Auipc, vec![operands[0].clone(), label.clone()]),
                real(Addi, vec![operands[0].clone(), operands[0].clone(), label]),
            ]
        }
        _ => vec![real(kind, operands.clone())],
    }
}

fn reg_of(op: &Operand) -> RegisterKind {
    match op {
        Operand::Register(r) => *r,
        _ => unreachable!("semantic analysis already validated this operand is a register"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::RegisterKind;

    fn node(kind: InstructionKind, operands: Vec<Operand>) -> Node {
        Node::Instruction { kind, operands, line: 1, column: 1 }
    }

    fn x(n: u8) -> Operand {
        Operand::Register(RegisterKind::X(n))
    }

    fn expanded(kind: InstructionKind, operands: Vec<Operand>) -> ExpandedInstruction {
        ExpandedInstruction { kind, operands, line: 1, column: 1 }
    }

    #[test]
    fn expands_nop() {
        let n = node(InstructionKind::Nop, vec![]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Addi, vec![x(0), x(0), Operand::Immediate(0)])]);
    }

    #[test]
    fn expands_li_within_12_bits_to_addi() {
        let n = node(InstructionKind::Li, vec![x(1), Operand::Immediate(100)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Addi, vec![x(1), x(0), Operand::Immediate(100)])]);
    }

    #[test]
    fn expands_li_beyond_12_bits_to_lui_addi() {
        let n = node(InstructionKind::Li, vec![x(1), Operand::Immediate(0x12345678)]);
        let result = expand(&n);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].kind, InstructionKind::Lui);
        assert_eq!(result[1].kind, InstructionKind::Addi);
        // Putting the two halves back together should give the original value.
        let hi = match result[0].operands[1] {
            Operand::Immediate(v) => v,
            _ => panic!("expected immediate"),
        };
        let lo = match result[1].operands[2] {
            Operand::Immediate(v) => v,
            _ => panic!("expected immediate"),
        };
        assert_eq!((hi << 12) + lo, 0x12345678);
    }

    #[test]
    fn expands_li_with_negative_lo12_adjusts_hi() {
        // For this value, the lower part comes out negative, so the upper
        // part needs to be adjusted up by one to compensate.
        let n = node(InstructionKind::Li, vec![x(1), Operand::Immediate(0x7FFFFFFF)]);
        let result = expand(&n);
        let hi = match result[0].operands[1] {
            Operand::Immediate(v) => v,
            _ => panic!("expected immediate"),
        };
        let lo = match result[1].operands[2] {
            Operand::Immediate(v) => v,
            _ => panic!("expected immediate"),
        };
        assert_eq!((hi << 12) + lo, 0x7FFFFFFF);
        assert_eq!(lo, -1);
    }

    #[test]
    fn expands_mv() {
        let n = node(InstructionKind::Mv, vec![x(1), x(2)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Addi, vec![x(1), x(2), Operand::Immediate(0)])]);
    }

    #[test]
    fn expands_not() {
        let n = node(InstructionKind::Not, vec![x(1), x(2)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Xori, vec![x(1), x(2), Operand::Immediate(-1)])]);
    }

    #[test]
    fn expands_neg() {
        let n = node(InstructionKind::Neg, vec![x(1), x(2)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Sub, vec![x(1), x(0), x(2)])]);
    }

    #[test]
    fn expands_seqz() {
        let n = node(InstructionKind::Seqz, vec![x(1), x(2)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Sltiu, vec![x(1), x(2), Operand::Immediate(1)])]);
    }

    #[test]
    fn expands_snez() {
        let n = node(InstructionKind::Snez, vec![x(1), x(2)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Sltu, vec![x(1), x(0), x(2)])]);
    }

    #[test]
    fn expands_sltz() {
        let n = node(InstructionKind::Sltz, vec![x(1), x(2)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Slt, vec![x(1), x(2), x(0)])]);
    }

    #[test]
    fn expands_sgtz() {
        let n = node(InstructionKind::Sgtz, vec![x(1), x(2)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Slt, vec![x(1), x(0), x(2)])]);
    }

    #[test]
    fn expands_beqz() {
        let n = node(InstructionKind::Beqz, vec![x(1), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Beq, vec![x(1), x(0), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_bnez() {
        let n = node(InstructionKind::Bnez, vec![x(1), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Bne, vec![x(1), x(0), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_blez() {
        let n = node(InstructionKind::Blez, vec![x(1), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Bge, vec![x(0), x(1), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_bgez() {
        let n = node(InstructionKind::Bgez, vec![x(1), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Bge, vec![x(1), x(0), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_bltz() {
        let n = node(InstructionKind::Bltz, vec![x(1), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Blt, vec![x(1), x(0), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_bgtz() {
        let n = node(InstructionKind::Bgtz, vec![x(1), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Blt, vec![x(0), x(1), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_bgt() {
        let n = node(InstructionKind::Bgt, vec![x(1), x(2), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Blt, vec![x(2), x(1), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_ble() {
        let n = node(InstructionKind::Ble, vec![x(1), x(2), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Bge, vec![x(2), x(1), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_bgtu() {
        let n = node(InstructionKind::Bgtu, vec![x(1), x(2), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Bltu, vec![x(2), x(1), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_bleu() {
        let n = node(InstructionKind::Bleu, vec![x(1), x(2), Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Bgeu, vec![x(2), x(1), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_j() {
        let n = node(InstructionKind::J, vec![Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Jal, vec![x(0), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_jr() {
        let n = node(InstructionKind::Jr, vec![x(1)]);
        assert_eq!(
            expand(&n),
            vec![expanded(InstructionKind::Jalr, vec![x(0), Operand::Offset { imm: 0, base: RegisterKind::X(1) }])]
        );
    }

    #[test]
    fn expands_ret() {
        let n = node(InstructionKind::Ret, vec![]);
        assert_eq!(
            expand(&n),
            vec![expanded(InstructionKind::Jalr, vec![x(0), Operand::Offset { imm: 0, base: RegisterKind::X(1) }])]
        );
    }

    #[test]
    fn expands_call() {
        let n = node(InstructionKind::Call, vec![Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Jal, vec![x(1), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_tail() {
        let n = node(InstructionKind::Tail, vec![Operand::LabelRef("l".into())]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Jal, vec![x(0), Operand::LabelRef("l".into())])]);
    }

    #[test]
    fn expands_la() {
        let n = node(InstructionKind::La, vec![x(1), Operand::LabelRef("values".into())]);
        let result = expand(&n);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].kind, InstructionKind::Auipc);
        assert_eq!(result[0].operands, vec![x(1), Operand::LabelRef("values".into())]);
        assert_eq!(result[1].kind, InstructionKind::Addi);
        assert_eq!(result[1].operands, vec![x(1), x(1), Operand::LabelRef("values".into())]);
    }

    #[test]
    fn real_instruction_passes_through_unchanged() {
        let n = node(InstructionKind::Add, vec![x(1), x(2), x(3)]);
        assert_eq!(expand(&n), vec![expanded(InstructionKind::Add, vec![x(1), x(2), x(3)])]);
    }
}
