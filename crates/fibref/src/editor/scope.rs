//! The reader-level scope at a cursor: the locals and parameters in
//! scope there, and the buffer's own top-level definitions. It reads the
//! buffer through the reader (`syntax::read_all`): the text before the
//! cursor, closed by the delimiters still open, reads even when the
//! rest of the buffer does not.

use super::analysis::Delims;
use crate::syntax::{read_all, Form, FormKind};

/// A local or a parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    pub name: String,
    /// The head of its annotation (`Point` of `p: Point`), if written.
    pub ann: Option<String>,
}

/// A top-level definition of the buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Def {
    pub name: String,
    pub kind: &'static str,
    pub detail: String,
}

/// What the buffer says about the cursor's surroundings.
#[derive(Default)]
pub struct Scope {
    pub locals: Vec<Local>,
    pub defs: Vec<Def>,
}

/// An open form on the way to the cursor: its items and the index of
/// the item being written.
struct Frame<'a> {
    items: &'a [Form],
    idx: usize,
}

/// The scope at byte offset `word_start` of `src` (the start of the
/// partial word at the cursor).
pub fn scope_at(src: &str, word_start: usize, path: &str) -> Scope {
    let prefix =
        src[..word_start].trim_end_matches(|c: char| c.is_whitespace() || "'`~@#^".contains(c));
    let mut delims = Delims::default();
    prefix.chars().for_each(|c| delims.step(c));
    if delims.in_string() {
        return Scope::default();
    }
    let closed = format!("{prefix}{}", delims.closers());
    let Ok(forms) = read_all(&closed, path) else {
        return whole_buffer(src, path);
    };
    let mut scope = whole_buffer(src, path);
    if scope.defs.is_empty() {
        scope.defs = top_defs(&forms, &closed);
    }
    scope.locals = locals(&frames(&forms, delims.open.len()));
    scope
}

fn whole_buffer(src: &str, path: &str) -> Scope {
    Scope {
        locals: Vec::new(),
        defs: read_all(src, path)
            .map(|f| top_defs(&f, src))
            .unwrap_or_default(),
    }
}

fn seq(f: &Form) -> Option<&[Form]> {
    match &f.kind {
        FormKind::List(v) | FormKind::Vec(v) | FormKind::Map(v) => Some(v),
        _ => None,
    }
}

fn sym(f: &Form) -> &str {
    f.as_sym().unwrap_or("")
}

fn head(items: &[Form]) -> &str {
    items.first().map_or("", sym)
}

/// The `open` forms the cursor is inside, outermost first: the last
/// top-level form, then each last item, `open` deep.
fn frames(forms: &[Form], open: usize) -> Vec<Frame<'_>> {
    let mut out = Vec::new();
    let mut cur = forms.last();
    for level in 0..open {
        let Some(items) = cur.and_then(seq) else {
            break;
        };
        let last = level + 1 == open;
        out.push(Frame {
            items,
            idx: if last {
                items.len()
            } else {
                items.len().saturating_sub(1)
            },
        });
        cur = items.last();
    }
    out
}

fn locals(path: &[Frame]) -> Vec<Local> {
    let mut out = Vec::new();
    for (k, fr) in path.iter().enumerate() {
        let parent = k.checked_sub(1).map(|p| head(path[p].items));
        let c = fr.items;
        match (parent, head(c)) {
            (Some("match"), _) if fr.idx >= 1 => pattern(&c[0], &mut out),
            (Some("defn"), _) if fr.idx >= 1 => params_of(&c[0], &mut out),
            (_, "let" | "let*") => let_form(c, fr.idx, path.get(k + 1), &mut out),
            (_, "loop") if fr.idx > 1 => bindings(c.get(1), usize::MAX, &mut out),
            (_, "defun" | "defmacro") => def_params(c, fr.idx, &mut out),
            (_, "fn") => fn_params(c, fr.idx, &mut out),
            (_, "if-let" | "when-let" | "if-some" | "when-some" | "try-let" | "dotimes") => {
                pair_form(c, fr.idx, &mut out)
            }
            _ => {}
        }
    }
    out
}

fn let_form(c: &[Form], idx: usize, next: Option<&Frame>, out: &mut Vec<Local>) {
    if idx > 1 {
        bindings(c.get(1), usize::MAX, out);
    } else if let (1, Some(inner)) = (idx, next) {
        bindings(c.get(1), inner.idx, out);
    }
}

fn pair_form(c: &[Form], idx: usize, out: &mut Vec<Local>) {
    let Some(b) = c.get(1).filter(|_| idx > 1) else {
        return;
    };
    match seq(b) {
        Some(items) if items.first().and_then(seq).is_some() => bindings(Some(b), usize::MAX, out),
        Some(items) => items.first().into_iter().for_each(|p| pattern(p, out)),
        None => {}
    }
}

/// The names a binding list binds, from its first `done` items: pairs
/// `((p e) ..)` or the flat `[p e ..]`, where `p:` takes a type.
fn bindings(b: Option<&Form>, done: usize, out: &mut Vec<Local>) {
    let Some(b) = b else { return };
    let Some(items) = seq(b) else { return };
    let done = done.min(items.len());
    if matches!(b.kind, FormKind::Vec(_)) {
        flat_bindings(&items[..done], out);
        return;
    }
    for pair in items[..done]
        .iter()
        .filter_map(seq)
        .filter(|p| p.len() >= 2)
    {
        annotated_or_pattern(pair, out);
    }
}

fn flat_bindings(items: &[Form], out: &mut Vec<Local>) {
    let mut i = 0;
    while i < items.len() {
        let width = if sym(&items[i]).ends_with(':') && sym(&items[i]).len() > 1 {
            3
        } else {
            2
        };
        if i + width <= items.len() {
            annotated_or_pattern(&items[i..i + width], out);
        }
        i += width;
    }
}

fn annotated_or_pattern(item: &[Form], out: &mut Vec<Local>) {
    let name = sym(&item[0]);
    if name.len() > 1 && name.ends_with(':') {
        out.push(Local {
            name: name.trim_end_matches(':').to_string(),
            ann: item.get(1).map(type_head),
        });
    } else {
        pattern(&item[0], out);
    }
}

fn type_head(f: &Form) -> String {
    match &f.kind {
        FormKind::Sym(s) => s.clone(),
        _ => seq(f).map_or(String::new(), |i| head(i).to_string()),
    }
}

/// The names a pattern binds (syntax §3.6).
fn pattern(f: &Form, out: &mut Vec<Local>) {
    match &f.kind {
        FormKind::Sym(s) if !matches!(s.as_str(), "_" | "&" | "nil") => out.push(Local {
            name: s.trim_end_matches(':').to_string(),
            ann: None,
        }),
        FormKind::List(items) if matches!(items.get(1).map(|k| &k.kind), Some(FormKind::Kw(k)) if k == "as") =>
        {
            pattern(&items[0], out);
            items.get(2).into_iter().for_each(|n| pattern(n, out));
        }
        FormKind::List(items) => items.iter().skip(1).for_each(|p| pattern(p, out)),
        FormKind::Vec(items) => items.iter().for_each(|p| pattern(p, out)),
        _ => {}
    }
}

fn def_params(c: &[Form], idx: usize, out: &mut Vec<Local>) {
    let at = c
        .iter()
        .skip(2)
        .position(|f| seq(f).is_some())
        .map(|i| i + 2);
    if let Some(p) = at.filter(|p| idx > *p) {
        params_of(&c[p], out);
    }
}

fn fn_params(c: &[Form], idx: usize, out: &mut Vec<Local>) {
    let p = if c.get(1).and_then(Form::as_sym).is_some() {
        2
    } else {
        1
    };
    if let Some(f) = c.get(p).filter(|_| idx > p) {
        params_of(f, out);
    }
}

/// The parameters of a parameter list: `x: T` and plain `x`, with `&`
/// and `:borrow` skipped.
fn params_of(list: &Form, out: &mut Vec<Local>) {
    let Some(items) = seq(list) else { return };
    let mut i = 0;
    while i < items.len() {
        match &items[i].kind {
            FormKind::Sym(s) if s == "->" => break,
            FormKind::Sym(s) if s.len() > 1 && s.ends_with(':') => {
                out.push(Local {
                    name: s.trim_end_matches(':').to_string(),
                    ann: items.get(i + 1).map(type_head),
                });
                i += 1;
            }
            FormKind::Sym(s) if s != "&" => out.push(Local {
                name: s.clone(),
                ann: None,
            }),
            _ => {}
        }
        i += 1;
    }
}

/// The definitions among top-level `forms` read from `text`.
pub fn top_defs(forms: &[Form], text: &str) -> Vec<Def> {
    let mut defs = Vec::new();
    for items in forms.iter().filter_map(|f| f.as_list()) {
        let Some(name) = items.get(1) else { continue };
        let n = match &name.kind {
            FormKind::Sym(s) => s.clone(),
            _ => seq(name).map_or(String::new(), |i| head(i).to_string()),
        };
        if n.is_empty() {
            continue;
        }
        let mut add = |name: &str, kind: &'static str, detail: String| {
            defs.push(Def {
                name: name.to_string(),
                kind,
                detail,
            })
        };
        match head(items) {
            "defun" | "defn" => add(&n, "function", signature(items, text)),
            "defmacro" => add(&n, "macro", signature(items, text)),
            "def" => add(&n, "variable", String::new()),
            "defstruct" | "defrecord" => {
                add(&n, "struct", String::new());
            }
            "defenum" => {
                add(&n, "enum", String::new());
                for v in items.iter().skip(2) {
                    let vn = if sym(v).is_empty() {
                        seq(v).map_or("", head)
                    } else {
                        sym(v)
                    };
                    if vn.starts_with(char::is_uppercase) {
                        add(vn, "variant", n.clone());
                    }
                }
            }
            "defprotocol" => {
                add(&n, "protocol", String::new());
                for m in items.iter().skip(2).filter_map(seq) {
                    if !head(m).is_empty() {
                        add(head(m), "method", n.clone());
                    }
                }
            }
            _ => {}
        }
    }
    defs
}

/// `(x: i64) -> i64` as written in the source.
fn signature(items: &[Form], text: &str) -> String {
    let Some(p) = items
        .iter()
        .skip(2)
        .position(|f| seq(f).is_some())
        .map(|i| i + 2)
    else {
        return String::new();
    };
    let start = items[p].pos.start;
    let ret = items
        .get(p + 1)
        .filter(|a| sym(a) == "->")
        .and_then(|_| items.get(p + 2));
    let end = ret.map_or(items[p].pos.end, |r| r.pos.end);
    text.get(start..end).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<String> {
        let at = src.find('|').unwrap_or(src.len());
        let text = src.replace('|', "");
        scope_at(&text, at, "t.fib")
            .locals
            .into_iter()
            .map(|l| l.name)
            .collect()
    }

    #[test]
    fn parameters_and_let_bindings_are_in_scope_in_the_body_only_after_their_values() {
        assert_eq!(
            names("(defun f (a: i64 b: str) -> i64 (let ((x 1) (y 2)) |"),
            ["a", "b", "x", "y"]
        );
        assert_eq!(
            names("(defun f (a: i64) -> i64 (let ((x 1) (y |"),
            ["a", "x"]
        );
        assert_eq!(
            names("(defun f (a: i64) -> i64 (let ((x 1) (y (+ x |"),
            ["a", "x"]
        );
        assert_eq!(
            names("(defun f (a: i64) -> i64 (let [x 1 y: i64 2 z |"),
            ["a", "x", "y"]
        );
    }

    #[test]
    fn match_clauses_loops_and_fns_bind_for_their_bodies() {
        assert_eq!(names("(match v ((some w) |"), ["w"]);
        assert_eq!(names("(match v ((some w) 1) (nil |"), [] as [&str; 0]);
        assert_eq!(names("(loop ((i 0)) (fn (e) |"), ["i", "e"]);
        assert_eq!(names("(loop ((i |"), [] as [&str; 0]);
    }

    #[test]
    fn a_closed_form_before_the_cursor_binds_nothing_there() {
        assert_eq!(
            names("(defun f (a: i64) -> i64 a)\n(defun g (b: i64) -> i64 |"),
            ["b"]
        );
        assert_eq!(names("(defun g (b: i64) -> i64 (let ((x 1)) x) |"), ["b"]);
    }

    #[test]
    fn top_level_definitions_are_found_with_their_kinds() {
        let src = "(defun f (a: i64) -> i64 a)\n(defstruct P (x: i64))\n(defenum E A (B i64))\n(defprotocol Q (m (self) -> i64))";
        let forms = read_all(src, "t").expect("reads");
        let defs: Vec<_> = top_defs(&forms, src)
            .into_iter()
            .map(|d| (d.name, d.kind))
            .collect();
        let want = [
            ("f", "function"),
            ("P", "struct"),
            ("E", "enum"),
            ("A", "variant"),
            ("B", "variant"),
            ("Q", "protocol"),
            ("m", "method"),
        ];
        assert_eq!(defs, want.map(|(a, b)| (a.to_string(), b)));
    }
}
