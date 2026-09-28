//! The emitted lIR is a real artefact (spec/compiler.md §1): for every
//! case the compiler can lower, the text `fibc emit` would print must
//! re-read and re-check cleanly with `crates/lir` (spec/lir.md §10),
//! with no LLVM involved. A case the compiler does not lower yet is
//! listed as pending, never counted.

use std::path::Path;

use fibc::compile::compile;
use fibc::front::{check, Front};
use fibref::cases::{list_cases, parse_header, Verdict};

#[test]
fn every_emitted_module_rereads_and_rechecks() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let cases = list_cases(&dir).expect("cases/ownership readable");
    assert!(!cases.is_empty(), "no cases");
    let (mut emitted, mut pending, mut bad) = (0, Vec::new(), Vec::new());
    for path in &cases {
        let source = std::fs::read_to_string(path).expect("readable");
        let header = parse_header(path, &source).expect("header");
        if matches!(header.verdict, Verdict::Reject { .. }) {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let checked = match check(&source, name) {
            Front::Checked(c) => c,
            _ => {
                bad.push(format!("{name}: the front end rejected an accept case"));
                continue;
            }
        };
        match compile(&checked) {
            Err(u) => pending.push(format!("{name} ({})", u.0)),
            Ok(text) => match lir::parse_and_check(&text) {
                Ok(_) => emitted += 1,
                Err(ds) => bad.push(format!("{name}: {}", ds[0])),
            },
        }
    }
    assert!(
        bad.is_empty(),
        "emitted lIR that lir rejects:\n{}",
        bad.join("\n")
    );
    assert!(emitted > 0, "nothing was emitted");
    eprintln!(
        "emitted and rechecked {emitted} modules; pending {}: {}",
        pending.len(),
        pending.join(", ")
    );
}
