//! Generated inputs built from tokens rather than from existing files.
//!
//! Token soup comes in three modes, because an input with one invalid
//! token tells the differential test about that token only (the dump of
//! an error is one line): **clean** (every token valid, delimiters
//! balanced, maps with pairs: the dump is long and tests every kind of
//! node), **one bad** (a clean input with one invalid token spliced in:
//! the error's position and message) and **chaos** (valid and invalid
//! tokens, delimiters that need not match, any separator). **Targeted**
//! inputs end a clean input with a snippet that provokes one named read
//! error. Deep nesting is in `nest.rs`.

use super::rng::Rng;
use super::tables::*;

/// Separators that are whitespace to the reader (a comma followed by
/// whitespace is one too).
const WHITESPACE: [&str; 8] = [" ", "  ", "\n", "\r\n", "\t", ", ", ",\n", "\n\n"];

/// Separators of chaos mode, some of which join tokens or unquote.
const SEPARATORS: [&str; 7] = ["", " ", "\n", "\r\n", "\t", ", ", ","];
const SEPARATOR_WEIGHTS: [usize; 7] = [30, 30, 15, 10, 5, 5, 5];

const PREFIXES: [&str; 6] = ["'", "`", "~", "~@", "@", "&"];

/// Symbols that can follow `&` (which must apply to a symbol).
const SAFE_SYMBOLS: [&str; 6] = ["x", "foo", "a/b", "->", ".", "é"];

/// What a stretch of text starts with, for a few characters that make
/// the column and byte offset differ.
const LEADS: [&str; 4] = ["é ", "; é😀\n", "😀 ", "\"é😀\" "];

/// A random input of token soup in one of the three modes.
pub fn soup(rng: &mut Rng) -> String {
    match rng.weighted(&[6, 2, 2]) {
        0 => clean(rng).text,
        1 => one_bad(rng),
        _ => chaos(rng),
    }
}

/// A clean input and the offsets between its tokens.
struct Built {
    text: String,
    marks: Vec<usize>,
}

/// The state of a clean input being written.
struct Builder {
    text: String,
    /// The delimiters open now: their kind, and the forms read inside.
    open: Vec<(usize, usize)>,
    marks: Vec<usize>,
}

fn clean(rng: &mut Rng) -> Built {
    let mut b = Builder {
        text: preamble(rng),
        open: Vec::new(),
        marks: Vec::new(),
    };
    for _ in 0..rng.between(1, 40) {
        match rng.weighted(&[52, 14, 14, 9, 4, 4]) {
            0 => b.atom(rng, true),
            1 => b.open(rng),
            2 => b.close(rng),
            3 => b.prefixed(rng),
            4 => b.discard(rng),
            _ => b.comment(rng),
        }
    }
    while !b.open.is_empty() {
        b.close(rng);
    }
    match rng.weighted(&[6, 3, 1]) {
        0 => b.text.push('\n'),
        1 => {}
        _ => b.text.push_str("; the end"),
    }
    Built {
        text: b.text,
        marks: b.marks,
    }
}

/// An optional byte order mark and an optional multi-byte lead.
fn preamble(rng: &mut Rng) -> String {
    let mut text = String::new();
    if rng.one_in(10) {
        text.push('\u{FEFF}');
    }
    if rng.one_in(8) {
        text.push_str(rng.pick(&LEADS));
    }
    text
}

impl Builder {
    fn mark(&mut self) {
        self.marks.push(self.text.len());
    }

    /// A form was completed inside the innermost open delimiter.
    fn form_done(&mut self) {
        if let Some(top) = self.open.last_mut() {
            top.1 += 1;
        }
    }

    fn whitespace(&mut self, rng: &mut Rng) {
        self.text.push_str(rng.pick(&WHITESPACE));
    }

    /// A valid atom and whitespace; `counted` is false for an atom that
    /// `#_` discards.
    fn atom(&mut self, rng: &mut Rng, counted: bool) {
        self.text.push_str(valid_atom(rng));
        self.finish_atom(rng, counted);
    }

    fn finish_atom(&mut self, rng: &mut Rng, counted: bool) {
        if counted {
            self.form_done();
        }
        self.whitespace(rng);
        self.mark();
    }

    fn open(&mut self, rng: &mut Rng) {
        let kind = rng.weighted(&[6, 3, 1]);
        self.open.push((kind, 0));
        self.text.push(OPENERS[kind]);
        if rng.one_in(3) {
            self.whitespace(rng);
        }
        self.mark();
    }

    /// Closes the innermost delimiter, first completing a map's pair.
    fn close(&mut self, rng: &mut Rng) {
        let Some((kind, count)) = self.open.last().copied() else {
            return;
        };
        if kind == 2 && count % 2 == 1 {
            self.atom(rng, true);
        }
        self.open.pop();
        self.text.push(CLOSERS[kind]);
        self.form_done();
        if !rng.one_in(3) {
            self.whitespace(rng);
        }
        self.mark();
    }

    /// A prefix and, tight against it, an atom or an opening delimiter.
    fn prefixed(&mut self, rng: &mut Rng) {
        let prefix = rng.pick(&PREFIXES);
        self.text.push_str(prefix);
        if prefix == "&" {
            self.text.push_str(rng.pick(&SAFE_SYMBOLS));
            self.finish_atom(rng, true);
        } else if rng.one_in(4) {
            self.open(rng);
        } else {
            self.atom(rng, true);
        }
    }

    fn discard(&mut self, rng: &mut Rng) {
        self.text.push_str("#_");
        if rng.one_in(2) {
            self.text.push(' ');
        }
        self.atom(rng, false);
    }

    fn comment(&mut self, rng: &mut Rng) {
        self.text.push_str(rng.pick(COMMENTS));
        self.text.push_str(rng.pick(&["\n", "\r\n", "\n"]));
        self.mark();
    }
}

/// A token that reads as one form on its own.
fn valid_atom(rng: &mut Rng) -> &'static str {
    let table = match rng.weighted(&[14, 5, 9, 7, 6, 5, 8]) {
        0 => SYMBOLS_OK,
        1 => KEYWORDS_OK,
        2 => INTS_OK,
        3 => FLOATS_OK,
        4 => STRINGS_OK,
        5 => CHARS_OK,
        _ => LITERALS_OK,
    };
    rng.pick(table)
}

/// A clean input followed by a snippet of group `group` of [`TARGETED`]:
/// the snippet's error is the first the reader meets.
pub fn targeted(rng: &mut Rng, group: usize) -> String {
    let snippets = TARGETED[group % TARGETED.len()];
    format!("{}\n{}", clean(rng).text, rng.pick(snippets))
}

/// A clean input with one invalid token spliced in between two tokens.
fn one_bad(rng: &mut Rng) -> String {
    let mut built = clean(rng);
    let bad = bad_token(rng);
    let at = match built.marks.len() {
        0 => built.text.len(),
        n => built.marks[rng.below(n)],
    };
    built.text.insert_str(at, bad);
    built.text
}

fn bad_token(rng: &mut Rng) -> &'static str {
    let table = match rng.weighted(&[2, 2, 4, 3, 4, 3, 3, 5, 3, 3]) {
        0 => SYMBOLS_BAD,
        1 => KEYWORDS_BAD,
        2 => INTS_BAD,
        3 => FLOATS_BAD,
        4 => STRINGS_BAD,
        5 => CHARS_BAD,
        6 => ODD_BAD,
        7 => MISC_BAD,
        8 => INT_RANGE_BAD,
        _ => FLOAT_RANGE_BAD,
    };
    rng.pick(table)
}

/// Valid and invalid tokens, delimiters that need not match, any separator.
fn chaos(rng: &mut Rng) -> String {
    let mut out = preamble(rng);
    let mut open: Vec<usize> = Vec::new();
    for _ in 0..rng.between(1, 60) {
        let token = chaos_token(rng, &mut open);
        out.push_str(&token);
        out.push_str(SEPARATORS[rng.weighted(&SEPARATOR_WEIGHTS)]);
    }
    if rng.one_in(3) {
        out.push('\n');
    }
    out
}

fn either(
    rng: &mut Rng,
    ok: &'static [&'static str],
    bad: &'static [&'static str],
) -> &'static str {
    let table = if rng.one_in(3) { bad } else { ok };
    rng.pick(table)
}

fn chaos_token(rng: &mut Rng, open: &mut Vec<usize>) -> String {
    let text = match rng.weighted(&[14, 5, 9, 7, 8, 5, 3, 10, 9, 6, 3, 3, 3, 2]) {
        0 => either(rng, SYMBOLS_OK, SYMBOLS_BAD),
        1 => either(rng, KEYWORDS_OK, KEYWORDS_BAD),
        2 => either(rng, INTS_OK, INTS_BAD),
        3 => either(rng, FLOATS_OK, FLOATS_BAD),
        4 => either(rng, STRINGS_OK, STRINGS_BAD),
        5 => either(rng, CHARS_OK, CHARS_BAD),
        6 => rng.pick(LITERALS_OK),
        7 => return chaos_opener(rng, open),
        8 => return chaos_closer(rng, open),
        9 => rng.pick(&PREFIXES),
        10 => "#_",
        11 => rng.pick(MISC_BAD),
        12 => {
            return format!(
                "{}{}",
                rng.pick(COMMENTS),
                rng.pick(&["\n", "\r\n", "\n", ""])
            )
        }
        _ => rng.pick(ODD_BAD),
    };
    text.to_string()
}

fn chaos_opener(rng: &mut Rng, open: &mut Vec<usize>) -> String {
    let kind = rng.weighted(&[6, 3, 1]);
    open.push(kind);
    OPENERS[kind].to_string()
}

/// A closer: the matching one most of the time, any other now and then.
fn chaos_closer(rng: &mut Rng, open: &mut Vec<usize>) -> String {
    let kind = match open.pop() {
        Some(kind) if !rng.one_in(6) => kind,
        _ => rng.below(CLOSERS.len()),
    };
    CLOSERS[kind].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(text: &str) -> String {
        fibref::dump::dump_source(text, "soup")
    }

    fn reads(text: &str) -> bool {
        !dump(text).starts_with("error ")
    }

    #[test]
    fn every_ok_token_reads_and_every_bad_token_fails() {
        let ok = [
            SYMBOLS_OK,
            KEYWORDS_OK,
            INTS_OK,
            FLOATS_OK,
            STRINGS_OK,
            CHARS_OK,
            LITERALS_OK,
        ];
        for table in ok {
            for token in table {
                assert!(reads(token), "{token:?} should read: {}", dump(token));
                assert!(reads(&format!("x {token} y")), "{token:?} in context");
            }
        }
        let bad = [
            SYMBOLS_BAD,
            KEYWORDS_BAD,
            INTS_BAD,
            FLOATS_BAD,
            STRINGS_BAD,
            CHARS_BAD,
            ODD_BAD,
            MISC_BAD,
            INT_RANGE_BAD,
            FLOAT_RANGE_BAD,
        ];
        for table in bad {
            for token in table {
                let text = format!("x {token}");
                assert!(!reads(&text), "{token:?} should not read: {}", dump(&text));
            }
        }
        for comment in COMMENTS {
            assert!(reads(&format!("{comment}\nx")), "{comment:?}");
        }
    }

    #[test]
    fn every_targeted_snippet_provokes_the_error_of_its_group() {
        assert_eq!(TARGETED.len(), TARGETED_KINDS.len());
        for (group, kind) in TARGETED_KINDS.iter().enumerate() {
            for snippet in TARGETED[group] {
                let first = dump(&format!("x\n{snippet}"));
                let want = format!("error {kind} ");
                assert!(first.starts_with(&want), "{snippet:?}: {first}");
            }
            for seed in 0..20 {
                let text = targeted(&mut Rng::new(seed), group);
                let first = dump(&text);
                assert!(
                    first.starts_with(&format!("error {kind} ")),
                    "{text:?}: {first}"
                );
            }
        }
    }

    #[test]
    fn clean_soup_always_reads_and_uses_every_kind_of_node() {
        let mut nodes = std::collections::BTreeSet::new();
        for seed in 0..300 {
            let text = clean(&mut Rng::new(seed)).text;
            let d = dump(&text);
            assert!(!d.starts_with("error "), "seed {seed}: {text:?}\n{d}");
            nodes.extend(
                d.lines()
                    .map(|l| l.split_whitespace().next().unwrap_or("").to_string()),
            );
        }
        for node in [
            "sym", "kw", "int", "flt", "str", "chr", "bool", "nil", "list", "vec", "map",
        ] {
            assert!(nodes.contains(node), "no clean input has a {node}");
        }
    }

    #[test]
    fn one_bad_token_usually_makes_the_input_fail() {
        let failing = (0..300)
            .filter(|seed| !reads(&one_bad(&mut Rng::new(*seed))))
            .count();
        assert!(failing > 150, "only {failing} of 300 fail");
    }
}
