//! Command-line entry point for `fibref`.
//!
//! Exit codes, so that CI can tell the states apart (`spec/method.md`):
//! `0` no case failed and every header parsed, `1` at least one case
//! failed or had a bad header, `2` bad usage or an unreadable directory.
//! Pending cases do not fail the exit code, but they are printed
//! prominently and are never counted as passes.

#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use fibref::cases::{render, run_dir, Outcome};
use fibref::eval::{run_source_with, Interpreter};

const USAGE: &str = "usage: fibref <command>

commands:
  cases [dir]     run every case in dir (default cases/ownership) against
                  the verdict in its header
  explain <file>  print the ownership checker's decisions for file (types §9)
  run <file>      run file's main with the reference interpreter and print
                  its result and the memory audit (exit 1 if rejected or failed)
  read [--print] <file>..
                  print the reader's dump of each file (spec/bootstrap.md §2),
                  or with --print each top-level form as the printer writes
                  it, one per line; exit 1 if any file does not read, 2 if
                  one cannot be read
  help            print this message";

/// The directory `cases` runs when none is given.
const DEFAULT_CASES_DIR: &str = "cases/ownership";

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Run the cases in a directory.
    Cases { dir: String },
    /// Print the ownership decisions for a file.
    Explain { file: String },
    /// Run a file's `main`.
    Run { file: String, args: Vec<String> },
    /// Print the reader's dump of each file, or (`print`) its forms as
    /// the printer writes them.
    Read { files: Vec<String>, print: bool },
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
        [cmd, file] if cmd == "explain" => Command::Explain { file: file.clone() },
        [cmd, file] if cmd == "run" => Command::Run {
            file: file.clone(),
            args: Vec::new(),
        },
        [cmd, file, dashes, rest @ ..] if cmd == "run" && dashes == "--" => Command::Run {
            file: file.clone(),
            args: rest.to_vec(),
        },
        [cmd, flag, files @ ..] if cmd == "read" && flag == "--print" && !files.is_empty() => {
            Command::Read {
                files: files.to_vec(),
                print: true,
            }
        }
        [cmd, files @ ..] if cmd == "read" && !files.is_empty() && files[0] != "--print" => {
            Command::Read {
                files: files.to_vec(),
                print: false,
            }
        }
        [cmd] if cmd == "help" || cmd == "--help" || cmd == "-h" => Command::Help,
        _ => Command::Invalid,
    }
}

/// Runs the cases in `dir` through the current evaluator and prints the report.
fn run_cases(dir: &str) -> ExitCode {
    let report = match run_dir(Path::new(dir), &Interpreter) {
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

/// Checks `file` through the ownership pass and prints its decisions,
/// or its errors (exit 1); exit 2 if it cannot be read.
fn run_explain(file: &str) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fibref: cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let (text, code) = match fibref::own::check_source(&source, file) {
        Ok(c) => (
            fibref::own::explain::explain(&c.typed, &c.owned),
            ExitCode::SUCCESS,
        ),
        Err(e) => (format!("rejected:\n{e}\n"), ExitCode::from(1)),
    };
    match write_stdout(&text) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => ExitCode::from(2),
        _ => code,
    }
}

/// Runs `file` through the whole pipeline and prints `main`'s result and
/// the audit: exit 0 if it ran (whatever the audit says, which is
/// printed), 1 if it was rejected or its run failed, 2 if unreadable.
fn run_file(file: &str, args: &[String]) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fibref: cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let (text, code) = match run_source_with(&source, file, args) {
        Outcome::Compiled { result, audit } => (
            format!("result: {result}\naudit:  {audit}\n"),
            ExitCode::SUCCESS,
        ),
        Outcome::Rejected { message } => (format!("rejected:\n{message}\n"), ExitCode::from(1)),
        Outcome::Trapped { message, errors } => {
            let audit = if errors.is_empty() {
                "clean at the abort".to_string()
            } else {
                errors.join("; ")
            };
            let text = format!("trapped:\n{message}\naudit:  {audit}\n");
            (text, ExitCode::from(1))
        }
        Outcome::Failed { message } => (format!("failed:\n{message}\n"), ExitCode::from(1)),
        Outcome::Unsupported { reason } => (format!("unsupported: {reason}\n"), ExitCode::from(1)),
    };
    match write_stdout(&text) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => ExitCode::from(2),
        _ => code,
    }
}

/// Prints the reader's dump of each file, or with `print` its forms as the
/// printer writes them (`dump::print_source`), headed by `== file`: exit 1
/// if a file does not read, 2 if one cannot be read (its dump is the line
/// `unreadable`).
fn read_files(files: &[String], print: bool) -> ExitCode {
    let mut text = String::new();
    let mut status = 0u8;
    for file in files {
        text.push_str(&format!("== {file}\n"));
        match std::fs::read_to_string(file) {
            Ok(source) => {
                let dump = if print {
                    fibref::dump::print_source(&source, file)
                } else {
                    fibref::dump::dump_source(&source, file)
                };
                if dump.starts_with("error ") {
                    status = status.max(1);
                }
                text.push_str(&dump);
            }
            Err(_) => {
                text.push_str("unreadable\n");
                status = 2;
            }
        }
    }
    match write_stdout(&text) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => ExitCode::from(2),
        _ => ExitCode::from(status),
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
        Command::Explain { file } => run_explain(&file),
        Command::Run { file, args } => run_file(&file, &args),
        Command::Read { files, print } => read_files(&files, print),
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
    fn explain_takes_one_file() {
        assert_eq!(
            parse(&args(&["explain", "a.fib"])),
            Command::Explain {
                file: "a.fib".to_string()
            }
        );
        assert_eq!(parse(&args(&["explain"])), Command::Invalid);
    }

    #[test]
    fn run_takes_one_file() {
        assert_eq!(
            parse(&args(&["run", "a.fib"])),
            Command::Run {
                file: "a.fib".to_string(),
                args: Vec::new()
            }
        );
        assert_eq!(
            parse(&args(&["run", "a.fib", "--", "x"])),
            Command::Run {
                file: "a.fib".to_string(),
                args: vec!["x".to_string()]
            }
        );
        assert_eq!(parse(&args(&["run"])), Command::Invalid);
    }

    #[test]
    fn read_takes_one_or_more_files() {
        assert_eq!(
            parse(&args(&["read", "a.fib", "b.fib"])),
            Command::Read {
                files: vec!["a.fib".to_string(), "b.fib".to_string()],
                print: false
            }
        );
        assert_eq!(parse(&args(&["read"])), Command::Invalid);
    }

    #[test]
    fn read_print_is_a_flag_before_the_files() {
        assert_eq!(
            parse(&args(&["read", "--print", "a.fib", "b.fib"])),
            Command::Read {
                files: vec!["a.fib".to_string(), "b.fib".to_string()],
                print: true
            }
        );
        // The flag alone names no file; after a file it is a file name.
        assert_eq!(parse(&args(&["read", "--print"])), Command::Invalid);
        assert_eq!(
            parse(&args(&["read", "a.fib", "--print"])),
            Command::Read {
                files: vec!["a.fib".to_string(), "--print".to_string()],
                print: false
            }
        );
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
