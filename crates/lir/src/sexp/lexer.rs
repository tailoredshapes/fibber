//! Tokens: parentheses, braces, brackets, strings, vector types and atoms.

use crate::diag::{err, Pos, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Open,
    Close,
    BraceOpen,
    BraceClose,
    BracketOpen,
    BracketClose,
    Atom(String),
    Str(Vec<u8>),
    VecType(String, String),
}

/// Split `src` into tokens with their positions.
pub fn tokens(src: &str) -> Result<Vec<(Tok, Pos)>> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        i: 0,
        pos: Pos { line: 1, col: 1 },
    };
    let mut out = Vec::new();
    while let Some(t) = lx.next()? {
        out.push(t);
    }
    Ok(out)
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    pos: Pos,
}

fn is_delim(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '{' | '}' | '[' | ']' | '"' | ';' | ',')
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        if c == '\n' {
            self.pos.line += 1;
            self.pos.col = 1;
        } else {
            self.pos.col += 1;
        }
        Some(c)
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek() {
            if c == ';' {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.bump();
                }
            } else if c.is_whitespace() || c == ',' {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn next(&mut self) -> Result<Option<(Tok, Pos)>> {
        self.skip_space();
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        let tok = match c {
            '(' => self.single(Tok::Open),
            ')' => self.single(Tok::Close),
            '{' => self.single(Tok::BraceOpen),
            '}' => self.single(Tok::BraceClose),
            '[' => self.single(Tok::BracketOpen),
            ']' => self.single(Tok::BracketClose),
            '"' => Tok::Str(self.string(start)?),
            '<' => self.vector(start)?,
            _ => Tok::Atom(self.atom()),
        };
        Ok(Some((tok, start)))
    }

    fn single(&mut self, t: Tok) -> Tok {
        self.bump();
        t
    }

    fn atom(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek().filter(|&c| !is_delim(c)) {
            s.push(c);
            self.bump();
        }
        s
    }

    fn string(&mut self, start: Pos) -> Result<Vec<u8>> {
        self.bump();
        let mut out = Vec::new();
        loop {
            let at = self.pos;
            match self.bump() {
                None => return err(start, "unterminated string"),
                Some('"') => return Ok(out),
                Some('\\') => out.push(self.escape(at)?),
                Some(c) => {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
    }

    fn escape(&mut self, at: Pos) -> Result<u8> {
        let c = self.bump();
        Ok(match c {
            Some('n') => b'\n',
            Some('t') => b'\t',
            Some('r') => b'\r',
            Some('0') => 0,
            Some('\\') => b'\\',
            Some('"') => b'"',
            Some('x') => return self.hex_escape(at),
            Some(c) => return err(at, format!("invalid escape \\{}", c.escape_debug())),
            None => return err(at, "unterminated string"),
        })
    }

    fn hex_escape(&mut self, at: Pos) -> Result<u8> {
        let hi = self.bump().and_then(|c| c.to_digit(16));
        let lo = self.bump().and_then(|c| c.to_digit(16));
        match (hi, lo) {
            (Some(h), Some(l)) => Ok((h * 16 + l) as u8),
            _ => err(at, "invalid escape \\x: two hex digits expected"),
        }
    }

    /// `<N x T>` as one token.
    fn vector(&mut self, start: Pos) -> Result<Tok> {
        self.bump();
        let mut body = String::new();
        loop {
            match self.bump() {
                Some('>') => break,
                Some(c) if c == '\n' || c == '(' || c == ')' => {
                    return err(start, "unterminated vector type")
                }
                Some(c) => body.push(c),
                None => return err(start, "unterminated vector type"),
            }
        }
        let parts: Vec<&str> = body.split_whitespace().collect();
        match parts.as_slice() {
            [n, "x", e] => Ok(Tok::VecType(n.to_string(), e.to_string())),
            _ => err(start, format!("malformed vector type <{body}>")),
        }
    }
}
