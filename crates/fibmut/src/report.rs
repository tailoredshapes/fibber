//! What a run found, and how it is written down: a table of how the
//! mutants ended, one of how each operator fared, and each survivor with
//! its edit as a diff.

use std::fmt::Write;

use crate::ops::{Mutant, OPERATORS};

/// How one mutant ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A case failed. `class` is how (`result`, `trap`, `audit`, `allocs`,
    /// `compile`, `failed`, `timeout`, `crash`, `other`), `case` which case
    /// was first to fail, `detail` what the harness said.
    Killed {
        case: String,
        class: &'static str,
        detail: String,
    },
    /// Every case that loads the module passed.
    Survived,
    /// The module does not compile with the edit; no case was run.
    Invalid(String),
}

/// One mutant and its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// Its place in the module's list of all mutants in source order.
    pub id: usize,
    pub line: usize,
    pub mutant: Mutant,
    pub verdict: Verdict,
}

/// A whole run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub module: String,
    pub tool: String,
    pub seed: u64,
    pub src: String,
    /// The mutants of the module, by operator, before any filter.
    pub sites: Vec<(&'static str, usize)>,
    /// How many remained after `--ops` and `--lines`, and how many were run.
    pub filtered: usize,
    pub selected: usize,
    pub dropped: Vec<(String, String)>,
    pub loading: usize,
    pub done: Vec<Done>,
}

/// The classes of a kill, in the order the table lists them.
const CLASSES: [&str; 9] = [
    "result", "trap", "audit", "allocs", "compile", "failed", "timeout", "crash", "other",
];

impl Report {
    pub fn killed(&self) -> usize {
        self.done
            .iter()
            .filter(|d| matches!(d.verdict, Verdict::Killed { .. }))
            .count()
    }

    pub fn survivors(&self) -> Vec<&Done> {
        self.done
            .iter()
            .filter(|d| d.verdict == Verdict::Survived)
            .collect()
    }

    fn invalid(&self) -> usize {
        self.done
            .iter()
            .filter(|d| matches!(d.verdict, Verdict::Invalid(_)))
            .count()
    }

    fn killed_by(&self, class: &str) -> usize {
        self.done
            .iter()
            .filter(|d| matches!(&d.verdict, Verdict::Killed { class: c, .. } if *c == class))
            .count()
    }
}

/// The lines of `s` that hold `start..end`, with their text.
fn lines_around(s: &str, start: usize, end: usize) -> String {
    let from = s[..start].rfind('\n').map_or(0, |i| i + 1);
    let to = s[end..].find('\n').map_or(s.len(), |i| end + i);
    s[from..to].to_string()
}

/// The edit as the line or lines it changes, before (`-`) and after (`+`).
pub fn diff(src: &str, m: &Mutant) -> String {
    let after = m.apply(src);
    let mut out = String::new();
    for line in lines_around(src, m.start, m.end).lines() {
        let _ = writeln!(out, "- {}", line.trim_end());
    }
    for line in lines_around(&after, m.start, m.start + m.text.len()).lines() {
        let _ = writeln!(out, "+ {}", line.trim_end());
    }
    out
}

fn head(r: &Report) -> String {
    let mut out = format!("fibmut: {} (tool {}, seed {})\n", r.module, r.tool, r.seed);
    let all: usize = r.sites.iter().map(|(_, n)| n).sum();
    let by: Vec<String> = r.sites.iter().map(|(op, n)| format!("{op} {n}")).collect();
    let _ = writeln!(
        out,
        "mutants: {all} in the module ({}); {} after the filters; {} run",
        by.join(", "),
        r.filtered,
        r.done.len()
    );
    let _ = writeln!(
        out,
        "cases: {} selected, {} passed the unmutated module, {} of them load it",
        r.selected + r.dropped.len(),
        r.selected,
        r.loading
    );
    for (case, why) in &r.dropped {
        let why: String = why.chars().take(120).collect();
        let _ = writeln!(out, "  dropped {case}: {why}");
    }
    out
}

fn outcomes(r: &Report) -> String {
    let mut out = String::from("\noutcome              count\n");
    for class in CLASSES {
        let n = r.killed_by(class);
        if n > 0 || ["result", "trap", "audit", "timeout"].contains(&class) {
            let _ = writeln!(out, "killed by {class:<10} {n:>5}");
        }
    }
    let survived = r.survivors().len();
    let _ = writeln!(out, "survived           {survived:>7}");
    let _ = writeln!(
        out,
        "invalid            {:>7}   (does not compile)",
        r.invalid()
    );
    let _ = writeln!(
        out,
        "killed of killed+survived: {} of {}",
        r.killed(),
        r.killed() + survived
    );
    out
}

fn operators(r: &Report) -> String {
    let mut out = String::from("\noperator   run killed survived invalid\n");
    for op in OPERATORS {
        let of: Vec<&Done> = r.done.iter().filter(|d| d.mutant.op == op).collect();
        if of.is_empty() {
            continue;
        }
        let count = |f: fn(&Verdict) -> bool| of.iter().filter(|d| f(&d.verdict)).count();
        let killed = count(|v| matches!(v, Verdict::Killed { .. }));
        let survived = count(|v| *v == Verdict::Survived);
        let invalid = count(|v| matches!(v, Verdict::Invalid(_)));
        let _ = writeln!(
            out,
            "{op:<8} {:>5} {killed:>6} {survived:>8} {invalid:>7}",
            of.len()
        );
    }
    out
}

/// One line for a mutant: its id, operator, line, and how it ended.
pub fn line(d: &Done) -> String {
    let end = match &d.verdict {
        Verdict::Killed { case, class, .. } => format!("killed by {case} ({class})"),
        Verdict::Survived => "SURVIVED".to_string(),
        Verdict::Invalid(why) => format!("invalid: {why}"),
    };
    format!("m{} {} line {}: {end}", d.id, d.mutant.op, d.line)
}

/// The report: the tables, the survivors with their diffs, and with
/// `verbose` a line for every mutant.
pub fn render(r: &Report, verbose: bool) -> String {
    let mut out = head(r);
    out.push_str(&outcomes(r));
    out.push_str(&operators(r));
    if verbose {
        out.push('\n');
        for d in &r.done {
            let _ = writeln!(out, "{}", line(d));
        }
    }
    for d in r.survivors() {
        let _ = write!(
            out,
            "\nSURVIVOR m{} [{}] {}:{}\n{}",
            d.id,
            d.mutant.op,
            r.module,
            d.line,
            diff(&r.src, &d.mutant)
        );
    }
    out
}

#[cfg(test)]
mod tests;
