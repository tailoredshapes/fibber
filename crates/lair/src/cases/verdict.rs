//! Deciding one case: accept or reject, on each path.

use std::path::Path;

use super::exec::{run, Outcome};
use super::header::{parse, Expect, Header, Paths};

/// Run a case; `Err` describes the first disagreement with its header.
pub fn run_case(lair: &Path, path: &Path, scratch: &Path, n: usize) -> Result<(), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("cannot read: {e}"))?;
    let h = parse(&src)?;
    let file = path.to_string_lossy().into_owned();
    let exe = scratch.join(format!("case{n}"));
    let exe_s = exe.to_string_lossy().into_owned();
    if h.paths != Paths::Aot {
        let jit = run(lair, &["run", &file])?;
        judge("jit", &h, &jit)?;
    }
    if h.paths != Paths::Jit {
        let build = run(lair, &["build", &file, "-o", &exe_s, "-O", "2"])?;
        match &h.expect {
            Expect::Reject(_) => judge("aot", &h, &build)?,
            Expect::Accept { .. } => {
                if build.status != 0 {
                    return Err(format!("aot: build failed: {}", build.stderr.trim()));
                }
                let ran = run(&exe, &[])?;
                let _ = std::fs::remove_file(&exe);
                judge("aot", &h, &ran)?;
            }
        }
    }
    ir_checks(&h, &src, &file)
}

fn judge(path: &str, h: &Header, o: &Outcome) -> Result<(), String> {
    match &h.expect {
        Expect::Accept { exit, out } => {
            if o.status != *exit {
                return Err(format!(
                    "{path}: exit {} (expected {exit}); stderr: {}",
                    o.status,
                    o.stderr.trim()
                ));
            }
            let got: Vec<&str> = o.stdout.lines().collect();
            if got != out.iter().map(String::as_str).collect::<Vec<_>>() {
                return Err(format!("{path}: output {got:?}, expected {out:?}"));
            }
            Ok(())
        }
        Expect::Reject(text) => {
            if o.status == 0 {
                return Err(format!(
                    "{path}: accepted, expected an error containing {text:?}"
                ));
            }
            if o.status != 1 || o.stderr.contains("internal error") || o.stderr.contains("panicked")
            {
                return Err(format!(
                    "{path}: failed badly (status {}): {}",
                    o.status,
                    o.stderr.trim()
                ));
            }
            if !o.stderr.contains(text.as_str()) {
                return Err(format!(
                    "{path}: error {:?} does not contain {text:?}",
                    o.stderr.trim()
                ));
            }
            Ok(())
        }
    }
}

fn ir_checks(h: &Header, src: &str, file: &str) -> Result<(), String> {
    if h.ir.is_empty() && h.ir_not.is_empty() {
        return Ok(());
    }
    let ir = crate::emit_llvm(src, file).map_err(|e| format!("ir: {}", e.render(file)))?;
    if let Some(t) = h.ir.iter().find(|t| !ir.contains(t.as_str())) {
        return Err(format!("ir: does not contain {t:?}"));
    }
    if let Some(t) = h.ir_not.iter().find(|t| ir.contains(t.as_str())) {
        return Err(format!("ir: contains {t:?}"));
    }
    Ok(())
}
