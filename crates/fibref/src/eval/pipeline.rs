//! The whole pipeline (`spec/method.md` rule 2): read, expand (user
//! macros run by [`MacroEvaluator`]), type, check ownership, evaluate
//! the `def`s and then `main`, and audit the heap at the end.

use crate::cases::{AuditSummary, Evaluator, Outcome, Value};
use crate::expand::{expand_program, ExpandCtx};
use crate::heap::{AuditReport, LeakClass};
use crate::own::program::BodyKey;
use crate::own::{check_forms, CheckError, Checked};
use crate::syntax::read_all;
use crate::types::infer::UnitRef;
use crate::types::prelude_forms;

use super::alloc::Placement;
use super::call::{Jump, Target};
use super::error::{RunError, RunErrorKind, R};
use super::interp::Interp;
use super::macros::MacroEvaluator;
use super::threads::CloseOnDrop;
use super::value::Val;
use crate::own::program::OwnedProgram;
use crate::types::TypedProgram;

/// The stack the evaluator runs on. Evaluation stops with a depth
/// error when it has used [`STACK_BUDGET`](super::STACK_BUDGET) of it
/// (`tests/eval_adversarial.rs` runs a recursion into that error).
pub const STACK_BYTES: usize = 1024 * 1024 * 1024;

/// The reference interpreter as a case [`Evaluator`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Interpreter;

impl Evaluator for Interpreter {
    fn run(&self, source: &str) -> Outcome {
        run_source(source, "<case>")
    }
}

/// Runs the program `source` (named `file` in positions) on a thread
/// with [`STACK_BYTES`] of stack.
pub fn run_source(source: &str, file: &str) -> Outcome {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("fibref-eval".into())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, || run_here(source, file));
        match worker.map(|h| h.join()) {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Outcome::Failed {
                message: "internal error: the evaluator panicked".into(),
            },
            Err(e) => Outcome::Failed {
                message: format!("cannot start the evaluator: {e}"),
            },
        }
    })
}

/// The pipeline on the calling thread.
fn run_here(source: &str, file: &str) -> Outcome {
    let checked = match check(source, file) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    let (result, report) = run_checked(&checked);
    match result {
        Ok(n) => Outcome::Compiled {
            result: Value::Int(n),
            audit: summary(&report),
        },
        Err(e) if e.kind == RunErrorKind::Trap => Outcome::Trapped {
            message: e.to_string(),
            errors: abort_errors(&report),
        },
        Err(e) => Outcome::Failed {
            message: e.to_string(),
        },
    }
}

/// The audit of a run that a trap aborted (types §2.12): what is live
/// then is not a leak and the scopes open then are not errors, but a
/// live object that still refers to a freed one is.
pub fn abort_errors(report: &AuditReport) -> Vec<String> {
    report.dangling.iter().map(dangling_text).collect()
}

fn dangling_text(d: &crate::heap::DanglingRef) -> String {
    format!(
        "dangling reference {}.{} -> {}",
        d.holder, d.field, d.target
    )
}

/// The front end with user macros run: the checked program, or the
/// outcome that stops the pipeline.
fn check(source: &str, file: &str) -> Result<Checked, Outcome> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).map_err(|m| Outcome::Failed {
        message: format!("the prelude does not expand: {m}"),
    })?;
    let forms = read_all(source, file).map_err(|e| Outcome::Rejected {
        message: e.to_string(),
    })?;
    let mut runner = MacroEvaluator::new(&forms, prelude.clone());
    let forms = expand_program(forms, &mut ctx, &mut runner).map_err(|e| Outcome::Rejected {
        message: e.to_string(),
    })?;
    check_forms(&forms, &prelude).map_err(|e| match e {
        CheckError::Internal(m) | CheckError::Prelude(m) => Outcome::Failed { message: m },
        e => Outcome::Rejected {
            message: e.to_string(),
        },
    })
}

/// Evaluates a checked program: `main`'s result (or why the run
/// stopped) and the heap's audit.
/// Runs on the calling thread, which must have [`STACK_BYTES`] of stack
/// ([`run_source`] makes one).
pub fn run_checked(c: &Checked) -> (R<i64>, AuditReport) {
    with_threads(&c.typed, &c.owned, None, |it| {
        let result = it.run_main();
        (result, it.finish())
    })
}

/// Runs `f` on a new interpreter over `p` and `o` (in the expansion
/// context `ctx`, for a macro) that can start threads (`threads`):
/// their OS threads are scoped to this call, and every thread the
/// program started has finished when it returns.
pub fn with_threads<T>(
    p: &TypedProgram,
    o: &OwnedProgram,
    ctx: Option<&ExpandCtx>,
    f: impl for<'s> FnOnce(&mut Interp<'s>) -> T,
) -> T {
    std::thread::scope(|s| {
        let mut it = Interp::new(p, o);
        it.ctx = ctx;
        it.sched.spawner = Some(s);
        // Declared after `it`, so dropped first: the idle workers are
        // released before the interpreter goes.
        let _close = CloseOnDrop(std::sync::Arc::clone(&it.sched.turn));
        let r = f(&mut it);
        it.drain();
        r
    })
}

impl<'p> Interp<'p> {
    /// The `def`s in checking order (syntax §3.19: each value made
    /// immortal), then `main`; then, `main` having returned, every
    /// thread still running is waited for (§6.8). The first error or
    /// trap of any thread stops the run.
    pub fn run_main(&mut self) -> R<i64> {
        let r = self.run_main_thread();
        if let Err(e) = &r {
            self.sched.stop(e.clone());
        }
        self.drain();
        match self.sched.abort.clone() {
            Some(e) => Err(e),
            None => r,
        }
    }

    fn run_main_thread(&mut self) -> R<i64> {
        self.eval_defs()?;
        let main = self
            .p
            .fun("main")
            .ok_or_else(|| RunError::internal("the program has no main"))?;
        let v = self.call_body(BodyKey::Fun(main), Vec::new())?;
        v.as_int()
    }

    /// Evaluates every `def` (syntax §3.19, types §8.2).
    pub fn eval_defs(&mut self) -> R<()> {
        for u in &self.p.units {
            if let UnitRef::Def(d) = u {
                let v = self.call_body(BodyKey::Def(*d), Vec::new())?;
                if let Some(id) = v.obj() {
                    self.heap.immortalise(id)?;
                }
                self.defs[d.0 as usize] = Some(v);
            }
        }
        Ok(())
    }

    /// An ordinary call of a body from outside the program.
    pub fn call_body(&mut self, key: BodyKey, args: Vec<Val>) -> R<Val> {
        let pos = match key {
            BodyKey::Fun(f) | BodyKey::AllOwned(f) => self.p.globals.fun(f).pos.clone(),
            BodyKey::Def(d) => self.p.globals.def(d).pos.clone(),
            BodyKey::Method(i, m) | BodyKey::MethodOwned(i, m) => {
                self.p.globals.instances[i].methods[m].pos.clone()
            }
        };
        self.invoke(Jump {
            target: Target::Body(key),
            args,
            pos,
            placement: Placement::Heap,
        })
    }

    /// Ends the run, once every thread has finished: the heap's audit.
    pub fn finish(&mut self) -> AuditReport {
        self.drain();
        std::mem::take(&mut self.heap).finish()
    }
}

/// The harness's view of an audit report.
pub fn summary(report: &AuditReport) -> AuditSummary {
    let mut errors = abort_errors(report);
    errors.extend(
        report
            .open_scopes
            .iter()
            .map(|s| format!("{s} never ended")),
    );
    errors.extend(
        report
            .leaks
            .iter()
            .filter(|l| l.class == LeakClass::ImmutableCycle)
            .map(|l| format!("{} is on a cycle of immutable objects", l.id)),
    );
    AuditSummary {
        clean: report.is_clean(),
        leak_cycles: report.ids_in(LeakClass::LeakCycle).len(),
        leaks: report.bugs().count(),
        errors,
    }
}
