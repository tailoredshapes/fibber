//! What every test of the C interface from fibber needs: `liblair.so` and
//! the `lair` binary built once for the profile the test was built with,
//! `compiler/jit-demo.fib` built once against the library with
//! `fibc build -L DIR -l lair`, and the demo run with its address space
//! capped (the machine has been taken down by unbounded runs before).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// How long one run of the demo may take: a macro that loops, or a
/// mailbox that waits for a reply that never comes, fails the test and
/// does not hang the suite. A run takes a few seconds.
const LIMIT: Duration = Duration::from_secs(180);

/// What was built: the `lair` binary (for `lair check`, the oracle of the
/// checker's text) and the demo, which links `liblair.so` from the same
/// directory.
pub struct Built {
    pub lair: PathBuf,
    pub demo: PathBuf,
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn repo() -> PathBuf {
    manifest().join("../..")
}

/// `target/<profile>`, found from this test's own executable
/// (`target/<profile>/deps/capi-HASH`), which is right whatever
/// `CARGO_TARGET_DIR` or `--release` said.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("own path");
    exe.parent()
        .and_then(Path::parent)
        .expect("target/<profile>/deps")
        .to_path_buf()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `cargo build -p lair` (the library, which makes `liblair.so`, and the
/// `lair` binary) for this test's profile; the profile directory.
fn build_lair() -> PathBuf {
    let dir = profile_dir();
    let profile = dir.file_name().and_then(|p| p.to_str()).unwrap_or("debug");
    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo
        .args(["build", "-p", "lair", "-j2"])
        .current_dir(manifest());
    match profile {
        "debug" => {}
        "release" => {
            cargo.arg("--release");
        }
        other => {
            cargo.args(["--profile", other]);
        }
    }
    if let Some(t) = std::env::var_os("CARGO_TARGET_DIR") {
        cargo.env("CARGO_TARGET_DIR", t);
    }
    let out = cargo.output().expect("cargo runs");
    assert!(
        out.status.success(),
        "cargo build -p lair failed:\n{}",
        text(&out.stderr)
    );
    assert!(dir.join("liblair.so").exists(), "liblair.so was not built");
    assert!(dir.join("lair").exists(), "the lair binary was not built");
    dir
}

/// `fibc build compiler/jit-demo.fib -o OUT -L LIB -l lair`.
fn build_demo(lib_dir: &Path) -> PathBuf {
    let out_dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("capi-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("a directory for the demo");
    let demo = out_dir.join("jit-demo");
    let built = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .arg("build")
        .arg(repo().join("compiler/jit-demo.fib"))
        .arg("-o")
        .arg(&demo)
        .arg("-L")
        .arg(lib_dir)
        .args(["-l", "lair"])
        .output()
        .expect("fibc runs");
    assert!(
        built.status.success(),
        "fibc build compiler/jit-demo.fib failed:\n{}{}",
        text(&built.stdout),
        text(&built.stderr)
    );
    demo
}

/// The library, the binary and the demo, built on first use by whichever
/// test comes first and shared by the rest.
pub fn built() -> &'static Built {
    static BUILT: OnceLock<Built> = OnceLock::new();
    BUILT.get_or_init(|| {
        let lib_dir = build_lair();
        let demo = build_demo(&lib_dir);
        Built {
            lair: lib_dir.join("lair"),
            demo,
        }
    })
}

/// At most one demo run at a time: each starts an LLVM JIT.
pub fn one_at_a_time() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs `cmd` to its end, or kills it after `limit` and panics with what
/// it had printed.
fn run_limited(cmd: &mut Command, limit: Duration) -> Output {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the command starts");
    let drain = |mut pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
    };
    let out = drain(Box::new(child.stdout.take().expect("piped")));
    let err = drain(Box::new(child.stderr.take().expect("piped")));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("the child can be waited for") {
            break status;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            let (out, err) = (
                out.join().unwrap_or_default(),
                err.join().unwrap_or_default(),
            );
            panic!("killed after {limit:?}:\n{}\n{}", text(&out), text(&err));
        }
        thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    }
}

/// The demo run with `args`, its address space capped at 6 GB, no core
/// file, and no `LD_LIBRARY_PATH`: it finds `liblair.so` by the rpath
/// `fibc build -L` gave it, or it does not start.
pub fn run_demo(args: &[&str]) -> Output {
    let _one = one_at_a_time();
    run_limited(
        Command::new("sh")
            .args([
                "-c",
                "ulimit -c 0 2>/dev/null; ulimit -v 6000000; exec \"$0\" \"$@\"",
            ])
            .arg(&built().demo)
            .args(args)
            .env_remove("LD_LIBRARY_PATH"),
        LIMIT,
    )
}

/// The demo's standard output, after checking that it ended cleanly.
pub fn demo_stdout(args: &[&str]) -> String {
    clean_stdout(args, run_demo(args))
}

/// The standard output of a demo run `out` that was given `args`, after
/// checking that it ended cleanly.
pub fn clean_stdout(args: &[&str], out: Output) -> String {
    assert_eq!(
        out.status.code(),
        Some(0),
        "jit-demo {args:?}: {:?}\n{}\n{}",
        out.status,
        text(&out.stdout),
        text(&out.stderr)
    );
    text(&out.stdout)
}

/// The text of a run's standard output, whatever its status.
pub fn stdout_text(out: &Output) -> String {
    text(&out.stdout)
}

#[test]
#[should_panic(expected = "killed after")]
fn a_run_that_does_not_end_is_killed_and_fails_the_test() {
    run_limited(Command::new("sleep").arg("30"), Duration::from_millis(200));
}

#[test]
fn a_run_that_ends_gives_its_status_and_both_streams() {
    let out = run_limited(
        Command::new("sh").args(["-c", "echo out; echo err >&2; exit 3"]),
        Duration::from_secs(30),
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(text(&out.stdout), "out\n");
    assert_eq!(text(&out.stderr), "err\n");
}
