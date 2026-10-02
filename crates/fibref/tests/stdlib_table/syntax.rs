//! The rows of §4 that are syntax or a shared name, and what makes a case
//! call one of them.
//!
//! `cover::calls` finds a row's spelling or name as a token of a case's code.
//! Some rows have no such token (C2-23): `@x`, `~x` and `~@x` are a prefix in
//! front of a form, `#(f % %2)` and `#{a b}` a dispatch character and a bracket
//! (the lexer of the tests splits tokens at both), `(:k x)` a keyword in head
//! position, `(->Point x y)` the constructor of a record the case names
//! itself; and `some` is `Option`'s constructor in every `(some x)` of every
//! `match`, so the token `some` is in nearly every case and says nothing:
//! only the call `(some pred c)` with two operands is the row. This module
//! says, row by row, which text of a case's code is a call. A header that
//! lists such a row still cannot make it covered by itself.

use super::rows::Row;

/// The prefixes a spelling can be: the prefix and a placeholder `x`.
const PREFIXES: [&str; 5] = ["@", "'", "`", "~", "~@"];

/// The characters that end a token: what the code's tokens split at.
const DELIMITERS: &str = "()[]{},\"/";

/// The names that are also a constructor or a pattern: only a call with
/// exactly two operands is the row.
const TWO_OPERANDS: [&str; 1] = ["some"];

/// Whether the row is judged by this module alone (the token rule would be
/// satisfied by a use that is not the row).
pub fn judged_here(row: &Row) -> bool {
    TWO_OPERANDS.contains(&row.name.as_str())
}

/// Whether the code calls the row by one of the rules of this module.
pub fn called(code: &str, row: &Row) -> bool {
    if TWO_OPERANDS.contains(&row.name.as_str()) {
        return has_call_of_two(code, &row.name);
    }
    if let Some(prefix) = prefix_of(&row.spelling).or_else(|| prefix_of(&row.name)) {
        return has_prefix(code, prefix);
    }
    if let Some(dispatch) = ["#(", "#{"]
        .into_iter()
        .find(|d| row.spelling.starts_with(d))
    {
        return code.contains(dispatch);
    }
    if row.spelling == ":k" {
        return has_keyword_call(code);
    }
    if is_constructor_placeholder(&row.spelling) {
        return has_record_constructor(code);
    }
    false
}

/// `@` for `@x`, `~@` for `~@x`: the prefix of a placeholder spelling.
fn prefix_of(text: &str) -> Option<&'static str> {
    let stem = text.strip_suffix('x')?;
    PREFIXES.iter().copied().find(|p| *p == stem)
}

/// A prefix in front of something that is not blank. A `~` is not the start
/// of `~@x`, and an `@` after a `~` is not a dereference.
fn has_prefix(code: &str, prefix: &str) -> bool {
    code.match_indices(prefix).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + prefix.len()..].chars().next();
        let after_ok = after.is_some_and(|n| !n.is_whitespace() && (prefix != "~" || n != '@'));
        after_ok && !(prefix == "@" && before == Some('~'))
    })
}

/// `(:name` : a keyword right after an opening parenthesis.
fn has_keyword_call(code: &str) -> bool {
    code.match_indices("(:").any(|(at, _)| {
        let next = code[at + 2..].chars().next();
        next.is_some_and(|n| !n.is_whitespace() && n != ')')
    })
}

/// `->Name` stands for the constructor function of any record.
fn is_constructor_placeholder(spelling: &str) -> bool {
    let name = spelling.strip_prefix("->");
    name.and_then(|n| n.chars().next())
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// A token `->Name`: the arrow starts a token and an upper-case letter
/// follows (`->>` and `char->i32` are not constructors).
fn has_record_constructor(code: &str) -> bool {
    code.match_indices("->").any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let starts = before.is_none_or(|b| b.is_whitespace() || DELIMITERS.contains(b));
        let next = code[at + 2..].chars().next();
        starts && next.is_some_and(|n| n.is_ascii_uppercase())
    })
}

/// A call `(NAME a b)`: the head, a space, and exactly two forms.
fn has_call_of_two(code: &str, name: &str) -> bool {
    let head = format!("({name}");
    code.match_indices(&head).any(|(at, _)| {
        let rest = &code[at + head.len()..];
        rest.starts_with(char::is_whitespace) && operands(rest) == 2
    })
}

/// How many forms stand before the parenthesis that closes the call whose
/// head was just read. Strings arrive blanked (`cover::code_of`), so a quote
/// opens a string that the next quote closes; a prefix such as `#` or `'`
/// belongs to the form after it.
fn operands(rest: &str) -> usize {
    let (mut depth, mut count) = (0usize, 0usize);
    let (mut in_form, mut in_string) = (false, false);
    for c in rest.chars() {
        if in_string {
            in_string = c != '"';
            continue;
        }
        match c {
            c if c.is_whitespace() || c == ',' => in_form = false,
            '(' | '[' | '{' => {
                if depth == 0 && !in_form {
                    count += 1;
                }
                depth += 1;
                in_form = false;
            }
            ')' | ']' | '}' => {
                if depth == 0 {
                    return count;
                }
                depth -= 1;
                in_form = false;
            }
            _ if depth > 0 => in_string = c == '"',
            _ => {
                if !in_form {
                    count += 1;
                    in_form = true;
                }
                in_string = c == '"';
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, spelling: &str) -> Row {
        Row {
            section: "4.2 syntax".to_string(),
            name: name.to_string(),
            spelling: spelling.to_string(),
            tranche: 2,
            line: 1,
        }
    }

    #[test]
    fn a_prefix_row_is_called_by_its_own_prefix_and_not_by_a_longer_one() {
        let tilde = row("~x", "~x");
        let splice = row("~@x", "~@x");
        let deref = row("deref", "@x");
        assert!(called("`(do ~a)", &tilde) && !called("`(do ~@a)", &tilde));
        assert!(called("`(do ~@a)", &splice) && !called("`(do ~a)", &splice));
        assert!(called("(count @acc)", &deref));
        assert!(!called("`(do ~@a)", &deref), "~@a is no dereference");
        assert!(!called("(f ~ a)", &tilde), "a prefix needs a form after it");
    }

    #[test]
    fn a_dispatch_row_needs_its_dispatch_in_the_code() {
        let fun = row("#(...)", "#(f");
        let set = row("#{...}", "#{a");
        assert!(called("(map #(* % 2) xs)", &fun) && !called("(map #{1 2} xs)", &fun));
        assert!(called("(count #{1 2})", &set) && !called("(map #(* % 2) xs)", &set));
        assert!(!called("(map (fn (x) x) xs)", &fun));
    }

    #[test]
    fn a_keyword_row_needs_a_keyword_in_head_position() {
        let kw = row(":k", ":k");
        assert!(called("(:age p)", &kw) && called("(f (:age p 0))", &kw));
        assert!(!called("(assoc m :age 1)", &kw), "a map key is no call");
        assert!(!called("(: x)", &kw) && !called("(:)", &kw));
    }

    #[test]
    fn a_constructor_row_is_called_by_any_record_constructor() {
        let ctor = row("->Name", "->Point");
        assert!(called("(->Pt 1 2)", &ctor) && called("(map ->Point xs ys)", &ctor));
        assert!(called("(fib.coll/->Pt 1 2)", &ctor));
        assert!(!called("(->> xs (map f))", &ctor) && !called("(char->i32 c)", &ctor));
        assert!(!called("(-> x (f) (g))", &ctor));
    }

    #[test]
    fn some_is_called_only_by_a_call_with_two_operands() {
        let some = row("some", "some");
        assert!(called("(some even? xs)", &some));
        assert!(called("(some (fn (x) (get m x)) [1 2 3])", &some));
        assert!(called("(f (some #(get m %) ks))", &some));
        assert!(called("(some\n  p\n  \"  \")", &some));
        assert!(!called("(match o ((some x) x) (nil 0))", &some));
        assert!(!called("(some 5)", &some), "the constructor is no call");
        assert!(!called("(some p c d)", &some) && !called("(some)", &some));
        assert!(!called("(some? o)", &some) && !called("(somewhat a b)", &some));
        assert!(judged_here(&some) && !judged_here(&row("map", "map")));
    }

    #[test]
    fn an_ordinary_row_is_not_judged_here() {
        let map = row("map", "map");
        assert!(!called("(map f xs)", &map) && !judged_here(&map));
    }

    #[test]
    fn operands_counts_forms_not_characters() {
        assert_eq!(operands(" a b)"), 2);
        assert_eq!(operands(" (f x) [1 2])"), 2);
        assert_eq!(operands(" #(f %) '(a b) c)"), 3);
        assert_eq!(operands(" \"a b\" \"c d\")"), 2);
        assert_eq!(operands(" ) trailing"), 0);
        assert_eq!(operands(" a,b)"), 2);
    }
}
