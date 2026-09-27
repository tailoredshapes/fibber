//! Tokens (spec/syntax.md §1.1, §1.2): delimiters, prefix reader macros,
//! the form comment `#_`, and atoms, which are complete forms already.
//! Whitespace, `;` comments and separator commas are skipped here.

use super::chars::{is_constituent, is_forbidden, is_space, starts_form_char, valid_slashes};
use super::cursor::{Cursor, Mark};
use super::error::{ReadError, ReadErrorKind};
use super::form::FormKind;
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
            Prefix::Unquote => ",",
            Prefix::UnquoteSplicing => ",@",
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
    Atom(FormKind),
}

#[derive(Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) pos: Pos,
}

pub(crate) struct Lexer<'a> {
    cur: Cursor<'a>,
    /// Commas before this byte offset are known to be unquotes: they
    /// belong to a run of commas followed by a form. Remembering it keeps
    /// a long run of commas linear rather than quadratic.
    unquote_until: usize,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(src: &'a str, file: &str) -> Self {
        Lexer {
            cur: Cursor::new(src, file),
            unquote_until: 0,
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

    /// Skips whitespace, `;` comments and commas that do not unquote.
    fn skip_trivia(&mut self) {
        while let Some(c) = self.cur.peek() {
            if is_space(c) {
                self.cur.bump();
            } else if c == ';' {
                self.cur.bump_while(|c| c != '\n');
            } else if c == ',' && !self.starts_form(1) {
                // Every comma of this run is followed by commas and then
                // no form, so the whole run is whitespace.
                self.cur.bump_while(|c| c == ',');
            } else {
                return;
            }
        }
    }

    /// Whether the text `skip` bytes ahead starts a form (§1.2), looking
    /// through a run of commas: `,,x` is `(unquote (unquote x))`, and a
    /// run of commas not followed by a form is whitespace.
    fn starts_form(&mut self, skip: usize) -> bool {
        let off = self.cur.offset() + skip;
        let rest = self.cur.src_from(off);
        if rest.starts_with(',') && off < self.unquote_until {
            return true;
        }
        let after = rest.trim_start_matches(',');
        let starts = starts_form_char(after.chars().next());
        if starts {
            self.unquote_until = off + (rest.len() - after.len());
        }
        starts
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
            ',' | '@' | '&' => self.lex_tight_prefix(c, mark),
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

    /// `,` `,@` `@` `&`: each must be followed immediately by a form. A
    /// `&` that is not is the reserved symbol `&` itself (§1.1), which is
    /// what `(& x)` needs to read back.
    fn lex_tight_prefix(&mut self, c: char, mark: Mark) -> Result<TokenKind, ReadError> {
        self.cur.bump();
        let prefix = match c {
            ',' if self.cur.peek() == Some('@') => {
                self.cur.bump();
                Prefix::UnquoteSplicing
            }
            ',' => Prefix::Unquote,
            '@' => Prefix::Deref,
            _ if !self.starts_form(0) => {
                return Ok(TokenKind::Atom(FormKind::Sym("&".to_string())))
            }
            _ => Prefix::InOut,
        };
        if !self.starts_form(0) {
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
