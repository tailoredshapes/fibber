//! Forms as trees: printing them back to source, counting and
//! addressing their nodes in pre-order.

use lir::sexp::Sexp;

/// The source text of `forms`, one top-level form per line. Positions
/// are lost, which a mutant does not need.
pub fn print(forms: &[Sexp]) -> String {
    let mut out = String::new();
    for f in forms {
        write(f, &mut out);
        out.push('\n');
    }
    out
}

fn write(s: &Sexp, out: &mut String) {
    match s {
        Sexp::Atom(a, _) => out.push_str(a),
        Sexp::VecType(n, e, _) => out.push_str(&format!("<{n} x {e}>")),
        Sexp::Str(bytes, _) => write_str(bytes, out),
        Sexp::List(items, _) => seq(items, '(', ')', out),
        Sexp::Brace(items, _) => seq(items, '{', '}', out),
        Sexp::Bracket(items, _) => seq(items, '[', ']', out),
    }
}

fn seq(items: &[Sexp], open: char, close: char, out: &mut String) {
    out.push(open);
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        write(item, out);
    }
    out.push(close);
}

fn write_str(bytes: &[u8], out: &mut String) {
    out.push('"');
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push('"');
}

/// The children of a form, if it has any.
pub fn items(s: &Sexp) -> Option<&Vec<Sexp>> {
    match s {
        Sexp::List(items, _) | Sexp::Brace(items, _) | Sexp::Bracket(items, _) => Some(items),
        _ => None,
    }
}

fn items_mut(s: &mut Sexp) -> Option<&mut Vec<Sexp>> {
    match s {
        Sexp::List(items, _) | Sexp::Brace(items, _) | Sexp::Bracket(items, _) => Some(items),
        _ => None,
    }
}

/// How many nodes the forms hold, themselves included.
pub fn count(forms: &[Sexp]) -> usize {
    forms
        .iter()
        .map(|f| 1 + items(f).map_or(0, |v| count(v)))
        .sum()
}

/// The `n`th node in pre-order.
pub fn nth(forms: &[Sexp], mut n: usize) -> Option<&Sexp> {
    for f in forms {
        if n == 0 {
            return Some(f);
        }
        n -= 1;
        let inner = items(f).map_or(0, |v| count(v));
        if n < inner {
            return items(f).and_then(|v| nth(v, n));
        }
        n -= inner;
    }
    None
}

/// Apply `f` to the `n`th node in pre-order; false if there is none.
pub fn with_nth(forms: &mut [Sexp], mut n: usize, f: &mut dyn FnMut(&mut Sexp)) -> bool {
    for form in forms.iter_mut() {
        if n == 0 {
            f(form);
            return true;
        }
        n -= 1;
        let inner = items(form).map_or(0, |v| count(v));
        if n < inner {
            return items_mut(form).is_some_and(|v| with_nth(v, n, f));
        }
        n -= inner;
    }
    false
}

/// Every atom's text, in pre-order, duplicates included.
pub fn atoms(forms: &[Sexp]) -> Vec<String> {
    let mut out = Vec::new();
    for f in forms {
        match f {
            Sexp::Atom(a, _) => out.push(a.clone()),
            other => {
                if let Some(v) = items(other) {
                    out.extend(atoms(v));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_what_it_read() {
        let src = "(define (main i32) () (block entry (ret (i32 0))))\n(a \"x\\n\\\"\\x01\" <4 x i32> [2 x i8] {i32, ptr})\n";
        let forms = lir::sexp::read(src).unwrap();
        let printed = print(&forms);
        assert_eq!(
            printed,
            "(define (main i32) () (block entry (ret (i32 0))))\n(a \"x\\n\\\"\\x01\" <4 x i32> [2 x i8] {i32 ptr})\n"
        );
        assert_eq!(print(&lir::sexp::read(&printed).unwrap()), printed);
    }

    #[test]
    fn addresses_nodes_in_preorder() {
        let mut forms = lir::sexp::read("(a (b c) d) e").unwrap();
        assert_eq!(count(&forms), 7);
        assert_eq!(nth(&forms, 2).unwrap().describe(), "b");
        assert_eq!(nth(&forms, 4).unwrap().describe(), "c");
        assert_eq!(nth(&forms, 5).unwrap().describe(), "d");
        assert_eq!(nth(&forms, 6).unwrap().describe(), "e");
        assert!(nth(&forms, 7).is_none());
        assert!(with_nth(&mut forms, 3, &mut |s| {
            *s = Sexp::Atom("z".into(), s.pos())
        }));
        assert_eq!(print(&forms), "(a (z c) d)\ne\n");
        assert_eq!(atoms(&forms), ["a", "z", "c", "d", "e"]);
    }
}
