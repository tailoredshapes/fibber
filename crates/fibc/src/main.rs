//! Command-line entry point for `fibc` (spec/compiler.md §1).
//!
//! Exit codes of `run`: `main`'s result printed and 0; 3 rejected by
//! the front end; 4 the compiler cannot lower the program yet; 5 the
//! compilation failed; a trap aborts (134 under a shell). `build` exits
//! with the same codes (5 includes a link that failed) and 2 for a `-L`
//! directory it cannot use. `cases` exits 0 when no case failed, 1
//! otherwise, 2 on bad usage.

mod command;

use std::io::{self, Write};
#[cfg(feature = "llvm")]
use std::path::Path;
use std::process::ExitCode;

use fibc::compile::{compile, compile_executable};
use fibc::front::{check_in, Front};
use fibc::harness::child::{EXIT_COMPILE_FAILED, EXIT_REJECTED, EXIT_UNSUPPORTED};
use fibc::harness::gen::GenConfig;
#[cfg(feature = "llvm")]
use fibc::harness::Harness;
#[cfg(feature = "llvm")]
use fibref::cases::{render, Report, SelectError, Status};
use fibref::roots::Roots;
#[cfg(feature = "llvm")]
use lair::aot::{build_executable, library_dir, Options};
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
fn lower(file: &str, roots: &Roots) -> Result<String, ExitCode> {
    lower_kind(file, roots, false)
}

/// [`lower`], for an executable when `executable` (compiler.md §1).
fn lower_kind(file: &str, roots: &Roots, executable: bool) -> Result<String, ExitCode> {
    let source = read(file)?;
    let checked = match check_in(&source, file, roots) {
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
fn run(_file: &str, _roots: &Roots, _trace: bool, _args: &[String], _opt: u8) -> ExitCode {
    no_llvm("run")
}

#[cfg(not(feature = "llvm"))]
fn build(_file: &str, _roots: &Roots, _out: &str, _link: &Link, _opt: u8) -> ExitCode {
    no_llvm("build")
}

#[cfg(not(feature = "llvm"))]
fn cases(_dir: &str, _only: &[String]) -> ExitCode {
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
fn run(file: &str, roots: &Roots, trace: bool, args: &[String], opt: u8) -> ExitCode {
    let lir = match lower(file, roots) {
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
    let mut jit = match Jit::new(JitOptions { opt_level: opt }) {
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

/// `build`: the executable `out`, linked against `link`'s libraries by
/// `lair::aot::build_executable`, which searches and records (as an
/// rpath) each `-L` directory. A directory that cannot be one is refused
/// here first, before anything is compiled (exit 2); `lair` checks again.
#[cfg(feature = "llvm")]
fn build(file: &str, roots: &Roots, out: &str, link: &Link, opt: u8) -> ExitCode {
    if let Some(message) = link.dirs.iter().find_map(|d| library_dir(d).err()) {
        eprintln!("fibc: {message}");
        return ExitCode::from(2);
    }
    let lir = match lower_kind(file, roots, true) {
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
    let opts = Options {
        opt_level: opt,
        lib_dirs: link.dirs.clone(),
        libs: link.libs.clone(),
    };
    match build_executable(&module, file, Path::new(out), &opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

fn emit(file: &str, roots: &Roots) -> ExitCode {
    match lower(file, roots) {
        Ok(l) => write_stdout(&l),
        Err(code) => code,
    }
}

/// `emit-dump` (spec/bootstrap.md §8): the sections, the layout or the macro
/// module of each file, and the status the dump says.
fn emit_dump(files: &[String], roots: &Roots, opts: &fibc::emit_dump::Options) -> ExitCode {
    let (text, status) = fibc::emit_dump::emit_files(files, roots, opts);
    match write_stdout(&text) {
        ExitCode::SUCCESS => ExitCode::from(status),
        other => other,
    }
}

fn explain(file: &str, roots: &Roots) -> ExitCode {
    let source = match read(file) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match check_in(&source, file, roots) {
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

fn itrace(file: &str, roots: &Roots) -> ExitCode {
    let source = match read(file) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let run = fibc::harness::interp::run_in(&source, file, roots);
    // The trace is the whole of standard output; the outcome follows it
    // on standard error, in the words `fibref run` uses, so that
    // `2>&1 | grep -c '^A '` counts the allocations and nothing else.
    let code = write_stdout(&run.trace.render());
    eprintln!("{}", describe(&run.outcome));
    code
}

/// What `itrace` reports of the interpreter's run besides the trace.
fn describe(outcome: &fibref::cases::Outcome) -> String {
    use fibref::cases::Outcome;
    match outcome {
        Outcome::Compiled { result, audit } => format!("result: {result}\naudit:  {audit}"),
        Outcome::Rejected { message } => format!("rejected: {message}"),
        Outcome::Trapped { message, errors } => {
            format!("trap: {message}\naudit errors: {}", errors.len())
        }
        Outcome::Failed { message } => format!("failed: {message}"),
        Outcome::Unsupported { reason } => format!("unsupported: {reason}"),
    }
}

#[cfg(feature = "llvm")]
fn cases(dir: &str, only: &[String]) -> ExitCode {
    let fibc = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fibc: cannot find myself: {e}");
            return ExitCode::from(2);
        }
    };
    let report = match (Harness { fibc }).run_dir_only(Path::new(dir), only) {
        Ok(r) => r,
        Err(SelectError::NoMatch(prefix)) => {
            eprintln!("fibc: no case matches {prefix} in {dir}");
            return ExitCode::from(2);
        }
        Err(SelectError::Io(e)) => {
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
    let (dirs, args) = fibref::cmdline::split_roots(&args);
    let command = parse(&args);
    let takes_roots = matches!(
        command,
        Command::Run { .. }
            | Command::Build { .. }
            | Command::Emit { .. }
            | Command::EmitDump { .. }
            | Command::Explain { .. }
            | Command::Itrace { .. }
    );
    if !dirs.is_empty() && !takes_roots {
        eprintln!("fibc: -I belongs to a command that reads a program\n{USAGE}");
        return ExitCode::from(2);
    }
    let roots = Roots::from_env(&dirs, std::env::var_os("FIB_LIB").as_deref());
    match command {
        Command::Run {
            file,
            trace,
            args,
            opt,
        } => run(&file, &roots, trace, &args, opt),
        Command::Build {
            file,
            out,
            link,
            opt,
        } => build(&file, &roots, &out, &link, opt),
        Command::Emit { file } => emit(&file, &roots),
        Command::EmitDump { opts, files } => emit_dump(&files, &roots, &opts),
        Command::Explain { file } => explain(&file, &roots),
        Command::Itrace { file } => itrace(&file, &roots),
        Command::Cases { dir, only } => cases(&dir, &only),
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
