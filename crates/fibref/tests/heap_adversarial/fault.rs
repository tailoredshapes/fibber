//! Fault injection: a random legal program with exactly one fault
//! slipped in at a random point, either a release the program does not
//! own (an over-release) or a retain it never gives back (a forgotten
//! release). Under the counting semantics of §2 neither can go
//! unnoticed for ever: an over-release either frees an object something
//! still holds (a release of it, or a `Ref` to it at finish, is then a
//! double free) or leaves it live with fewer counts than holders; a
//! forgotten release leaves it live with more. So every run must end
//! with the heap refusing an operation or the report naming a bug. A
//! run that ends clean, or "cycle leak only", is the audit missing a
//! violation (`spec/method.md` rule 2).

use fibref::{AuditError, AuditReport, Heap, ObjId};

use crate::replay::{agree, catch_up, step_once, Lcg, Model, Replay};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    OverRelease,
    ForgottenRelease,
}

/// How a run's fault was caught.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Caught {
    /// The heap refused the faulty release itself.
    AtInjection,
    /// A later legal operation, or the final release of a binding, was
    /// refused as a double free or use after free.
    Refused,
    /// Nothing was refused; the report named a bug or a dangling `Ref`.
    AtFinish,
    /// Nothing was live to fault; the run stayed legal and clean.
    NoFault,
}

/// Mirrors an over-release the heap accepted: count -= 1 with no
/// binding behind it, freeing and cascading at zero. The heap planned
/// the whole cascade and found no freed object on it, so neither may
/// this.
fn force_release(model: &mut Model, root: ObjId) {
    let mut work = vec![root];
    while let Some(id) = work.pop() {
        let shadow = model.shadow(id);
        assert!(
            shadow.live,
            "the heap accepted a cascade through freed {id}"
        );
        shadow.count -= 1;
        if shadow.count == 0 {
            shadow.live = false;
            let fields = std::mem::take(&mut shadow.fields);
            work.extend(fields.iter().rev().filter_map(|v| v.as_ref()));
        }
    }
}

/// Injects the fault on a random live object. `Ok(Some)` names it,
/// `Ok(None)` means nothing was live, `Err` means the heap refused the
/// faulty operation on the spot.
fn inject(
    rng: &mut Lcg,
    heap: &mut Heap,
    model: &mut Model,
    fault: Fault,
) -> Result<Option<ObjId>, AuditError> {
    let Some(victim) = rng.pick(&model.live_ids()) else {
        return Ok(None);
    };
    match fault {
        Fault::OverRelease => {
            heap.release(victim)?;
            force_release(model, victim);
        }
        Fault::ForgottenRelease => {
            heap.retain(victim)?;
            model.shadow(victim).count += 1;
        }
    }
    Ok(Some(victim))
}

fn is_double_free(error: AuditError) -> bool {
    matches!(
        error,
        AuditError::ReleaseOfFreed { .. } | AuditError::UseAfterFree { .. }
    )
}

/// Releases every count the program still owns; the first refusal
/// ends the run.
fn teardown(rng: &mut Lcg, heap: &mut Heap, model: &mut Model) -> Result<(), AuditError> {
    let mut owned = model.owned_ids();
    while !owned.is_empty() {
        let id = owned.swap_remove(rng.below(owned.len()));
        while model.owned.get(&id).copied().unwrap_or(0) > 0 {
            heap.release(id)?;
            model.release_owned(id);
        }
    }
    Ok(())
}

/// A refusal is legitimate only after the fault went in, and only as a
/// double free or use after free. `at` says where it happened.
fn assert_refused_by_fault(seed: u64, at: &str, victim: Option<ObjId>, error: AuditError) {
    assert!(
        victim.is_some(),
        "seed {seed} {at}: refused before any fault: {error}"
    );
    assert!(
        is_double_free(error),
        "seed {seed} {at}: refused with {error}"
    );
}

/// Nothing was refused, so the report must name the fault; with no
/// victim the run was legal and must be clean.
fn assert_reported(seed: u64, fault: Fault, victim: Option<ObjId>, report: &AuditReport) -> Caught {
    let Some(victim) = victim else {
        assert!(report.is_clean() || report.is_cycle_leak_only());
        return Caught::NoFault;
    };
    assert!(
        report.bugs().next().is_some() || !report.dangling.is_empty(),
        "seed {seed}: an injected {fault:?} on {victim} went unreported: leaks {:?}, dangling {:?}",
        report.leaks,
        report.dangling
    );
    Caught::AtFinish
}

fn run(seed: u64, ops: usize, fault: Fault) -> Caught {
    let mut rng = Lcg(seed);
    let mut heap = Heap::new();
    let mut model = Model::new();
    let mut replay = Replay::new();
    let mut seen = 0;
    let fault_at = ops / 4 + rng.below(ops / 2);
    let mut victim = None;
    for step in 0..ops {
        if step == fault_at {
            match inject(&mut rng, &mut heap, &mut model, fault) {
                Ok(v) => victim = v,
                Err(e) => {
                    assert!(is_double_free(e), "seed {seed}: refused with {e}");
                    return Caught::AtInjection;
                }
            }
        }
        let result = step_once(&mut rng, &mut heap, &mut model, true);
        catch_up(&heap, &mut replay, &mut seen, seed, step);
        match result {
            Ok(()) => agree(&heap, &model, &replay, seed, step),
            Err(e) => {
                assert_refused_by_fault(seed, &format!("step {step}"), victim, e);
                return Caught::Refused;
            }
        }
    }
    if let Err(e) = teardown(&mut rng, &mut heap, &mut model) {
        assert_refused_by_fault(seed, "teardown", victim, e);
        return Caught::Refused;
    }
    assert_reported(seed, fault, victim, &heap.finish())
}

/// Runs every seed with `fault` and returns how each was caught.
fn sweep(fault: Fault) -> Vec<Caught> {
    (1..=48).map(|seed| run(seed, 600, fault)).collect()
}

#[test]
fn one_injected_over_release_is_always_caught() {
    let caught = sweep(Fault::OverRelease);
    let refused = caught.iter().filter(|c| **c == Caught::Refused).count();
    let at_finish = caught.iter().filter(|c| **c == Caught::AtFinish).count();
    eprintln!("over-release: {caught:?}");
    assert!(
        caught.iter().all(|c| *c != Caught::NoFault),
        "no run injected anything"
    );
    assert!(
        refused + at_finish > 0,
        "every fault was refused on the spot; nothing was exercised"
    );
}

#[test]
fn one_forgotten_release_is_always_caught_at_finish() {
    let caught = sweep(Fault::ForgottenRelease);
    eprintln!("forgotten release: {caught:?}");
    // A surplus count can never make the heap refuse anything; it can
    // only show up as a leak.
    assert!(
        caught.iter().all(|c| *c == Caught::AtFinish),
        "a forgotten release was caught some other way, or not at all: {caught:?}"
    );
}
