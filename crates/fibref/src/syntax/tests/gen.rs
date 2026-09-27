//! A small deterministic generator of forms for the round-trip property
//! test: a xorshift PRNG driven by a seed, no external crates. It
//! generates only forms the reader can produce (valid symbols and
//! keywords, in-range integers, finite floats), and leans on the edges:
//! every escape, control and invisible characters, width extremes,
//! negative zero, symbols like `x:`, `&`, `/`.

use std::sync::Arc;

use crate::syntax::{FltWidth, Form, FormKind, IntWidth, Pos};

pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub(super) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub(super) fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("below n")
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

const TRICKY_SYMS: &[&str] = &[
    "x",
    "x:",
    "->",
    ".",
    "...",
    "_",
    "&",
    "/",
    "seq/first",
    "quote",
    "unquote",
    "deref",
    "nil?",
    "a&b",
    "-",
    "+",
    "λ",
    "a#b",
    "--1",
    ".5",
    "+5",
    "set!",
];
const START: &[char] = &[
    'a', 'b', 'x', 'λ', 'é', '*', '+', '!', '?', '<', '=', '.', '_', '-',
];
const REST: &[char] = &[
    'a', 'z', 'λ', '😀', '*', '-', '.', '_', '0', '7', ':', '#', '&', '!', '>',
];
const STR_CHARS: &[char] = &[
    'a', '"', '\\', '\n', '\t', '\r', '\0', '\u{7}', '\u{A0}', '\u{FEFF}', 'é', '😀', ' ', ';',
    '(', ')', '\u{7F}', '\u{85}', '{', '\u{2028}', '\u{301}', ',', '@',
];

fn pos() -> Pos {
    Pos {
        file: Arc::from("gen"),
        line: 1,
        col: 1,
        start: 0,
        end: 0,
    }
}

fn name(rng: &mut Rng, first: &[char]) -> String {
    let mut s = String::new();
    s.push(rng.pick(first));
    for _ in 0..rng.below(5) {
        s.push(rng.pick(REST));
    }
    let digit_after_minus = s.starts_with('-') && s[1..].starts_with(|c: char| c.is_ascii_digit());
    if digit_after_minus {
        s.insert(0, 'a');
    }
    s
}

fn symbol(rng: &mut Rng) -> String {
    match rng.below(4) {
        0 => rng.pick(TRICKY_SYMS).to_string(),
        1 => format!("{}/{}", name(rng, START), name(rng, REST)),
        _ => name(rng, START),
    }
}

fn keyword(rng: &mut Rng) -> String {
    let first: Vec<char> = REST.iter().copied().filter(|c| *c != ':').collect();
    match rng.below(3) {
        0 => format!("{}/{}", name(rng, &first), name(rng, REST)),
        _ => name(rng, &first),
    }
}

fn int(rng: &mut Rng) -> FormKind {
    let width = rng.pick(&[IntWidth::I8, IntWidth::I16, IntWidth::I32, IntWidth::I64]);
    let (lo, hi) = width.range();
    let span = u128::try_from(hi - lo + 1).expect("positive");
    let value = match rng.below(5) {
        0 => lo,
        1 => hi,
        2 => 0,
        _ => lo + i128::try_from(u128::from(rng.next()) % span).expect("fits"),
    };
    let v = i64::try_from(value).expect("in range");
    FormKind::Int { v, width }
}

fn float(rng: &mut Rng) -> FormKind {
    let special = [
        0.0,
        -0.0,
        1e300,
        5e-324,
        f64::MAX,
        f64::MIN_POSITIVE,
        0.1,
        -1.5,
    ];
    if rng.below(2) == 0 {
        let bits = u32::try_from(rng.next() >> 32).expect("32 bits");
        let v = f32::from_bits(bits);
        let v = if v.is_finite() { v } else { 1.5 };
        return FormKind::Flt {
            v: f64::from(v),
            width: FltWidth::F32,
        };
    }
    let v = match rng.below(3) {
        0 => rng.pick(&special),
        _ => f64::from_bits(rng.next()),
    };
    let v = if v.is_finite() { v } else { -2.25 };
    FormKind::Flt {
        v,
        width: FltWidth::F64,
    }
}

fn any_char(rng: &mut Rng) -> char {
    if rng.below(2) == 0 {
        return rng.pick(STR_CHARS);
    }
    let raw = u32::try_from(rng.next() % 0x11_0000).expect("fits");
    char::from_u32(raw).unwrap_or('?')
}

fn atom(rng: &mut Rng) -> FormKind {
    match rng.below(9) {
        0 => FormKind::Sym(symbol(rng)),
        1 => FormKind::Kw(keyword(rng)),
        2 => int(rng),
        3 => float(rng),
        4 => FormKind::Str((0..rng.below(8)).map(|_| any_char(rng)).collect()),
        5 => FormKind::Chr(any_char(rng)),
        6 => FormKind::Bool(rng.below(2) == 0),
        7 => FormKind::Nil,
        _ => FormKind::Sym(rng.pick(TRICKY_SYMS).to_string()),
    }
}

/// A random form at most `depth` levels deep.
pub(super) fn form(rng: &mut Rng, depth: usize) -> Form {
    if depth == 0 || rng.below(3) == 0 {
        return Form::new(atom(rng), pos());
    }
    let shape = rng.below(3);
    let len = rng.below(6);
    let len = if shape == 2 { len - len % 2 } else { len };
    let items: Vec<Form> = (0..len).map(|_| form(rng, depth - 1)).collect();
    let kind = match shape {
        0 => FormKind::List(items),
        1 => FormKind::Vec(items),
        _ => FormKind::Map(items),
    };
    Form::new(kind, pos())
}
