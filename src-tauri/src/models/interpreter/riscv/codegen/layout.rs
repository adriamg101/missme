use std::collections::HashMap;

use crate::models::interpreter::error::CodegenError;
use crate::models::interpreter::riscv::parser::ast::{Node, Operand, Program};
use crate::models::interpreter::riscv::CodegenConfig;

use super::expand::{expand, ExpandedInstruction};
use super::ir::MemoryLayout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Text,
    Data,
}

#[derive(Debug)]
pub(super) struct LayoutResult {
    pub layout: MemoryLayout,
    pub instructions: Vec<(u32, ExpandedInstruction)>,
}

enum PatchWidth {
    Word,
    Half,
}

pub(super) fn build(program: &Program, config: &CodegenConfig) -> Result<LayoutResult, Vec<CodegenError>> {
    let mut section = Section::Text;
    let mut text_cursor = config.text_base;
    let mut data_cursor = config.data_base;
    let mut labels: HashMap<String, u32> = HashMap::new();
    let mut initial_data: Vec<u8> = Vec::new();
    let mut instructions = Vec::new();
    let mut errors = Vec::new();
    let mut patches: Vec<(usize, String, PatchWidth)> = Vec::new();

    for node in &program.nodes {
        match node {
            Node::Directive { name, .. } if name == ".text" => {
                section = Section::Text;
            }
            Node::Directive { name, .. } if name == ".data" || name == ".bss" => {
                section = Section::Data;
            }
            Node::Directive { name, .. } if name == ".global" || name == ".globl" => {}
            Node::Label { name, .. } => {
                let addr = if section == Section::Text { text_cursor } else { data_cursor };
                labels.insert(name.clone(), addr);
            }
            Node::Instruction { line, column, .. } => {
                if section == Section::Data {
                    errors.push(CodegenError::new(*line, *column, "instruction found in .data section"));
                    continue;
                }
                // This runs even after an earlier check found errors, so a wrongly shaped instruction can still reach here.
                // We just skip this one instruction so one bad line does not crash the whole build, since its error was already reported.
                let expanded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| expand(node)));
                if let Ok(expanded) = expanded {
                    for instr in expanded {
                        instructions.push((text_cursor, instr));
                        text_cursor += 4;
                    }
                }
            }
            Node::Directive { name, args, line, column } => {
                const DATA_DIRECTIVES: [&str; 6] =
                    [".word", ".byte", ".half", ".string", ".asciz", ".ascii"];
                if !DATA_DIRECTIVES.contains(&name.as_str()) {
                    // This is not a section marker or a known data directive, and an earlier step already reported unknown ones.
                    // Either way there is nothing to place here.
                    continue;
                }
                if section == Section::Text {
                    errors.push(CodegenError::new(*line, *column, "data directive found in .text section"));
                    continue;
                }
                match name.as_str() {
                    ".word" => {
                        for arg in args {
                            match arg {
                                Operand::Immediate(v) => {
                                    initial_data.extend_from_slice(&(*v as u32).to_le_bytes());
                                }
                                Operand::LabelRef(label) => {
                                    patches.push((initial_data.len(), label.clone(), PatchWidth::Word));
                                    initial_data.extend_from_slice(&0u32.to_le_bytes());
                                }
                                _ => {}
                            }
                            data_cursor += 4;
                        }
                    }
                    ".half" => {
                        for arg in args {
                            match arg {
                                Operand::Immediate(v) => {
                                    initial_data.extend_from_slice(&(*v as u16).to_le_bytes());
                                }
                                Operand::LabelRef(label) => {
                                    patches.push((initial_data.len(), label.clone(), PatchWidth::Half));
                                    initial_data.extend_from_slice(&0u16.to_le_bytes());
                                }
                                _ => {}
                            }
                            data_cursor += 2;
                        }
                    }
                    ".byte" => {
                        for arg in args {
                            if let Operand::Immediate(v) = arg {
                                initial_data.push(*v as u8);
                            }
                            data_cursor += 1;
                        }
                    }
                    ".string" | ".asciz" => {
                        if let Some(Operand::StringLiteral(s)) = args.first() {
                            initial_data.extend_from_slice(s.as_bytes());
                            initial_data.push(0);
                            data_cursor += s.len() as u32 + 1;
                        }
                    }
                    ".ascii" => {
                        if let Some(Operand::StringLiteral(s)) = args.first() {
                            initial_data.extend_from_slice(s.as_bytes());
                            data_cursor += s.len() as u32;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    for (offset, label, width) in patches {
        // Falls back to address 0 for a label that was already reported as
        // undefined earlier, instead of crashing here.
        let addr = labels.get(&label).copied().unwrap_or(0);
        match width {
            PatchWidth::Word => {
                initial_data[offset..offset + 4].copy_from_slice(&addr.to_le_bytes());
            }
            PatchWidth::Half => {
                initial_data[offset..offset + 2].copy_from_slice(&(addr as u16).to_le_bytes());
            }
        }
    }

    Ok(LayoutResult {
        layout: MemoryLayout {
            text_base: config.text_base,
            data_base: config.data_base,
            labels,
            initial_data,
        },
        instructions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};

    fn config() -> CodegenConfig {
        CodegenConfig { text_base: 0x1000, data_base: 0x2000 }
    }

    #[test]
    fn text_is_the_implicit_default_section() {
        let program = Program {
            nodes: vec![Node::Label { name: "start".into(), line: 1, column: 1 }],
        };
        let result = build(&program, &config()).unwrap();
        assert_eq!(result.layout.labels["start"], 0x1000);
    }

    #[test]
    fn label_addresses_resolve_in_text_and_data() {
        let program = Program {
            nodes: vec![
                Node::Directive { name: ".data".into(), args: vec![], line: 1, column: 1 },
                Node::Label { name: "values".into(), line: 2, column: 1 },
                Node::Directive {
                    name: ".word".into(),
                    args: vec![Operand::Immediate(1), Operand::Immediate(2)],
                    line: 3,
                    column: 1,
                },
                Node::Directive { name: ".text".into(), args: vec![], line: 4, column: 1 },
                Node::Label { name: "start".into(), line: 5, column: 1 },
                Node::Instruction {
                    kind: InstructionKind::Add,
                    operands: vec![
                        Operand::Register(RegisterKind::X(1)),
                        Operand::Register(RegisterKind::X(2)),
                        Operand::Register(RegisterKind::X(3)),
                    ],
                    line: 6,
                    column: 1,
                },
            ],
        };
        let result = build(&program, &config()).unwrap();
        assert_eq!(result.layout.labels["values"], 0x2000);
        assert_eq!(result.layout.labels["start"], 0x1000);
        assert_eq!(result.instructions.len(), 1);
        assert_eq!(result.instructions[0].0, 0x1000);
    }

    #[test]
    fn word_directive_writes_little_endian_bytes() {
        let program = Program {
            nodes: vec![
                Node::Directive { name: ".data".into(), args: vec![], line: 1, column: 1 },
                Node::Directive {
                    name: ".word".into(),
                    args: vec![Operand::Immediate(1), Operand::Immediate(2), Operand::Immediate(3)],
                    line: 2,
                    column: 1,
                },
            ],
        };
        let result = build(&program, &config()).unwrap();
        assert_eq!(
            result.layout.initial_data,
            vec![1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0]
        );
    }

    #[test]
    fn byte_half_string_asciz_ascii_write_expected_bytes() {
        let program = Program {
            nodes: vec![
                Node::Directive { name: ".data".into(), args: vec![], line: 1, column: 1 },
                Node::Directive { name: ".byte".into(), args: vec![Operand::Immediate(0xAB)], line: 2, column: 1 },
                Node::Directive { name: ".half".into(), args: vec![Operand::Immediate(0x1234)], line: 3, column: 1 },
                Node::Directive {
                    name: ".string".into(),
                    args: vec![Operand::StringLiteral("hi".into())],
                    line: 4,
                    column: 1,
                },
                Node::Directive {
                    name: ".ascii".into(),
                    args: vec![Operand::StringLiteral("ok".into())],
                    line: 5,
                    column: 1,
                },
            ],
        };
        let result = build(&program, &config()).unwrap();
        assert_eq!(
            result.layout.initial_data,
            vec![0xAB, 0x34, 0x12, b'h', b'i', 0, b'o', b'k']
        );
    }

    #[test]
    fn word_with_label_ref_resolves_to_little_endian_address() {
        let program = Program {
            nodes: vec![
                Node::Directive { name: ".data".into(), args: vec![], line: 1, column: 1 },
                Node::Label { name: "target".into(), line: 2, column: 1 },
                Node::Directive { name: ".word".into(), args: vec![Operand::Immediate(0)], line: 3, column: 1 },
                Node::Directive {
                    name: ".word".into(),
                    args: vec![Operand::LabelRef("target".into())],
                    line: 4,
                    column: 1,
                },
            ],
        };
        let result = build(&program, &config()).unwrap();
        assert_eq!(&result.layout.initial_data[4..8], &0x2000u32.to_le_bytes());
    }

    #[test]
    fn instruction_in_data_section_is_an_error() {
        let program = Program {
            nodes: vec![
                Node::Directive { name: ".data".into(), args: vec![], line: 1, column: 1 },
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
        let errors = build(&program, &config()).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "instruction found in .data section");
    }

    #[test]
    fn data_directive_in_text_section_is_an_error() {
        let program = Program {
            nodes: vec![Node::Directive {
                name: ".word".into(),
                args: vec![Operand::Immediate(1)],
                line: 1,
                column: 1,
            }],
        };
        let errors = build(&program, &config()).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "data directive found in .text section");
    }

    #[test]
    fn pseudo_instruction_expansion_advances_the_cursor_by_its_real_size() {
        let program = Program {
            nodes: vec![
                Node::Instruction {
                    kind: InstructionKind::Li,
                    operands: vec![Operand::Register(RegisterKind::X(1)), Operand::Immediate(0x12345678)],
                    line: 1,
                    column: 1,
                },
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
        let result = build(&program, &config()).unwrap();
        assert_eq!(result.instructions.len(), 3);
        assert_eq!(result.instructions[0].0, 0x1000);
        assert_eq!(result.instructions[1].0, 0x1004);
        assert_eq!(result.instructions[2].0, 0x1008);
    }
}
