//! Tokens (spec/syntax.md §1.1, §1.2): delimiters, prefix reader macros,
//! the form comment `#_`, and atoms, which are complete forms already.
//! Whitespace, commas and `;` comments are skipped here.

use super::chars::{is_constituent, is_forbidden, is_separator, starts_form_char, valid_slashes};
use super::cursor::{Cursor, Mark};
use super::error::{ReadError, ReadErrorKind};
use super::form::{Form, FormKind};
use super::literal::{lex_char, lex_string};
use super::number::parse_number;
use super::pos::Pos;

/// The three bracket pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delim {
    Paren,
    Bracket,
    Brace,
}

impl Delim {
    pub(crate) fn open(self) -> char {
        match self {
            Delim::Paren => '(',
            Delim::Bracket => '[',
            Delim::Brace => '{',
        }
    }

    pub(crate) fn close(self) -> char {
        match self {
            Delim::Paren => ')',
            Delim::Bracket => ']',
            Delim::Brace => '}',
        }
    }
}

/// The six prefix reader macros of §1.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Prefix {
    Quote,
    Quasiquote,
    Unquote,
    UnquoteSplicing,
    Deref,
    InOut,
}

impl Prefix {
    /// The head symbol of the list the prefix reads as.
    pub(crate) fn head(self) -> &'static str {
        match self {
            Prefix::Quote => "quote",
            Prefix::Quasiquote => "quasiquote",
            Prefix::Unquote => "unquote",
            Prefix::UnquoteSplicing => "unquote-splicing",
            Prefix::Deref => "deref",
            Prefix::InOut => "&",
        }
    }

    /// The prefix as written.
    pub(crate) fn text(self) -> &'static str {
        match self {
            Prefix::Quote => "'",
            Prefix::Quasiquote => "`",
            Prefix::Unquote => "~",
            Prefix::UnquoteSplicing => "~@",
            Prefix::Deref => "@",
            Prefix::InOut => "&",
        }
    }
}

#[derive(Debug)]
pub(crate) enum TokenKind {
    Open(Delim),
    Close(Delim),
    Prefix(Prefix),
    Discard,
    /// `#(`: opens an anonymous function.
    OpenFn,
    /// `#{`: opens a set literal.
    OpenSet,
    Atom(FormKind),
}

#[derive(Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) pos: Pos,
}

pub(crate) struct Lexer<'a> {
    cur: Cursor<'a>,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(src: &'a str, file: &str) -> Self {
        Lexer {
            cur: Cursor::new(src, file),
        }
    }

    /// The next token, or `None` at the end of the input.
    pub(crate) fn next_token(&mut self) -> Result<Option<Token>, ReadError> {
        self.skip_trivia();
        let mark = self.cur.mark();
        let Some(c) = self.cur.peek() else {
            return Ok(None);
        };
        let kind = self.lex_token(c, mark)?;
        Ok(Some(Token {
            kind,
            pos: self.cur.pos_from(mark),
        }))
    }

    /// Skips whitespace, commas and `;` comments.
    fn skip_trivia(&mut self) {
        while let Some(c) = self.cur.peek() {
            if is_separator(c) {
                self.cur.bump();
            } else if c == ';' {
                self.cur.bump_while(|c| c != '\n');
            } else {
                return;
            }
        }
    }

    fn error(&self, mark: Mark, kind: ReadErrorKind) -> ReadError {
        ReadError {
            kind,
            pos: self.cur.pos_from(mark),
        }
    }

    fn lex_token(&mut self, c: char, mark: Mark) -> Result<TokenKind, ReadError> {
        let single = match c {
            '(' => Some(TokenKind::Open(Delim::Paren)),
            '[' => Some(TokenKind::Open(Delim::Bracket)),
            '{' => Some(TokenKind::Open(Delim::Brace)),
            ')' => Some(TokenKind::Close(Delim::Paren)),
            ']' => Some(TokenKind::Close(Delim::Bracket)),
            '}' => Some(TokenKind::Close(Delim::Brace)),
            '\'' => Some(TokenKind::Prefix(Prefix::Quote)),
            '`' => Some(TokenKind::Prefix(Prefix::Quasiquote)),
            _ => None,
        };
        if let Some(kind) = single {
            self.cur.bump();
            return Ok(kind);
        }
        match c {
            '"' => lex_string(&mut self.cur).map(TokenKind::Atom),
            '\\' => lex_char(&mut self.cur).map(TokenKind::Atom),
            '~' | '@' | '&' => self.lex_tight_prefix(c, mark),
            '#' => self.lex_dispatch(mark),
            ':' => self.lex_keyword(mark),
            c if starts_number(c, self.cur.peek_nth(1)) => self.lex_number(mark),
            c if is_forbidden(c) => {
                self.cur.bump();
                Err(self.error(mark, ReadErrorKind::InvalidCharacter(c)))
            }
            _ => self.lex_symbol(mark),
        }
    }

    /// `~` `~@` `@` `&`: each must be followed immediately by a form. A
    /// `&` that is not is the reserved symbol `&` itself (§1.1), which is
    /// what `(& x)` needs to read back.
    fn lex_tight_prefix(&mut self, c: char, mark: Mark) -> Result<TokenKind, ReadError> {
        self.cur.bump();
        let prefix = match c {
            '~' if self.cur.peek() == Some('@') => {
                self.cur.bump();
                Prefix::UnquoteSplicing
            }
            '~' => Prefix::Unquote,
            '@' => Prefix::Deref,
            _ if !starts_form_char(self.cur.peek()) => {
                return Ok(TokenKind::Atom(FormKind::Sym("&".to_string())))
            }
            _ => Prefix::InOut,
        };
        if !starts_form_char(self.cur.peek()) {
            let kind = ReadErrorKind::PrefixWithoutForm(prefix.text());
            return Err(self.error(mark, kind));
        }
        Ok(TokenKind::Prefix(prefix))
    }

    fn lex_dispatch(&mut self, mark: Mark) -> Result<TokenKind, ReadError> {
        self.cur.bump();
        match self.cur.peek() {
            Some('_') => {
                self.cur.bump();
                Ok(TokenKind::Discard)
            }
            Some('(') => {
                self.cur.bump();
                Ok(TokenKind::OpenFn)
            }
            Some('{') => {
                self.cur.bump();
                Ok(TokenKind::OpenSet)
            }
            other => {
                self.cur.bump();
                Err(self.error(mark, ReadErrorKind::UnknownDispatch(other)))
            }
        }
    }

    fn lex_keyword(&mut self, mark: Mark) -> Result<TokenKind, ReadError> {
        self.cur.bump();
        self.cur.bump_while(is_constituent);
        let text = self.cur.text_from(mark);
        // Cannot panic: `text` starts with the one-byte `:`.
        let name = &text[1..];
        if name.is_empty() || name.starts_with(':') || !valid_slashes(name) {
            let kind = ReadErrorKind::InvalidKeyword(text.to_string());
            return Err(self.error(mark, kind));
        }
        Ok(TokenKind::Atom(FormKind::Kw(name.to_string())))
    }

    fn lex_number(&mut self, mark: Mark) -> Result<TokenKind, ReadError> {
        self.cur.bump_while(is_constituent);
        let text = self.cur.text_from(mark);
        let pos = self.cur.pos_from(mark);
        if let Some(ratio) = ratio(text, &pos) {
            return ratio
                .map(TokenKind::Atom)
                .map_err(|kind| self.error(mark, kind));
        }
        parse_number(text)
            .map(TokenKind::Atom)
            .map_err(|kind| self.error(mark, kind))
    }

    fn lex_symbol(&mut self, mark: Mark) -> Result<TokenKind, ReadError> {
        self.cur.bump_while(is_constituent);
        let text = self.cur.text_from(mark);
        let kind = match text {
            "true" => FormKind::Bool(true),
            "false" => FormKind::Bool(false),
            "nil" => FormKind::Nil,
            _ if valid_slashes(text) => FormKind::Sym(text.to_string()),
            _ => {
                let kind = ReadErrorKind::InvalidSymbol(text.to_string());
                return Err(self.error(mark, kind));
            }
        };
        Ok(TokenKind::Atom(kind))
    }
}

/// §1.1: a number starts with a digit, or with `-` followed by a digit.
fn starts_number(c: char, next: Option<char>) -> bool {
    c.is_ascii_digit() || (c == '-' && next.is_some_and(|n| n.is_ascii_digit()))
}

/// §1.1: `7/2` (an optional `-`, decimal digits, `/`, decimal digits) is
/// the list `(/ 7 2)`; each of the three forms has the position of its
/// own text. `None` when `text` is not of that shape.
fn ratio(text: &str, pos: &Pos) -> Option<Result<FormKind, ReadErrorKind>> {
    let (num, den) = text.split_once('/')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(num.strip_prefix('-').unwrap_or(num)) || !digits(den) {
        return None;
    }
    // All of the text is ASCII, so bytes are columns.
    let part = |from: usize, to: usize| Pos {
        col: pos.col + from,
        start: pos.start + from,
        end: pos.start + to,
        ..pos.clone()
    };
    let int = |from: usize, to: usize| {
        parse_number(&text[from..to]).map(|kind| Form::new(kind, part(from, to)))
    };
    let (a, b) = (num.len(), num.len() + 1);
    let items = int(0, a).and_then(|n| Ok((n, int(b, text.len())?)));
    Some(items.map(|(n, d)| {
        let slash = Form::new(FormKind::Sym("/".to_string()), part(a, b));
        FormKind::List(vec![slash, n, d])
    }))
}
