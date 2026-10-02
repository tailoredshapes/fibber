//! The words `fibc emit-dump` takes before the files (spec/bootstrap.md
//! §8.2), read by the one function the binary and the tests share, and
//! written from an [`Options`] by [`flags`].

use std::collections::BTreeSet;

use super::{Options, Section};

/// The options and the files of the words after `emit-dump` (the `-I` words
/// are the command line's own, taken out before): the words that start with
/// `--` before the first file are options (`--sections LIST`, `--fn
/// PREFIX`, `--layout`, `--macro NAME`; LIST is section names separated by
/// commas), and a lone `--` ends them, so that a file named like an option
/// can follow it. `None` for a word that is not an option, a section that
/// is not one, an empty `--fn` or `--macro` value, `--layout` or `--macro`
/// together with another mode (§8.2), or no file.
pub fn parse_args(args: &[String]) -> Option<(Options, Vec<String>)> {
    let mut opts = Options::default();
    let mut rest = args;
    while let [word, tail @ ..] = rest {
        rest = match word.as_str() {
            "--" => {
                rest = tail;
                break;
            }
            "--sections" => {
                let [list, after @ ..] = tail else {
                    return None;
                };
                opts.sections = Some(section_list(list)?);
                after
            }
            "--fn" => {
                let (prefix, after) = value(tail)?;
                opts.fn_prefix = Some(prefix);
                after
            }
            "--macro" => {
                let (name, after) = value(tail)?;
                opts.macro_name = Some(name);
                after
            }
            "--layout" => {
                opts.layout = true;
                tail
            }
            w if w.starts_with("--") => return None,
            _ => break,
        };
    }
    let alone = u8::from(opts.layout) + u8::from(opts.macro_name.is_some());
    let others = opts.sections.is_some() || opts.fn_prefix.is_some();
    if alone > 1 || (alone == 1 && others) {
        return None;
    }
    (!rest.is_empty()).then(|| (opts, rest.to_vec()))
}

/// The non-empty value that follows an option, and the words after it.
fn value(words: &[String]) -> Option<(String, &[String])> {
    match words {
        [v, after @ ..] if !v.is_empty() => Some((v.clone(), after)),
        _ => None,
    }
}

/// The sections a `--sections` list names: one per comma, each a known
/// name.
fn section_list(list: &str) -> Option<BTreeSet<Section>> {
    list.split(',').map(Section::from_name).collect()
}

/// The option words that make [`parse_args`] read `opts`, in the order
/// `--sections`, `--fn`, `--layout`, `--macro`; none for the defaults.
pub fn flags(opts: &Options) -> Vec<String> {
    let mut words = Vec::new();
    if let Some(set) = &opts.sections {
        let names: Vec<&str> = set.iter().map(|s| s.name()).collect();
        words.extend(["--sections".to_string(), names.join(",")]);
    }
    if let Some(prefix) = &opts.fn_prefix {
        words.extend(["--fn".to_string(), prefix.clone()]);
    }
    if opts.layout {
        words.push("--layout".to_string());
    }
    if let Some(name) = &opts.macro_name {
        words.extend(["--macro".to_string(), name.clone()]);
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
        let sections = Options {
            sections: Some(BTreeSet::from([Section::Fns, Section::Runtime])),
            fn_prefix: Some("f.main".to_string()),
            ..Options::default()
        };
        let line = words(&[
            "--sections",
            "fns,runtime",
            "--fn",
            "f.main",
            "a.fib",
            "b.fib",
        ]);
        assert_eq!(
            parse_args(&line),
            Some((sections, words(&["a.fib", "b.fib"])))
        );
        let layout = Options {
            layout: true,
            ..Options::default()
        };
        assert_eq!(
            parse_args(&words(&["--layout", "a.fib"])),
            Some((layout, words(&["a.fib"])))
        );
        let mac = Options {
            macro_name: Some("m".to_string()),
            ..Options::default()
        };
        assert_eq!(
            parse_args(&words(&["--macro", "m", "a.fib"])),
            Some((mac, words(&["a.fib"])))
        );
        assert_eq!(
            parse_args(&words(&["a.fib"])),
            Some((Options::default(), words(&["a.fib"])))
        );
    }

    #[test]
    fn a_word_that_is_not_an_option_a_bad_list_or_two_modes_are_refused() {
        for bad in [
            &["--layout"][..],
            &[],
            &["--bogus", "a.fib"],
            &["--sections"],
            &["--sections", "", "a.fib"],
            &["--sections", "fns,", "a.fib"],
            &["--sections", "fn", "a.fib"],
            &["--sections", "Fns", "a.fib"],
            &["--fn"],
            &["--fn", "", "a.fib"],
            &["--macro"],
            &["--macro", "", "a.fib"],
            &["--layout", "--macro", "m", "a.fib"],
            &["--layout", "--sections", "fns", "a.fib"],
            &["--layout", "--fn", "f", "a.fib"],
            &["--macro", "m", "--sections", "fns", "a.fib"],
            &["--macro", "m", "--fn", "f", "a.fib"],
        ] {
            assert_eq!(parse_args(&words(bad)), None, "{bad:?}");
        }
    }

    #[test]
    fn a_file_named_like_an_option_follows_two_dashes_or_a_file() {
        assert_eq!(
            parse_args(&words(&["--", "--layout"])),
            Some((Options::default(), words(&["--layout"])))
        );
        assert_eq!(
            parse_args(&words(&["a.fib", "--layout"])),
            Some((Options::default(), words(&["a.fib", "--layout"])))
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
                fn_prefix: Some("m.Show".to_string()),
                ..Options::default()
            },
            Options {
                layout: true,
                ..Options::default()
            },
            Options {
                macro_name: Some("unless".to_string()),
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
    fn a_section_is_printed_when_listed_or_by_default_and_fns_alone_with_a_prefix() {
        let plain = Options::default();
        assert!(Section::ALL.iter().all(|s| plain.wants(*s)));
        let only = Options {
            sections: Some(BTreeSet::from([Section::Main])),
            ..Options::default()
        };
        assert!(only.wants(Section::Main) && !only.wants(Section::Fns));
        let prefix = Options {
            fn_prefix: Some("f.".to_string()),
            ..Options::default()
        };
        assert!(prefix.wants(Section::Fns) && !prefix.wants(Section::Runtime));
        let both = Options {
            sections: Some(BTreeSet::from([Section::Runtime])),
            ..prefix
        };
        assert!(both.wants(Section::Runtime) && !both.wants(Section::Fns));
    }
}
