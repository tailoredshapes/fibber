//! Stack lifetimes that do not nest (types §6.11, §8.2): each stack
//! object ends where the plan ends it, in any order.

use super::{clean, traced};
use crate::heap::Event;

const TEMP_BEFORE_BINDING: &str = "(defstruct Pt (x: i64 y: i64))
     (defstruct Holder (v: i64))
     (defun unhold (h: Holder) -> i64 (. h v))
     (defun main () -> i64 (let ((h (Holder (. (Pt 1 2) x)))) (unhold h)))";

/// The temporary `(Pt 1 2)` is made before the `let` binding `h` and
/// ends at its step's end (syntax §2), while `h` lives on to the
/// `let`'s exit: the first object made is the first dropped.
#[test]
fn a_stack_temporary_ends_before_a_later_binding() {
    let (n, report) = traced(TEMP_BEFORE_BINDING);
    assert_eq!(n, 1);
    assert!(report.is_clean());
    let stack: Vec<_> = report
        .trace
        .iter()
        .filter_map(|e| match e {
            Event::AllocStack { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(stack.len(), 2, "Pt and Holder are both scope-local");
    let drops: Vec<_> = report
        .trace
        .iter()
        .filter_map(|e| match e {
            Event::Drop { id } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(drops, stack, "Pt (made first) is dropped first");
}

/// The same shape with the temporary a stack cell read through `@`.
#[test]
fn a_stack_cell_temporary_ends_before_a_later_binding() {
    clean(
        "(defstruct Holder (v: i64))
         (defun unhold (h: Holder) -> i64 (. h v))
         (defun main () -> i64 (let ((h (Holder @(cell 4)))) (unhold h)))",
        4,
    );
}
