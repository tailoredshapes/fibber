//! What kind of name the cursor is asking for, from the text before it
//! alone (so that it works on a buffer that does not read).

/// The class of candidates wanted.
#[derive(Debug, PartialEq, Eq)]
pub enum Want {
    /// A value, a form, a module or an alias.
    Name,
    /// The fields of the value named here: after `(. x `.
    Field(String),
    /// A type: after `x: ` or `->`.
    Type,
    /// The exports of the module aliased here: after `alias/`.
    Alias(String),
    /// A keyword: after `:`.
    Keyword,
}

/// The wish and where the word being typed starts.
#[derive(Debug, PartialEq, Eq)]
pub struct Ctx {
    pub want: Want,
    /// Byte offset of the first character of the partial word (the
    /// cursor itself when none has been typed).
    pub word_start: usize,
}

fn is_break(c: char) -> bool {
    c.is_whitespace() || "()[]{}\";,'`~@^".contains(c)
}

/// The start of the last word of `text`, which ends it.
fn word_start(text: &str) -> usize {
    text.char_indices()
        .rev()
        .find(|(_, c)| is_break(*c))
        .map_or(0, |(i, c)| i + c.len_utf8())
}

/// The kind of candidate wanted at the end of `before`.
pub fn classify(before: &str) -> Ctx {
    let start = word_start(before);
    let word = &before[start..];
    let want = if word.starts_with(':') {
        Want::Keyword
    } else if let Some(i) = word.rfind('/').filter(|i| *i > 0) {
        Want::Alias(word[..i].to_string())
    } else {
        after_word(&before[..start])
    };
    Ctx {
        want,
        word_start: start,
    }
}

/// The wish given the text before the partial word.
fn after_word(rest: &str) -> Want {
    if !rest.ends_with(char::is_whitespace) {
        return Want::Name;
    }
    let trimmed = rest.trim_end();
    let p = word_start(trimmed);
    let prev = &trimmed[p..];
    if prev == "->" || (prev.len() > 1 && prev.ends_with(':') && !prev.starts_with(':')) {
        return Want::Type;
    }
    let head = trimmed[..p].trim_end();
    let is_field_form = head.strip_suffix('.').is_some_and(|h| h.ends_with('('));
    if is_field_form && !prev.is_empty() && !prev.ends_with(':') {
        return Want::Field(prev.to_string());
    }
    Want::Name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn want(before: &str) -> Want {
        classify(before).want
    }

    #[test]
    fn a_type_is_wanted_after_a_name_with_a_colon_and_after_an_arrow() {
        assert_eq!(want("(defun f (x: "), Want::Type);
        assert_eq!(want("(defun f (x: i64) -> "), Want::Type);
        assert_eq!(want("(defun f (x: i6"), Want::Type);
        assert_eq!(want("(f :k "), Want::Name);
    }

    #[test]
    fn fields_are_wanted_after_a_dot_form_and_its_value() {
        assert_eq!(want("(. p "), Want::Field("p".into()));
        assert_eq!(want("(foo (. p na"), Want::Field("p".into()));
        assert_eq!(want("(. p"), Want::Name);
        assert_eq!(want("(x . p "), Want::Name);
    }

    #[test]
    fn an_alias_prefix_and_a_colon_make_their_own_classes() {
        assert_eq!(want("(seq/ma"), Want::Alias("seq".into()));
        assert_eq!(want("(f :ke"), Want::Keyword);
        assert_eq!(classify("(foo bar").word_start, 5);
        assert_eq!(classify("(foo ").word_start, 5);
    }
}
