use std::io;
use std::path::Path;

use super::error::{LexError, ParseError, SemanticError};
use super::riscv::RiscvConfig;

pub trait Language {
    fn preprocess(&self, source_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>>;
    fn tokenize(&self, preprocessed_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>>;
    fn parse(&self, tokens_path: &Path, out_path: &Path) -> io::Result<Vec<ParseError>>;
    fn analyze(&self, ast_path: &Path, config: &RiscvConfig) -> io::Result<Vec<SemanticError>>;
}
