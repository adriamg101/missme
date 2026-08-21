use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::models::interpreter::error::{format_errors, LexError};
use crate::models::interpreter::language::Language;
use crate::models::interpreter::riscv::{RiscV, RiscvConfig};

#[derive(Debug, Serialize)]
pub struct CompileResult {
    pub error_count: usize,
    pub error_output: String,
    pub config: RiscvConfig,
}

fn work_dir() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("missme");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[tauri::command]
pub fn compile(source: String, config: RiscvConfig) -> Result<CompileResult, String> {
    let dir = work_dir().map_err(|e| e.to_string())?;
    let source_path = dir.join("source.s");
    let preprocessed_path = dir.join("preprocessed.s");
    let tokens_path = dir.join("tokens.jsonl");

    fs::write(&source_path, &source).map_err(|e| e.to_string())?;

    let lang = RiscV;

    let mut errors: Vec<LexError> = lang
        .preprocess(&source_path, &preprocessed_path)
        .map_err(|e| e.to_string())?;
    errors.extend(
        lang.tokenize(&preprocessed_path, &tokens_path)
            .map_err(|e| e.to_string())?,
    );

    errors.sort_by_key(|e| (e.line, e.column));

    Ok(CompileResult {
        error_count: errors.len(),
        error_output: format_errors(&errors),
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
        let ok_result = compile("add x1, x2, x3\n".to_string(), ok_config.clone()).unwrap();
        assert_eq!(ok_result.error_count, 0);
        assert_eq!(ok_result.error_output, "");
        assert_eq!(ok_result.config.base, ok_config.base);

        let err_config = RiscvConfig {
            base: Base::Rv32i,
            extensions: HashSet::new(),
        };
        let source = ".include \"x.s\"\nadd x1, @x2, x3\n".to_string();
        let err_result = compile(source, err_config).unwrap();
        assert_eq!(err_result.error_count, 2);
        assert!(err_result.error_output.starts_with("1:1: error:"));
    }
}
