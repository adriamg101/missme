use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use crate::models::interpreter::error::LexError;

use super::token::{lookup_instruction, lookup_register, Token, TokenKind};

pub fn tokenize(preprocessed_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>> {
    let reader = BufReader::new(File::open(preprocessed_path)?);
    let mut writer = BufWriter::new(File::create(out_path)?);

    let mut errors = Vec::new();

    for (idx, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line_no = idx + 1;

        if line.trim().is_empty() {
            continue;
        }

        let tokens = tokenize_line(&line, line_no, &mut errors);
        for token in &tokens {
            serde_json::to_writer(&mut writer, token)?;
            writer.write_all(b"\n")?;
        }
    }

    writer.flush()?;
    Ok(errors)
}

fn tokenize_line(line: &str, line_no: usize, errors: &mut Vec<LexError>) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < chars.len() {
        let c = chars[i];
        let col = i + 1;

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c == ',' {
            tokens.push(Token::new(TokenKind::Comma, ",", line_no, col));
            i += 1;
            continue;
        }

        if c == ':' {
            tokens.push(Token::new(TokenKind::Colon, ":", line_no, col));
            i += 1;
            continue;
        }

        if c == '(' {
            tokens.push(Token::new(TokenKind::LParen, "(", line_no, col));
            i += 1;
            continue;
        }

        if c == ')' {
            tokens.push(Token::new(TokenKind::RParen, ")", line_no, col));
            i += 1;
            continue;
        }

        if c == '-' {
            tokens.push(Token::new(TokenKind::Minus, "-", line_no, col));
            i += 1;
            continue;
        }

        if c == '+' {
            tokens.push(Token::new(TokenKind::Plus, "+", line_no, col));
            i += 1;
            continue;
        }

        if c == '"' {
            let (lexeme, consumed, closed) = scan_string(&chars[i..]);
            if !closed {
                errors.push(LexError::new(line_no, col, "unterminated string literal"));
            } else {
                tokens.push(Token::new(TokenKind::StringLiteral, lexeme, line_no, col));
            }
            i += consumed;
            continue;
        }

        if c == '.' && chars.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic()) {
            let (lexeme, consumed) = scan_word(&chars[i..], true);
            tokens.push(Token::new(TokenKind::Directive, lexeme, line_no, col));
            i += consumed;
            continue;
        }

        if c.is_ascii_digit() {
            match scan_number(&chars[i..]) {
                Ok((lexeme, consumed)) => {
                    tokens.push(Token::new(TokenKind::Number, lexeme, line_no, col));
                    i += consumed;
                }
                Err(consumed) => {
                    errors.push(LexError::new(line_no, col, "malformed number literal"));
                    i += consumed;
                }
            }
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            let (lexeme, consumed) = scan_word(&chars[i..], false);
            let kind = if let Some(instr) = lookup_instruction(&lexeme) {
                TokenKind::Instruction(instr)
            } else if let Some(reg) = lookup_register(&lexeme) {
                TokenKind::Register(reg)
            } else {
                TokenKind::Identifier
            };
            tokens.push(Token::new(kind, lexeme, line_no, col));
            i += consumed;
            continue;
        }

        errors.push(LexError::new(
            line_no,
            col,
            format!("unexpected character '{}'", c),
        ));
        i += 1;
    }

    tokens.push(Token::new(TokenKind::Newline, "\n", line_no, chars.len() + 1));
    tokens
}

fn scan_word(chars: &[char], leading_dot: bool) -> (String, usize) {
    let mut end = if leading_dot { 1 } else { 0 };
    loop {
        while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
            end += 1;
        }
        // Instruction names can have several dot separated parts chained together, directives never do.
        if !leading_dot
            && chars.get(end) == Some(&'.')
            && chars.get(end + 1).is_some_and(|c| c.is_ascii_alphabetic())
        {
            end += 1;
            continue;
        }
        break;
    }
    (chars[..end].iter().collect(), end)
}

fn scan_string(chars: &[char]) -> (String, usize, bool) {
    let mut end = 1;
    while end < chars.len() {
        if chars[end] == '"' {
            return (chars[..=end].iter().collect(), end + 1, true);
        }
        end += 1;
    }
    (chars.iter().collect(), chars.len(), false)
}

fn scan_number(chars: &[char]) -> Result<(String, usize), usize> {
    if chars.len() >= 2 && chars[0] == '0' && (chars[1] == 'x' || chars[1] == 'X') {
        let mut end = 2;
        while end < chars.len() && chars[end].is_ascii_hexdigit() {
            end += 1;
        }
        if end == 2 || is_boundary_invalid(chars, end) {
            return Err(invalid_number_span(chars, end));
        }
        return Ok((chars[..end].iter().collect(), end));
    }

    if chars.len() >= 2 && chars[0] == '0' && (chars[1] == 'b' || chars[1] == 'B') {
        let mut end = 2;
        while end < chars.len() && (chars[end] == '0' || chars[end] == '1') {
            end += 1;
        }
        if end == 2 || is_boundary_invalid(chars, end) {
            return Err(invalid_number_span(chars, end));
        }
        return Ok((chars[..end].iter().collect(), end));
    }

    let mut end = 0;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if is_boundary_invalid(chars, end) {
        return Err(invalid_number_span(chars, end));
    }
    Ok((chars[..end].iter().collect(), end))
}

fn is_boundary_invalid(chars: &[char], end: usize) -> bool {
    chars
        .get(end)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
}

fn invalid_number_span(chars: &[char], mut end: usize) -> usize {
    while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
        end += 1;
    }
    end.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::token::{InstructionKind, RegisterKind};
    use std::io::Read;
    use tempfile::tempdir;

    fn run_tokenize(input: &str) -> (Vec<Token>, Vec<LexError>) {
        let dir = tempdir().unwrap();
        let source_path = dir.path().join("preprocessed.s");
        let out_path = dir.path().join("tokens.jsonl");
        std::fs::write(&source_path, input).unwrap();

        let errors = tokenize(&source_path, &out_path).unwrap();

        let mut raw = String::new();
        File::open(&out_path)
            .unwrap()
            .read_to_string(&mut raw)
            .unwrap();
        let tokens: Vec<Token> = raw
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        (tokens, errors)
    }

    #[test]
    fn tokenizes_label_directive_and_instruction() {
        let (tokens, errors) = run_tokenize("loop: .text\nadd x1, x2, x3\n");
        assert!(errors.is_empty());

        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier,
                TokenKind::Colon,
                TokenKind::Directive,
                TokenKind::Newline,
                TokenKind::Instruction(InstructionKind::Add),
                TokenKind::Register(RegisterKind::X(1)),
                TokenKind::Comma,
                TokenKind::Register(RegisterKind::X(2)),
                TokenKind::Comma,
                TokenKind::Register(RegisterKind::X(3)),
                TokenKind::Newline,
            ]
        );
        assert_eq!(tokens[0].lexeme, "loop");
        assert_eq!(tokens[2].lexeme, ".text");
    }

    #[test]
    fn tokenizes_numbers_in_all_bases() {
        let (tokens, errors) = run_tokenize("li x1, 42\nli x2, 0x2A\nli x3, 0b101010\n");
        assert!(errors.is_empty());
        let numbers: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Number)
            .map(|t| t.lexeme.clone())
            .collect();
        assert_eq!(numbers, vec!["42", "0x2A", "0b101010"]);
    }

    #[test]
    fn tokenizes_offset_register_syntax_and_string_literal() {
        let (tokens, errors) = run_tokenize("lw x1, 4(x2)\n.asciz \"hi\"\n");
        assert!(errors.is_empty());
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Instruction(InstructionKind::Lw),
                TokenKind::Register(RegisterKind::X(1)),
                TokenKind::Comma,
                TokenKind::Number,
                TokenKind::LParen,
                TokenKind::Register(RegisterKind::X(2)),
                TokenKind::RParen,
                TokenKind::Newline,
                TokenKind::Directive,
                TokenKind::StringLiteral,
                TokenKind::Newline,
            ]
        );
        let string_token = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .unwrap();
        assert_eq!(string_token.lexeme, "\"hi\"");
    }

    #[test]
    fn blank_lines_produce_no_tokens_but_keep_line_numbers_correct() {
        let (tokens, _errors) = run_tokenize("add x1, x2, x3\n\nsub x1, x2, x3\n");
        let last_line_tokens: Vec<_> = tokens.iter().filter(|t| t.lexeme == "sub").collect();
        assert_eq!(last_line_tokens[0].line, 3);
    }

    #[test]
    fn reports_invalid_character() {
        let (_tokens, errors) = run_tokenize("add x1, @x2, x3\n");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "unexpected character '@'");
        assert_eq!(errors[0].line, 1);
    }

    #[test]
    fn reports_unterminated_string_literal() {
        let (_tokens, errors) = run_tokenize(".asciz \"never closed\n");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "unterminated string literal");
    }

    #[test]
    fn tokenizes_negative_immediate_as_minus_then_number() {
        let (tokens, errors) = run_tokenize("addi sp, sp, -16\n");
        assert!(errors.is_empty());
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Instruction(InstructionKind::Addi),
                TokenKind::Register(RegisterKind::X(2)),
                TokenKind::Comma,
                TokenKind::Register(RegisterKind::X(2)),
                TokenKind::Comma,
                TokenKind::Minus,
                TokenKind::Number,
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn tokenizes_extension_and_pseudo_instructions() {
        let (tokens, errors) = run_tokenize("mul x1, x2, x3\nfadd.s f1, f2, f3\nli x1, 5\n");
        assert!(errors.is_empty());
        let kinds: Vec<_> = tokens
            .iter()
            .filter(|t| matches!(t.kind, TokenKind::Instruction(_)))
            .map(|t| t.kind.clone())
            .collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Instruction(InstructionKind::Mul),
                TokenKind::Instruction(InstructionKind::FaddS),
                TokenKind::Instruction(InstructionKind::Li),
            ]
        );
    }

    #[test]
    fn register_alias_and_numbered_forms_resolve_to_the_same_register() {
        let (tokens, errors) = run_tokenize("add sp, x2, fp\n");
        assert!(errors.is_empty());
        let regs: Vec<_> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Register(r) => Some(*r),
                _ => None,
            })
            .collect();
        assert_eq!(
            regs,
            vec![RegisterKind::X(2), RegisterKind::X(2), RegisterKind::X(8)]
        );
    }

    #[test]
    fn label_that_is_not_a_keyword_stays_an_identifier() {
        let (tokens, errors) = run_tokenize("loop: j loop\n");
        assert!(errors.is_empty());
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].lexeme, "loop");
        assert_eq!(tokens[2].kind, TokenKind::Instruction(InstructionKind::J));
        assert_eq!(tokens[3].kind, TokenKind::Identifier);
        assert_eq!(tokens[3].lexeme, "loop");
    }

    #[test]
    fn reports_malformed_number_literal() {
        let (_tokens, errors) = run_tokenize("li x1, 0xg1\n");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "malformed number literal");
    }
}
