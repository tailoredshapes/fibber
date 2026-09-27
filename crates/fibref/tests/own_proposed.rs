//! Every ```lisp block of `spec/drafts/PROPOSED_CASES.md` through the
//! whole front end: reading, expansion, lowering, the syntactic `&`
//! checks, typing and the ownership pass (`fibref::own::check_forms`).
//!
//! Proposed cases have no sign-off (method.md rule 3), so an `accept`
//! header is reported, not asserted: a block this checker rejects is
//! listed with its error. A `reject` header is asserted: the program
//! must be rejected with an error containing the header's text. A block
//! with several `main`s is split into one program per `main` as in
//! `types_proposed.rs`; a block whose program needs a user macro run at
//! expansion time is pending (no evaluator yet) and reported as such.

use std::path::PathBuf;

use fibref::expand::{expand_program, ExpandCtx, NoRunner};
use fibref::own::{check_forms, CheckError};
use fibref::syntax::{read_all, Form};
use fibref::types::prelude_forms;

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

/// What the text of one program says it expects: `accept`, `reject:
/// text`, or nothing.
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
        let mut prog: Vec<Form> = if k == 0 { Vec::new() } else { shared.to_vec() };
        if k == 0 {
            prog = forms[..=m].to_vec();
        } else {
            prog.extend_from_slice(&forms[mains[k - 1] + 1..=m]);
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

/// The first error of the whole front end, or `Ok`.
fn verdict(forms: Vec<Form>) -> Result<(), String> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx)?;
    let forms =
        expand_program(forms, &mut ctx, &mut NoRunner).map_err(|e| format!("expansion: {e}"))?;
    check_forms(&forms, &prelude)
        .map(|_| ())
        .map_err(|e| match e {
            CheckError::Type(es) => es[0].message.clone(),
            CheckError::Own(es) => es[0].message.clone(),
            other => other.to_string(),
        })
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Judgement {
    Match,
    Pending,
    NoHeader,
    AcceptRejected,
    RejectAccepted,
    RejectOtherText,
}

fn judge(expected: &Option<String>, got: &Result<(), String>) -> Judgement {
    let Some(exp) = expected else {
        return Judgement::NoHeader;
    };
    match (exp.strip_prefix("reject: "), got) {
        (_, Err(e)) if e.starts_with("expansion: ") => Judgement::Pending,
        (None, Ok(())) => Judgement::Match,
        (None, Err(_)) => Judgement::AcceptRejected,
        (Some(_), Ok(())) => Judgement::RejectAccepted,
        (Some(t), Err(e)) if e.contains(t.trim_end_matches('.')) => Judgement::Match,
        (Some(_), Err(_)) => Judgement::RejectOtherText,
    }
}

#[test]
fn proposed_cases_through_the_ownership_checker() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/drafts/PROPOSED_CASES.md");
    let md = std::fs::read_to_string(&path).expect("PROPOSED_CASES.md");
    let blocks = lisp_blocks(&md);
    assert!(blocks.len() > 60, "found only {} lisp blocks", blocks.len());
    let mut tally = std::collections::BTreeMap::new();
    let mut wrong_rejects = Vec::new();
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
                Ok(()) => "accepted".to_string(),
                Err(e) => format!("rejected: {e}"),
            };
            let exp_text = expected.clone().unwrap_or_else(|| "-".into());
            println!("{file} #{k} ({heading}): expected {exp_text}; {got_text}; {j:?}");
            if matches!(j, Judgement::RejectAccepted | Judgement::RejectOtherText) {
                wrong_rejects.push(format!("{file} #{k} ({heading}): {exp_text}; {got_text}"));
            }
        }
    }
    println!("tally: {tally:?}");
    assert!(
        wrong_rejects.is_empty(),
        "reject headers not met:\n{}",
        wrong_rejects.join("\n")
    );
}
