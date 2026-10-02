use super::*;
use crate::testutil::Project;

fn run_all(project: &Project, timeout_s: f64) -> Result<Report, String> {
    let cfg = project.config(timeout_s);
    run(&cfg, project.path(), &mut |_| {})
}

/// The verdict of the mutant whose edit makes `after` of the module's text.
fn verdict_of<'a>(report: &'a Report, edited: &str) -> &'a Verdict {
    let hit = report
        .done
        .iter()
        .find(|d| d.mutant.apply(&report.src).contains(edited))
        .unwrap_or_else(|| panic!("no mutant makes {edited}"));
    &hit.verdict
}

fn killed_by(v: &Verdict) -> (&str, &'static str) {
    match v {
        Verdict::Killed { case, class, .. } => (case.as_str(), class),
        other => panic!("expected a kill, got {other:?}"),
    }
}

#[test]
fn a_planted_survivor_is_reported() {
    let project = Project::new("fibmut-s1");
    let report = run_all(&project, 1.0).unwrap();
    let survivors: Vec<String> = report
        .survivors()
        .iter()
        .map(|d| d.mutant.apply(&report.src))
        .collect();
    assert!(
        survivors.iter().any(|s| s.contains("(+ a 0)")),
        "{survivors:?}"
    );
    assert!(
        survivors.iter().any(|s| s.contains("(< a 6)")),
        "{survivors:?}"
    );
    assert_eq!(survivors.len(), 2, "{survivors:?}");
    let text = crate::report::render(&report, false);
    assert!(
        text.contains("SURVIVOR") && text.contains("+ (defun f (a: i64) -> i64 (+ a 0))"),
        "{text}"
    );
}

#[test]
fn a_killed_mutant_names_the_case_that_killed_it_and_how() {
    let project = Project::new("fibmut-s2");
    let report = run_all(&project, 1.0).unwrap();
    assert_eq!(killed_by(verdict_of(&report, "(- a 1)")), ("a", "result"));
    assert_eq!(killed_by(verdict_of(&report, "(<= a 5)")), ("b", "trap"));
    match verdict_of(&report, "(<= a 5)") {
        Verdict::Killed { detail, .. } => {
            assert_eq!(detail, "the run trapped: boom at lib/m.fib:3")
        }
        other => panic!("{other:?}"),
    }
    let verbose = crate::report::render(&report, true);
    assert!(
        verbose.contains("arith line 2: killed by a (result)"),
        "{verbose}"
    );
    assert!(
        verbose.contains("cmp line 3: killed by b (trap)"),
        "{verbose}"
    );
}

#[test]
fn a_mutant_that_never_terminates_is_killed_by_the_timeout() {
    let project = Project::new("fibmut-s3");
    let report = run_all(&project, 1.0).unwrap();
    assert_eq!(killed_by(verdict_of(&report, "(+ a 2)")), ("a", "timeout"));
}

#[test]
fn a_module_that_does_not_compile_is_invalid_and_runs_no_case() {
    let project = Project::new("fibmut-s4");
    let report = run_all(&project, 1.0).unwrap();
    // the working copy's directory is taken out of the reason
    assert!(
        matches!(verdict_of(&report, "(< a 4)"), Verdict::Invalid(why) if why.contains("type error") && why == "lib/m.fib:3:20: type error")
    );
    // a crash while loading is a kill of its own kind
    assert_eq!(
        killed_by(verdict_of(&report, "(< a 0)")),
        ("(loading)", "crash")
    );
    // the cases saw the 4-and-0 mutants never: the log has no call after
    // either of them that the stand-in could attribute; count the calls
    let table = crate::report::render(&report, false);
    assert!(table.contains("invalid                  1"), "{table}");
}

#[test]
fn the_cases_that_pass_nothing_or_do_not_load_the_module_are_told_apart() {
    let project = Project::new("fibmut-s5");
    let report = run_all(&project, 1.0).unwrap();
    // bad.fib fails the unmutated module: dropped with the harness's words
    assert_eq!(report.dropped.len(), 1);
    assert_eq!(report.dropped[0].0, "bad");
    assert!(report.dropped[0].1.contains("result: expected 0, got 1"));
    // u.fib passes with the module unreadable: it does not load it
    assert_eq!((report.selected, report.loading), (3, 2));
    // and no mutant was ever run against it
    assert!(
        !project.calls().iter().skip(7).any(|c| c == "u.fib"),
        "{:?}",
        project.calls()
    );
}

#[test]
fn cases_that_do_not_load_the_module_stop_the_run() {
    let project = Project::new("fibmut-s6");
    let mut cfg = project.config(1.0);
    cfg.only = vec!["u".to_string()];
    let err = run(&cfg, project.path(), &mut |_| {}).err().unwrap();
    assert!(err.contains("do not load the module"), "{err}");
}

#[test]
fn a_run_with_no_passing_case_stops() {
    let project = Project::new("fibmut-s7");
    let mut cfg = project.config(1.0);
    cfg.only = vec!["bad".to_string()];
    let err = run(&cfg, project.path(), &mut |_| {}).err().unwrap();
    assert!(err.contains("no selected case passes"), "{err}");
}

#[test]
fn the_case_that_killed_goes_first_for_the_next_mutant() {
    let project = Project::new("fibmut-s8");
    project.set_module(
        "(ns m)\n(defun g (a: i64) -> bool (< a 5))\n(defun f (a: i64) -> i64 (+ a 1))\n",
    );
    let mut cfg = project.config(1.0);
    cfg.ops = vec!["cmp".to_string(), "arith".to_string()];
    let report = run(&cfg, project.path(), &mut |_| {}).unwrap();
    // `(<= a 5)` is killed by b alone; `(- a 1)` is killed by a and by b, and b
    // is tried first now, so a is never asked
    assert_eq!(killed_by(&report.done[0].verdict), ("b", "trap"));
    assert_eq!(killed_by(&report.done[1].verdict), ("b", "trap"));
    let calls = project.calls();
    let tail = &calls[calls.len() - 3..];
    assert_eq!(tail, ["a.fib", "b.fib", "b.fib"], "{calls:?}");
}

#[test]
fn the_runner_leaves_the_tree_as_it_found_it() {
    let project = Project::new("fibmut-s9");
    let before = project.snapshot();
    let report = run_all(&project, 1.0).unwrap();
    assert!(report.done.len() >= 7);
    assert_eq!(project.snapshot(), before);
    let modules: Vec<_> = before
        .iter()
        .filter(|(p, _)| p.ends_with("lib/m.fib"))
        .collect();
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].1, crate::testutil::MODULE.as_bytes());
}

#[test]
fn the_working_copy_is_what_the_tool_is_run_in() {
    // the stand-in reads FIB_LIB: had the run edited the project's own lib,
    // the snapshot test above would fail; here the copy is gone afterwards
    let project = Project::new("fibmut-s10");
    let report = run_all(&project, 1.0).unwrap();
    assert!(report.done.iter().any(|d| d.verdict != Verdict::Survived));
    let leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("fibmut-{}-", std::process::id()))
        })
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn the_filters_and_the_sample_choose_the_mutants() {
    let project = Project::new("fibmut-s11");
    let mut cfg = project.config(1.0);
    let all = plan(&cfg, project.path()).unwrap();
    assert_eq!((all.all.len(), all.filtered, all.chosen.len()), (7, 7, 7));
    cfg.ops = vec!["const".to_string()];
    assert_eq!(plan(&cfg, project.path()).unwrap().filtered, 5);
    cfg.ops.clear();
    cfg.lines = Some((3, 3));
    assert_eq!(plan(&cfg, project.path()).unwrap().filtered, 4);
    cfg.lines = None;
    cfg.max = 3;
    let three = plan(&cfg, project.path()).unwrap();
    assert_eq!((three.filtered, three.chosen.len()), (7, 3));
    assert_eq!(three.chosen, plan(&cfg, project.path()).unwrap().chosen);
    cfg.ops = vec!["nope".to_string()];
    assert!(plan(&cfg, project.path())
        .err()
        .unwrap()
        .contains("no operator nope"));
}

#[test]
fn a_module_outside_the_library_or_unreadable_is_refused() {
    let project = Project::new("fibmut-s12");
    let mut cfg = project.config(1.0);
    cfg.module = project.path().join("cases/stdlib/a.fib");
    let err = run(&cfg, project.path(), &mut |_| {}).err().unwrap();
    assert!(err.contains("is not under the library"), "{err}");
    cfg.module = project.path().join("lib/missing.fib");
    assert!(plan(&cfg, project.path())
        .err()
        .unwrap()
        .contains("cannot read"));
    project.set_module("(defun f (");
    cfg.module = project.path().join("lib/m.fib");
    assert!(plan(&cfg, project.path())
        .err()
        .unwrap()
        .contains("cannot split the module"));
}

#[test]
fn the_module_name_is_the_one_in_its_ns_form() {
    assert_eq!(
        module_name("(ns fib.seq.x (:use fib.core))"),
        Some("fib.seq.x".to_string())
    );
    assert_eq!(module_name(";; c\n(ns a)"), Some("a".to_string()));
    assert_eq!(module_name("(defun f () 1)"), None);
}
