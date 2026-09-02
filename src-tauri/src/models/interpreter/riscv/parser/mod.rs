mod grammar;
pub mod ast;

use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter};
use std::path::Path;

use crate::models::interpreter::error::ParseError;
use crate::models::interpreter::riscv::lexer::token::{Token, TokenKind};

use ast::{Node, Program};
use grammar::parse_statement;

pub fn parse(tokens_path: &Path, out_path: &Path) -> io::Result<Vec<ParseError>> {
    let reader = BufReader::new(File::open(tokens_path)?);
    let mut errors = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();
    let mut current: Vec<Token> = Vec::new();

    for line in reader.lines() {
        let token: Token = serde_json::from_str(&line?)?;
        if token.kind == TokenKind::Newline {
            let (mut stmt_nodes, err) = parse_statement(&current);
            nodes.append(&mut stmt_nodes);
            if let Some(err) = err {
                errors.push(err);
            }
            current.clear();
        } else {
            current.push(token);
        }
    }

    if !current.is_empty() {
        let (mut stmt_nodes, err) = parse_statement(&current);
        nodes.append(&mut stmt_nodes);
        if let Some(err) = err {
            errors.push(err);
        }
    }

    let program = Program { nodes };
    let writer = BufWriter::new(File::create(out_path)?);
    serde_json::to_writer(writer, &program)?;

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
    use ast::Operand;
    use std::fs;
    use tempfile::tempdir;

    fn write_tokens(path: &Path, tokens: &[Token]) {
        let mut contents = String::new();
        for token in tokens {
            contents.push_str(&serde_json::to_string(token).unwrap());
            contents.push('\n');
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn parses_valid_tokens_into_ast_with_no_errors() {
        let dir = tempdir().unwrap();
        let tokens_path = dir.path().join("tokens.jsonl");
        let ast_path = dir.path().join("ast.json");

        write_tokens(
            &tokens_path,
            &[
                Token::new(TokenKind::Instruction(InstructionKind::Add), "add", 1, 1),
                Token::new(TokenKind::Register(RegisterKind::X(1)), "x1", 1, 5),
                Token::new(TokenKind::Comma, ",", 1, 7),
                Token::new(TokenKind::Register(RegisterKind::X(2)), "x2", 1, 9),
                Token::new(TokenKind::Comma, ",", 1, 11),
                Token::new(TokenKind::Register(RegisterKind::X(3)), "x3", 1, 13),
                Token::new(TokenKind::Newline, "\n", 1, 14),
            ],
        );

        let errors = parse(&tokens_path, &ast_path).unwrap();
        assert!(errors.is_empty());

        let contents = fs::read_to_string(&ast_path).unwrap();
        let program: Program = serde_json::from_str(&contents).unwrap();
        assert_eq!(
            program,
            Program {
                nodes: vec![Node::Instruction {
                    kind: InstructionKind::Add,
                    operands: vec![
                        Operand::Register(RegisterKind::X(1)),
                        Operand::Register(RegisterKind::X(2)),
                        Operand::Register(RegisterKind::X(3)),
                    ],
                    line: 1,
                    column: 1,
                }]
            }
        );
    }

    #[test]
    fn recovers_after_a_syntax_error_and_keeps_parsing() {
        let dir = tempdir().unwrap();
        let tokens_path = dir.path().join("tokens.jsonl");
        let ast_path = dir.path().join("ast.json");

        write_tokens(
            &tokens_path,
            &[
                // Line 1: not a valid statement, just a lone comma.
                Token::new(TokenKind::Comma, ",", 1, 1),
                Token::new(TokenKind::Newline, "\n", 1, 2),
                // Line 2: a valid instruction.
                Token::new(TokenKind::Instruction(InstructionKind::Ecall), "ecall", 2, 1),
                Token::new(TokenKind::Newline, "\n", 2, 6),
            ],
        );

        let errors = parse(&tokens_path, &ast_path).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 1);

        let contents = fs::read_to_string(&ast_path).unwrap();
        let program: Program = serde_json::from_str(&contents).unwrap();
        assert_eq!(
            program,
            Program {
                nodes: vec![Node::Instruction {
                    kind: InstructionKind::Ecall,
                    operands: vec![],
                    line: 2,
                    column: 1,
                }]
            }
        );
    }
}
