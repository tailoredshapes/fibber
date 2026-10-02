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

use fibref::cases::{render, run_dir, run_dir_only, Outcome, SelectError};
use fibref::eval::{run_source_in, Interpreter};
use fibref::expand_dump::Options;
use fibref::roots::Roots;

const USAGE: &str = "usage: fibref <command>

commands:
  cases [dir [--only prefix..]]
                  run every case in dir (default cases/ownership) against
                  the verdict in its header; with --only, the cases whose
                  names start with one of the prefixes (a prefix that
                  matches no case is an error, exit 2). A case with an
                  `open` label that fails as it says is OPEN: listed, not
                  a failure (cases/stdlib/README.md)
  explain [-I dir].. <file>
                  print the ownership checker's decisions for file (types §9)
  run [-I dir].. <file> [-- arg..]
                  run file's main with the reference interpreter and print
                  its result and the memory audit (exit 1 if rejected or failed);
                  the args after -- are (args), a byte that is not UTF-8 in
                  one becoming U+FFFD as Rust's from_utf8_lossy makes it;
                  a module is found beside file, then under each -I dir in
                  order, then under each directory of $FIB_LIB, then in the
                  library the executable carries (spec/syntax.md §5)
  read [--print] <file>..
                  print the reader's dump of each file (spec/bootstrap.md §2),
                  or with --print each top-level form as the printer writes
                  it, one per line; exit 1 if any file does not read, 2 if
                  one cannot be read
  expand [options] <file>..
                  print the dump of each program after expansion, module by
                  module (spec/bootstrap.md §5); exit 1 if a program does not
                  read, load or expand, 2 if a file cannot be read. Options,
                  before the files: --prelude (each file is a library
                  prelude), --context (print what the context holds after
                  each module), --implicit (print the sections of the
                  implicit modules too), --implicit-lib A,B (the implicit
                  modules of this dump), --no-runner (a user macro call is
                  pending), --max-steps N, --max-depth N, --max-forms N
                  (smaller limits)
  types [options] <file>..
                  print the dump of each program after type checking, module
                  by module (spec/bootstrap.md §6); exit 1 if a program does
                  not read, load, expand or type, 2 if a file cannot be
                  read. Options, before the files: --stage lower|infer,
                  --sections A,B (type, protocol, instance, fun, def,
                  extern, unit, error, ast, tables), --library (no main
                  needed), --prelude (each file is a library prelude),
                  --implicit, --implicit-lib A,B, --ast, --tables
  own [OPTION..] <file>..
                  print the ownership decisions of each file (the dump
                  of spec/bootstrap.md section 7). Options, before the
                  files: --sections A,B (body, facts, summary, taken,
                  error, explain), --library, --prelude, --implicit,
                  --implicit-lib A,B
  help           print this message";

/// The directory `cases` runs when none is given.
const DEFAULT_CASES_DIR: &str = "cases/ownership";

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Run the cases in a directory.
    Cases { dir: String, only: Vec<String> },
    /// Print the ownership decisions for a file.
    Explain { file: String },
    /// Run a file's `main`.
    Run { file: String, args: Vec<String> },
    /// Print the reader's dump of each file, or (`print`) its forms as
    /// the printer writes them.
    Read { files: Vec<String>, print: bool },
    /// Print the expansion dump of each file (`fibref::expand_dump`).
    Expand { files: Vec<String>, opts: Options },
    /// Print the types dump of each file (`fibref::types_dump`).
    Types {
        files: Vec<String>,
        opts: fibref::types_dump::Options,
    },
    /// Print the ownership dump of each file (`fibref::own_dump`).
    Own {
        files: Vec<String>,
        opts: fibref::own_dump::Options,
    },
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
            only: Vec::new(),
        },
        [cmd, dir] if cmd == "cases" => Command::Cases {
            dir: dir.clone(),
            only: Vec::new(),
        },
        [cmd, dir, flag, prefixes @ ..]
            if cmd == "cases" && flag == "--only" && !prefixes.is_empty() =>
        {
            Command::Cases {
                dir: dir.clone(),
                only: prefixes.to_vec(),
            }
        }
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
        [cmd, rest @ ..] if cmd == "expand" => match fibref::expand_dump::parse_args(rest) {
            Some((opts, files)) => Command::Expand { files, opts },
            None => Command::Invalid,
        },
        [cmd, rest @ ..] if cmd == "types" => match fibref::types_dump::parse_args(rest) {
            Some((opts, files)) => Command::Types { files, opts },
            None => Command::Invalid,
        },
        [cmd, rest @ ..] if cmd == "own" => match fibref::own_dump::parse_args(rest) {
            Some((opts, files)) => Command::Own { files, opts },
            None => Command::Invalid,
        },
        [cmd] if cmd == "help" || cmd == "--help" || cmd == "-h" => Command::Help,
        _ => Command::Invalid,
    }
}

/// Runs the cases in `dir` through the current evaluator and prints the report.
fn run_cases(dir: &str, only: &[String]) -> ExitCode {
    let ran = if only.is_empty() {
        run_dir(Path::new(dir), &Interpreter).map_err(SelectError::Io)
    } else {
        run_dir_only(Path::new(dir), &Interpreter, only)
    };
    let report = match ran {
        Ok(report) => report,
        Err(SelectError::NoMatch(prefix)) => {
            eprintln!("fibref: no case matches {prefix} in {dir}");
            return ExitCode::from(2);
        }
        Err(SelectError::Io(e)) => {
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
    finish(&render(&report), verdict)
}

/// Checks `file` through the ownership pass and prints its decisions,
/// or its errors (exit 1); exit 2 if it cannot be read.
fn run_explain(file: &str, roots: &Roots) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fibref: cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let (text, code) = match fibref::own::check_source_in(&source, file, roots) {
        Ok(c) => (
            fibref::own::explain::explain(&c.typed, &c.owned),
            ExitCode::SUCCESS,
        ),
        Err(e) => (format!("rejected:\n{e}\n"), ExitCode::from(1)),
    };
    finish(&text, code)
}

/// Runs `file` through the whole pipeline and prints `main`'s result and
/// the audit: exit 0 if it ran (whatever the audit says, which is
/// printed), 1 if it was rejected or its run failed, 2 if unreadable.
fn run_file(file: &str, args: &[String], roots: &Roots) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fibref: cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let (text, code) = match run_source_in(&source, file, args, roots).0 {
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
    finish(&text, code)
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
    finish(&text, ExitCode::from(status))
}

/// Prints the expansion dump of each file (`fibref::expand_dump`): exit 1
/// if a program ends in an error record, 2 if a file cannot be read.
fn expand_files(files: &[String], opts: &Options) -> ExitCode {
    let (text, status) = fibref::expand_dump::expand_files(files, opts);
    finish(&text, ExitCode::from(status))
}

/// Prints the types dump of each file (`fibref::types_dump`): exit 1 if
/// a program ends in an error record, 2 if a file cannot be read.
fn types_files(files: &[String], opts: &fibref::types_dump::Options) -> ExitCode {
    let (text, status) = fibref::types_dump::types_files(files, opts);
    finish(&text, ExitCode::from(status))
}

/// Prints the ownership dump of each file (`fibref::own_dump`): exit 1 if
/// a program ends in an error record, 2 if a file cannot be read.
fn own_files(files: &[String], opts: &fibref::own_dump::Options) -> ExitCode {
    let (text, status) = fibref::own_dump::own_files(files, opts);
    finish(&text, ExitCode::from(status))
}

/// Prints `text` to stdout and returns `code`, the verdict of the
/// command, unless the report could not be written: then it says why on
/// stderr and exits 2, so that a full device is not a pass. A reader that
/// went away (`fibref cases | head`) is not a failure: the report was
/// computed, so its exit code stands and nothing is said.
fn finish(text: &str, code: ExitCode) -> ExitCode {
    match write_stdout(text) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => code,
        Err(e) => {
            eprintln!("fibref: cannot write the report: {e}");
            ExitCode::from(2)
        }
        Ok(()) => code,
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
    let args = match fibref::cmdline::from_os(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(word) => {
            eprintln!(
                "fibref: an argument before `--` is not UTF-8: {}",
                word.to_string_lossy()
            );
            return ExitCode::from(2);
        }
    };
    let (dirs, args) = fibref::cmdline::split_roots(&args);
    let command = parse(&args);
    if !dirs.is_empty() && !matches!(command, Command::Run { .. } | Command::Explain { .. }) {
        eprintln!("fibref: -I belongs to `run` and `explain`\n{USAGE}");
        return ExitCode::from(2);
    }
    let roots = Roots::from_env(&dirs, std::env::var_os("FIB_LIB").as_deref());
    match command {
        Command::Cases { dir, only } => run_cases(&dir, &only),
        Command::Explain { file } => run_explain(&file, &roots),
        Command::Run { file, args } => run_file(&file, &args, &roots),
        Command::Read { files, print } => read_files(&files, print),
        Command::Expand { files, opts } => expand_files(&files, &opts),
        Command::Types { files, opts } => types_files(&files, &opts),
        Command::Own { files, opts } => own_files(&files, &opts),
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
mod tests;
