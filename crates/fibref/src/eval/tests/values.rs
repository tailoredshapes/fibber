//! Scalars, strings, `Option`, enums, conversions, dispatch.

use super::{clean, failed};

#[test]
fn arithmetic_traps_on_overflow_and_division_by_zero() {
    // types §2.12 (Decided, owner, 2026-09-27): Rust's semantics.
    clean("(defun main () -> i64 (sext i64 (+ 126i8 1i8)))", 127);
    let msg = failed("(defun main () -> i64 (sext i64 (+ 127i8 1i8)))");
    assert!(msg.contains("trap: integer overflow in + at i8"), "{msg}");
    let msg = failed("(defun main () -> i64 (/ 1 (- 1 1)))");
    assert!(msg.contains("trap: integer / by zero"), "{msg}");
    let min = "(- (- 0 9223372036854775807) 1)";
    let msg = failed(&format!("(defun main () -> i64 (/ {min} -1))"));
    assert!(msg.contains("trap: integer overflow in / at i64"), "{msg}");
    let msg = failed(&format!("(defun main () -> i64 (neg {min}))"));
    assert!(msg.contains("trap: integer overflow in neg"), "{msg}");
}

#[test]
fn shifts_mask_and_float_conversions_saturate() {
    clean("(defun main () -> i64 (+ (shl 1 64) (sar -1 65)))", 0);
    clean("(defun main () -> i64 (fptosi i64 (/ 0.0 0.0)))", 0);
    clean("(defun main () -> i64 (sext i64 (fptosi i8 300.0)))", 127);
    clean("(defun main () -> i64 (fptosi i64 (/ 1.0 0.0)))", i64::MAX);
}

#[test]
fn zext_and_sext_keep_their_meaning_through_the_pipeline() {
    clean("(defun main () -> i64 (zext i64 (trunc i8 255)))", 255);
    clean("(defun main () -> i64 (sext i64 (trunc i8 255)))", -1);
}

#[test]
fn float_bit_casts_keep_every_bit() {
    // types §2.12: the IEEE 754 pattern, a NaN's payload and a zero's
    // sign included; an f32 is held widened, and a signalling NaN of it
    // must not be quieted (eval/float_bits.rs).
    clean(
        "(defun main () -> i64 (f64->bits 1.0))",
        0x3FF0_0000_0000_0000,
    );
    clean(
        "(defun main () -> i64 (f64->bits (bits->f64 9221120237041090561)))",
        0x7FF8_0000_0000_0001,
    );
    clean(
        "(defun main () -> i64 (sext i64 (f32->bits -0.0f32)))",
        i64::from(i32::MIN),
    );
    clean(
        "(defun main () -> i64 (sext i64 (f32->bits (bits->f32 2139095041i32))))",
        0x7F80_0001,
    );
    clean(
        "(defun main () -> i64 (sext i64 (f32->bits (bits->f32 2143289345i32))))",
        0x7FC0_0001,
    );
}

#[test]
fn strings_are_bytes() {
    clean(
        "(defun main () -> i64 (str-len (str-concat \"ab\" (str-slice \"xyz\" 1 3))))",
        4,
    );
    clean(
        "(defun main () -> i64 (if (starts-with? \"hello\" \"he\") 1 0))",
        1,
    );
}

#[test]
fn option_carries_real_tags_even_around_nil() {
    clean(
        "(defun main () -> i64 (match (some nil) ((some _) 1) (nil 0)))",
        1,
    );
    clean("(defun main () -> i64 (if (= (some 1) (some 1)) 1 0))", 1);
}

#[test]
fn a_fieldless_enum_compares_by_declaration_order() {
    clean(
        "(defenum C Red Green) (defun main () -> i64 (if (< Red Green) 1 0))",
        1,
    );
}

#[test]
fn show_and_hash_are_built_in_for_scalars() {
    clean("(defun main () -> i64 (str-len (show 12345)))", 5);
    clean(
        "(defun main () -> i64 (if (= (hash \"a\") (hash \"a\")) 1 0))",
        1,
    );
}

#[test]
fn a_generic_function_dispatches_on_the_receiver_at_run_time() {
    clean(
        "(defun describe (x) (str-len (show x)))
         (defun main () -> i64 (+ (describe 7) (describe true)))",
        5,
    );
}

#[test]
fn a_def_is_evaluated_once_before_main() {
    clean(
        "(def table [1 2 3]) (defun main () -> i64 (+ (count table) (nth table 2)))",
        6,
    );
}

#[test]
fn a_vector_past_one_trie_level_reads_back_in_order() {
    clean(
        "(defun main () -> i64
           (let ((v (range 2000)))
             (+ (nth v 0) (+ (nth v 1023) (+ (nth v 1024) (nth v 1999))))))",
        1023 + 1024 + 1999,
    );
}

#[test]
fn show_of_a_str_is_itself_and_of_a_float_is_positional_with_a_fraction() {
    // types §2.12 (Decided, 2026-09-30): no quotes, no exponent, `.0`.
    let src = "(defun main () -> i64
                 (if (and (str-eq (show \"a b\") \"a b\")
                          (and (str-eq (show 100.0) \"100.0\")
                               (and (str-eq (show 1e21) \"1000000000000000000000.0\")
                                    (and (str-eq (show 0.1f32) \"0.1\")
                                         (and (str-eq (show -0.0) \"-0.0\")
                                              (str-eq (show (/ -1.0 0.0)) \"-inf\"))))))
                     1 0))";
    super::clean(src, 1);
    assert_eq!(
        super::super::arith::float_text(f64::NAN, crate::types::ty::Scalar::F64),
        "NaN"
    );
    assert_eq!(
        super::super::arith::float_text(1e-7, crate::types::ty::Scalar::F64),
        "0.0000001"
    );
}
