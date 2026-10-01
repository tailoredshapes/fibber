//! Generated programs for the expander test (spec/bootstrap.md §5), all
//! from a seed and the same on every run: mutations of the cases, token
//! soup of macro uses, programs of user macros, programs of several
//! modules, and programs a limit must stop. A few hundred in all, small
//! (under 4 KB a file), never thousands: the machine has been taken down
//! by sweeps before.

use std::path::{Path, PathBuf};

use crate::modgen::module_program;
use crate::mutate::mutate;
use crate::rng::Rng;
use crate::soup::soup;
use crate::usermacros::{limit_macro, user_macros};

/// The seeds of the standard run, each making [`PER_SEED`] programs.
pub const SEEDS: [u64; 4] = [1, 2, 0x5EED, 0xE8A0];

/// The programs made from each seed of the standard run (360 in all).
pub const PER_SEED: usize = 90;

/// The longest program the mutations start from, in bytes: the larger
/// cases are the slowest to expand and add nothing a smaller one lacks.
const MAX_SOURCE: usize = 3000;

/// What a program was made as, which decides how it is run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mutation,
    Soup,
    UserMacro,
    Module,
    /// A macro that a limit stops: the option that limits it.
    Limit(&'static str),
    /// One of the fixed inputs of `edge.rs`.
    Edge,
}

/// One generated program: its files, the first being `main.fib`.
pub struct Input {
    pub kind: Kind,
    pub name: String,
    pub files: Vec<(String, String)>,
}

/// What input `k` of a seed is: in twelve, four mutations, three soups,
/// two of user macros, two of modules and one stopped by a limit, so every
/// seed mixes all five.
const PATTERN: [&str; 12] = [
    "mutation",
    "soup",
    "usermacro",
    "mutation",
    "module",
    "soup",
    "limit",
    "mutation",
    "soup",
    "module",
    "usermacro",
    "mutation",
];

/// The standard run: [`PER_SEED`] programs from each of [`SEEDS`].
pub fn standard_run(sources: &[String]) -> Vec<Input> {
    SEEDS
        .iter()
        .flat_map(|seed| generate(*seed, PER_SEED, sources))
        .collect()
}

/// `count` programs from `seed`; program `k` depends on the seed, `k` and
/// the sources (the case texts the mutations start from) only.
pub fn generate(seed: u64, count: usize, sources: &[String]) -> Vec<Input> {
    let usable: Vec<&String> = sources.iter().filter(|s| s.len() <= MAX_SOURCE).collect();
    (0..count)
        .map(|k| {
            let mut rng = Rng::new(seed ^ (k as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
            for _ in 0..4 {
                rng.next();
            }
            let label = PATTERN[k % PATTERN.len()];
            let (kind, files) = make(label, &mut rng, &usable);
            Input {
                kind,
                name: format!("gen-s{seed}-{k:04}-{label}"),
                files,
            }
        })
        .collect()
}

fn single(text: String) -> Vec<(String, String)> {
    vec![("main.fib".to_string(), text)]
}

fn make(label: &str, rng: &mut Rng, sources: &[&String]) -> (Kind, Vec<(String, String)>) {
    match label {
        "mutation" => match mutated(rng, sources) {
            Some(text) => (Kind::Mutation, single(text)),
            None => (Kind::Soup, single(soup(rng))),
        },
        "usermacro" => (Kind::UserMacro, single(user_macros(rng))),
        "module" => (Kind::Module, module_program(rng)),
        "limit" => {
            let (text, flag) = limit_macro(rng);
            (Kind::Limit(flag), single(text))
        }
        _ => (Kind::Soup, single(soup(rng))),
    }
}

/// A mutant of one of the sources: a few tries at one that reads.
fn mutated(rng: &mut Rng, sources: &[&String]) -> Option<String> {
    if sources.is_empty() {
        return None;
    }
    (0..8).find_map(|_| mutate(sources[rng.below(sources.len())], rng))
}

/// A program written to disk: what it was made as, its main file, and
/// whether any module of it defines a macro.
#[derive(Clone, Debug)]
pub struct Written {
    pub kind: Kind,
    pub main: PathBuf,
    pub defmacro: bool,
}

/// Writes each program into a directory of its own under `dir`, so that
/// the modules of one program are not seen by another, and returns the
/// main files in order.
pub fn write_inputs(dir: &Path, inputs: &[Input]) -> Vec<Written> {
    inputs
        .iter()
        .map(|input| {
            let home = dir.join(&input.name);
            std::fs::create_dir_all(&home).expect("the scratch directory is writable");
            for (name, text) in &input.files {
                std::fs::write(home.join(name), text).expect("the scratch directory is writable");
            }
            let main = home.join("main.fib");
            Written {
                kind: input.kind,
                defmacro: crate::programs::defines_macro(&main),
                main,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    fn sources() -> Vec<String> {
        vec![
            "(defstruct P (a: i64))\n(defun f (x: i64) -> i64 (when x 1))\n".to_string(),
            "(defmacro m (x) `(do ,x))\n(defun main () -> i64 (m 1))\n".to_string(),
        ]
    }

    #[test]
    fn the_standard_run_is_three_hundred_sixty_programs_and_the_same_every_time() {
        let a = standard_run(&sources());
        let b = standard_run(&sources());
        assert_eq!(a.len(), 360);
        assert!(a
            .iter()
            .zip(&b)
            .all(|(x, y)| x.name == y.name && x.files == y.files));
        let names: std::collections::HashSet<&str> = a.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names.len(), a.len(), "names are unique");
    }

    #[test]
    fn every_kind_is_made_and_no_program_is_large() {
        let all = standard_run(&sources());
        let count = |f: &dyn Fn(Kind) -> bool| all.iter().filter(|i| f(i.kind)).count();
        assert!(count(&|k| k == Kind::Mutation) >= 100);
        assert!(count(&|k| k == Kind::Soup) >= 80);
        assert!(count(&|k| k == Kind::UserMacro) >= 60);
        assert!(count(&|k| k == Kind::Module) >= 60);
        for flag in ["--max-steps", "--max-forms", "--max-depth"] {
            assert!(count(&|k| k == Kind::Limit(flag)) >= 5, "{flag}");
        }
        for input in &all {
            for (name, text) in &input.files {
                assert!(
                    text.len() < 4000,
                    "{} {name}: {} bytes",
                    input.name,
                    text.len()
                );
            }
        }
    }

    #[test]
    fn programs_are_written_each_in_its_own_directory_with_main_first() {
        let dir = TempDir::new("gen-write");
        let inputs = generate(7, 12, &sources());
        let written = write_inputs(dir.path(), &inputs);
        assert_eq!(written.len(), 12);
        for (w, input) in written.iter().zip(&inputs) {
            assert!(w.main.ends_with("main.fib"));
            assert!(w.main.parent().expect("a directory").ends_with(&input.name));
            assert_eq!(
                std::fs::read_to_string(&w.main).expect("written"),
                input.files[0].1
            );
        }
    }

    #[test]
    fn without_sources_a_mutation_falls_back_to_a_soup() {
        let all = generate(3, 12, &[]);
        assert!(all.iter().all(|i| i.kind != Kind::Mutation));
        assert_eq!(all.len(), 12);
    }

    #[test]
    fn a_mutation_is_a_changed_source() {
        let src = sources();
        let all = generate(5, 24, &src);
        for input in all.iter().filter(|i| i.kind == Kind::Mutation) {
            assert!(!src.contains(&input.files[0].1), "{}", input.name);
        }
    }
}
