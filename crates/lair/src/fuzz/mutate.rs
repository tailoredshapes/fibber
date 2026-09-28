//! Random mutations of a case's forms: the ways a compiler bug or a
//! hostile emitter might deform valid lIR.
//!
//! Every op has the one signature `Op` so that a table can hold them;
//! `drop_form` needs the vector, so all of them take it.
#![allow(clippy::ptr_arg)]

use lir::sexp::Sexp;

use super::grammar::{random_instruction, INTERESTING_NUMBERS, KEYWORDS, TYPES};
use super::rng::Rng;
use super::tree::{atoms, count, items, nth, with_nth};
use lir::diag::Pos;

/// One accept case, read once.
pub struct Case {
    pub name: String,
    pub forms: Vec<Sexp>,
    /// Every atom of the case, the pool names and labels are drawn from.
    pub atoms: Vec<String>,
}

impl Case {
    pub fn new(name: String, forms: Vec<Sexp>) -> Case {
        let atoms = atoms(&forms);
        Case { name, forms, atoms }
    }
}

/// A mutation: it changes `forms` and says what it did, or gives up
/// (`None`) when the forms offer nothing to apply it to. Every op has
/// this one signature so that OPS can hold them; most need only a
/// slice, `drop_form` needs the vector.
type Op = fn(&mut Rng, &[Case], &Case, &mut Vec<Sexp>) -> Option<&'static str>;

const OPS: [Op; 11] = [
    swap_type,
    drop_operand,
    dup_operand,
    rename_atom,
    change_number,
    insert_instruction,
    splice,
    swap_siblings,
    replace_head,
    unwrap,
    drop_form,
];

/// One to three mutations of `case`, and their names.
pub fn mutate(rng: &mut Rng, corpus: &[Case], case: &Case) -> (Vec<Sexp>, Vec<&'static str>) {
    let mut forms = case.forms.clone();
    let mut applied = Vec::new();
    let wanted = 1 + rng.below(3);
    let mut tries = 0;
    while applied.len() < wanted && tries < 20 {
        tries += 1;
        let k = rng.below(OPS.len() + super::deep::OPS.len());
        let op = if k < OPS.len() {
            OPS[k]
        } else {
            super::deep::OPS[k - OPS.len()]
        };
        if let Some(name) = op(rng, corpus, case, &mut forms) {
            applied.push(name);
        }
    }
    (forms, applied)
}

fn pos() -> Pos {
    Pos::default()
}

fn atom(text: &str) -> Sexp {
    Sexp::Atom(text.to_string(), pos())
}

/// A random node index.
fn any_node(rng: &mut Rng, forms: &[Sexp]) -> usize {
    rng.below(count(forms))
}

/// A random node satisfying `want`, found within a few tries.
fn some_node(rng: &mut Rng, forms: &[Sexp], want: &dyn Fn(&Sexp) -> bool) -> Option<usize> {
    (0..24)
        .map(|_| any_node(rng, forms))
        .find(|&n| nth(forms, n).is_some_and(want))
}

fn is_type_atom(s: &Sexp) -> bool {
    match s {
        Sexp::Atom(a, _) => TYPES.contains(&a.as_str()) || a.starts_with("%struct."),
        Sexp::VecType(..) | Sexp::Bracket(..) => true,
        _ => false,
    }
}

fn is_number(s: &Sexp) -> bool {
    s.atom().is_some_and(|a| {
        let body = a.strip_prefix('-').unwrap_or(a);
        body.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// A random type: a scalar keyword, a struct of the case, a vector or
/// an array.
pub fn random_type(rng: &mut Rng, case: &Case) -> Sexp {
    match rng.below(6) {
        0 => {
            let structs: Vec<&String> = case
                .atoms
                .iter()
                .filter(|a| a.starts_with("%struct."))
                .collect();
            rng.pick(&structs).map_or(atom("ptr"), |s| atom(s))
        }
        1 => Sexp::VecType(
            ["4", "2", "0", "1025", "8"][rng.below(5)].into(),
            ["i32", "i64", "double", "i1", "ptr"][rng.below(5)].into(),
            pos(),
        ),
        2 => Sexp::Bracket(
            vec![
                atom(["4", "0", "1", "4294967296", "-1"][rng.below(5)]),
                atom("x"),
                atom(TYPES[rng.below(TYPES.len())]),
            ],
            pos(),
        ),
        _ => atom(TYPES[rng.below(TYPES.len())]),
    }
}

fn swap_type(
    rng: &mut Rng,
    _: &[Case],
    case: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = some_node(rng, forms, &is_type_atom)?;
    let ty = random_type(rng, case);
    with_nth(forms, n, &mut |s| *s = ty.clone()).then_some("swap-type")
}

fn drop_operand(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = some_node(rng, forms, &|s| items(s).is_some_and(|v| v.len() >= 2))?;
    let r = rng.next_u64() as usize;
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) | Sexp::Brace(v, _) | Sexp::Bracket(v, _) = s {
            let i = 1 + r % (v.len() - 1);
            v.remove(i);
        }
    })
    .then_some("drop-operand")
}

fn dup_operand(rng: &mut Rng, _: &[Case], _: &Case, forms: &mut Vec<Sexp>) -> Option<&'static str> {
    let n = some_node(rng, forms, &|s| items(s).is_some_and(|v| !v.is_empty()))?;
    let r = rng.next_u64() as usize;
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) | Sexp::Brace(v, _) | Sexp::Bracket(v, _) = s {
            let i = r % v.len();
            let copy = v[i].clone();
            v.insert(i, copy);
        }
    })
    .then_some("dup-operand")
}

fn rename_atom(
    rng: &mut Rng,
    _: &[Case],
    case: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = some_node(rng, forms, &|s| s.atom().is_some())?;
    let name = rng.pick(&case.atoms)?.clone();
    with_nth(forms, n, &mut |s| *s = atom(&name)).then_some("rename-atom")
}

fn change_number(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = some_node(rng, forms, &is_number)?;
    let v = INTERESTING_NUMBERS[rng.below(INTERESTING_NUMBERS.len())];
    with_nth(forms, n, &mut |s| *s = atom(v)).then_some("change-number")
}

fn insert_instruction(
    rng: &mut Rng,
    _: &[Case],
    case: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = some_node(
        rng,
        forms,
        &|s| matches!(s, Sexp::List(v, _) if !v.is_empty()),
    )?;
    let instr = random_instruction(rng, case);
    let r = rng.next_u64() as usize;
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) = s {
            let i = 1 + r % v.len();
            v.insert(i, instr.clone());
        }
    })
    .then_some("insert-instruction")
}

/// A subtree of another case in place of a node of this one.
fn splice(rng: &mut Rng, corpus: &[Case], _: &Case, forms: &mut Vec<Sexp>) -> Option<&'static str> {
    let other = rng.pick(corpus)?;
    let donor = nth(&other.forms, any_node(rng, &other.forms))?.clone();
    let n = any_node(rng, forms);
    with_nth(forms, n, &mut |s| *s = donor.clone()).then_some("splice")
}

fn swap_siblings(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = some_node(rng, forms, &|s| items(s).is_some_and(|v| v.len() >= 2))?;
    let (a, b) = (rng.next_u64() as usize, rng.next_u64() as usize);
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) | Sexp::Brace(v, _) | Sexp::Bracket(v, _) = s {
            let (i, j) = (a % v.len(), b % v.len());
            v.swap(i, j);
        }
    })
    .then_some("swap-siblings")
}

fn replace_head(
    rng: &mut Rng,
    _: &[Case],
    _: &Case,
    forms: &mut Vec<Sexp>,
) -> Option<&'static str> {
    let n = some_node(
        rng,
        forms,
        &|s| matches!(s, Sexp::List(v, _) if v.first().is_some_and(|h| h.atom().is_some())),
    )?;
    let word = KEYWORDS[rng.below(KEYWORDS.len())];
    with_nth(forms, n, &mut |s| {
        if let Sexp::List(v, _) = s {
            v[0] = atom(word);
        }
    })
    .then_some("replace-head")
}

/// A list replaced by one of its children.
fn unwrap(rng: &mut Rng, _: &[Case], _: &Case, forms: &mut Vec<Sexp>) -> Option<&'static str> {
    let n = some_node(rng, forms, &|s| items(s).is_some_and(|v| v.len() >= 2))?;
    let r = rng.next_u64() as usize;
    with_nth(forms, n, &mut |s| {
        if let Some(v) = items(s) {
            let child = v[r % v.len()].clone();
            *s = child;
        }
    })
    .then_some("unwrap")
}

fn drop_form(rng: &mut Rng, _: &[Case], _: &Case, forms: &mut Vec<Sexp>) -> Option<&'static str> {
    if forms.len() < 2 {
        return None;
    }
    let i = rng.below(forms.len());
    forms.remove(i);
    Some("drop-form")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::tree::print;

    fn case(src: &str) -> Case {
        Case::new("t".into(), lir::sexp::read(src).unwrap())
    }

    #[test]
    fn every_op_applies_to_a_small_case_and_is_deterministic() {
        let c = case("(declare printf i32 (ptr ...))\n(define (main i32) ((i32 argc) (ptr argv)) (block entry (let ((p (alloca %struct.s)) (x (add argc (i32 -7)))) (store x p) (br (icmp slt x (i32 4)) a b))) (block a (ret (load i32 p))) (block b (ret (i32 0))))");
        let corpus = [c];
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..400 {
            let mut rng = Rng::new(seed);
            let (forms, ops) = mutate(&mut rng, &corpus, &corpus[0]);
            seen.extend(ops.iter().copied());
            let again = mutate(&mut Rng::new(seed), &corpus, &corpus[0]);
            assert_eq!(print(&forms), print(&again.0));
            assert!(lir::sexp::read(&print(&forms)).is_ok(), "{}", print(&forms));
        }
        for op in [
            "swap-type",
            "drop-operand",
            "dup-operand",
            "rename-atom",
            "change-number",
            "insert-instruction",
            "splice",
            "swap-siblings",
            "replace-head",
            "unwrap",
            "drop-form",
        ] {
            assert!(seen.contains(op), "{op} never applied");
        }
    }
}
