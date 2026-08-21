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
}
