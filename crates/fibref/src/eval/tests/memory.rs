//! What the heap sees: stack objects, immortal literals, unique writes,
//! weak references, shared objects, audit failures.

use super::{clean, count, traced};
use crate::cases::{run_dir, Evaluator, Outcome, Status};
use crate::eval::Interpreter;
use crate::heap::{Event, Kind};

#[test]
fn stack_object_at_owned_parameter_is_never_counted() {
    let (n, report) = traced(
        "(defun walk (b n) (if (= n 0) (unbox b) (walk (Box (+ (unbox b) 1)) (- n 1))))
         (defun main () -> i64 (let ((x (Box 31))) (+ 1 (walk x 10))))",
    );
    assert_eq!(n, 42);
    assert!(report.is_clean());
    let stack: Vec<_> = report
        .trace
        .iter()
        .filter_map(|e| match e {
            Event::AllocStack { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(stack.len(), 1, "one scope-local Box");
    let x = stack[0];
    let counted = count(
        &report,
        |e| matches!(e, Event::Retain { id, .. } | Event::Release { id, .. } if *id == x),
    );
    assert_eq!(counted, 0, "no count operation reaches a stack object");
    assert_eq!(count(&report, |e| *e == Event::Drop { id: x }), 1);
}

#[test]
fn string_literals_are_immortal_and_built_once() {
    let (n, report) = traced(
        "(defun f () \"lit\")
         (defun main () -> i64 (+ (str-len (f)) (str-len (f))))",
    );
    assert_eq!(n, 6);
    let immortal = count(&report, |e| matches!(e, Event::AllocImmortal { .. }));
    assert_eq!(immortal, 1);
}

#[test]
fn a_unique_array_is_written_in_place_and_a_shared_one_copied() {
    let (n, report) = traced(
        "(defun main () -> i64
           (let ((c (cell (array 3 0))))
             (do (array-set! &c 0 5)
                 (let ((keep @c))
                   (do (array-set! &c 1 6)
                       (+ (array-get keep 1) (array-get @c 1)))))))",
    );
    assert_eq!(n, 6);
    assert!(report.is_clean());
    assert_eq!(
        count(&report, |e| matches!(e, Event::WriteUnique { .. })),
        1
    );
}

#[test]
fn a_def_value_is_immortalised() {
    let (_, report) = traced("(def t [1 2]) (defun main () -> i64 (count t))");
    assert!(count(&report, |e| matches!(e, Event::Immortalised { .. })) > 0);
    assert!(report.is_clean());
}

#[test]
fn a_spawned_closure_is_share_marked_and_its_task_freed() {
    let (n, report) = traced(
        "(defun main () -> i64
           (let ((v [1 2 3])) (join (spawn (fn () (count v))))))",
    );
    assert_eq!(n, 3);
    assert!(report.is_clean());
    assert!(count(&report, |e| matches!(e, Event::Shared { .. })) >= 1);
}

#[test]
fn an_async_task_runs_only_when_driven() {
    clean(
        "(defun main () -> i64
           (let ((a (atom 0))
                 (t (async (do (reset! a 5) 1))))
             (+ (* 10 @a) (block-on t))))",
        1,
    );
}

#[test]
fn the_weak_box_is_an_object_of_its_own() {
    let (n, report) = traced(
        "(defun main () -> i64
           (let ((v (conj [] 1)) (w (weak v))) (if (nil? @w) 0 1)))",
    );
    assert_eq!(n, 1);
    assert!(report.is_clean());
    assert_eq!(count(&report, |e| matches!(e, Event::Weak { .. })), 1);
    let boxes = count(&report, |e| {
        matches!(
            e,
            Event::Alloc {
                kind: Kind::Immutable,
                ..
            }
        )
    });
    assert!(boxes >= 1);
}

#[test]
fn two_weak_references_to_one_object_share_its_box() {
    let (n, report) = traced(
        "(defun main () -> i64
           (let ((v (conj [] 1)) (w1 (weak v)) (w2 (weak v)))
             (if (nil? @w2) 0 (match @w1 ((some x) (count x)) (nil 0)))))",
    );
    assert_eq!(n, 1);
    assert!(report.is_clean());
    let boxes = count(&report, |e| matches!(e, Event::Weak { .. }));
    assert_eq!(boxes, 2, "two weak references taken");
    let allocs = report
        .trace
        .iter()
        .filter(|e| matches!(e, Event::Alloc { .. }))
        .count();
    let (_, one) = traced(
        "(defun main () -> i64
           (let ((v (conj [] 1)) (w1 (weak v)) (w2 w1))
             (if (nil? @w2) 0 (match @w1 ((some x) (count x)) (nil 0)))))",
    );
    let allocs_one = one
        .trace
        .iter()
        .filter(|e| matches!(e, Event::Alloc { .. }))
        .count();
    assert_eq!(allocs, allocs_one, "the second weak allocates no box");
}

/// Types §6.10 on case 07's shape: the tail call releases `acc` before
/// the jump, so each version of the vector dies as soon as the next
/// exists and the live objects stay bounded by the final vector (a trie
/// of about 1000 / 32 nodes), not by the recursion depth.
#[test]
fn a_tail_recursive_accumulator_frees_each_version_at_the_jump() {
    let (n, report) = traced(
        "(defun build (n acc) (if (= n 0) acc (build (- n 1) (conj acc n))))
         (defun main () -> i64 (count (build 1000 [])))",
    );
    assert_eq!(n, 1000);
    assert!(report.is_clean());
    let (mut live, mut max) = (0i64, 0i64);
    for e in &report.trace {
        match e {
            Event::Alloc { .. } => live += 1,
            Event::Free { .. } => live -= 1,
            _ => {}
        }
        max = max.max(live);
    }
    assert!(max < 120, "at most {max} objects live at once");
}

/// A private cell whose write-back comes while a stack temporary of a
/// later argument is still in its scope: the cell is emptied at the
/// write-back (so the move happens there) and dropped when the inner
/// scope has ended, after the temporary.
#[test]
fn a_private_cell_ends_after_a_later_arguments_stack_temporary() {
    let (n, report) = traced(
        "(defun f (&v b) (push! &v (unbox b)))
         (defun main () -> i64
           (let ((c (cell [7]))) (do (f &c (Box 5)) (+ (count @c) (nth @c 1)))))",
    );
    assert_eq!(n, 7);
    assert!(report.is_clean());
    let stack: Vec<(crate::heap::ObjId, Kind)> = report
        .trace
        .iter()
        .filter_map(|e| match e {
            Event::AllocStack { id, kind, .. } => Some((*id, *kind)),
            _ => None,
        })
        .collect();
    let drop_at = |id| report.trace.iter().position(|e| *e == Event::Drop { id });
    let cells: Vec<_> = stack.iter().filter(|(_, k)| *k == Kind::Cell).collect();
    let boxed: Vec<_> = stack
        .iter()
        .filter(|(_, k)| *k == Kind::Immutable)
        .collect();
    assert_eq!(boxed.len(), 1, "{stack:?}");
    let private = cells
        .iter()
        .find(|(id, _)| drop_at(*id) > drop_at(boxed[0].0))
        .expect("a private cell dropped after the Box");
    assert!(drop_at(private.0).is_some());
}

/// A program that allocates heap objects: two cells, each holding a
/// struct.
const COUNTED: &str = "(defstruct Pt (x: i64 y: i64))
(defun main () -> i64
  (let ((a (cell (Pt 1 2)))
        (b (cell (Pt 3 4))))
    (+ (. @a x) (. @b y))))";

/// The interpreter's count of a run is the `A` lines of its trace.
#[test]
fn the_interpreter_counts_the_a_lines_of_its_trace() {
    let (n, report) = traced(COUNTED);
    assert_eq!(n, 5);
    let a_lines = count(&report, |e| matches!(e, Event::Alloc { .. }));
    assert_eq!(crate::heap::trace_allocs(&report.trace), a_lines as u64);
    let path = std::path::Path::new("<test>");
    let (outcome, allocs) = Interpreter.run_counted(COUNTED, path);
    assert!(matches!(outcome, Outcome::Compiled { .. }), "{outcome:?}");
    assert_eq!(allocs, Some(a_lines as u64));
    assert!(a_lines >= 2, "the two cells are heap objects: {a_lines}");
}

/// A rejected program has no count, a trapped one has the count up to
/// the abort.
#[test]
fn a_run_that_did_not_start_has_no_count() {
    let path = std::path::Path::new("<test>");
    let (outcome, allocs) = Interpreter.run_counted("(defun main () -> i64 oops)", path);
    assert!(matches!(outcome, Outcome::Rejected { .. }), "{outcome:?}");
    assert_eq!(allocs, None);
    let trap = "(defun main () -> i64 (let ((v [1 2 3])) (quot (nth v 0) 0)))";
    let (outcome, allocs) = Interpreter.run_counted(trap, path);
    assert!(matches!(outcome, Outcome::Trapped { .. }), "{outcome:?}");
    assert!(matches!(allocs, Some(n) if n > 0), "{allocs:?}");
}

/// The bound is tight: the count of the program is `N`, so `<= N` passes
/// and `<= N-1` fails, through the real runner and the real interpreter.
#[test]
fn a_bound_one_below_the_count_fails_in_the_real_runner() {
    let (_, report) = traced(COUNTED);
    let n = crate::heap::trace_allocs(&report.trace);
    let dir = std::env::temp_dir().join(format!("fibref-allocs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let write = |name: &str, max: u64| {
        let head = format!(
            ";; spec: §4\n;; expect: accept\n;; result: 5\n;; audit: clean\n;; allocs: <= {max}\n"
        );
        std::fs::write(dir.join(name), format!("{head}{COUNTED}\n")).expect("write");
    };
    write("1-exact.fib", n);
    write("2-below.fib", n - 1);
    write("3-above.fib", n + 1);
    let report = run_dir(&dir, &Interpreter).expect("runs");
    let _ = std::fs::remove_dir_all(&dir);
    let status = |i: usize| report.results[i].status.clone();
    assert_eq!(status(0), Status::Pass, "{:?}", report.results);
    assert_eq!(
        status(1),
        Status::Fail(format!(
            "allocs: expected at most {} heap objects, the run allocated {n}",
            n - 1
        ))
    );
    assert_eq!(status(2), Status::Pass);
}
