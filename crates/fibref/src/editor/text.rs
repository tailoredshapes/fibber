//! Positions in a buffer. `fibref complete` and `fibref diagnostics`
//! count lines from 1 (LSP from 0) and columns from 0, in UTF-16 code
//! units, which is a character for any text of the Basic Multilingual
//! Plane. Everything inside works in byte offsets.

/// The byte offset of `line` (1-based) and `col` (0-based, UTF-16
/// units) in `src`; a position past the end of its line is the end of
/// the line, one past the last line the end of the text.
pub fn offset_of(src: &str, line: usize, col: usize) -> usize {
    let mut start = 0;
    for _ in 1..line.max(1) {
        match src[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return src.len(),
        }
    }
    let mut units = 0;
    for (i, c) in src[start..].char_indices() {
        if c == '\n' || units >= col {
            return start + i;
        }
        units += c.len_utf16();
    }
    src.len()
}

/// The line (1-based) and column (0-based, UTF-16 units) of byte offset
/// `off`, clamped to the text and to a character boundary.
pub fn line_col(src: &str, off: usize) -> (usize, usize) {
    let mut off = off.min(src.len());
    while !src.is_char_boundary(off) {
        off -= 1;
    }
    let before = &src[..off];
    let line = before.matches('\n').count() + 1;
    let from = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[from..].encode_utf16().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_and_positions_agree_across_lines_and_wide_characters() {
        let src = "ab\nc\u{e9}\u{1F600}d\n";
        for off in [0, 1, 2, 3, 4, 6, 10, 11] {
            let (l, c) = line_col(src, off);
            assert_eq!(offset_of(src, l, c), off, "{off} -> {l}:{c}");
        }
        assert_eq!(line_col(src, 10), (2, 4));
    }

    #[test]
    fn positions_past_the_end_clamp() {
        assert_eq!(offset_of("ab\ncd", 1, 99), 2);
        assert_eq!(offset_of("ab\ncd", 9, 0), 5);
    }
}
