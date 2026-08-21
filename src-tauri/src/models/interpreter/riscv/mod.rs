mod lexer;
mod parser;
mod semantic;

use std::collections::HashSet;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::{LexError, ParseError, SemanticError};
use super::language::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Base {
    Rv32i,
    Rv64i,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Extension {
    M,
    F,
    D,
    Q,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiscvConfig {
    pub base: Base,
    pub extensions: HashSet<Extension>,
}

pub struct RiscV;

impl Language for RiscV {
    fn preprocess(&self, source_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>> {
        lexer::preprocessor::preprocess(source_path, out_path)
    }

    fn tokenize(&self, preprocessed_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>> {
        lexer::tokenizer::tokenize(preprocessed_path, out_path)
    }

    fn parse(&self, tokens_path: &Path, out_path: &Path) -> io::Result<Vec<ParseError>> {
        parser::parse(tokens_path, out_path)
    }

    fn analyze(&self, ast_path: &Path, config: &RiscvConfig) -> io::Result<Vec<SemanticError>> {
        semantic::analyze(ast_path, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn end_to_end_preprocess_then_tokenize_produces_tokens_file() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.s");
        let preprocessed = dir.path().join("preprocessed.s");
        let tokens_path = dir.path().join("tokens.jsonl");
        std::fs::write(&source, "add x1, x2, x3 # comment\n").unwrap();

        let lang = RiscV;
        let pre_errors = lang.preprocess(&source, &preprocessed).unwrap();
        assert!(pre_errors.is_empty());

        let tok_errors = lang.tokenize(&preprocessed, &tokens_path).unwrap();
        assert!(tok_errors.is_empty());

        let mut contents = String::new();
        File::open(&tokens_path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents.lines().count(), 7);
    }

    #[test]
    fn language_parse_produces_ast_file() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.s");
        let preprocessed = dir.path().join("preprocessed.s");
        let tokens_path = dir.path().join("tokens.jsonl");
        let ast_path = dir.path().join("ast.json");
        std::fs::write(&source, "add x1, x2, x3\n").unwrap();

        let lang = RiscV;
        lang.preprocess(&source, &preprocessed).unwrap();
        lang.tokenize(&preprocessed, &tokens_path).unwrap();
        let parse_errors = lang.parse(&tokens_path, &ast_path).unwrap();

        assert!(parse_errors.is_empty());
        assert!(ast_path.exists());
    }

    #[test]
    fn riscv_config_serializes_with_expected_shape() {
        let config = RiscvConfig {
            base: Base::Rv32i,
            extensions: HashSet::from([Extension::M, Extension::F]),
        };
        let json: serde_json::Value = serde_json::to_value(&config).unwrap();
        assert_eq!(json["base"], "RV32I");
        let exts = json["extensions"].as_array().unwrap();
        assert_eq!(exts.len(), 2);
    }

    #[test]
    fn language_analyze_resolves_forward_label_reference() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.s");
        let preprocessed = dir.path().join("preprocessed.s");
        let tokens_path = dir.path().join("tokens.jsonl");
        let ast_path = dir.path().join("ast.json");
        std::fs::write(&source, "start:\n  jal x1, end\n  add x1, x2, x3\nend:\n").unwrap();

        let lang = RiscV;
        let config = RiscvConfig { base: Base::Rv32i, extensions: HashSet::new() };

        lang.preprocess(&source, &preprocessed).unwrap();
        lang.tokenize(&preprocessed, &tokens_path).unwrap();
        lang.parse(&tokens_path, &ast_path).unwrap();
        let semantic_errors = lang.analyze(&ast_path, &config).unwrap();

        assert!(semantic_errors.is_empty());
    }

    #[test]
    fn language_analyze_reports_undefined_label() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.s");
        let preprocessed = dir.path().join("preprocessed.s");
        let tokens_path = dir.path().join("tokens.jsonl");
        let ast_path = dir.path().join("ast.json");
        std::fs::write(&source, "jal x1, missing\n").unwrap();

        let lang = RiscV;
        let config = RiscvConfig { base: Base::Rv32i, extensions: HashSet::new() };

        lang.preprocess(&source, &preprocessed).unwrap();
        lang.tokenize(&preprocessed, &tokens_path).unwrap();
        lang.parse(&tokens_path, &ast_path).unwrap();
        let semantic_errors = lang.analyze(&ast_path, &config).unwrap();

        assert_eq!(semantic_errors.len(), 1);
        assert_eq!(semantic_errors[0].message, "undefined label 'missing'");
    }
}
