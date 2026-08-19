//! Defensive hand-written parser with explicit input, token, and nesting limits.

use crate::{
    AstValue, Diagnostic, DiagnosticSeverity, Item, JDL_AST_VERSION, JdlDocument, SourceSpan,
};

/// Parser resource limits for untrusted JDL input.
#[derive(Clone, Copy, Debug)]
pub struct ParseLimits {
    pub max_source_bytes: usize,
    pub max_tokens: usize,
    pub max_depth: usize,
    pub max_items: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 1024 * 1024,
            max_tokens: 100_000,
            max_depth: 32,
            max_items: 20_000,
        }
    }
}

/// One or more syntax diagnostics.
#[derive(Clone, Debug, thiserror::Error)]
#[error("JDL parse failed with {diagnostics_len} diagnostic(s)")]
pub struct ParseError {
    pub diagnostics: Vec<Diagnostic>,
    diagnostics_len: usize,
}

impl ParseError {
    fn one(source_name: &str, code: &str, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            diagnostics: vec![Diagnostic {
                severity: DiagnosticSeverity::Error,
                code: code.into(),
                message: message.into(),
                source_name: source_name.into(),
                span,
            }],
            diagnostics_len: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Word(String),
    String(String),
    LBrace,
    RBrace,
    Colon,
    Equal,
    Semicolon,
    Arrow,
    Dot,
    Comma,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    span: SourceSpan,
}

/// Parses one complete `project` document.
pub fn parse(
    source_name: impl Into<String>,
    source: &str,
    limits: ParseLimits,
) -> Result<JdlDocument, ParseError> {
    let source_name = source_name.into();
    if source.len() > limits.max_source_bytes {
        return Err(ParseError::one(
            &source_name,
            "JDL_SOURCE_TOO_LARGE",
            format!("source exceeds {} bytes", limits.max_source_bytes),
            SourceSpan {
                start: 0,
                end: source.len(),
            },
        ));
    }
    let tokens = lex(&source_name, source, limits.max_tokens)?;
    Parser {
        source_name: &source_name,
        source,
        tokens,
        position: 0,
        limits,
        item_count: 0,
    }
    .document()
}

struct Parser<'a> {
    source_name: &'a str,
    source: &'a str,
    tokens: Vec<Token>,
    position: usize,
    limits: ParseLimits,
    item_count: usize,
}

impl Parser<'_> {
    fn document(mut self) -> Result<JdlDocument, ParseError> {
        let start = self.expect_word_value("project")?.span.start;
        let project_name = self.expect_word()?.0;
        self.expect(TokenKind::LBrace)?;
        let items = self.items(1)?;
        let end = self.expect(TokenKind::RBrace)?.span.end;
        if let Some(token) = self.peek() {
            return Err(self.error(
                "JDL_TRAILING_INPUT",
                "unexpected input after project",
                token.span,
            ));
        }
        Ok(JdlDocument {
            ast_version: JDL_AST_VERSION.into(),
            source_name: self.source_name.into(),
            project_name,
            items,
            span: SourceSpan { start, end },
        })
    }

    fn items(&mut self, depth: usize) -> Result<Vec<Item>, ParseError> {
        if depth > self.limits.max_depth {
            let span = self
                .peek()
                .map_or(SourceSpan::default(), |token| token.span);
            return Err(self.error(
                "JDL_NESTING_LIMIT",
                "maximum declaration depth exceeded",
                span,
            ));
        }
        let mut items = Vec::new();
        while !matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::RBrace) | None
        ) {
            self.item_count += 1;
            if self.item_count > self.limits.max_items {
                let span = self
                    .peek()
                    .map_or(SourceSpan::default(), |token| token.span);
                return Err(self.error(
                    "JDL_ITEM_LIMIT",
                    "maximum declaration count exceeded",
                    span,
                ));
            }
            items.push(self.item(depth)?);
        }
        Ok(items)
    }

    fn item(&mut self, depth: usize) -> Result<Item, ParseError> {
        let keyword = self.expect_word()?;
        match keyword.0.as_str() {
            "module" | "block" => {
                let name = self.expect_word()?.0;
                self.expect(TokenKind::LBrace)?;
                let items = self.items(depth + 1)?;
                let end = self.expect(TokenKind::RBrace)?.span.end;
                let span = SourceSpan {
                    start: keyword.1.start,
                    end,
                };
                Ok(if keyword.0 == "module" {
                    Item::Module { name, items, span }
                } else {
                    Item::Block { name, items, span }
                })
            }
            "interface" => {
                let (name, _) = self.expect_word()?;
                let end = self.expect(TokenKind::Semicolon)?.span.end;
                Ok(Item::Interface {
                    name,
                    span: SourceSpan {
                        start: keyword.1.start,
                        end,
                    },
                })
            }
            "component" => {
                let (name, _) = self.expect_word()?;
                let component_type = if self.consume(TokenKind::Colon).is_some() {
                    Some(self.expect_word_or_string()?.0)
                } else {
                    None
                };
                let end = self.expect(TokenKind::Semicolon)?.span.end;
                Ok(Item::Component {
                    name,
                    component_type,
                    span: SourceSpan {
                        start: keyword.1.start,
                        end,
                    },
                })
            }
            "net" => {
                let (name, _) = self.expect_word()?;
                let end = self.expect(TokenKind::Semicolon)?.span.end;
                Ok(Item::Net {
                    name,
                    span: SourceSpan {
                        start: keyword.1.start,
                        end,
                    },
                })
            }
            "property" => {
                let (name, _) = self.expect_word()?;
                self.expect(TokenKind::Colon)?;
                let (property_type, _) = self.expect_word()?;
                self.expect(TokenKind::Equal)?;
                let value = self.value()?;
                let end = self.expect(TokenKind::Semicolon)?.span.end;
                Ok(Item::Property {
                    name,
                    property_type,
                    value,
                    span: SourceSpan {
                        start: keyword.1.start,
                        end,
                    },
                })
            }
            "connect" => {
                let from = self.path()?;
                self.expect(TokenKind::Arrow)?;
                let to = self.path()?;
                let end = self.expect(TokenKind::Semicolon)?.span.end;
                Ok(Item::Connect {
                    from,
                    to,
                    span: SourceSpan {
                        start: keyword.1.start,
                        end,
                    },
                })
            }
            "constraint" => {
                let (name, _) = self.expect_word()?;
                self.expect(TokenKind::Equal)?;
                let (expression, end) = self.expression_until_semicolon()?;
                Ok(Item::Constraint {
                    name,
                    expression,
                    span: SourceSpan {
                        start: keyword.1.start,
                        end,
                    },
                })
            }
            "assert" => {
                let (expression, end) = self.expression_until_semicolon()?;
                Ok(Item::Assertion {
                    expression,
                    span: SourceSpan {
                        start: keyword.1.start,
                        end,
                    },
                })
            }
            other => Err(self.error(
                "JDL_UNKNOWN_DECLARATION",
                format!("unknown declaration `{other}`"),
                keyword.1,
            )),
        }
    }

    fn value(&mut self) -> Result<AstValue, ParseError> {
        let token = self
            .next()
            .ok_or_else(|| self.eof("expected property value"))?;
        match token.kind {
            TokenKind::String(value) => Ok(AstValue::String(value)),
            TokenKind::Word(value) if value == "true" => Ok(AstValue::Boolean(true)),
            TokenKind::Word(value) if value == "false" => Ok(AstValue::Boolean(false)),
            TokenKind::Word(value) if begins_numeric(&value) => Ok(AstValue::Quantity(value)),
            TokenKind::Word(value) => Ok(AstValue::Identifier(value)),
            _ => Err(self.error("JDL_EXPECTED_VALUE", "expected a literal value", token.span)),
        }
    }

    fn path(&mut self) -> Result<String, ParseError> {
        let mut path = self.expect_word()?.0;
        while self.consume(TokenKind::Dot).is_some() {
            path.push('.');
            path.push_str(&self.expect_word()?.0);
        }
        Ok(path)
    }

    fn expression_until_semicolon(&mut self) -> Result<(String, usize), ParseError> {
        let start = self
            .peek()
            .ok_or_else(|| self.eof("expected expression"))?
            .span
            .start;
        let mut end = start;
        while let Some(token) = self.peek().cloned() {
            if token.kind == TokenKind::Semicolon {
                let semicolon = self.next().expect("peeked token");
                let expression = self.source[start..end].trim().to_owned();
                if expression.is_empty() {
                    return Err(self.error(
                        "JDL_EMPTY_EXPRESSION",
                        "expression cannot be empty",
                        token.span,
                    ));
                }
                return Ok((expression, semicolon.span.end));
            }
            end = token.span.end;
            self.position += 1;
        }
        Err(self.eof("expected `;` after expression"))
    }

    fn expect_word(&mut self) -> Result<(String, SourceSpan), ParseError> {
        let token = self.next().ok_or_else(|| self.eof("expected identifier"))?;
        if let TokenKind::Word(value) = token.kind {
            Ok((value, token.span))
        } else {
            Err(self.error("JDL_EXPECTED_IDENTIFIER", "expected identifier", token.span))
        }
    }

    fn expect_word_or_string(&mut self) -> Result<(String, SourceSpan), ParseError> {
        let token = self
            .next()
            .ok_or_else(|| self.eof("expected identifier or string"))?;
        match token.kind {
            TokenKind::Word(value) | TokenKind::String(value) => Ok((value, token.span)),
            _ => Err(self.error(
                "JDL_EXPECTED_TYPE",
                "expected identifier or string",
                token.span,
            )),
        }
    }

    fn expect_word_value(&mut self, expected: &str) -> Result<Token, ParseError> {
        let token = self
            .next()
            .ok_or_else(|| self.eof(&format!("expected `{expected}`")))?;
        if token.kind == TokenKind::Word(expected.into()) {
            Ok(token)
        } else {
            Err(self.error(
                "JDL_EXPECTED_KEYWORD",
                format!("expected `{expected}`"),
                token.span,
            ))
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        let token = self
            .next()
            .ok_or_else(|| self.eof("unexpected end of input"))?;
        if token.kind == expected {
            Ok(token)
        } else {
            Err(self.error(
                "JDL_UNEXPECTED_TOKEN",
                format!("expected {expected:?}"),
                token.span,
            ))
        }
    }

    fn consume(&mut self, expected: TokenKind) -> Option<Token> {
        if self.peek().is_some_and(|token| token.kind == expected) {
            self.next()
        } else {
            None
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        self.position += usize::from(token.is_some());
        token
    }

    fn eof(&self, message: &str) -> ParseError {
        self.error(
            "JDL_UNEXPECTED_EOF",
            message,
            SourceSpan {
                start: self.source.len(),
                end: self.source.len(),
            },
        )
    }

    fn error(&self, code: &str, message: impl Into<String>, span: SourceSpan) -> ParseError {
        ParseError::one(self.source_name, code, message, span)
    }
}

fn lex(source_name: &str, source: &str, max_tokens: usize) -> Result<Vec<Token>, ParseError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        let start = index;
        let kind = match bytes[index] {
            b'{' => {
                index += 1;
                TokenKind::LBrace
            }
            b'}' => {
                index += 1;
                TokenKind::RBrace
            }
            b':' => {
                index += 1;
                TokenKind::Colon
            }
            b'=' => {
                index += 1;
                TokenKind::Equal
            }
            b';' => {
                index += 1;
                TokenKind::Semicolon
            }
            b'.' => {
                index += 1;
                TokenKind::Dot
            }
            b',' => {
                index += 1;
                TokenKind::Comma
            }
            b'-' if bytes.get(index + 1) == Some(&b'>') => {
                index += 2;
                TokenKind::Arrow
            }
            b'"' => {
                index += 1;
                let mut value = String::new();
                let mut terminated = false;
                while index < bytes.len() {
                    match bytes[index] {
                        b'"' => {
                            index += 1;
                            terminated = true;
                            break;
                        }
                        b'\\' => {
                            index += 1;
                            let escaped = *bytes.get(index).ok_or_else(|| {
                                ParseError::one(
                                    source_name,
                                    "JDL_UNTERMINATED_STRING",
                                    "unterminated string escape",
                                    SourceSpan {
                                        start,
                                        end: source.len(),
                                    },
                                )
                            })?;
                            let character = match escaped {
                                b'n' => '\n',
                                b'r' => '\r',
                                b't' => '\t',
                                b'"' => '"',
                                b'\\' => '\\',
                                _ => {
                                    return Err(ParseError::one(
                                        source_name,
                                        "JDL_INVALID_ESCAPE",
                                        "only n, r, t, quote and backslash escapes are allowed",
                                        SourceSpan {
                                            start: index - 1,
                                            end: index + 1,
                                        },
                                    ));
                                }
                            };
                            value.push(character);
                            index += 1;
                        }
                        byte if byte.is_ascii() => {
                            value.push(char::from(byte));
                            index += 1;
                        }
                        _ => {
                            let remainder = &source[index..];
                            let character = remainder.chars().next().expect("valid UTF-8 source");
                            value.push(character);
                            index += character.len_utf8();
                        }
                    }
                }
                if !terminated {
                    return Err(ParseError::one(
                        source_name,
                        "JDL_UNTERMINATED_STRING",
                        "unterminated string",
                        SourceSpan {
                            start,
                            end: source.len(),
                        },
                    ));
                }
                TokenKind::String(value)
            }
            byte if byte.is_ascii_digit() => {
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || matches!(bytes[index], b'.' | b'_' | b'-' | b'+' | b'%'))
                {
                    index += 1;
                }
                TokenKind::Word(source[start..index].into())
            }
            byte if is_word_byte(byte) => {
                index += 1;
                while index < bytes.len() && is_word_byte(bytes[index]) {
                    index += 1;
                }
                TokenKind::Word(source[start..index].into())
            }
            _ => {
                return Err(ParseError::one(
                    source_name,
                    "JDL_INVALID_CHARACTER",
                    format!("invalid character at byte {index}"),
                    SourceSpan {
                        start,
                        end: start + 1,
                    },
                ));
            }
        };
        tokens.push(Token {
            kind,
            span: SourceSpan { start, end: index },
        });
        if tokens.len() > max_tokens {
            return Err(ParseError::one(
                source_name,
                "JDL_TOKEN_LIMIT",
                format!("source exceeds {max_tokens} tokens"),
                SourceSpan {
                    start: 0,
                    end: source.len(),
                },
            ));
        }
    }
    Ok(tokens)
}

const fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'%' | b'<' | b'>')
}

fn begins_numeric(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_digit)
}
