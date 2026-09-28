//! The rule-6 suite (spec/compiler.md §5): every case in
//! cases/ownership runs interpreted and compiled. A case the compiler
//! reports unsupported is Pending: listed, never a pass; the test fails
//! on any Fail or header error, and says how many are pending.

use std::path::{Path, PathBuf};

use fibc::harness::Harness;
use fibref::cases::{render, Status};

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership")
}

#[test]
fn ownership_cases_agree_interpreted_and_compiled() {
    let harness = Harness {
        fibc: PathBuf::from(env!("CARGO_BIN_EXE_fibc")),
    };
    let report = harness
        .run_dir(&cases_dir())
        .expect("cases/ownership readable");
    let text = render(&report);
    let pending: Vec<&str> = report
        .results
        .iter()
        .filter(|r| matches!(r.status, Status::Pending(_)))
        .map(|r| r.path.file_name().and_then(|n| n.to_str()).unwrap_or(""))
        .collect();
    assert!(report.counts.total() > 0, "no cases found");
    assert!(
        report.ok(),
        "{text}\n{} failed, {} pending",
        report.counts.fail,
        report.counts.pending
    );
    if !pending.is_empty() {
        eprintln!(
            "PENDING ({} of {}): {}",
            pending.len(),
            report.counts.total(),
            pending.join(" ")
        );
    }
}
