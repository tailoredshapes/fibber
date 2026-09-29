//! Generated programs through the rule-6 harness (method.md rule 5,
//! compiler.md §5): the first seeds of `fibgen`, sizes 1 to 6 in
//! turn, run interpreted and compiled, and every one must pass the
//! model's verdict or be pending; a pending program is listed, never
//! counted as a pass.

use std::path::PathBuf;

use fibc::harness::gen::{run, GenConfig};
use fibc::harness::Harness;
use fibref::cases::{render, Status};

#[test]
fn generated_programs_agree_interpreted_and_compiled() {
    let harness = Harness {
        fibc: PathBuf::from(env!("CARGO_BIN_EXE_fibc")),
    };
    let dir = std::env::temp_dir().join(format!("fibc-gen-test-{}", std::process::id()));
    let cfg = GenConfig {
        seed: 1,
        count: 120,
        size: None,
        jobs: std::thread::available_parallelism().map_or(2, |n| n.get()),
        dir: dir.clone(),
    };
    let r = run(&harness, &cfg).expect("the programs are written and run");
    let text = render(&r.report);
    assert!(r.report.counts.total() > 0, "no program ran");
    assert!(
        r.report.ok(),
        "{text}\n{} failed; the failing programs are kept in {}",
        r.report.counts.fail,
        dir.display()
    );
    let pending: Vec<String> = r
        .report
        .results
        .iter()
        .filter(|c| matches!(c.status, Status::Pending(_)))
        .map(|c| c.name())
        .collect();
    eprintln!(
        "{} programs: {} pass, {} pending, {} model gaps{}",
        r.report.counts.total(),
        r.report.counts.pass,
        r.report.counts.pending,
        r.model_gaps.len(),
        if pending.is_empty() {
            String::new()
        } else {
            format!("\nPENDING: {}", pending.join(" "))
        }
    );
    let _ = std::fs::remove_dir_all(&dir);
}
