//! One run: the working copy, the baseline, the control, and the mutants.
//!
//! The module under test is copied, with the library and the cases, into a
//! temporary directory laid out as the repository is (so that a case's
//! relative `roots` find the copy), and every edit is made there; the
//! files of the repository are only ever read. Before any mutant:
//!
//! 1. the baseline runs every selected case on the unmutated copy; a case
//!    that does not pass cannot judge anything and is dropped, listed;
//! 2. the control runs them again on a copy whose module cannot be read.
//!    The cases that then fail are the ones that load the module; if there
//!    are none, every mutant would survive and the run stops instead.
//!
//! A mutant is then written into the copy, checked to compile by a program
//! that only loads the module, and run against the loading cases, one process
//! at a time, stopping at the first that fails. The cases that killed a
//! mutant go first for the next one.

use std::fs;
use std::path::{Path, PathBuf};

use crate::casesel::{self, Case};
use crate::fsutil::{below, copy_tree, TempDir};
use crate::ops::{self, Mutant, OPERATORS};
use crate::report::{Done, Report, Verdict};
use crate::runner::{Check, Ending, Sandbox, Tool};
use crate::sample::sample;
use crate::sexp;

/// What to run and how.
pub struct Config {
    pub module: PathBuf,
    pub lib: PathBuf,
    pub cases: PathBuf,
    pub only: Vec<String>,
    pub max: usize,
    pub seed: u64,
    pub ops: Vec<String>,
    pub lines: Option<(usize, usize)>,
    pub tool: Tool,
}

/// The cases that passed the unmutated module, and the ones dropped with
/// the reason (case name, the harness's words).
type Baseline = (Vec<Case>, Vec<(String, String)>);

/// What a run says while it works, a line at a time.
pub type Say<'a> = &'a mut dyn FnMut(&str);

/// The mutants of the module and the ones chosen to run.
pub struct Plan {
    pub src: String,
    pub module_rel: PathBuf,
    pub all: Vec<Mutant>,
    pub chosen: Vec<(usize, Mutant)>,
    pub filtered: usize,
}

/// Lists the mutants of the module and chooses the sample. Reads files, writes none.
pub fn plan(cfg: &Config, base: &Path) -> Result<Plan, String> {
    if let Some(bad) = cfg.ops.iter().find(|o| !OPERATORS.contains(&o.as_str())) {
        return Err(format!(
            "no operator {bad}; the operators are {}",
            OPERATORS.join(", ")
        ));
    }
    let module_rel = below(&cfg.module, base)?;
    let src = fs::read_to_string(base.join(&module_rel))
        .map_err(|e| format!("cannot read {}: {e}", module_rel.display()))?;
    let all = ops::mutants(&src)
        .map_err(|e| format!("{}: cannot split the module: {e}", module_rel.display()))?;
    let wanted = |(_, m): &(usize, Mutant)| {
        let line = m.line(&src);
        (cfg.ops.is_empty() || cfg.ops.iter().any(|o| o == m.op))
            && cfg.lines.is_none_or(|(a, b)| a <= line && line <= b)
    };
    let kept: Vec<(usize, Mutant)> = all.iter().cloned().enumerate().filter(wanted).collect();
    let filtered = kept.len();
    let chosen = sample(kept, cfg.max, cfg.seed);
    Ok(Plan {
        src,
        module_rel,
        all,
        chosen,
        filtered,
    })
}

/// The working copy.
struct Stage {
    _tmp: TempDir,
    sb: Sandbox,
    module: PathBuf,
    check: Option<PathBuf>,
}

/// The name in the `ns` form of the module, if it has one.
fn module_name(src: &str) -> Option<String> {
    let forms = sexp::parse(src).ok()?;
    let ns = forms.iter().find(|f| f.head(src) == Some("ns"))?;
    ns.kids.get(1)?.atom(src).map(str::to_string)
}

impl Stage {
    fn new(cfg: &Config, base: &Path, plan: &Plan) -> Result<Stage, String> {
        let io = |what: &str, e: std::io::Error| format!("{what}: {e}");
        let (lib_rel, cases_rel) = (below(&cfg.lib, base)?, below(&cfg.cases, base)?);
        let under = plan.module_rel.strip_prefix(&lib_rel).map_err(|_| {
            format!(
                "{} is not under the library {}",
                plan.module_rel.display(),
                lib_rel.display()
            )
        })?;
        let tmp = TempDir::new("fibmut").map_err(|e| io("cannot make the working copy", e))?;
        let root = tmp.path().to_path_buf();
        copy_tree(&base.join(&lib_rel), &root.join(&lib_rel))
            .map_err(|e| io("cannot copy the library", e))?;
        copy_tree(&base.join(&cases_rel), &root.join(&cases_rel))
            .map_err(|e| io("cannot copy the cases", e))?;
        let check = module_name(&plan.src).map(|name| {
            let main = PathBuf::from("fibmut-check/main.fib");
            let text = format!("(ns main (:require [{name} :as m]))\n(defun main () -> i64 0)\n");
            (main, text)
        });
        if let Some((main, text)) = &check {
            fs::create_dir_all(root.join(main).parent().unwrap_or(&root))
                .map_err(|e| io("cannot write the check", e))?;
            fs::write(root.join(main), text).map_err(|e| io("cannot write the check", e))?;
        }
        Ok(Stage {
            module: root.join(&lib_rel).join(under),
            sb: Sandbox {
                lib: root.join(&lib_rel),
                cases: cases_rel,
                root,
            },
            check: check.map(|(main, _)| main),
            _tmp: tmp,
        })
    }

    fn write(&self, text: &str) -> Result<(), String> {
        fs::write(&self.module, text).map_err(|e| format!("cannot write the working copy: {e}"))
    }

    /// `text` with the working copy's directory taken out of its paths, so that
    /// a message names `lib/fib/seq/x.fib:3:4` and not a temporary directory.
    fn scrub(&self, text: &str) -> String {
        let root = format!("{}/", self.sb.root.display());
        text.replace(&root, "")
    }
}

fn why(e: &Ending) -> String {
    match e {
        Ending::Pass => "passed".to_string(),
        Ending::NotPass(d) | Ending::Crash(d) | Ending::BadHeader(d) => d.clone(),
        Ending::Fail { detail, .. } => detail.clone(),
        Ending::Timeout => "timed out".to_string(),
    }
}

/// Runs every case on the unmutated copy: the ones that pass, and the
/// ones that cannot judge, with the reason.
fn baseline(tool: &Tool, stage: &Stage, cases: Vec<Case>, say: Say) -> Result<Baseline, String> {
    let (mut passing, mut dropped) = (Vec::new(), Vec::new());
    for case in cases {
        let ending = tool.run_case(&stage.sb, &case.arg)?;
        say(&format!(
            "baseline {}: {}",
            case.key,
            if ending == Ending::Pass {
                "pass"
            } else {
                "not a pass"
            }
        ));
        match ending {
            Ending::Pass => passing.push(case),
            other => dropped.push((case.key, why(&other))),
        }
    }
    if passing.is_empty() {
        return Err(format!(
            "no selected case passes the unmutated module; dropped: {dropped:?}"
        ));
    }
    Ok((passing, dropped))
}

/// The cases that notice a module that cannot be read: the ones that load it.
fn control(
    tool: &Tool,
    stage: &Stage,
    src: &str,
    passing: Vec<Case>,
    say: Say,
) -> Result<Vec<Case>, String> {
    stage.write(&format!("{src}\n("))?;
    let mut loading = Vec::new();
    for case in passing {
        if tool.run_case(&stage.sb, &case.arg)? != Ending::Pass {
            loading.push(case);
        }
    }
    stage.write(src)?;
    say(&format!("control: {} cases load the module", loading.len()));
    if loading.is_empty() {
        return Err("the selected cases do not load the module: with it unreadable they all still pass, so every mutant would survive".to_string());
    }
    Ok(loading)
}

fn killed(stage: &Stage, case: &str, class: &'static str, detail: &str) -> Verdict {
    let detail: String = stage.scrub(detail).chars().take(240).collect();
    Verdict::Killed {
        case: case.to_string(),
        class,
        detail,
    }
}

/// The end of one mutant already written into the copy. `order` is the
/// order the cases are tried in; the one that kills goes to the front.
fn judge(
    tool: &Tool,
    stage: &Stage,
    active: &[Case],
    order: &mut Vec<usize>,
) -> Result<Verdict, String> {
    if let Some(main) = &stage.check {
        match tool.check(&stage.sb, main)? {
            Check::Compiles => {}
            Check::Invalid(why) => return Ok(Verdict::Invalid(stage.scrub(&why))),
            Check::Timeout => {
                return Ok(killed(
                    stage,
                    "(loading)",
                    "timeout",
                    "the module did not finish loading",
                ))
            }
            Check::Crash(why) => return Ok(killed(stage, "(loading)", "crash", &why)),
        }
    }
    for pos in 0..order.len() {
        let case = &active[order[pos]];
        let (class, detail) = match tool.run_case(&stage.sb, &case.arg)? {
            Ending::Pass => continue,
            Ending::NotPass(d) | Ending::BadHeader(d) => ("other", d),
            Ending::Fail { class, detail } => (class, detail),
            Ending::Timeout => ("timeout", "no answer within the time limit".to_string()),
            Ending::Crash(d) => ("crash", d),
        };
        let first = order.remove(pos);
        order.insert(0, first);
        return Ok(killed(stage, &case.key, class, &detail));
    }
    Ok(Verdict::Survived)
}

/// Runs a whole review. Nothing in `base` is written.
pub fn run(cfg: &Config, base: &Path, say: Say) -> Result<Report, String> {
    let plan = plan(cfg, base)?;
    let stage = Stage::new(cfg, base, &plan)?;
    let listed = casesel::list(&base.join(&stage.sb.cases))
        .map_err(|e| format!("cannot list the cases: {e}"))?;
    let selected = casesel::select(listed, &cfg.only)?;
    let (passing, dropped) = baseline(&cfg.tool, &stage, selected, say)?;
    let selected = passing.len();
    let active = control(&cfg.tool, &stage, &plan.src, passing, say)?;
    let mut order: Vec<usize> = (0..active.len()).collect();
    let mut done = Vec::new();
    for (id, mutant) in &plan.chosen {
        stage.write(&mutant.apply(&plan.src))?;
        let verdict = judge(&cfg.tool, &stage, &active, &mut order)?;
        let line = mutant.line(&plan.src);
        let d = Done {
            id: *id,
            line,
            mutant: mutant.clone(),
            verdict,
        };
        say(&crate::report::line(&d));
        done.push(d);
    }
    stage.write(&plan.src)?;
    let sites = OPERATORS
        .iter()
        .map(|op| (*op, plan.all.iter().filter(|m| m.op == *op).count()))
        .collect();
    Ok(Report {
        module: plan.module_rel.display().to_string(),
        tool: cfg.tool.kind.name().to_string(),
        seed: cfg.seed,
        src: plan.src,
        sites,
        filtered: plan.filtered,
        selected,
        dropped,
        loading: active.len(),
        done,
    })
}

#[cfg(test)]
mod tests;
