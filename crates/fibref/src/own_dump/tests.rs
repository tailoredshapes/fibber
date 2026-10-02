use std::collections::BTreeSet;
use std::path::PathBuf;

use super::*;

/// A scratch directory holding `files` (name, text), removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(label: &str, files: &[(&str, &str)]) -> Dir {
        let dir = std::env::temp_dir().join(format!("fibref-own-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        for (name, text) in files {
            std::fs::write(dir.join(name), text).expect("writable");
        }
        Dir(dir)
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The dump of the one file `src` and its status, with the directory
/// made `D` and every number of three digits or more made `#`: ids move
/// with the library, the shape of the text does not.
fn run(label: &str, src: &str, opts: &Options) -> (String, u8) {
    let dir = Dir::new(label, &[("t.fib", src)]);
    let (text, status) = own_files(&[dir.file("t.fib")], opts);
    (
        shown(&text.replace(&dir.0.to_string_lossy().into_owned(), "D")),
        status,
    )
}

/// `text` with every run of three digits or more made `#`.
fn shown(text: &str) -> String {
    let mut out = String::new();
    let mut digits = String::new();
    for c in text.chars().chain(std::iter::once('\n')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        out.push_str(if digits.len() >= 3 { "#" } else { &digits });
        digits.clear();
        out.push(c);
    }
    out.pop();
    out
}

fn only(sections: &[Section]) -> Options {
    Options {
        sections: Some(sections.iter().copied().collect::<BTreeSet<_>>()),
        ..Options::default()
    }
}

/// The lines of the `body` line `head` and the lines indented under it.
fn body<'a>(text: &'a str, head: &str) -> Vec<&'a str> {
    let all: Vec<&str> = text.lines().collect();
    let at = all.iter().position(|l| *l == head).expect("the body");
    let len = all[at + 1..]
        .iter()
        .take_while(|l| l.starts_with(' '))
        .count();
    all[at + 1..=at + len].to_vec()
}

/// The lines of `text` that start with `prefix`.
fn lines<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    text.lines().filter(|l| l.starts_with(prefix)).collect()
}

const PROGRAM: &str = "(defstruct Pt (x: i64 y: i64))
(defun mk (n: i64) (fn (x) (+ x n)))
(defun grow (v: (Vec i64)) -> (Vec i64)
  (loop ((x v))
    (if (> (count x) 3) x (recur (conj x 1)))))
(defun bar (&a) (append &a 2))
(defun f (v: (Vec (Vec i64))) -> i64
  (match v
    ([[x & p] & q] :when (> x 0) (count q))
    ([_ & q] :when false 1)
    (_ 0)))
(defun g () -> i64 (. (Pt 1 2) x))
(defun keep (s: Pt) -> Pt s)
(defun main () -> i64
  (let ((x (cell [3 4])))
    (bar &x)
    (+ (. (keep (Pt 1 2)) y) (count @x))))
";

#[test]
fn a_closure_body_has_params_exprs_bindings_calls_closures_and_allocs() {
    let (text, status) = run("closure", PROGRAM, &only(&[Section::Body]));
    assert_eq!(status, 0);
    assert!(text.starts_with("== D/t.fib\n-- module main D/t.fib\nbody fun mk\n"));
    assert_eq!(
        body(&text, "body fun mk"),
        [
            "  param B# n scalar escapes=0 declared-borrow=0",
            "  expr E# scalar",
            "  expr E# scalar",
            "  expr E# scalar",
            "  expr E# owned",
            "  binding B# n scalar scope-local=0",
            "  binding B# x scalar scope-local=0",
            "  call E# method Num.+ tail=tail head=scalar args=scalar,scalar write-backs= jump=release env:# jump",
            "  closure E# async=0 captures=#:scalar escaping=yes(returned) heap=yes(returned)",
            "    closure-param E# B# x scalar escapes=0 declared-borrow=0",
            "  alloc E# heap",
        ]
    );
}

#[test]
fn a_loop_prints_its_recur_its_after_operations_and_why_a_parameter_is_owned() {
    let (text, _) = run("loop", PROGRAM, &only(&[Section::Body]));
    let grow = body(&text, "body fun grow");
    assert_eq!(
        grow[0],
        "  param B# v owned:loop(through loop variable x) escapes=1 declared-borrow=0"
    );
    assert_eq!(grow[1], "  expr E# borrowed b:# after retain v:# loop-init");
    assert!(grow.contains(&"  expr E# owned after release b:# param-exit"));
    assert!(grow.contains(&"  binding B# x owns scope-local=0"));
    assert_eq!(
        lines(&text, "  recur"),
        ["  recur E# args=move jump=release b:# recur-old"]
    );
}

#[test]
fn an_amp_argument_is_acquired_and_written_back_and_a_forwarded_one_is_not() {
    let (text, _) = run("amp", PROGRAM, &only(&[Section::Body]));
    let bar = body(&text, "body fun bar");
    assert_eq!(bar[0], "  param B# a amp escapes=0 declared-borrow=0");
    assert!(bar.contains(&"  binding B# a amp-param scope-local=0"));
    assert!(bar.contains(
        &"  call E# fun append tail=tail head=scalar args=forward,scalar write-backs= jump="
    ));
    let main = body(&text, "body fun main");
    assert!(main
        .contains(&"  call E# fun bar tail=none head=scalar args=acquire write-backs=0:# jump="));
    assert!(
        main.contains(&"  call E# builtin cell tail=none head=scalar args=move write-backs= jump=")
    );
    assert!(main.contains(&"  binding B# x owns scope-local=1"));
    assert!(main.contains(&"  alloc E# stack"));
}

#[test]
fn a_false_guard_prints_its_edge_and_a_stack_temporary_its_end() {
    let (text, _) = run("guard", PROGRAM, &only(&[Section::Body]));
    assert_eq!(
        lines(&text, "  guard-fail"),
        [
            "  guard-fail E# release b:# scope-exit; release b:# scope-exit",
            "  guard-fail E# release b:# scope-exit",
        ]
    );
    let g = body(&text, "body fun g");
    assert!(g.contains(&"  expr E# scalar after end-stack v:# scope-exit"));
    assert_eq!(&g[g.len() - 2..], ["  alloc E# stack", "  stack-temp E#"]);
}

#[test]
fn a_tail_call_prints_its_callee_its_ordinary_reason_and_its_jump() {
    let (text, _) = run("tail", PROGRAM, &only(&[Section::Body]));
    let calls = lines(&text, "  call");
    assert!(calls.contains(&"  call E# fun count tail=ordinary:frame-owned:0 head=scalar args=borrow write-backs= jump="));
    let last = "  call E# method Num.+ tail=tail head=scalar args=scalar,scalar write-backs= jump=end-stack b:# jump; release v:# jump";
    assert!(calls.contains(&last));
}

#[test]
fn facts_print_the_owned_the_escaping_the_loop_variables_and_the_closures_of_each_unit() {
    let (text, _) = run("facts", PROGRAM, &only(&[Section::Facts]));
    let head = "-- module main D/t.fib\nfacts fun mk\n  escaping E# returned\n  heap E# returned\n";
    assert!(text.contains(head), "{text}");
    let grow = "facts fun grow
  owned B# loop(through loop variable x)
  escapes B# through loop variable x: passed to an escaping parameter
  escapes B# passed to an escaping parameter
  loop-rule1 B#
facts fun bar\n";
    assert!(text.contains(grow), "{text}");
    assert!(text.contains("facts fun keep\n  owned B# rule1(returned)\n  escapes B# returned\n"));
    assert!(!text.contains("body ") && !text.contains("summary "));
}

#[test]
fn summaries_taken_functions_and_taken_methods_print_per_module() {
    let src = "(defprotocol Area (area (self) -> i64))
(defstruct Pt (x: i64))
(impl Area Pt (area (self) (. self x)))
(defun inc (n: i64) -> i64 (+ n 1))
(defun keep (s: Pt) -> Pt s)
(defun main () -> i64 (let ((g inc) (h area) (p (Pt 3))) (+ (g 1) (h (keep p)))))
";
    let (text, status) = run(
        "taken",
        src,
        &only(&[Section::Summary, Section::Taken, Section::Body]),
    );
    assert_eq!(status, 0);
    assert_eq!(
        lines(&text, "summary"),
        ["summary inc (0,0)", "summary keep (1,1)", "summary main"]
    );
    assert_eq!(
        lines(&text, "taken"),
        ["taken value inc", "taken method # area"]
    );
    assert!(text.contains("body allowned inc\n") && text.contains("body method # area\n"));
    assert!(text.contains("body methodowned # area\n  param B# self owned:declared "));
}

#[test]
fn explain_is_the_text_of_fibref_explain_and_is_not_printed_by_default() {
    let dir = Dir::new("explain", &[("t.fib", PROGRAM)]);
    let file = dir.file("t.fib");
    let (text, _) = own_files(std::slice::from_ref(&file), &only(&[Section::Explain]));
    let checked = crate::own::check_source(PROGRAM, &file).expect("accepted");
    let explained = crate::own::explain::explain(&checked.typed, &checked.owned);
    assert_eq!(
        text,
        format!("== {file}\n-- module main {file}\n{explained}")
    );
    let (plain, _) = own_files(&[file], &Options::default());
    assert!(!plain.contains("defun mk :") && plain.contains("body fun mk\n"));
}

/// The dump of `src` as it is, not shown.
fn raw(label: &str, src: &str, opts: &Options) -> (String, u8) {
    let dir = Dir::new(label, &[("t.fib", src)]);
    own_files(&[dir.file("t.fib")], opts)
}

#[test]
fn every_table_is_printed_in_order_of_its_ids() {
    let (text, status) = raw("order", PROGRAM, &Options::default());
    assert_eq!(status, 0);
    let tables = [
        "  expr ",
        "  binding ",
        "  call ",
        "  guard-fail ",
        "  escapes ",
    ];
    for table in tables {
        let mut seen = 0;
        let mut last = 0u32;
        for l in text.lines() {
            if l.starts_with("body ") || l.starts_with("facts ") {
                last = 0;
            }
            let Some(rest) = l.strip_prefix(table) else {
                continue;
            };
            let id: u32 = rest.split(' ').next().expect("an id")[1..]
                .parse()
                .expect("a number");
            assert!(id > last, "{table}{id} follows {last}");
            (last, seen) = (id, seen + 1);
        }
        assert!(seen > 1, "{table} is in the program");
    }
}

#[test]
fn the_same_program_prints_the_same_text_on_every_run() {
    let first = raw("run1", PROGRAM, &only(&Section::ALL)).0;
    for run in 2..5 {
        assert_eq!(
            first,
            raw(&format!("run{run}"), PROGRAM, &only(&Section::ALL))
                .0
                .replace(&format!("run{run}"), "run1")
        );
    }
}

#[test]
fn a_file_without_a_final_newline_prints_what_the_same_file_with_one_does() {
    let src = "(defun main () -> i64 (+ 1 2))";
    let (plain, a) = run("nonl", src, &Options::default());
    let (newline, b) = run("nonl", &format!("{src}\n"), &Options::default());
    assert_eq!((a, b), (0, 0));
    assert_eq!(plain, newline);
    assert!(plain.contains("body fun main\n"));
}

const AMP_TWICE: &str = "(defun bar (&a &b) (append &a 1) (append &b 2))
(defun main () -> i64 (let ((x (cell [3 4]))) (bar &x &x) (count @x)))";
const AMP_IN_ASYNC: &str = "(defun fill (&buf) (async (await (yield)) (append &buf 1)))
(defun main () -> i64 (let ((b (cell []))) (block-on (fill &b)) (count @b)))";
const AMP_CAPTURED: &str = "(defun make-pusher (&v) (fn (x) (append &v x)))
(defun main () -> i64 (let ((v (cell []))) (let ((push (make-pusher &v))) (push 1) (count @v))))";
const BORROW_ESCAPES: &str = "(defun keep (xs: (Vec i64) :borrow) -> (Vec i64) xs)
(defun main () -> i64 (count (keep [1 2])))";
const IMPL_ESCAPES: &str = "(defprotocol Keeper (keep (self x: (Box i64) :borrow) -> (Box i64) x))
(defstruct Jar (n: i64))
(impl Keeper Jar)
(defun main () -> i64 (unbox (keep (Jar 0) (Box 7))))";

#[test]
fn each_ownership_error_is_a_record_with_its_kind_and_no_section() {
    let cases = [
        (AMP_TWICE, "error AmpTwice 2:", "more than one & parameter"),
        (AMP_IN_ASYNC, "error AmpInAsync 1:", "async"),
        (
            AMP_CAPTURED,
            "error AmpCaptured 1:",
            "captured by escaping closure",
        ),
        (BORROW_ESCAPES, "error BorrowEscapes 1:", "declared :borrow"),
        (
            IMPL_ESCAPES,
            "error ImplEscapes 1:",
            "makes parameter x escape",
        ),
    ];
    for (src, head, words) in cases {
        let (text, status) = run("errors", src, &Options::default());
        let (first, rest) = text.split_once("\n").expect("a header");
        assert_eq!((first, status), ("== D/t.fib", 1), "{text}");
        assert!(
            rest.starts_with(head) && rest.contains(words),
            "{head}: {rest}"
        );
        assert!(
            !rest.contains("body ") && !rest.contains("-- module"),
            "{rest}"
        );
    }
}

#[test]
fn the_syntactic_checks_come_before_a_type_error_and_a_type_error_before_the_pass() {
    let src = "(defun bar (&a &b) (append &a 1) (append &b 2))
(defun main () -> i64 (let ((x (cell [3 4]))) (bar &x &x) (count @x)) \"a\")";
    let (both, _) = run("both", src, &Options::default());
    assert!(
        both.contains("error AmpTwice") && !both.contains("error Unify"),
        "{both}"
    );
    let (text, status) = run("typed", "(defun main () -> i64 \"a\")", &Options::default());
    assert_eq!(status, 1);
    assert_eq!(
        text,
        "== D/t.fib\nerror Unify 1:23 22..25: cannot unify str with i64\n"
    );
}

#[test]
fn a_file_that_does_not_expand_prints_the_expanders_record_and_a_missing_file_is_unreadable() {
    let (text, status) = run(
        "expand",
        "(defun main () -> i64 (when))",
        &Options::default(),
    );
    assert_eq!(status, 1);
    assert_eq!(text, "== D/t.fib\n-- module main D/t.fib\nerror MacroArity 1:23 22..28: macro when takes at least 1 argument(s), got 0\n");
    let dir = Dir::new("missing", &[("t.fib", "(defun main () -> i64 0)")]);
    let files = [dir.file("t.fib"), dir.file("none.fib")];
    let (text, status) = own_files(&files, &only(&[Section::Summary]));
    assert_eq!(status, 2);
    assert!(
        !text.contains("body") && text.ends_with("none.fib\nunreadable\n"),
        "{text}"
    );
}

#[test]
fn sections_without_error_leave_a_rejected_file_empty_with_status_one() {
    let (text, status) = run("quiet", BORROW_ESCAPES, &only(&[Section::Body]));
    assert_eq!((text.as_str(), status), ("== D/t.fib\n", 1));
}

#[test]
fn library_needs_no_main_and_prelude_checks_a_file_alone() {
    let src = "(defun twice (n: i64) -> i64 (+ n n))\n";
    let (text, status) = run("nomain", src, &Options::default());
    assert_eq!(status, 1, "{text}");
    let lib = Options {
        library: true,
        ..Options::default()
    };
    let (text, status) = run("library", src, &lib);
    assert_eq!(status, 0);
    assert_eq!(lines(&text, "body"), ["body fun twice"]);
}

#[test]
fn a_file_given_as_a_prelude_is_checked_alone_and_prints_what_the_implicit_prelude_has() {
    let path = format!("{}/../../lib/prelude.fib", env!("CARGO_MANIFEST_DIR"));
    let sections = Some(BTreeSet::from([Section::Body, Section::Summary]));
    let pre = Options {
        prelude: true,
        sections: sections.clone(),
        ..Options::default()
    };
    let (alone, status) = own_files(std::slice::from_ref(&path), &pre);
    assert_eq!(status, 0);
    let (_, alone) = alone.split_once('\n').expect("a header");
    let section = alone
        .strip_prefix(&format!("-- module fib.prelude {path}\n"))
        .expect("its module");
    assert!(
        section.starts_with("body ") && section.contains("\nsummary "),
        "{section}"
    );
    let implicit = Options {
        implicit: true,
        implicit_lib: Some(Vec::new()),
        sections,
        ..Options::default()
    };
    let (text, _) = run("prelude", "(defun main () -> i64 0)\n", &implicit);
    let own = text
        .split("-- module fib.prelude lib/prelude.fib\n")
        .nth(1)
        .expect("the prelude");
    let own = own.split("-- module main").next().expect("up to main");
    assert_eq!(shown(section), own);
}

#[test]
fn implicit_prints_the_prelude_and_the_implicit_modules_first() {
    let src = "(defun main () -> i64 0)\n";
    let one = Options {
        implicit: true,
        implicit_lib: Some(vec!["fib.core".to_string()]),
        sections: Some(BTreeSet::from([Section::Summary])),
        ..Options::default()
    };
    let (text, status) = run("implicit", src, &one);
    assert_eq!(status, 0, "{text}");
    let heads = lines(&text, "-- module");
    assert_eq!(
        heads.first(),
        Some(&"-- module fib.prelude lib/prelude.fib")
    );
    assert!(
        heads.iter().any(|h| h.starts_with("-- module fib.core ")),
        "{heads:?}"
    );
    assert_eq!(heads.last(), Some(&"-- module main D/t.fib"));
    let (plain, _) = run(
        "implicit",
        src,
        &Options {
            implicit: false,
            ..one
        },
    );
    assert_eq!(lines(&plain, "-- module"), ["-- module main D/t.fib"]);
}
