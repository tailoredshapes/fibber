//! A report, not a verdict: every ```lisp block of
//! `spec/drafts/PROPOSED_CASES.md` through the whole pipeline with the
//! reference interpreter (`fibref::eval::Interpreter`). A block whose
//! text starts with a case header (`;; spec:` .. `;; expect:` ..) is
//! judged against it by the case harness's own `judge`; every other
//! block (companions with several `main`s, fragments) is still run, one
//! program per `main`, so that a panic anywhere is caught, and a
//! companion whose `main` carries an inline verdict (`; accept, 42,
//! clean`, `; result 7`, `; reject: text`) is judged against that.
//!
//! Proposed cases have no sign-off (method.md rule 3): a mismatch may
//! be an error of the draft, of the spec or of the interpreter, so the
//! test prints the tally and every mismatch and does not fail on them.
//! It fails if the evaluator panics on any program, or if a program
//! does not finish within the time limit.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use fibref::cases::{
    judge, parse_header, AuditExpect, Evaluator, Expected, Header, Outcome, Status, Verdict,
};
use fibref::eval::Interpreter;
use fibref::syntax::{read_all, Form};

/// How long one program may run.
const LIMIT: Duration = Duration::from_secs(600);

/// `(heading, first line, text)` of every ```lisp block.
fn lisp_blocks(md: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let (mut heading, mut open, mut body) = (String::new(), None, String::new());
    for (i, line) in md.lines().enumerate() {
        match open {
            None if line.starts_with('#') => {
                heading = line.trim_start_matches('#').trim().to_string()
            }
            None if line.trim() == "```lisp" => open = Some(i + 2),
            Some(start) if line.trim() == "```" => {
                out.push((heading.clone(), start, std::mem::take(&mut body)));
                open = None;
            }
            Some(_) => body.push_str(&format!("{line}\n")),
            None => {}
        }
    }
    out
}

/// The programs of a block, each with the source line of its `main`:
/// the block itself when it has at most one `main`, else one per
/// `main` (the forms before the first `main` are shared, those between
/// two `main`s belong to the second), printed back to text.
fn programs(text: &str, file: &str) -> Vec<(String, Option<String>)> {
    let Ok(forms) = read_all(text, file) else {
        return vec![(text.to_string(), None)];
    };
    let is_main = |f: &Form| {
        f.as_list().is_some_and(|l| {
            l.len() > 1 && l[0].as_sym() == Some("defun") && l[1].as_sym() == Some("main")
        })
    };
    let mains: Vec<usize> = (0..forms.len()).filter(|i| is_main(&forms[*i])).collect();
    let lines: Vec<&str> = text.lines().collect();
    // The comment with the verdict is on one of the lines of `main`'s
    // form: from its first line to the line before the next form.
    let line_of = |i: usize| {
        let from = forms[i].pos.line.saturating_sub(1);
        let to = forms.get(i + 1).map_or(lines.len(), |f| f.pos.line - 1);
        lines
            .get(from..to.max(from))?
            .iter()
            .find(|l| l.contains("; accept") || l.contains("; reject:") || l.contains("; result"))
            .map(|l| l.to_string())
    };
    if mains.len() < 2 {
        return vec![(text.to_string(), mains.first().and_then(|m| line_of(*m)))];
    }
    let shared = &forms[..mains[0]];
    let mut out = Vec::new();
    for (k, &m) in mains.iter().enumerate() {
        let own: &[Form] = if k == 0 {
            &forms[mains[0]..=m]
        } else {
            &forms[mains[k - 1] + 1..=m]
        };
        let prog: Vec<String> = shared.iter().chain(own).map(|f| f.to_string()).collect();
        out.push((prog.join("\n"), line_of(m)));
    }
    out
}

/// Runs `src` on its own thread with a time limit.
fn run_limited(src: String) -> Option<Outcome> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(Interpreter.run(&src));
    });
    rx.recv_timeout(LIMIT).ok()
}

/// The verdict a companion's comment on its `main` gives, as a header.
fn inline_header(line: &Option<String>) -> Option<Header> {
    let comment = line
        .as_deref()?
        .split_once(';')?
        .1
        .trim_start_matches(';')
        .trim();
    let verdict = if let Some(text) = comment.strip_prefix("reject:") {
        Verdict::Reject {
            error: text.trim().to_string(),
        }
    } else {
        let rest = comment
            .strip_prefix("accept,")
            .or_else(|| comment.strip_prefix("result"))?;
        let n: i64 = rest.split(',').next()?.trim().parse().ok()?;
        Verdict::Accept {
            result: Expected::Int(n),
            audit: AuditExpect::Clean,
        }
    };
    Some(Header {
        spec: "inline".to_string(),
        verdict,
    })
}

fn describe(o: &Outcome) -> String {
    match o {
        Outcome::Compiled { result, audit } => format!("result {result}, audit {audit}"),
        Outcome::Rejected { message } => format!("rejected: {}", first_line(message)),
        Outcome::Failed { message } => format!("failed: {}", first_line(message)),
        Outcome::Trapped { message, .. } => format!("trapped: {}", first_line(message)),
        Outcome::Unsupported { reason } => format!("unsupported: {reason}"),
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

/// What the runs found.
#[derive(Default)]
struct Tally {
    pass: usize,
    unjudged: usize,
    mismatches: Vec<String>,
    broken: Vec<String>,
}

impl Tally {
    /// Runs one program and records how it compares with `header`.
    fn program(&mut self, name: &str, prog: String, header: Option<Header>) {
        let Some(outcome) = run_limited(prog) else {
            self.broken
                .push(format!("{name}: no outcome within {LIMIT:?}"));
            return;
        };
        if let Outcome::Failed { message } = &outcome {
            if message.contains("panicked") {
                self.broken.push(format!("{name}: {message}"));
            }
        }
        let got = describe(&outcome);
        let Some(h) = header else {
            self.unjudged += 1;
            println!("{name}: no header; {got}");
            return;
        };
        match judge(&h, &outcome) {
            Status::Pass => {
                self.pass += 1;
                println!("{name}: pass; {got}");
            }
            status => {
                println!("{name}: {status}");
                self.mismatches.push(format!("{name}: {status}"));
            }
        }
    }
}

#[test]
fn proposed_cases_through_the_interpreter() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/drafts/PROPOSED_CASES.md");
    let md = std::fs::read_to_string(&path).expect("PROPOSED_CASES.md");
    let blocks = lisp_blocks(&md);
    assert!(blocks.len() > 60, "found only {} lisp blocks", blocks.len());
    let mut tally = Tally::default();
    for (heading, line, text) in &blocks {
        let file = format!("PROPOSED_CASES.md:{line}");
        let block_header = parse_header(Path::new(&file), text).ok();
        for (k, (prog, main_line)) in programs(text, &file).into_iter().enumerate() {
            let header = block_header.clone().or_else(|| inline_header(&main_line));
            tally.program(&format!("{file} #{k} ({heading})"), prog, header);
        }
    }
    println!(
        "tally: {} match their header or inline verdict, {} do not, {} have neither",
        tally.pass,
        tally.mismatches.len(),
        tally.unjudged
    );
    for m in &tally.mismatches {
        println!("mismatch: {m}");
    }
    assert!(
        tally.broken.is_empty(),
        "the evaluator broke:\n{}",
        tally.broken.join("\n")
    );
}
