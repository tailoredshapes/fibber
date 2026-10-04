//! `lair dump-ast` over the whole lIR suite (docs/design/lair-ast-dump.md).

use lair::dump::dump;
use std::path::{Path, PathBuf};

fn lir_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut es: Vec<_> = std::fs::read_dir(dir).expect("dir").flatten().collect();
    es.sort_by_key(|e| e.path());
    for e in es {
        let p = e.path();
        if p.is_dir() {
            lir_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "lir") {
            out.push(p);
        }
    }
}

#[test]
fn every_parsed_case_dumps_with_known_tags_and_deterministically() {
    let mut files = Vec::new();
    lir_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/lir"),
        &mut files,
    );
    let (mut parsed, mut tags) = (0, std::collections::BTreeSet::new());
    for f in &files {
        let src = std::fs::read_to_string(f).expect("read");
        let Ok(m) = lir::parse(&src) else { continue };
        parsed += 1;
        let a = dump(&m);
        assert_eq!(a, dump(&m), "{}", f.display());
        assert!(a.starts_with("module\n"));
        for l in a.lines() {
            let tag = l.trim_start().split(' ').next().unwrap_or("");
            assert_ne!(tag, "unknown", "{}: {l}", f.display());
            tags.insert(tag.to_string());
        }
    }
    assert!(parsed > 250, "only {parsed} of {} parsed", files.len());
    // 5 item tags, block, bind, case, and the expression tags.
    assert!(tags.len() >= 45, "{} tags seen: {tags:?}", tags.len());
}

fn dump_ast_of(text: &str, name: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("lair-dump-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let file = dir.join(name);
    std::fs::write(&file, text).expect("write");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lair"))
        .args(["dump-ast", &file.to_string_lossy()])
        .output()
        .expect("lair runs");
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_command_prints_the_dump_and_fails_with_a_diagnostic_on_bad_input() {
    let good = "(define (f void) () (block e (ret)))";
    let (ok, out, _) = dump_ast_of(good, "good.lir");
    assert!(
        ok && out.starts_with("module\n  define 1:1 name=\"f\""),
        "{out}"
    );
    // An unknown instruction: nothing on standard output, the diagnostic with the file name.
    let (ok, out, err) = dump_ast_of(&good.replace("ret", "retx"), "bad.lir");
    assert!(!ok && out.is_empty(), "{out}");
    assert!(
        err.ends_with("bad.lir:1:30: error: unknown instruction retx\n"),
        "{err}"
    );
    // An unclosed list.
    let (ok, out, err) = dump_ast_of("(define (f void) ()", "open.lir");
    assert!(
        !ok && out.is_empty() && err.contains("unclosed"),
        "{out}{err}"
    );
}

#[test]
fn a_changed_module_changes_the_dump() {
    let a = "(define (f i32) () (block e (ret (i32 1))))";
    let (_, d1, _) = dump_ast_of(a, "a.lir");
    let (_, d2, _) = dump_ast_of(&a.replace("(i32 1)", "(i32 2)"), "b.lir");
    assert_ne!(d1, d2);
    assert!(d1.contains("val=1\n") && d2.contains("val=2\n"));
}
