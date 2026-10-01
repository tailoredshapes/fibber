//! Generated inputs for the differential test (spec/bootstrap.md §3 item
//! 3), all from a seed and the same on every run: mutations of the corpus,
//! token soup, clean inputs ending in a snippet that provokes one named
//! read error, and deep nesting. Each input is under 4 KB and valid UTF-8
//! (it is a `String`).

use std::path::{Path, PathBuf};

use super::nest::deep;
use super::rng::Rng;
use super::soup::{soup, targeted};

/// No input is longer than this many bytes.
pub const MAX_BYTES: usize = 4000;

/// The most inputs a manual run may ask for.
pub const MAX_COUNT: usize = 2000;

/// The seeds of the standard run, each making [`PER_SEED`] inputs.
pub const SEEDS: [u64; 6] = [1, 2, 3, 0x5EED, 0xF1BB, 0xC0_FFEE];

/// The inputs made from each seed of the standard run (300 in all).
pub const PER_SEED: usize = 50;

/// One generated input: a name that carries its seed and index, and its text.
pub struct Input {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Mutation,
    Soup,
    /// A clean input and a snippet that provokes one read error.
    Targeted,
    Deep,
}

/// What input `k` is: in twelve, four mutations, three soups, three
/// targeted and two deep ones, so every seed mixes all four.
const PATTERN: [Kind; 12] = [
    Kind::Mutation,
    Kind::Soup,
    Kind::Targeted,
    Kind::Mutation,
    Kind::Deep,
    Kind::Soup,
    Kind::Targeted,
    Kind::Mutation,
    Kind::Soup,
    Kind::Targeted,
    Kind::Mutation,
    Kind::Deep,
];

/// How many targeted inputs come before input `k`: the snippet group of
/// the next one is this plus the seed, so the groups go round and every
/// seed starts at a different one.
fn targeted_before(k: usize) -> usize {
    let per_block = PATTERN.iter().filter(|p| **p == Kind::Targeted).count();
    let in_block = PATTERN[..k % PATTERN.len()]
        .iter()
        .filter(|p| **p == Kind::Targeted)
        .count();
    k / PATTERN.len() * per_block + in_block
}

/// The standard run: [`PER_SEED`] inputs from each of [`SEEDS`].
pub fn standard_run(corpus: &[String]) -> Vec<Input> {
    SEEDS
        .iter()
        .flat_map(|seed| generate(*seed, PER_SEED, corpus))
        .collect()
}

/// `count` inputs from `seed`; input `k` depends on the seed, `k` and the
/// corpus only.
pub fn generate(seed: u64, count: usize, corpus: &[String]) -> Vec<Input> {
    (0..count)
        .map(|k| {
            let mut rng = Rng::new(seed ^ (k as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
            for _ in 0..4 {
                rng.next();
            }
            let (label, text) = match PATTERN[k % PATTERN.len()] {
                Kind::Mutation => ("mutation", mutated(&mut rng, corpus)),
                Kind::Soup => ("soup", soup(&mut rng)),
                Kind::Targeted => {
                    let group = seed as usize + targeted_before(k);
                    ("targeted", targeted(&mut rng, group))
                }
                Kind::Deep => ("deep", deep(&mut rng)),
            };
            Input {
                name: format!("fuzz-s{seed}-{k:04}-{label}.fib"),
                text: clamp(text),
            }
        })
        .collect()
}

/// Writes the inputs into `dir` and returns their paths in order.
pub fn write_inputs(dir: &Path, inputs: &[Input]) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).expect("the scratch directory is writable");
    inputs
        .iter()
        .map(|input| {
            let path = dir.join(&input.name);
            std::fs::write(&path, &input.text).expect("the scratch directory is writable");
            path
        })
        .collect()
}

/// The seed and count of a manual run, read through `var` (the process
/// environment in the test, a table in the unit tests):
/// `BOOTSTRAP_FUZZ_SEED` (default 1) and `BOOTSTRAP_FUZZ_COUNT` (default
/// 1000, at most [`MAX_COUNT`]). Text that is not a number is the default.
pub fn manual_settings(var: impl Fn(&str) -> Option<String>) -> (u64, usize) {
    let number = |name: &str| var(name).and_then(|v| v.trim().parse::<u64>().ok());
    let seed = number("BOOTSTRAP_FUZZ_SEED").unwrap_or(1);
    let count = number("BOOTSTRAP_FUZZ_COUNT").map_or(1000, |n| n.clamp(1, MAX_COUNT as u64));
    (seed, usize::try_from(count).expect("at most MAX_COUNT"))
}

/// The largest index at or below `i` (and at most the length) that is a
/// character boundary of `text`.
fn boundary(text: &str, i: usize) -> usize {
    let mut i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn pick_boundary(rng: &mut Rng, text: &str) -> usize {
    boundary(text, rng.below(text.len() + 1))
}

/// A boundary at least `at` and at most `max` bytes past it.
fn span_end(rng: &mut Rng, text: &str, at: usize, max: usize) -> usize {
    boundary(text, at + rng.between(0, max))
}

fn clamp(mut text: String) -> String {
    let end = boundary(&text, MAX_BYTES);
    text.truncate(end);
    text
}

/// The longest stretch of a corpus file a mutation starts from.
const WINDOW: usize = 3000;

fn mutated(rng: &mut Rng, corpus: &[String]) -> String {
    let mut text = match corpus.len() {
        0 => String::new(),
        n => {
            let chosen = rng.below(n);
            window(rng, &corpus[chosen])
        }
    };
    for _ in 0..rng.between(1, 3) {
        text = mutate_once(rng, &text);
    }
    text
}

/// `text`, or a stretch of it of [`WINDOW`] bytes if it is longer.
fn window(rng: &mut Rng, text: &str) -> String {
    if text.len() <= WINDOW {
        return text.to_string();
    }
    let start = boundary(text, rng.below(text.len() - WINDOW));
    text[start..boundary(text, start + WINDOW)].to_string()
}

fn mutate_once(rng: &mut Rng, text: &str) -> String {
    let a = pick_boundary(rng, text);
    let b = span_end(rng, text, a, 40);
    match rng.weighted(&[2, 2, 1, 4]) {
        0 => format!("{}{}", &text[..a], &text[b..]),
        1 => format!("{}{}{}", &text[..b], &text[a..b], &text[b..]),
        2 => {
            let c = span_end(rng, text, b, 40);
            let d = span_end(rng, text, c, 40);
            let parts = [
                &text[..a],
                &text[c..d],
                &text[b..c],
                &text[a..b],
                &text[d..],
            ];
            parts.concat()
        }
        _ => format!("{}{}{}", &text[..a], rng.pick(FRAGMENTS), &text[a..]),
    }
}

/// Text that changes how what is around it reads.
const FRAGMENTS: &[&str] = &[
    "\"", "\\", "\\u{", "\\x", "#_", "#", ",@", ",", "@", "&", "'", "`", ":", "::", "/", "0x",
    "0b", "1e", "_", "i8", "f32", "é", "😀", "\r\n", "\u{FEFF}", "\u{A0}", "\u{85}", "\0", "(",
    ")", "[", "]", "{", "}", ";", "\n", "-", ".", "e+", "\\u{41}", "\\n", "1.5", "300i8",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{corpus_files, texts};
    use crate::tool::repo_root;
    use std::collections::{BTreeMap, BTreeSet};

    fn corpus() -> Vec<String> {
        texts(&corpus_files(&repo_root()))
    }

    #[test]
    fn the_same_seed_gives_the_same_inputs_and_another_seed_other_ones() {
        let c = corpus();
        let (a, b, other) = (
            generate(1, 30, &c),
            generate(1, 30, &c),
            generate(2, 30, &c),
        );
        assert!(a
            .iter()
            .zip(&b)
            .all(|(x, y)| x.name == y.name && x.text == y.text));
        assert!(a.iter().zip(&other).any(|(x, y)| x.text != y.text));
    }

    #[test]
    fn the_standard_run_is_300_distinct_small_inputs() {
        let inputs = standard_run(&corpus());
        assert_eq!(inputs.len(), 300);
        assert!(inputs.iter().all(|i| i.text.len() <= MAX_BYTES), "size");
        let names: BTreeSet<&str> = inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names.len(), 300, "names are unique");
        let texts: BTreeSet<&str> = inputs.iter().map(|i| i.text.as_str()).collect();
        assert!(texts.len() > 280, "{} distinct texts", texts.len());
    }

    /// What the standard run's inputs read as: how many inputs end in each
    /// kind of read error, and how many nodes of each kind are produced.
    fn outcomes(inputs: &[Input]) -> (BTreeMap<String, usize>, BTreeMap<String, usize>) {
        let (mut errors, mut nodes) = (BTreeMap::new(), BTreeMap::new());
        for input in inputs {
            let dump = fibref::dump::dump_source(&input.text, &input.name);
            for line in dump.lines() {
                let mut words = line.split_whitespace();
                match (words.next(), words.next()) {
                    (Some("error"), Some(kind)) => {
                        *errors.entry(kind.to_string()).or_default() += 1
                    }
                    (Some(node), _) => *nodes.entry(node.to_string()).or_default() += 1,
                    _ => {}
                }
            }
        }
        (errors, nodes)
    }

    /// The generated inputs must reach every read error and every kind of
    /// node, or the differential test over them proves little. Token soup
    /// and deep nesting alone must do it (the corpus, which the mutations
    /// start from, is left empty, so this holds however it grows).
    #[test]
    fn the_standard_run_reaches_every_read_error_and_every_node_kind() {
        let (errors, nodes) = outcomes(&standard_run(&[]));
        let want_errors = [
            "UnterminatedString",
            "BadEscape",
            "BadUnicodeEscape",
            "BadCharLiteral",
            "InvalidCharacter",
            "InvalidNumber",
            "IntegerOutOfRange",
            "FloatOutOfRange",
            "InvalidSymbol",
            "InvalidKeyword",
            "UnknownDispatch",
            "PrefixWithoutForm",
            "DiscardWithoutForm",
            "InOutNotSymbol",
            "Unclosed",
            "UnexpectedClose",
            "MismatchedClose",
            "OddMapEntries",
            "TooDeep",
        ];
        let rare: Vec<String> = want_errors
            .iter()
            .filter(|k| errors.get(**k).copied().unwrap_or(0) < 2)
            .map(|k| format!("{k} {}", errors.get(*k).copied().unwrap_or(0)))
            .collect();
        assert!(
            rare.is_empty(),
            "reached fewer than 2 times: {rare:?}; all {errors:?}"
        );
        let want_nodes = [
            "sym", "kw", "int", "flt", "str", "chr", "bool", "nil", "list", "vec", "map",
        ];
        let rare: Vec<&str> = want_nodes
            .iter()
            .copied()
            .filter(|k| nodes.get(*k).copied().unwrap_or(0) < 10)
            .collect();
        assert!(
            rare.is_empty(),
            "produced fewer than 10 times: {rare:?}; all {nodes:?}"
        );
    }

    #[test]
    fn mutations_of_the_corpus_both_read_and_fail() {
        let mutations: Vec<Input> = standard_run(&corpus())
            .into_iter()
            .filter(|i| i.name.ends_with("-mutation.fib"))
            .collect();
        let failing = mutations
            .iter()
            .filter(|i| fibref::dump::dump_source(&i.text, "m").starts_with("error "))
            .count();
        assert!(mutations.len() >= 100, "{} mutations", mutations.len());
        assert!(
            failing * 5 >= mutations.len(),
            "only {failing} of {} fail",
            mutations.len()
        );
        assert!(
            failing * 5 <= mutations.len() * 4,
            "{failing} of {} fail",
            mutations.len()
        );
    }

    #[test]
    fn the_inputs_reach_the_text_the_task_names() {
        let all: String = standard_run(&corpus())
            .iter()
            .map(|i| i.text.as_str())
            .collect();
        for needle in [
            "\r\n", "\u{FEFF}", "\u{A0}", "\u{85}", "\0", "😀", "é", "#_", ",@", "\\u{",
        ] {
            assert!(all.contains(needle), "no input contains {needle:?}");
        }
    }

    #[test]
    fn mutations_keep_character_boundaries_on_multibyte_text() {
        let c = vec!["é😀λ(a \"é😀\")\n".repeat(20)];
        for seed in 0..40 {
            for input in generate(seed, 12, &c) {
                assert!(input.text.len() <= MAX_BYTES);
            }
        }
    }

    #[test]
    fn manual_settings_default_clamp_and_ignore_garbage() {
        let with = |pairs: &'static [(&'static str, &'static str)]| {
            manual_settings(move |k| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            })
        };
        assert_eq!(with(&[]), (1, 1000));
        assert_eq!(
            with(&[("BOOTSTRAP_FUZZ_COUNT", "50"), ("BOOTSTRAP_FUZZ_SEED", "9")]),
            (9, 50)
        );
        assert_eq!(with(&[("BOOTSTRAP_FUZZ_COUNT", "999999")]).1, MAX_COUNT);
        assert_eq!(with(&[("BOOTSTRAP_FUZZ_COUNT", "0")]).1, 1);
        assert_eq!(
            with(&[
                ("BOOTSTRAP_FUZZ_COUNT", "many"),
                ("BOOTSTRAP_FUZZ_SEED", "-3")
            ]),
            (1, 1000)
        );
    }
}
