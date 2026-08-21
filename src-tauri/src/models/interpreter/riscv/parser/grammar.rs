use crate::models::interpreter::error::ParseError;
use crate::models::interpreter::riscv::lexer::token::{Token, TokenKind};

use super::ast::{Node, Operand};

pub(super) fn parse_statement(tokens: &[Token]) -> (Vec<Node>, Option<ParseError>) {
    if tokens.is_empty() {
        return (Vec::new(), None);
    }

    let mut nodes = Vec::new();
    let mut pos = 0;

    if let (Some(t0), Some(t1)) = (tokens.get(0), tokens.get(1)) {
        if t0.kind == TokenKind::Identifier && t1.kind == TokenKind::Colon {
            nodes.push(Node::Label { name: t0.lexeme.clone(), line: t0.line, column: t0.column });
            pos = 2;
        }
    }

    if pos >= tokens.len() {
        return (nodes, None);
    }

    let head = &tokens[pos];
    match &head.kind {
        TokenKind::Instruction(kind) => match parse_operand_list(&tokens[pos + 1..]) {
            Ok(operands) => {
                nodes.push(Node::Instruction {
                    kind: *kind,
                    operands,
                    line: head.line,
                    column: head.column,
                });
                (nodes, None)
            }
            Err(err) => (nodes, Some(err)),
        },
        TokenKind::Directive => match parse_operand_list(&tokens[pos + 1..]) {
            Ok(operands) => {
                nodes.push(Node::Directive {
                    name: head.lexeme.clone(),
                    args: operands,
                    line: head.line,
                    column: head.column,
                });
                (nodes, None)
            }
            Err(err) => (nodes, Some(err)),
        },
        _ => (
            nodes,
            Some(ParseError::new(
                head.line,
                head.column,
                format!("expected instruction or directive, found '{}'", head.lexeme),
            )),
        ),
    }
}

fn parse_operand_list(tokens: &[Token]) -> Result<Vec<Operand>, ParseError> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut operands = Vec::new();
    let mut pos = 0;

    loop {
        let rest = &tokens[pos..];
        if rest.is_empty() {
            let last = tokens.last().unwrap();
            return Err(ParseError::new(last.line, last.column, "expected operand after ','"));
        }

        let (operand, consumed) = parse_operand(rest)?;
        operands.push(operand);
        pos += consumed;

        match tokens.get(pos) {
            None => break,
            Some(t) if t.kind == TokenKind::Comma => pos += 1,
            Some(t) => {
                return Err(ParseError::new(
                    t.line,
                    t.column,
                    format!("expected ',' or end of line, found '{}'", t.lexeme),
                ))
            }
        }
    }

    Ok(operands)
}

fn parse_operand(tokens: &[Token]) -> Result<(Operand, usize), ParseError> {
    let first = &tokens[0];

    match &first.kind {
        TokenKind::Register(reg) => Ok((Operand::Register(*reg), 1)),
        TokenKind::StringLiteral => Ok((Operand::StringLiteral(first.lexeme.clone()), 1)),
        TokenKind::Identifier => Ok((Operand::LabelRef(first.lexeme.clone()), 1)),
        TokenKind::Number | TokenKind::Minus | TokenKind::Plus => parse_immediate_or_offset(tokens),
        _ => Err(ParseError::new(
            first.line,
            first.column,
            format!("expected operand, found '{}'", first.lexeme),
        )),
    }
}

fn parse_immediate_or_offset(tokens: &[Token]) -> Result<(Operand, usize), ParseError> {
    let mut pos = 0;
    let mut sign = 1i64;

    match tokens[pos].kind {
        TokenKind::Minus => {
            sign = -1;
            pos += 1;
        }
        TokenKind::Plus => {
            pos += 1;
        }
        _ => {}
    }

    let number_tok = tokens
        .get(pos)
        .ok_or_else(|| ParseError::new(tokens[0].line, tokens[0].column, "expected number after sign"))?;

    if number_tok.kind != TokenKind::Number {
        return Err(ParseError::new(
            number_tok.line,
            number_tok.column,
            format!("expected number, found '{}'", number_tok.lexeme),
        ));
    }

    let value = sign
        * parse_number_literal(&number_tok.lexeme)
            .map_err(|msg| ParseError::new(number_tok.line, number_tok.column, msg))?;
    pos += 1;

    if let Some(t) = tokens.get(pos) {
        if t.kind == TokenKind::LParen {
            let base_tok = tokens
                .get(pos + 1)
                .ok_or_else(|| ParseError::new(t.line, t.column, "expected register after '('"))?;
            let base = match base_tok.kind {
                TokenKind::Register(reg) => reg,
                _ => {
                    return Err(ParseError::new(
                        base_tok.line,
                        base_tok.column,
                        format!("expected register, found '{}'", base_tok.lexeme),
                    ))
                }
            };
            let rparen_tok = tokens
                .get(pos + 2)
                .ok_or_else(|| ParseError::new(base_tok.line, base_tok.column, "expected ')' after register"))?;
            if rparen_tok.kind != TokenKind::RParen {
                return Err(ParseError::new(
                    rparen_tok.line,
                    rparen_tok.column,
                    format!("expected ')', found '{}'", rparen_tok.lexeme),
                ));
            }
            return Ok((Operand::Offset { imm: value, base }, pos + 3));
        }
    }

    Ok((Operand::Immediate(value), pos))
}

fn parse_number_literal(lexeme: &str) -> Result<i64, String> {
    if let Some(hex) = lexeme.strip_prefix("0x").or_else(|| lexeme.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).map_err(|_| format!("malformed hex literal '{}'", lexeme))
    } else if let Some(bin) = lexeme.strip_prefix("0b").or_else(|| lexeme.strip_prefix("0B")) {
        i64::from_str_radix(bin, 2).map_err(|_| format!("malformed binary literal '{}'", lexeme))
    } else {
        lexeme
            .parse::<i64>()
            .map_err(|_| format!("malformed number literal '{}'", lexeme))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};

    fn tok(kind: TokenKind, lexeme: &str, line: usize, column: usize) -> Token {
        Token::new(kind, lexeme, line, column)
    }

    #[test]
    fn empty_statement_produces_no_nodes() {
        let (nodes, err) = parse_statement(&[]);
        assert!(err.is_none());
        assert_eq!(nodes, vec![]);
    }

    #[test]
    fn parses_r_type_instruction() {
        let tokens = vec![
            tok(TokenKind::Instruction(InstructionKind::Add), "add", 1, 1),
            tok(TokenKind::Register(RegisterKind::X(1)), "x1", 1, 5),
            tok(TokenKind::Comma, ",", 1, 7),
            tok(TokenKind::Register(RegisterKind::X(2)), "x2", 1, 9),
            tok(TokenKind::Comma, ",", 1, 11),
            tok(TokenKind::Register(RegisterKind::X(3)), "x3", 1, 13),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(
            nodes,
            vec![Node::Instruction {
                kind: InstructionKind::Add,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(2)),
                    Operand::Register(RegisterKind::X(3)),
                ],
                line: 1,
                column: 1,
            }]
        );
    }

    #[test]
    fn parses_label_then_instruction_on_same_line() {
        let tokens = vec![
            tok(TokenKind::Identifier, "loop", 3, 1),
            tok(TokenKind::Colon, ":", 3, 5),
            tok(TokenKind::Instruction(InstructionKind::Jal), "jal", 3, 7),
            tok(TokenKind::Register(RegisterKind::X(1)), "x1", 3, 11),
            tok(TokenKind::Comma, ",", 3, 13),
            tok(TokenKind::Identifier, "loop", 3, 15),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(
            nodes,
            vec![
                Node::Label { name: "loop".into(), line: 3, column: 1 },
                Node::Instruction {
                    kind: InstructionKind::Jal,
                    operands: vec![
                        Operand::Register(RegisterKind::X(1)),
                        Operand::LabelRef("loop".into()),
                    ],
                    line: 3,
                    column: 7,
                },
            ]
        );
    }

    #[test]
    fn label_alone_produces_only_label_node() {
        let tokens = vec![
            tok(TokenKind::Identifier, "done", 9, 1),
            tok(TokenKind::Colon, ":", 9, 5),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(nodes, vec![Node::Label { name: "done".into(), line: 9, column: 1 }]);
    }

    #[test]
    fn parses_offset_operand() {
        let tokens = vec![
            tok(TokenKind::Instruction(InstructionKind::Lw), "lw", 2, 1),
            tok(TokenKind::Register(RegisterKind::X(5)), "x5", 2, 4),
            tok(TokenKind::Comma, ",", 2, 6),
            tok(TokenKind::Number, "4", 2, 8),
            tok(TokenKind::LParen, "(", 2, 9),
            tok(TokenKind::Register(RegisterKind::X(2)), "sp", 2, 10),
            tok(TokenKind::RParen, ")", 2, 12),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(
            nodes,
            vec![Node::Instruction {
                kind: InstructionKind::Lw,
                operands: vec![
                    Operand::Register(RegisterKind::X(5)),
                    Operand::Offset { imm: 4, base: RegisterKind::X(2) },
                ],
                line: 2,
                column: 1,
            }]
        );
    }

    #[test]
    fn parses_negative_immediate() {
        let tokens = vec![
            tok(TokenKind::Instruction(InstructionKind::Addi), "addi", 1, 1),
            tok(TokenKind::Register(RegisterKind::X(1)), "x1", 1, 6),
            tok(TokenKind::Comma, ",", 1, 8),
            tok(TokenKind::Register(RegisterKind::X(0)), "x0", 1, 10),
            tok(TokenKind::Comma, ",", 1, 12),
            tok(TokenKind::Minus, "-", 1, 14),
            tok(TokenKind::Number, "1", 1, 15),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(
            nodes,
            vec![Node::Instruction {
                kind: InstructionKind::Addi,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(0)),
                    Operand::Immediate(-1),
                ],
                line: 1,
                column: 1,
            }]
        );
    }

    #[test]
    fn parses_hex_immediate() {
        let tokens = vec![
            tok(TokenKind::Instruction(InstructionKind::Addi), "addi", 1, 1),
            tok(TokenKind::Register(RegisterKind::X(1)), "x1", 1, 6),
            tok(TokenKind::Comma, ",", 1, 8),
            tok(TokenKind::Register(RegisterKind::X(0)), "x0", 1, 10),
            tok(TokenKind::Comma, ",", 1, 12),
            tok(TokenKind::Number, "0x1F", 1, 14),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(
            nodes,
            vec![Node::Instruction {
                kind: InstructionKind::Addi,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(0)),
                    Operand::Immediate(31),
                ],
                line: 1,
                column: 1,
            }]
        );
    }

    #[test]
    fn parses_binary_immediate() {
        let tokens = vec![
            tok(TokenKind::Instruction(InstructionKind::Addi), "addi", 1, 1),
            tok(TokenKind::Register(RegisterKind::X(1)), "x1", 1, 6),
            tok(TokenKind::Comma, ",", 1, 8),
            tok(TokenKind::Register(RegisterKind::X(0)), "x0", 1, 10),
            tok(TokenKind::Comma, ",", 1, 12),
            tok(TokenKind::Number, "0b101010", 1, 14),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(
            nodes,
            vec![Node::Instruction {
                kind: InstructionKind::Addi,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(0)),
                    Operand::Immediate(42),
                ],
                line: 1,
                column: 1,
            }]
        );
    }

    #[test]
    fn parses_directive_with_string_arg() {
        let tokens = vec![
            tok(TokenKind::Directive, ".string", 5, 1),
            tok(TokenKind::StringLiteral, "hello", 5, 9),
        ];

        let (nodes, err) = parse_statement(&tokens);
        assert!(err.is_none());

        assert_eq!(
            nodes,
            vec![Node::Directive {
                name: ".string".into(),
                args: vec![Operand::StringLiteral("hello".into())],
                line: 5,
                column: 1,
            }]
        );
    }

    #[test]
    fn unexpected_token_at_start_of_statement_is_an_error() {
        let tokens = vec![tok(TokenKind::Comma, ",", 4, 1)];

        let (_, err) = parse_statement(&tokens);
        let err = err.unwrap();

        assert_eq!(err.line, 4);
        assert_eq!(err.column, 1);
    }

    #[test]
    fn trailing_comma_is_an_error() {
        let tokens = vec![
            tok(TokenKind::Instruction(InstructionKind::Add), "add", 1, 1),
            tok(TokenKind::Register(RegisterKind::X(1)), "x1", 1, 5),
            tok(TokenKind::Comma, ",", 1, 7),
        ];

        let (_, err) = parse_statement(&tokens);
        let err = err.unwrap();

        assert_eq!(err.message, "expected operand after ','");
    }

    #[test]
    fn label_is_preserved_when_instruction_on_same_line_fails_to_parse() {
        let tokens = vec![
            tok(TokenKind::Identifier, "loop", 3, 1),
            tok(TokenKind::Colon, ":", 3, 5),
            tok(TokenKind::Comma, ",", 3, 7),
        ];

        let (nodes, err) = parse_statement(&tokens);

        assert_eq!(nodes, vec![Node::Label { name: "loop".into(), line: 3, column: 1 }]);
        let err = err.unwrap();
        assert_eq!(err.line, 3);
        assert_eq!(err.column, 7);
    }
}
