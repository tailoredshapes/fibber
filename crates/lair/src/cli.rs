//! Command-line handling for `lair`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lair::aot::{self, Options, Output};
use lair::{Jit, JitOptions};

const USAGE: &str = "usage:
  lair check FILE.lir
  lair run [-O n] FILE.lir [ARGS..]
  lair build FILE.lir -o OUT [-O n] [--emit obj|asm|llvm] [-l LIB]..
  lair emit-llvm FILE.lir
  lair cases DIR..";

pub fn dispatch(args: &[String]) -> Result<ExitCode, String> {
    let (cmd, rest) = args.split_first().ok_or(USAGE)?;
    match cmd.as_str() {
        "check" => check(rest),
        "run" => run(rest),
        "build" => build(rest),
        "emit-llvm" => emit_llvm(rest),
        "cases" => cases(rest),
        _ => Err(USAGE.into()),
    }
}

fn read(file: &str) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("{file}: cannot read: {e}"))
}

fn check(rest: &[String]) -> Result<ExitCode, String> {
    let [file] = rest else {
        return Err(USAGE.into());
    };
    lair::check_source(&read(file)?).map_err(|e| e.render(file))?;
    Ok(ExitCode::SUCCESS)
}

fn emit_llvm(rest: &[String]) -> Result<ExitCode, String> {
    let [file] = rest else {
        return Err(USAGE.into());
    };
    print!(
        "{}",
        lair::emit_llvm(&read(file)?, file).map_err(|e| e.render(file))?
    );
    Ok(ExitCode::SUCCESS)
}

fn opt_level(s: &str) -> Result<u8, String> {
    s.parse::<u8>()
        .ok()
        .filter(|n| *n <= 3)
        .ok_or_else(|| format!("bad -O level {s}"))
}

/// JIT-compile the module and run `main`; exit with its status.
fn run(rest: &[String]) -> Result<ExitCode, String> {
    let (opt, rest) = match rest {
        [o, n, tail @ ..] if o == "-O" => (opt_level(n)?, tail),
        _ => (0, rest),
    };
    let (file, prog_args) = rest.split_first().ok_or(USAGE)?;
    let m = lair::for_executable(&read(file)?).map_err(|e| e.render(file))?;
    let mut jit = Jit::new(JitOptions { opt_level: opt }).map_err(|e| e.render(file))?;
    jit.add_module(file, &m).map_err(|e| e.render(file))?;
    let takes_args = !jit
        .signature("main")
        .map_err(|e| e.render(file))?
        .params
        .is_empty();
    let argv_s: Vec<std::ffi::CString> = std::iter::once(file)
        .chain(prog_args)
        .map(|a| std::ffi::CString::new(a.as_str()).unwrap_or_default())
        .collect();
    let mut argv: Vec<*const std::os::raw::c_char> = argv_s.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());
    // SAFETY: check_main fixed main's type to one of these two C
    // signatures; argv outlives the call and ends in a null pointer.
    let code = unsafe {
        if takes_args {
            let f: extern "C" fn(i32, *const *const std::os::raw::c_char) -> i32 =
                jit.function("main").map_err(|e| e.render(file))?;
            f(argv_s.len() as i32, argv.as_ptr())
        } else {
            let f: extern "C" fn() -> i32 = jit.function("main").map_err(|e| e.render(file))?;
            f()
        }
    };
    // exit() flushes the C library's buffers, as returning from a C main would.
    std::process::exit(code)
}

fn build(rest: &[String]) -> Result<ExitCode, String> {
    let (file, rest) = rest.split_first().ok_or(USAGE)?;
    let mut opts = Options::default();
    let (mut out, mut emit) = (None, Output::Object);
    let mut exe = true;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        let mut val = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "-o" => out = Some(PathBuf::from(val()?)),
            "-O" => opts.opt_level = opt_level(&val()?)?,
            "-l" => opts.libs.push(val()?),
            "--emit" => {
                exe = false;
                emit = match val()?.as_str() {
                    "obj" => Output::Object,
                    "asm" => Output::Assembly,
                    "llvm" => Output::LlvmIr,
                    other => return Err(format!("unknown --emit {other}")),
                }
            }
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    let out = out.ok_or("build needs -o OUT")?;
    let src = read(file)?;
    if exe {
        let m = lair::for_executable(&src).map_err(|e| e.render(file))?;
        aot::build_executable(&m, file, &out, &opts).map_err(|e| e.render(file))?;
    } else {
        let m = lair::check_source(&src).map_err(|e| e.render(file))?;
        let bytes = aot::emit(&m, file, emit, &opts).map_err(|e| e.render(file))?;
        std::fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn cases(rest: &[String]) -> Result<ExitCode, String> {
    let dirs: Vec<PathBuf> = rest.iter().map(PathBuf::from).collect();
    let found = lair::cases::collect(&dirs);
    if found.is_empty() {
        return Err("no cases found".into());
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let scratch = std::env::temp_dir().join(format!("lair-cases-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let reports = lair::cases::run_all(&me, &found, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(summarise(&reports))
}

fn category(p: &Path) -> String {
    p.parent()
        .and_then(|d| d.file_name())
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn summarise(reports: &[lair::cases::Report]) -> ExitCode {
    let mut cats: Vec<(String, usize, usize)> = Vec::new();
    for r in reports {
        let c = category(&r.path);
        match &r.failure {
            None => println!("ok    {}", r.path.display()),
            Some(f) => println!("FAIL  {}: {f}", r.path.display()),
        }
        if cats.last().is_none_or(|l| l.0 != c) {
            cats.push((c, 0, 0));
        }
        if let Some(l) = cats.last_mut() {
            l.1 += 1;
            l.2 += usize::from(r.failure.is_none());
        }
    }
    for (c, n, ok) in &cats {
        println!("{c}: {n} cases, {ok} pass, {} fail", n - ok);
    }
    let failed = reports.iter().filter(|r| r.failure.is_some()).count();
    println!(
        "total: {} cases, {} pass, {failed} fail",
        reports.len(),
        reports.len() - failed
    );
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
