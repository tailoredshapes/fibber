use std::collections::BTreeSet;
use std::path::PathBuf;

use super::*;

/// A scratch directory holding `files` (name, text), removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(label: &str, files: &[(&str, &str)]) -> Dir {
        let dir = std::env::temp_dir().join(format!("fibref-types-{label}-{}", std::process::id()));
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

    /// `text` with the directory made `D`.
    fn shown(&self, text: &str) -> String {
        text.replace(&self.0.to_string_lossy().into_owned(), "D")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The options of a dump with no implicit module: these tests are about
/// the program's own definitions, and the library's would fill the dump.
fn plain() -> Options {
    Options {
        implicit_lib: Some(Vec::new()),
        ..Options::default()
    }
}

fn with(f: impl FnOnce(&mut Options)) -> Options {
    let mut opts = plain();
    f(&mut opts);
    opts
}

fn only(sections: &[Section]) -> Options {
    with(|o| o.sections = Some(sections.iter().copied().collect()))
}

/// The dump of `names` in `dir`, the directory made `D`, and the status.
fn run(dir: &Dir, names: &[&str], opts: &Options) -> (String, u8) {
    let files: Vec<String> = names.iter().map(|n| dir.file(n)).collect();
    let (text, status) = types_files(&files, opts);
    (dir.shown(&text), status)
}

/// The dump of the one program `src`.
fn dump(label: &str, src: &str, opts: &Options) -> (String, u8) {
    let dir = Dir::new(label, &[("t.fib", src)]);
    run(&dir, &["t.fib"], opts)
}

/// `text` with the ids of expressions and bindings erased (`E#`, `B#`),
/// which follow the size of the prelude.
fn erase_ids(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut prev = ' ';
    while let Some(c) = chars.next() {
        out.push(c);
        if (c == 'E' || c == 'B')
            && !prev.is_alphanumeric()
            && chars.peek().is_some_and(char::is_ascii_digit)
        {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
            out.push('#');
        }
        prev = c;
    }
    out
}

/// The first words of the lines that are not indented.
fn heads(text: &str) -> BTreeSet<String> {
    let first = |l: &str| l.split(' ').next().unwrap_or("").to_string();
    text.lines()
        .filter(|l| !l.starts_with(' '))
        .map(first)
        .collect()
}

fn set(words: &[&str]) -> BTreeSet<String> {
    words.iter().map(|w| w.to_string()).collect()
}

const EX: &str = "(defstruct Sq (n: i64))
(defenum Shape (Circle r: i64) (Rect w: i64 h: i64) Empty)
(defprotocol Area (area (self) -> i64))
(impl Area Sq (area (self) (* (. self n) (. self n))))
(extern puts (ptr) -> i32)
(def limit 10)
(defun twice (f x) (f (f x)))
(defun main () -> i64
  (twice (fn (n) (+ n 1)) (area (Sq limit))))
";

#[test]
fn an_accepted_program_prints_its_declarations_schemes_and_units_in_one_section() {
    let (text, status) = dump("ex", EX, &plain());
    assert_eq!(status, 0);
    let expected = "== D/t.fib
-- module main D/t.fib
type Sq () struct 1:1 0..23
  field n : i64
type Shape () enum 2:1 24..82
  variant Circle (i64)
  variant Rect (i64 i64)
  variant Empty ()
protocol Area (Self) 3:1 83..122 supers
  method area : ∀Self. (Area Self) ⇒ (fn :send (Self) i64) params self
instance 81 (Area Sq) vars 0 4:1 123..177
  method area : ∀Self. (Area Self) ⇒ (fn :send (Self) i64) params self
fun twice : ∀a ς0. (fn :send ((fn ς0 (a) a) a) a) params f x 7:1 220..249
fun main : (fn :send () i64) params 8:1 250..317
def limit : i64 6:1 205..219
extern puts : (fn :send (ptr) i32) 5:1 178..204
unit scc twice
unit def limit
unit scc main
unit impl 81 area
";
    assert_eq!(text, expected);
}

#[test]
fn each_section_prints_alone_and_only_its_own_lines() {
    let cases = [
        (Section::Type, "type"),
        (Section::Protocol, "protocol"),
        (Section::Instance, "instance"),
        (Section::Fun, "fun"),
        (Section::Def, "def"),
        (Section::Extern, "extern"),
        (Section::Unit, "unit"),
    ];
    for (section, head) in cases {
        let (text, status) = dump("alone", EX, &only(&[section]));
        assert_eq!(status, 0);
        let want = set(&["==", "--", head]);
        assert_eq!(heads(&text), want, "--sections {}", section.name());
    }
    // The error section of an accepted program has nothing in it.
    let (text, _) = dump("alone", EX, &only(&[Section::Error]));
    assert_eq!(text, "== D/t.fib\n-- module main D/t.fib\n");
    // Two sections keep the order of the dump, not of the list.
    let (text, _) = dump("alone", EX, &only(&[Section::Unit, Section::Def]));
    let order: Vec<&str> = text.lines().skip(2).collect();
    assert_eq!(
        order,
        [
            "def limit : i64 6:1 205..219",
            "unit scc twice",
            "unit def limit",
            "unit scc main",
            "unit impl 81 area"
        ]
    );
}

#[test]
fn stage_lower_prints_the_declared_signatures_with_holes_and_no_units() {
    let lower = with(|o| o.stage = Stage::Lower);
    let (text, status) = dump("lower", EX, &lower);
    assert_eq!(status, 0);
    assert!(text.contains("fun twice : (fn (_ _) _) params f x 7:1 220..249\n"));
    assert!(text.contains("fun main : (fn () i64) params 8:1 250..317\n"));
    assert!(text.contains("def limit : _ 6:1 205..219\n"));
    assert!(text.contains("extern puts : (fn :send (ptr) i32) 5:1 178..204\n"));
    assert!(!text.contains("unit "), "{text}");
    // Annotations, `&` parameters and bounds show as written.
    let src =
        "(defun f (a: i64 &c: (Cell i64)) -> bool :where ((Eq a)) true) (defun main () -> i64 0)";
    let (text, _) = dump("lower2", src, &lower);
    assert!(
        text.contains("fun f : (fn (i64 (& (Cell i64))) bool) where (Eq a) params a &c "),
        "{text}"
    );
}

#[test]
fn the_ast_section_prints_every_body_one_node_a_line_depth_first() {
    let opts = with(|o| {
        o.stage = Stage::Lower;
        o.sections = Some(BTreeSet::new());
        o.ast = true;
    });
    let (text, status) = dump("ast", EX, &opts);
    assert_eq!(status, 0);
    let expected = "== D/t.fib
-- module main D/t.fib
ast fun twice
  B# param f 7:15 234..235
  B# param x 7:17 236..237
  E# call 1 7:20 239..248
    E# local B# f 7:21 240..241
    E# call 1 7:23 242..247
      E# local B# f 7:24 243..244
      E# local B# x 7:26 245..246
ast fun main
  ret i64
  E# call 2 9:3 274..316
    E# global fun twice 9:4 275..280
    E# fn 1 captures[] 9:10 281..297
      B# param n 9:15 286..287
      E# call 2 9:18 289..296
        E# global method Num + 9:19 290..291
        E# local B# n 9:21 292..293
        E# lit int 1 i64 9:23 294..295
    E# call 1 9:27 298..315
      E# global method Area area 9:28 299..303
      E# call 1 9:33 304..314
        E# global ctor Sq - 9:34 305..307
        E# global def limit 9:37 308..313
ast def limit
  E# lit int 10 i64 6:12 216..218
ast impl 81 area
  B# param self 4:22 144..148
  E# call 2 4:28 150..175
    E# global method Num * 4:29 151..152
    E# field n \"self\" 4:31 153..163
      E# local B# self 4:34 156..160
    E# field n \"self\" 4:42 164..174
      E# local B# self 4:45 167..171
";
    // The positions above are of this text; the ids are erased.
    assert_eq!(erase_ids(&text), expected);
}

#[test]
fn the_tables_section_prints_types_instantiations_resolutions_and_colours_under_each_unit() {
    let opts = with(|o| {
        o.sections = Some(BTreeSet::from([Section::Tables]));
    });
    let (text, status) = dump("tables", EX, &opts);
    assert_eq!(status, 0);
    let text = erase_ids(&text);
    assert_eq!(text.matches("-- builtins\n").count(), 1);
    assert!(text.contains("builtin 0 "), "{text}");
    let body = &text[text.find("== D/t.fib").expect("the file")..];
    for line in [
        "unit scc twice\n  binding B# f param (fn ς0 (a) a)\n",
        "  inst E# i64 | send\n",
        "  res E# instance 81\n",
        "  fn E# send\n",
        "  binding B# n param i64\n",
        "unit impl 81 area\n  binding B# self param Sq\n",
    ] {
        assert!(body.contains(line), "{line:?} in\n{body}");
    }
    // Once per run, not per file.
    let dir = Dir::new("tables2", &[("a.fib", EX), ("b.fib", EX)]);
    let (text, _) = run(&dir, &["a.fib", "b.fib"], &opts);
    assert_eq!(text.matches("-- builtins\n").count(), 1);
    assert!(text.starts_with("-- builtins\n"));
}

#[test]
fn the_library_flag_needs_no_main_and_without_it_a_missing_main_is_an_error() {
    let src = "(defun f () -> i64 1)";
    let (text, status) = dump("lib", src, &plain());
    assert_eq!(
        text,
        "== D/t.fib\nerror Other 0:0 0..0@<builtin>: the program has no main\n"
    );
    assert_eq!(status, 1);
    let (text, status) = dump("lib", src, &with(|o| o.library = true));
    assert_eq!(status, 0);
    assert!(
        text.contains("fun f : (fn :send () i64) params 1:1 0..21\n"),
        "{text}"
    );
}

#[test]
fn the_implicit_library_has_no_section_unless_asked_and_the_prelude_has_one_with_it() {
    let default = Options::default();
    let (text, status) = dump("implicit", EX, &default);
    assert_eq!(status, 0);
    assert_eq!(
        text.matches("-- module ").count(),
        1,
        "the main module only"
    );
    let all = Options {
        implicit: true,
        sections: Some(BTreeSet::from([Section::Fun])),
        ..Options::default()
    };
    let (text, status) = dump("implicit", EX, &all);
    assert_eq!(status, 0);
    let modules: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("-- module "))
        .collect();
    assert_eq!(modules[0], "-- module fib.prelude lib/prelude.fib");
    assert_eq!(modules.last(), Some(&"-- module main D/t.fib"));
    assert!(
        modules.contains(&"-- module fib.core lib/fib/core.fib"),
        "{modules:?}"
    );
    // An implicit list names the library of this dump.
    let list = Options {
        implicit: true,
        implicit_lib: Some(vec!["fib.char".to_string()]),
        ..Options::default()
    };
    let (text, _) = dump("implicit", EX, &list);
    assert!(text.contains("-- module fib.char "), "{text}");
    assert!(!text.contains("-- module fib.core "));
}

#[test]
fn a_file_given_as_a_prelude_is_checked_alone_and_prints_the_section_the_implicit_prelude_has() {
    let path = format!("{}/../../lib/prelude.fib", env!("CARGO_MANIFEST_DIR"));
    let sections = [
        Section::Type,
        Section::Protocol,
        Section::Instance,
        Section::Fun,
    ];
    let as_prelude = Options {
        prelude: true,
        sections: Some(BTreeSet::from(sections)),
        ..Options::default()
    };
    let (alone, status) = types_files(std::slice::from_ref(&path), &as_prelude);
    assert_eq!(status, 0);
    let (_, rest) = alone.split_once('\n').expect("a header");
    assert!(rest.starts_with(&format!("-- module fib.prelude {path}\ntype List (a) enum")));
    let implicit = Options {
        implicit: true,
        sections: Some(BTreeSet::from(sections)),
        implicit_lib: Some(Vec::new()),
        ..Options::default()
    };
    let dir = Dir::new("prelude", &[("t.fib", EX)]);
    let (text, _) = run(&dir, &["t.fib"], &implicit);
    let section = text
        .split("-- module main")
        .next()
        .expect("the prelude first");
    let body = |t: &str| t.split_once("\ntype").map(|(_, b)| b.to_string());
    assert_eq!(
        body(rest),
        body(section),
        "the same lines, under another file name"
    );
}

#[test]
fn a_file_that_does_not_read_load_or_expand_prints_the_expanders_record() {
    let files = [
        ("rd.fib", "(defun main () -> i64 (+ 1 \n"),
        ("ex.fib", "(defun main () -> i64 (when))\n"),
        (
            "ld.fib",
            "(ns m (:use no.such.mod))\n(defun main () -> i64 0)\n",
        ),
    ];
    let dir = Dir::new("records", &files);
    let (text, status) = run(&dir, &["rd.fib"], &plain());
    assert_eq!(
        text,
        "== D/rd.fib\nerror Unclosed 1:23 22..23: unclosed (\n"
    );
    assert_eq!(status, 1);
    let (text, status) = run(&dir, &["ex.fib"], &plain());
    let want = "== D/ex.fib\n-- module main D/ex.fib\nerror MacroArity 1:23 22..28: macro when takes at least 1 argument(s), got 0\n";
    assert_eq!((text.as_str(), status), (want, 1));
    let (text, status) = run(&dir, &["ld.fib"], &plain());
    let want = "== D/ld.fib\nerror ModuleMissing 0:0 0..0: module no.such.mod is not at D/no/such/mod.fib\n";
    assert_eq!((text.as_str(), status), (want, 1));
}

#[test]
fn an_unreadable_file_is_reported_and_the_largest_status_wins() {
    let dir = Dir::new(
        "status",
        &[("ok.fib", "(defun main () -> i64 0)"), ("bad.fib", "")],
    );
    std::fs::write(dir.file("bad.fib"), [0xff, 0xfe]).expect("writable");
    std::fs::write(dir.file("err.fib"), "(defun main () -> i64 nope)").expect("writable");
    let one = |names: &[&str]| run(&dir, names, &plain());
    assert_eq!(one(&["ok.fib"]).1, 0);
    assert_eq!(one(&["ok.fib", "err.fib"]).1, 1);
    let (text, status) = one(&["err.fib", "bad.fib", "ok.fib"]);
    assert_eq!(status, 2);
    assert!(
        text.contains("== D/bad.fib\nunreadable\n== D/ok.fib\n-- module main"),
        "{text}"
    );
    assert_eq!(
        one(&["missing.fib"]),
        ("== D/missing.fib\nunreadable\n".to_string(), 2)
    );
}

mod errors;
