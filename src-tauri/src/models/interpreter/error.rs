use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LexError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl LexError {
    pub fn new(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            column,
            message: message.into(),
        }
    }

    pub fn format(&self) -> String {
        format!("{}:{}: error: {}", self.line, self.column, self.message)
    }
}

pub fn format_errors(errors: &[LexError]) -> String {
    errors
        .iter()
        .map(LexError::format)
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl ParseError {
    pub fn new(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            column,
            message: message.into(),
        }
    }

    pub fn format(&self) -> String {
        format!("{}:{}: error: {}", self.line, self.column, self.message)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SemanticError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl SemanticError {
    pub fn new(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            column,
            message: message.into(),
        }
    }

    pub fn format(&self) -> String {
        format!("{}:{}: error: {}", self.line, self.column, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_as_gcc_style_message() {
        let err = LexError::new(3, 7, "unexpected character '@'");
        assert_eq!(err.format(), "3:7: error: unexpected character '@'");
    }

    #[test]
    fn format_errors_joins_multiple_with_newline() {
        let errors = vec![LexError::new(2, 9, "first"), LexError::new(5, 1, "second")];
        assert_eq!(
            format_errors(&errors),
            "2:9: error: first\n5:1: error: second"
        );
    }

    #[test]
    fn format_errors_returns_empty_string_for_no_errors() {
        assert_eq!(format_errors(&[]), "");
    }

    #[test]
    fn parse_error_formats_as_gcc_style_message() {
        let err = ParseError::new(4, 2, "expected operand, found ','");
        assert_eq!(err.format(), "4:2: error: expected operand, found ','");
    }

    #[test]
    fn semantic_error_formats_as_gcc_style_message() {
        let err = SemanticError::new(6, 1, "label 'loop' is already defined");
        assert_eq!(err.format(), "6:1: error: label 'loop' is already defined");
    }
}
