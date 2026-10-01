use std::path::PathBuf;

use super::*;

/// A scratch directory holding `files` (name, text), removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(label: &str, files: &[(&str, &str)]) -> Dir {
        let dir =
            std::env::temp_dir().join(format!("fibref-expand-{label}-{}", std::process::id()));
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

fn run(dir: &Dir, name: &str, opts: &Options) -> (String, u8) {
    expand_files(&[dir.file(name)], opts)
}

/// The text of `run` with the directory made `D`, so expectations can be
/// written down.
fn shown(dir: &Dir, name: &str, opts: &Options) -> (String, u8) {
    let (text, status) = run(dir, name, opts);
    (
        text.replace(&dir.0.to_string_lossy().into_owned(), "D"),
        status,
    )
}

#[test]
fn a_program_prints_its_expanded_forms_under_its_module_and_the_positions_of_the_source() {
    let dir = Dir::new(
        "simple",
        &[("m.fib", "(defun f (x: i64) -> i64 (when x 1 2))\n")],
    );
    let (text, status) = shown(&dir, "m.fib", &Options::default());
    assert_eq!(status, 0);
    let expected = "== D/m.fib
-- module main D/m.fib
list 6 1:1 0..38
  sym \"defun\" 1:2 1..6
  sym \"f\" 1:8 7..8
  list 2 1:10 9..17
    sym \"x:\" 1:11 10..12
    sym \"i64\" 1:14 13..16
  sym \"->\" 1:19 18..20
  sym \"i64\" 1:22 21..24
  list 4 1:26 25..37
    sym \"if\" 1:26 25..37
    sym \"x\" 1:32 31..32
    list 3 1:26 25..37
      sym \"do\" 1:26 25..37
      int 1 i64 1:34 33..34
      int 2 i64 1:36 35..36
    list 0 1:26 25..37
";
    assert_eq!(text, expected);
}

#[test]
fn the_expansion_of_the_prelude_is_the_context_a_program_derives_over() {
    let source = "(defstruct P (a: i64))\n(derive Eq P)\n(derive Eq Option)\n";
    let dir = Dir::new("derive", &[("m.fib", source)]);
    let (text, status) = run(&dir, "m.fib", &Options::default());
    assert_eq!(status, 0, "{text}");
    assert_eq!(text.matches("sym \"impl\"").count(), 2, "{text}");
}

#[test]
fn an_expansion_error_is_one_record_with_the_variant_and_the_message() {
    let dir = Dir::new("err", &[("m.fib", "(defun f () -> i64 1)\n(f)\n")]);
    let (text, status) = shown(&dir, "m.fib", &Options::default());
    assert_eq!(status, 1);
    assert_eq!(
        text,
        "== D/m.fib\n-- module main D/m.fib\n\
         error ExpressionAtTopLevel 2:1 22..25: expression at top level\n"
    );
}

#[test]
fn a_file_that_does_not_read_is_the_record_of_the_reader_dump() {
    let dir = Dir::new("read", &[("m.fib", "(a\n")]);
    let (text, status) = shown(&dir, "m.fib", &Options::default());
    assert_eq!(status, 1);
    assert_eq!(text, "== D/m.fib\nerror Unclosed 1:1 0..1: unclosed (\n");
    let file = dir.file("m.fib");
    assert_eq!(
        text.replace('D', &dir.0.to_string_lossy()).lines().nth(1),
        Some("error Unclosed 1:1 0..1: unclosed (")
    );
    assert_eq!(
        crate::dump::dump_source("(a\n", &file),
        "error Unclosed 1:1 0..1: unclosed (\n"
    );
}

#[test]
fn a_file_that_cannot_be_read_is_unreadable_and_the_largest_status_wins() {
    let dir = Dir::new(
        "unreadable",
        &[("bad.fib", "(f)\n"), ("ok.fib", "(defun f () -> i64 1)\n")],
    );
    std::fs::write(dir.0.join("not-utf8.fib"), b"(a \xFF)").expect("writable");
    let files = [
        dir.file("ok.fib"),
        dir.file("bad.fib"),
        dir.file("not-utf8.fib"),
    ];
    let (text, status) = expand_files(&files, &Options::default());
    assert_eq!(status, 2);
    assert!(text.ends_with("unreadable\n"), "{text}");
    assert_eq!(expand_files(&files[..2], &Options::default()).1, 1);
    assert_eq!(expand_files(&files[..1], &Options::default()).1, 0);
    assert_eq!(
        expand_files(&[dir.file("absent.fib")], &Options::default()).1,
        2
    );
}

const WITH_MACRO: &str = "(defmacro twice (x) `(+ ,x ,x))\n(defun main () -> i64 (twice 21))\n";

#[test]
fn a_user_macro_runs_in_the_evaluator_and_is_pending_without_a_runner() {
    let dir = Dir::new("macro", &[("m.fib", WITH_MACRO)]);
    let (text, status) = run(&dir, "m.fib", &Options::default());
    assert_eq!(status, 0, "{text}");
    assert!(
        text.contains("sym \"+\" 2:23 "),
        "the call's position builds the form: {text}"
    );
    assert!(
        text.contains("sym \"defmacro\" 1:2 "),
        "the definition stays: {text}"
    );
    let none = Options {
        runner: RunnerKind::None,
        ..Options::default()
    };
    let (text, status) = shown(&dir, "m.fib", &none);
    assert_eq!(status, 1);
    assert!(
        text.ends_with(
            "error MacroNeedsEvaluator 2:23 54..64: macro twice needs the evaluator to expand (pending)\n"
        ),
        "{text}"
    );
}

#[test]
fn modules_print_in_dependency_order_each_under_its_own_file() {
    let main = "(ns main (:use u))\n(defun main () -> i64 (one))\n";
    let dir = Dir::new(
        "modules",
        &[
            ("main.fib", main),
            ("u.fib", "(ns u)\n(defun one () -> i64 1)\n"),
        ],
    );
    let (text, status) = shown(&dir, "main.fib", &Options::default());
    assert_eq!(status, 0, "{text}");
    let headers: Vec<&str> = text.lines().filter(|l| l.starts_with("-- ")).collect();
    assert_eq!(
        headers,
        ["-- module u D/u.fib", "-- module main D/main.fib"]
    );
}

#[test]
fn a_program_that_cannot_be_loaded_is_a_record_of_its_kind() {
    let dir = Dir::new(
        "load",
        &[
            ("missing.fib", "(ns main (:use nowhere))\n"),
            ("mismatch.fib", "(ns main (:use v))\n"),
            ("v.fib", "(ns w)\n"),
            ("cycle.fib", "(ns main (:use c))\n"),
            ("c.fib", "(ns c (:use c))\n"),
            ("badns.fib", "(ns main (:use 1))\n"),
            ("deep.fib", "(ns main (:use e))\n"),
            ("e.fib", "(ns e (:use"),
        ],
    );
    let first_record = |name: &str| {
        let (text, status) = shown(&dir, name, &Options::default());
        assert_eq!(status, 1, "{name}: {text}");
        text.lines().nth(1).expect("a record").to_string()
    };
    assert_eq!(
        first_record("missing.fib"),
        "error ModuleMissing 0:0 0..0: module nowhere is not at D/nowhere.fib"
    );
    assert_eq!(
        first_record("mismatch.fib"),
        "error ModuleMismatch 0:0 0..0: D/v.fib declares (ns w) but is required as v"
    );
    assert_eq!(
        first_record("cycle.fib"),
        "error ModuleCycle 0:0 0..0: modules require each other in a cycle: main -> c -> c"
    );
    assert_eq!(
        first_record("badns.fib"),
        "error BadNs 1:1 0..18: a :use item is a module name"
    );
    assert_eq!(
        first_record("deep.fib"),
        "error Unclosed 1:7 6..7@D/e.fib: unclosed ("
    );
}

#[test]
fn the_prelude_mode_dumps_the_expanders_own_prelude_then_the_file_in_one_module() {
    let dir = Dir::new(
        "prelude",
        &[("lib.fib", "(defstruct Box (v: i64))\n(derive Eq Box)\n")],
    );
    let opts = Options {
        prelude: true,
        ..Options::default()
    };
    let (text, status) = shown(&dir, "lib.fib", &opts);
    assert_eq!(status, 0, "{text}");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("== D/lib.fib"));
    assert_eq!(lines.next(), Some("-- module fib.prelude D/lib.fib"));
    assert!(text.contains("@<prelude>\n"), "{text}");
    assert!(text.contains("sym \"Box\" 1:12 11..14\n"), "{text}");
    let source = std::fs::read_to_string(dir.file("lib.fib")).expect("readable");
    let own = dump_forms_in(&expand_prelude_forms(), &dir.file("lib.fib"));
    assert!(
        text.contains(&own.replace(&dir.0.to_string_lossy().into_owned(), "D")),
        "{text}"
    );
    assert!(!source.is_empty());
}

fn expand_prelude_forms() -> Vec<Form> {
    expand_prelude(&mut ExpandCtx::new()).expect("the prelude expands")
}

#[test]
fn the_context_lists_what_the_module_registered_in_sorted_order() {
    let source = "(defstruct Zed :private (a: i64 b))\n(defenum E (A x: i64) (B))\n\
                  (defmacro mm (p ... r) p)\n(defmacro aa () 1)\n";
    let dir = Dir::new("context", &[("m.fib", source)]);
    let opts = Options {
        context: true,
        runner: RunnerKind::None,
        ..Options::default()
    };
    let (text, status) = shown(&dir, "m.fib", &opts);
    assert_eq!(status, 0, "{text}");
    let at = text.find("-- context main\n").expect("a context section");
    let ctx: Vec<&str> = text[at..].lines().collect();
    assert_eq!(ctx[0], "-- context main");
    assert!(ctx[1].starts_with("gensyms "), "{ctx:?}");
    assert_eq!(&ctx[2..4], ["counters steps 0 forms 0", "scope \"main\""]);
    assert!(ctx.contains(&"private \"Zed\""), "{ctx:?}");
    let names = |kind: &str| -> Vec<String> {
        let prefix = format!("{kind} ");
        ctx.iter()
            .filter_map(|l| l.strip_prefix(&prefix))
            .map(str::to_string)
            .collect()
    };
    for kind in ["macro", "struct", "enum"] {
        let all = names(kind);
        let mut sorted = all.clone();
        sorted.sort();
        assert_eq!(all, sorted, "{kind}s are sorted by name");
    }
    assert_eq!(names("macro"), ["\"main/aa\" public", "\"main/mm\" public"]);
    assert_eq!(names("struct"), ["\"Zed\""], "the prelude's are not listed");
    assert_eq!(names("enum"), ["\"E\""]);
    assert!(ctx.contains(&"  rest \"r\""), "{ctx:?}");
    assert!(ctx.contains(&"  params \"p\""), "{ctx:?}");
    assert!(
        ctx.contains(&"  params"),
        "no trailing space for none: {ctx:?}"
    );
    assert!(ctx.contains(&"      field \"x\""), "{ctx:?}");
    assert!(ctx.contains(&"    variant \"B\" 0"), "{ctx:?}");
}

#[test]
fn a_program_lists_what_differs_from_the_prelude_and_the_prelude_lists_everything() {
    // `Box` is the prelude's; the program defines it again, differently.
    let source = "(defstruct Box (w: str))\n(defstruct Mine (a: i64))\n";
    let dir = Dir::new("baseline", &[("m.fib", source)]);
    let program = Options {
        context: true,
        ..Options::default()
    };
    let (text, _) = run(&dir, "m.fib", &program);
    let structs: Vec<&str> = text.lines().filter(|l| l.starts_with("struct ")).collect();
    assert_eq!(structs, ["struct \"Box\"", "struct \"Mine\""]);
    assert!(
        !text.contains("enum \"Vec\""),
        "an unchanged prelude type is not listed"
    );
    let lib = Options {
        prelude: true,
        context: true,
        ..Options::default()
    };
    let (text, _) = run(&dir, "m.fib", &lib);
    for listed in [
        "enum \"Option\"",
        "enum \"Form\"",
        "enum \"List\"",
        "struct \"Box\"",
    ] {
        assert!(text.lines().any(|l| l == listed), "{listed}");
    }
    assert!(text.lines().count() < 5000, "{}", text.lines().count());
}

#[test]
fn the_context_of_a_module_program_adds_up_and_shows_the_scope_of_each_module() {
    let util = "(ns util)\n(defmacro twice (x) `(do ,x ,x))\n(defstruct S (a: i64))\n";
    let main = "(ns main (:require [util :as u]) (:use util))\n(defstruct T (b: i64))\n";
    let dir = Dir::new("ctx-modules", &[("main.fib", main), ("util.fib", util)]);
    let opts = Options {
        context: true,
        runner: RunnerKind::None,
        ..Options::default()
    };
    let (text, status) = shown(&dir, "main.fib", &opts);
    assert_eq!(status, 0, "{text}");
    let sections: Vec<&str> = text.lines().filter(|l| l.starts_with("-- ")).collect();
    assert_eq!(
        sections,
        [
            "-- module util D/util.fib",
            "-- context util",
            "-- module main D/main.fib",
            "-- context main"
        ]
    );
    let last = &text[text.rfind("-- context main").expect("a section")..];
    assert!(
        last.contains("scope \"main\"\n  use \"util\"\n  alias \"u\" \"util\"\n"),
        "{last}"
    );
    for listed in [
        "macro \"util/twice\" public",
        "struct \"S\"",
        "struct \"T\"",
    ] {
        assert!(last.lines().any(|l| l == listed), "{listed}: {last}");
    }
}

#[test]
fn the_context_counts_the_gensyms_and_the_steps_of_the_last_form() {
    let dir = Dir::new(
        "counts",
        &[(
            "m.fib",
            "(defun f (n: i64) -> i64 (dotimes (i n) (when true 1)))\n",
        )],
    );
    let opts = Options {
        context: true,
        ..Options::default()
    };
    let (text, _) = run(&dir, "m.fib", &opts);
    // The prelude's expansion has used some gensyms already; `dotimes` one.
    let used = |source: &str| {
        let dir = Dir::new("counts-base", &[("m.fib", source)]);
        let (text, _) = run(&dir, "m.fib", &opts);
        let line = text
            .lines()
            .find(|l| l.starts_with("gensyms "))
            .expect("a count");
        line["gensyms ".len()..].parse::<u64>().expect("a number")
    };
    let base = used("(defun f (n: i64) -> i64 n)\n");
    assert_eq!(
        used("(defun f (n: i64) -> i64 (dotimes (i n) 1))\n"),
        base + 1
    );
    assert!(text.contains(&format!("gensyms {}\n", base + 1)), "{text}");
    assert!(text.contains("counters steps 2 forms "), "{text}");
}

#[test]
fn smaller_limits_than_the_defaults_reach_their_errors_on_small_inputs() {
    let source = "(defun f () -> i64 (when true (when true (when true 1))))\n";
    let dir = Dir::new("limits", &[("m.fib", source)]);
    let one = |limits| {
        let opts = Options {
            limits,
            ..Options::default()
        };
        let (text, status) = run(&dir, "m.fib", &opts);
        (text.lines().last().unwrap_or("").to_string(), status)
    };
    let (line, status) = one(LimitOverrides {
        steps: Some(2),
        ..LimitOverrides::default()
    });
    assert_eq!(status, 1);
    assert!(line.starts_with("error TooManySteps 1:"), "{line}");
    assert!(
        line.ends_with("more than 2 macro expansions in one top-level form"),
        "{line}"
    );
    let (line, _) = one(LimitOverrides {
        depth: Some(3),
        ..LimitOverrides::default()
    });
    assert!(
        line.starts_with("error TooDeep 1:") && line.ends_with("deeper than 3 levels"),
        "{line}"
    );
    let (line, _) = one(LimitOverrides {
        forms: Some(5),
        ..LimitOverrides::default()
    });
    assert!(line.starts_with("error TooLarge 1:"), "{line}");
    let (line, status) = one(LimitOverrides::default());
    assert_eq!(status, 0);
    assert!(!line.starts_with("error"), "{line}");
}

#[test]
fn every_expansion_error_has_the_name_of_its_variant() {
    let k = ExpandErrorKind::MacroArity {
        name: "m".into(),
        expected: "1".into(),
        found: 2,
    };
    assert_eq!(expand_kind_name(&k), "MacroArity");
    assert_eq!(expand_kind_name(&ExpandErrorKind::NsNotFirst), "NsNotFirst");
    assert_eq!(
        expand_kind_name(&ExpandErrorKind::BadLiteral {
            error: crate::syntax::ReadErrorKind::DiscardWithoutForm
        }),
        "BadLiteral"
    );
}
