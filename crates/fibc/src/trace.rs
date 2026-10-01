//! The canonical free trace (spec/compiler.md §4): the sequence of
//! allocations and frees that the interpreter and the compiled program
//! must agree on (method.md rule 6), derived from the interpreter's
//! heap events on one side and parsed from the runtime's trace-mode
//! output on the other, and compared.

use std::collections::HashMap;
use std::fmt;

use fibref::{Event, Kind, ObjId};

/// The class of an object in the trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// An immutable object: struct, variant, string, array, closure,
    /// weak box, task.
    Object,
    /// A cell.
    Cell,
    /// An atom.
    Atom,
}

impl Class {
    fn letter(self) -> char {
        match self {
            Class::Object => 'o',
            Class::Cell => 'c',
            Class::Atom => 'a',
        }
    }

    fn from_letter(c: &str) -> Option<Class> {
        match c {
            "o" => Some(Class::Object),
            "c" => Some(Class::Cell),
            "a" => Some(Class::Atom),
            _ => None,
        }
    }

    fn of_kind(k: Kind) -> Class {
        match k {
            Kind::Immutable => Class::Object,
            Kind::Cell => Class::Cell,
            Kind::Atom => Class::Atom,
        }
    }
}

/// One line of the trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Line {
    /// `A n k`: heap allocation `n` of class `k`.
    Alloc(u64, Class),
    /// `F n`: heap object `n` freed.
    Free(u64),
    /// `S n k`: stack allocation `n` of class `k`.
    Stack(u64, Class),
    /// `D n`: stack object `n`'s scope ended.
    End(u64),
    /// `T`: a thread was spawned.
    Thread,
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Line::Alloc(n, k) => write!(f, "A {n} {}", k.letter()),
            Line::Free(n) => write!(f, "F {n}"),
            Line::Stack(n, k) => write!(f, "S {n} {}", k.letter()),
            Line::End(n) => write!(f, "D {n}"),
            Line::Thread => write!(f, "T"),
        }
    }
}

/// A whole trace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    pub lines: Vec<Line>,
}

impl Trace {
    /// The interpreter's side: `fibref`'s heap events with the object
    /// ids renumbered in allocation order, immortals skipped.
    pub fn from_events(events: &[Event]) -> Trace {
        let mut ids: HashMap<ObjId, u64> = HashMap::new();
        let mut lines = Vec::new();
        let mut next = 0u64;
        let mut number = |id: ObjId, ids: &mut HashMap<ObjId, u64>| {
            next += 1;
            ids.insert(id, next);
            next
        };
        // The events of `def` evaluation (everything up to the last
        // `Immortalised`, syntax §3.19) are not traced: the compiled
        // program holds those objects as static data (compiler.md §4).
        for e in fibref::heap::traced(events) {
            match *e {
                Event::Alloc { id, kind } => {
                    let n = number(id, &mut ids);
                    lines.push(Line::Alloc(n, Class::of_kind(kind)));
                }
                Event::AllocStack { id, kind, .. } => {
                    let n = number(id, &mut ids);
                    lines.push(Line::Stack(n, Class::of_kind(kind)));
                }
                Event::Free { id } => {
                    if let Some(n) = ids.get(&id) {
                        lines.push(Line::Free(*n));
                    }
                }
                Event::Drop { id } => {
                    if let Some(n) = ids.get(&id) {
                        lines.push(Line::End(*n));
                    }
                }
                _ => {}
            }
        }
        Trace { lines }
    }

    /// The compiled side: the trace lines among the runtime's standard
    /// error output; any other line is left alone.
    pub fn parse(text: &str) -> Trace {
        let lines = text.lines().filter_map(parse_line).collect();
        Trace { lines }
    }

    /// The number of heap objects allocated: the `A` lines, which the
    /// case header key `allocs` bounds. It equals the interpreter's
    /// `fibref::heap::trace_allocs` on the same run, since the two
    /// traces are the same lines.
    pub fn allocs(&self) -> u64 {
        self.lines
            .iter()
            .filter(|l| matches!(l, Line::Alloc(..)))
            .count() as u64
    }

    /// Whether a thread was spawned.
    pub fn threaded(&self) -> bool {
        self.lines.contains(&Line::Thread)
    }

    /// The text of the trace, one line each.
    pub fn render(&self) -> String {
        self.lines.iter().map(|l| format!("{l}\n")).collect()
    }
}

fn parse_line(s: &str) -> Option<Line> {
    let w: Vec<&str> = s.split(' ').collect();
    let n = |i: usize| w.get(i).and_then(|t| t.parse::<u64>().ok());
    let k = |i: usize| w.get(i).and_then(|t| Class::from_letter(t));
    match w.first().copied()? {
        "A" if w.len() == 3 => Some(Line::Alloc(n(1)?, k(2)?)),
        "S" if w.len() == 3 => Some(Line::Stack(n(1)?, k(2)?)),
        "F" if w.len() == 2 => Some(Line::Free(n(1)?)),
        "D" if w.len() == 2 => Some(Line::End(n(1)?)),
        "T" if w.len() == 1 => Some(Line::Thread),
        _ => None,
    }
}

/// Compares the interpreter's trace with the compiled program's
/// (compiler.md §4): line for line, or by multiset when a thread was
/// spawned. `Ok(())` when they agree, else what differs first.
pub fn compare(interp: &Trace, compiled: &Trace) -> Result<(), String> {
    if compiled.threaded() || interp.threaded() {
        return compare_multiset(interp, compiled);
    }
    for (i, (a, b)) in interp.lines.iter().zip(&compiled.lines).enumerate() {
        if a != b {
            return Err(format!(
                "trace line {}: interpreter `{a}`, compiled `{b}`",
                i + 1
            ));
        }
    }
    match interp.lines.len().cmp(&compiled.lines.len()) {
        std::cmp::Ordering::Equal => Ok(()),
        std::cmp::Ordering::Less => Err(format!(
            "trace line {}: compiled has `{}`, interpreter ended",
            interp.lines.len() + 1,
            compiled.lines[interp.lines.len()]
        )),
        std::cmp::Ordering::Greater => Err(format!(
            "trace line {}: interpreter has `{}`, compiled ended",
            compiled.lines.len() + 1,
            interp.lines[compiled.lines.len()]
        )),
    }
}

/// The per-class counts of a threaded run: allocations, frees, stack
/// allocations, ends, and the number of objects live at the end.
fn tally(t: &Trace) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut live: HashMap<u64, Class> = HashMap::new();
    for l in &t.lines {
        let key = match l {
            Line::Alloc(n, k) => {
                live.insert(*n, *k);
                format!("A {}", k.letter())
            }
            Line::Stack(n, k) => {
                live.insert(*n, *k);
                format!("S {}", k.letter())
            }
            Line::Free(n) | Line::End(n) => {
                live.remove(n);
                if matches!(l, Line::Free(_)) { "F" } else { "D" }.to_string()
            }
            // The interpreter's trace has no thread events.
            Line::Thread => continue,
        };
        *counts.entry(key).or_default() += 1;
    }
    for k in live.values() {
        *counts.entry(format!("live {}", k.letter())).or_default() += 1;
    }
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort();
    out
}

/// A threaded run (compiler.md §4): the objects live at exit must
/// agree per class, and the compiled program may allocate more than
/// the interpreter but never fewer, since a `swap!` attempt that
/// loses its compare discards what it built and how often that
/// happens under real threads is unspecified (types §8.6).
fn compare_multiset(interp: &Trace, compiled: &Trace) -> Result<(), String> {
    let (a, b) = (tally(interp), tally(compiled));
    let live = |t: &[(String, usize)]| -> Vec<(String, usize)> {
        t.iter()
            .filter(|(k, _)| k.starts_with("live"))
            .cloned()
            .collect()
    };
    let more =
        |t: &[(String, usize)], k: &str| t.iter().find(|(x, _)| x == k).map_or(0, |(_, n)| *n);
    let fewer = a
        .iter()
        .filter(|(k, _)| k.starts_with("A ") || k.starts_with("S "))
        .any(|(k, n)| more(&b, k) < *n);
    if live(&a) == live(&b) && !fewer {
        return Ok(());
    }
    let show = |t: &[(String, usize)]| {
        t.iter()
            .map(|(k, n)| format!("{k}={n}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Err(format!(
        "threaded trace tallies differ: interpreter [{}], compiled [{}]",
        show(&a),
        show(&b)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(text: &str) -> Trace {
        Trace::parse(text)
    }

    #[test]
    fn parses_and_renders_its_own_lines() {
        let tr = t("A 1 o\nnoise\nS 2 c\nF 1\nD 2\nT\n");
        assert_eq!(tr.lines.len(), 5);
        assert_eq!(tr.render(), "A 1 o\nS 2 c\nF 1\nD 2\nT\n");
        assert!(tr.threaded());
    }

    #[test]
    fn events_are_renumbered_without_immortals() {
        use fibref::{Heap, Value};
        let mut heap = Heap::new();
        heap.alloc_immortal(Kind::Immutable, vec![]).unwrap();
        let c = heap.alloc(Kind::Cell, vec![Value::Nil]).unwrap();
        let scope = heap.open_scope();
        heap.alloc_in_scope(scope, Kind::Immutable, vec![]).unwrap();
        heap.retain(c).unwrap();
        heap.end_scope(scope).unwrap();
        heap.release(c).unwrap();
        heap.release(c).unwrap();
        assert_eq!(
            Trace::from_events(heap.trace()).render(),
            "A 1 c\nS 2 o\nD 2\nF 1\n"
        );
    }

    #[test]
    fn allocs_are_the_a_lines_on_both_sides() {
        let tr = t("A 1 o\nS 2 c\nA 3 a\nF 1\nD 2\nA 4 c\n");
        assert_eq!(tr.allocs(), 3);
        assert_eq!(t("").allocs(), 0);
        // The interpreter's count and its trace's A lines are one number.
        use fibref::{Heap, Value};
        let mut heap = Heap::new();
        heap.alloc_immortal(Kind::Immutable, vec![]).unwrap();
        heap.alloc(Kind::Cell, vec![Value::Nil]).unwrap();
        let scope = heap.open_scope();
        heap.alloc_in_scope(scope, Kind::Immutable, vec![]).unwrap();
        heap.alloc(Kind::Atom, vec![Value::Nil]).unwrap();
        assert_eq!(Trace::from_events(heap.trace()).allocs(), 2);
        assert_eq!(fibref::heap::trace_allocs(heap.trace()), 2);
    }

    #[test]
    fn exact_comparison_names_the_first_difference() {
        assert_eq!(compare(&t("A 1 o\nF 1\n"), &t("A 1 o\nF 1\n")), Ok(()));
        let e = compare(&t("A 1 o\nF 1\n"), &t("A 1 o\n")).unwrap_err();
        assert!(e.contains("line 2") && e.contains("compiled ended"), "{e}");
        let e = compare(&t("A 1 o\n"), &t("A 1 c\n")).unwrap_err();
        assert!(e.contains("line 1"), "{e}");
    }

    #[test]
    fn threaded_comparison_is_by_tally() {
        let a = t("A 1 o\nA 2 c\nF 2\nF 1\n");
        let b = t("A 1 c\nT\nA 2 o\nF 1\nF 2\n");
        assert_eq!(compare(&a, &b), Ok(()));
        let c = t("A 1 c\nT\nA 2 o\nF 1\n");
        assert!(compare(&a, &c).is_err());
        // More allocations than the interpreter's, all freed: a swap!
        // that retried under contention.
        let d = t("A 1 c\nT\nA 2 o\nA 3 o\nF 3\nF 1\nF 2\n");
        assert_eq!(compare(&a, &d), Ok(()));
        let e = t("T\nA 1 o\nF 1\n");
        assert!(compare(&a, &e).is_err(), "fewer allocations");
    }
}
