//! Command-line entry point for `fibc` (spec/compiler.md §1).
//!
//! Exit codes of `run`: `main`'s result printed and 0; 3 rejected by
//! the front end; 4 the compiler cannot lower the program yet; 5 the
//! compilation failed; a trap aborts (134 under a shell). `cases` exits
//! 0 when no case failed, 1 otherwise, 2 on bad usage.

use std::io::{self, Write};
#[cfg(feature = "llvm")]
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use fibc::compile::{compile, compile_executable};
use fibc::front::{check, Front};
use fibc::harness::child::{EXIT_COMPILE_FAILED, EXIT_REJECTED, EXIT_UNSUPPORTED};
use fibc::harness::gen::GenConfig;
#[cfg(feature = "llvm")]
use fibc::harness::Harness;
#[cfg(feature = "llvm")]
use fibref::cases::{render, Report, Status};
#[cfg(feature = "llvm")]
use lair::{Jit, JitOptions};

const USAGE: &str = "usage: fibc <command>

commands:
  run [--trace] <file> [-- arg..]
                         compile file through the JIT and run its main; the
                         args after -- are (args)
  build <file> -o <out>  compile file to an executable
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

const DEFAULT_CASES_DIR: &str = "cases/ownership";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run {
        file: String,
        trace: bool,
        args: Vec<String>,
    },
    Build {
        file: String,
        out: String,
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

fn parse(args: &[String]) -> Command {
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
        ["build", file, "-o", out] => Command::Build {
            file: file.to_string(),
            out: out.to_string(),
        },
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

fn read(file: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(file).map_err(|e| {
        eprintln!("fibc: cannot read {file}: {e}");
        ExitCode::from(2)
    })
}

/// The front end and the lowering: the lIR text, or the exit code that
/// says why not (its message already printed).
fn lower(file: &str) -> Result<String, ExitCode> {
    lower_kind(file, false)
}

/// [`lower`], for an executable when `executable` (compiler.md §1).
fn lower_kind(file: &str, executable: bool) -> Result<String, ExitCode> {
    let source = read(file)?;
    let checked = match check(&source, file) {
        Front::Checked(c) => c,
        Front::Rejected(m) => {
            eprintln!("rejected:\n{m}");
            return Err(ExitCode::from(EXIT_REJECTED as u8));
        }
        Front::Failed(m) => {
            eprintln!("failed: {m}");
            return Err(ExitCode::from(EXIT_COMPILE_FAILED as u8));
        }
    };
    let lowered = if executable {
        compile_executable(&checked)
    } else {
        compile(&checked)
    };
    lowered.map_err(|u| {
        eprintln!("unsupported: {}", u.0);
        ExitCode::from(EXIT_UNSUPPORTED as u8)
    })
}

#[cfg(not(feature = "llvm"))]
fn no_llvm(what: &str) -> ExitCode {
    eprintln!("fibc: `{what}` needs the llvm feature (this build emits lIR only)");
    ExitCode::from(2)
}

#[cfg(not(feature = "llvm"))]
fn run(_file: &str, _trace: bool, _args: &[String]) -> ExitCode {
    no_llvm("run")
}

#[cfg(not(feature = "llvm"))]
fn build(_file: &str, _out: &str) -> ExitCode {
    no_llvm("build")
}

#[cfg(not(feature = "llvm"))]
fn cases(_dir: &str) -> ExitCode {
    no_llvm("cases")
}

#[cfg(not(feature = "llvm"))]
fn gen(_cfg: GenConfig) -> ExitCode {
    no_llvm("gen")
}

#[cfg(feature = "llvm")]
fn gen(cfg: GenConfig) -> ExitCode {
    let fibc = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fibc: cannot find myself: {e}");
            return ExitCode::from(2);
        }
    };
    let r = match fibc::harness::gen::run(&Harness { fibc }, &cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fibc: cannot write programs to {}: {e}", cfg.dir.display());
            return ExitCode::from(2);
        }
    };
    // The rows of everything that is not a pass, then every count.
    let others: Vec<_> = r
        .report
        .results
        .iter()
        .filter(|c| !matches!(c.status, Status::Pass))
        .cloned()
        .collect();
    let mut text = if others.is_empty() {
        String::new()
    } else {
        render(&Report::from_results(others))
    };
    let c = &r.report.counts;
    text.push_str(&format!(
        "fibc gen: {} programs from seed {}: {} pass, {} fail, {} pending, {} header error; {} model gaps{}\n",
        c.total(),
        cfg.seed,
        c.pass,
        c.fail,
        c.pending,
        c.header_error,
        r.model_gaps.len(),
        if c.total() == c.pass {
            String::new()
        } else {
            format!("; the programs kept in {}", cfg.dir.display())
        }
    ));
    let code = if r.report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    match write_stdout(&text) {
        ExitCode::SUCCESS => code,
        other => other,
    }
}

#[cfg(feature = "llvm")]
fn run(file: &str, trace: bool, args: &[String]) -> ExitCode {
    let lir = match lower(file) {
        Ok(l) => l,
        Err(code) => return code,
    };
    if trace {
        std::env::set_var("FIB_TRACE", "1");
    }
    let fail = |e: lair::Error| {
        eprintln!("compile failed: {}", e.render(file));
        ExitCode::from(EXIT_COMPILE_FAILED as u8)
    };
    let module = match lair::for_executable(&lir) {
        Ok(m) => m,
        Err(e) => return fail(e),
    };
    let mut jit = match Jit::new(JitOptions::default()) {
        Ok(j) => j,
        Err(e) => return fail(e),
    };
    if let Err(e) = jit.add_module(file, &module) {
        return fail(e);
    }
    // The command line main receives: the file, then the arguments.
    let argv: Vec<std::ffi::CString> = std::iter::once(file)
        .chain(args.iter().map(String::as_str))
        .map(|a| std::ffi::CString::new(a).unwrap_or_default())
        .collect();
    let ptrs: Vec<*const std::ffi::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    // SAFETY: `compile` defined main as (main i32) ((i32 argc) (ptr
    // argv)) (spec/lir.md §7.2), argv holds argc valid C strings that
    // outlive the call, and the JIT outlives it too.
    let code = unsafe {
        match jit.function::<extern "C" fn(i32, *const *const std::ffi::c_char) -> i32>("main") {
            Ok(f) => f(ptrs.len() as i32, ptrs.as_ptr()),
            Err(e) => return fail(e),
        }
    };
    std::process::exit(code)
}

#[cfg(feature = "llvm")]
fn build(file: &str, out: &str) -> ExitCode {
    let lir = match lower_kind(file, true) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let fail = |e: lair::Error| {
        eprintln!("compile failed: {}", e.render(file));
        ExitCode::from(EXIT_COMPILE_FAILED as u8)
    };
    let module = match lair::for_executable(&lir) {
        Ok(m) => m,
        Err(e) => return fail(e),
    };
    let opts = lair::aot::Options {
        libs: vec!["pthread".into()],
        ..lair::aot::Options::default()
    };
    match lair::aot::build_executable(&module, file, Path::new(out), &opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

fn emit(file: &str) -> ExitCode {
    match lower(file) {
        Ok(l) => write_stdout(&l),
        Err(code) => code,
    }
}

fn explain(file: &str) -> ExitCode {
    let source = match read(file) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match check(&source, file) {
        Front::Checked(c) => write_stdout(&fibref::own::explain::explain(&c.typed, &c.owned)),
        Front::Rejected(m) => {
            eprintln!("rejected:\n{m}");
            ExitCode::from(EXIT_REJECTED as u8)
        }
        Front::Failed(m) => {
            eprintln!("failed: {m}");
            ExitCode::from(EXIT_COMPILE_FAILED as u8)
        }
    }
}

fn itrace(file: &str) -> ExitCode {
    let source = match read(file) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let run = fibc::harness::interp::run(&source, file);
    eprintln!("{:?}", run.outcome);
    write_stdout(&run.trace.render())
}

#[cfg(feature = "llvm")]
fn cases(dir: &str) -> ExitCode {
    let fibc = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fibc: cannot find myself: {e}");
            return ExitCode::from(2);
        }
    };
    let report = match (Harness { fibc }).run_dir(Path::new(dir)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fibc: cannot read cases in {dir}: {e}");
            return ExitCode::from(2);
        }
    };
    if report.counts.total() == 0 {
        eprintln!("fibc: no case files in {dir}; a run with nothing to run is not a pass");
    }
    let code = if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    match write_stdout(&render(&report)) {
        ExitCode::SUCCESS => code,
        other => other,
    }
}

fn write_stdout(text: &str) -> ExitCode {
    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fibc: cannot write: {e}");
            ExitCode::from(2)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Command::Run { file, trace, args } => run(&file, trace, &args),
        Command::Build { file, out } => build(&file, &out),
        Command::Emit { file } => emit(&file),
        Command::Explain { file } => explain(&file),
        Command::Itrace { file } => itrace(&file),
        Command::Cases { dir } => cases(&dir),
        Command::Gen(cfg) => gen(cfg),
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
            Command::Build {
                file: "a.fib".into(),
                out: "a".into()
            }
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
}
