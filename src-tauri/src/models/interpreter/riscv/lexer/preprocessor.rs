use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use crate::models::interpreter::error::LexError;

struct BlockCommentState {
    active: bool,
    start_line: usize,
    start_col: usize,
}

pub fn preprocess(source_path: &Path, out_path: &Path) -> io::Result<Vec<LexError>> {
    let reader = BufReader::new(File::open(source_path)?);
    let mut writer = BufWriter::new(File::create(out_path)?);

    let mut errors = Vec::new();
    let mut block = BlockCommentState {
        active: false,
        start_line: 0,
        start_col: 0,
    };

    for (idx, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line_no = idx + 1;
        let cleaned = process_line(&line, line_no, &mut block, &mut errors);
        writeln!(writer, "{}", cleaned)?;
    }

    if block.active {
        errors.push(LexError::new(
            block.start_line,
            block.start_col,
            "unterminated block comment",
        ));
    }

    writer.flush()?;
    Ok(errors)
}

fn process_line(
    line: &str,
    line_no: usize,
    block: &mut BlockCommentState,
    errors: &mut Vec<LexError>,
) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut kept = String::new();
    let mut in_string = false;

    while i < chars.len() {
        let col = i + 1;

        if block.active {
            if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                block.active = false;
                i += 2;
                // Insert a space so the words on either side of the removed
                // comment don't accidentally get joined together (so
                // `a/*c*/b` becomes `a b`, not `ab`).
                kept.push(' ');
                continue;
            }
            i += 1;
            continue;
        }

        if in_string {
            kept.push(chars[i]);
            if chars[i] == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if chars[i] == '"' {
            in_string = true;
            kept.push(chars[i]);
            i += 1;
            continue;
        }

        if chars[i] == '#' {
            break;
        }

        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            block.active = true;
            block.start_line = line_no;
            block.start_col = col;
            i += 2;
            continue;
        }

        kept.push(chars[i]);
        i += 1;
    }

    let trimmed = kept.trim_start();
    if trimmed.starts_with(".include") {
        let after = &trimmed[".include".len()..];
        let is_directive_boundary = after
            .chars()
            .next()
            .map_or(true, |c| !(c.is_alphanumeric() || c == '_'));
        if is_directive_boundary {
            let leading_ws = kept.len() - trimmed.len();
            errors.push(LexError::new(
                line_no,
                leading_ws + 1,
                "'.include' is not supported: only a single file is allowed",
            ));
        }
    }

    normalize_whitespace_outside_strings(&kept)
}

/// Collapses runs of spaces in the middle of a line down to a single space.
/// The leading whitespace at the start of the line is kept exactly as-is
/// (so error positions still line up with what the user sees in the
/// editor), and whitespace inside `"..."` text is left untouched so string
/// contents aren't changed.
fn normalize_whitespace_outside_strings(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let leading_len = chars.iter().take_while(|c| c.is_whitespace()).count();
    let leading: String = chars[..leading_len].iter().collect();

    let mut rest = String::new();
    let mut in_string = false;
    let mut prev_was_space = false;

    for &c in &chars[leading_len..] {
        if in_string {
            rest.push(c);
            if c == '"' {
                in_string = false;
            }
            prev_was_space = false;
            continue;
        }

        if c == '"' {
            in_string = true;
            rest.push(c);
            prev_was_space = false;
            continue;
        }

        if c.is_whitespace() {
            if !prev_was_space {
                rest.push(' ');
                prev_was_space = true;
            }
            continue;
        }

        rest.push(c);
        prev_was_space = false;
    }

    if !in_string {
        while rest.ends_with(' ') {
            rest.pop();
        }
    }

    format!("{}{}", leading, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;

    fn run_preprocess(input: &str) -> (String, Vec<LexError>) {
        let dir = tempdir().unwrap();
        let source_path = dir.path().join("source.s");
        let out_path = dir.path().join("preprocessed.s");
        std::fs::write(&source_path, input).unwrap();

        let errors = preprocess(&source_path, &out_path).unwrap();

        let mut output = String::new();
        File::open(&out_path)
            .unwrap()
            .read_to_string(&mut output)
            .unwrap();

        (output, errors)
    }

    #[test]
    fn strips_line_comments_and_preserves_line_count() {
        let (output, errors) = run_preprocess("add x1, x2, x3 # sum\n\nsub x1, x2, x3\n");
        assert_eq!(output, "add x1, x2, x3\n\nsub x1, x2, x3\n");
        assert!(errors.is_empty());
    }

    #[test]
    fn strips_multi_line_block_comments() {
        let (output, errors) = run_preprocess(
            "add x1, x2, x3 /* start\nstill inside\nend */ sub x4, x5, x6\n",
        );
        // The closing `*/` on line 3 is right next to the space that
        // separated it from `sub`. The extra space we insert to avoid
        // merging words ends up counted as leading whitespace on that line,
        // since everything before it was inside the comment, so two leading
        // spaces end up kept instead of being trimmed away.
        assert_eq!(output, "add x1, x2, x3\n\n  sub x4, x5, x6\n");
        assert!(errors.is_empty());
    }

    #[test]
    fn collapses_interior_whitespace_but_preserves_leading_indentation_length() {
        let (output, _errors) = run_preprocess("    add   x1,  x2,   x3   \n");
        assert_eq!(output, "    add x1, x2, x3\n");
    }

    #[test]
    fn does_not_treat_hash_inside_string_literal_as_comment() {
        let (output, errors) = run_preprocess(".asciz \"a # b\"\n");
        assert_eq!(output, ".asciz \"a # b\"\n");
        assert!(errors.is_empty());
    }

    #[test]
    fn does_not_collapse_whitespace_inside_string_literal() {
        let (output, _errors) = run_preprocess(".asciz \"hello   world\"\n");
        assert_eq!(output, ".asciz \"hello   world\"\n");
    }

    #[test]
    fn reports_unterminated_block_comment() {
        let (_output, errors) = run_preprocess("add x1, x2, x3\n/* never closed\n");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 2);
        assert_eq!(errors[0].column, 1);
    }

    #[test]
    fn reports_include_directive_as_error() {
        let (_output, errors) = run_preprocess(".include \"other.s\"\n");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 1);
        assert_eq!(errors[0].column, 1);
    }
}
