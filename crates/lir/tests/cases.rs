//! The checker over the whole case suite, without LLVM: every `accept`
//! case of cases/lir passes `lir::parse_and_check`, and every `reject`
//! case fails it with the error its header names (spec/lir.md §13).
//! `lair`'s harness runs the same cases through the JIT and AOT.

use std::path::{Path, PathBuf};

fn cases(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("cases/lir exists").flatten() {
        let p = e.path();
        if p.is_dir() {
            cases(&p, out);
        } else if p.extension().is_some_and(|x| x == "lir") {
            out.push(p);
        }
    }
}

fn header(src: &str, key: &str) -> Option<String> {
    src.lines()
        .take_while(|l| l.starts_with(";;"))
        .find_map(|l| {
            let body = l.trim_start_matches(";;").trim_start();
            let (k, v) = body.split_once(':')?;
            (k == key).then(|| v.trim().to_string())
        })
}

#[test]
fn every_case_gets_its_verdict_from_the_checker() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cases/lir");
    let mut files = Vec::new();
    cases(&root, &mut files);
    assert!(files.len() > 200, "found {} cases", files.len());
    let mut failures = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).expect("readable");
        let expect = header(&src, "expect").unwrap_or_default();
        let result =
            lir::parse_and_check(&src).and_then(|m| lir::check_main(&m).map_err(|d| vec![d]));
        match (expect.as_str(), result) {
            ("accept", Ok(())) => {}
            ("accept", Err(ds)) => failures.push(format!("{}: rejected: {}", f.display(), ds[0])),
            ("reject", Ok(())) => failures.push(format!("{}: accepted", f.display())),
            ("reject", Err(ds)) => {
                let want = header(&src, "error").unwrap_or_default();
                if !ds.iter().any(|d| d.message.contains(&want)) {
                    failures.push(format!("{}: {} (wanted {want:?})", f.display(), ds[0]));
                }
            }
            (other, _) => failures.push(format!("{}: bad header {other:?}", f.display())),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
