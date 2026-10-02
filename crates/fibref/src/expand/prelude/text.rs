//! `print-str`, `pr-str`, `println-str` and `prn-str` (stdlib §4.14;
//! tranche 2 X3): the printers that return their text. The text is the one
//! `print`, `pr`, `println` and `prn` write (R5, `print.rs`): the pieces
//! joined by one space, a literal `nil` as the word `nil`, `Show` for the
//! first and third and `Debug` for the second and fourth.
//!
//! ```text
//! (print-str)       ⟹ ""                 (println-str) ⟹ "\n"
//! (print-str a)     ⟹ (fib.prelude/show a)
//! (print-str a b)   ⟹ (fib.prelude/str-concat (fib.prelude/show a) (fib.prelude/str-concat " " (fib.prelude/show b)))
//! (println-str a b) ⟹ the text of (print-str a b), then "\n" concatenated once
//! ```
//!
//! They coexist with the one-argument function twins of `fib.print`, which
//! is what a value position such as `(map println-str xs)` names.

use crate::syntax::{Form, Pos};

use super::print::{concat, join, pieces};
use crate::expand::build::string;

/// The text of the call `(NAME a ..)`: `debug` picks `Debug`, `newline`
/// adds the newline after the joined pieces.
pub(super) fn text(items: Vec<Form>, pos: &Pos, debug: bool, newline: bool) -> Form {
    let pieces = pieces(items, pos, debug);
    if pieces.is_empty() {
        return string(if newline { "\n" } else { "" }, pos);
    }
    let joined = join(pieces, Some(" "), pos);
    if newline {
        concat(joined, string("\n", pos), pos)
    } else {
        joined
    }
}
