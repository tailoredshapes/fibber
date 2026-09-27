//! The reader proper: tokens to forms, with an explicit stack.
//!
//! Nesting is tracked on a heap-allocated stack of open frames, never by
//! recursion, so no input can overflow the native stack while reading.
//! The stack is capped at [`MAX_DEPTH`] frames (every open delimiter,
//! pending prefix macro and pending `#_` counts one); deeper input is the
//! error [`ReadErrorKind::TooDeep`]. The cap exists because `Form` is a
//! tree whose `Drop`, `Display` and `PartialEq` recurse, and because no
//! program needs more.

use super::error::{ReadError, ReadErrorKind};
use super::form::{Form, FormKind};
use super::lexer::{Delim, Lexer, Prefix, Token, TokenKind};
use super::pos::Pos;

/// The deepest nesting the reader accepts.
pub const MAX_DEPTH: usize = 1000;

enum Frame {
    Seq {
        delim: Delim,
        open: Pos,
        items: Vec<Form>,
    },
    Prefix {
        prefix: Prefix,
        pos: Pos,
    },
    Discard {
        pos: Pos,
    },
}

struct Reader {
    stack: Vec<Frame>,
    out: Vec<Form>,
}

fn error(kind: ReadErrorKind, pos: Pos) -> ReadError {
    ReadError { kind, pos }
}

/// Reads every top-level form of `source`; see [`super::read_all`].
pub(crate) fn read_all(source: &str, file: &str) -> Result<Vec<Form>, ReadError> {
    let mut lexer = Lexer::new(source, file);
    let mut reader = Reader {
        stack: Vec::new(),
        out: Vec::new(),
    };
    while let Some(token) = lexer.next_token()? {
        reader.feed(token)?;
    }
    reader.finish()
}

impl Reader {
    fn feed(&mut self, token: Token) -> Result<(), ReadError> {
        let pos = token.pos;
        match token.kind {
            TokenKind::Open(delim) => self.push(Frame::Seq {
                delim,
                open: pos,
                items: Vec::new(),
            }),
            TokenKind::Close(delim) => self.close(delim, pos),
            TokenKind::Prefix(prefix) => self.push(Frame::Prefix { prefix, pos }),
            TokenKind::Discard => self.push(Frame::Discard { pos }),
            TokenKind::Atom(kind) => self.complete(Form::new(kind, pos)),
        }
    }

    fn push(&mut self, frame: Frame) -> Result<(), ReadError> {
        if self.stack.len() >= MAX_DEPTH {
            let pos = match frame {
                Frame::Seq { open: pos, .. }
                | Frame::Prefix { pos, .. }
                | Frame::Discard { pos } => pos,
            };
            return Err(error(ReadErrorKind::TooDeep { limit: MAX_DEPTH }, pos));
        }
        self.stack.push(frame);
        Ok(())
    }

    fn close(&mut self, found: Delim, pos: Pos) -> Result<(), ReadError> {
        match self.stack.pop() {
            None => Err(error(
                ReadErrorKind::UnexpectedClose {
                    found: found.close(),
                },
                pos,
            )),
            Some(Frame::Seq { delim, open, items }) if delim == found => {
                let form = build_seq(delim, open.through(&pos), items)?;
                self.complete(form)
            }
            Some(Frame::Seq { delim, open, .. }) => {
                let kind = ReadErrorKind::MismatchedClose {
                    open: delim.open(),
                    open_line: open.line,
                    open_col: open.col,
                    found: found.close(),
                };
                Err(error(kind, pos))
            }
            Some(frame) => Err(unfinished(frame)),
        }
    }

    /// Hands a finished form to the innermost open frame, applying and
    /// popping any prefix frames on the way.
    fn complete(&mut self, mut form: Form) -> Result<(), ReadError> {
        loop {
            let (prefix, pos) = match self.stack.last_mut() {
                None => {
                    self.out.push(form);
                    return Ok(());
                }
                Some(Frame::Seq { items, .. }) => {
                    items.push(form);
                    return Ok(());
                }
                Some(Frame::Discard { .. }) => {
                    self.stack.pop();
                    return Ok(());
                }
                Some(Frame::Prefix { prefix, pos }) => (*prefix, pos.clone()),
            };
            self.stack.pop();
            form = apply_prefix(prefix, pos, form)?;
        }
    }

    fn finish(mut self) -> Result<Vec<Form>, ReadError> {
        match self.stack.pop() {
            None => Ok(self.out),
            Some(frame) => Err(unfinished(frame)),
        }
    }
}

/// The error for a frame still open at a close or at the end of input.
fn unfinished(frame: Frame) -> ReadError {
    match frame {
        Frame::Seq { delim, open, .. } => {
            error(ReadErrorKind::Unclosed { open: delim.open() }, open)
        }
        Frame::Prefix { prefix, pos } => {
            error(ReadErrorKind::PrefixWithoutForm(prefix.text()), pos)
        }
        Frame::Discard { pos } => error(ReadErrorKind::DiscardWithoutForm, pos),
    }
}

fn build_seq(delim: Delim, pos: Pos, items: Vec<Form>) -> Result<Form, ReadError> {
    let kind = match delim {
        Delim::Paren => FormKind::List(items),
        Delim::Bracket => FormKind::Vec(items),
        Delim::Brace if !items.len().is_multiple_of(2) => {
            let count = items.len();
            return Err(error(ReadErrorKind::OddMapEntries { count }, pos));
        }
        Delim::Brace => FormKind::Map(items),
    };
    Ok(Form::new(kind, pos))
}

/// §1.2: `'x` is `(quote x)`, and so on; `&` requires a symbol.
fn apply_prefix(prefix: Prefix, pos: Pos, form: Form) -> Result<Form, ReadError> {
    let whole = pos.through(&form.pos);
    if prefix == Prefix::InOut && !matches!(form.kind, FormKind::Sym(_)) {
        return Err(error(ReadErrorKind::InOutNotSymbol, whole));
    }
    let head = Form::new(FormKind::Sym(prefix.head().to_string()), pos);
    Ok(Form::new(FormKind::List(vec![head, form]), whole))
}
