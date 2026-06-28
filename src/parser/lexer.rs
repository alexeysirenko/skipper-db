use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Int(i64),
    Str(String),
    Star,
    Comma,
    Semicolon,
    LParen,
    RParen,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub pos: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub pos: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at position {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for ParseError {}

pub fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let start = i;
        let c = bytes[i];
        let kind = match c {
            _ if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'*' => single(&mut i, TokenKind::Star),
            b',' => single(&mut i, TokenKind::Comma),
            b';' => single(&mut i, TokenKind::Semicolon),
            b'(' => single(&mut i, TokenKind::LParen),
            b')' => single(&mut i, TokenKind::RParen),
            b'=' => single(&mut i, TokenKind::Eq),
            b'!' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    i += 2;
                    TokenKind::NotEq
                } else {
                    return Err(err(start, "expected '=' after '!'"));
                }
            }
            b'<' => two_or_one(&mut i, bytes, b'=', TokenKind::LtEq, TokenKind::Lt),
            b'>' => two_or_one(&mut i, bytes, b'=', TokenKind::GtEq, TokenKind::Gt),
            b'\'' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'\'' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(err(start, "unterminated string literal"));
                }
                let text = input[i + 1..j].to_string();
                i = j + 1;
                TokenKind::Str(text)
            }
            _ if c.is_ascii_digit() || (c == b'-' && next_is_digit(bytes, i)) => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                let value = input[i..j]
                    .parse::<i64>()
                    .map_err(|_| err(start, "integer literal out of range"))?;
                i = j;
                TokenKind::Int(value)
            }
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                let text = input[i..j].to_string();
                i = j;
                TokenKind::Ident(text)
            }
            _ => return Err(err(start, format!("unexpected character '{}'", c as char))),
        };
        tokens.push(Token { kind, pos: start });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        pos: input.len(),
    });
    Ok(tokens)
}

fn single(i: &mut usize, kind: TokenKind) -> TokenKind {
    *i += 1;
    kind
}

fn two_or_one(
    i: &mut usize,
    bytes: &[u8],
    second: u8,
    two: TokenKind,
    one: TokenKind,
) -> TokenKind {
    if bytes.get(*i + 1) == Some(&second) {
        *i += 2;
        two
    } else {
        *i += 1;
        one
    }
}

fn next_is_digit(bytes: &[u8], i: usize) -> bool {
    bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit())
}

fn err(pos: usize, message: impl Into<String>) -> ParseError {
    ParseError {
        message: message.into(),
        pos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn lexes_select() {
        assert_eq!(
            kinds("SELECT id, name FROM users"),
            vec![
                TokenKind::Ident("SELECT".to_string()),
                TokenKind::Ident("id".to_string()),
                TokenKind::Comma,
                TokenKind::Ident("name".to_string()),
                TokenKind::Ident("FROM".to_string()),
                TokenKind::Ident("users".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_operators_and_punctuation() {
        assert_eq!(
            kinds("(* , ; = != < <= > >=)"),
            vec![
                TokenKind::LParen,
                TokenKind::Star,
                TokenKind::Comma,
                TokenKind::Semicolon,
                TokenKind::Eq,
                TokenKind::NotEq,
                TokenKind::Lt,
                TokenKind::LtEq,
                TokenKind::Gt,
                TokenKind::GtEq,
                TokenKind::RParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_literals() {
        assert_eq!(
            kinds("42 -7 'Alice'"),
            vec![
                TokenKind::Int(42),
                TokenKind::Int(-7),
                TokenKind::Str("Alice".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tracks_positions() {
        let tokens = tokenize("a >= 1").unwrap();
        assert_eq!(tokens[0].pos, 0);
        assert_eq!(tokens[1].pos, 2);
        assert_eq!(tokens[2].pos, 5);
    }

    #[test]
    fn rejects_unterminated_string() {
        let e = tokenize("'oops").unwrap_err();
        assert_eq!(e.pos, 0);
        assert!(e.message.contains("unterminated"));
    }

    #[test]
    fn rejects_unexpected_character() {
        assert!(tokenize("a @ b").is_err());
    }
}
