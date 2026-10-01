//! How the self-hosted expander is judged (spec/bootstrap.md §5): the
//! tool `compiler/expand.fib`, built by `fibc build`, must print exactly
//! what the Rust expander's dump is (`fibref::expand_dump`, what `fibref
//! expand` prints), with the same exit status, over
//!
//! 1. every program of `cases/ownership`, `cases/modules` (a directory
//!    with a `main.fib` is one program), `lib/` and `compiler/`, each
//!    told apart by whether it defines a macro;
//! 2. `lib/prelude.fib`, as a library prelude (`--prelude`);
//! 3. 360 generated programs from fixed seeds (`gen.rs`: mutations of the
//!    cases, token soup of the prelude macros, programs of user macros,
//!    programs of several modules, programs a limit stops) and 70 fixed
//!    ones, one for each way expansion can fail and the rewrites and
//!    nestings the cases show once (`edge.rs`);
//! 4. paths that cannot be read, and the tool run with words that name no
//!    program (`usage.rs`).
//!
//! Each is run plain, with `--context` (what the expander's context holds
//! after each module) and under three smaller limits. At **stage 2a** the
//! tool has no macro runner and every program is run with `--no-runner`
//! (a call of a user macro is the pending error `MacroNeedsEvaluator`); at
//! **stage 2b** the interpreter's macro evaluator is the oracle as well
//! (no flag). The stage is the environment variable
//! `BOOTSTRAP_EXPAND_STAGE` or else the marker file
//! `compiler/expand/stage`, which the porters change to `2b` when macros
//! run (`stage.rs`); a value that is neither is an error.
//!
//! **The tool does not exist yet** (M6 step 2 has not begun): until
//! `compiler/expand.fib` exists, `the_self_hosted_expander_matches_...`
//! skips with a loud message, and fails once the file exists and its
//! output differs. Everything the test is made of is judged without it:
//! the oracle over the whole corpus (`the_oracle_*`), the generators, the
//! comparison with stand-in tools that are faithful and damaged
//! (`standin.rs`), and with small real tools built by `fibc`
//! (`build.rs`).
//!
//! The comparison is byte for byte on standard output and on the exit
//! status (`compare.rs`, shared with `bootstrap.rs`); a failure names the
//! first line that differs and the input. Run it with
//! `LLVM_SYS_211_PREFIX=... cargo test -p fibc --test bootstrap_expand
//! -- --nocapture` (cargo shows the standard error of a passing test only
//! with `--nocapture`, which is where the skip message and the counts
//! are). At most four processes run at a time, 100 files to a process,
//! each capped at 4 GiB of address space and two minutes.

#![cfg(unix)]

// Shared with bootstrap.rs, which uses more of each than this does.
#[allow(dead_code)]
#[path = "bootstrap/compare.rs"]
mod compare;
#[allow(dead_code)]
#[path = "bootstrap/corpus.rs"]
mod corpus;
#[path = "bootstrap/rng.rs"]
mod rng;
#[path = "bootstrap/tmp.rs"]
mod tmp;
#[allow(dead_code)]
#[path = "bootstrap/tool.rs"]
mod tool;

#[path = "bootstrap_expand/build.rs"]
mod build;
#[path = "bootstrap_expand/edge.rs"]
mod edge;
#[path = "bootstrap_expand/gen.rs"]
mod gen;
#[path = "bootstrap_expand/groups.rs"]
mod groups;
#[path = "bootstrap_expand/modgen.rs"]
mod modgen;
#[path = "bootstrap_expand/mutate.rs"]
mod mutate;
#[path = "bootstrap_expand/programs.rs"]
mod programs;
#[path = "bootstrap_expand/run.rs"]
mod run;
#[path = "bootstrap_expand/soup.rs"]
mod soup;
#[path = "bootstrap_expand/stage.rs"]
mod stage;
#[path = "bootstrap_expand/standin.rs"]
mod standin;
#[path = "bootstrap_expand/usage.rs"]
mod usage;
#[path = "bootstrap_expand/usermacros.rs"]
mod usermacros;

use std::path::{Path, PathBuf};

use gen::Written;
use groups::Plan;
use stage::Stage;
use tmp::TempDir;

/// The stage's plan, the programs generated for it, and the text that
/// says what it is made of.
struct Prepared {
    plan: Plan,
    summary: String,
}

/// The programs a stage runs: the corpus, the generated and edge programs
/// written under `dir`, the prelude and the unreadable paths.
fn prepare(root: &Path, dir: &Path, stage: Stage) -> Prepared {
    let corpus = programs::programs(root);
    let sources: Vec<String> = corpus
        .iter()
        .filter(|p| p.main.starts_with(root.join("cases/ownership")))
        .filter_map(|p| std::fs::read_to_string(&p.main).ok())
        .collect();
    let mut inputs = gen::standard_run(&sources);
    let generated = inputs.len();
    inputs.extend(edge::edge_inputs());
    let written: Vec<Written> = gen::write_inputs(&dir.join("gen"), &inputs);
    let unreadable = corpus::unreadable_files(&dir.join("unreadable"));
    let plan = groups::plan(
        stage,
        &corpus,
        &written,
        &root.join("lib/prelude.fib"),
        &unreadable,
    );
    let summary = describe(
        stage,
        &plan,
        corpus.len(),
        generated,
        inputs.len() - generated,
    );
    Prepared { plan, summary }
}

fn describe(stage: Stage, plan: &Plan, corpus: usize, generated: usize, edge: usize) -> String {
    let how = match stage {
        Stage::A => "no macro runner: every program as --no-runner expands it",
        Stage::B => "the macro evaluator as well: every program with the macros run",
    };
    let groups: usize = plan.groups.iter().map(|g| g.files.len()).sum();
    format!(
        "stage {} ({how})\n  {} programs without a defmacro (the stage-2a list: they expand \
         completely with no runner), {} with one (the stage-2b list: they need the runner): \
         {corpus} of the corpus, {generated} generated and {edge} fixed, less the generated \
         programs that only a limit stops, which have groups of their own\n  {} groups, \
         {groups} runs of a file in all",
        stage.name(),
        plan.macro_free.len(),
        plan.with_defmacro.len(),
        plan.groups.len()
    )
}

/// Writes the two lists of the stage into the target directory, for a
/// person to read: the paths of the macro-free programs and of those with
/// a `defmacro`.
fn write_lists(plan: &Plan) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bootstrap_expand");
    std::fs::create_dir_all(&dir).expect("the target directory is writable");
    let list = |files: &[PathBuf]| {
        files
            .iter()
            .map(|f| format!("{}\n", f.display()))
            .collect::<String>()
    };
    std::fs::write(dir.join("stage-2a.list"), list(&plan.macro_free)).expect("writable");
    std::fs::write(dir.join("stage-2b.list"), list(&plan.with_defmacro)).expect("writable");
    dir
}

#[test]
fn the_self_hosted_expander_matches_the_rust_expander() {
    let root = build::root();
    let stage = stage::current(&root).unwrap_or_else(|e| panic!("{e}"));
    let dir = TempDir::new("expander");
    let prepared = prepare(&root, dir.path(), stage);
    let lists = write_lists(&prepared.plan);
    eprintln!("{}\n  lists: {}", prepared.summary, lists.display());
    let over = std::env::var_os(build::VARIABLE).map(PathBuf::from);
    let source = match build::expander_source(&root, over) {
        Ok(Some(source)) => source,
        Ok(None) => {
            eprintln!("{}", build::skip_message(&root));
            return;
        }
        Err(why) => panic!("{why}"),
    };
    let sut = build::build_expander(&source, dir.path(), &root);
    let (mut text, mut ok) = run::run_groups(&sut, &prepared.plan.groups);
    let refusals = usage::check_usage(&sut);
    ok &= refusals.is_empty();
    text.push_str(&format!(
        "usage: {} word lists, {} failing\n",
        usage::BAD_WORDS.len(),
        refusals.len()
    ));
    text.extend(refusals.iter().map(|f| f.to_string()));
    assert!(
        ok,
        "the self-hosted expander differs from the Rust expander:\n{text}"
    );
    eprintln!("{text}");
}

#[test]
fn the_lists_and_the_counts_say_what_each_stage_runs() {
    let root = build::root();
    for stage in [Stage::A, Stage::B] {
        let dir = TempDir::new(&format!("lists-{}", stage.name()));
        let prepared = prepare(&root, dir.path(), stage);
        let p = &prepared.plan;
        assert!(p.macro_free.len() >= 500, "{}", prepared.summary);
        assert!(p.with_defmacro.len() >= 100, "{}", prepared.summary);
        assert!(prepared
            .summary
            .starts_with(&format!("stage {} (", stage.name())));
        let all: std::collections::HashSet<&PathBuf> =
            p.macro_free.iter().chain(&p.with_defmacro).collect();
        assert_eq!(
            all.len(),
            p.macro_free.len() + p.with_defmacro.len(),
            "no program is in both"
        );
        eprintln!("{}", prepared.summary);
    }
}

/// The kinds of error the standard run must reach at least once: the
/// variants of `ExpandErrorKind` that a program can provoke, the ways a
/// program may fail to load, and a read error. A list written here, not
/// derived from the enums, so that a variant added later is noticed to be
/// unreached (the harness cannot judge what it never runs).
const REACHED: [&str; 31] = [
    "MacroNeedsEvaluator",
    "MacroPhase",
    "MacroFailed",
    "MacroArity",
    "UnquoteOutsideQuasiquote",
    "SpliceOutsideList",
    "Malformed",
    "MacroNamesCoreForm",
    "DeriveProtocol",
    "DeriveTarget",
    "NotAStruct",
    "NotAnEnum",
    "BadReflection",
    "ThreadStep",
    "InOutOutsideArgument",
    "NilCalled",
    "BraceInPattern",
    "ExpressionAtTopLevel",
    "DefinitionInExpression",
    "NsNotFirst",
    "TooManySteps",
    "TooDeep",
    "TooLarge",
    "BadLiteral",
    "AmbiguousMacro",
    "BadNs",
    "ModuleMissing",
    "ModuleCycle",
    "ModuleMismatch",
    "Unclosed",
    "MismatchedClose",
];

#[test]
fn the_oracle_expands_every_group_of_both_stages_and_reaches_every_kind_of_error() {
    use std::collections::BTreeMap;
    let root = build::root();
    let dir = TempDir::new("oracle-all");
    let started = std::time::Instant::now();
    let prepared = prepare(&root, dir.path(), Stage::B);
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for group in &prepared.plan.groups {
        let group_started = std::time::Instant::now();
        let outcome = run::oracle(&group.files, &group.opts);
        eprintln!(
            "  {} ({} files, {} bytes) took {:?}",
            group.label,
            group.files.len(),
            outcome.stdout.len(),
            group_started.elapsed()
        );
        let text = String::from_utf8_lossy(&outcome.stdout);
        for bad in ["internal error", "cannot start the expander"] {
            assert!(
                !text.contains(bad),
                "{}: the oracle failed: {bad}",
                group.label
            );
        }
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("error ") {
                let kind = rest.split(' ').next().unwrap_or("");
                *kinds.entry(kind.to_string()).or_insert(0) += 1;
            }
        }
    }
    eprintln!(
        "the oracle over every group of stage 2b took {:?}: {kinds:?}",
        started.elapsed()
    );
    for kind in REACHED {
        assert!(
            kinds.contains_key(kind),
            "no program reaches {kind}: {kinds:?}"
        );
    }
}

#[test]
#[ignore = "writes the stage's programs where a person can look at them: BOOTSTRAP_EXPAND_KEEP=DIR"]
fn keep_the_generated_programs_in_a_directory() {
    let Some(keep) = std::env::var_os("BOOTSTRAP_EXPAND_KEEP").map(PathBuf::from) else {
        return;
    };
    let prepared = prepare(&build::root(), &keep, Stage::B);
    eprintln!("{}", prepared.summary);
}

/// The index, at or after `from`, of a file whose dump has a line below
/// the module header, which is what a stand-in's `Damage::Character`
/// changes.
fn damageable(files: &[PathBuf], opts: &fibref::expand_dump::Options, from: usize) -> usize {
    (from..files.len())
        .find(|i| {
            let outcome = run::oracle(&files[*i..=*i], opts);
            outcome.stdout.iter().filter(|b| **b == b'\n').count() >= 3
        })
        .expect("some file has a dump of three lines")
}

/// The pipeline over the whole corpus, with stand-ins that replay the
/// oracle: faithful, it finds nothing (over the real group, in batches of
/// 100); with
/// a character of one file's dump changed, it names that file and no other,
/// from the middle of the group, past the first batch.
#[test]
fn a_fault_planted_in_the_replay_of_the_whole_corpus_is_reported_and_localized() {
    let root = build::root();
    let dir = TempDir::new("canary");
    let prepared = prepare(&root, &dir.path().join("p"), Stage::A);
    let group = prepared
        .plan
        .groups
        .iter()
        .find(|g| g.label == "corpus (no runner, plain)")
        .expect("the corpus group");
    assert!(group.files.len() > 1000, "{}", group.files.len());
    let sut = |damage| run::Sut {
        tool: standin::replay_tool(&dir.path().join("t"), &group.files, &group.opts, damage),
        cwd: root.clone(),
    };
    let faithful = run::check_group(&sut(standin::Damage::Faithful), group);
    assert!(faithful.is_empty(), "{}", run::report(group, &faithful));
    let at = damageable(&group.files, &group.opts, group.files.len() / 2);
    let failures = run::check_group(&sut(standin::Damage::Character(at)), group);
    assert_eq!(failures.len(), 1, "{}", run::report(group, &failures));
    assert_eq!(
        failures[0].subject,
        group.files[at].display().to_string(),
        "{}",
        failures[0]
    );
    eprintln!(
        "canary: one character of {} changed in the replay of {} files: {} failing, named",
        group.files[at].display(),
        group.files.len(),
        failures.len()
    );
}
