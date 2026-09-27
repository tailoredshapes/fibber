//! Property: printing a form and reading the text gives an equal form,
//! and printing that gives the same text.

use super::gen::{form, Rng};
use crate::syntax::read_all;

#[test]
fn print_then_read_is_identity() {
    let mut checked = 0;
    for seed in 0..4000u64 {
        let mut rng = Rng::new(seed);
        let count = 1 + rng.below(4);
        let forms: Vec<_> = (0..count).map(|_| form(&mut rng, 4)).collect();
        let sep = if seed % 2 == 0 { " " } else { "\n" };
        let text = forms
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(sep);
        let back = read_all(&text, "rt").unwrap_or_else(|e| panic!("seed {seed}: {text:?}: {e}"));
        assert_eq!(back, forms, "seed {seed}: {text:?}");
        let again = back
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(sep);
        assert_eq!(again, text, "seed {seed}");
        for f in &back {
            assert!(f.pos.end <= text.len() && f.pos.start < f.pos.end);
        }
        checked += count;
    }
    assert!(checked > 4000, "generated only {checked} forms");
}

#[test]
fn the_generator_reaches_every_kind() {
    use crate::syntax::{Form, FormKind};
    fn walk(f: &Form, seen: &mut [bool; 11]) {
        let (i, kids): (usize, &[Form]) = match &f.kind {
            FormKind::Sym(_) => (0, &[]),
            FormKind::Kw(_) => (1, &[]),
            FormKind::Int { .. } => (2, &[]),
            FormKind::Flt { .. } => (3, &[]),
            FormKind::Str(_) => (4, &[]),
            FormKind::Chr(_) => (5, &[]),
            FormKind::Bool(_) => (6, &[]),
            FormKind::Nil => (7, &[]),
            FormKind::List(k) => (8, k),
            FormKind::Vec(k) => (9, k),
            FormKind::Map(k) => (10, k),
        };
        seen[i] = true;
        kids.iter().for_each(|k| walk(k, seen));
    }
    let mut seen = [false; 11];
    for seed in 0..200 {
        walk(&form(&mut Rng::new(seed), 4), &mut seen);
    }
    assert_eq!(seen, [true; 11]);
}
