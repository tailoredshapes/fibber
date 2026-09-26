//! Random programs that build cycles, checked against an independent
//! leak classifier. The program is legal throughout, so at the end,
//! once every binding is released, the only thing that can be live is
//! what a cycle through a cell keeps (§6): every leak must be
//! `LeakCycle`, none may dangle. A second variant forgets exactly one
//! binding: that object must be a `Leak`, and everything else must be
//! classified exactly as the rule in the heap's own documentation says
//! (reachable from a cell on a cycle, count equal to the live `Ref`
//! fields naming it).

use std::collections::{HashMap, HashSet};

use fibref::{Heap, LeakClass, ObjId};

use crate::replay::{agree, catch_up, step_once, Lcg, Model, Replay};

/// Everything reachable from `from` over live `Ref` edges, iteratively.
fn reach(succ: &HashMap<ObjId, Vec<ObjId>>, from: &[ObjId]) -> HashSet<ObjId> {
    let mut seen = HashSet::new();
    let mut stack: Vec<ObjId> = from.to_vec();
    while let Some(id) = stack.pop() {
        if seen.insert(id) {
            if let Some(next) = succ.get(&id) {
                stack.extend(next.iter().copied());
            }
        }
    }
    seen
}

/// The class the audit must give each live object, and how many live
/// `Ref` fields name it, computed from the model alone.
fn expected(model: &Model) -> HashMap<ObjId, (LeakClass, usize)> {
    let live = model.live_ids();
    let live_set: HashSet<ObjId> = live.iter().copied().collect();
    let mut succ: HashMap<ObjId, Vec<ObjId>> = HashMap::new();
    let mut held: HashMap<ObjId, usize> = HashMap::new();
    for &id in &live {
        for target in model.objects[&id].fields.iter().filter_map(|v| v.as_ref()) {
            assert!(
                live_set.contains(&target),
                "the model holds a Ref to a dead object"
            );
            succ.entry(id).or_default().push(target);
            *held.entry(target).or_insert(0) += 1;
        }
    }
    let empty: Vec<ObjId> = Vec::new();
    let roots: Vec<ObjId> = live
        .iter()
        .copied()
        .filter(|&m| {
            model.objects[&m].kind.is_mutable()
                && reach(&succ, succ.get(&m).unwrap_or(&empty)).contains(&m)
        })
        .collect();
    let on_cycle_path = reach(&succ, &roots);
    live.iter()
        .map(|&id| {
            let held = held.get(&id).copied().unwrap_or(0);
            let class = if on_cycle_path.contains(&id) && model.objects[&id].count == held {
                LeakClass::LeakCycle
            } else {
                LeakClass::Leak
            };
            (id, (class, held))
        })
        .collect()
}

/// Runs a random cyclic program, releases every binding but (when
/// `forget` is set) one count on one object, and returns the model,
/// the report and the forgotten object.
fn run(seed: u64, ops: usize, forget: bool) -> (Model, fibref::AuditReport, Option<ObjId>) {
    let mut rng = Lcg(seed);
    let mut heap = Heap::new();
    let mut model = Model::new();
    let mut replay = Replay::new();
    let mut seen = 0;
    for step in 0..ops {
        step_once(&mut rng, &mut heap, &mut model, true).unwrap_or_else(|e| {
            panic!("seed {seed} step {step}: a legal operation was refused: {e}")
        });
        catch_up(&heap, &mut replay, &mut seen, seed, step);
        agree(&heap, &model, &replay, seed, step);
    }
    let mut owned = model.owned_ids();
    let forgotten = if forget { rng.pick(&owned) } else { None };
    while !owned.is_empty() {
        let id = owned.swap_remove(rng.below(owned.len()));
        let keep = usize::from(Some(id) == forgotten);
        while model.owned.get(&id).copied().unwrap_or(0) > keep {
            heap.release(id).expect("the binding's count");
            model.release_owned(id);
        }
    }
    catch_up(&heap, &mut replay, &mut seen, seed, ops);
    agree(&heap, &model, &replay, seed, ops);
    (model, heap.finish(), forgotten)
}

/// The report lists exactly the model's live objects, in allocation
/// order, each with the expected class, count and held.
fn check(seed: u64, model: &Model, report: &fibref::AuditReport) {
    let want = expected(model);
    let listed: Vec<ObjId> = report.leaks.iter().map(|l| l.id).collect();
    assert_eq!(
        listed,
        model.live_ids(),
        "seed {seed}: the leaks are not the live objects"
    );
    assert!(
        report.dangling.is_empty(),
        "seed {seed}: a legal program dangles"
    );
    for leak in &report.leaks {
        let (class, held) = want[&leak.id];
        assert_eq!(
            (leak.class, leak.count, leak.held),
            (class, model.objects[&leak.id].count, held),
            "seed {seed}: {} misclassified (class, count, held)",
            leak.id
        );
    }
    assert!(report.ids_in(LeakClass::ImmutableCycle).is_empty());
}

#[test]
fn random_cyclic_programs_leak_only_permitted_cycles_once_every_binding_is_released() {
    let mut leaked_runs = 0;
    for seed in 1..=24 {
        let (model, report, _) = run(seed, 800, false);
        check(seed, &model, &report);
        assert!(
            report.bugs().next().is_none(),
            "seed {seed}: a legal program with every binding released has a leak that is not a cycle: {:?}",
            report.bugs().collect::<Vec<_>>()
        );
        assert!(report.is_clean() || report.is_cycle_leak_only());
        if !report.leaks.is_empty() {
            leaked_runs += 1;
        }
    }
    assert!(
        leaked_runs > 0,
        "no run built a cycle; the test exercised nothing"
    );
}

#[test]
fn one_forgotten_binding_in_a_random_cyclic_program_is_a_bug_and_nothing_else_is_misclassified() {
    let mut forgotten_runs = 0;
    for seed in 1..=24 {
        let (model, report, forgotten) = run(seed, 800, true);
        check(seed, &model, &report);
        let Some(forgotten) = forgotten else {
            continue;
        };
        forgotten_runs += 1;
        assert!(
            report.bugs().any(|l| l.id == forgotten),
            "seed {seed}: the forgotten binding on {forgotten} was not reported as a bug: {:?}",
            report.leaks
        );
        assert!(!report.is_cycle_leak_only());
        assert!(!report.is_clean());
    }
    assert!(forgotten_runs > 0, "no run had a binding to forget");
}
