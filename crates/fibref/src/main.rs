//! Command-line entry point for `fibref`.
//!
//! Exit codes, so that CI can tell the states apart (`spec/method.md`):
//! `0` no case failed and every header parsed, `1` at least one case
//! failed or had a bad header, `2` bad usage or an unreadable directory.
//! Pending cases do not fail the exit code, but they are printed
//! prominently and are never counted as passes.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use fibref::cases::{render, run_dir, PendingEvaluator};

const USAGE: &str = "usage: fibref <command>

commands:
  cases [dir]   run every case in dir (default cases/ownership) against
                the verdict in its header
  help          print this message";

/// The directory `cases` runs when none is given.
const DEFAULT_CASES_DIR: &str = "cases/ownership";

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Run the cases in a directory.
    Cases { dir: String },
    /// Print usage and exit successfully.
    Help,
    /// Print usage and exit with an error: the arguments made no sense.
    Invalid,
}

/// Parses the arguments after the program name.
fn parse(args: &[String]) -> Command {
    match args {
        [cmd] if cmd == "cases" => Command::Cases {
            dir: DEFAULT_CASES_DIR.to_string(),
        },
        [cmd, dir] if cmd == "cases" => Command::Cases { dir: dir.clone() },
        [cmd] if cmd == "help" || cmd == "--help" || cmd == "-h" => Command::Help,
        _ => Command::Invalid,
    }
}

/// Runs the cases in `dir` through the current evaluator and prints the report.
fn run_cases(dir: &str) -> ExitCode {
    let report = match run_dir(Path::new(dir), &PendingEvaluator) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("fibref: cannot read cases in {dir}: {e}");
            return ExitCode::from(2);
        }
    };
    if report.counts.total() == 0 {
        eprintln!("fibref: no case files in {dir}; a run with nothing to run is not a pass");
    }
    let verdict = if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    match write_stdout(&render(&report)) {
        Ok(()) => verdict,
        // The reader went away (`fibref cases | head`): the report was
        // computed, so its exit code stands and nothing is said.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => verdict,
        Err(e) => {
            eprintln!("fibref: cannot write the report: {e}");
            ExitCode::from(2)
        }
    }
}

/// Writes `text` to stdout and returns the error instead of panicking
/// the way `print!` does on a closed pipe.
fn write_stdout(text: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(text.as_bytes())?;
    out.flush()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Command::Cases { dir } => run_cases(&dir),
        Command::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Invalid => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cases_takes_one_directory() {
        assert_eq!(
            parse(&args(&["cases", "cases/other"])),
            Command::Cases {
                dir: "cases/other".to_string()
            }
        );
    }

    #[test]
    fn cases_without_a_directory_uses_the_default() {
        assert_eq!(
            parse(&args(&["cases"])),
            Command::Cases {
                dir: "cases/ownership".to_string()
            }
        );
    }

    #[test]
    fn cases_with_extra_arguments_is_invalid() {
        assert_eq!(parse(&args(&["cases", "a", "b"])), Command::Invalid);
    }

    #[test]
    fn no_arguments_is_invalid() {
        assert_eq!(parse(&args(&[])), Command::Invalid);
    }

    #[test]
    fn unknown_command_is_invalid() {
        assert_eq!(
            parse(&args(&["bogus", "cases/ownership"])),
            Command::Invalid
        );
    }

    #[test]
    fn help_spellings_are_help() {
        for spelling in ["help", "--help", "-h"] {
            assert_eq!(parse(&args(&[spelling])), Command::Help, "{spelling}");
        }
    }
}
