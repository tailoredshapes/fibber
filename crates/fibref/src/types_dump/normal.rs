//! The renumbering of variable ids (spec/bootstrap.md §6): the checker
//! prints an unsolved colour variable as `?ς7` and a type variable as
//! `?7`, with the number the order of allocation gave it, which a port
//! that allocates in another order would not reproduce. The dump prints
//! them renumbered by first occurrence within each record.

/// `text` with each colour variable `?ςN` and type variable `?N` renamed
/// `?ς1`, `?ς2`, .. and `?1`, `?2`, .. in the order of their first
/// occurrence (the two kinds count apart). A variable is a whole token:
/// it starts the text or follows a space, a parenthesis, a bracket, a
/// brace, a quotation mark or a comma, and ends the text or is followed
/// by one of them; anything else (`a?7`, `?7x`, `?ς`) is left as it is.
pub fn normalise(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut seen: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let starts = chars[i] == '?' && (i == 0 || is_break(chars[i - 1]));
        if let Some((colour, digits, end)) = starts.then(|| variable(&chars, i + 1)).flatten() {
            let list = &mut seen[usize::from(!colour)];
            let n = match list.iter().position(|d| *d == digits) {
                Some(at) => at + 1,
                None => {
                    list.push(digits);
                    list.len()
                }
            };
            out.push_str(&format!("{}{n}", if colour { "?ς" } else { "?" }));
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn is_break(c: char) -> bool {
    c.is_whitespace() || "()[]{}\",'`;".contains(c)
}

/// The variable whose digits (after an optional `ς`) start at `at`: whether
/// it is a colour, its digits, and where it ends.
fn variable(chars: &[char], at: usize) -> Option<(bool, String, usize)> {
    let colour = chars.get(at) == Some(&'ς');
    let from = at + usize::from(colour);
    let end = from
        + chars[from..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
    let whole = chars.get(end).is_none_or(|c| is_break(*c));
    (end > from && whole).then(|| (colour, chars[from..end].iter().collect(), end))
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn colour_and_type_variables_are_numbered_by_first_occurrence_each_kind_apart() {
        assert_eq!(
            normalise("?ς7 ⊑ ?ς12 and ?ς7, ?9 then ?4 ?9"),
            "?ς1 ⊑ ?ς2 and ?ς1, ?1 then ?2 ?1"
        );
        assert_eq!(normalise("(fn ?ς40 (a) a)"), "(fn ?ς1 (a) a)");
    }

    #[test]
    fn only_whole_tokens_are_variables() {
        for same in ["a?7", "?7x", "?ς", "?", "x ?ςa", "??7", "ς7", "?-7"] {
            assert_eq!(normalise(same), same);
        }
        assert_eq!(normalise("?3"), "?1");
        assert_eq!(normalise("a ?3\n?5 ?3"), "a ?1\n?2 ?1");
    }
}
