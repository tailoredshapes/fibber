//! Programs that define and call user macros (spec/bootstrap.md §5): the
//! shapes the cases use (quasiquote with splices, `gensym`, reflection,
//! macros that build definitions, macros that call macros, a user macro
//! that shadows a prelude one), with calls whose operands are soup and
//! whose arity is sometimes wrong, and the ones that only a limit stops
//! (a macro that calls itself, one that grows, one that nests).

use crate::rng::Rng;
use crate::soup::Soup;

/// A macro definition, the macros it calls, and its calls: a call
/// template with `{}` for an operand, and whether it stands at top level.
struct Def {
    source: &'static str,
    needs: &'static [&'static str],
    call: &'static str,
    top_level: bool,
}

const DEFS: [Def; 16] = [
    Def {
        source: "(defmacro twice (x) `(do ,x ,x))",
        needs: &[],
        call: "(twice {})",
        top_level: false,
    },
    Def {
        source: "(defmacro sum (... xs) `(+ ,@xs))",
        needs: &[],
        call: "(sum {} {} 3)",
        top_level: false,
    },
    Def {
        source: "(defmacro keep (x) (let ((g (gensym \"k\"))) `(let ((,g ,x)) ,g)))",
        needs: &[],
        call: "(keep {})",
        top_level: false,
    },
    Def {
        source: "(defmacro nfields (name) (Int (count (struct-fields name)) :i64))",
        needs: &[],
        call: "(nfields P)",
        top_level: false,
    },
    Def {
        source: "(defmacro isenum (n) (if (enum? n) '1 '0))",
        needs: &[],
        call: "(isenum E)",
        top_level: false,
    },
    Def {
        source: "(defmacro defrecord (name fields) `(do (defstruct ,name ,fields) (derive Eq ,name)))",
        needs: &[],
        call: "(defrecord Rec (x: i64 y: str))",
        top_level: true,
    },
    Def {
        source: "(defmacro mkdef (name v) `(defun ,name () -> i64 ,v))",
        needs: &[],
        call: "(mkdef made {})",
        top_level: true,
    },
    Def {
        source: "(defmacro nest (x) `(twice (twice ,x)))",
        needs: &["twice"],
        call: "(nest {})",
        top_level: false,
    },
    Def {
        source: "(defmacro swap-if (form) (match form ((List [(Sym \"if\") c t e]) (List [(Sym \"if\") c e t])) (_ form)))",
        needs: &[],
        call: "(swap-if (if {} {} {}))",
        top_level: false,
    },
    Def {
        source: "(defmacro or-zero (e) `(match ,e ((some x) x) (nil 0)))",
        needs: &[],
        call: "(or-zero {})",
        top_level: false,
    },
    Def {
        source: "(defmacro variants (n) (Int (count (enum-variants n)) :i64))",
        needs: &[],
        call: "(variants E)",
        top_level: false,
    },
    Def {
        source: "(defmacro mklist (... xs) (List xs))",
        needs: &[],
        call: "(mklist {} {})",
        top_level: false,
    },
    Def {
        source: "(defmacro bad () (Int 300 :i8))",
        needs: &[],
        call: "(bad)",
        top_level: false,
    },
    Def {
        source: "(defmacro when (c ... b) `(if ,c (do ,@b) 0))",
        needs: &[],
        call: "(when {} {})",
        top_level: false,
    },
    Def {
        source: "(defmacro if (a) a)",
        needs: &[],
        call: "(if {})",
        top_level: false,
    },
    Def {
        source: "(defmacro fl () (Flt 2.5 :f32))",
        needs: &[],
        call: "(fl)",
        top_level: false,
    },
];

/// Macros that only a small limit stops, and what limits them.
const LIMIT_DEFS: [(&str, &str, &str); 3] = [
    (
        "(defmacro loopy (x) `(loopy ,x))",
        "(loopy 1)",
        "--max-steps",
    ),
    (
        "(defmacro grow (... xs) `(grow 1 ,@xs))",
        "(grow 1)",
        "--max-forms",
    ),
    (
        "(defmacro deep (x) `(do (deep ,x)))",
        "(deep 1)",
        "--max-depth",
    ),
];

/// `source` with each `{}` filled by an operand from `soup`.
fn fill(source: &str, soup: &mut Soup) -> String {
    let mut out = String::new();
    let mut rest = source;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        out.push_str(&soup.expr(1));
        rest = &rest[i + 2..];
    }
    out + rest
}

/// A call with its operands dropped or one more added once in six each:
/// the arity error.
fn skew(call: String, rng: &mut Rng) -> String {
    match rng.below(6) {
        0 => match call.split_once(' ') {
            Some((head, _)) => format!("{head})"),
            None => call,
        },
        1 if call.ends_with(')') => format!("{} 7)", &call[..call.len() - 1]),
        _ => call,
    }
}

/// A program of user macros: some declarations, one to three macros
/// (and those they call), and a call of each.
pub fn user_macros(rng: &mut Rng) -> String {
    let mut chosen: Vec<usize> = Vec::new();
    for _ in 0..rng.between(1, 3) {
        let i = rng.below(DEFS.len());
        for dep in DEFS[i].needs {
            let at = DEFS
                .iter()
                .position(|d| d.source.contains(&format!("defmacro {dep} ")));
            chosen.extend(at.filter(|a| !chosen.contains(a)));
        }
        if !chosen.contains(&i) {
            chosen.push(i);
        }
    }
    let mut parts: Vec<String> = vec![
        "(defstruct P (a: i64 b: str))".into(),
        "(defenum E (A x: i64) (B))".into(),
    ];
    parts.extend(chosen.iter().map(|i| DEFS[*i].source.to_string()));
    let mut soup = Soup { rng };
    let mut calls = Vec::new();
    for i in &chosen {
        let call = fill(DEFS[*i].call, &mut soup);
        let call = skew(call, soup.rng);
        if DEFS[*i].top_level {
            parts.push(call);
        } else {
            calls.push(call);
        }
    }
    parts.push(format!("(defun main () -> i64 (do {} 0))", calls.join(" ")));
    parts.join("\n") + "\n"
}

/// A program with one macro that a limit must stop, and the option that
/// limits it.
pub fn limit_macro(rng: &mut Rng) -> (String, &'static str) {
    let (source, call, flag) = LIMIT_DEFS[rng.below(LIMIT_DEFS.len())];
    (
        format!("{source}\n(defun main () -> i64 (do {call} 0))\n"),
        flag,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibref::syntax::read_all;

    #[test]
    fn the_programs_read_are_deterministic_and_define_a_macro() {
        for seed in 0..300 {
            let text = user_macros(&mut Rng::new(seed));
            assert!(read_all(&text, "u").is_ok(), "{text}");
            assert!(text.contains("(defmacro "), "{text}");
            assert_eq!(text, user_macros(&mut Rng::new(seed)));
        }
    }

    #[test]
    fn a_macro_that_another_calls_comes_with_it() {
        let with_nest = (0..300)
            .map(|s| user_macros(&mut Rng::new(s)))
            .filter(|t| t.contains("(defmacro nest "))
            .collect::<Vec<_>>();
        assert!(!with_nest.is_empty());
        assert!(with_nest.iter().all(|t| t.contains("(defmacro twice ")));
    }

    #[test]
    fn every_definition_is_a_macro_and_every_limit_macro_names_its_flag() {
        for d in &DEFS {
            assert!(d.source.starts_with("(defmacro "), "{}", d.source);
            assert!(read_all(d.source, "d").is_ok(), "{}", d.source);
        }
        for s in 0..20 {
            let (text, flag) = limit_macro(&mut Rng::new(s));
            assert!(read_all(&text, "l").is_ok(), "{text}");
            assert!(flag.starts_with("--max-"));
        }
    }

    #[test]
    fn calls_fill_their_holes_and_skew_changes_the_arity_sometimes() {
        let mut rng = Rng::new(3);
        let mut soup = Soup { rng: &mut rng };
        let filled = fill("(f {} {})", &mut soup);
        assert!(
            !filled.contains("{}") && filled.starts_with("(f "),
            "{filled}"
        );
        let changed = (0..60)
            .filter(|s| skew("(f 1 2)".to_string(), &mut Rng::new(*s)) != "(f 1 2)")
            .count();
        assert!(changed > 0 && changed < 60, "{changed}");
    }
}
