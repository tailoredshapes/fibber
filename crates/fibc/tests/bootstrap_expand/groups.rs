//! What is run for a stage (spec/bootstrap.md §5): the groups of files
//! and the words each is run with, and the lists the stage is made of.
//!
//! Stage 2a compares every program, as `--no-runner` expands it: a
//! program with no `defmacro` is expanded as any expander does, and one
//! with it up to the first call of a user macro, which is the pending
//! error `MacroNeedsEvaluator`. Stage 2b adds the interpreter's macro
//! evaluator as the oracle for every program, so that the macros are
//! really run.

use std::path::{Path, PathBuf};

use fibref::expand_dump::{LimitOverrides, Options, RunnerKind};

use crate::gen::{Kind, Written};
use crate::programs::Program;
use crate::run::Group;
use crate::stage::Stage;

/// The smaller limits the corpus is also expanded under, each of which a
/// good part of the cases reach: two macro expansions, nesting of ten,
/// 150 forms.
pub const LIMITS: [LimitOverrides; 3] = [
    LimitOverrides {
        steps: Some(2),
        depth: None,
        forms: None,
    },
    LimitOverrides {
        steps: None,
        depth: Some(10),
        forms: None,
    },
    LimitOverrides {
        steps: None,
        depth: None,
        forms: Some(150),
    },
];

/// The limits that keep an expansion by the evaluator short on a
/// generated program, whose mutated macro may call itself: the default
/// limits would let it run for minutes.
const GENERATED_CAP: LimitOverrides = LimitOverrides {
    steps: Some(500),
    depth: None,
    forms: Some(50_000),
};

/// What a stage runs: the groups, and the corpus programs by kind.
pub struct Plan {
    pub groups: Vec<Group>,
    /// The corpus programs without a `defmacro` in any module: expanded
    /// the same with a runner or without one.
    pub macro_free: Vec<PathBuf>,
    /// The corpus programs with one: expanded without a runner up to the
    /// first call of a user macro (stage 2a), and with the evaluator
    /// (stage 2b).
    pub with_defmacro: Vec<PathBuf>,
}

/// The runners the stage compares with, in order.
fn runners(stage: Stage) -> Vec<RunnerKind> {
    match stage {
        Stage::A => vec![RunnerKind::None],
        Stage::B => vec![RunnerKind::None, RunnerKind::Evaluator],
    }
}

fn runner_word(runner: RunnerKind) -> &'static str {
    match runner {
        RunnerKind::None => "no runner",
        RunnerKind::Evaluator => "evaluator",
    }
}

fn paths<'a>(mains: impl Iterator<Item = &'a PathBuf>) -> Vec<PathBuf> {
    mains.cloned().collect()
}

/// The groups of one runner over one set of files: plain, with the
/// context, and under each of the smaller limits.
fn over(
    label: &str,
    runner: RunnerKind,
    files: &[PathBuf],
    cap: Option<LimitOverrides>,
) -> Vec<Group> {
    let capped = |limits: LimitOverrides| LimitOverrides {
        steps: limits.steps.or(cap.and_then(|c| c.steps)),
        depth: limits.depth.or(cap.and_then(|c| c.depth)),
        forms: limits.forms.or(cap.and_then(|c| c.forms)),
    };
    let base = |limits: LimitOverrides, context: bool| Options {
        context,
        runner,
        limits: capped(limits),
        ..Options::default()
    };
    let name = |what: &str| format!("{label} ({}, {what})", runner_word(runner));
    let mut groups = vec![
        Group::new(
            &name("plain"),
            files.to_vec(),
            base(LimitOverrides::default(), false),
        ),
        Group::new(
            &name("context"),
            files.to_vec(),
            base(LimitOverrides::default(), true),
        ),
    ];
    for limits in LIMITS {
        let what = format!("limits {limits:?}");
        groups.push(Group::new(
            &name(&what),
            files.to_vec(),
            base(limits, false),
        ));
    }
    groups
}

/// The directory of the programs that are run with implicit modules.
const IMPLICIT_DIR: &str = "compiler/tests/expand/implicit";

/// The implicit modules and the words each group of the implicit programs
/// is run with: `--implicit-lib`, then `--implicit` and `--context`.
const IMPLICIT_RUNS: [(&str, bool, bool); 6] = [
    ("fib.x,fib.y", false, false),
    ("fib.x,fib.y", true, false),
    ("fib.x,fib.y", true, true),
    ("fib.x,fib.y", false, true),
    ("fib.y,fib.x", true, false),
    ("", true, true),
];

/// The groups of the programs `main-*.fib` of `compiler/tests/expand/implicit`,
/// which the groups above run with no implicit module (the default, today):
/// each is run with implicit modules as well (`--implicit-lib`), their
/// sections printed or left out (`--implicit`) and with the context.
fn implicit_groups(stage: Stage, programs: &[Program]) -> Vec<Group> {
    let files: Vec<PathBuf> = programs
        .iter()
        .map(|p| &p.main)
        .filter(|m| m.parent().is_some_and(|d| d.ends_with(IMPLICIT_DIR)))
        .filter(|m| {
            m.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("main-"))
        })
        .cloned()
        .collect();
    let mut groups = Vec::new();
    for runner in runners(stage).into_iter().filter(|_| !files.is_empty()) {
        for (lib, implicit, context) in IMPLICIT_RUNS {
            let opts = Options {
                context,
                implicit,
                implicit_lib: Some(
                    lib.split(',')
                        .filter(|n| !n.is_empty())
                        .map(String::from)
                        .collect(),
                ),
                runner,
                ..Options::default()
            };
            let what = format!("implicit-lib {lib:?}, implicit {implicit}, context {context}");
            let label = format!("implicit modules ({}, {what})", runner_word(runner));
            groups.push(Group::new(&label, files.clone(), opts));
        }
    }
    groups
}

/// The groups of the programs a limit must stop: each with the option
/// that stops it and a small value for it, under the evaluator only (with
/// no runner the call is pending before a limit is reached).
fn limit_groups(written: &[Written]) -> Vec<Group> {
    [
        (
            "--max-steps",
            LimitOverrides {
                steps: Some(6),
                depth: None,
                forms: None,
            },
        ),
        (
            "--max-forms",
            LimitOverrides {
                steps: None,
                depth: None,
                forms: Some(120),
            },
        ),
        (
            "--max-depth",
            LimitOverrides {
                steps: None,
                depth: Some(10),
                forms: None,
            },
        ),
    ]
    .into_iter()
    .filter_map(|(flag, limits)| {
        let files: Vec<PathBuf> = written
            .iter()
            .filter(|w| w.kind == Kind::Limit(flag))
            .map(|w| w.main.clone())
            .collect();
        let opts = Options {
            runner: RunnerKind::Evaluator,
            limits,
            ..Options::default()
        };
        (!files.is_empty()).then(|| Group::new(&format!("macros stopped by {flag}"), files, opts))
    })
    .collect()
}

/// `lib/prelude.fib` as a library prelude, plain and with its context.
fn prelude_groups(prelude: &Path) -> Vec<Group> {
    [false, true]
        .into_iter()
        .map(|context| {
            let opts = Options {
                prelude: true,
                context,
                ..Options::default()
            };
            Group::new("the prelude", vec![prelude.to_path_buf()], opts)
        })
        .collect()
}

/// The programs a runner is needed for and those it is not, as two
/// lists: the corpus and the generated ones that are not stopped by a
/// limit (those have groups of their own and are not listed).
fn classified(programs: &[Program], written: &[Written]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let listed: Vec<(&PathBuf, bool)> = programs
        .iter()
        .map(|p| (&p.main, p.defmacro))
        .chain(
            written
                .iter()
                .filter(|w| !matches!(w.kind, Kind::Limit(_)))
                .map(|w| (&w.main, w.defmacro)),
        )
        .collect();
    let kind = |defmacro: bool| {
        paths(
            listed
                .iter()
                .filter(|(_, d)| *d == defmacro)
                .map(|(main, _)| *main),
        )
    };
    (kind(false), kind(true))
}

/// The groups of `stage` over the corpus `programs`, the generated and
/// edge programs `written`, the prelude and the unreadable paths.
pub fn plan(
    stage: Stage,
    programs: &[Program],
    written: &[Written],
    prelude: &Path,
    unreadable: &[PathBuf],
) -> Plan {
    let corpus = paths(programs.iter().map(|p| &p.main));
    let generated = paths(
        written
            .iter()
            .filter(|w| !matches!(w.kind, Kind::Limit(_)))
            .map(|w| &w.main),
    );
    let mut groups = Vec::new();
    for runner in runners(stage) {
        let cap = (runner == RunnerKind::Evaluator).then_some(GENERATED_CAP);
        groups.extend(over("corpus", runner, &corpus, None));
        groups.extend(over("generated", runner, &generated, cap));
    }
    groups.extend(implicit_groups(stage, programs));
    if stage == Stage::B {
        groups.extend(limit_groups(written));
    }
    groups.extend(prelude_groups(prelude));
    groups.push(Group::new(
        "unreadable paths",
        unreadable.to_vec(),
        Options::default(),
    ));
    let (macro_free, with_defmacro) = classified(programs, written);
    Plan {
        groups,
        macro_free,
        with_defmacro,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibref::expand_dump::flags;

    fn program(name: &str, defmacro: bool) -> Program {
        Program {
            main: PathBuf::from(name),
            defmacro,
        }
    }

    fn written(kind: Kind, name: &str, defmacro: bool) -> Written {
        Written {
            kind,
            main: PathBuf::from(name),
            defmacro,
        }
    }

    fn sample(stage: Stage) -> Plan {
        let programs = [program("a.fib", false), program("b.fib", true)];
        let generated = [
            written(Kind::Soup, "s.fib", false),
            written(Kind::Limit("--max-steps"), "l.fib", true),
            written(Kind::Edge, "e.fib", true),
        ];
        plan(
            stage,
            &programs,
            &generated,
            Path::new("p.fib"),
            &[PathBuf::from("u.fib")],
        )
    }

    #[test]
    fn stage_2a_runs_everything_with_no_runner_and_counts_the_programs_by_kind() {
        let p = sample(Stage::A);
        assert_eq!(
            p.macro_free,
            [PathBuf::from("a.fib"), PathBuf::from("s.fib")]
        );
        assert_eq!(
            p.with_defmacro,
            [PathBuf::from("b.fib"), PathBuf::from("e.fib")]
        );
        assert!(p.groups.iter().all(|g| g.opts.prelude
            || g.label.starts_with("unreadable")
            || g.opts.runner == RunnerKind::None));
        assert!(p
            .groups
            .iter()
            .all(|g| !g.label.starts_with("macros stopped")));
    }

    #[test]
    fn stage_2b_adds_the_evaluator_over_every_program_and_the_limit_programs() {
        let p = sample(Stage::B);
        let evaluator: Vec<&Group> = p
            .groups
            .iter()
            .filter(|g| g.opts.runner == RunnerKind::Evaluator && !g.opts.prelude)
            .collect();
        assert!(evaluator.iter().any(|g| g.label.starts_with("corpus")));
        assert!(evaluator
            .iter()
            .any(|g| g.label == "macros stopped by --max-steps"));
        let stopped = p
            .groups
            .iter()
            .find(|g| g.label.starts_with("macros stopped"))
            .expect("a group");
        assert_eq!(stopped.files, [PathBuf::from("l.fib")]);
        assert_eq!(flags(&stopped.opts), ["--max-steps", "6"]);
    }

    #[test]
    fn every_stage_has_the_prelude_the_context_the_limits_and_the_unreadable_paths() {
        for stage in [Stage::A, Stage::B] {
            let p = sample(stage);
            let has = |f: &dyn Fn(&Group) -> bool| p.groups.iter().any(f);
            assert!(has(&|g| g.opts.prelude
                && !g.opts.context
                && g.files == [PathBuf::from("p.fib")]));
            assert!(has(&|g| g.opts.prelude && g.opts.context));
            assert!(has(&|g| g.label.starts_with("corpus") && g.opts.context));
            for l in LIMITS {
                assert!(
                    has(&|g| g.label.starts_with("corpus") && g.opts.limits == l),
                    "{l:?}"
                );
            }
            assert!(has(
                &|g| g.label == "unreadable paths" && g.files == [PathBuf::from("u.fib")]
            ));
        }
    }

    #[test]
    fn the_limit_programs_are_not_in_the_plain_groups_and_the_edge_ones_are() {
        let p = sample(Stage::A);
        let generated = p
            .groups
            .iter()
            .find(|g| g.label.starts_with("generated (no runner, plain"))
            .expect("a group");
        assert_eq!(
            generated.files,
            [PathBuf::from("s.fib"), PathBuf::from("e.fib")]
        );
    }

    #[test]
    fn a_generated_group_under_the_evaluator_is_capped_and_the_corpus_is_not() {
        let p = sample(Stage::B);
        let cap = |label: &str| {
            p.groups
                .iter()
                .find(|g| g.label == label)
                .map(|g| g.opts.limits)
                .expect("a group")
        };
        assert_eq!(cap("generated (evaluator, plain)").steps, Some(500));
        assert_eq!(cap("generated (evaluator, plain)").forms, Some(50_000));
        assert_eq!(cap("corpus (evaluator, plain)"), LimitOverrides::default());
    }

    #[test]
    fn the_implicit_programs_are_run_with_implicit_modules_and_each_way_of_printing_them() {
        let dir = format!("/r/{IMPLICIT_DIR}");
        let programs = [
            program("a.fib", false),
            program(&format!("{dir}/main-a.fib"), false),
            program(&format!("{dir}/util.fib"), false),
            program(&format!("{dir}/fib/main-b.fib"), false),
        ];
        for stage in [Stage::A, Stage::B] {
            let p = plan(stage, &programs, &[], Path::new("p.fib"), &[]);
            let ours: Vec<&Group> = p
                .groups
                .iter()
                .filter(|g| g.label.starts_with("implicit modules"))
                .collect();
            assert_eq!(ours.len(), IMPLICIT_RUNS.len() * runners(stage).len());
            let main = PathBuf::from(format!("{dir}/main-a.fib"));
            assert!(ours.iter().all(|g| g.files == [main.clone()]));
            let words: Vec<Vec<String>> = ours.iter().map(|g| flags(&g.opts)).collect();
            let has = |w: &[&str]| words.contains(&w.iter().map(|s| s.to_string()).collect());
            let (x, y) = ("fib.x,fib.y", "fib.y,fib.x");
            let with = |w: &[&str]| has(&[w, &["--no-runner"]].concat());
            assert!(with(&["--context", "--implicit", "--implicit-lib", x]));
            assert!(with(&["--implicit", "--implicit-lib", y]));
            assert!(with(&["--context", "--implicit", "--implicit-lib", ""]));
            // the evaluator's groups are stage 2b's
            assert_eq!(has(&["--implicit", "--implicit-lib", y]), stage == Stage::B);
        }
        let only = [program("a.fib", false)];
        let none = plan(Stage::A, &only, &[], Path::new("p.fib"), &[]);
        let ours = |g: &Group| g.label.starts_with("implicit modules");
        assert!(none.groups.iter().all(|g| !ours(g)));
    }
}
