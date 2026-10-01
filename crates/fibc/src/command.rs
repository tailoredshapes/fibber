//! The command line of `fibc` (spec/compiler.md §1): the usage text and
//! the parse of the arguments into a [`Command`]. Nothing here runs a
//! command; `main.rs` does.

use std::path::PathBuf;

use fibc::harness::gen::GenConfig;

pub const USAGE: &str = "usage: fibc <command>

commands:
  run [--trace] <file> [-- arg..]
                         compile file through the JIT and run its main; the
                         args after -- are (args), a byte that is not UTF-8 in
                         one becoming U+FFFD as Rust's from_utf8_lossy makes it
  build <file> -o <out> [-L dir].. [-l lib]..
                         compile file to an executable; each -l lib links
                         -llib, each -L dir is searched for libraries and
                         becomes an rpath (as an absolute path), so the
                         executable finds its libraries without
                         LD_LIBRARY_PATH (-Ldir and -llib are also taken)
  emit <file>            print the lIR module of file
  explain <file>         print the ownership checker's decisions (types §9)
  itrace <file>          run file in the reference interpreter and print its
                         canonical trace (compiler.md §4)
  cases [dir]            run every case in dir (default cases/ownership)
                         interpreted and compiled, and compare (method.md rule 6)
  gen --seed S --count N [--size K] [--jobs J] [--dir D]
                         generate N programs with fibgen from seed S (size K, or
                         sizes 1..=6 in turn) and run each interpreted and
                         compiled against the model's verdict; a program that
                         does not pass is kept in D. J defaults to at most 8:
                         a job holds a heap trace and a JIT-compiling child
  help                   print this message";

pub const DEFAULT_CASES_DIR: &str = "cases/ownership";

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
    },
    Build {
        file: String,
        out: String,
        link: Link,
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
/// `-l LIB` or `-lLIB`, `-o OUT`. `None` when the value is missing or
/// empty.
fn flag_value<'a>(
    flag: &'a str,
    rest: &mut impl Iterator<Item = &'a &'a str>,
) -> Option<(&'a str, &'a str)> {
    let glued = match flag.get(..2) {
        Some(name @ ("-L" | "-l")) if flag.len() > 2 => Some((name, &flag[2..])),
        _ => None,
    };
    let (name, value) = match glued {
        Some(pair) => pair,
        None => (flag, *rest.next()?),
    };
    (!value.is_empty()).then_some((name, value))
}

/// `build FILE -o OUT` and any number of `-L DIR` and `-l LIB`, in any
/// order after the file; `-o` exactly once. Anything else is `Invalid`.
fn parse_build(file: &str, rest: &[&str]) -> Command {
    let (mut out, mut link) = (None, Link::default());
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let Some((name, value)) = flag_value(flag, &mut it) else {
            return Command::Invalid;
        };
        match name {
            "-o" if out.is_none() => out = Some(value),
            "-L" => link.dirs.push(value.to_string()),
            "-l" => link.libs.push(value.to_string()),
            _ => return Command::Invalid,
        }
    }
    match out {
        Some(out) => Command::Build {
            file: file.to_string(),
            out: out.to_string(),
            link,
        },
        None => Command::Invalid,
    }
}

pub fn parse(args: &[String]) -> Command {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["run", file] => Command::Run {
            file: file.to_string(),
            trace: false,
            args: Vec::new(),
        },
        ["run", "--trace", file] => Command::Run {
            file: file.to_string(),
            trace: true,
            args: Vec::new(),
        },
        ["run", file, "--", rest @ ..] => Command::Run {
            file: file.to_string(),
            trace: false,
            args: rest.iter().map(|s| s.to_string()).collect(),
        },
        ["run", "--trace", file, "--", rest @ ..] => Command::Run {
            file: file.to_string(),
            trace: true,
            args: rest.iter().map(|s| s.to_string()).collect(),
        },
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
        },
        ["cases", dir] => Command::Cases {
            dir: dir.to_string(),
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
        Command::Build {
            file: file.into(),
            out: out.into(),
            link: Link {
                dirs: dirs.iter().map(|s| s.to_string()).collect(),
                libs: libs.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    #[test]
    fn parses_each_command() {
        assert_eq!(
            parse(&args(&["run", "--trace", "a.fib"])),
            Command::Run {
                file: "a.fib".into(),
                trace: true,
                args: Vec::new()
            }
        );
        assert_eq!(
            parse(&args(&["run", "a.fib", "--", "x", "y"])),
            Command::Run {
                file: "a.fib".into(),
                trace: false,
                args: vec!["x".into(), "y".into()]
            }
        );
        assert_eq!(
            parse(&args(&["build", "a.fib", "-o", "a"])),
            build("a.fib", "a", &[], &[])
        );
        assert_eq!(
            parse(&args(&["cases"])),
            Command::Cases {
                dir: DEFAULT_CASES_DIR.into()
            }
        );
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
}
