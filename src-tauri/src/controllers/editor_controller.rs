use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::models::interpreter::error::{CodegenError, LexError, ParseError, SemanticError};
use crate::models::interpreter::language::Language;
use crate::models::interpreter::riscv::{CodegenConfig, RiscV, RiscvConfig};

#[derive(Debug, Serialize)]
pub struct CompileResult {
    pub error_count: usize,
    pub error_output: String,
    pub config: RiscvConfig,
}

enum CompileErrorEntry {
    Lex(LexError),
    Parse(ParseError),
    Semantic(SemanticError),
    Codegen(CodegenError),
}

fn line_col(entry: &CompileErrorEntry) -> (usize, usize) {
    match entry {
        CompileErrorEntry::Lex(e) => (e.line, e.column),
        CompileErrorEntry::Parse(e) => (e.line, e.column),
        CompileErrorEntry::Semantic(e) => (e.line, e.column),
        CompileErrorEntry::Codegen(e) => (e.line, e.column),
    }
}

fn format_entry(entry: &CompileErrorEntry) -> String {
    match entry {
        CompileErrorEntry::Lex(e) => e.format(),
        CompileErrorEntry::Parse(e) => e.format(),
        CompileErrorEntry::Semantic(e) => e.format(),
        CompileErrorEntry::Codegen(e) => e.format(),
    }
}

fn work_dir() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("missme");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[tauri::command]
pub fn compile(
    source: String,
    config: RiscvConfig,
    codegen_config: Option<CodegenConfig>,
) -> Result<CompileResult, String> {
    let codegen_config = codegen_config.unwrap_or_default();
    let dir = work_dir().map_err(|e| e.to_string())?;
    let source_path = dir.join("source.s");
    let preprocessed_path = dir.join("preprocessed.s");
    let tokens_path = dir.join("tokens.jsonl");
    let ast_path = dir.join("ast.json");
    let codegen_path = dir.join("codegen.json");

    fs::write(&source_path, &source).map_err(|e| e.to_string())?;

    let lang = RiscV;

    let mut entries: Vec<CompileErrorEntry> = Vec::new();
    entries.extend(
        lang.preprocess(&source_path, &preprocessed_path)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(CompileErrorEntry::Lex),
    );
    entries.extend(
        lang.tokenize(&preprocessed_path, &tokens_path)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(CompileErrorEntry::Lex),
    );
    entries.extend(
        lang.parse(&tokens_path, &ast_path)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(CompileErrorEntry::Parse),
    );
    entries.extend(
        lang.analyze(&ast_path, &config)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(CompileErrorEntry::Semantic),
    );
    entries.extend(
        lang.generate(&ast_path, &config, &codegen_config, &codegen_path)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(CompileErrorEntry::Codegen),
    );

    entries.sort_by_key(line_col);

    let error_output = entries
        .iter()
        .map(format_entry)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(CompileResult {
        error_count: entries.len(),
        error_output,
        config,
    })
}

// Both scenarios below share the fixed `missme` work directory by design
// (see spec), so they are combined into one test and run sequentially to
// avoid a file-write race that would occur if cargo ran them as separate,
// parallel tests.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::{Base, Extension};
    use std::collections::HashSet;

    #[test]
    fn compile_handles_valid_and_invalid_sources() {
        let ok_config = RiscvConfig {
            base: Base::Rv32i,
            extensions: HashSet::from([Extension::M]),
        };
        let ok_result = compile("add x1, x2, x3\n".to_string(), ok_config.clone(), None).unwrap();
        assert_eq!(ok_result.error_count, 0);
        assert_eq!(ok_result.error_output, "");
        assert_eq!(ok_result.config.base, ok_config.base);

        let err_config = RiscvConfig {
            base: Base::Rv32i,
            extensions: HashSet::new(),
        };
        let source = ".include \"x.s\"\nadd x1, @x2, x3\n".to_string();
        let err_result = compile(source, err_config, None).unwrap();
        // 3, not 2: preprocess leaves the unsupported `.include` line intact
        // (it only reports the error, it doesn't strip the line), so with
        // all four stages always running, semantic analysis also flags that
        // same line as an unknown directive, alongside the tokenizer's
        // "unexpected character '@'" error.
        assert_eq!(err_result.error_count, 3);
        assert!(err_result.error_output.starts_with("1:1: error:"));
        assert!(err_result
            .error_output
            .contains("unknown directive '.include'"));
        assert!(err_result.error_output.contains("unexpected character '@'"));

        let semantic_err_config = RiscvConfig {
            base: Base::Rv32i,
            extensions: HashSet::new(),
        };
        let semantic_source = "jal x1, missing\n".to_string();
        let semantic_result = compile(semantic_source, semantic_err_config, None).unwrap();
        assert_eq!(semantic_result.error_count, 1);
        assert!(semantic_result
            .error_output
            .contains("undefined label 'missing'"));

        let codegen_err_config = RiscvConfig {
            base: Base::Rv32i,
            extensions: HashSet::new(),
        };
        let codegen_source = ".data\nadd x1, x2, x3\n".to_string();
        let codegen_result = compile(codegen_source, codegen_err_config, None).unwrap();
        assert_eq!(codegen_result.error_count, 1);
        assert!(codegen_result
            .error_output
            .contains("instruction found in .data section"));
    }
}
