use super::*;
use std::os::unix::fs::PermissionsExt;

fn exec(code: Option<i32>, stdout: &str, stderr: &str, timed_out: bool) -> Exec {
    Exec {
        code,
        stdout: stdout.into(),
        stderr: stderr.into(),
        timed_out,
    }
}

const PASS: &str =
    "case  status  detail\n000-a.fib  pass\n\n1 cases: 1 pass, 0 fail, 0 pending, 0 header error\n";

fn failing(detail: &str) -> String {
    format!("case  status  detail\n000-a.fib  FAIL    {detail}\n\n1 cases: 0 pass, 1 fail, 0 pending, 0 header error\n")
}

#[test]
fn the_counts_line_is_read() {
    assert_eq!(parse_counts(PASS), Some(Counts { total: 1, pass: 1 }));
    assert_eq!(
        parse_counts("x\n12 cases: 9 pass, 2 fail, 0 pending, 1 header error, 3 open\n"),
        Some(Counts { total: 12, pass: 9 })
    );
    assert_eq!(parse_counts("no counts here"), None);
}

#[test]
fn a_pass_is_one_case_and_one_pass() {
    assert_eq!(
        classify_case(&exec(Some(0), PASS, "", false)),
        Ok(Ending::Pass)
    );
    let open = "case  status  detail\n000-a.fib  OPEN  L1: boom\n\n1 cases: 0 pass, 0 fail, 0 pending, 0 header error, 1 open\nOPEN: 1 of 1";
    assert_eq!(
        classify_case(&exec(Some(0), open, "", false)),
        Ok(Ending::NotPass("000-a.fib OPEN L1: boom".to_string()))
    );
    // two cases when one was asked for: the selection was ambiguous
    let two = "2 cases: 2 pass, 0 fail, 0 pending, 0 header error\n";
    assert!(classify_case(&exec(Some(0), two, "", false)).is_err());
    assert!(classify_case(&exec(Some(0), "", "", false)).is_err());
}

fn assert_classes(cases: &[(&str, &'static str)]) {
    for &(detail, class) in cases {
        let got = classify_case(&exec(Some(1), &failing(detail), "", false));
        let want = Ok(Ending::Fail {
            class,
            detail: detail.to_string(),
        });
        assert_eq!(got, want, "{detail}");
    }
}

#[test]
fn an_accept_case_that_failed_is_classified_by_how_the_harness_says_it_failed() {
    assert_classes(&[
        ("result: expected 3, got 4", "result"),
        (
            "result: expected 3, got 4; audit: expected clean, got clean=false",
            "result",
        ),
        (
            "audit: expected clean, got clean=false leak-cycles=0 leaks=1 errors=0",
            "audit",
        ),
        (
            "allocs: expected at most 5 heap objects, the run allocated 6",
            "allocs",
        ),
        ("the run trapped: nth: index out of range", "trap"),
        (
            "expected accept, but rejected: result: expected is not a word",
            "compile",
        ),
        ("the run failed: leak", "failed"),
        ("something new", "other"),
    ]);
}

#[test]
fn a_reject_or_trap_case_that_failed_is_classified_too() {
    assert_classes(&[
        (
            "trapped, but the trap does not contain the expected text: expected \"a\", got \"b\"",
            "trap",
        ),
        (
            "expected a trap with \"x\", but the run finished: result 1, audit clean",
            "trap",
        ),
        (
            "expected reject with \"x\", but compiled: result 1, audit clean",
            "result",
        ),
        (
            "rejected, but the error does not contain the expected text: expected \"a\", got \"b\"",
            "result",
        ),
        (
            "expected reject with \"x\", but compiled and the run failed: boom",
            "failed",
        ),
    ]);
}

#[test]
fn a_timeout_a_crash_and_a_setup_error_are_three_things() {
    assert_eq!(
        classify_case(&exec(Some(124), "", "", true)),
        Ok(Ending::Timeout)
    );
    assert!(
        matches!(classify_case(&exec(Some(139), "", "segfault\n", false)), Ok(Ending::Crash(d)) if d.contains("segfault"))
    );
    assert!(matches!(
        classify_case(&exec(Some(101), "", "panicked", false)),
        Ok(Ending::Crash(_))
    ));
    assert!(matches!(
        classify_case(&exec(None, "", "", false)),
        Ok(Ending::Crash(_))
    ));
    for code in [2, 125, 126, 127] {
        let err = classify_case(&exec(Some(code), "", "no case matches 999-", false))
            .err()
            .unwrap();
        assert!(err.contains("no case matches 999-"), "{err}");
    }
    let header = "case  status  detail\n000-a.fib  HEADER  line 1: bad\n\n1 cases: 0 pass, 0 fail, 0 pending, 1 header error\n";
    assert_eq!(
        classify_case(&exec(Some(1), header, "", false)),
        Ok(Ending::BadHeader(
            "000-a.fib HEADER line 1: bad".to_string()
        ))
    );
}

#[test]
fn a_missing_tool_is_a_setup_error_not_a_verdict() {
    let dir = crate::fsutil::TempDir::new("fibmut-r1").unwrap();
    let sb = Sandbox {
        root: dir.path().to_path_buf(),
        lib: dir.path().to_path_buf(),
        cases: PathBuf::from("cases"),
    };
    let tool = Tool {
        kind: ToolKind::Fibref,
        program: PathBuf::from("/no/such/fibref"),
        vmem_kb: 0,
        timeout_s: 5.0,
    };
    // sh runs, `exec timeout .. nice /no/such/fibref` fails with 127
    let err = tool.run_case(&sb, "x.fib").err().unwrap();
    assert!(err.contains("did not run"), "{err}");
}

#[test]
fn the_wrapper_applies_the_memory_limit_and_the_time_limit() {
    let dir = crate::fsutil::TempDir::new("fibmut-r2").unwrap();
    let script = dir.path().join("probe.sh");
    std::fs::write(&script, "#!/bin/sh\nulimit -v\nnice\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let sb = Sandbox {
        root: dir.path().to_path_buf(),
        lib: dir.path().to_path_buf(),
        cases: PathBuf::from("c"),
    };
    let tool = Tool {
        kind: ToolKind::Fibref,
        program: script,
        vmem_kb: 1_234_567,
        timeout_s: 5.0,
    };
    // `nice` adds ten to the niceness it inherits, up to 19
    let own: i32 = String::from_utf8(Command::new("nice").output().unwrap().stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let e = tool.exec(&sb, &[]).unwrap();
    let said: Vec<&str> = e.stdout.lines().collect();
    assert_eq!(
        said,
        ["1234567".to_string(), (own + 10).min(19).to_string()],
        "{:?}",
        e.stdout
    );
    // a limit of 0 adds none: the process has the one it inherits
    let inherited = String::from_utf8(
        Command::new("sh")
            .args(["-c", "ulimit -v"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let unlimited = Tool { vmem_kb: 0, ..tool };
    assert_eq!(
        unlimited.exec(&sb, &[]).unwrap().stdout.lines().next(),
        Some(inherited.trim())
    );
}
