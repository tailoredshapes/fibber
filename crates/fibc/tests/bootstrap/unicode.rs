//! Inputs for the character classes the reader decides by hand.
//!
//! The Rust reader leans on `char::is_whitespace` and `char::is_control`
//! (syntax §1.1: which characters may not appear outside strings and
//! comments); the fibber port has to spell out those Unicode tables, and
//! a code point wrong in a table shows only on an input that has it. So
//! every scalar value those two predicates accept goes into a small file,
//! with its neighbours in code point order (an error at a range's edge)
//! and a few near misses (a byte order mark, zero-width characters), in
//! three places: between two symbol characters, as a character literal,
//! and inside a string. Look-alikes of ASCII digits go into the places
//! digits are read, to show that only ASCII digits count.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Scalars that look like a class member but are not one.
const NEAR_MISSES: [char; 12] = [
    '\u{FEFF}',
    '\u{200B}',
    '\u{200C}',
    '\u{200D}',
    '\u{2060}',
    '\u{180E}',
    '\u{FFFE}',
    '\u{FFFF}',
    '\u{10FFFF}',
    '\u{D7FF}',
    '\u{E000}',
    '\u{301}',
];

/// Scalars that are not ASCII digits or hex digits but look like them.
const LOOKALIKES: [char; 10] = [
    '\u{663}',
    '\u{FF11}',
    '\u{B2}',
    '\u{BD}',
    '\u{2167}',
    '\u{FF21}',
    '\u{FF41}',
    '\u{966}',
    '\u{1D7CF}',
    '\u{2460}',
];

/// Where a look-alike goes, `{}` standing for it.
const DIGIT_PLACES: [&str; 9] = [
    "1{}",
    "0x{}",
    "0b{}",
    "\"\\u{{{}}}\"",
    "\"\\x{}0\"",
    "1e{}",
    "1.{}",
    "-{}",
    "{}1",
];

/// The scalars the whitespace and control predicates accept, each one's
/// neighbours, and the near misses.
pub fn class_scalars() -> BTreeSet<char> {
    let members: Vec<char> = (0..=0x10FFFFu32)
        .filter_map(char::from_u32)
        .filter(|c| c.is_whitespace() || c.is_control())
        .collect();
    let mut all: BTreeSet<char> = members.iter().copied().collect();
    for c in &members {
        let n = u32::from(*c);
        all.extend(char::from_u32(n.wrapping_sub(1)));
        all.extend(char::from_u32(n + 1));
    }
    all.extend(NEAR_MISSES);
    all
}

/// Writes the inputs into `dir`: three files for each class scalar and
/// one for each look-alike in each digit place.
pub fn files(dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).expect("the scratch directory is writable");
    let mut paths = Vec::new();
    let mut write = |name: String, text: String| {
        let path = dir.join(format!("{name}.fib"));
        std::fs::write(&path, text).expect("the scratch directory is writable");
        paths.push(path);
    };
    for c in class_scalars() {
        let hex = format!("{:04X}", u32::from(c));
        write(format!("class-{hex}-between"), format!("a{c}b\n"));
        write(format!("class-{hex}-char"), format!("\\{c}\n"));
        write(format!("class-{hex}-string"), format!("\"a{c}b\" ; {c}\n"));
    }
    for c in LOOKALIKES {
        for (i, place) in DIGIT_PLACES.iter().enumerate() {
            let hex = format!("{:04X}", u32::from(c));
            write(
                format!("lookalike-{hex}-{i}"),
                place.replace("{}", &c.to_string()),
            );
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    #[test]
    fn the_scalars_include_the_unicode_spaces_the_controls_and_their_edges() {
        let all = class_scalars();
        for c in [
            '\0', '\t', '\n', '\u{B}', '\r', ' ', '\u{7F}', '\u{85}', '\u{9F}', '\u{A0}',
            '\u{1680}', '\u{2000}', '\u{200A}', '\u{2028}', '\u{2029}', '\u{202F}', '\u{205F}',
            '\u{3000}', '\u{FEFF}', '\u{200B}', '~', '\u{A1}', '\u{1FFF}', '\u{200B}', '\u{3001}',
        ] {
            assert!(all.contains(&c), "U+{:04X} is missing", u32::from(c));
        }
        assert!(all.len() > 100, "{} scalars", all.len());
    }

    #[test]
    fn the_files_are_small_and_the_reader_both_reads_and_rejects_them() {
        let dir = TempDir::new("unicode-files");
        let files = files(dir.path());
        assert!(files.len() > 400, "{} files", files.len());
        let (mut read, mut failed) = (0, 0);
        for f in &files {
            let text = std::fs::read_to_string(f).expect("UTF-8");
            assert!(text.len() < 64, "{}", f.display());
            let dump = fibref::dump::dump_source(&text, "u");
            if dump.starts_with("error ") {
                failed += 1;
            } else {
                read += 1;
            }
        }
        assert!(read > 150 && failed > 150, "{read} read, {failed} fail");
    }
}
