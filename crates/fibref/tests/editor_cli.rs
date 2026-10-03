//! `fibref complete`, `fibref diagnostics` and `fibref lsp` as an editor
//! runs them: the binary, a buffer, a cursor. Each fixture of
//! `tests/complete/` is `NAME.fib`, whose `<|>` marks the cursor (and is
//! not part of the text), and `NAME.expect`, whose lines are
//!
//! * `has KIND LABEL`: an item with that label and kind is offered
//! * `not LABEL`: no item has that label
//! * `only KIND LABEL`: the items are exactly the `only` lines
//! * `detail LABEL TEXT`: that item's detail is TEXT
//! * `stdin NAME`: the text goes on standard input with `--path DIR/NAME`
//!   (a file that is not on disk) instead of being read from its file
//!
//! and `#` lines are comments.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use fibref::json::{parse, Json};

const FIBREF: &str = env!("CARGO_BIN_EXE_fibref");

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/complete")
}

/// Runs `fibref ARGS` with `stdin` and returns its status and stdout.
fn run(args: &[&str], stdin: &str) -> (i32, String) {
    let mut child = Command::new(FIBREF)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("fibref starts");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("writes");
    let out = child.wait_with_output().expect("finishes");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// The buffer without its `<|>` and the cursor's line (from 1) and
/// column (from 0).
fn cursor(text: &str) -> (String, usize, usize) {
    let at = text.find("<|>").expect("a <|> cursor");
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count());
    (text.replacen("<|>", "", 1), line, col)
}

/// The items `fibref complete` printed: `(label, kind, detail)`.
fn items(out: &str) -> Vec<(String, String, String)> {
    let doc = parse(out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"));
    let list = doc.get("items").and_then(Json::as_arr).expect("items");
    let text = |i: &Json, k: &str| i.get(k).and_then(Json::as_str).expect(k).to_string();
    list.iter()
        .map(|i| (text(i, "label"), text(i, "kind"), text(i, "detail")))
        .collect()
}

fn check(name: &str) -> Vec<String> {
    let base = dir();
    let (text, line, col) =
        cursor(&fs::read_to_string(base.join(format!("{name}.fib"))).expect("fixture"));
    let expect = fs::read_to_string(base.join(format!("{name}.expect"))).expect("expectations");
    let (line, col) = (line.to_string(), col.to_string());
    let stdin_name = expect.lines().find_map(|l| l.strip_prefix("stdin "));
    let (status, out) = match stdin_name {
        Some(n) => {
            let path = base.join(n);
            run(
                &[
                    "complete",
                    "-",
                    &line,
                    &col,
                    "--path",
                    path.to_str().expect("UTF-8"),
                ],
                &text,
            )
        }
        None => {
            let tmp = std::env::temp_dir()
                .join(format!("fibref-complete-{name}-{}.fib", std::process::id()));
            fs::write(&tmp, &text).expect("writes");
            let r = run(&["complete", tmp.to_str().expect("UTF-8"), &line, &col], "");
            fs::remove_file(&tmp).expect("removes");
            r
        }
    };
    assert_eq!(status, 0, "{name}: exit status; output {out}");
    verdicts(name, &expect, &items(&out))
}

fn verdicts(name: &str, expect: &str, got: &[(String, String, String)]) -> Vec<String> {
    let mut fails = Vec::new();
    let mut only: Vec<(String, String)> = Vec::new();
    for l in expect
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let w: Vec<&str> = l.splitn(3, ' ').collect();
        let find = |label: &str| got.iter().find(|i| i.0 == label);
        let ok = match w[..] {
            ["has", kind, label] => find(label).is_some_and(|i| i.1 == kind),
            ["not", label] => find(label).is_none(),
            ["only", kind, label] => {
                only.push((kind.to_string(), label.to_string()));
                true
            }
            ["detail", label, text] => find(label).is_some_and(|i| i.2 == text),
            ["stdin", _] => true,
            _ => panic!("{name}: bad expectation line {l:?}"),
        };
        if !ok {
            fails.push(format!("{name}: `{l}` does not hold"));
        }
    }
    if !only.is_empty() {
        let mut have: Vec<_> = got.iter().map(|i| (i.1.clone(), i.0.clone())).collect();
        have.sort();
        only.sort();
        if have != only {
            fails.push(format!(
                "{name}: items are {have:?}, expected exactly {only:?}"
            ));
        }
    }
    fails
}

#[test]
fn every_fixture_holds() {
    let mut names: Vec<String> = fs::read_dir(dir())
        .expect("fixtures")
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .to_str()?
                .strip_suffix(".expect")
                .map(str::to_string)
        })
        .collect();
    names.sort();
    assert!(names.len() >= 8, "fixtures missing: {names:?}");
    let fails: Vec<String> = names.iter().flat_map(|n| check(n)).collect();
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

#[test]
fn the_verdict_lines_can_fail() {
    let got = [("a".to_string(), "function".to_string(), "d".to_string())];
    let bad = "has variable a\nnot a\ndetail a x\nhas function b\nonly function b\n";
    assert_eq!(verdicts("t", bad, &got).len(), 5);
    let good = "has function a\nnot b\ndetail a d\nonly function a\n";
    assert!(verdicts("t", good, &got).is_empty());
}

#[test]
fn diagnostics_locate_the_error_in_the_buffer_and_say_nothing_of_a_good_one() {
    let (status, out) = run(
        &["diagnostics", "-"],
        "(defun f () -> i64 1)\n(defun g () -> i64\n  (+ 1 \"s\"))\n",
    );
    assert_eq!(status, 0);
    let doc = parse(&out).expect("JSON");
    let list = doc
        .get("diagnostics")
        .and_then(Json::as_arr)
        .expect("diagnostics");
    assert_eq!(list.len(), 1, "{out}");
    let line = list[0].get("line").and_then(Json::as_usize);
    assert!(matches!(line, Some(2 | 3)), "{out}");
    for k in ["col", "endLine", "endCol"] {
        assert!(
            list[0].get(k).and_then(Json::as_usize).is_some(),
            "{k} in {out}"
        );
    }
    assert!(list[0]
        .get("message")
        .and_then(Json::as_str)
        .is_some_and(|m| !m.is_empty()));
    assert_eq!(
        list[0].get("severity").and_then(Json::as_str),
        Some("error")
    );
    let (status, out) = run(&["diagnostics", "-"], "(defun f () -> i64 1)\n");
    assert_eq!((status, out.trim()), (0, "{\"diagnostics\":[]}"));
}

#[test]
fn an_unreadable_buffer_is_a_diagnostic_at_the_unclosed_form_and_exit_zero() {
    let (status, out) = run(&["diagnostics", "-"], "(defun f () -> i64 1)\n(defun g (\n");
    assert_eq!(status, 0);
    let doc = parse(&out).expect("JSON");
    let first = &doc.get("diagnostics").and_then(Json::as_arr).expect("list")[0];
    assert_eq!(first.get("line").and_then(Json::as_usize), Some(2), "{out}");
}

#[test]
fn bad_usage_is_status_two_and_a_missing_file_too() {
    assert_eq!(run(&["complete"], "").0, 2);
    assert_eq!(run(&["complete", "-", "x", "1"], "").0, 2);
    assert_eq!(run(&["complete", "/nonexistent/x.fib", "1", "0"], "").0, 2);
    assert_eq!(run(&["diagnostics"], "").0, 2);
}
