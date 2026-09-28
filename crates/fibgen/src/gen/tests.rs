use super::*;
use crate::model::{expected, ModelError};
use crate::print;

#[test]
fn same_seed_same_program() {
    assert_eq!(generate(42, 4), generate(42, 4));
    assert_ne!(
        print::program(&generate(42, 4)),
        print::program(&generate(43, 4))
    );
}

/// Every generated program is one the model can evaluate: the
/// generator keeps programs inside the model's language, and the only
/// traps it makes on purpose are integer arithmetic left unmasked
/// (gen/nums.rs).
#[test]
fn the_model_evaluates_every_generated_program() {
    for seed in 0..300 {
        let p = generate(seed, 1 + (seed % 6) as u32);
        match expected(&p) {
            Ok(_) => {}
            Err(ModelError::Budget) => {}
            Err(ModelError::Trap(m)) if m.starts_with("integer ") => {}
            Err(e) => panic!("seed {seed}: {e:?}\n{}", print::program(&p)),
        }
    }
}

#[test]
fn main_has_type_i64_and_helpers_are_annotated() {
    let p = generate(7, 5);
    assert_eq!(p.main.ty, Ty::Int);
    let text = print::program(&p);
    assert!(text.contains("(defun main () -> i64"));
    for f in &p.funs {
        assert!(text.contains(&format!("(defun {}", f.name)));
    }
}

#[test]
fn larger_sizes_make_larger_programs() {
    let small: usize = (0..50).map(|s| generate(s, 1).size()).sum();
    let large: usize = (0..50).map(|s| generate(s, 6).size()).sum();
    assert!(large > small, "{large} <= {small}");
}

/// An impl on `(Hook k)` never uses `self` as a value, which is legal
/// only under a `(Hook :local)` head, and a `Rank` impl for `(Hook k)`
/// has a `Score` impl for `(Hook k)` (types §1.3, §4.1 rule 1).
#[test]
fn rigid_hook_impls_read_self_only_through_fields() {
    for seed in 0..400 {
        let p = generate(seed, 1 + (seed % 6) as u32);
        let hooks: Vec<&ImplDef> = p
            .impls
            .iter()
            .filter(|i| matches!(i.target, Ty::Hook(_)))
            .collect();
        for i in &hooks {
            if i.colour_var {
                assert!(!i.methods.iter().any(|m| self_as_value(&m.body)));
            }
        }
        let rigid = |q: crate::ty::Proto| hooks.iter().any(|i| i.proto == q && i.colour_var);
        if rigid(crate::ty::Proto::Rank) {
            assert!(rigid(crate::ty::Proto::Score), "seed {seed}");
        }
    }
}
