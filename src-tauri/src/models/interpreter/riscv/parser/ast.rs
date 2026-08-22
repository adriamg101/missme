use serde::{Deserialize, Serialize};

use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operand {
    Register(RegisterKind),
    Immediate(i64),
    Offset { imm: i64, base: RegisterKind },
    LabelRef(String),
    StringLiteral(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Label {
        name: String,
        line: usize,
        column: usize,
    },
    Instruction {
        kind: InstructionKind,
        operands: Vec<Operand>,
        line: usize,
        column: usize,
    },
    Directive {
        name: String,
        args: Vec<Operand>,
        line: usize,
        column: usize,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub nodes: Vec<Node>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_round_trips_through_json() {
        let program = Program {
            nodes: vec![
                Node::Label { name: "loop".into(), line: 1, column: 1 },
                Node::Instruction {
                    kind: InstructionKind::Add,
                    operands: vec![
                        Operand::Register(RegisterKind::X(1)),
                        Operand::Register(RegisterKind::X(2)),
                        Operand::Register(RegisterKind::X(3)),
                    ],
                    line: 2,
                    column: 1,
                },
            ],
        };

        let json = serde_json::to_string(&program).unwrap();
        let back: Program = serde_json::from_str(&json).unwrap();

        assert_eq!(back, program);
    }
}
