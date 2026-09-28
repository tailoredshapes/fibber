//! Tokens to forms, with the nesting limit of spec/lir.md §1.

use super::lexer::{tokens, Tok};
use super::{Sexp, MAX_DEPTH};
use crate::diag::{err, Pos, Result};

/// Read every top-level form of `src`.
pub fn read(src: &str) -> Result<Vec<Sexp>> {
    let toks = tokens(src)?;
    let mut r = Reader { toks, i: 0 };
    let mut out = Vec::new();
    while r.i < r.toks.len() {
        out.push(r.form(0)?);
    }
    Ok(out)
}

struct Reader {
    toks: Vec<(Tok, Pos)>,
    i: usize,
}

impl Reader {
    fn form(&mut self, depth: usize) -> Result<Sexp> {
        let (tok, pos) = self.toks[self.i].clone();
        self.i += 1;
        match tok {
            Tok::Open => Ok(Sexp::List(self.seq(depth, pos, Tok::Close)?, pos)),
            Tok::BraceOpen => Ok(Sexp::Brace(self.seq(depth, pos, Tok::BraceClose)?, pos)),
            Tok::Close => err(pos, "unexpected )"),
            Tok::BraceClose => err(pos, "unexpected }"),
            Tok::Atom(a) => Ok(Sexp::Atom(a, pos)),
            Tok::Str(s) => Ok(Sexp::Str(s, pos)),
            Tok::VecType(n, e) => Ok(Sexp::VecType(n, e, pos)),
        }
    }

    fn seq(&mut self, depth: usize, open: Pos, close: Tok) -> Result<Vec<Sexp>> {
        if depth + 1 > MAX_DEPTH {
            return err(open, format!("nesting deeper than {MAX_DEPTH}"));
        }
        let mut items = Vec::new();
        loop {
            match self.toks.get(self.i) {
                None => return err(open, "unexpected end of input: unclosed list"),
                Some((t, _)) if *t == close => {
                    self.i += 1;
                    return Ok(items);
                }
                Some((Tok::Close, p)) | Some((Tok::BraceClose, p)) => {
                    return err(*p, "mismatched closing bracket")
                }
                Some(_) => items.push(self.form(depth + 1)?),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_forms_with_positions() {
        let f = read("(a (b \"x\\n\") { c })\n<4 x i32>").unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(
            f[1],
            Sexp::VecType("4".into(), "i32".into(), Pos { line: 2, col: 1 })
        );
        match &f[0] {
            Sexp::List(items, _) => assert_eq!(items.len(), 3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn commas_are_whitespace() {
        let atoms = |src: &str| match &read(src).unwrap()[0] {
            Sexp::Brace(items, _) => items.iter().map(|i| i.describe()).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        assert_eq!(atoms("{ptr, ptr}"), vec!["ptr", "ptr"]);
        assert_eq!(atoms("{ptr, ptr}"), atoms("{ptr ptr}"));
    }

    #[test]
    fn rejects_bad_escapes_and_unterminated_forms() {
        for (src, msg) in [
            ("\"a\\qb\"", "invalid escape \\q"),
            ("\"abc", "unterminated string"),
            ("(a", "unexpected end of input"),
            (")", "unexpected )"),
            ("(a }", "mismatched closing bracket"),
        ] {
            let e = read(src).unwrap_err();
            assert!(e.message.contains(msg), "{src}: {}", e.message);
        }
    }

    #[test]
    fn nesting_limit_fires_before_the_stack_runs_out() {
        let deep = "(".repeat(200_000);
        let e = read(&deep).unwrap_err();
        assert!(e.message.contains("nesting deeper than 512"));
        let ok = format!("{}{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
        assert!(read(&ok).is_ok());
    }
}
