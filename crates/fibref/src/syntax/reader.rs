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

/// What a sequence frame becomes when it closes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SeqKind {
    /// A list, vector or map, by its delimiter.
    Plain,
    /// `#{..}`, the list `(hash-set ..)`.
    Set,
    /// `#(..)`, the list `(fn (%1 .. %n) (..))`.
    Fn,
}

/// The most parameters a `#(` has.
const MAX_FN_PARAMS: usize = 255;

/// The parameters met so far in the `#(` being read: for each index, the
/// position of its first use.
#[derive(Default)]
struct FnParams {
    uses: Vec<Option<Pos>>,
}

enum Frame {
    Seq {
        delim: Delim,
        kind: SeqKind,
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
    /// Set while a `#(` is open; there is at most one.
    params: Option<FnParams>,
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
        params: None,
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
            TokenKind::Open(delim) => self.open(delim, SeqKind::Plain, pos),
            TokenKind::OpenSet => self.open(Delim::Brace, SeqKind::Set, pos),
            TokenKind::OpenFn if self.params.is_some() => Err(error(ReadErrorKind::NestedFn, pos)),
            TokenKind::OpenFn => {
                self.open(Delim::Paren, SeqKind::Fn, pos)?;
                self.params = Some(FnParams::default());
                Ok(())
            }
            TokenKind::Close(delim) => self.close(delim, pos),
            TokenKind::Prefix(prefix) => self.push(Frame::Prefix { prefix, pos }),
            TokenKind::Discard => self.push(Frame::Discard { pos }),
            TokenKind::Atom(FormKind::Sym(name)) if self.params.is_some() => {
                let name = self.param(name, &pos)?;
                self.complete(Form::new(FormKind::Sym(name), pos))
            }
            TokenKind::Atom(kind) => self.complete(Form::new(kind, pos)),
        }
    }

    fn open(&mut self, delim: Delim, kind: SeqKind, open: Pos) -> Result<(), ReadError> {
        self.push(Frame::Seq {
            delim,
            kind,
            open,
            items: Vec::new(),
        })
    }

    /// A symbol read inside `#(`: `%` and `%N` are parameters, noted
    /// with the position of their first use; `%` is named `%1`.
    fn param(&mut self, name: String, pos: &Pos) -> Result<String, ReadError> {
        let Some(digits) = name.strip_prefix('%') else {
            return Ok(name);
        };
        let bad = |reason| {
            let kind = ReadErrorKind::BadFnParam {
                text: name.clone(),
                reason,
            };
            error(kind, pos.clone())
        };
        let index = if digits.is_empty() {
            1
        } else if digits == "&" {
            return Err(error(ReadErrorKind::UnsupportedRest, pos.clone()));
        } else if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(name);
        } else if digits.starts_with('0') {
            return Err(bad(
                "parameters are numbered from %1, without a leading zero",
            ));
        } else {
            match digits.parse::<usize>() {
                Ok(n) if n <= MAX_FN_PARAMS => n,
                _ => return Err(bad("a #( has at most 255 parameters")),
            }
        };
        if let Some(p) = self.params.as_mut() {
            if p.uses.len() < index {
                p.uses.resize(index, None);
            }
            p.uses[index - 1].get_or_insert_with(|| pos.clone());
        }
        Ok(format!("%{index}"))
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
            Some(Frame::Seq {
                delim,
                kind,
                open,
                items,
            }) if delim == found => {
                let params = if kind == SeqKind::Fn {
                    self.params.take()
                } else {
                    None
                };
                let form = build_seq(delim, kind, params, open.through(&pos), items)?;
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

fn build_seq(
    delim: Delim,
    kind: SeqKind,
    params: Option<FnParams>,
    pos: Pos,
    items: Vec<Form>,
) -> Result<Form, ReadError> {
    // The `#(` or `#{` token: the first two characters.
    let open = Pos {
        end: pos.start + 2,
        ..pos.clone()
    };
    let sym = |name: &str, at: &Pos| Form::new(FormKind::Sym(name.to_string()), at.clone());
    let kind = match (kind, delim) {
        (SeqKind::Set, _) => {
            let mut all = vec![sym("hash-set", &open)];
            all.extend(items);
            FormKind::List(all)
        }
        (SeqKind::Fn, _) => {
            let uses = params.map(|p| p.uses).unwrap_or_default();
            let names = uses
                .iter()
                .enumerate()
                .map(|(i, first)| sym(&format!("%{}", i + 1), first.as_ref().unwrap_or(&open)));
            let params = Form::new(FormKind::List(names.collect()), open.clone());
            let body = Form::new(FormKind::List(items), pos.clone());
            FormKind::List(vec![sym("fn", &open), params, body])
        }
        (_, Delim::Paren) => FormKind::List(items),
        (_, Delim::Bracket) => FormKind::Vec(items),
        (_, Delim::Brace) if !items.len().is_multiple_of(2) => {
            let count = items.len();
            return Err(error(ReadErrorKind::OddMapEntries { count }, pos));
        }
        (_, Delim::Brace) => FormKind::Map(items),
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
