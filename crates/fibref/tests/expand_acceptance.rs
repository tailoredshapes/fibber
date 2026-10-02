//! Acceptance tests for the expander (`fibref::expand`) against the real
//! inputs: every case under `cases/ownership/` must expand, leaving only
//! core forms and calls, and five of them to the exact shape the spec
//! implies. The ```lisp blocks of `spec/drafts/PROPOSED_CASES.md` are
//! expanded as a report, not a verdict.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use fibref::expand::{
    expand_program, ExpandCtx, ExpandErrorKind, NoRunner, CORE_FORMS, PRELUDE_MACROS,
};
use fibref::syntax::{read_all, Form, FormKind};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn read_file(rel: &str) -> String {
    let path = PathBuf::from(ROOT).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Reads and expands one case, which must succeed.
fn expand_case(name: &str) -> (Vec<Form>, ExpandCtx) {
    let text = read_file(&format!("cases/ownership/{name}"));
    let forms = read_all(&text, name).unwrap_or_else(|e| panic!("{e}"));
    let mut ctx = ExpandCtx::new();
    let out = expand_program(forms, &mut ctx, &mut NoRunner).unwrap_or_else(|e| panic!("{e}"));
    (out, ctx)
}

fn printed(forms: &[Form]) -> Vec<String> {
    forms.iter().map(|f| f.to_string()).collect()
}

/// The first form in `forms` that is not core or a call: a list headed
/// by a prelude macro or by `quasiquote`/`unquote`/`unquote-splicing`,
/// or a `[..]`/`{..}` literal. `quote` bodies are data and `ns` clauses
/// are not expressions, so neither is searched.
fn residue(forms: &[Form]) -> Option<&Form> {
    let own = defined(forms);
    let mut stack: Vec<&Form> = forms.iter().collect();
    while let Some(f) = stack.pop() {
        match &f.kind {
            FormKind::Vec(_) | FormKind::Map(_) => return Some(f),
            FormKind::List(items) => {
                let head = items.first().and_then(Form::as_sym).unwrap_or("");
                let quasi = ["quasiquote", "unquote", "unquote-splicing"];
                // `for-each` is also the library function (§4.4); only the
                // literal-range form must have become a loop.
                let macro_use = match head {
                    "for-each" => is_range_loop(items),
                    // `(range n)` is the library function; only `(range
                    // a b)` must have been rewritten.
                    "range" => items.len() == 3,
                    // `update` and `reduce` are also library functions (R6b):
                    // only the calls the function cannot serve are rewritten,
                    // `(update m k f x ..)`, `(update m k (fnil g d))` and
                    // `(reduce f c)`; the rest is not judged here.
                    "update" => is_update_rewrite(items),
                    "reduce" => items.len() == 3,
                    // The operators, the collection functions and `swap!`
                    // (R6a) are the builtins' and the library's own functions
                    // for the binary call and are rewritten only for more:
                    // three or more operands (`assoc`: five, a collection
                    // and two pairs).
                    "+" | "-" | "*" | "<" | ">" | "<=" | ">=" | "=" | "max" | "min" | "bit-and"
                    | "bit-or" | "bit-xor" | "conj" | "dissoc" | "merge" | "swap!" => {
                        items.len() >= 4
                    }
                    "assoc" => items.len() >= 6,
                    _ => PRELUDE_MACROS.contains(&head),
                };
                // a module's own definition of the name beats the macro (R14)
                let macro_use = macro_use && !own.contains(head);
                if macro_use || quasi.contains(&head) {
                    return Some(f);
                }
                if head != "quote" && head != "ns" {
                    stack.extend(expressions(head, items));
                }
            }
            _ => {}
        }
    }
    None
}

/// The items of a list that are searched for residue: all of them,
/// except that a pattern, where brackets are a vector pattern (syntax
/// §1.4, §3.6), is not an expression: the first item of a `match`
/// clause and of a `let` binding pair are skipped.
fn expressions<'f>(head: &str, items: &'f [Form]) -> Vec<&'f Form> {
    let without_pattern = |f: &'f Form| -> Vec<&'f Form> {
        match f.as_list() {
            Some([_, rest @ ..]) => rest.iter().collect(),
            _ => vec![f],
        }
    };
    match (head, items) {
        ("match", [h, s, clauses @ ..]) => {
            let mut out = vec![h, s];
            out.extend(clauses.iter().flat_map(without_pattern));
            out
        }
        ("let", [h, bs, body @ ..]) => {
            let mut out = vec![h];
            out.extend(bs.as_list().unwrap_or(&[]).iter().flat_map(without_pattern));
            out.extend(body);
            out
        }
        _ => items.iter().collect(),
    }
}

/// The first symbol of a list form, or the symbol itself: the name of a
/// variant of a `defenum` (`Name` or `(Name f: T ..)`), of a method of a
/// `defprotocol` (`(name (self) ..)`).
fn first_name(f: &Form) -> Option<&str> {
    match &f.kind {
        FormKind::Sym(s) => Some(s.as_str()),
        _ => f.as_list()?.first()?.as_sym(),
    }
}

/// The names the top-level forms define: functions, constants, externs,
/// structs and enums with their variants, and the methods of protocols. A
/// call of one of these is the module's own, not a use of the prelude macro
/// of the same name.
fn defined(forms: &[Form]) -> HashSet<&str> {
    let mut out = HashSet::new();
    for form in forms {
        let items = form.as_list().unwrap_or(&[]);
        let head = items.first().and_then(Form::as_sym).unwrap_or("");
        let name = items.get(1).and_then(Form::as_sym);
        match head {
            "defun" | "extern" | "defstruct" => out.extend(name),
            "def" => out.extend(name.map(|n| n.strip_suffix(':').unwrap_or(n))),
            "defenum" | "defprotocol" => out.extend(items.iter().skip(2).filter_map(first_name)),
            _ => {}
        }
    }
    out
}

/// Whether `(for-each r f)` has a literal `(range a b)` or `(range n)`
/// and a literal one-parameter `fn`.
fn is_range_loop(items: &[Form]) -> bool {
    let head =
        |f: &Form, n: &str| f.as_list().and_then(|i| i.first()).and_then(Form::as_sym) == Some(n);
    match items {
        [_, r, f] => {
            let len = r.as_list().map_or(0, <[Form]>::len);
            let range = head(r, "range") && (len == 2 || len == 3);
            let params = f.as_list().and_then(|i| i.get(1)).and_then(Form::as_list);
            range && head(f, "fn") && params.is_some_and(|p| p.len() == 1)
        }
        _ => false,
    }
}

/// Whether `(update m k f ..)` is a call the macro rewrites: one with
/// extra arguments, or with a literal `(fnil g d)` for `f`.
fn is_update_rewrite(items: &[Form]) -> bool {
    let fnil = |f: &Form| {
        let l = f.as_list().unwrap_or(&[]);
        l.len() == 3 && l[0].as_sym() == Some("fnil")
    };
    items.len() >= 5 || (items.len() == 4 && fnil(&items[3]))
}

/// Every list head in `forms` outside `quote`, sorted and deduplicated.
fn heads(forms: &[Form]) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<&Form> = forms.iter().collect();
    while let Some(f) = stack.pop() {
        if let FormKind::List(items) = &f.kind {
            if let Some(h) = items.first().and_then(Form::as_sym) {
                out.push(h.to_string());
                if h == "quote" {
                    continue;
                }
            }
            stack.extend(items);
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn every_case_expands_to_core_forms_and_calls() {
    let mut names: Vec<String> = fs::read_dir(PathBuf::from(ROOT).join("cases/ownership"))
        .unwrap_or_else(|e| panic!("{e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".fib"))
        .collect();
    names.sort();
    assert!(names.len() >= 20, "{names:?}");
    for name in &names {
        // User macros need the evaluator (NoRunner reports them pending);
        // the case runner expands and runs those cases.
        if read_file(&format!("cases/ownership/{name}")).contains("(defmacro") {
            continue;
        }
        let (out, _) = expand_case(name);
        assert!(!out.is_empty(), "{name}");
        if let Some(f) = residue(&out) {
            panic!("{name}: not expanded at {}: {f}", f.pos);
        }
        for top in &out {
            let head = top.as_list().and_then(|i| i.first()).and_then(Form::as_sym);
            assert!(
                head.is_some_and(|h| CORE_FORMS.contains(&h)),
                "{name}: {top}"
            );
        }
    }
}

#[test]
fn residue_detector_fires() {
    let forms = read_all("(defun f () (when a b) [1])", "t").unwrap_or_else(|e| panic!("{e}"));
    assert!(residue(&forms).is_some());
    let forms = read_all("(defun f () '(when [a]))", "t").unwrap_or_else(|e| panic!("{e}"));
    assert!(residue(&forms).is_none());
    let src = "(defun f () (for-each (range 0 3) (fn (i) i)))";
    let forms = read_all(src, "t").unwrap_or_else(|e| panic!("{e}"));
    assert!(residue(&forms).is_some());
    let src = "(defun f () (for-each xs (fn (i) i)))";
    let forms = read_all(src, "t").unwrap_or_else(|e| panic!("{e}"));
    assert!(residue(&forms).is_none());
    // R14: a call of a name the module defines is its function, not a macro use
    for (src, residue_expected) in [
        ("(defun when (a b) a) (defun f () (when a b))", false),
        ("(defun f () (when a b)) (defun when (a b) a)", false),
        ("(defun f () (when a b)) (defun g () 1)", true),
        ("(defenum E (when x: i64)) (defun f () (when 1))", false),
        ("(defstruct str (a: i64)) (defun f () (str 1))", false),
        (
            "(defprotocol P (list (self) -> i64)) (defun f (x) (list x))",
            false,
        ),
        ("(def and: i64 1) (defun f () (and a b))", false),
        ("(defun f () (str a b))", true),
    ] {
        let forms = read_all(src, "t").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(residue(&forms).is_some(), residue_expected, "{src}");
    }
    // R6b: the macros that are also functions are residue only when rewritten
    for (src, residue_expected) in [
        ("(defun f () (defn g [x] x))", true),
        ("(defun f () (update m k f x))", true),
        ("(defun f () (update m k (fnil g d)))", true),
        ("(defun f () (update m k f))", false),
        ("(defun f () (reduce f c))", true),
        ("(defun f () (reduce f 0 c))", false),
        // R6a: the binary call is the function's; three operands are a macro call
        ("(defun f () (+ a b))", false),
        ("(defun f () (+ a b c))", true),
        ("(defun f () (< a b c))", true),
        ("(defun f () (max a b))", false),
        ("(defun f () (max a b c))", true),
        ("(defun f () (assoc m k v))", false),
        ("(defun f () (assoc m k v k2 v2))", true),
        ("(defun f () (swap! a f))", false),
        ("(defun f () (swap! a f x))", true),
    ] {
        let forms = read_all(src, "t").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(residue(&forms).is_some(), residue_expected, "{src}");
    }
}

#[test]
fn case_05_list_becomes_cons_cells() {
    let (out, _) = expand_case("05-closures-share-state.fib");
    assert_eq!(
        printed(&out),
        [
            "(defun make-counter () (let ((n (cell 0))) \
             (fib.prelude/Cons (fn () (set! n (+ (deref n) 1)) (deref n)) (fib.prelude/Cons (fn () (deref n)) fib.prelude/Empty))))",
            "(defun main () -> i64 (let ((c (make-counter))) \
             (let ((inc (nth c 0)) (get (nth c 1))) (inc) (inc) (get))))",
        ]
    );
}

#[test]
fn case_10_plet_spawns_and_joins_and_pmap_stays_a_call() {
    let (out, _) = expand_case("10-atom-old-value.fib");
    assert_eq!(
        printed(&out),
        ["(defun main () -> i64 \
          (let ((#a.1 (fib.prelude/spawn (fn () (atom (fib.prelude/vec-empty)))))) \
          (let ((a (fib.prelude/join #a.1))) \
          (pmap (fn (i) (let ((snapshot (deref a))) (swap! a (fn (c) (conj c i))) (count snapshot))) \
          (range 1000)) \
          (count (deref a)))))"]
    );
    let h = heads(&out);
    assert!(
        h.contains(&"fib.prelude/spawn".to_string()) && h.contains(&"fib.prelude/join".to_string())
    );
    assert!(!h.contains(&"plet".to_string()));
}

#[test]
fn case_13_pmap_is_a_call_over_a_closure_capturing_the_cell() {
    let (out, _) = expand_case("13-reject-cell-crosses-thread.fib");
    assert_eq!(
        printed(&out),
        ["(defun main () -> i64 (let ((n (cell 0))) \
          (pmap (fn (i) (set! n (+ (deref n) i))) (range 10)) (deref n)))"]
    );
}

#[test]
fn case_15_vector_literals_are_prelude_calls() {
    let (out, ctx) = expand_case("15-cycle-through-cell-leaks.fib");
    assert_eq!(
        printed(&out),
        [
            "(defstruct Knot (items: (Cell (Vec Knot))))",
            "(defun main () -> i64 (let ((k (Knot (cell (fib.prelude/vec-empty))))) \
             (set! (. k items) (fib.prelude/vec-conj (fib.prelude/vec-empty) k)) \
             (count (deref (. k items)))))",
        ]
    );
    let knot = ctx.struct_info("Knot").map(|s| s.fields.len());
    assert_eq!(knot, Some(1), "defstruct registered");
}

#[test]
fn case_19_if_let_becomes_match() {
    let (out, ctx) = expand_case("19-weak-parent-pointer.fib");
    assert_eq!(
        printed(&out),
        [
            "(defstruct Node (parent: (Option (Weak Node)) children: (Cell (Vec Node))))",
            "(defun add-child (parent) (let ((c (Node (some (weak parent)) (cell (fib.prelude/vec-empty))))) \
             (set! (. parent children) (conj (deref (. parent children)) c)) c))",
            "(defun depth (n: Node) -> i64 (match (. n parent) (nil 0) \
             ((some w) (match (deref w) ((fib.prelude/some p) (+ 1 (depth p))) (_ 0)))))",
            "(defun main () -> i64 (let ((root (Node nil (cell (fib.prelude/vec-empty))))) \
             (let ((a (add-child root))) (let ((b (add-child a))) (depth b)))))",
        ]
    );
    assert!(ctx.struct_info("Node").is_some());
    // §1.3: the match built by if-let carries the if-let call's position.
    let depth = out[2].as_list().unwrap_or(&[]);
    let clause = depth[5].as_list().unwrap_or(&[])[3]
        .as_list()
        .unwrap_or(&[]);
    let built = &clause[1];
    assert_eq!(built.as_list().and_then(|i| i[0].as_sym()), Some("match"));
    let text = read_file("cases/ownership/19-weak-parent-pointer.fib");
    let line = text
        .lines()
        .position(|l| l.contains("(if-let"))
        .map(|i| i + 1);
    assert_eq!(Some(built.pos.line), line);
    assert_eq!(built.pos.col, 15);
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
    out
}

#[test]
fn report_proposed_cases_that_do_not_expand() {
    let md = read_file("spec/drafts/PROPOSED_CASES.md");
    let blocks = lisp_blocks(&md);
    let (mut ok, mut pending, mut failed, mut unread) = (0, Vec::new(), Vec::new(), Vec::new());
    for (heading, line, body) in &blocks {
        let file = format!("PROPOSED_CASES.md:{line} ({heading})");
        let forms = match read_all(body, &file) {
            Ok(forms) => forms,
            Err(e) => {
                unread.push(e.to_string());
                continue;
            }
        };
        match expand_program(forms, &mut ExpandCtx::new(), &mut NoRunner) {
            Ok(_) => ok += 1,
            Err(e) if matches!(e.kind, ExpandErrorKind::MacroNeedsEvaluator { .. }) => {
                pending.push(e.to_string())
            }
            Err(e) => failed.push(e.to_string()),
        }
    }
    println!(
        "PROPOSED_CASES.md: {} lisp blocks: {ok} expand, {} pending (user macro), {} fail, {} do not read",
        blocks.len(),
        pending.len(),
        failed.len(),
        unread.len()
    );
    for u in &unread {
        println!("  NOT READ: {u}");
    }
    for p in &pending {
        println!("  PENDING: {p}");
    }
    for f in &failed {
        println!("  FAIL: {f}");
    }
}
