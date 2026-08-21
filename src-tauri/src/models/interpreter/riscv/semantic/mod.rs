mod checks;
mod symbol_table;

use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

use crate::models::interpreter::error::SemanticError;

use crate::models::interpreter::riscv::lexer::token::InstructionKind;
use super::parser::ast::{Node, Operand, Program};
use super::RiscvConfig;
use checks::{check_config, validate_directive, validate_operands};
use symbol_table::SymbolTable;

pub fn analyze(ast_path: &Path, config: &RiscvConfig) -> io::Result<Vec<SemanticError>> {
    let reader = BufReader::new(File::open(ast_path)?);
    let program: Program = serde_json::from_reader(reader)?;

    let mut errors = Vec::new();
    let mut symbols = SymbolTable::new();
    let mut pending: Vec<(String, usize, usize)> = Vec::new();

    for (index, node) in program.nodes.iter().enumerate() {
        match node {
            Node::Label { name, line, column } => {
                if let Err(err) = symbols.define(name, index, *line, *column) {
                    errors.push(err);
                }
            }
            Node::Instruction { kind, operands, line, column } => {
                if let Err(err) = check_config(*kind, config, *line, *column) {
                    errors.push(err);
                }
                if let Err(err) = validate_operands(*kind, operands, config, *line, *column) {
                    errors.push(err);
                }
                if *kind != InstructionKind::Fence {
                    for operand in operands {
                        if let Operand::LabelRef(label) = operand {
                            pending.push((label.clone(), *line, *column));
                        }
                    }
                }
            }
            Node::Directive { name, args, line, column } => {
                if let Err(err) = validate_directive(name, args, *line, *column) {
                    errors.push(err);
                }
                for arg in args {
                    if let Operand::LabelRef(label) = arg {
                        pending.push((label.clone(), *line, *column));
                    }
                }
            }
        }
    }

    for (label, line, column) in pending {
        if symbols.resolve(&label).is_none() {
            errors.push(SemanticError::new(line, column, format!("undefined label '{}'", label)));
        }
    }

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
    use crate::models::interpreter::riscv::Base;
    use std::collections::HashSet;
    use tempfile::tempdir;

    fn write_program(path: &Path, nodes: Vec<Node>) {
        let program = Program { nodes };
        let file = File::create(path).unwrap();
        serde_json::to_writer(file, &program).unwrap();
    }

    fn base_config() -> RiscvConfig {
        RiscvConfig { base: Base::Rv32i, extensions: HashSet::new() }
    }

    #[test]
    fn forward_label_reference_resolves() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        write_program(
            &ast_path,
            vec![
                Node::Instruction {
                    kind: InstructionKind::Jal,
                    operands: vec![Operand::Register(RegisterKind::X(1)), Operand::LabelRef("end".into())],
                    line: 1,
                    column: 1,
                },
                Node::Label { name: "end".into(), line: 2, column: 1 },
            ],
        );

        let errors = analyze(&ast_path, &base_config()).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn undefined_label_reference_is_an_error() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        write_program(
            &ast_path,
            vec![Node::Instruction {
                kind: InstructionKind::Jal,
                operands: vec![Operand::Register(RegisterKind::X(1)), Operand::LabelRef("missing".into())],
                line: 1,
                column: 1,
            }],
        );

        let errors = analyze(&ast_path, &base_config()).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "undefined label 'missing'");
    }

    #[test]
    fn duplicate_label_is_an_error() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        write_program(
            &ast_path,
            vec![
                Node::Label { name: "loop".into(), line: 1, column: 1 },
                Node::Label { name: "loop".into(), line: 3, column: 1 },
            ],
        );

        let errors = analyze(&ast_path, &base_config()).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 3);
    }

    #[test]
    fn fence_identifier_operands_are_not_treated_as_label_references() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        write_program(
            &ast_path,
            vec![Node::Instruction {
                kind: InstructionKind::Fence,
                operands: vec![Operand::LabelRef("rw".into()), Operand::LabelRef("rw".into())],
                line: 1,
                column: 1,
            }],
        );

        let errors = analyze(&ast_path, &base_config()).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_operand_count_is_reported_alongside_label_errors() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        write_program(
            &ast_path,
            vec![
                Node::Instruction {
                    kind: InstructionKind::Add,
                    operands: vec![Operand::Register(RegisterKind::X(1))],
                    line: 1,
                    column: 1,
                },
                Node::Instruction {
                    kind: InstructionKind::Jal,
                    operands: vec![Operand::Register(RegisterKind::X(1)), Operand::LabelRef("missing".into())],
                    line: 2,
                    column: 1,
                },
            ],
        );

        let errors = analyze(&ast_path, &base_config()).unwrap();
        assert_eq!(errors.len(), 2);
    }
}
