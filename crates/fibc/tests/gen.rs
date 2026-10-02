//! Generated programs through the rule-6 harness (method.md rule 5,
//! compiler.md §5): the first seeds of `fibgen`, sizes 1 to 6 in
//! turn, run interpreted and compiled, and every one must pass the
//! model's verdict or be pending; a pending program is listed, never
//! counted as a pass.

use std::path::PathBuf;

use fibc::harness::gen::{run, run_kind, GenConfig};
use fibc::harness::Harness;
use fibgen::driver::GenKind;
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
        jobs: std::thread::available_parallelism().map_or(2, |n| n.get().min(8)),
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

/// The library pipelines of stdlib §8.1 item 3 (`fibgen run --kind
/// pipelines`): seeds 1 to 200, sizes 1 to 6 in turn, each a program of
/// the four facades whose answer and call count the model computes, run
/// interpreted and compiled. Every one must be an accept or a predicted
/// trap, with equal results and free traces: none pending (the library is
/// all in the compiler's language), none a model gap.
#[test]
fn generated_pipelines_agree_interpreted_and_compiled() {
    let harness = Harness {
        fibc: PathBuf::from(env!("CARGO_BIN_EXE_fibc")),
    };
    let dir = std::env::temp_dir().join(format!("fibc-gen-pipelines-{}", std::process::id()));
    let cfg = GenConfig {
        seed: 1,
        count: 200,
        size: None,
        jobs: 2,
        dir: dir.clone(),
    };
    let r = run_kind(&harness, &cfg, GenKind::Pipelines).expect("the programs are written and run");
    let text = render(&r.report);
    assert_eq!(
        r.report.counts.total(),
        200,
        "every program has a verdict\n{text}"
    );
    assert!(
        r.report.ok(),
        "{text}\n{} failed; the failing programs are kept in {}",
        r.report.counts.fail,
        dir.display()
    );
    assert_eq!(
        r.report.counts.pending, 0,
        "no program may be pending\n{text}"
    );
    assert!(r.model_gaps.is_empty(), "model gaps: {:?}", r.model_gaps);
    eprintln!(
        "{} pipelines: {} pass",
        r.report.counts.total(),
        r.report.counts.pass
    );
    let _ = std::fs::remove_dir_all(&dir);
}
