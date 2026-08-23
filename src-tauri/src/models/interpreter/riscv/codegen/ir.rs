use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::interpreter::riscv::lexer::token::InstructionKind;
use crate::models::interpreter::riscv::parser::ast::Operand;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrEntry {
    pub address: u32,
    pub kind: InstructionKind,
    pub operands: Vec<Operand>,
    pub bytecode: u32,
    pub source_line: usize,
    pub source_column: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryLayout {
    pub text_base: u32,
    pub data_base: u32,
    pub labels: HashMap<String, u32>,
    pub initial_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodegenOutput {
    pub layout: MemoryLayout,
    pub entries: Vec<IrEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::RegisterKind;

    #[test]
    fn codegen_output_round_trips_through_json() {
        let output = CodegenOutput {
            layout: MemoryLayout {
                text_base: 0x00400000,
                data_base: 0x10000000,
                labels: HashMap::from([("start".to_string(), 0x00400000u32)]),
                initial_data: vec![1, 0, 0, 0],
            },
            entries: vec![IrEntry {
                address: 0x00400000,
                kind: InstructionKind::Add,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(2)),
                    Operand::Register(RegisterKind::X(3)),
                ],
                bytecode: 0x003100B3,
                source_line: 1,
                source_column: 1,
            }],
        };

        let json = serde_json::to_string(&output).unwrap();
        let back: CodegenOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(back, output);
    }
}
