//! Acceptance tests for the reader (`fibref::syntax`) against the real
//! inputs: the twenty cases under `cases/ownership/`, the Appendix A code
//! blocks of `spec/syntax.md`, and (as a report, not a verdict) the code
//! blocks of `spec/drafts/PROPOSED_CASES.md`.

use std::fs;
use std::path::PathBuf;

use fibref::syntax::{read_all, Form, FormKind, IntWidth};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Top-level form counts, established by reading each file by hand.
const CASE_FORMS: [(&str, usize); 20] = [
    ("01-return-part-of-argument.fib", 2),
    ("02-structural-sharing.fib", 3),
    ("03-store-borrowed-value.fib", 2),
    ("04-branch-dependent-owner.fib", 2),
    ("05-closures-share-state.fib", 2),
    ("06-capture-borrowed-param.fib", 2),
    ("07-recursive-accumulator.fib", 2),
    ("08-mutate-while-iterating.fib", 2),
    ("09-iterator-outlives-source.fib", 2),
    ("10-atom-old-value.fib", 1),
    ("11-borrow-across-await.fib", 2),
    ("12-reject-same-binding-twice-inout.fib", 2),
    ("13-reject-cell-crosses-thread.fib", 1),
    ("14-reject-inout-in-async.fib", 2),
    ("15-cycle-through-cell-leaks.fib", 2),
    ("16-coordinated-update-single-atom.fib", 3),
    ("17-inout-and-borrow-same-call.fib", 2),
    ("18-reject-inout-captured-by-escaping-closure.fib", 2),
    ("19-weak-parent-pointer.fib", 4),
    ("20-weak-ref-to-dead-object.fib", 1),
];

fn case_path(name: &str) -> PathBuf {
    PathBuf::from(ROOT).join("cases/ownership").join(name)
}

fn read_case(name: &str) -> Vec<Form> {
    let path = case_path(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    read_all(&text, name).unwrap_or_else(|e| panic!("{e}"))
}

fn sym(name: &str) -> FormKind {
    FormKind::Sym(name.to_string())
}

fn list(f: &Form) -> &[Form] {
    f.as_list().unwrap_or_else(|| panic!("not a list: {f}"))
}

/// Every list in `f`, depth first.
fn lists<'a>(f: &'a Form, out: &mut Vec<&'a [Form]>) {
    let items = match &f.kind {
        FormKind::List(items) => {
            out.push(items);
            items
        }
        FormKind::Vec(items) | FormKind::Map(items) => items,
        _ => return,
    };
    items.iter().for_each(|i| lists(i, out));
}

/// The list headed by the symbol `head`, searched through `forms`.
fn find<'a>(forms: &'a [Form], head: &str) -> &'a [Form] {
    let mut all = Vec::new();
    forms.iter().for_each(|f| lists(f, &mut all));
    all.into_iter()
        .find(|l| l.first().and_then(Form::as_sym) == Some(head))
        .unwrap_or_else(|| panic!("no ({head} ...) form"))
}

fn is_inout(f: &Form, name: &str) -> bool {
    matches!(f.as_list(), Some([h, x]) if h.kind == sym("&") && x.kind == sym(name))
}

#[test]
fn the_twenty_cases_are_all_listed() {
    let mut on_disk: Vec<String> = fs::read_dir(PathBuf::from(ROOT).join("cases/ownership"))
        .expect("cases/ownership is readable")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".fib"))
        .collect();
    on_disk.sort();
    let listed: Vec<String> = CASE_FORMS.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(on_disk, listed);
}

#[test]
fn every_case_reads_with_the_expected_form_count() {
    for (name, count) in CASE_FORMS {
        let forms = read_case(name);
        assert_eq!(forms.len(), count, "{name}");
        // The header is ;; comments: the first form starts on the first
        // line that begins with `(`.
        let text = fs::read_to_string(case_path(name)).expect("readable");
        let first = text
            .lines()
            .position(|l| l.starts_with('('))
            .expect("a form")
            + 1;
        assert_eq!((forms[0].pos.line, forms[0].pos.col), (first, 1), "{name}");
        for f in &forms {
            assert_eq!(
                list(f)[0]
                    .as_sym()
                    .map(|s| s == "defun" || s == "defstruct"),
                Some(true)
            );
            let back = read_all(&f.to_string(), name).expect("printed form reads");
            assert_eq!(back, vec![f.clone()], "{name}: round trip");
        }
    }
}

#[test]
fn case_12_passes_x_twice_in_out() {
    let forms = read_case("12-reject-same-binding-twice-inout.fib");
    let call = find(&forms, "bar");
    assert_eq!(call.len(), 3, "(bar &x &x)");
    assert!(is_inout(&call[1], "x") && is_inout(&call[2], "x"));
    let params = list(&list(&forms[0])[2]);
    assert!(is_inout(&params[0], "a") && is_inout(&params[1], "b"));
}

#[test]
fn case_19_annotations_nil_and_deref() {
    let forms = read_case("19-weak-parent-pointer.fib");
    let depth = list(&forms[2]);
    assert_eq!(depth[1].kind, sym("depth"));
    let params = list(&depth[2]);
    assert_eq!(
        (&params[0].kind, &params[1].kind),
        (&sym("n:"), &sym("Node"))
    );
    assert_eq!((&depth[3].kind, &depth[4].kind), (&sym("->"), &sym("i64")));
    let node = find(&forms[3..], "Node");
    assert_eq!(node[1].kind, FormKind::Nil);
    let deref = find(&forms, "deref");
    assert_eq!(deref[1].to_string(), "(. parent children)");
    let clause = find(&forms, "match");
    assert_eq!(list(&clause[2])[0].kind, FormKind::Nil);
}

#[test]
fn cases_02_08_15_literal_shapes() {
    let forms = read_case("02-structural-sharing.fib");
    let let_form = find(&forms, "let");
    let binding = list(&list(&let_form[1])[0]);
    let int = |v| {
        Form::new(
            FormKind::Int {
                v,
                width: IntWidth::I64,
            },
            binding[1].pos.clone(),
        )
    };
    assert_eq!(binding[1].kind, FormKind::Vec(vec![int(1), int(2), int(3)]));
    let forms = read_case("08-mutate-while-iterating.fib");
    assert!(is_inout(&list(&list(&forms[0])[2])[0], "v"));
    let forms = read_case("15-cycle-through-cell-leaks.fib");
    assert_eq!(list(&forms[0])[2].to_string(), "(items: (Cell (Vec Knot)))");
    assert_eq!(find(&forms, "count")[1].to_string(), "(deref (. k items))");
}

/// The ```lisp blocks of a markdown text, each with the nearest heading
/// above it and its first line number.
fn lisp_blocks(md: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let (mut heading, mut open, mut body) = (String::new(), None, String::new());
    for (i, line) in md.lines().enumerate() {
        match open {
            None if line.starts_with('#') => {
                heading = line.trim_start_matches('#').trim().to_string()
            }
            None if line.trim() == "```lisp" => open = Some(i + 2),
            Some(start) if line.trim() == "```" => {
                out.push((heading.clone(), start, std::mem::take(&mut body)));
                open = None;
            }
            Some(_) => body.push_str(&format!("{line}\n")),
            None => {}
        }
    }
    assert!(open.is_none(), "unterminated ```lisp block");
    out
}

fn spec(rel: &str) -> String {
    let path = PathBuf::from(ROOT).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn appendix_a_blocks_read_and_match_the_case_files() {
    let md = spec("spec/syntax.md");
    let start = md.find("## Appendix A").expect("Appendix A heading");
    let end = md[start..]
        .find("\n## Open decisions")
        .map_or(md.len(), |e| start + e);
    let blocks = lisp_blocks(&md[start..end]);
    assert_eq!(blocks.len(), 20, "one block per case");
    for ((heading, line, body), (name, count)) in blocks.iter().zip(CASE_FORMS) {
        assert_eq!(heading, name, "block at appendix line {line}");
        let forms = read_all(body, heading).unwrap_or_else(|e| panic!("{heading}: {e}"));
        assert_eq!(forms.len(), count, "{heading}");
        assert_eq!(
            forms,
            read_case(name),
            "{heading}: appendix and case file differ"
        );
    }
}

#[test]
fn report_proposed_cases_that_do_not_read() {
    let md = spec("spec/drafts/PROPOSED_CASES.md");
    let blocks = lisp_blocks(&md);
    assert_eq!(
        blocks.len(),
        md.matches("```lisp").count(),
        "every block extracted"
    );
    let mut failures = Vec::new();
    for (heading, line, body) in &blocks {
        let file = format!("PROPOSED_CASES.md:{line} ({heading})");
        if let Err(e) = read_all(body, &file) {
            failures.push(e.to_string());
        }
    }
    println!(
        "PROPOSED_CASES.md: {} lisp blocks, {} do not read",
        blocks.len(),
        failures.len()
    );
    for f in &failures {
        println!("  NOT READ: {f}");
    }
}

#[test]
fn report_other_syntax_md_blocks_that_do_not_read() {
    let md = spec("spec/syntax.md");
    let blocks = lisp_blocks(&md);
    let failures: Vec<String> = blocks
        .iter()
        .filter_map(|(h, line, body)| read_all(body, &format!("syntax.md:{line} ({h})")).err())
        .map(|e| e.to_string())
        .collect();
    println!(
        "syntax.md: {} lisp blocks, {} do not read",
        blocks.len(),
        failures.len()
    );
    for f in &failures {
        println!("  NOT READ: {f}");
    }
}
