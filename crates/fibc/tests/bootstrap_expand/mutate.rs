//! Mutations of a program's source that keep it reading (spec/bootstrap.md
//! §5): delete a top-level form, duplicate one, swap two, wrap one in
//! `(do ..)`, rename a symbol. Each works on the byte spans the Rust
//! reader gives, and a mutant that no longer reads is dropped, so the
//! expander, not the reader, is what the mutant reaches.

use fibref::syntax::{read_all, Form, FormKind};

use crate::rng::Rng;

/// Names a rename may pick besides the symbols of the program: core forms,
/// prelude macros and reflection calls, which change what a form means.
const NAMES: [&str; 22] = [
    "if",
    "let",
    "match",
    "fn",
    "do",
    "quote",
    "quasiquote",
    "unquote",
    "defun",
    "defmacro",
    "derive",
    "when",
    "cond",
    "and",
    "->",
    "->>",
    "dotimes",
    "for-each",
    "range",
    "nil",
    "&",
    "struct-fields",
];

/// The top-level forms of `text` as byte spans, if it reads.
fn spans(text: &str) -> Option<Vec<(usize, usize)>> {
    let forms = read_all(text, "m").ok()?;
    Some(forms.iter().map(|f| (f.pos.start, f.pos.end)).collect())
}

/// Every symbol of `forms` whose position covers exactly its own name (a
/// prefix form's head symbol covers its prefix characters, and is not
/// one), as `(start, end)`.
fn symbols(text: &str, forms: &[Form], out: &mut Vec<(usize, usize)>) {
    for f in forms {
        match &f.kind {
            FormKind::Sym(s) if text.get(f.pos.start..f.pos.end) == Some(s.as_str()) => {
                out.push((f.pos.start, f.pos.end));
            }
            FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) => {
                symbols(text, items, out);
            }
            _ => {}
        }
    }
}

fn delete(text: &str, spans: &[(usize, usize)], rng: &mut Rng) -> String {
    let (s, e) = spans[rng.below(spans.len())];
    let e = if text[e..].starts_with('\n') {
        e + 1
    } else {
        e
    };
    format!("{}{}", &text[..s], &text[e..])
}

fn duplicate(text: &str, spans: &[(usize, usize)], rng: &mut Rng) -> String {
    let (s, e) = spans[rng.below(spans.len())];
    format!("{}\n{}{}", &text[..e], &text[s..e], &text[e..])
}

fn swap(text: &str, spans: &[(usize, usize)], rng: &mut Rng) -> String {
    let (a, b) = (rng.below(spans.len()), rng.below(spans.len()));
    let (i, j) = (a.min(b), a.max(b));
    if i == j {
        return text.to_string();
    }
    let ((si, ei), (sj, ej)) = (spans[i], spans[j]);
    format!(
        "{}{}{}{}{}",
        &text[..si],
        &text[sj..ej],
        &text[ei..sj],
        &text[si..ei],
        &text[ej..]
    )
}

fn wrap(text: &str, spans: &[(usize, usize)], rng: &mut Rng) -> String {
    let (s, e) = spans[rng.below(spans.len())];
    format!("{}(do {}){}", &text[..s], &text[s..e], &text[e..])
}

/// One symbol of the program replaced by another name: one from the
/// program or one of [`NAMES`].
fn rename(text: &str, rng: &mut Rng) -> String {
    let Ok(forms) = read_all(text, "m") else {
        return text.to_string();
    };
    let mut syms = Vec::new();
    symbols(text, &forms, &mut syms);
    if syms.is_empty() {
        return text.to_string();
    }
    let (s, e) = syms[rng.below(syms.len())];
    let name = if rng.one_in(2) {
        let (ns, ne) = syms[rng.below(syms.len())];
        text[ns..ne].to_string()
    } else {
        rng.pick(&NAMES).to_string()
    };
    format!("{}{name}{}", &text[..s], &text[e..])
}

/// One mutation of `text`, which reads; the mutant, if it reads too.
fn once(text: &str, rng: &mut Rng) -> Option<String> {
    let spans = spans(text)?;
    let mutant = match (spans.is_empty(), rng.below(5)) {
        (true, _) | (_, 4) => rename(text, rng),
        (false, 0) => delete(text, &spans, rng),
        (false, 1) => duplicate(text, &spans, rng),
        (false, 2) => swap(text, &spans, rng),
        (false, _) => wrap(text, &spans, rng),
    };
    read_all(&mutant, "m").ok().map(|_| mutant)
}

/// `text` mutated one to three times, each step a mutant that reads; the
/// last such mutant, or `None` if none was made.
pub fn mutate(text: &str, rng: &mut Rng) -> Option<String> {
    let mut current = text.to_string();
    let steps = rng.between(1, 3);
    let mut changed = false;
    for _ in 0..steps {
        if let Some(next) = once(&current, rng) {
            changed |= next != current;
            current = next;
        }
    }
    changed.then_some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "(defstruct P (a: i64))\n(defun f (x: i64) -> i64 (when x 1))\n'(q r)\n";

    #[test]
    fn each_operation_changes_the_text_and_keeps_it_reading_or_is_dropped() {
        let spans = spans(SOURCE).expect("reads");
        assert_eq!(spans.len(), 3);
        let mut rng = Rng::new(1);
        assert_eq!(
            delete(SOURCE, &[spans[1]], &mut rng),
            "(defstruct P (a: i64))\n'(q r)\n"
        );
        assert_eq!(
            duplicate(SOURCE, &[spans[0]], &mut rng),
            format!("(defstruct P (a: i64))\n{}", SOURCE)
        );
        assert_eq!(
            wrap(SOURCE, &[spans[2]], &mut rng),
            "(defstruct P (a: i64))\n(defun f (x: i64) -> i64 (when x 1))\n(do '(q r))\n"
        );
        let swapped = swap(SOURCE, &spans, &mut Rng::new(5));
        assert!(read_all(&swapped, "m").is_ok());
    }

    #[test]
    fn a_swap_exchanges_two_forms_and_keeps_what_lies_between() {
        let text = "(a)\n; between\n(b)";
        let sp = spans(text).expect("reads");
        // Choose the two forms by seeding until the two differ.
        let swapped = (0..20)
            .map(|s| swap(text, &sp, &mut Rng::new(s)))
            .find(|t| t != text)
            .expect("some seed picks two forms");
        assert_eq!(swapped, "(b)\n; between\n(a)");
    }

    #[test]
    fn a_rename_replaces_one_symbol_and_never_a_prefix_forms_head() {
        let text = "'x";
        let forms = read_all(text, "m").expect("reads");
        let mut syms = Vec::new();
        symbols(text, &forms, &mut syms);
        assert_eq!(
            syms,
            [(1, 2)],
            "only x: the head `quote` covers the quote mark"
        );
        let renamed = rename("(f a b)", &mut Rng::new(3));
        assert_ne!(renamed, "(f a b)");
        assert!(read_all(&renamed, "m").is_ok());
    }

    #[test]
    fn mutants_are_deterministic_read_and_differ_from_the_original() {
        let a = (0..30)
            .filter_map(|s| mutate(SOURCE, &mut Rng::new(s)))
            .collect::<Vec<_>>();
        let b = (0..30)
            .filter_map(|s| mutate(SOURCE, &mut Rng::new(s)))
            .collect::<Vec<_>>();
        assert_eq!(a, b);
        assert!(a.len() >= 20, "{}", a.len());
        for m in &a {
            assert!(read_all(m, "m").is_ok(), "{m:?}");
            assert_ne!(m, SOURCE);
        }
    }

    #[test]
    fn text_that_does_not_read_has_no_mutants() {
        assert_eq!(mutate("(a", &mut Rng::new(1)), None);
    }
}
