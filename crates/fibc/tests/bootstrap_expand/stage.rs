//! Which stage of the expander port is judged (spec/bootstrap.md §5):
//! `2a`, the expander with no user macros (a call of one is the pending
//! error of `NoRunner`), or `2b`, which runs them through
//! `compiler/lair/expand.fib`. The porters switch to `2b` when it works:
//! the environment variable `BOOTSTRAP_EXPAND_STAGE` says it for one run,
//! and the marker file `compiler/expand/stage` says it for the
//! repository; with neither, the stage is `2a`.

use std::path::Path;

/// The environment variable that names the stage of one run.
pub const VARIABLE: &str = "BOOTSTRAP_EXPAND_STAGE";

/// The marker file, under the repository root.
pub const MARKER: &str = "compiler/expand/stage";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// No user macros: every program is compared as `--no-runner` expands
    /// it.
    A,
    /// User macros run: 2a, and every program compared as the
    /// interpreter's macro evaluator expands it.
    B,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::A => "2a",
            Stage::B => "2b",
        }
    }

    pub fn parse(text: &str) -> Option<Stage> {
        match text.trim() {
            "2a" => Some(Stage::A),
            "2b" => Some(Stage::B),
            _ => None,
        }
    }
}

/// The first line of a marker file that is not blank and not a `;`
/// comment.
fn marker_stage(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with(';'))
}

/// The stage of this run: the variable (`var`) when it is set, else the
/// marker file's text (`marker`, when the file exists), else `2a`. A
/// value that is not a stage is an error, not a quiet default: a typo
/// must not judge the wrong stage.
pub fn choose(var: Option<String>, marker: Option<String>) -> Result<Stage, String> {
    if let Some(v) = var {
        return Stage::parse(&v).ok_or_else(|| format!("{VARIABLE}={v:?} is not 2a or 2b"));
    }
    match marker.as_deref().map(|m| marker_stage(m).unwrap_or("")) {
        None => Ok(Stage::A),
        Some(line) => Stage::parse(line)
            .ok_or_else(|| format!("{MARKER} says {line:?}, which is not 2a or 2b")),
    }
}

/// The stage under `root`, from the process environment and the marker.
pub fn current(root: &Path) -> Result<Stage, String> {
    choose(
        std::env::var(VARIABLE).ok(),
        std::fs::read_to_string(root.join(MARKER)).ok(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_wins_then_the_marker_then_2a() {
        assert_eq!(choose(None, None), Ok(Stage::A));
        assert_eq!(choose(None, Some("2b\n".into())), Ok(Stage::B));
        assert_eq!(choose(Some("2a".into()), Some("2b".into())), Ok(Stage::A));
        assert_eq!(choose(Some("2b".into()), None), Ok(Stage::B));
    }

    #[test]
    fn a_marker_may_start_with_comments_and_blank_lines() {
        let text = "; the stage of the expander port\n\n  2b  \n";
        assert_eq!(choose(None, Some(text.into())), Ok(Stage::B));
    }

    #[test]
    fn a_value_that_is_not_a_stage_is_an_error_not_a_default() {
        assert!(choose(Some("2c".into()), None).is_err());
        assert!(choose(None, Some("".into())).is_err());
        assert!(choose(None, Some("; only a comment\n".into())).is_err());
        assert!(choose(None, Some("b".into())).is_err());
    }

    #[test]
    fn the_stages_name_themselves() {
        assert_eq!(Stage::A.name(), "2a");
        assert_eq!(Stage::B.name(), "2b");
        assert_eq!(Stage::parse(" 2a\n"), Some(Stage::A));
    }
}
