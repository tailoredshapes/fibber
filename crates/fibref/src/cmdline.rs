//! The command line of `fibref` and `fibc`, from the operating system's
//! words to strings. A word is bytes, not always UTF-8 (`std::env::args`
//! panics on one that is not). The words before `--` are the tool's own
//! (commands, flags, paths) and must be UTF-8, so that no path is quietly
//! changed into another; the words after it are the program's `(args)`
//! (syntax §4.3), where each invalid sequence becomes U+FFFD exactly as
//! `String::from_utf8_lossy` makes it, whose maximal-subpart rule the
//! compiled runtime's `fib.str-from-lossy` repeats for a built
//! executable's own command line.

use std::ffi::OsString;

/// The words of `args` as strings: an error carrying the first word
/// before the first `--` that is not UTF-8.
pub fn from_os(args: impl IntoIterator<Item = OsString>) -> Result<Vec<String>, OsString> {
    let mut words = Vec::new();
    let mut program = false;
    for word in args {
        if program {
            words.push(word.to_string_lossy().into_owned());
        } else {
            let text = word.into_string()?;
            program = text == "--";
            words.push(text);
        }
    }
    Ok(words)
}

/// The library roots named on a command line (spec/compiler.md §1,
/// **Proposed**): every `-I DIR` and `-IDIR` among the words before the
/// first `--`, in order, and the words that are left, in order. The
/// flag may stand anywhere there, as `cc`'s may, so each command parses
/// what is left as before. A `-I` without a directory, or with an empty
/// one or `--`, is left in the words, which the command's own parse
/// refuses.
pub fn split_roots(words: &[String]) -> (Vec<String>, Vec<String>) {
    let (mut dirs, mut rest) = (Vec::new(), Vec::new());
    let mut it = words.iter().peekable();
    let mut program = false;
    while let Some(word) = it.next() {
        program = program || word == "--";
        if program {
            rest.push(word.clone());
            continue;
        }
        match word.strip_prefix("-I") {
            Some("") => match it.next_if(|d| !d.is_empty() && *d != "--") {
                Some(dir) => dirs.push(dir.clone()),
                None => rest.push(word.clone()),
            },
            Some(dir) => dirs.push(dir.to_string()),
            None => rest.push(word.clone()),
        }
    }
    (dirs, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    fn os(bytes: &[u8]) -> OsString {
        OsString::from_vec(bytes.to_vec())
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn roots_are_taken_from_anywhere_before_the_dashes_in_order() {
        let (dirs, rest) = split_roots(&words(&[
            "run", "-I", "a", "f.fib", "-Ib", "-I", "c", "--", "-I", "d", "x",
        ]));
        assert_eq!(dirs, ["a", "b", "c"]);
        assert_eq!(rest, ["run", "f.fib", "--", "-I", "d", "x"]);
    }

    #[test]
    fn a_flag_without_a_directory_is_left_for_the_command_to_refuse() {
        let (dirs, rest) = split_roots(&words(&["run", "f.fib", "-I"]));
        assert!(dirs.is_empty());
        assert_eq!(rest, ["run", "f.fib", "-I"]);
        let (dirs, rest) = split_roots(&words(&["run", "-I", "", "f.fib"]));
        assert!(dirs.is_empty());
        assert_eq!(rest, ["run", "-I", "", "f.fib"]);
    }

    #[test]
    fn words_that_only_start_with_i_are_not_roots() {
        let (dirs, rest) = split_roots(&words(&["run", "-i", "Inc.fib", "I"]));
        assert!(dirs.is_empty());
        assert_eq!(rest, ["run", "-i", "Inc.fib", "I"]);
    }

    #[test]
    fn utf8_words_pass_through_unchanged() {
        let args = ["run", "a.fib", "--", "x", "é"].map(OsString::from);
        assert_eq!(
            from_os(args),
            Ok(["run", "a.fib", "--", "x", "é"].map(String::from).to_vec())
        );
    }

    #[test]
    fn a_word_after_the_dashes_that_is_not_utf8_is_lossy() {
        let args = vec![
            OsString::from("run"),
            OsString::from("a.fib"),
            OsString::from("--"),
            os(b"a\xffb"),
            os(b"\xe2\x82"),
        ];
        assert_eq!(
            from_os(args),
            Ok(["run", "a.fib", "--", "a\u{fffd}b", "\u{fffd}"]
                .map(String::from)
                .to_vec())
        );
    }

    #[test]
    fn a_word_before_the_dashes_that_is_not_utf8_is_an_error() {
        let bad = os(b"a\xff.fib");
        let args = vec![OsString::from("run"), bad.clone(), OsString::from("--")];
        assert_eq!(from_os(args), Err(bad));
    }

    #[test]
    fn only_the_first_dashes_end_the_names() {
        let args = vec![OsString::from("run"), OsString::from("--"), os(b"--\xff")];
        assert_eq!(
            from_os(args),
            Ok(["run", "--", "--\u{fffd}"].map(String::from).to_vec())
        );
    }
}
