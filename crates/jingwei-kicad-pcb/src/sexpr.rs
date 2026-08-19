use thiserror::Error;

/// Minimal typed S-expression used by the `KiCad` anti-corruption adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SExpr {
    /// Bare atom.
    Atom(String),
    /// Quoted string.
    String(String),
    /// Parenthesized list.
    List(Vec<SExpr>),
}

/// S-expression parse error.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SExprError {
    /// Input exceeds the configured parser budget.
    #[error("input is {actual} bytes, exceeding the {maximum} byte limit")]
    InputTooLarge {
        /// Observed bytes.
        actual: usize,
        /// Allowed bytes.
        maximum: usize,
    },
    /// Unexpected end of input.
    #[error("unexpected end of S-expression")]
    UnexpectedEnd,
    /// Unexpected token.
    #[error("unexpected token at byte {offset}: {token}")]
    UnexpectedToken {
        /// Byte offset.
        offset: usize,
        /// Token description.
        token: String,
    },
    /// String is missing its closing quote.
    #[error("unterminated string at byte {0}")]
    UnterminatedString(usize),
    /// Multiple top-level expressions were supplied.
    #[error("trailing data after top-level expression at byte {0}")]
    TrailingData(usize),
}

/// Parses exactly one bounded S-expression.
///
/// # Errors
///
/// Returns [`SExprError`] for oversized, truncated, malformed or trailing input.
pub fn parse_sexpr(input: &str, maximum_bytes: usize) -> Result<SExpr, SExprError> {
    if input.len() > maximum_bytes {
        return Err(SExprError::InputTooLarge {
            actual: input.len(),
            maximum: maximum_bytes,
        });
    }
    let mut parser = Parser {
        bytes: input.as_bytes(),
        cursor: 0,
    };
    parser.skip_space_and_comments();
    let expression = parser.parse_value()?;
    parser.skip_space_and_comments();
    if parser.cursor != parser.bytes.len() {
        return Err(SExprError::TrailingData(parser.cursor));
    }
    Ok(expression)
}

/// Deterministically serializes an S-expression.
pub fn serialize_sexpr(expression: &SExpr) -> String {
    fn write(expression: &SExpr, output: &mut String) {
        match expression {
            SExpr::Atom(value) => output.push_str(value),
            SExpr::String(value) => {
                output.push('"');
                for character in value.chars() {
                    match character {
                        '"' => output.push_str("\\\""),
                        '\\' => output.push_str("\\\\"),
                        '\n' => output.push_str("\\n"),
                        other => output.push(other),
                    }
                }
                output.push('"');
            }
            SExpr::List(values) => {
                output.push('(');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(' ');
                    }
                    write(value, output);
                }
                output.push(')');
            }
        }
    }
    let mut output = String::new();
    write(expression, &mut output);
    output
}

struct Parser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl Parser<'_> {
    fn parse_value(&mut self) -> Result<SExpr, SExprError> {
        self.skip_space_and_comments();
        match self.bytes.get(self.cursor).copied() {
            Some(b'(') => self.parse_list(),
            Some(b'"') => self.parse_string(),
            Some(b')') => Err(SExprError::UnexpectedToken {
                offset: self.cursor,
                token: ")".to_owned(),
            }),
            Some(_) => self.parse_atom(),
            None => Err(SExprError::UnexpectedEnd),
        }
    }

    fn parse_list(&mut self) -> Result<SExpr, SExprError> {
        self.cursor += 1;
        let mut values = Vec::new();
        loop {
            self.skip_space_and_comments();
            match self.bytes.get(self.cursor).copied() {
                Some(b')') => {
                    self.cursor += 1;
                    return Ok(SExpr::List(values));
                }
                Some(_) => values.push(self.parse_value()?),
                None => return Err(SExprError::UnexpectedEnd),
            }
        }
    }

    fn parse_string(&mut self) -> Result<SExpr, SExprError> {
        let start = self.cursor;
        self.cursor += 1;
        let mut value = String::new();
        while let Some(byte) = self.bytes.get(self.cursor).copied() {
            self.cursor += 1;
            match byte {
                b'"' => return Ok(SExpr::String(value)),
                b'\\' => {
                    let escaped = self
                        .bytes
                        .get(self.cursor)
                        .copied()
                        .ok_or(SExprError::UnterminatedString(start))?;
                    self.cursor += 1;
                    value.push(match escaped {
                        b'n' => '\n',
                        b'"' => '"',
                        b'\\' => '\\',
                        other => char::from(other),
                    });
                }
                other => value.push(char::from(other)),
            }
        }
        Err(SExprError::UnterminatedString(start))
    }

    fn parse_atom(&mut self) -> Result<SExpr, SExprError> {
        let start = self.cursor;
        while let Some(byte) = self.bytes.get(self.cursor).copied() {
            if byte.is_ascii_whitespace() || matches!(byte, b'(' | b')' | b';') {
                break;
            }
            self.cursor += 1;
        }
        if self.cursor == start {
            return Err(SExprError::UnexpectedToken {
                offset: start,
                token: char::from(self.bytes[start]).to_string(),
            });
        }
        let value = String::from_utf8_lossy(&self.bytes[start..self.cursor]).into_owned();
        Ok(SExpr::Atom(value))
    }

    fn skip_space_and_comments(&mut self) {
        loop {
            while self
                .bytes
                .get(self.cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.cursor += 1;
            }
            if self.bytes.get(self.cursor) == Some(&b';') {
                while self
                    .bytes
                    .get(self.cursor)
                    .is_some_and(|byte| *byte != b'\n')
                {
                    self.cursor += 1;
                }
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_comments_round_trip() {
        let parsed = parse_sexpr("; fixture\n(root \"a\\\"b\" (child 1))", 1024).unwrap();
        assert_eq!(serialize_sexpr(&parsed), "(root \"a\\\"b\" (child 1))");
    }
}
