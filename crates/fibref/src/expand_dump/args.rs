//! The words `fibref expand` and `compiler/expand.fib` take before the
//! files (spec/bootstrap.md §5), read by the one function the binary and
//! the tests share, and written from an [`Options`] by [`flags`].

use super::{LimitOverrides, Options, RunnerKind};

/// The options and the files of the words after `expand`: the words that
/// start with `--` before the first file are options (`--prelude`,
/// `--context`, `--implicit`, `--implicit-lib LIST`, `--no-runner`,
/// `--max-steps N`, `--max-depth N`, `--max-forms N`, each number decimal
/// digits, LIST module names separated by commas, or nothing), and a lone
/// `--` ends them, so that a file named like an option can follow it.
/// `None` for a word that is not an option, a number that is not digits,
/// a LIST with an empty name, or no file.
pub fn parse_args(args: &[String]) -> Option<(Options, Vec<String>)> {
    let mut opts = Options::default();
    let mut rest = args;
    while let [word, tail @ ..] = rest {
        rest = match word.as_str() {
            "--" => {
                rest = tail;
                break;
            }
            "--prelude" => {
                opts.prelude = true;
                tail
            }
            "--context" => {
                opts.context = true;
                tail
            }
            "--implicit" => {
                opts.implicit = true;
                tail
            }
            "--implicit-lib" => {
                let [list, after @ ..] = tail else {
                    return None;
                };
                opts.implicit_lib = Some(module_list(list)?);
                after
            }
            "--no-runner" => {
                opts.runner = RunnerKind::None;
                tail
            }
            "--max-steps" => limit(&mut opts.limits.steps, tail)?,
            "--max-depth" => limit(&mut opts.limits.depth, tail)?,
            "--max-forms" => limit(&mut opts.limits.forms, tail)?,
            w if w.starts_with("--") => return None,
            _ => break,
        };
    }
    (!rest.is_empty()).then(|| (opts, rest.to_vec()))
}

/// The module names of a `--implicit-lib` list: none for the empty text,
/// else one per comma, each not empty.
fn module_list(list: &str) -> Option<Vec<String>> {
    if list.is_empty() {
        return Some(Vec::new());
    }
    let names: Vec<String> = list.split(',').map(str::to_string).collect();
    names.iter().all(|n| !n.is_empty()).then_some(names)
}

/// Reads the number that starts `tail` into `slot`; the words after it.
fn limit<'a>(slot: &mut Option<usize>, tail: &'a [String]) -> Option<&'a [String]> {
    let [n, after @ ..] = tail else { return None };
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    *slot = Some(n.parse::<usize>().ok()?);
    Some(after)
}

/// The option words that make [`parse_args`] read `opts`, in the order
/// `--prelude`, `--context`, `--implicit`, `--implicit-lib`, `--no-runner`,
/// `--max-steps`, `--max-depth`, `--max-forms`; none for the defaults.
pub fn flags(opts: &Options) -> Vec<String> {
    let mut words = Vec::new();
    let flag = |words: &mut Vec<String>, on: bool, word: &str| {
        if on {
            words.push(word.to_string());
        }
    };
    flag(&mut words, opts.prelude, "--prelude");
    flag(&mut words, opts.context, "--context");
    flag(&mut words, opts.implicit, "--implicit");
    if let Some(lib) = &opts.implicit_lib {
        words.push("--implicit-lib".to_string());
        words.push(lib.join(","));
    }
    flag(&mut words, opts.runner == RunnerKind::None, "--no-runner");
    let LimitOverrides {
        steps,
        depth,
        forms,
    } = opts.limits;
    for (word, value) in [
        ("--max-steps", steps),
        ("--max-depth", depth),
        ("--max-forms", forms),
    ] {
        if let Some(n) = value {
            words.push(word.to_string());
            words.push(n.to_string());
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_come_before_one_or_more_files() {
        let opts = Options {
            prelude: true,
            context: true,
            implicit: true,
            implicit_lib: Some(words(&["fib.a", "fib.b"])),
            runner: RunnerKind::None,
            limits: LimitOverrides {
                steps: Some(3),
                depth: Some(4),
                forms: Some(5),
            },
        };
        let all = words(&[
            "--prelude",
            "--context",
            "--implicit",
            "--implicit-lib",
            "fib.a,fib.b",
            "--no-runner",
            "--max-steps",
            "3",
            "--max-depth",
            "4",
            "--max-forms",
            "5",
            "a.fib",
            "b.fib",
        ]);
        assert_eq!(parse_args(&all), Some((opts, words(&["a.fib", "b.fib"]))));
        assert_eq!(
            parse_args(&words(&["a.fib"])),
            Some((Options::default(), words(&["a.fib"])))
        );
    }

    #[test]
    fn no_file_an_unknown_option_and_a_limit_that_is_not_digits_are_refused() {
        for bad in [
            &["--context"][..],
            &[],
            &["--bogus", "a.fib"],
            &["--max-steps", "x", "a.fib"],
            &["--max-steps"],
            &["--max-steps", "-1", "a.fib"],
            &["--max-steps", "+1", "a.fib"],
            &["--max-steps", "", "a.fib"],
            &["--max-steps", "99999999999999999999999", "a.fib"],
            &["--implicit-lib"],
            &["--implicit-lib", "a.fib"],
            &["--implicit-lib", "a,,b", "a.fib"],
            &["--implicit-lib", ",a", "a.fib"],
            &["--implicit-lib", "a,", "a.fib"],
        ] {
            assert_eq!(parse_args(&words(bad)), None, "{bad:?}");
        }
    }

    #[test]
    fn a_file_named_like_an_option_follows_two_dashes_or_a_file() {
        assert_eq!(
            parse_args(&words(&["--", "--context"])),
            Some((Options::default(), words(&["--context"])))
        );
        assert_eq!(
            parse_args(&words(&["a.fib", "--context"])),
            Some((Options::default(), words(&["a.fib", "--context"])))
        );
    }

    #[test]
    fn the_flags_of_options_read_back_as_those_options() {
        let all = [
            Options::default(),
            Options {
                prelude: true,
                ..Options::default()
            },
            Options {
                implicit: true,
                implicit_lib: Some(Vec::new()),
                ..Options::default()
            },
            Options {
                implicit_lib: Some(words(&["fib.core", "fib.seq"])),
                ..Options::default()
            },
            Options {
                context: true,
                runner: RunnerKind::None,
                limits: LimitOverrides {
                    steps: Some(0),
                    depth: None,
                    forms: Some(12),
                },
                ..Options::default()
            },
        ];
        for opts in all {
            let mut line = flags(&opts);
            line.push("f.fib".to_string());
            let read = parse_args(&line);
            assert_eq!(read, Some((opts.clone(), words(&["f.fib"]))), "{opts:?}");
        }
        assert_eq!(flags(&Options::default()), Vec::<String>::new());
        let none = Options {
            implicit_lib: Some(Vec::new()),
            ..Options::default()
        };
        assert_eq!(flags(&none), words(&["--implicit-lib", ""]));
    }
}
