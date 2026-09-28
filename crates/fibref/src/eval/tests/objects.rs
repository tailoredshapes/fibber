//! The object table is read only through the audited heap (method.md
//! rule 2): a freed object's contents are never read without an error.

use crate::eval::alloc::Placement;
use crate::eval::interp::Interp;
use crate::eval::RunError;
use crate::heap::{AuditError, Op};

fn checked() -> crate::own::Checked {
    crate::own::check_source("(defun main () -> i64 0)", "<test>").expect("checks")
}

#[test]
fn reading_a_freed_string_is_a_use_after_free() {
    let c = checked();
    let mut it = Interp::new(&c.typed, &c.owned);
    let s = it
        .new_str("gone".into(), Placement::Heap)
        .expect("allocates");
    assert_eq!(it.string(&s).expect("live"), "gone");
    it.release(&s).expect("frees");
    let id = s.expect_obj("a string").expect("an object");
    let err = it.string(&s).expect_err("a read of a freed string");
    let want = RunError::from(AuditError::UseAfterFree { id, op: Op::Read });
    assert_eq!(err.to_string(), want.to_string());
}

#[test]
fn reading_an_ended_stack_object_is_caught() {
    let c = checked();
    let mut it = Interp::new(&c.typed, &c.owned);
    let s = it
        .new_str("stack".into(), Placement::Stack)
        .expect("allocates");
    it.end_stack(&s).expect("ends");
    let id = s.expect_obj("a string").expect("an object");
    let err = it.string(&s).expect_err("a read after the scope");
    let want = AuditError::StackUseAfterScope { id, op: Op::Read };
    assert_eq!(err.to_string(), RunError::from(want).to_string());
    assert!(it.fields(id).is_err(), "the side table is audited too");
}
