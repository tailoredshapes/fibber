//! What the fuzzer knows of lIR's grammar (spec/lir.md §2 to §7): the
//! words it swaps in, and random instructions built from them with
//! operands drawn from the case under mutation.

use lir::diag::Pos;
use lir::sexp::Sexp;

use super::mutate::{random_type, Case};
use super::rng::Rng;

pub const TYPES: [&str; 9] = [
    "i1", "i8", "i16", "i32", "i64", "float", "double", "ptr", "void",
];

/// Every word the parser knows, instruction or modifier.
pub const KEYWORDS: [&str; 87] = [
    "add",
    "sub",
    "mul",
    "sdiv",
    "udiv",
    "srem",
    "urem",
    "fadd",
    "fsub",
    "fmul",
    "fdiv",
    "frem",
    "and",
    "or",
    "xor",
    "shl",
    "lshr",
    "ashr",
    "fneg",
    "ctpop",
    "sadd-overflow",
    "ssub-overflow",
    "smul-overflow",
    "icmp",
    "fcmp",
    "select",
    "trunc",
    "zext",
    "sext",
    "fptrunc",
    "fpext",
    "fptoui",
    "fptosi",
    "uitofp",
    "sitofp",
    "ptrtoint",
    "inttoptr",
    "bitcast",
    "fptosi-sat",
    "fptoui-sat",
    "extractelement",
    "insertelement",
    "shufflevector",
    "extractvalue",
    "insertvalue",
    "alloca",
    "load",
    "store",
    "getelementptr",
    "inbounds",
    "volatile",
    "align",
    "atomic-load",
    "atomic-store",
    "atomicrmw",
    "cmpxchg",
    "fence",
    "xchg",
    "nand",
    "max",
    "min",
    "umax",
    "umin",
    "fmax",
    "fmin",
    "weak",
    "singlethread",
    "ret",
    "br",
    "switch",
    "unreachable",
    "trap",
    "phi",
    "call",
    "tailcall",
    "indirect-call",
    "indirect-tailcall",
    "let",
    "block",
    "define",
    "declare",
    "declare-global",
    "defstruct",
    "global",
    "constant",
    "string",
    "zeroinitializer",
];

const ORDERINGS: [&str; 6] = [
    "monotonic",
    "acquire",
    "release",
    "acq_rel",
    "seq_cst",
    "unordered",
];

const IPREDS: [&str; 10] = [
    "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
];

const FPREDS: [&str; 14] = [
    "oeq", "one", "olt", "ole", "ogt", "oge", "ord", "uno", "ueq", "une", "ult", "ule", "ugt",
    "uge",
];

pub const INTERESTING_NUMBERS: [&str; 22] = [
    "0",
    "1",
    "-1",
    "2",
    "7",
    "63",
    "64",
    "127",
    "128",
    "255",
    "256",
    "2147483647",
    "2147483648",
    "-2147483648",
    "4294967295",
    "4294967296",
    "9223372036854775807",
    "-9223372036854775808",
    "9223372036854775808",
    "99999999999999999999",
    "1.5",
    "nan",
];

fn pos() -> Pos {
    Pos::default()
}

fn atom(text: &str) -> Sexp {
    Sexp::Atom(text.to_string(), pos())
}

fn list(items: Vec<Sexp>) -> Sexp {
    Sexp::List(items, pos())
}

/// A name, label or literal of the case: what an operand slot gets.
fn leaf(rng: &mut Rng, case: &Case) -> Sexp {
    match rng.below(4) {
        0 => list(vec![
            atom(TYPES[rng.below(TYPES.len())]),
            atom(INTERESTING_NUMBERS[rng.below(INTERESTING_NUMBERS.len())]),
        ]),
        1 => atom(["null", "@printf", "@main", "entry", "x"][rng.below(5)]),
        _ => rng.pick(&case.atoms).map_or(atom("x"), |a| atom(a)),
    }
}

fn label(rng: &mut Rng, case: &Case) -> Sexp {
    rng.pick(&case.atoms).map_or(atom("entry"), |a| atom(a))
}

/// One instruction of the grammar with random operands.
pub fn random_instruction(rng: &mut Rng, case: &Case) -> Sexp {
    let f = |rng: &mut Rng| leaf(rng, case);
    let ty = random_type(rng, case);
    match rng.below(24) {
        0..=3 => list(vec![atom(KEYWORDS[rng.below(18)]), f(rng), f(rng)]),
        4 => list(vec![
            atom("icmp"),
            atom(IPREDS[rng.below(IPREDS.len())]),
            f(rng),
            f(rng),
        ]),
        5 => list(vec![
            atom("fcmp"),
            atom(FPREDS[rng.below(FPREDS.len())]),
            f(rng),
            f(rng),
        ]),
        6 => list(vec![atom(KEYWORDS[26 + rng.below(14)]), ty, f(rng)]),
        7 => list(vec![atom("load"), ty, f(rng)]),
        8 => list(vec![atom("store"), f(rng), f(rng)]),
        9 => list(vec![atom("alloca"), ty]),
        10 => list(vec![
            atom("getelementptr"),
            ty,
            f(rng),
            list(vec![atom("i32"), atom("0")]),
            list(vec![
                atom("i32"),
                atom(INTERESTING_NUMBERS[rng.below(INTERESTING_NUMBERS.len())]),
            ]),
        ]),
        11 => list(vec![
            atom("extractvalue"),
            f(rng),
            atom(INTERESTING_NUMBERS[rng.below(INTERESTING_NUMBERS.len())]),
        ]),
        12 => list(vec![atom("select"), f(rng), f(rng), f(rng)]),
        13 => list(vec![atom("ret"), f(rng)]),
        14 => list(vec![atom("br"), label(rng, case)]),
        15 => list(vec![atom("br"), f(rng), label(rng, case), label(rng, case)]),
        16 => list(vec![atom("call"), f(rng), f(rng)]),
        17 => list(vec![atom("phi"), ty, list(vec![label(rng, case), f(rng)])]),
        18 => list(vec![atom(["unreachable", "trap"][rng.below(2)])]),
        19 => list(vec![
            atom("atomicrmw"),
            atom(["add", "xchg", "max", "nand"][rng.below(4)]),
            atom(ORDERINGS[rng.below(ORDERINGS.len())]),
            f(rng),
            f(rng),
        ]),
        20 => list(vec![
            atom("cmpxchg"),
            atom(ORDERINGS[rng.below(ORDERINGS.len())]),
            f(rng),
            f(rng),
            f(rng),
        ]),
        21 => list(vec![
            atom("switch"),
            f(rng),
            label(rng, case),
            list(vec![list(vec![f(rng), label(rng, case)])]),
        ]),
        22 => list(vec![
            atom("let"),
            list(vec![list(vec![atom("x"), f(rng)])]),
            f(rng),
        ]),
        _ => list(vec![
            atom("indirect-call"),
            f(rng),
            list(vec![atom("fn"), ty]),
            f(rng),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::tree::print;

    #[test]
    fn keyword_slices_line_up_with_their_uses() {
        assert_eq!(KEYWORDS[17], "ashr");
        assert_eq!(KEYWORDS[26], "trunc");
        assert_eq!(KEYWORDS[39], "fptoui-sat");
    }

    #[test]
    fn random_instructions_read_back() {
        let case = Case::new(
            "t".into(),
            lir::sexp::read("(define (main i32) () (block entry (ret (i32 0))))").unwrap(),
        );
        let mut rng = Rng::new(9);
        for _ in 0..200 {
            let src = print(&[random_instruction(&mut rng, &case)]);
            assert!(lir::sexp::read(&src).is_ok(), "{src}");
        }
    }
}
