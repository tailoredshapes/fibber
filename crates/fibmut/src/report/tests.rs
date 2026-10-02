use super::*;

fn mutant(op: &'static str, start: usize, end: usize, text: &str) -> Mutant {
    Mutant {
        op,
        start,
        end,
        text: text.to_string(),
    }
}

const SRC: &str = "(ns m)\n(defun f (a: i64)\n  -> i64\n  (+ a 1))\n";

fn done(id: usize, m: Mutant, verdict: Verdict) -> Done {
    Done {
        id,
        line: m.line(SRC),
        mutant: m,
        verdict,
    }
}

fn report() -> Report {
    let plus = SRC.find("(+ a 1)").unwrap() + 1;
    let one = SRC.find("1)").unwrap();
    Report {
        module: "lib/m.fib".to_string(),
        tool: "fibref".to_string(),
        seed: 3,
        src: SRC.to_string(),
        sites: vec![("arith", 1), ("const", 2)],
        filtered: 3,
        selected: 2,
        dropped: vec![("bad".to_string(), "result: expected 0, got 1".to_string())],
        loading: 2,
        done: vec![
            done(
                0,
                mutant("arith", plus, plus + 1, "-"),
                Verdict::Killed {
                    case: "010-x".into(),
                    class: "result",
                    detail: "result: expected 1, got 2".into(),
                },
            ),
            done(1, mutant("const", one, one + 1, "2"), Verdict::Survived),
            done(
                2,
                mutant("const", one, one + 1, "0"),
                Verdict::Invalid("rejected".into()),
            ),
        ],
    }
}

#[test]
fn the_outcome_table_counts_each_kind() {
    let text = squash(&render(&report(), false));
    for row in [
        "killed by result 1",
        "killed by trap 0",
        "killed by timeout 0",
        "survived 1",
        "invalid 1 (does not compile)",
        "killed of killed+survived: 1 of 2",
        "arith 1 1 0 0",
        "const 2 0 1 1",
    ] {
        assert!(text.contains(&format!("\n{row}\n")), "{row}: {text}");
    }
    // a class that killed nothing and is not one of the four always shown is left out
    assert!(!text.contains("killed by allocs"), "{text}");
}

/// The text with every run of spaces made one.
fn squash(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_header_says_what_was_chosen_dropped_and_loaded() {
    let text = render(&report(), false);
    assert!(
        text.starts_with("fibmut: lib/m.fib (tool fibref, seed 3)\n"),
        "{text}"
    );
    assert!(
        text.contains("mutants: 3 in the module (arith 1, const 2); 3 after the filters; 3 run"),
        "{text}"
    );
    assert!(
        text.contains("cases: 3 selected, 2 passed the unmutated module, 2 of them load it"),
        "{text}"
    );
    assert!(
        text.contains("dropped bad: result: expected 0, got 1"),
        "{text}"
    );
}

#[test]
fn a_survivor_comes_with_its_edit_as_a_diff() {
    let text = render(&report(), false);
    assert!(
        text.contains("SURVIVOR m1 [const] lib/m.fib:4\n-   (+ a 1))\n+   (+ a 2))\n"),
        "{text}"
    );
    assert!(
        !text.contains("SURVIVOR m0") && !text.contains("SURVIVOR m2"),
        "{text}"
    );
}

#[test]
fn verbose_gives_a_line_for_every_mutant() {
    let text = render(&report(), true);
    assert!(
        text.contains("m0 arith line 4: killed by 010-x (result)"),
        "{text}"
    );
    assert!(text.contains("m1 const line 4: SURVIVED"), "{text}");
    assert!(
        text.contains("m2 const line 4: invalid: rejected"),
        "{text}"
    );
    assert!(!render(&report(), false).contains("m0 arith line"));
}

#[test]
fn a_diff_of_an_edit_over_several_lines_shows_each_line() {
    let src = "(a\n b)\n(c)\n";
    let m = mutant("branch", 1, 5, "x\n y");
    assert_eq!(diff(src, &m), "- (a\n-  b)\n+ (x\n+  y)\n");
}

#[test]
fn the_counts_read_the_verdicts() {
    let r = report();
    assert_eq!((r.killed(), r.survivors().len(), r.invalid()), (1, 1, 1));
    assert_eq!(r.survivors()[0].id, 1);
}
