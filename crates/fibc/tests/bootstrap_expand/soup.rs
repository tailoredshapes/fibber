//! Token soup of macro uses (spec/bootstrap.md §5): programs made of the
//! prelude macros of syntax §4.4, quasiquote, literal collections and the
//! core forms, with operands drawn at random, well formed and malformed,
//! so that every rewrite and every error of the expander is reached. All
//! of it is a function of the generator's state.

use fibref::expand::PRELUDE_MACROS;

use crate::rng::Rng;

/// Declarations a program may begin with: what `derive` and the
/// reflection calls read.
const TYPES: [&str; 8] = [
    "(defstruct P (a: i64 b: str))",
    "(defstruct (Pair a b) (fst: a snd: b))",
    "(defstruct Q :private (z: i64))",
    "(defstruct W (x y))",
    "(defenum E (A x: i64) (B) (C i64 str))",
    "(defenum (Res a b) (Ok v: a) (Err e: b))",
    "(defenum Color Red Green)",
    "(defenum K :private (K1 n: i64) (K2))",
];

/// Operands that are atoms or small forms.
const ATOMS: [&str; 24] = [
    "x", "n", "xs", "f", "1", "0", "-2i8", "2.5", "\"s\"", "\\a", "true", "false", "nil", ":k",
    "[]", "[1 2]", "{}", "{1 2}", "()", "(f x)", "(& x)", "'x", "(g 1 2)", "else",
];

/// Patterns for `let`, `match`, `if-let` and `when-let`.
const PATTERNS: [&str; 8] = [
    "x", "_", "[a b]", "(some x)", "nil", "(P a b)", "{1 2}", "(A y)",
];

/// The state of one program being made.
pub struct Soup<'a> {
    pub rng: &'a mut Rng,
}

impl Soup<'_> {
    fn atom(&mut self) -> String {
        self.rng.pick(&ATOMS).to_string()
    }

    /// `n` expressions, space-separated.
    fn exprs(&mut self, n: usize, depth: usize) -> String {
        let all: Vec<String> = (0..n).map(|_| self.expr(depth)).collect();
        all.join(" ")
    }

    /// Up to `most` expressions, space-separated.
    fn some_exprs(&mut self, most: usize, depth: usize) -> String {
        let n = self.rng.between(0, most);
        self.exprs(n, depth)
    }

    /// An expression: an atom, a core form or a macro use.
    pub fn expr(&mut self, depth: usize) -> String {
        if depth == 0 || self.rng.one_in(5) {
            return self.atom();
        }
        let d = depth - 1;
        match self.rng.below(12) {
            0..=6 => self.macro_use(d),
            7 => format!("(if {})", self.exprs(3, d)),
            8 => self.binding_form(d),
            9 => self.quasi(d),
            10 => format!("(do {})", self.some_exprs(3, d)),
            _ => format!("[{}]", self.some_exprs(3, d)),
        }
    }

    fn binding_form(&mut self, d: usize) -> String {
        let pat = self.rng.pick(&PATTERNS);
        match self.rng.below(4) {
            0 => format!("(let (({pat} {})) {})", self.expr(d), self.expr(d)),
            1 => format!(
                "(match {} ({pat} {}) (_ {}))",
                self.expr(d),
                self.expr(d),
                self.expr(d)
            ),
            2 => format!(
                "(fn ({}) {})",
                self.rng.pick(&["i", "x", "x: i64", "nil"]),
                self.expr(d)
            ),
            _ => format!("(loop ((i 0)) {})", self.expr(d)),
        }
    }

    /// A quasiquote with unquotes and splices at random levels.
    fn quasi(&mut self, d: usize) -> String {
        let parts = [
            format!(",{}", self.expr(d)),
            format!(",@{}", self.expr(d)),
            self.atom(),
            format!("(a ,{} ,@{})", self.expr(d), self.expr(d)),
            "`(b ,,x)".to_string(),
            "[,@xs]".to_string(),
            "{,x ,@ys}".to_string(),
        ];
        let n = self.rng.between(1, 3);
        let picked: Vec<String> = (0..n)
            .map(|_| parts[self.rng.below(parts.len())].clone())
            .collect();
        format!("`({})", picked.join(" "))
    }

    /// A use of a macro of the prelude (from `PRELUDE_MACROS`, so a macro
    /// added later is reached with operands at random).
    pub fn macro_use(&mut self, d: usize) -> String {
        let name = self.rng.pick(&PRELUDE_MACROS);
        let n = self.rng.between(0, 3);
        match name {
            "when" | "unless" | "while" => format!("({name} {})", self.exprs(n, d)),
            "and" | "or" | "list" | "dbg" | "assert" | "range" => {
                format!("({name} {})", self.exprs(n, d))
            }
            "cond" => self.cond(d),
            "if-let" | "when-let" => self.let_macro(name, d),
            "plet" => self.plet(d),
            "dotimes" => self.dotimes(d),
            "for-each" => self.for_each(d),
            "->" | "->>" | "doto" => self.threaded(name, d),
            "derive" => self.derive(),
            _ => format!("({name} {})", self.exprs(n, d)),
        }
    }

    fn cond(&mut self, d: usize) -> String {
        let n = self.rng.between(0, 3);
        let mut clauses: Vec<String> = (0..n)
            .map(|_| match self.rng.below(8) {
                0 => self.expr(d),
                1 => format!("({})", self.expr(d)),
                _ => format!("({} {})", self.expr(d), self.expr(d)),
            })
            .collect();
        if self.rng.one_in(2) {
            let tail = if self.rng.one_in(2) { "else" } else { ":else" };
            clauses.push(format!("({tail} {})", self.expr(d)));
        }
        if self.rng.one_in(6) {
            clauses.reverse();
        }
        format!("(cond {})", clauses.join(" "))
    }

    fn let_macro(&mut self, name: &str, d: usize) -> String {
        let pat = self.rng.pick(&PATTERNS);
        let binding = match self.rng.below(6) {
            0 => format!("({pat})"),
            1 => pat.to_string(),
            _ => format!("({pat} {})", self.expr(d)),
        };
        let n = self.rng.between(0, 3);
        format!("({name} {binding} {})", self.exprs(n, d))
    }

    fn plet(&mut self, d: usize) -> String {
        let n = self.rng.between(0, 3);
        let pairs: Vec<String> = (0..n)
            .map(|_| match self.rng.below(5) {
                0 => "(a)".to_string(),
                1 => format!("(b: i64 {})", self.expr(d)),
                2 => format!("(1 {})", self.expr(d)),
                _ => format!("({} {})", self.rng.pick(&["a", "b", "c"]), self.expr(d)),
            })
            .collect();
        format!("(plet ({}) {})", pairs.join(" "), self.expr(d))
    }

    fn dotimes(&mut self, d: usize) -> String {
        let head = match self.rng.below(6) {
            0 => "i".to_string(),
            1 => format!("(1 {})", self.expr(d)),
            _ => format!("(i {})", self.expr(d)),
        };
        let n = self.rng.between(0, 2);
        format!("(dotimes {head} {})", self.exprs(n, d))
    }

    fn for_each(&mut self, d: usize) -> String {
        let range = match self.rng.below(4) {
            0 => format!("(range {})", self.expr(d)),
            1 => self.expr(d),
            _ => format!("(range {} {})", self.expr(d), self.expr(d)),
        };
        let f = match self.rng.below(5) {
            0 => self.expr(d),
            1 => format!("(fn (i: i64) {})", self.expr(d)),
            2 => format!("(fn () {})", self.expr(d)),
            _ => format!("(fn (i) {})", self.expr(d)),
        };
        format!("(for-each {range} {f})")
    }

    fn threaded(&mut self, name: &str, d: usize) -> String {
        let n = self.rng.between(0, 3);
        let steps: Vec<String> = (0..n)
            .map(|_| match self.rng.below(6) {
                0 => "f".to_string(),
                1 => "()".to_string(),
                2 => self.rng.pick(&["1", "\"s\"", "nil", "[1]"]).to_string(),
                _ => format!(
                    "({} {})",
                    self.rng.pick(&["g", "h", "+", "conj"]),
                    self.expr(d)
                ),
            })
            .collect();
        format!("({name} {} {})", self.expr(d), steps.join(" "))
    }

    fn derive(&mut self) -> String {
        let proto = self
            .rng
            .pick(&["Eq", "Ord", "Hash", "Show", "Debug", "eq", "(Eq)"]);
        let target = self.rng.pick(&[
            "P", "Pair", "Q", "W", "E", "Res", "Color", "K", "Option", "List", "Nope", "x",
        ]);
        match self.rng.below(12) {
            0 => format!("(derive {proto})"),
            1 => format!("(derive {proto} {target} {target})"),
            _ => format!("(derive {proto} {target})"),
        }
    }

    /// One top-level item: a definition around an expression, a bare
    /// macro use or `derive`, or a `do` of both.
    fn item(&mut self, depth: usize) -> String {
        match self.rng.below(7) {
            0 => format!("(defun main () -> i64 {})", self.expr(depth)),
            1 => format!(
                "(defun f (x: i64 xs: (Vec i64)) -> i64 {})",
                self.expr(depth)
            ),
            2 => format!("(def v: i64 {})", self.expr(depth)),
            3 => self.derive(),
            4 => format!(
                "(do {} (defun g () -> i64 {}))",
                self.derive(),
                self.expr(depth)
            ),
            5 => self.macro_use(depth),
            _ => format!("(impl Eq P (= (self y) {}))", self.expr(depth)),
        }
    }
}

/// A program of soup: some declarations, then one to four items.
pub fn soup(rng: &mut Rng) -> String {
    let mut parts: Vec<String> = Vec::new();
    for _ in 0..rng.between(0, 3) {
        parts.push(rng.pick(&TYPES).to_string());
    }
    let mut s = Soup { rng };
    let items = s.rng.between(1, 4);
    let depth = s.rng.between(1, 3);
    for _ in 0..items {
        parts.push(s.item(depth));
    }
    parts.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibref::syntax::read_all;

    #[test]
    fn the_same_state_makes_the_same_program() {
        assert_eq!(soup(&mut Rng::new(9)), soup(&mut Rng::new(9)));
        assert_ne!(soup(&mut Rng::new(9)), soup(&mut Rng::new(10)));
    }

    #[test]
    fn the_programs_read_and_every_prelude_macro_turns_up() {
        let mut seen = std::collections::HashSet::new();
        let mut read = 0;
        for seed in 0..400 {
            let text = soup(&mut Rng::new(seed));
            if read_all(&text, "s").is_ok() {
                read += 1;
            }
            for name in PRELUDE_MACROS {
                if text.contains(&format!("({name} ")) {
                    seen.insert(name);
                }
            }
        }
        assert!(read >= 360, "only {read} of 400 read");
        assert_eq!(seen.len(), PRELUDE_MACROS.len(), "{seen:?}");
    }

    #[test]
    fn a_program_is_small() {
        for seed in 0..200 {
            let text = soup(&mut Rng::new(seed));
            assert!(text.len() < 3500, "{} bytes: {text}", text.len());
        }
    }
}
