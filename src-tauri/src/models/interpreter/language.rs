use std::io;
use std::path::Path;

use super::error::{CodegenError, LexError, ParseError, SemanticError};
use super::riscv::{CodegenConfig, RiscvConfig};

pub trait Language {
    fn preprocess(&self, source_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>>;
    fn tokenize(&self, preprocessed_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>>;
    fn parse(&self, tokens_path: &Path, out_path: &Path) -> io::Result<Vec<ParseError>>;
    fn analyze(&self, ast_path: &Path, config: &RiscvConfig) -> io::Result<Vec<SemanticError>>;
    fn generate(
        &self,
        ast_path: &Path,
        riscv_config: &RiscvConfig,
        codegen_config: &CodegenConfig,
        out_path: &Path,
    ) -> io::Result<Vec<CodegenError>>;
}
