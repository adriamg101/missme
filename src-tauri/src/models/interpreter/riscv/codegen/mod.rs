mod encode;
mod expand;
pub mod ir;
mod layout;

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;

use crate::models::interpreter::error::CodegenError;
use crate::models::interpreter::riscv::lexer::token::InstructionKind;
use crate::models::interpreter::riscv::parser::ast::{Operand, Program};
use crate::models::interpreter::riscv::{CodegenConfig, RiscvConfig};

use encode::encode;
use expand::ExpandedInstruction;
use ir::{CodegenOutput, IrEntry};
use layout::build;

pub fn generate(
    ast_path: &Path,
    _riscv_config: &RiscvConfig,
    codegen_config: &CodegenConfig,
    out_path: &Path,
) -> io::Result<Vec<CodegenError>> {
    let reader = BufReader::new(File::open(ast_path)?);
    let program: Program = serde_json::from_reader(reader)?;

    let layout_result = match build(&program, codegen_config) {
        Ok(result) => result,
        Err(errors) => return Ok(errors),
    };

    let mut errors = Vec::new();
    let mut entries = Vec::new();

    for (address, instr) in &layout_result.instructions {
        // Codegen always runs even when semantic analysis already reported
        // errors elsewhere in the program (every stage always runs, so the
        // user sees every problem at once). An instruction with an invalid
        // operand shape that semantic analysis already flagged could still
        // reach here; resolve_operands/encode assume a valid shape and may
        // panic on one that isn't. Rather than let one malformed
        // instruction crash the whole compile, skip just that instruction
        // — its real error is already reported by semantic analysis.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let resolved = resolve_operands(instr, *address, &layout_result.layout.labels, &mut errors);
            let bytecode = encode(instr.kind, &resolved);
            (resolved, bytecode)
        }));
        if let Ok((resolved, bytecode)) = outcome {
            entries.push(IrEntry {
                address: *address,
                kind: instr.kind,
                operands: resolved,
                bytecode,
                source_line: instr.line,
                source_column: instr.column,
            });
        }
    }

    if !errors.is_empty() {
        return Ok(errors);
    }

    let output = CodegenOutput { layout: layout_result.layout, entries };
    let writer = BufWriter::new(File::create(out_path)?);
    serde_json::to_writer(writer, &output)?;

    Ok(Vec::new())
}

fn resolve_operands(
    instr: &ExpandedInstruction,
    address: u32,
    labels: &HashMap<String, u32>,
    errors: &mut Vec<CodegenError>,
) -> Vec<Operand> {
    use InstructionKind::*;

    match instr.kind {
        Beq | Bne | Blt | Bge | Bltu | Bgeu => {
            let mut ops = instr.operands.clone();
            let offset = match label_address(&instr.operands[2], labels) {
                Some(target) => {
                    let offset = target as i64 - address as i64;
                    if !(-4096..=4094).contains(&offset) {
                        errors.push(CodegenError::new(instr.line, instr.column, "branch target out of range"));
                    }
                    offset
                }
                // Label doesn't resolve: semantic analysis already reported
                // "undefined label" for this line. Don't also report a
                // spurious range error on top of a meaningless offset.
                None => 0,
            };
            ops[2] = Operand::Immediate(offset);
            ops
        }
        Jal => {
            let mut ops = instr.operands.clone();
            let offset = match label_address(&instr.operands[1], labels) {
                Some(target) => {
                    let offset = target as i64 - address as i64;
                    if !(-1_048_576..=1_048_574).contains(&offset) {
                        errors.push(CodegenError::new(instr.line, instr.column, "jump target out of range"));
                    }
                    offset
                }
                None => 0,
            };
            ops[1] = Operand::Immediate(offset);
            ops
        }
        Auipc if matches!(&instr.operands[1], Operand::LabelRef(_)) => {
            // First half of a `la` expansion (see expand.rs): compute hi20
            // now; the paired `addi` (next instruction, same address+4)
            // computes lo12 below.
            let offset = label_address(&instr.operands[1], labels)
                .map(|target| target as i64 - address as i64)
                .unwrap_or(0);
            let lo = (offset & 0xFFF) as i32;
            let lo_signed = if lo >= 2048 { lo - 4096 } else { lo };
            let hi = (offset - lo_signed as i64) >> 12;
            vec![instr.operands[0].clone(), Operand::Immediate(hi)]
        }
        Addi if matches!(instr.operands.get(2), Some(Operand::LabelRef(_))) => {
            // Second half of a `la` expansion: same target, same math,
            // keep only lo12. `address` here is the auipc's address + 4.
            let offset = label_address(&instr.operands[2], labels)
                .map(|target| target as i64 - (address as i64 - 4))
                .unwrap_or(0);
            let lo = (offset & 0xFFF) as i32;
            let lo_signed = if lo >= 2048 { lo - 4096 } else { lo };
            vec![instr.operands[0].clone(), instr.operands[1].clone(), Operand::Immediate(lo_signed as i64)]
        }
        _ => instr.operands.clone(),
    }
}

/// Resolves a label reference to its address. Semantic analysis normally
/// guarantees every referenced label exists, but codegen still runs even
/// when semantic analysis already reported errors (every stage always
/// runs, so the user sees every problem at once) — in that situation an
/// undefined label reaches here too. Returns `None` rather than panicking
/// or reporting a duplicate/misleading error here: semantic analysis has
/// already reported the real "undefined label" error for this line, so
/// callers should degrade gracefully (skip range checks, use a placeholder
/// offset) rather than pile on a second, confusing error.
fn label_address(op: &Operand, labels: &HashMap<String, u32>) -> Option<u32> {
    match op {
        Operand::LabelRef(name) => labels.get(name).copied(),
        _ => unreachable!("expected a label reference"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::RegisterKind;
    use crate::models::interpreter::riscv::parser::ast::Node;
    use crate::models::interpreter::riscv::{Base, RiscvConfig};
    use std::collections::HashSet;
    use tempfile::tempdir;

    fn write_program(path: &Path, nodes: Vec<Node>) {
        let program = Program { nodes };
        let file = File::create(path).unwrap();
        serde_json::to_writer(file, &program).unwrap();
    }

    fn riscv_config() -> RiscvConfig {
        RiscvConfig { base: Base::Rv32i, extensions: HashSet::new() }
    }

    fn x(n: u8) -> Operand {
        Operand::Register(RegisterKind::X(n))
    }

    #[test]
    fn jal_to_a_nearby_label_resolves_to_the_correct_offset() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        let out_path = dir.path().join("codegen.json");
        write_program(
            &ast_path,
            vec![
                Node::Instruction { kind: InstructionKind::Jal, operands: vec![x(1), Operand::LabelRef("end".into())], line: 1, column: 1 },
                Node::Instruction {
                    kind: InstructionKind::Add,
                    operands: vec![x(1), x(2), x(3)],
                    line: 2,
                    column: 1,
                },
                Node::Label { name: "end".into(), line: 3, column: 1 },
            ],
        );

        let config = CodegenConfig { text_base: 0x1000, data_base: 0x2000 };
        let errors = generate(&ast_path, &riscv_config(), &config, &out_path).unwrap();
        assert!(errors.is_empty());

        let output: CodegenOutput = serde_json::from_reader(File::open(&out_path).unwrap()).unwrap();
        assert_eq!(output.entries[0].operands[1], Operand::Immediate(8));
    }

    #[test]
    fn branch_target_out_of_range_is_an_error() {
        // Exercise the range check directly via resolve_operands: a branch
        // whose target sits far outside a branch's +-4KiB encodable range.
        let mut labels = HashMap::new();
        labels.insert("far".to_string(), 0x1000 + 100_000);
        let instr = ExpandedInstruction {
            kind: InstructionKind::Beq,
            operands: vec![x(1), x(2), Operand::LabelRef("far".into())],
            line: 1,
            column: 1,
        };
        let mut errors = Vec::new();
        resolve_operands(&instr, 0x1000, &labels, &mut errors);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "branch target out of range");
    }

    #[test]
    fn jump_target_out_of_range_is_an_error() {
        let mut labels = HashMap::new();
        labels.insert("far".to_string(), 0x1000 + 2_000_000);
        let instr = ExpandedInstruction {
            kind: InstructionKind::Jal,
            operands: vec![x(1), Operand::LabelRef("far".into())],
            line: 1,
            column: 1,
        };
        let mut errors = Vec::new();
        resolve_operands(&instr, 0x1000, &labels, &mut errors);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "jump target out of range");
    }

    #[test]
    fn la_expansion_reconstructs_the_exact_offset_to_the_label() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        let out_path = dir.path().join("codegen.json");
        write_program(
            &ast_path,
            vec![
                Node::Instruction { kind: InstructionKind::La, operands: vec![x(1), Operand::LabelRef("values".into())], line: 1, column: 1 },
                Node::Directive { name: ".data".into(), args: vec![], line: 2, column: 1 },
                Node::Label { name: "values".into(), line: 3, column: 1 },
                Node::Directive { name: ".word".into(), args: vec![Operand::Immediate(1)], line: 4, column: 1 },
            ],
        );

        let config = CodegenConfig::default();
        let errors = generate(&ast_path, &riscv_config(), &config, &out_path).unwrap();
        assert!(errors.is_empty());

        let output: CodegenOutput = serde_json::from_reader(File::open(&out_path).unwrap()).unwrap();
        let auipc_addr = output.entries[0].address;
        let hi = match output.entries[0].operands[1] {
            Operand::Immediate(v) => v,
            _ => panic!("expected immediate"),
        };
        let lo = match output.entries[1].operands[2] {
            Operand::Immediate(v) => v,
            _ => panic!("expected immediate"),
        };
        let reconstructed = auipc_addr as i64 + (hi << 12) + lo;
        assert_eq!(reconstructed as u32, output.layout.labels["values"]);
    }

    #[test]
    fn generate_end_to_end_on_a_representative_program() {
        let dir = tempdir().unwrap();
        let ast_path = dir.path().join("ast.json");
        let out_path = dir.path().join("codegen.json");
        write_program(
            &ast_path,
            vec![
                Node::Directive { name: ".data".into(), args: vec![], line: 1, column: 1 },
                Node::Label { name: "values".into(), line: 2, column: 1 },
                Node::Directive {
                    name: ".word".into(),
                    args: vec![Operand::Immediate(1), Operand::Immediate(2), Operand::Immediate(3)],
                    line: 3,
                    column: 1,
                },
                Node::Directive { name: ".text".into(), args: vec![], line: 4, column: 1 },
                Node::Label { name: "start".into(), line: 5, column: 1 },
                Node::Instruction {
                    kind: InstructionKind::Addi,
                    operands: vec![x(1), x(0), Operand::Immediate(16)],
                    line: 6,
                    column: 1,
                },
                Node::Instruction {
                    kind: InstructionKind::Lw,
                    operands: vec![x(2), Operand::Offset { imm: 0, base: RegisterKind::X(1) }],
                    line: 7,
                    column: 1,
                },
            ],
        );

        let config = CodegenConfig::default();
        let errors = generate(&ast_path, &riscv_config(), &config, &out_path).unwrap();
        assert!(errors.is_empty());
        assert!(out_path.exists());

        let output: CodegenOutput = serde_json::from_reader(File::open(&out_path).unwrap()).unwrap();
        assert_eq!(output.entries.len(), 2);
        assert!(!output.layout.initial_data.is_empty());
    }
}
