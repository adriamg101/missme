use std::collections::HashMap;

use crate::models::interpreter::error::SemanticError;

#[derive(Debug, Default)]
pub(super) struct SymbolTable {
    labels: HashMap<String, usize>,
}

impl SymbolTable {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn define(
        &mut self,
        name: &str,
        node_index: usize,
        line: usize,
        column: usize,
    ) -> Result<(), SemanticError> {
        if self.labels.contains_key(name) {
            return Err(SemanticError::new(
                line,
                column,
                format!("label '{}' is already defined", name),
            ));
        }
        self.labels.insert(name.to_string(), node_index);
        Ok(())
    }

    pub(super) fn resolve(&self, name: &str) -> Option<usize> {
        self.labels.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defines_and_resolves_a_label() {
        let mut table = SymbolTable::new();
        table.define("loop", 3, 1, 1).unwrap();
        assert_eq!(table.resolve("loop"), Some(3));
    }

    #[test]
    fn duplicate_definition_is_an_error() {
        let mut table = SymbolTable::new();
        table.define("loop", 3, 1, 1).unwrap();
        let err = table.define("loop", 7, 2, 1).unwrap_err();
        assert_eq!(err.message, "label 'loop' is already defined");
        assert_eq!(err.line, 2);
    }

    #[test]
    fn unresolved_label_is_none() {
        let table = SymbolTable::new();
        assert_eq!(table.resolve("missing"), None);
    }
}
