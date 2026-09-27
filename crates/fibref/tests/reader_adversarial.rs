//! Adversarial tests for the reader (`fibref::syntax`, spec/syntax.md
//! §1): pathological nesting, huge numbers, escape edges, line endings,
//! a BOM, non-ASCII symbols, comments at the very end, form comments in
//! odd places, and nested quasiquotes.
//!
//! Nesting contract: the reader uses an explicit stack, not recursion,
//! and rejects nesting deeper than `MAX_DEPTH` with `TooDeep`. 100 000
//! levels of any nesting construct must return that error, not overflow.

use fibref::syntax::{read_all, Form, FormKind, IntWidth, ReadErrorKind, MAX_DEPTH};

const HUGE: usize = 100_000;

fn kind_of_error(src: &str) -> ReadErrorKind {
    match read_all(src, "adv") {
        Ok(forms) => panic!("read {} forms from {} bytes", forms.len(), src.len()),
        Err(e) => e.kind,
    }
}

fn show(src: &str) -> String {
    let forms = read_all(src, "adv").unwrap_or_else(|e| panic!("{src:?}: {e}"));
    forms
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

fn too_deep() -> ReadErrorKind {
    ReadErrorKind::TooDeep { limit: MAX_DEPTH }
}

#[test]
fn a_hundred_thousand_levels_of_each_nesting_construct() {
    // A run of commas with no form after it is whitespace (§1.2).
    assert!(read_all(&",".repeat(HUGE), "adv")
        .expect("reads")
        .is_empty());
    for open in ["(", "[", "{", "'", "`", "@", "#_", ",@", "' ", "#_ "] {
        let src = open.repeat(HUGE);
        assert_eq!(kind_of_error(&src), too_deep(), "{open:?} x {HUGE}");
        let closed = format!("{}x{}", open.repeat(HUGE), ")".repeat(HUGE));
        assert_eq!(kind_of_error(&closed), too_deep(), "{open:?}x) x {HUGE}");
    }
    let unquotes = format!("{}x", ",".repeat(HUGE));
    assert_eq!(kind_of_error(&unquotes), too_deep());
    let balanced = format!("{}{}", "(".repeat(HUGE), ")".repeat(HUGE));
    assert_eq!(kind_of_error(&balanced), too_deep());
    let mixed = "([{'@".repeat(HUGE / 5);
    assert_eq!(kind_of_error(&mixed), too_deep());
}

#[test]
fn the_deepest_accepted_form_prints_and_compares() {
    let src = format!("{}x{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
    let forms = read_all(&src, "adv").expect("MAX_DEPTH levels read");
    assert_eq!(forms[0].to_string(), src);
    assert_eq!(
        read_all(&forms[0].to_string(), "adv").expect("reads"),
        forms
    );
    let quoted = format!("{}x", "'".repeat(MAX_DEPTH));
    assert_eq!(read_all(&quoted, "adv").expect("reads").len(), 1);
    let one_more = format!("{}x", "'".repeat(MAX_DEPTH + 1));
    assert_eq!(kind_of_error(&one_more), too_deep());
}

#[test]
fn wide_and_long_inputs_are_fine() {
    let wide = format!("({})", "x ".repeat(HUGE));
    let forms = read_all(&wide, "adv").expect("wide list");
    assert_eq!(forms[0].as_list().map(<[Form]>::len), Some(HUGE));
    let long = "(a) ".repeat(HUGE);
    assert_eq!(read_all(&long, "adv").expect("many forms").len(), HUGE);
    let closers = ")".repeat(HUGE);
    assert_eq!(
        kind_of_error(&closers),
        ReadErrorKind::UnexpectedClose { found: ')' }
    );
}

#[test]
fn huge_integers() {
    let big = "9".repeat(10_000);
    let out = |t: &str, width| ReadErrorKind::IntegerOutOfRange {
        text: t.to_string(),
        width,
    };
    assert_eq!(kind_of_error(&big), out(&big, IntWidth::I64));
    let neg = format!("-{big}i8");
    assert_eq!(kind_of_error(&neg), out(&neg, IntWidth::I8));
    let hex = format!("0x{}", "F".repeat(1000));
    assert_eq!(kind_of_error(&hex), out(&hex, IntWidth::I64));
    let underscored = format!("1{}", "_000".repeat(6));
    assert_eq!(show(&underscored), "1000000000000000000");
    let over = format!("10{}", "_000".repeat(6));
    assert_eq!(kind_of_error(&over), out(&over, IntWidth::I64));
    assert_eq!(show("-9223372036854775808"), "-9223372036854775808");
    assert_eq!(show("-0b1000_0000i8"), "-128i8");
    let zeros = format!("{}1", "0".repeat(10_000));
    assert_eq!(show(&zeros), "1");
    let float = format!("1{}.0", "0".repeat(400));
    assert!(matches!(
        kind_of_error(&float),
        ReadErrorKind::FloatOutOfRange { .. }
    ));
}

fn string(src: &str) -> String {
    match read_all(src, "adv").map(|f| f[0].kind.clone()) {
        Ok(FormKind::Str(s)) => s,
        other => panic!("{src:?}: {other:?}"),
    }
}

#[test]
fn escape_edges() {
    assert_eq!(string(r#""\\\\""#), "\\\\");
    assert_eq!(string(r#""\\n""#), "\\n");
    assert_eq!(string(r#""\"\"""#), "\"\"");
    assert_eq!(string(r#""\x7F\x00\x0a""#), "\u{7F}\0\n");
    assert_eq!(string(r#""\u{000041}""#), "A");
    assert_eq!(string(r#""\u{10ffff}""#), "\u{10FFFF}");
    assert_eq!(string(r#""\u{E000}\u{D7FF}""#), "\u{E000}\u{D7FF}");
    assert_eq!(string("\"\\\\\n\""), "\\\n");
    // The escape ends the string only if the quote is not escaped.
    assert_eq!(string(r#""a\\" "#), "a\\");
    for bad in [
        r#""\u{DFFF}""#,
        r#""\u{D800}""#,
        r#""\u{0000041}""#,
        r#""\u{-1}""#,
        r#""\u{ 41}""#,
        r#""\u{41""#,
    ] {
        let k = kind_of_error(bad);
        assert!(
            matches!(k, ReadErrorKind::BadUnicodeEscape { .. }),
            "{bad}: {k:?}"
        );
    }
    for bad in [
        r#""\x""#,
        r#""\x8""#,
        r#""\x80""#,
        r#""\X41""#,
        r#""\U{41}""#,
        "\"\\\u{A0}\"",
    ] {
        let k = kind_of_error(bad);
        assert!(matches!(k, ReadErrorKind::BadEscape { .. }), "{bad}: {k:?}");
    }
    // A backslash at the very end leaves the string open.
    assert_eq!(kind_of_error("\"abc\\"), ReadErrorKind::UnterminatedString);
    // Characters: the escapes are separate from string escapes.
    assert_eq!(show(r"\u{0}"), r"\u{0}");
    assert_eq!(show(r"\u{20}"), r"\space");
    assert_eq!(show(r#"[\" \\ \; \u]"#), r#"[\" \\ \; \u]"#);
}

#[test]
fn crlf_line_endings() {
    let lf = ";; header\n(defun f (x)\n  [1 2])\n";
    let crlf = lf.replace('\n', "\r\n");
    let a = read_all(lf, "lf").expect("lf");
    let b = read_all(&crlf, "crlf").expect("crlf");
    assert_eq!(a, b);
    let items = b[0].as_list().expect("list");
    assert_eq!((b[0].pos.line, b[0].pos.col), (2, 1));
    assert_eq!((items[3].pos.line, items[3].pos.col), (3, 3));
    assert_eq!(&crlf[items[3].pos.start..items[3].pos.end], "[1 2]");
    // A comment ending in CRLF does not swallow the next line.
    assert_eq!(show("a ; c\r\nb"), "a b");
    // CR alone separates tokens.
    assert_eq!(show("a\rb"), "a b");
}

#[test]
fn byte_order_mark() {
    assert_eq!(show("\u{FEFF};; header\n(a)"), "(a)");
    assert_eq!(show("\u{FEFF}"), "");
    // Only at offset 0: a second BOM, or one after whitespace, is invalid.
    let k = ReadErrorKind::InvalidCharacter('\u{FEFF}');
    assert_eq!(kind_of_error("\u{FEFF}\u{FEFF}(a)"), k);
    assert_eq!(kind_of_error(" \u{FEFF}(a)"), k);
    assert_eq!(kind_of_error("(a\u{FEFF})"), k);
    // Inside a string it is data.
    assert_eq!(string("\"\u{FEFF}\""), "\u{FEFF}");
}

#[test]
fn non_ascii_symbols_follow_section_1_1() {
    for s in [
        "λ",
        "λx:",
        "café/naïve",
        "数据",
        "😀",
        "x→y",
        "Ω-1",
        "ÿ",
        "a\u{301}",
    ] {
        assert_eq!(show(s), s);
        assert_eq!(read_all(s, "adv").expect("reads")[0].as_sym(), Some(s));
    }
    // Non-ASCII digits do not start numbers; only 0-9 do.
    assert_eq!(show("٣"), "٣");
    // Unicode whitespace is not a separator and not a symbol character.
    for ws in ['\u{A0}', '\u{2003}', '\u{3000}', '\u{85}', '\u{2029}'] {
        let src = format!("a{ws}b");
        assert_eq!(kind_of_error(&src), ReadErrorKind::InvalidCharacter(ws));
    }
    // Zero-width characters are not whitespace: they are constituents.
    assert_eq!(show("a\u{200B}b"), "a\u{200B}b");
}

#[test]
fn comments_at_end_of_file_without_newline() {
    assert_eq!(show("(a) ; trailing"), "(a)");
    assert_eq!(show("(a);"), "(a)");
    assert_eq!(show(";"), "");
    assert_eq!(show("x #_y"), "x");
    assert_eq!(show("x #_(y z)"), "x");
    assert_eq!(
        kind_of_error("(a ; unclosed"),
        ReadErrorKind::Unclosed { open: '(' }
    );
    assert_eq!(kind_of_error("x #_"), ReadErrorKind::DiscardWithoutForm);
    assert_eq!(kind_of_error("x #_ ; c"), ReadErrorKind::DiscardWithoutForm);
    assert_eq!(
        kind_of_error("\"; not a comment"),
        ReadErrorKind::UnterminatedString
    );
}

#[test]
fn form_comment_before_a_closing_delimiter() {
    for src in [
        "(a #_)",
        "[#_]",
        "{#_}",
        "(a #_ )",
        "(a #_ ;c\n)",
        "(a #_#_b)",
        "('#_)",
    ] {
        let k = kind_of_error(src);
        assert!(
            matches!(
                k,
                ReadErrorKind::DiscardWithoutForm | ReadErrorKind::PrefixWithoutForm(_)
            ),
            "{src}: {k:?}"
        );
    }
    assert_eq!(kind_of_error("(a #_)"), ReadErrorKind::DiscardWithoutForm);
    assert_eq!(show("(a #_b)"), "(a)");
    assert_eq!(show("(a #_#_b c)"), "(a)");
    assert_eq!(show("{#_k}"), "{}");
    assert_eq!(show("[#_[#_x y] z]"), "[z]");
}

#[test]
fn quasiquote_nesting() {
    assert_eq!(show("```x"), "(quasiquote (quasiquote (quasiquote x)))");
    assert_eq!(
        show("`(a `(b ,(c ,d)))"),
        "(quasiquote (a (quasiquote (b (unquote (c (unquote d)))))))"
    );
    assert_eq!(show(",@,@x"), "(unquote-splicing (unquote-splicing x))");
    assert_eq!(show("`,'x"), "(quasiquote (unquote (quote x)))");
    assert_eq!(
        show("`(,@[1 2] ,@{})"),
        "(quasiquote ((unquote-splicing [1 2]) (unquote-splicing {})))"
    );
    // The reader does not check that unquote is inside a quasiquote:
    // that is the expander's job (§3.16).
    assert_eq!(show(",x ,@y"), "(unquote x) (unquote-splicing y)");
    let nested = format!("{}x", "`,".repeat(MAX_DEPTH / 2));
    assert_eq!(read_all(&nested, "adv").expect("reads").len(), 1);
}

#[test]
fn errors_never_panic_on_arbitrary_prefixes_of_real_input() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../cases/ownership/19-weak-parent-pointer.fib"
    ))
    .expect("case 19");
    let mut errors = 0;
    for (i, _) in text.char_indices() {
        errors += usize::from(read_all(&text[..i], "cut").is_err());
        errors += usize::from(read_all(&text[i..], "cut").is_err());
    }
    assert!(errors > 0, "truncation must produce some errors");
}
