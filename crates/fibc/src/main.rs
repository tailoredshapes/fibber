//! Command-line entry point for `fibc` (spec/compiler.md §1).
//!
//! Exit codes of `run`: `main`'s result printed and 0; 3 rejected by
//! the front end; 4 the compiler cannot lower the program yet; 5 the
//! compilation failed; a trap aborts (134 under a shell). `build` exits
//! with the same codes (5 includes a link that failed) and 2 for a `-L`
//! directory it cannot use. `cases` exits 0 when no case failed, 1
//! otherwise, 2 on bad usage.

mod command;
#[cfg(feature = "llvm")]
mod link;

use std::io::{self, Write};
#[cfg(feature = "llvm")]
use std::path::Path;
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

use command::{parse, Command, Link, USAGE};

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
fn build(_file: &str, _out: &str, _link: &Link) -> ExitCode {
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

/// `build`: the executable `out`, linked against `link`'s libraries.
/// A `-L` directory that cannot be an rpath is refused first (exit 2).
#[cfg(feature = "llvm")]
fn build(file: &str, out: &str, link: &Link) -> ExitCode {
    let dirs: Result<Vec<_>, String> = link.dirs.iter().map(|d| link::library_dir(d)).collect();
    let dirs = match dirs {
        Ok(dirs) => dirs,
        Err(message) => {
            eprintln!("fibc: {message}");
            return ExitCode::from(2);
        }
    };
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
    match link::build_executable(&module, file, Path::new(out), &dirs, &link.libs) {
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
    let args = match fibref::cmdline::from_os(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(word) => {
            eprintln!(
                "fibc: an argument before `--` is not UTF-8: {}",
                word.to_string_lossy()
            );
            return ExitCode::from(2);
        }
    };
    match parse(&args) {
        Command::Run { file, trace, args } => run(&file, trace, &args),
        Command::Build { file, out, link } => build(&file, &out, &link),
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
