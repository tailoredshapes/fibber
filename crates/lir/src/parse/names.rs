//! Which atoms may be names (spec/lir.md §5.3).

use crate::diag::{err, Pos, Result};
use crate::types::scalar_keyword;

const RESERVED: &[&str] = &[
    "null",
    "void",
    "unordered",
    "monotonic",
    "acquire",
    "release",
    "acq_rel",
    "seq_cst",
    "singlethread",
    "weak",
    "inbounds",
    "volatile",
    "align",
    "zeroinitializer",
    "private",
    "internal",
    "external",
    "hidden",
];

/// A type keyword or one of the reserved words.
pub fn is_reserved(s: &str) -> bool {
    RESERVED.contains(&s) || scalar_keyword(s).is_some()
}

fn looks_numeric(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    body.starts_with(|c: char| c.is_ascii_digit()) || matches!(body, "inf" | "nan")
}

/// Check a local or global name at its binding site.
pub fn valid_name(s: &str, pos: Pos) -> Result<String> {
    if s.is_empty() || s.starts_with('@') || s.starts_with('%') || looks_numeric(s) || s == "..." {
        return err(pos, format!("invalid name {s}"));
    }
    if is_reserved(s) {
        return err(pos, format!("reserved word {s} used as a name"));
    }
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_reserved_words() {
        let p = Pos::default();
        assert!(valid_name("fib.unique?", p).is_ok());
        assert!(valid_name("count.Vec.i64", p).is_ok());
        for bad in ["@x", "%x", "12", "-3", "nan"] {
            assert!(valid_name(bad, p)
                .unwrap_err()
                .message
                .contains("invalid name"));
        }
        for r in [
            "i32", "acquire", "null", "ptr", "weak", "volatile", "hidden",
        ] {
            assert!(valid_name(r, p)
                .unwrap_err()
                .message
                .contains("reserved word"));
        }
    }
}
