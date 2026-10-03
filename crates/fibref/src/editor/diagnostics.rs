//! `fibref diagnostics`: the front end's first errors as positions.

use super::analysis::{analyse, Diag};
use super::text::line_col;
use crate::json::Json;
use crate::roots::Roots;

/// The problems in `src`, the text of the file `path`.
pub fn diagnostics(src: &str, path: &str, roots: &Roots) -> Vec<Diag> {
    analyse(src, path, roots).diags
}

/// A diagnostic's range: `(line, col, end line, end col)`, lines from 1
/// and columns from 0 in UTF-16 units. An empty range is widened to the
/// character at it (or the one before it at the end of the text), so
/// that an editor has something to underline.
pub fn range(src: &str, d: &Diag) -> (usize, usize, usize, usize) {
    let mut end = d.end;
    if end <= d.start {
        end = src
            .get(d.start..)
            .and_then(|rest| rest.chars().next())
            .map_or(d.start, |c| d.start + c.len_utf8());
    }
    let (l, c) = line_col(src, d.start);
    let (el, ec) = line_col(src, end);
    (l, c, el, ec)
}

/// `{"diagnostics":[{"line","col","endLine","endCol","message","severity"}]}`.
pub fn diagnostics_json(src: &str, diags: &[Diag]) -> Json {
    let list = diags
        .iter()
        .map(|d| {
            let (l, c, el, ec) = range(src, d);
            Json::obj([
                ("line", Json::int(l)),
                ("col", Json::int(c)),
                ("endLine", Json::int(el)),
                ("endCol", Json::int(ec)),
                ("message", Json::str(&d.message)),
                ("severity", Json::str("error")),
            ])
        })
        .collect();
    Json::obj([("diagnostics", Json::Arr(list))])
}
