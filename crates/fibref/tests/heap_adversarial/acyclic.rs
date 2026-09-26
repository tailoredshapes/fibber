//! A differential check on random acyclic programs. Each program is
//! run against the heap and, in step, against the two independent
//! models in `replay`: a plain reference counter written from
//! `spec/ownership.md` §2, and a replay of the heap's own trace. After
//! every operation all three must agree on which objects are live and
//! what their counts are, and the trace must be well formed at every
//! point (`spec/method.md` rule 6: the trace is what the compiler is
//! compared against, so a trace that does not describe the heap is
//! worthless). At the end every binding is released, so nothing may be
//! live.
//!
//! The program is acyclic by construction (a `Ref` only ever points at
//! an older object), so the audit must come back clean.

use fibref::Heap;

use crate::replay::{agree, catch_up, step_once, Lcg, Model, Replay};

/// Runs one random program of `ops` operations, then releases every
/// count the program still owns and audits.
fn run(seed: u64, ops: usize) {
    let mut rng = Lcg(seed);
    let mut heap = Heap::new();
    let mut model = Model::new();
    let mut replay = Replay::new();
    let mut seen = 0;
    for step in 0..ops {
        step_once(&mut rng, &mut heap, &mut model, false).unwrap_or_else(|e| {
            panic!("seed {seed} step {step}: a legal operation was refused: {e}")
        });
        catch_up(&heap, &mut replay, &mut seen, seed, step);
        agree(&heap, &model, &replay, seed, step);
    }
    let mut owned = model.owned_ids();
    while !owned.is_empty() {
        let id = owned.swap_remove(rng.below(owned.len()));
        while model.owned.get(&id).copied().unwrap_or(0) > 0 {
            heap.release(id).expect("the binding's count");
            model.release_owned(id);
        }
    }
    catch_up(&heap, &mut replay, &mut seen, seed, ops);
    agree(&heap, &model, &replay, seed, ops);
    assert!(
        model.live_ids().is_empty(),
        "the model still has live objects"
    );
    assert!(
        replay.pending_free.is_none() && replay.pending_retain.is_none(),
        "seed {seed}: the trace ends mid-operation"
    );
    let allocated = model.ids.len();
    let report = heap.finish();
    assert!(
        report.is_clean(),
        "seed {seed}: an acyclic program with every binding released leaked: {:?}",
        report.leaks
    );
    assert_eq!((report.allocated, report.freed), (allocated, allocated));
}

#[test]
fn random_acyclic_programs_agree_with_the_counting_model_and_their_own_trace() {
    for seed in 1..=12 {
        run(seed, 1500);
    }
}

#[test]
fn a_long_random_program_agrees_with_the_counting_model_and_its_own_trace() {
    run(0xf1bbe7, 6000);
}
