//! What a C caller writes around each call of the interface, shared by
//! `capi.rs`, `capi_mailbox.rs` and `capi_misuse.rs`: strings as pointer
//! and length, errors taken and freed, and a macro-time module whose
//! hooks go to a mailbox.

#![allow(dead_code)] // each test binary uses some of it

use std::ffi::c_char;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use lair::capi::*;
use lair::{Jit, JitOptions};

pub fn s(x: &str) -> (*const c_char, usize) {
    (x.as_ptr().cast(), x.len())
}

/// The text of an error, which this takes and frees; `None` for null.
pub unsafe fn take(e: *mut LairError) -> Option<String> {
    if e.is_null() {
        return None;
    }
    let mut len = 0usize;
    let p = lair_error_text(e, &mut len);
    let text = String::from_utf8(std::slice::from_raw_parts(p.cast::<u8>(), len).to_vec()).unwrap();
    assert_eq!(*p.add(len), 0, "the text is NUL terminated");
    lair_error_free(e);
    Some(text)
}

pub unsafe fn new_jit(opt: i32) -> *mut LairJit {
    let mut j = ptr::null_mut();
    assert_eq!(take(lair_jit_new(opt, &mut j)), None);
    assert!(!j.is_null());
    j
}

pub unsafe fn add(j: *mut LairJit, name: &str, src: &str) -> Option<String> {
    let ((n, nl), (c, cl)) = (s(name), s(src));
    take(lair_jit_add_source(j, n, nl, c, cl))
}

pub unsafe fn address(j: *mut LairJit, name: &str) -> Result<usize, String> {
    let (n, nl) = s(name);
    let mut a = 0usize;
    match take(lair_jit_address(j, n, nl, &mut a)) {
        None => Ok(a),
        Some(e) => {
            assert_eq!(a, 0, "the out-parameter is untouched on failure");
            Err(e)
        }
    }
}

pub unsafe fn c_entry(j: *mut LairJit, name: &str) -> Result<usize, String> {
    let (n, nl) = s(name);
    let mut a = 0usize;
    match take(lair_jit_c_entry(j, n, nl, &mut a)) {
        None => Ok(a),
        Some(e) => Err(e),
    }
}

pub unsafe fn call(addr: usize, args: &[i64]) -> i64 {
    lair_call_i64(addr, args.as_ptr(), args.len())
}

/// What the Rust API says adding `src` to a session that already holds
/// `before` (name, source) reports: the oracle for error texts.
pub fn rust_says(before: &[(&str, &str)], name: &str, src: &str) -> String {
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    for (n, m) in before {
        jit.add_source(n, m).unwrap();
    }
    jit.add_source(name, src).unwrap_err().to_string()
}

/// A macro-time module as `fibm.set-hooks` makes one (`c/hooked.lir`, which
/// the C consumer reads too).
pub const HOOKED: &str = include_str!("../c/hooked.lir");

pub struct Hooked {
    pub jit: *mut LairJit,
    pub install: usize,
}

impl Hooked {
    pub unsafe fn new(opt: i32) -> Hooked {
        let jit = new_jit(opt);
        assert_eq!(add(jit, "hooked", HOOKED), None);
        Hooked {
            jit,
            install: address(jit, "install").unwrap(),
        }
    }
    pub unsafe fn at(&self, name: &str) -> usize {
        address(self.jit, name).unwrap()
    }
    /// `fibm.set-hooks`: install the hooks, with `c` as the context.
    pub unsafe fn install(&self, c: *mut Call) {
        call(
            self.install,
            &[
                lair_hook1_address() as i64,
                lair_hook2_address() as i64,
                c as i64,
            ],
        );
    }
    pub unsafe fn start(&self, c: *mut Call, name: &str, args: &[i64]) {
        lair_call_start(c, self.at(name), args.as_ptr(), args.len());
    }
}

pub unsafe fn fault(c: *mut Call) -> Option<String> {
    let mut len = 0usize;
    let p = lair_call_fault(c, &mut len);
    if p.is_null() {
        assert_eq!(len, 0);
        return None;
    }
    let text = String::from_utf8(std::slice::from_raw_parts(p.cast::<u8>(), len).to_vec()).unwrap();
    assert_eq!(*p.add(len), 0, "NUL terminated");
    Some(text)
}

/// Serve every hook call of the running call: arity 1 answered by `f1`,
/// arity 2 by `f2`. The requests seen, and the call's result.
pub unsafe fn serve(
    c: *mut Call,
    f1: impl Fn(i64) -> i64,
    f2: impl Fn(i64, i64) -> i64,
) -> (Vec<Vec<i64>>, i64) {
    let mut seen = Vec::new();
    loop {
        match lair_call_wait(c) {
            0 => return (seen, lair_call_result(c)),
            1 => {
                let a = lair_call_hook_arg(c, 0);
                seen.push(vec![a]);
                lair_call_hook_reply(c, f1(a));
            }
            2 => {
                let (a, b) = (lair_call_hook_arg(c, 0), lair_call_hook_arg(c, 1));
                seen.push(vec![a, b]);
                lair_call_hook_reply(c, f2(a, b));
            }
            k => panic!("wait returned {k}: {:?}", fault(c)),
        }
    }
}

pub unsafe fn parked(h: &Hooked, name: &str, arg: i64) -> *mut Call {
    let c = lair_call_new();
    h.install(c);
    h.start(c, name, &[arg]);
    assert_eq!(lair_call_wait(c), 1);
    c
}

/// Ends the whole test binary, loudly, if a test is still running after
/// `secs`: a mailbox that deadlocks must fail, not hang the suite.
pub struct Watchdog(Arc<AtomicBool>);

pub fn watchdog(secs: u64) -> Watchdog {
    let done = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&done);
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(secs));
        if !seen.load(Ordering::SeqCst) {
            // Not eprintln!: the test harness would capture and drop it.
            let _ = writeln!(
                std::io::stderr(),
                "watchdog: a test ran for over {secs} s: a mailbox deadlock?"
            );
            std::process::exit(101);
        }
    });
    Watchdog(done)
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A scratch directory under the temp dir, removed when dropped, also
/// when the test panics.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("lair-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
