//! Deeply nested input: 990 to 1010 open frames around one atom, on both
//! sides of the reader's limit of 1000 (spec/bootstrap.md §3 item 3).

use super::rng::Rng;
use super::tables::{CLOSERS, OPENERS};

/// Deeply nested input: 990 to 1010 open frames (delimiters, prefixes and
/// `#_` each count one; the limit is 1000), an atom, and the closers,
/// sometimes missing, wrong or in excess.
///
/// `#_` goes only right after an opening delimiter, or first: elsewhere
/// it would leave a prefix or another `#_` waiting for a form that never
/// comes, and every input would end in that one error. One input in three
/// has such `#_` frames (each discards everything inside it, so the dump
/// stays short); the others print the whole nest, a line per level.
pub fn deep(rng: &mut Rng) -> String {
    let frames = rng.between(990, 1010);
    let inout_at = rng.one_in(8).then(|| rng.below(frames));
    let discards = rng.one_in(3);
    let mut out = String::new();
    let mut closers: Vec<char> = Vec::new();
    let mut after_open = true;
    for i in 0..frames {
        after_open = if inout_at == Some(i) {
            out.push('&');
            false
        } else {
            deep_frame(rng, &mut out, &mut closers, after_open && discards)
        };
        if after_open && rng.one_in(60) {
            out.push('\n');
        }
    }
    if rng.one_in(6) {
        // A pending `#_` is a frame too: at 1000 this one is the 1001st.
        out.push_str("#_y ");
    }
    out.push_str(rng.pick(&["x", "1", "nil", "\"s\"", ":k", "\\a"]));
    out.push_str(&closing(rng, closers));
    out
}

/// Opens one frame; true when it is an opening delimiter (where `#_` may
/// follow).
fn deep_frame(rng: &mut Rng, out: &mut String, closers: &mut Vec<char>, discard_ok: bool) -> bool {
    match rng.weighted(&[62, 10, 5, 14, 9]) {
        1 => open_frame(out, closers, 1),
        2 => {
            // A map takes its pair: a key, then the nested form.
            open_frame(out, closers, 2);
            out.push_str(":k ");
            false
        }
        3 => {
            out.push_str(rng.pick(&["'", "`", ",", ",@", "@"]));
            false
        }
        4 if discard_ok => {
            out.push_str("#_");
            false
        }
        _ => open_frame(out, closers, 0),
    }
}

fn open_frame(out: &mut String, closers: &mut Vec<char>, kind: usize) -> bool {
    out.push(OPENERS[kind]);
    closers.push(CLOSERS[kind]);
    true
}

/// The closers in order, damaged one time in five.
fn closing(rng: &mut Rng, mut closers: Vec<char>) -> String {
    closers.reverse();
    match rng.weighted(&[80, 7, 7, 6]) {
        1 => closers.truncate(closers.len().saturating_sub(rng.between(1, 3))),
        2 if !closers.is_empty() => {
            let at = rng.below(closers.len());
            closers[at] = CLOSERS[rng.below(CLOSERS.len())];
        }
        3 => closers.push(')'),
        _ => {}
    }
    closers.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(text: &str) -> String {
        fibref::dump::dump_source(text, "deep")
    }

    #[test]
    fn deep_inputs_straddle_the_limit_and_most_print_a_deep_tree() {
        let mut rng = Rng::new(9);
        let dumps: Vec<String> = (0..100).map(|_| dump(&deep(&mut rng))).collect();
        let too_deep = dumps
            .iter()
            .filter(|d| d.starts_with("error TooDeep"))
            .count();
        let read = dumps.iter().filter(|d| !d.starts_with("error ")).count();
        let tall = dumps.iter().filter(|d| d.lines().count() > 900).count();
        assert!(
            (20..=70).contains(&too_deep),
            "{too_deep} of 100 are too deep"
        );
        assert!(read >= 15, "only {read} of 100 deep inputs read");
        assert!(
            tall >= 10,
            "only {tall} of 100 deep inputs print a nest over 900 lines"
        );
    }
}
