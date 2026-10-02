//! The header and the exports agree: every `#[no_mangle]` function of
//! `src/capi` is declared in `include/lair.h`, with as many parameters,
//! and every `lair_` function the header declares exists (method.md:
//! the header is hand-written, so a test is what keeps it true).

use std::collections::BTreeMap;
use std::path::Path;

/// What the check reads from a function's two declarations.
type Arities = BTreeMap<String, usize>;

/// The `{ name -> parameter count }` of every `#[no_mangle]` function of
/// `src`, or the first one that is not `pub extern "C"` (a function
/// that is not `pub` is not exported by a `cdylib` of this crate's shape).
fn exported(src: &str) -> Result<Arities, String> {
    let mut found = Arities::new();
    let mut rest = src;
    while let Some(at) = rest.find("#[no_mangle]") {
        rest = &rest[at + "#[no_mangle]".len()..];
        let Some(f) = rest.find("fn ") else {
            return Err("#[no_mangle] with no function after it".into());
        };
        let head = &rest[..f];
        let head = head.rsplit(['\n', ']']).next().unwrap_or("");
        if !head.trim_start().starts_with("pub ") || !head.contains("extern \"C\"") {
            return Err(format!(
                "a #[no_mangle] function is not `pub ... extern \"C\"`: {}",
                rest[f..].lines().next().unwrap_or("")
            ));
        }
        let sig = &rest[f + 3..];
        let open = sig.find('(').ok_or("no parameter list")?;
        let name = sig[..open].trim().to_string();
        found.insert(name, params(&sig[open..])?);
    }
    Ok(found)
}

/// The parameter count of the list that starts at the `(` of `s`.
fn params(s: &str) -> Result<usize, String> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(count(&s[1..i]));
                }
            }
            _ => {}
        }
    }
    Err("an unterminated parameter list".into())
}

/// The parameters in the text between the parentheses: separated by
/// commas outside brackets, `void` alone meaning none.
fn count(list: &str) -> usize {
    let list = list.trim().trim_end_matches(',').trim();
    if list.is_empty() || list == "void" {
        return 0;
    }
    let mut depth = 0i32;
    let mut n = 1;
    for c in list.chars() {
        match c {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => depth -= 1,
            ',' if depth == 0 => n += 1,
            _ => {}
        }
    }
    n
}

/// The header without its comments.
fn uncomment(h: &str) -> String {
    let mut out = String::new();
    let mut rest = h;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("/*") {
            rest = r.split_once("*/").map_or("", |(_, after)| after);
        } else if let Some(r) = rest.strip_prefix("//") {
            rest = r.split_once('\n').map_or("", |(_, after)| after);
        } else {
            let mut cs = rest.chars();
            out.extend(cs.next());
            rest = cs.as_str();
        }
    }
    out
}

/// The `{ name -> parameter count }` of every `lair_` function the
/// header declares: an identifier `lair_...` followed by `(`.
fn declared(header: &str) -> Result<Arities, String> {
    let h = uncomment(header);
    let mut found = Arities::new();
    let mut from = 0;
    while let Some(at) = h[from..].find("lair_") {
        let start = from + at;
        let end = h[start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(h.len(), |e| start + e);
        from = end;
        let after = h[end..].trim_start();
        if after.starts_with('(') && !h[..start].ends_with(|c: char| c.is_ascii_alphanumeric()) {
            found.insert(h[start..end].to_string(), params(after)?);
        }
    }
    Ok(found)
}

/// Every disagreement between the exports and the header, one per line.
fn compare(rust: &Arities, header: &Arities) -> Result<(), String> {
    let mut problems = Vec::new();
    for (name, n) in rust {
        match header.get(name) {
            None => problems.push(format!("{name} is exported but not declared in lair.h")),
            Some(m) if m != n => problems.push(format!(
                "{name} takes {n} parameters in Rust but {m} in lair.h"
            )),
            Some(_) => {}
        }
    }
    for name in header.keys().filter(|n| !rust.contains_key(*n)) {
        problems.push(format!("{name} is declared in lair.h but not exported"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// The exports of every file of `src/capi` but this one.
fn all_exports(dir: &Path) -> Result<Arities, String> {
    let mut all = Arities::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|x| x == "rs") && !path.ends_with("header.rs") {
            let src = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            all.extend(exported(&src).map_err(|e| format!("{}: {e}", path.display()))?);
        }
    }
    Ok(all)
}

#[test]
fn the_header_declares_exactly_what_the_crate_exports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rust = all_exports(&root.join("src/capi")).unwrap();
    let header = std::fs::read_to_string(root.join("include/lair.h")).unwrap();
    assert!(rust.len() >= 22, "found only {:?}", rust.keys());
    compare(&rust, &declared(&header).unwrap()).unwrap();
}

#[test]
fn the_header_is_a_usable_c_header() {
    let h = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("include/lair.h"))
        .unwrap();
    let code = uncomment(&h);
    for needle in [
        "#ifndef LAIR_H",
        "#define LAIR_H",
        "#include <stddef.h>",
        "#include <stdint.h>",
        "extern \"C\" {",
        "#endif /* LAIR_H */",
    ] {
        assert!(
            code.contains(needle) || h.contains(needle),
            "lair.h lacks {needle}"
        );
    }
}

#[test]
fn the_check_fires_on_each_kind_of_drift() {
    let rust_src =
        "#[no_mangle]\npub unsafe extern \"C\" fn lair_a(x: *const u8, n: usize) -> i32 { 0 }\n\
        /// docs\n#[no_mangle]\n#[allow(x)]\npub extern \"C\" fn lair_b() -> usize { 0 }\n";
    let rust = exported(rust_src).unwrap();
    assert_eq!(rust.get("lair_a"), Some(&2));
    assert_eq!(rust.get("lair_b"), Some(&0));
    let good = declared("int lair_a(const char *x, size_t n);\nsize_t lair_b(void);").unwrap();
    assert!(compare(&rust, &good).is_ok());
    // A function the header lacks.
    let e = compare(
        &rust,
        &declared("int lair_a(const char *x, size_t n);").unwrap(),
    );
    assert_eq!(
        e.unwrap_err(),
        "lair_b is exported but not declared in lair.h"
    );
    // A declaration with nothing behind it.
    let e = compare(
        &rust,
        &declared("int lair_a(int a, int b); size_t lair_b(void); void lair_c(void);").unwrap(),
    );
    assert_eq!(
        e.unwrap_err(),
        "lair_c is declared in lair.h but not exported"
    );
    // A different parameter count.
    let e = compare(
        &rust,
        &declared("int lair_a(int a); size_t lair_b(void);").unwrap(),
    );
    assert_eq!(
        e.unwrap_err(),
        "lair_a takes 2 parameters in Rust but 1 in lair.h"
    );
    // Names in comments and types are not declarations.
    let d = declared(
        "/* lair_x() */ // lair_y()\ntypedef struct lair_z lair_z;\nint lair_a(int a, int b);",
    )
    .unwrap();
    assert_eq!(d.keys().collect::<Vec<_>>(), ["lair_a"]);
    // An export that is not pub.
    let e = exported("#[no_mangle]\nextern \"C\" fn lair_q() {}");
    assert!(e.unwrap_err().contains("not `pub"));
}
