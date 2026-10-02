//! The words `fibref own` and `compiler/own.fib` take before the files
//! (spec/bootstrap.md §7.2), read by the one function the binary and the
//! tests share, and written from an [`Options`] by [`flags`].

use std::collections::BTreeSet;

use super::{Options, Section};
use crate::expand_dump::module_list;

/// The options and the files of the words after `own`: the words that
/// start with `--` before the first file are options (`--sections LIST`,
/// `--library`, `--implicit`, `--implicit-lib LIST`, `--prelude`; LIST of
/// `--sections` is section names separated by commas, of `--implicit-lib`
/// module names separated by commas or nothing), and a lone `--` ends
/// them, so that a file named like an option can follow it. `None` for a
/// word that is not an option, a section that is not one, or no file.
pub fn parse_args(args: &[String]) -> Option<(Options, Vec<String>)> {
    let mut opts = Options::default();
    let mut rest = args;
    while let [word, tail @ ..] = rest {
        rest = match (word.as_str(), tail) {
            ("--", _) => {
                rest = tail;
                break;
            }
            ("--sections", [list, after @ ..]) => {
                opts.sections = Some(section_list(list)?);
                after
            }
            ("--implicit-lib", [list, after @ ..]) => {
                opts.implicit_lib = Some(module_list(list)?);
                after
            }
            ("--sections" | "--implicit-lib", _) => return None,
            ("--library", _) => flag(&mut opts.library, tail),
            ("--implicit", _) => flag(&mut opts.implicit, tail),
            ("--prelude", _) => flag(&mut opts.prelude, tail),
            (w, _) if w.starts_with("--") => return None,
            _ => break,
        };
    }
    (!rest.is_empty()).then(|| (opts, rest.to_vec()))
}

fn flag<'a>(slot: &mut bool, tail: &'a [String]) -> &'a [String] {
    *slot = true;
    tail
}

/// The sections a `--sections` list names: one per comma, each a known
/// name.
fn section_list(list: &str) -> Option<BTreeSet<Section>> {
    list.split(',').map(Section::from_name).collect()
}

/// The option words that make [`parse_args`] read `opts`, in the order
/// `--sections`, `--library`, `--implicit`, `--implicit-lib`, `--prelude`;
/// none for the defaults.
pub fn flags(opts: &Options) -> Vec<String> {
    let mut words = Vec::new();
    if let Some(set) = &opts.sections {
        let names: Vec<&str> = set.iter().map(|s| s.name()).collect();
        words.extend(["--sections".to_string(), names.join(",")]);
    }
    let on = |words: &mut Vec<String>, set: bool, word: &str| {
        if set {
            words.push(word.to_string());
        }
    };
    on(&mut words, opts.library, "--library");
    on(&mut words, opts.implicit, "--implicit");
    if let Some(lib) = &opts.implicit_lib {
        words.extend(["--implicit-lib".to_string(), lib.join(",")]);
    }
    on(&mut words, opts.prelude, "--prelude");
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
            sections: Some(BTreeSet::from([Section::Error, Section::Body])),
            library: true,
            implicit: true,
            implicit_lib: Some(words(&["fib.a", "fib.b"])),
            roots: Vec::new(),
            prelude: true,
        };
        let all = words(&[
            "--sections",
            "body,error",
            "--library",
            "--implicit",
            "--implicit-lib",
            "fib.a,fib.b",
            "--prelude",
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
    fn no_file_an_unknown_option_or_section_are_refused() {
        for bad in [
            &["--library"][..],
            &[],
            &["--bogus", "a.fib"],
            &["--stage", "infer", "a.fib"],
            &["--ast", "a.fib"],
            &["--sections"],
            &["--sections", "", "a.fib"],
            &["--sections", "body,", "a.fib"],
            &["--sections", "bodies", "a.fib"],
            &["--sections", "Body", "a.fib"],
            &["--sections", "type", "a.fib"],
            &["--implicit-lib"],
            &["--implicit-lib", "a,,b", "a.fib"],
        ] {
            assert_eq!(parse_args(&words(bad)), None, "{bad:?}");
        }
    }

    #[test]
    fn a_file_named_like_an_option_follows_two_dashes_or_a_file() {
        assert_eq!(
            parse_args(&words(&["--", "--library"])),
            Some((Options::default(), words(&["--library"])))
        );
        assert_eq!(
            parse_args(&words(&["a.fib", "--library"])),
            Some((Options::default(), words(&["a.fib", "--library"])))
        );
    }

    #[test]
    fn the_flags_of_options_read_back_as_those_options() {
        let all = [
            Options::default(),
            Options {
                sections: Some(BTreeSet::from(Section::ALL)),
                ..Options::default()
            },
            Options {
                implicit: true,
                implicit_lib: Some(Vec::new()),
                ..Options::default()
            },
            Options {
                library: true,
                prelude: true,
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
    }

    #[test]
    fn a_section_is_printed_when_listed_and_explain_only_when_listed() {
        let plain = Options::default();
        assert!(Section::ALL[..5].iter().all(|s| plain.wants(*s)));
        assert!(!plain.wants(Section::Explain));
        let only = Options {
            sections: Some(BTreeSet::from([Section::Explain, Section::Taken])),
            ..Options::default()
        };
        assert!(only.wants(Section::Explain) && only.wants(Section::Taken));
        assert!(!only.wants(Section::Body) && !only.wants(Section::Error));
    }
}
