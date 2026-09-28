//! Mutations that tend to leave a module valid, so that it reaches the
//! lowering and LLVM: the checker's front line is easy to hit, the
//! backend behind it is what these are for.
//!
//! Every op has the one signature `Op` so that a table can hold them;
//! `drop_form` needs the vector, so all of them take it.
#![allow(clippy::ptr_arg)]

use lir::diag::Pos;
use lir::sexp::Sexp;

use super::mutate::{random_type, Case};
use super::rng::Rng;
use super::tree::{count, items, nth, with_nth};

type Op = fn(&mut Rng, &[Case], &Case, &mut Vec<Sexp>) -> Option<&'static str>;

pub const OPS: [Op; 5] = [
    swap_type_everywhere,
    change_predicate,
    change_ordering,
    add_modifier,
    duplicate_block,
];

const IPREDS: [&str; 10] = [
    "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
];
const FPREDS: [&str; 14] = [
    "oeq", "one", "olt", "ole", "ogt", "oge", "ord", "uno", "ueq", "une", "ult", "ule", "ugt",
    "uge",
];
const ORDERINGS: [&str; 6] = [
    "monotonic",
    "acquire",
    "release",
    "acq_rel",
    "seq_cst",
    "unordered",
];

fn atom(text: &str) -> Sexp {
    Sexp::Atom(text.to_string(), Pos::default())
}

/// Replace every atom equal to `from` under `s` by `to`.
fn replace_all(s: &mut Sexp, from: &str, to: &Sexp) -> usize {
    match s {
        Sexp::Atom(a, _) if a == from => {
            *s = to.clone();
            1
        }
        Sexp::List(v, _) | Sexp::Brace(v, _) | Sexp::Bracket(v, _) => {
            v.iter_mut().map(|c| replace_all(c, from, to)).sum()
        }
        _ => 0,
    }
}

/// One type for another throughout one top-level form: a function
/// whose i32s all become i64s is mostly still well typed.
fn swap_type_everywhere(
    rng: &mut Rng,
    _: &[Case],
    case: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let from = ["i1", "i8", "i16", "i32", "i64", "float", "double", "ptr"][rng.below(8)];
    let to = random_type(rng, case);
    let i = rng.below(forms.len());
    (replace_all(&mut forms[i], from, &to) > 0).then_some("swap-type-everywhere")
}

/// A random node whose head is `head`, with the index of one of the
/// words to change.
fn form_with_head(rng: &mut Rng, forms: &[Sexp], head: &[&str]) -> Option<usize> {
    let total = count(forms);
    (0..40).map(|_| rng.below(total)).find(|&n| {
        nth(forms, n).is_some_and(|s| {
            items(s).is_some_and(|v| {
                v.first()
                    .and_then(Sexp::atom)
                    .is_some_and(|h| head.contains(&h))
            })
        })
    })
}

fn set_word(forms: &mut [Sexp], n: usize, at: usize, word: &str) -> bool {
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) = s {
            if at < v.len() {
                v[at] = atom(word);
            }
        }
    })
}

fn change_predicate(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = form_with_head(rng, forms, &["icmp", "fcmp"])?;
    let is_int = nth(forms, n).and_then(items).and_then(|v| v[0].atom()) == Some("icmp");
    let word = if is_int {
        IPREDS[rng.below(IPREDS.len())]
    } else {
        FPREDS[rng.below(FPREDS.len())]
    };
    set_word(forms, n, 1, word).then_some("change-predicate")
}

fn change_ordering(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = form_with_head(
        rng,
        forms,
        &[
            "atomic-load",
            "atomic-store",
            "atomicrmw",
            "cmpxchg",
            "fence",
        ],
    )?;
    let word = ORDERINGS[rng.below(ORDERINGS.len())];
    let at = 1 + rng.below(3);
    set_word(forms, n, at, word).then_some("change-ordering")
}

/// `volatile`, `inbounds` or `(align N)` slipped into a memory form.
fn add_modifier(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = form_with_head(rng, forms, &["load", "store", "alloca", "getelementptr"])?;
    let modifier = match rng.below(3) {
        0 => atom("volatile"),
        1 => atom("inbounds"),
        _ => Sexp::List(
            vec![
                atom("align"),
                atom(["1", "2", "8", "16", "64", "4096", "1073741824", "3", "0"][rng.below(9)]),
            ],
            Pos::default(),
        ),
    };
    let r = rng.next_u64() as usize;
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) = s {
            let at = 1 + r % v.len();
            v.insert(at, modifier.clone());
        }
    })
    .then_some("add-modifier")
}

/// A block copied under a fresh label: unreachable, and so under the
/// stricter dominance rule of §5.3 (item 4 of §14).
fn duplicate_block(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = form_with_head(rng, forms, &["define"])?;
    let label = format!("dup{}", rng.below(1000));
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) = s {
            let blocks: Vec<usize> = (0..v.len())
                .filter(|&i| {
                    items(&v[i]).is_some_and(|b| b.first().and_then(Sexp::atom) == Some("block"))
                })
                .collect();
            if let Some(&i) = blocks.last() {
                let mut copy = v[i].clone();
                if let Sexp::List(b, _) = &mut copy {
                    if b.len() > 1 {
                        b[1] = atom(&label);
                    }
                }
                v.push(copy);
            }
        }
    })
    .then_some("duplicate-block")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::tree::print;

    #[test]
    fn deep_ops_keep_a_typical_case_valid_more_often_than_not() {
        let src = "(define (main i32) () (block entry (let ((p (alloca i32)) (c (icmp slt (i32 1) (i32 2)))) (store (i32 5) p) (fence seq_cst) (br c a b))) (block a (ret (load i32 p))) (block b (ret (i32 0))))";
        let case = Case::new("t".into(), lir::sexp::read(src).unwrap());
        let corpus = [case];
        let (mut applied, mut valid) = (0, 0);
        for seed in 0..200 {
            let mut rng = Rng::new(seed);
            let mut forms = corpus[0].forms.clone();
            let op = OPS[rng.below(OPS.len())];
            if op(&mut rng, &corpus, &corpus[0], &mut forms).is_some() {
                applied += 1;
                let src = print(&forms);
                assert!(lir::sexp::read(&src).is_ok(), "{src}");
                valid += usize::from(lir::parse_and_check(&src).is_ok());
            }
        }
        assert!(applied > 100, "{applied}");
        assert!(valid * 2 > applied, "{valid} of {applied} valid");
    }
}
