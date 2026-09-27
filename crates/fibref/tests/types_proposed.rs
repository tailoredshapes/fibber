//! A report, not a verdict: type-checks every ```lisp block of
//! `spec/drafts/PROPOSED_CASES.md` (proposed cases have no sign-off) and
//! prints, per program, what its header expects and what the checker
//! says. A block with several `main`s is split into one program per
//! `main`: the forms before the first `main` are shared, the forms
//! between two `main`s belong to the second.
//!
//! A header's `reject` with a type-error text should fail with that
//! text; a `reject` with an ownership text (types §6.14, the next pass)
//! should type-check; an `accept` should type-check.

use std::path::PathBuf;

use fibref::expand::{expand_program, ExpandCtx, NoRunner};
use fibref::syntax::{read_all, Form};
use fibref::types::{check_program, prelude_forms};

/// The error texts of §6.14 that the ownership pass reports.
const OWNERSHIP: [&str; 5] = [
    "passed to more than one & parameter",
    "& parameter captured by escaping closure",
    "& parameter in async function",
    "declared :borrow but escapes",
    "makes parameter",
];

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

/// What the text of one program says it expects.
fn expectation(text: &str) -> Option<String> {
    let mut verdict = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix(";; expect:") {
            verdict = Some(v.trim().to_string());
        } else if let Some(e) = l.strip_prefix(";; error:") {
            verdict = Some(format!("reject: {}", e.trim()));
        } else if let Some(i) = l.find("; reject:") {
            verdict = Some(format!("reject: {}", l[i + 9..].trim()));
        } else if l.contains("; accept") || l.contains("; result") {
            verdict = Some("accept".to_string());
        }
    }
    verdict
}

/// Splits a block into programs, one per `main`, each with its text.
fn programs(forms: Vec<Form>, text: &str) -> Vec<(Vec<Form>, String)> {
    let is_main = |f: &Form| {
        f.as_list().is_some_and(|l| {
            l.len() > 1 && l[0].as_sym() == Some("defun") && l[1].as_sym() == Some("main")
        })
    };
    let mains: Vec<usize> = forms
        .iter()
        .enumerate()
        .filter(|(_, f)| is_main(f))
        .map(|(i, _)| i)
        .collect();
    if mains.len() < 2 {
        return vec![(forms, text.to_string())];
    }
    let lines: Vec<&str> = text.lines().collect();
    let shared = &forms[..mains[0]];
    let mut out = Vec::new();
    for (k, &m) in mains.iter().enumerate() {
        let start = if k == 0 { mains[0] } else { mains[k - 1] + 1 };
        let mut prog: Vec<Form> = if k == 0 { Vec::new() } else { shared.to_vec() };
        prog.extend_from_slice(&forms[start..=m]);
        if k == 0 {
            prog = forms[..=m].to_vec();
        }
        let from = if k == 0 {
            0
        } else {
            forms[mains[k - 1]].pos.line
        };
        let to = mains
            .get(k + 1)
            .map_or(lines.len(), |n| forms[*n].pos.line - 1);
        out.push((
            prog,
            lines[from.min(lines.len())..to.min(lines.len())].join("\n"),
        ));
    }
    out
}

fn verdict(forms: Vec<Form>) -> Result<(), String> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx)?;
    let forms =
        expand_program(forms, &mut ctx, &mut NoRunner).map_err(|e| format!("expansion: {e}"))?;
    check_program(&forms, &prelude)
        .map(|_| ())
        .map_err(|es| es[0].message.clone())
}

fn judge(expected: &Option<String>, got: &Result<(), String>) -> &'static str {
    let Some(exp) = expected else {
        return "no header";
    };
    match (exp.strip_prefix("reject: "), got) {
        (_, Err(e)) if e.starts_with("expansion: ") => "pending",
        (None, Ok(())) => "match",
        (None, Err(_)) => "MISMATCH",
        (Some(t), Ok(())) if OWNERSHIP.iter().any(|o| t.contains(o)) => "match (ownership rejects)",
        (Some(_), Ok(())) => "MISMATCH",
        (Some(t), Err(e)) if e.contains(t.trim_end_matches('.')) => "match",
        (Some(_), Err(_)) => "MISMATCH",
    }
}

#[test]
fn report_proposed_cases() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/drafts/PROPOSED_CASES.md");
    let md = std::fs::read_to_string(&path).expect("PROPOSED_CASES.md");
    let blocks = lisp_blocks(&md);
    assert!(!blocks.is_empty(), "no lisp blocks found");
    let mut tally = std::collections::BTreeMap::new();
    for (heading, line, body) in &blocks {
        let file = format!("PROPOSED_CASES.md:{line}");
        let forms = match read_all(body, &file) {
            Ok(f) => f,
            Err(e) => {
                println!("{file} ({heading}): does not read: {e}");
                continue;
            }
        };
        for (k, (prog, text)) in programs(forms, body).into_iter().enumerate() {
            let expected = expectation(&text);
            let got = verdict(prog);
            let j = judge(&expected, &got);
            *tally.entry(j).or_insert(0) += 1;
            let got_text = match &got {
                Ok(()) => "type-checks".to_string(),
                Err(e) => format!("fails: {e}"),
            };
            let exp_text = expected.clone().unwrap_or_else(|| "-".into());
            println!("{file} #{k} ({heading}): expected {exp_text}; {got_text}; {j}");
        }
    }
    println!("tally: {tally:?}");
}
