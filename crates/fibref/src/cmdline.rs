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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    fn os(bytes: &[u8]) -> OsString {
        OsString::from_vec(bytes.to_vec())
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
