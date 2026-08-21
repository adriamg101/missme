use std::io;
use std::path::Path;

use super::error::LexError;

pub trait Language {
    fn preprocess(&self, source_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>>;
    fn tokenize(&self, preprocessed_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>>;
}
