//! The command line.

use std::path::PathBuf;

use crate::runner::{Tool, ToolKind};
use crate::session::Config;

pub const USAGE: &str = "usage: fibmut --module FILE [options]

Mutates one fibber library module and runs the cases that exercise it
against each mutant; a mutant no case kills is a case the library lacks.
Works on a copy of --lib and --cases in a temporary directory; nothing in
the repository is written.

options:
  --module FILE     the module to mutate (required), a file under --lib
  --lib DIR         the library root (default lib)
  --cases DIR       the cases (default cases/stdlib); a case's own `roots`
                    header must name the library relative to the case
  --only PATTERNS   cases to judge with, comma separated; * and ? match, a
                    pattern with neither is a prefix (default: every case)
  --max N           run at most N mutants, a seeded sample (default 150)
  --seed N          the seed of the sample (default 1)
  --timeout SECS    the limit for one process (default 20)
  --tool NAME       fibref or fibc (default fibref)
  --bin PATH        the tool's executable (default: its name, on PATH)
  --vmem KB         address space limit per process, 0 for none (default 4000000)
  --ops LIST        only these operators, comma separated:
                    cmp arith bool const branch clause swap stmt exit
  --lines A[-B]     only the mutants on these lines of the module
  --out DIR         write each survivor's diff and the report there
  --verbose         one line for every mutant in the report
  --list            list the chosen mutants and stop; runs nothing
  --fail-on-survivor  exit 1 when a mutant survives
  --help            this text

Exit status: 0 the review ran, 1 it ran and --fail-on-survivor saw a
survivor, 2 a bad command line or a run that could not be made.";

/// A parsed command line.
pub struct Args {
    pub config: Config,
    pub out: Option<PathBuf>,
    pub verbose: bool,
    pub list: bool,
    pub fail_on_survivor: bool,
}

pub enum Parsed {
    Help,
    Run(Box<Args>),
}

fn number<T: std::str::FromStr>(flag: &str, text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{flag}: {text} is not a number"))
}

fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn lines(text: &str) -> Result<(usize, usize), String> {
    match text.split_once('-') {
        Some((a, b)) => Ok((number("--lines", a)?, number("--lines", b)?)),
        None => number("--lines", text).map(|n| (n, n)),
    }
}

/// The options as they are read, before the checks that need them all.
struct Draft {
    config: Config,
    module: Option<PathBuf>,
    bin: Option<PathBuf>,
    out: Option<PathBuf>,
}

impl Draft {
    fn new() -> Draft {
        let tool = Tool {
            kind: ToolKind::Fibref,
            program: PathBuf::new(),
            vmem_kb: 4_000_000,
            timeout_s: 20.0,
        };
        let config = Config {
            module: PathBuf::new(),
            lib: PathBuf::from("lib"),
            cases: PathBuf::from("cases/stdlib"),
            only: Vec::new(),
            max: 150,
            seed: 1,
            ops: Vec::new(),
            lines: None,
            tool,
        };
        Draft {
            config,
            module: None,
            bin: None,
            out: None,
        }
    }

    /// An option that takes a value.
    fn set(&mut self, flag: &str, value: &str) -> Result<(), String> {
        let c = &mut self.config;
        match flag {
            "--module" => self.module = Some(PathBuf::from(value)),
            "--lib" => c.lib = PathBuf::from(value),
            "--cases" => c.cases = PathBuf::from(value),
            "--only" => c.only.extend(list(value)),
            "--max" => c.max = number(flag, value)?,
            "--seed" => c.seed = number(flag, value)?,
            "--timeout" => c.tool.timeout_s = number(flag, value)?,
            "--vmem" => c.tool.vmem_kb = number(flag, value)?,
            "--ops" => c.ops.extend(list(value)),
            "--lines" => c.lines = Some(lines(value)?),
            "--bin" => self.bin = Some(PathBuf::from(value)),
            "--out" => self.out = Some(PathBuf::from(value)),
            "--tool" => c.tool.kind = tool(value)?,
            _ => return Err(format!("unknown option {flag}")),
        }
        Ok(())
    }
}

/// Parses the arguments after the program name.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut draft = Draft::new();
    let (mut verbose, mut list_only, mut fail) = (false, false, false);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--verbose" => verbose = true,
            "--list" => list_only = true,
            "--fail-on-survivor" => fail = true,
            _ => {
                let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
                draft.set(flag, value)?;
            }
        }
    }
    let mut config = draft.config;
    config.module = draft.module.ok_or("--module is required")?;
    config.tool.program = draft
        .bin
        .unwrap_or_else(|| PathBuf::from(config.tool.kind.name()));
    if config.tool.timeout_s <= 0.0 {
        return Err("--timeout must be above zero".to_string());
    }
    let out = draft.out;
    Ok(Parsed::Run(Box::new(Args {
        config,
        out,
        verbose,
        list: list_only,
        fail_on_survivor: fail,
    })))
}

fn tool(name: &str) -> Result<ToolKind, String> {
    match name {
        "fibref" => Ok(ToolKind::Fibref),
        "fibc" => Ok(ToolKind::Fibc),
        _ => Err(format!("--tool is fibref or fibc, not {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(args: &[&str]) -> Args {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&args) {
            Ok(Parsed::Run(a)) => *a,
            _ => panic!("expected a run"),
        }
    }

    fn parse_err(args: &[&str]) -> String {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&args) {
            Err(e) => e,
            _ => panic!("expected an error"),
        }
    }

    #[test]
    fn the_defaults_are_the_plans() {
        let a = parse_ok(&["--module", "lib/fib/seq/protocols.fib"]);
        let c = &a.config;
        assert_eq!(
            (c.max, c.seed, c.tool.timeout_s, c.tool.vmem_kb),
            (150, 1, 20.0, 4_000_000)
        );
        assert_eq!(c.cases, PathBuf::from("cases/stdlib"));
        assert_eq!(c.tool.program, PathBuf::from("fibref"));
        assert!(!a.verbose && !a.list && !a.fail_on_survivor && a.out.is_none());
    }

    #[test]
    fn every_option_that_takes_a_value_is_read() {
        let a = parse_ok(&[
            "--module",
            "m.fib",
            "--lib",
            "l",
            "--cases",
            "c",
            "--only",
            "240-*,250-*",
            "--only",
            "ref-",
            "--max",
            "9",
            "--seed",
            "4",
            "--timeout",
            "1.5",
            "--tool",
            "fibc",
            "--bin",
            "/x/fibc",
            "--vmem",
            "0",
            "--ops",
            "cmp,const",
            "--lines",
            "10-20",
            "--out",
            "o",
        ]);
        let c = &a.config;
        assert_eq!(
            (c.module.clone(), c.lib.clone(), c.cases.clone()),
            ("m.fib".into(), "l".into(), "c".into())
        );
        assert_eq!(c.only, vec!["240-*", "250-*", "ref-"]);
        assert_eq!(
            (c.max, c.seed, c.tool.timeout_s, c.tool.vmem_kb),
            (9, 4, 1.5, 0)
        );
        assert_eq!(
            (c.tool.kind, c.tool.program.clone()),
            (ToolKind::Fibc, PathBuf::from("/x/fibc"))
        );
        assert_eq!(
            (c.ops.clone(), c.lines),
            (vec!["cmp".to_string(), "const".to_string()], Some((10, 20)))
        );
        assert_eq!(a.out, Some(PathBuf::from("o")));
    }

    #[test]
    fn the_switches_and_a_single_line_are_read() {
        let a = parse_ok(&["--module", "m", "--verbose", "--list", "--fail-on-survivor"]);
        assert!(a.verbose && a.list && a.fail_on_survivor);
        let b = parse_ok(&[
            "--module",
            "m",
            "--ops",
            "cmp,const",
            "--lines",
            "10-20",
            "--out",
            "o",
        ]);
        assert_eq!(
            (b.config.ops.clone(), b.config.lines),
            (vec!["cmp".to_string(), "const".to_string()], Some((10, 20)))
        );
        assert_eq!(b.out, Some(PathBuf::from("o")));
        assert_eq!(
            parse_ok(&["--module", "m", "--lines", "7"]).config.lines,
            Some((7, 7))
        );
    }

    #[test]
    fn a_bad_command_line_says_what_is_wrong() {
        assert_eq!(parse_err(&[]), "--module is required");
        assert_eq!(parse_err(&["--module"]), "--module needs a value");
        assert_eq!(
            parse_err(&["--module", "m", "--max", "x"]),
            "--max: x is not a number"
        );
        assert_eq!(
            parse_err(&["--module", "m", "--tool", "gcc"]),
            "--tool is fibref or fibc, not gcc"
        );
        assert_eq!(
            parse_err(&["--module", "m", "--bogus", "1"]),
            "unknown option --bogus"
        );
        assert_eq!(
            parse_err(&["--module", "m", "--timeout", "0"]),
            "--timeout must be above zero"
        );
        assert!(matches!(parse(&["--help".to_string()]), Ok(Parsed::Help)));
    }
}
