//! Reading the function table of `spec/stdlib.md` §4.
//!
//! A row is a line that starts `| ` and has at least six cells when split
//! on the pipes that are not escaped (`\|`: a signature holds them). The
//! cells are the Clojure name, the verdict, the fibber spelling, the
//! signature, the tranche `T` and the note. The lines between the heading
//! `## 4.` and `## 5.` are the table; a `### 4.n` heading names the section
//! the rows below belong to. The header row of each section and its
//! `|---|` rule have a `T` that is no number and are skipped; any other row
//! with six cells whose `T` is no number is an error, so a typo in the table
//! cannot hide a row. Rows with fewer cells are the not-offered list of §4.17.

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The `### 4.n` heading the row is under.
    pub section: String,
    /// The Clojure name: cell 1 without its code span and without `(new)`.
    pub name: String,
    /// The first symbol of the fibber spelling, cell 3.
    pub spelling: String,
    /// The tranche that delivers the row.
    pub tranche: u8,
    /// The line of the spec, 1-based.
    pub line: usize,
}

/// The cells of a table line, split on the pipes that are not escaped,
/// without the text before the first pipe and the blank text after the last.
pub fn split_cells(line: &str) -> Vec<String> {
    let mut pieces = vec![String::new()];
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '|' {
            pieces.push(String::new());
            continue;
        }
        let escaped_pipe = c == '\\' && chars.peek() == Some(&'|');
        if let Some(piece) = pieces.last_mut() {
            piece.push(c);
            if escaped_pipe {
                piece.push('|');
            }
        }
        if escaped_pipe {
            chars.next();
        }
    }
    pieces.remove(0);
    if pieces.last().is_some_and(|p| p.trim().is_empty()) {
        pieces.pop();
    }
    pieces.iter().map(|p| p.trim().to_string()).collect()
}

/// A cell without its code span: as many backticks are taken off each end
/// as the shorter run has, then the spaces. `` ``x` `` (the quasiquote row,
/// whose span the page left unbalanced) and ``` `` `x `` ``` are both the
/// two characters backtick and `x`.
pub fn strip_span(cell: &str) -> String {
    let cell = cell.trim();
    let lead = cell.chars().take_while(|c| *c == '`').count();
    let trail = cell.chars().rev().take_while(|c| *c == '`').count();
    if lead == cell.len() {
        return cell.to_string();
    }
    let cut = lead.min(trail);
    cell[cut..cell.len() - cut].trim().to_string()
}

/// The name of a row: the code span of cell 1, and no `(new)` after it.
fn name_of(cell: &str) -> String {
    let cell = cell.trim();
    let cell = cell.strip_suffix("(new)").unwrap_or(cell);
    strip_span(cell)
}

/// The first symbol of a spelling cell: `(subs s a b)` is `subs`, `@x` is
/// `@x`, `[a b & r]` is `a` (the name cell carries the `&`). The one
/// exception is a definition form, `(impl Debug T ..)`, whose first symbol
/// is no more than the form's name: it is spelled by the protocol it
/// instantiates, `Debug`, which is what a case that defines one contains.
fn spelling_of(cell: &str) -> String {
    let span = strip_span(cell);
    let mut words = span.split_whitespace();
    let symbol = |w: Option<&str>| {
        let w = w.unwrap_or("").trim_start_matches(['(', '[']);
        w.split(')').next().unwrap_or("").to_string()
    };
    let first = symbol(words.next());
    if first == "impl" {
        return symbol(words.next());
    }
    first
}

/// Reads the table out of the text of `spec/stdlib.md`.
pub fn parse_table(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let mut section = String::new();
    let mut inside = false;
    for (index, line) in text.lines().enumerate() {
        if line.starts_with("## 4.") {
            inside = true;
        } else if line.starts_with("## ") && inside {
            break;
        }
        if !inside {
            continue;
        }
        if let Some(heading) = line.strip_prefix("### ") {
            section = heading.to_string();
        } else if line.starts_with("| ") {
            if let Some(row) = row_of(&section, line, index + 1)? {
                rows.push(row);
            }
        }
    }
    if rows.is_empty() {
        return Err("no row between `## 4.` and `## 5.`".to_string());
    }
    Ok(rows)
}

/// The row of one table line, if it is one.
fn row_of(section: &str, line: &str, number: usize) -> Result<Option<Row>, String> {
    let cells = split_cells(line);
    if cells.len() < 6 {
        return Ok(None);
    }
    let Ok(tranche) = cells[4].parse::<u8>() else {
        let rule = cells[4].chars().all(|c| c == '-' || c == ':');
        if rule || cells[4] == "T" {
            return Ok(None);
        }
        return Err(format!(
            "line {number}: the tranche `{}` is no number",
            cells[4]
        ));
    };
    let name = name_of(&cells[0]);
    if name.is_empty() {
        return Err(format!("line {number}: a row with no name"));
    }
    Ok(Some(Row {
        section: section.to_string(),
        name,
        spelling: spelling_of(&cells[2]),
        tranche,
        line: number,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
intro | not a row
## 4. The function table
### 4.1 Builtins
| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `+` | adapt | `(+ a b ..) (+)` | `t t -> t \\| Num t` | 1 | a note with `code` |
| `subs-from` (new) | new | `(subs-from s a)` | `str i64 -> str` | 1 | |
| ``x` | adapt | ``x` | reader | 2 | the unbalanced span |
| `` `y `` | adapt | `` `y `` | reader | 3 | the balanced span |
| `deref` | adapt | `@x` | `a -> b` | 1 | |
| `&` | adapt | `[a b & r]` | pattern | 1 | |
### 4.17 Not offered
| `iterator-seq` | omit | - | Java objects |
## 5. Deviations
| `after` | keep | `(after)` | x | 1 | not in the table |
";

    fn find<'a>(rows: &'a [Row], name: &str) -> &'a Row {
        rows.iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no row {name}"))
    }

    #[test]
    fn rows_are_read_between_the_headings_with_their_section() {
        let rows = parse_table(FIXTURE).unwrap();
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["+", "subs-from", "`x", "`y", "deref", "&"]);
        assert_eq!(find(&rows, "+").section, "4.1 Builtins");
        assert_eq!(find(&rows, "+").tranche, 1);
        assert_eq!(find(&rows, "+").line, 6);
    }

    #[test]
    fn an_escaped_pipe_does_not_split_a_cell() {
        let cells = split_cells("| `+` | adapt | `(+ a b)` | `t t -> t \\| Num t` | 1 | n |");
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[3], "`t t -> t \\| Num t`");
        assert_eq!(cells[4], "1");
    }

    #[test]
    fn a_new_marker_and_a_code_span_are_taken_off_the_name() {
        let rows = parse_table(FIXTURE).unwrap();
        assert_eq!(find(&rows, "subs-from").spelling, "subs-from");
        assert_eq!(name_of("`foo` (new)"), "foo");
        assert_eq!(name_of("`` `x ``"), "`x");
        assert_eq!(name_of("``x`"), "`x");
        assert_eq!(name_of("`a` "), "a");
    }

    #[test]
    fn the_spelling_is_the_first_symbol_of_the_cell() {
        let rows = parse_table(FIXTURE).unwrap();
        assert_eq!(find(&rows, "+").spelling, "+");
        assert_eq!(find(&rows, "deref").spelling, "@x");
        assert_eq!(find(&rows, "&").spelling, "a");
        assert_eq!(find(&rows, "`x").spelling, "`x");
        assert_eq!(spelling_of("`(if-let [x e] a b)`"), "if-let");
        assert_eq!(spelling_of("`(m)`"), "m");
        assert_eq!(spelling_of("`Long/MAX_VALUE`"), "Long/MAX_VALUE");
        // an `impl` form is spelled by its protocol; a bare one has none
        assert_eq!(spelling_of("`(impl Debug T ..)`"), "Debug");
        assert_eq!(spelling_of("`(impl (Reducible e) T ..)`"), "Reducible");
        assert_eq!(spelling_of("`(impl)`"), "");
    }

    #[test]
    fn rows_with_fewer_than_six_cells_are_the_not_offered_list() {
        let rows = parse_table(FIXTURE).unwrap();
        assert!(rows.iter().all(|r| r.name != "iterator-seq"));
    }

    #[test]
    fn a_tranche_that_is_no_number_is_an_error_but_the_header_and_rule_are_not() {
        let text = "## 4. T\n| `a` | keep | `(a)` | x | one | n |\n";
        let err = parse_table(text).unwrap_err();
        assert!(err.contains("line 2") && err.contains("`one`"), "{err}");
    }

    #[test]
    fn a_missing_table_is_an_error() {
        assert!(parse_table("# nothing\n").is_err());
        assert!(parse_table("## 4. T\n").is_err());
    }

    /// The counts of the page, so that a parse that drops rows is found:
    /// whoever adds, removes or moves a row of the table updates them here.
    #[test]
    fn the_real_table_has_the_rows_the_page_counts() {
        let text = std::fs::read_to_string(crate::stdlib_table::SPEC).unwrap();
        let rows = parse_table(&text).unwrap();
        let of = |t: u8| rows.iter().filter(|r| r.tranche == t).count();
        let counts: Vec<usize> = (1..=5).map(of).collect();
        assert_eq!((rows.len(), counts), (720, vec![207, 89, 226, 116, 82]));
    }
}
