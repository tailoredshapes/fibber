//! Tests of the emit dump: each section and each option once, the records
//! of every way a file can stop, and that the text `fibc emit` prints is
//! the sections put together.

use std::thread;

use fibref::eval::STACK_BYTES;
use fibref::roots::Roots;
use fibref::types::CHECK_STACK;

use super::sections::{fn_name, HEADER};
use super::*;
use crate::compile::compile;
use crate::front::{check, Front};

/// A program with something in every section: an `extern` that is called
/// (externs), strings (statics), a keyword that is shown (keywords), a
/// `def` of a struct (defs), a quoted form (quotes), a struct, an enum,
/// options and a closure (types), and a macro.
const SOURCE: &str = "(extern abs (i32) -> i32)
(def greeting \"hello\")
(defstruct P (x: i64 s: str))
(def origin (P 9 \"z\"))
(defenum Shape (Circle r: i64) (Rect w: i64 h: i64) Empty)
(defmacro twice (x) (List [(Sym \"do\") x x]))
(defun shape (f: Form) -> i64
  (match f ((List [(Sym \"do\") & steps]) (count steps)) (_ 0)))
(defun sq (x: i64) -> i64 (* x x))
(defun kw (k: keyword) -> str (show k))
(defun area (s: Shape) -> i64
  (match s ((Circle r) r) ((Rect w h) (* w h)) (Empty 0)))
(defun main () -> i64
  (let ((p (P 4 \"ab\")) (o (some 3)) (q (some \"x\")) (f (fn (n) (+ n 1))))
    (do (kw :alpha)
        (twice (sq 2))
        (+ (shape (quote (do a b))) (area (Rect 2 3)))
        (+ (str-len greeting) (+ (. origin x) (+ (f (. p x)) (unsafe (sext i64 (abs -3i32)))))))))";

/// `fibc emit-dump`'s text and status for one program (named `t.fib`), on
/// a thread with the stack the expander and the checker need.
fn dump(source: &str, opts: &Options) -> (String, u8) {
    thread::scope(|scope| {
        thread::Builder::new()
            .stack_size(STACK_BYTES.max(CHECK_STACK))
            .spawn_scoped(scope, || {
                let one = dump_file(source, "t.fib", &Roots::default(), opts);
                (one.text, one.status)
            })
            .expect("a thread")
            .join()
            .expect("no panic")
    })
}

fn only(words: &[&str]) -> Options {
    let line: Vec<String> = ["--sections".to_string(), words.join(","), "f".to_string()].into();
    parse_args(&line).expect("options").0
}

/// The text cut at the section lines: each section's name and body.
fn split(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        match line.strip_prefix(HEADER) {
            Some(name) => out.push((name.trim_end().to_string(), String::new())),
            None => out.last_mut().expect("a header first").1.push_str(line),
        }
    }
    out
}

/// The functions of a `fns` section: each from its `(define` line on.
fn functions(body: &str) -> Vec<&str> {
    let starts: Vec<usize> = body
        .match_indices("(define ")
        .map(|(i, _)| i)
        .filter(|i| *i == 0 || body.as_bytes()[i - 1] == b'\n')
        .collect();
    let ends = starts.iter().skip(1).copied().chain([body.len()]);
    starts.iter().zip(ends).map(|(a, b)| &body[*a..b]).collect()
}

#[test]
fn the_sections_put_together_are_the_text_fibc_emit_prints_and_the_same_on_every_run() {
    let (text, status) = dump(SOURCE, &Options::default());
    assert_eq!(status, 0, "{text}");
    let names: Vec<String> = split(&text).into_iter().map(|(n, _)| n).collect();
    let all: Vec<&str> = Section::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(names, all);
    let Front::Checked(checked) = check(SOURCE, "t.fib") else {
        panic!("the front end rejected the program");
    };
    let emitted = compile(&checked).expect("compiles");
    let bare: String = text
        .split_inclusive('\n')
        .filter(|l| !l.starts_with(HEADER))
        .collect();
    assert!(bare == emitted, "the sections are not the text of `emit`");
    // Every run hashes its tables with new keys: a second run in this
    // process prints the same.
    assert!(dump(SOURCE, &Options::default()).0 == text);
}

#[test]
fn each_section_alone_is_its_part_of_the_text_and_is_not_empty_here() {
    let (all, _) = dump(SOURCE, &Options::default());
    let parts = split(&all);
    for (name, body) in &parts {
        assert!(!body.is_empty(), "section {name} is empty for the program");
        assert!(Section::from_name(name).is_some());
        let (alone, status) = dump(SOURCE, &only(&[name.as_str()]));
        assert_eq!(status, 0);
        assert_eq!(split(&alone), vec![(name.clone(), body.clone())], "{name}");
    }
}

#[test]
fn fn_keeps_the_functions_whose_mangled_name_starts_with_the_prefix() {
    let (all, _) = dump(SOURCE, &only(&["fns"]));
    let every = functions(&split(&all)[0].1).len();
    let opts = |words: &[&str]| {
        let line: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        parse_args(&line).expect("options").0
    };
    // `--fn` alone prints the functions and nothing else.
    let (text, _) = dump(SOURCE, &opts(&["--fn", "f.s", "f"]));
    let parts = split(&text);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].0, "fns");
    let kept = functions(&parts[0].1);
    assert!(!kept.is_empty() && kept.len() < every);
    assert!(kept
        .iter()
        .all(|f| fn_name(f).unwrap_or("").starts_with("f.s")));
    // Every function with that prefix is kept: the filter loses none.
    let named = functions(&split(&all)[0].1)
        .into_iter()
        .filter(|f| fn_name(f).unwrap_or("").starts_with("f.s"))
        .count();
    assert_eq!(kept.len(), named);
    // No function has the prefix: the section is there and empty.
    let (none, _) = dump(SOURCE, &opts(&["--fn", "no.such", "f"]));
    assert_eq!(none, format!("{HEADER}fns\n"));
    // With `--sections` the others print too.
    let (two, _) = dump(
        SOURCE,
        &opts(&["--sections", "main,fns", "--fn", "f.s", "f"]),
    );
    let names: Vec<String> = split(&two).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["fns", "main"]);
}

/// The columns of the layout line of the type printed `name`.
fn layout_line<'t>(text: &'t str, name: &str) -> &'t str {
    let head = format!("{name} mangle ");
    text.lines()
        .find(|l| l.starts_with(&head))
        .unwrap_or_else(|| panic!("no layout line for {name}"))
}

#[test]
fn layout_prints_the_ground_types_and_the_type_table_and_no_body() {
    let (text, status) = dump(
        SOURCE,
        &parse_args(&["--layout".into(), "f".into()]).expect("").0,
    );
    assert_eq!(status, 0, "{text}");
    let headers: Vec<&str> = text.lines().filter(|l| l.starts_with(";; ==")).collect();
    assert_eq!(headers, [";; == layout", ";; == section types"]);
    assert!(!text.contains("tailcc (f."), "a body was emitted");
    // The columns, by the rules of types §8 worked by hand: a header of 16
    // bytes, then the fields at their alignments.
    assert_eq!(
        layout_line(&text, "i64"),
        "i64 mangle i64 lir i64 class scalar repr i64 object -"
    );
    assert_eq!(
        layout_line(&text, "unit"),
        "unit mangle unit lir void class unit repr unit object -"
    );
    assert_eq!(
        layout_line(&text, "str"),
        "str mangle str lir ptr class ptr repr str object fib.str tid 0 size 24 offsets -"
    );
    assert_eq!(
        layout_line(&text, "(Option str)"),
        "(Option str) mangle $fib.builtin/Option.str_ lir ptr class opt \
         repr $fib.builtin/Option.str_ object -"
    );
    let boxed = layout_line(&text, "(Option i64)");
    assert!(boxed.contains(" class boxed "), "{boxed}");
    assert!(boxed.ends_with(" size 32 offsets v0:- v1:24"), "{boxed}");
    let (head, rest) = layout_line(&text, "P")
        .split_once(" tid ")
        .expect("an object");
    assert_eq!(head, "P mangle P lir ptr class ptr repr str object o.P");
    let (tid, tail) = rest.split_once(' ').expect("columns");
    assert!(tid.parse::<u32>().expect("a number") > 3, "{tid}");
    assert_eq!(tail, "size 32 offsets 16,24");
    let shape = layout_line(&text, "Shape");
    assert!(
        shape.ends_with(" size 40 offsets v0:24 v1:24,32 v2:-"),
        "{shape}"
    );
    let closure = layout_line(&text, "(fn :local (i64) i64)");
    assert!(
        closure.ends_with(" class ptr repr str object -"),
        "{closure}"
    );
    // The objects of the table are the tids of the lines.
    let table = text.split(";; == section types\n").nth(1).expect("types");
    assert!(
        table.contains("(defstruct o.P (i64 i32 i32 i64 ptr))"),
        "{table}"
    );
    assert!(table.contains("(constant internal fib.types "));
}

/// Types accept this and the ownership pass does not.
const BORROW_ESCAPES: &str = "(defun keep (xs: (Vec i64) :borrow) -> (Vec i64) xs)
(defun main () -> i64 (count (keep [1 2])))";

#[test]
fn layout_needs_no_ownership_plan_and_the_sections_do() {
    let (text, status) = dump(BORROW_ESCAPES, &Options::default());
    assert_eq!(status, 1);
    assert!(text.starts_with("error BorrowEscapes 1:"), "{text}");
    assert!(!text.contains(HEADER));
    let layout = parse_args(&["--layout".into(), "f".into()])
        .expect("options")
        .0;
    let (text, status) = dump(BORROW_ESCAPES, &layout);
    assert_eq!(status, 0, "{text}");
    assert!(text.starts_with(";; == layout\n"));
}

#[test]
fn macro_prints_the_macro_time_module_of_one_macro() {
    let opts = parse_args(&["--macro".into(), "twice".into(), "f".into()])
        .expect("")
        .0;
    let (text, status) = dump(SOURCE, &opts);
    assert_eq!(status, 0, "{text}");
    let (head, module) = text.split_once('\n').expect("a header");
    assert_eq!(head, ";; == macro main/twice");
    assert!(module.contains("fibm.init.0"), "no entry points");
    if let Err(e) = lir::parse_and_check(module) {
        panic!("the macro module does not check: {}", e[0]);
    }
    // The key names it too.
    let by_key = parse_args(&["--macro".into(), "main/twice".into(), "f".into()]).expect("");
    assert_eq!(dump(SOURCE, &by_key.0).0, text);
    let missing = parse_args(&["--macro".into(), "nope".into(), "f".into()]).expect("");
    assert_eq!(
        dump(SOURCE, &missing.0),
        ("error NoMacro 0:0 0..0: no macro nope\n".to_string(), 1)
    );
}

#[test]
fn every_way_a_file_stops_is_a_record_or_a_word_and_a_status() {
    let plain = Options::default();
    // The type checker's record, as `fibref types` prints it.
    assert_eq!(
        dump("(defun main () -> i64 \"a\")", &plain),
        (
            "error Unify 1:23 22..25: cannot unify str with i64\n".to_string(),
            1
        )
    );
    // The expander's, under the module's line (§6.6).
    assert_eq!(
        dump("(defun main () -> i64 (when))", &plain),
        (
            "-- module main t.fib\nerror MacroArity 1:23 22..28: macro when takes at least 1 \
             argument(s), got 0\n"
                .to_string(),
            1
        )
    );
    // The reader's.
    let (text, status) = dump("(defun main () -> i64 (+ 1 2)", &plain);
    assert!(
        text.starts_with("error ") && !text.contains(HEADER),
        "{text}"
    );
    assert_eq!(status, 1);
    // A program the compiler cannot lower yet: the first word of the
    // message, and 3.
    let (text, status) = dump(
        "(defprotocol Sz (size (self) -> i64))
         (defstruct (W a) (x: a))
         (impl Sz i64 (size (self) 1))
         (impl Sz (W a) :where ((Sz a))
           (size (self) (if false (size (W (W (. self x)))) (size (. self x)))))
         (defun main () -> i64 (size (W 5)))",
        &plain,
    );
    assert_eq!((text.as_str(), status), ("unsupported the\n", 3));
    // A file that cannot be read.
    let files = ["/nonexistent/e0.fib".to_string()];
    assert_eq!(
        emit_files(&files, &Roots::default(), &plain),
        ("== /nonexistent/e0.fib\nunreadable\n".to_string(), 2)
    );
}
