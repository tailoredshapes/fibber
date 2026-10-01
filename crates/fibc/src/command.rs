//! The command line of `fibc` (spec/compiler.md §1): the usage text and
//! the parse of the arguments into a [`Command`]. Nothing here runs a
//! command; `main.rs` does.

use std::path::PathBuf;

use fibc::harness::gen::GenConfig;

pub const USAGE: &str = "usage: fibc <command>

Every command that reads a program (run, build, emit, explain, itrace) takes
-I dir (or -Idir), any number of times, anywhere before --: a module is found
beside the file, then under each -I dir in order, then under each directory of
$FIB_LIB, then in the library the executable carries (spec/syntax.md §5).

commands:
  run [--trace] [-O N] <file> [-- arg..]
                         compile file through the JIT and run its main; the
                         args after -- are (args), a byte that is not UTF-8 in
                         one becoming U+FFFD as Rust's from_utf8_lossy makes it
  build <file> -o <out> [-O N] [-L dir].. [-l lib]..
                         compile file to an executable; each -l lib links
                         -llib, each -L dir is searched for libraries and
                         becomes an rpath (as an absolute path), so the
                         executable finds its libraries without
                         LD_LIBRARY_PATH (-Ldir and -llib are also taken)

-O N (N from 0 to 3, or -ON) is the LLVM optimisation level of run and build:
build defaults to 2, run to 0 (a run compiles the program afresh every time,
and -O 2 costs about two and a half times as long to compile as -O 0).
  emit <file>            print the lIR module of file
  explain <file>         print the ownership checker's decisions (types §9)
  itrace <file>          run file in the reference interpreter and print its
                         canonical trace (compiler.md §4)
  cases [dir [--only prefix..]]
                         run every case in dir (default cases/ownership)
                         interpreted and compiled, and compare (method.md rule 6);
                         with --only, the cases whose names start with one of the
                         prefixes (a prefix that matches no case is an error);
                         a case with an `open` label that fails as it says is
                         OPEN: listed, not a failure (cases/stdlib/README.md)
  gen --seed S --count N [--size K] [--jobs J] [--dir D]
                         generate N programs with fibgen from seed S (size K, or
                         sizes 1..=6 in turn) and run each interpreted and
                         compiled against the model's verdict; a program that
                         does not pass is kept in D. J defaults to at most 8:
                         a job holds a heap trace and a JIT-compiling child
  help                   print this message";

pub const DEFAULT_CASES_DIR: &str = "cases/ownership";

/// The LLVM optimisation level of `run` without `-O`: none, because a run
/// compiles the program afresh every time and `-O 2` costs about two and
/// a half times as long to compile (stdlib design §6.4, C6).
pub const RUN_OPT_LEVEL: u8 = 0;

/// The level of `build` without `-O`: the executable is compiled once and
/// run many times (stdlib design §7 C6).
pub const BUILD_OPT_LEVEL: u8 = 2;

/// What `build` links besides the program: the `-L` directories and
/// the `-l` library names, each in the order given.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Link {
    pub dirs: Vec<String>,
    pub libs: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run {
        file: String,
        trace: bool,
        args: Vec<String>,
        opt: u8,
    },
    Build {
        file: String,
        out: String,
        link: Link,
        opt: u8,
    },
    Emit {
        file: String,
    },
    Explain {
        file: String,
    },
    Itrace {
        file: String,
    },
    Cases {
        dir: String,
        only: Vec<String>,
    },
    Gen(GenConfig),
    Help,
    Invalid,
}

/// `gen`'s options; a bad or missing value is `Invalid`.
fn parse_gen(args: &[&str]) -> Command {
    let mut cfg = GenConfig {
        seed: 1,
        count: 100,
        size: None,
        // Each job holds an interpreter's whole heap trace beside a
        // JIT-compiling child: bound by memory, not by processors.
        jobs: std::thread::available_parallelism().map_or(2, |n| n.get().min(8)),
        dir: std::env::temp_dir().join(format!("fibc-gen-{}", std::process::id())),
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let Some(val) = it.next() else {
            return Command::Invalid;
        };
        let num = val.parse::<u64>();
        match (*flag, num) {
            ("--seed", Ok(n)) => cfg.seed = n,
            ("--count", Ok(n)) => cfg.count = n as usize,
            ("--size", Ok(n)) => cfg.size = Some(n as u32),
            ("--jobs", Ok(n)) => cfg.jobs = n as usize,
            ("--dir", _) => cfg.dir = PathBuf::from(val),
            _ => return Command::Invalid,
        }
    }
    Command::Gen(cfg)
}

/// A flag and its value, as `cc` takes them: `-L DIR` or `-LDIR`,
/// `-l LIB` or `-lLIB`, `-O N` or `-ON`, `-o OUT`. `None` when the value
/// is missing or empty.
fn flag_value<'a>(
    flag: &'a str,
    rest: &mut impl Iterator<Item = &'a &'a str>,
) -> Option<(&'a str, &'a str)> {
    let glued = match flag.get(..2) {
        Some(name @ ("-L" | "-l" | "-O")) if flag.len() > 2 => Some((name, &flag[2..])),
        _ => None,
    };
    let (name, value) = match glued {
        Some(pair) => pair,
        None => (flag, *rest.next()?),
    };
    (!value.is_empty()).then_some((name, value))
}

/// An optimisation level: one digit, 0 to 3.
fn opt_level(text: &str) -> Option<u8> {
    match text {
        "0" => Some(0),
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        _ => None,
    }
}

/// `build FILE -o OUT` and any number of `-L DIR` and `-l LIB`, and at
/// most one `-O N`, in any order after the file; `-o` exactly once.
/// Anything else is `Invalid`.
fn parse_build(file: &str, rest: &[&str]) -> Command {
    let (mut out, mut link, mut opt) = (None, Link::default(), None);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let Some((name, value)) = flag_value(flag, &mut it) else {
            return Command::Invalid;
        };
        match name {
            "-o" if out.is_none() => out = Some(value),
            "-L" => link.dirs.push(value.to_string()),
            "-l" => link.libs.push(value.to_string()),
            "-O" if opt.is_none() => match opt_level(value) {
                Some(n) => opt = Some(n),
                None => return Command::Invalid,
            },
            _ => return Command::Invalid,
        }
    }
    match out {
        Some(out) => Command::Build {
            file: file.to_string(),
            out: out.to_string(),
            link,
            opt: opt.unwrap_or(BUILD_OPT_LEVEL),
        },
        None => Command::Invalid,
    }
}

/// `run [--trace] [-O N] FILE [-- ARG..]`: the flags in either order
/// before the file. A flag has to be followed by a word that is not `--`,
/// the file, so `run --trace` is the file `--trace` as it always was.
fn parse_run(rest: &[&str]) -> Command {
    let (mut trace, mut opt, mut at) = (false, None, 0);
    while rest.get(at + 1).is_some_and(|next| *next != "--") {
        let word = rest[at];
        if word == "--trace" && !trace {
            trace = true;
            at += 1;
        } else if word.starts_with("-O") && opt.is_none() {
            let (value, used) = match word.get(2..) {
                Some("") => (rest[at + 1], 2),
                Some(glued) => (glued, 1),
                None => return Command::Invalid,
            };
            match opt_level(value) {
                Some(n) => opt = Some(n),
                None => return Command::Invalid,
            }
            at += used;
        } else {
            break;
        }
    }
    let (file, args) = match &rest[at..] {
        [file] => (file, Vec::new()),
        [file, "--", args @ ..] => (file, args.iter().map(|s| s.to_string()).collect()),
        _ => return Command::Invalid,
    };
    Command::Run {
        file: file.to_string(),
        trace,
        args,
        opt: opt.unwrap_or(RUN_OPT_LEVEL),
    }
}

pub fn parse(args: &[String]) -> Command {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["run", rest @ ..] => parse_run(rest),
        ["build", file, rest @ ..] => parse_build(file, rest),
        ["emit", file] => Command::Emit {
            file: file.to_string(),
        },
        ["explain", file] => Command::Explain {
            file: file.to_string(),
        },
        ["itrace", file] => Command::Itrace {
            file: file.to_string(),
        },
        ["cases"] => Command::Cases {
            dir: DEFAULT_CASES_DIR.to_string(),
            only: Vec::new(),
        },
        ["cases", dir] => Command::Cases {
            dir: dir.to_string(),
            only: Vec::new(),
        },
        ["cases", dir, "--only", prefixes @ ..] if !prefixes.is_empty() => Command::Cases {
            dir: dir.to_string(),
            only: prefixes.iter().map(|p| p.to_string()).collect(),
        },
        ["gen", rest @ ..] => parse_gen(rest),
        ["help" | "--help" | "-h"] => Command::Help,
        _ => Command::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn build(file: &str, out: &str, dirs: &[&str], libs: &[&str]) -> Command {
        build_at(file, out, dirs, libs, BUILD_OPT_LEVEL)
    }

    fn build_at(file: &str, out: &str, dirs: &[&str], libs: &[&str], opt: u8) -> Command {
        Command::Build {
            file: file.into(),
            out: out.into(),
            link: Link {
                dirs: dirs.iter().map(|s| s.to_string()).collect(),
                libs: libs.iter().map(|s| s.to_string()).collect(),
            },
            opt,
        }
    }

    fn run_at(file: &str, trace: bool, words: &[&str], opt: u8) -> Command {
        Command::Run {
            file: file.into(),
            trace,
            args: words.iter().map(|s| s.to_string()).collect(),
            opt,
        }
    }

    #[test]
    fn parses_each_command() {
        assert_eq!(
            parse(&args(&["run", "--trace", "a.fib"])),
            run_at("a.fib", true, &[], 0)
        );
        assert_eq!(
            parse(&args(&["run", "a.fib", "--", "x", "y"])),
            run_at("a.fib", false, &["x", "y"], 0)
        );
        assert_eq!(
            parse(&args(&["build", "a.fib", "-o", "a"])),
            build("a.fib", "a", &[], &[])
        );
        assert_eq!(
            parse(&args(&["cases"])),
            Command::Cases {
                dir: DEFAULT_CASES_DIR.into(),
                only: Vec::new()
            }
        );
        assert_eq!(
            parse(&args(&["cases", "d", "--only", "240-", "241-"])),
            Command::Cases {
                dir: "d".into(),
                only: vec!["240-".into(), "241-".into()]
            }
        );
        assert_eq!(parse(&args(&["cases", "d", "--only"])), Command::Invalid);
        assert_eq!(parse(&args(&["cases", "--only", "1"])), Command::Invalid);
        assert_eq!(parse(&args(&["run"])), Command::Invalid);
        assert_eq!(parse(&args(&[])), Command::Invalid);
        assert_eq!(parse(&args(&["-h"])), Command::Help);
    }

    #[test]
    fn build_takes_any_number_of_dirs_and_libs_in_order() {
        let got = parse(&args(&[
            "build", "a.fib", "-o", "a", "-L", "d1", "-l", "lair", "-L", "d2", "-l", "m",
        ]));
        assert_eq!(got, build("a.fib", "a", &["d1", "d2"], &["lair", "m"]));
    }

    #[test]
    fn build_takes_the_flags_before_the_output_and_glued_to_their_value() {
        assert_eq!(
            parse(&args(&[
                "build", "a.fib", "-L", "d", "-lx", "-Ld2", "-o", "a"
            ])),
            build("a.fib", "a", &["d", "d2"], &["x"])
        );
    }

    #[test]
    fn build_refuses_what_it_cannot_make_sense_of() {
        let bad: [&[&str]; 11] = [
            &["build", "a.fib"],
            &["build", "a.fib", "-L", "d"],
            &["build", "a.fib", "-o"],
            &["build", "a.fib", "-o", "a", "-L"],
            &["build", "a.fib", "-o", "a", "-l"],
            &["build", "a.fib", "-o", "a", "-o", "b"],
            &["build", "a.fib", "-o", "a", "-L", ""],
            &["build", "a.fib", "-o", "a", "-l", ""],
            &["build", "a.fib", "-o", "a", "-x"],
            &["build", "a.fib", "-o", "a", "extra"],
            &["build", "-o", "a"],
        ];
        for line in bad {
            assert_eq!(parse(&args(line)), Command::Invalid, "{line:?}");
        }
    }

    #[test]
    fn a_library_directory_or_name_may_look_like_a_flag() {
        // The value after -L or -l is the value, whatever it starts with,
        // as `cc -L -x` takes it; only the glued form decides by prefix.
        assert_eq!(
            parse(&args(&[
                "build", "a.fib", "-o", "a", "-L", "-x", "-l", "-y"
            ])),
            build("a.fib", "a", &["-x"], &["-y"])
        );
    }

    #[test]
    fn build_defaults_to_level_2_and_run_to_level_0() {
        assert_eq!(BUILD_OPT_LEVEL, 2);
        assert_eq!(RUN_OPT_LEVEL, 0);
        assert_eq!(
            parse(&args(&["build", "a.fib", "-o", "a"])),
            build_at("a.fib", "a", &[], &[], 2)
        );
        assert_eq!(
            parse(&args(&["run", "a.fib"])),
            run_at("a.fib", false, &[], 0)
        );
    }

    #[test]
    fn build_takes_the_level_apart_or_glued_in_any_place_after_the_file() {
        for (line, level) in [
            (&["build", "a.fib", "-o", "a", "-O", "0"][..], 0),
            (&["build", "a.fib", "-O", "1", "-o", "a"][..], 1),
            (&["build", "a.fib", "-O3", "-o", "a"][..], 3),
            (
                &["build", "a.fib", "-o", "a", "-L", "d", "-O2", "-l", "m"][..],
                2,
            ),
        ] {
            let (dirs, libs): (&[&str], &[&str]) = if line.contains(&"-L") {
                (&["d"], &["m"])
            } else {
                (&[], &[])
            };
            assert_eq!(
                parse(&args(line)),
                build_at("a.fib", "a", dirs, libs, level),
                "{line:?}"
            );
        }
    }

    #[test]
    fn run_takes_the_level_and_trace_in_either_order_before_the_file() {
        for (line, trace, level) in [
            (&["run", "-O", "2", "a.fib"][..], false, 2),
            (&["run", "-O1", "a.fib"][..], false, 1),
            (&["run", "--trace", "-O", "3", "a.fib"][..], true, 3),
            (&["run", "-O0", "--trace", "a.fib"][..], true, 0),
        ] {
            assert_eq!(
                parse(&args(line)),
                run_at("a.fib", trace, &[], level),
                "{line:?}"
            );
        }
        assert_eq!(
            parse(&args(&["run", "-O", "2", "a.fib", "--", "-O", "7"])),
            run_at("a.fib", false, &["-O", "7"], 2)
        );
    }

    #[test]
    fn the_level_is_a_digit_from_0_to_3_and_given_once() {
        let bad: [&[&str]; 13] = [
            &["build", "a.fib", "-o", "a", "-O"],
            &["build", "a.fib", "-o", "a", "-O", "4"],
            &["build", "a.fib", "-o", "a", "-O", "-1"],
            &["build", "a.fib", "-o", "a", "-O", "+2"],
            &["build", "a.fib", "-o", "a", "-O", "02"],
            &["build", "a.fib", "-o", "a", "-O", "s"],
            &["build", "a.fib", "-o", "a", "-O", ""],
            &["build", "a.fib", "-o", "a", "-O2", "-O", "1"],
            &["run", "-O", "a.fib"],
            &["run", "-O", "4", "a.fib"],
            &["run", "-O2", "-O", "1", "a.fib"],
            &["run", "-O", "2"],
            &["run", "--trace", "--trace", "a.fib"],
        ];
        for line in bad {
            assert_eq!(parse(&args(line)), Command::Invalid, "{line:?}");
        }
        // A last word is the file, as `run --trace` always was.
        assert_eq!(parse(&args(&["run", "-O2"])), run_at("-O2", false, &[], 0));
    }

    #[test]
    fn the_usage_names_the_level_and_its_defaults() {
        assert!(USAGE.contains("run [--trace] [-O N] <file>"));
        assert!(USAGE.contains("build <file> -o <out> [-O N]"));
        assert!(USAGE.contains("build defaults to 2, run to 0"));
    }
}
