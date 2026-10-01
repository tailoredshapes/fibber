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
            "defn" | "defn-" => self.defn(name, d),
            "update" => self.update(d),
            "reduce" => self.reduce(d),
            "+" | "-" | "*" | "<" | ">" | "<=" | ">=" | "=" | "max" | "min" | "bit-and"
            | "bit-or" | "bit-xor" | "conj" | "assoc" | "dissoc" | "merge" | "swap!" => {
                self.variadic(name, d)
            }
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

    /// `defn` and `defn-` (stdlib R6b): mostly well formed, with a
    /// docstring, annotations, and now and then a rest marker, a list of
    /// clauses, a parameter list that is not a vector, or no parameters.
    fn defn(&mut self, name: &str, d: usize) -> String {
        let doc = if self.rng.one_in(3) { "\"doc\" " } else { "" };
        let params = self.rng.pick(&[
            "[]",
            "[x]",
            "[x: i64 y: i64]",
            "[x: i64]",
            "[x & r]",
            "[& r]",
            "(x)",
            "x",
        ]);
        let note = self
            .rng
            .pick(&["", "", "-> i64 ", ":where ((Eq a)) ", ":private "]);
        let n = self.rng.between(0, 2);
        let body = self.exprs(n, d);
        match self.rng.below(12) {
            0 => format!("({name} f ([x] {}) ([x y] {}))", self.expr(d), self.expr(d)),
            1 => format!("({name} f{})", if doc.is_empty() { "" } else { " \"doc\"" }),
            2 => format!("({name} 1 [x] {})", self.expr(d)),
            3 => format!("({name})"),
            _ => format!("({name} f {doc}{params} {note}{body})"),
        }
    }

    /// The operators, the collection functions and `swap!` (stdlib R6a): a
    /// call of none to five operands, among them now and then a literal
    /// `nil` (`merge` skips it), a field path and a call (a comparison
    /// binds the calls first), so the folds, the binary calls they decline
    /// and the errors of `-`, `assoc` and `merge` are all reached.
    fn variadic(&mut self, name: &str, d: usize) -> String {
        let n = self.rng.between(0, 5);
        let operands: Vec<String> = (0..n)
            .map(|_| match self.rng.below(7) {
                0 => "nil".to_string(),
                1 => "(. p f)".to_string(),
                _ => self.expr(d),
            })
            .collect();
        format!("({name} {})", operands.join(" "))
    }

    /// `update`: with extra arguments, a literal `fnil`, and the calls the
    /// macro declines.
    fn update(&mut self, d: usize) -> String {
        let f = match self.rng.below(6) {
            0 => format!("(fnil {} {})", self.expr(d), self.expr(d)),
            1 => format!("(fnil {})", self.expr(d)),
            2 => format!("(fnil {} {} {})", self.expr(d), self.expr(d), self.expr(d)),
            3 => self.rng.pick(&["+", "inc", "f"]).to_string(),
            _ => self.expr(d),
        };
        let extra = self.some_exprs(2, d);
        match self.rng.below(8) {
            0 => format!("(update {})", self.some_exprs(2, d)),
            _ => format!("(update {} {} {f} {extra})", self.expr(d), self.expr(d)),
        }
    }

    /// A form whose tails are `reduced`, plain values and `recur`, through
    /// the forms a tail passes (the rewrite of `reduce`, stdlib R6b).
    fn tail(&mut self, d: usize) -> String {
        if d == 0 {
            return match self.rng.below(5) {
                0 => "(reduced a)".to_string(),
                1 => "(recur 1)".to_string(),
                2 => format!("(reduced {})", self.atom()),
                3 => self.rng.pick(&["(reduced)", "(reduced a b)"]).to_string(),
                _ => self.atom(),
            };
        }
        let e = self.expr(d - 1);
        let (t, u) = (self.tail(d - 1), self.tail(d - 1));
        match self.rng.below(11) {
            0 => format!("(if {e} {t} {u})"),
            1 => format!("(let ((y {e})) {t})"),
            2 => format!("(do {e} {t})"),
            3 => format!("(match {e} (1 {t}) (_ {u}))"),
            4 => format!("(cond ({e} {t}) (else {u}))"),
            5 => format!("(when {e} {t})"),
            6 => format!("(unless {e} {t})"),
            7 => format!("(if-let (p {e}) {t} {u})"),
            8 => format!("(when-let (p {e}) {t})"),
            9 => format!("(loop ((i 0)) {t})"),
            _ => t,
        }
    }

    /// `reduce`: two arguments, a literal head, a literal `fn` with a
    /// `reduced` in its tails, or a call the macro declines.
    fn reduce(&mut self, d: usize) -> String {
        let c = self.expr(d);
        match self.rng.below(10) {
            0 => format!("(reduce {})", self.some_exprs(1, d)),
            1 => format!(
                "(reduce {} {} {} {c})",
                self.expr(d),
                self.expr(d),
                self.expr(d)
            ),
            2 => format!(
                "(reduce {} {c})",
                self.rng
                    .pick(&["+", "*", "str", "conj", "merge", "concat", "-", "f"])
            ),
            3 => format!("(reduce {} {c})", self.expr(d)),
            4 => format!("(reduce {} 0 {c})", self.expr(d)),
            _ => {
                let note = self.rng.pick(&["", "", "", "-> i64 ", ":where ((Eq a)) "]);
                let params = self
                    .rng
                    .pick(&["(a x)", "(a x)", "(a)", "(a x y)", "go (a x)"]);
                let depth = self.rng.between(0, 3);
                let body = self.tail(depth);
                format!("(reduce (fn {params} {note}{body}) {} {c})", self.expr(d))
            }
        }
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
