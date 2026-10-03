//! A splitter for fibber source text: the nesting of lists, vectors and
//! maps, with the byte span of every form.
//!
//! It is not the reader and shares no code with it. The reader is the
//! oracle of `spec/bootstrap.md`; a mutation tool built on it could not
//! tell a fault of the reader from a fault of the tool. What the splitter
//! takes from `spec/syntax.md` section 1: strings with escapes, `\c`
//! character literals, `;` comments, the `#_` form comment, the reader
//! prefixes, and the alphabet of a symbol. It checks nothing else (a
//! number is just an atom here); a text it cannot split is an error and
//! its module is not mutated.

use std::fmt;

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    List,
    Vector,
    Map,
    /// A symbol, keyword, number, boolean or `nil`: a run of symbol characters.
    Atom,
    Str,
    /// A `\c` literal, `\newline` and the like.
    Char,
    /// A reader prefix (`'` `` ` `` `~` `~@` `@`) and the one form it applies to.
    Prefix,
}

/// One form and where it is: `start..end` are byte offsets into the
/// source, `kids` the forms inside a list, vector or map, or the one form
/// a prefix applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub kids: Vec<Node>,
}

impl Node {
    /// The source text of the form.
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }

    /// The text of an atom, `None` for any other node.
    pub fn atom<'a>(&self, src: &'a str) -> Option<&'a str> {
        (self.kind == Kind::Atom).then(|| self.text(src))
    }

    /// The first atom of a list: `if` in `(if c a b)`.
    pub fn head<'a>(&self, src: &'a str) -> Option<&'a str> {
        match self.kind {
            Kind::List => self.kids.first()?.atom(src),
            _ => None,
        }
    }
}

/// Why a text could not be split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "byte {}: {}", self.offset, self.message)
    }
}

/// The reader's own limit (`spec/syntax.md` 1.6) is a thousand; this one is
/// the same, so a text the reader accepts is never too deep here.
const MAX_DEPTH: usize = 1000;

/// The 1-based line of a byte offset.
pub fn line_of(src: &str, offset: usize) -> usize {
    1 + src.as_bytes()[..offset.min(src.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Splits `src` into its top-level forms.
pub fn parse(src: &str) -> Result<Vec<Node>, ParseError> {
    let mut p = Parser {
        b: src.as_bytes(),
        pos: 0,
    };
    let mut forms = Vec::new();
    loop {
        p.skip(0)?;
        if p.pos >= p.b.len() {
            return Ok(forms);
        }
        forms.push(p.form(0)?);
    }
}

struct Parser<'a> {
    b: &'a [u8],
    pos: usize,
}

fn fail<T>(offset: usize, message: &'static str) -> Result<T, ParseError> {
    Err(ParseError { offset, message })
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn is_closer(b: u8) -> bool {
    matches!(b, b')' | b']' | b'}')
}

/// A byte that ends a symbol: white space and the characters `spec/syntax.md` 1.1
/// leaves out of the alphabet. A byte of a multi-byte character is never one.
fn is_delimiter(b: u8) -> bool {
    is_space(b)
        || is_closer(b)
        || matches!(
            b,
            b'(' | b'[' | b'{' | b'"' | b';' | b'\'' | b'`' | b',' | b'@' | b'~' | b'\\'
        )
}

impl Parser<'_> {
    fn peek(&self, at: usize) -> Option<u8> {
        self.b.get(at).copied()
    }

    /// Skips white space, comments, white-space commas and `#_ form`.
    fn skip(&mut self, depth: usize) -> Result<(), ParseError> {
        loop {
            match self.peek(self.pos) {
                Some(c) if is_space(c) => self.pos += 1,
                Some(b';') => {
                    while self.peek(self.pos).is_some_and(|c| c != b'\n') {
                        self.pos += 1;
                    }
                }
                Some(b',') => self.pos += 1,
                Some(b'#') if self.peek(self.pos + 1) == Some(b'_') => {
                    self.pos += 2;
                    self.skip(depth + 1)?;
                    self.form(depth + 1)?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn form(&mut self, depth: usize) -> Result<Node, ParseError> {
        if depth > MAX_DEPTH {
            return fail(self.pos, "forms nest too deeply");
        }
        let Some(c) = self.peek(self.pos) else {
            return fail(self.pos, "a form was expected");
        };
        match c {
            b'(' => self.sequence(Kind::List, b')', depth),
            b'[' => self.sequence(Kind::Vector, b']', depth),
            b'{' => self.sequence(Kind::Map, b'}', depth),
            c if is_closer(c) => fail(self.pos, "a closer with no opener"),
            b'"' => self.string(),
            b'\\' => self.character(),
            b'\'' | b'`' | b'@' | b'~' => self.prefix(depth),
            _ => self.atom(),
        }
    }

    fn sequence(&mut self, kind: Kind, close: u8, depth: usize) -> Result<Node, ParseError> {
        let start = self.pos;
        self.pos += 1;
        let mut kids = Vec::new();
        loop {
            self.skip(depth + 1)?;
            match self.peek(self.pos) {
                None => return fail(start, "an opener is never closed"),
                Some(c) if c == close => {
                    self.pos += 1;
                    return Ok(Node {
                        kind,
                        start,
                        end: self.pos,
                        kids,
                    });
                }
                Some(c) if is_closer(c) => return fail(self.pos, "a closer of the wrong kind"),
                Some(_) => kids.push(self.form(depth + 1)?),
            }
        }
    }

    fn prefix(&mut self, depth: usize) -> Result<Node, ParseError> {
        let start = self.pos;
        let spliced = self.peek(start) == Some(b'~') && self.peek(start + 1) == Some(b'@');
        self.pos += if spliced { 2 } else { 1 };
        self.skip(depth + 1)?;
        let kid = self.form(depth + 1)?;
        Ok(Node {
            kind: Kind::Prefix,
            start,
            end: kid.end,
            kids: vec![kid],
        })
    }

    fn string(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek(self.pos) {
                None => return fail(start, "a string is never closed"),
                Some(b'\\') => self.pos += 2,
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(Node {
                        kind: Kind::Str,
                        start,
                        end: self.pos,
                        kids: Vec::new(),
                    });
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    /// `\` and one character, or the whole run of symbol characters when the
    /// first one can continue a symbol (`\newline`, `\u{41}`).
    fn character(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        let Some(first) = self.peek(start + 1) else {
            return fail(start, "a character literal has no character");
        };
        self.pos = start + 1 + utf8_len(first);
        if !is_delimiter(first) {
            while self.peek(self.pos).is_some_and(|c| !is_delimiter(c)) {
                self.pos += 1;
            }
        }
        Ok(Node {
            kind: Kind::Char,
            start,
            end: self.pos.min(self.b.len()),
            kids: Vec::new(),
        })
    }

    fn atom(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        while self.peek(self.pos).is_some_and(|c| !is_delimiter(c)) {
            self.pos += 1;
        }
        if self.pos == start {
            return fail(start, "a character that starts no form");
        }
        Ok(Node {
            kind: Kind::Atom,
            start,
            end: self.pos,
            kids: Vec::new(),
        })
    }
}

/// The length in bytes of the UTF-8 character that begins with `lead`.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests;
