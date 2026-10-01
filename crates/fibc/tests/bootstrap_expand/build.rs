//! The tool under test: `compiler/expand.fib` built by `fibc build`, and
//! the tests that show the whole path (build, run, compare) judges a real
//! program: small tools written in fibber that print the oracle's text,
//! or something else.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::run::Sut;
use crate::tool::{repo_root, Tool};

/// The environment variable that names another expander source to build
/// in place of `compiler/expand.fib` (a copy of `compiler/` being tried,
/// as `BOOTSTRAP_READER` does for the reader).
pub const VARIABLE: &str = "BOOTSTRAP_EXPANDER";

/// What the test builds: the source named by `over` (it must exist: a
/// path that does not is a mistake, not a reason to skip), else
/// `compiler/expand.fib` of `root`, or nothing when that does not exist
/// yet (M6 step 2 has not begun).
pub fn expander_source(root: &Path, over: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    if let Some(path) = over {
        return if path.is_file() {
            Ok(Some(path))
        } else {
            Err(format!("{VARIABLE}={} is not a file", path.display()))
        };
    }
    let path = root.join("compiler/expand.fib");
    Ok(path.is_file().then_some(path))
}

/// The message of the skipped test: loud, and says how to stop skipping.
pub fn skip_message(root: &Path) -> String {
    format!(
        "\n*** SKIPPED: {} does not exist yet, so the self-hosted expander is not \
         compared with the Rust one. The corpus, the generated programs and the oracle \
         over them were checked; nothing was compared. The test fails, not skips, once \
         the file exists and its output differs. ***\n",
        root.join("compiler/expand.fib").display()
    )
}

/// Builds the expander `source` into `dir` with `fibc build` and returns
/// the tool and the directory to run it in (the repository root, where it
/// finds `lib/prelude.fib`).
pub fn build_expander(source: &Path, dir: &Path, root: &Path) -> Sut {
    let exe = dir.join("expand");
    let out = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .arg("build")
        .arg(source)
        .arg("-o")
        .arg(&exe)
        .current_dir(root)
        .output()
        .expect("fibc runs");
    assert!(
        out.status.success(),
        "fibc build {} failed ({}):\n{}{}",
        source.display(),
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Sut {
        tool: Tool::program(&exe),
        cwd: root.to_path_buf(),
    }
}

/// The repository root of this test.
pub fn root() -> PathBuf {
    repo_root()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{check_group, Group};
    use crate::tmp::TempDir;
    use fibref::expand_dump::Options;

    /// `text` as a fibber string literal.
    fn literal(text: &str) -> String {
        let mut out = String::from("\"");
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    /// A tool that prints `== FILE` for its last argument, then `body`
    /// and a newline, and ends with `status`; `trap` makes it trap.
    fn toy(body: &str, status: i64, trap: bool) -> String {
        let end = if trap {
            "(trap \"todo: expand\")".to_string()
        } else {
            status.to_string()
        };
        format!(
            "(defun main () -> i64\n  (let ((all (args)))\n    (do (println (str-join [\"== \" (nth all (- (count all) 1)) \"\\n\" {}]))\n        {end})))\n",
            literal(body)
        )
    }

    /// One small input, the toy's source and the oracle's text for it.
    struct Fixture {
        dir: TempDir,
        file: PathBuf,
        body: String,
    }

    fn fixture(label: &str) -> Fixture {
        let dir = TempDir::new(label);
        let file = dir.path().join("in.fib");
        std::fs::write(
            &file,
            "(defun f (x: i64) -> i64 (when x \"é\" 2))\n(derive Eq Option)\n",
        )
        .expect("writable");
        let oracle = crate::run::oracle(std::slice::from_ref(&file), &Options::default());
        let text = String::from_utf8(oracle.stdout).expect("UTF-8");
        let body = text
            .strip_prefix(&format!("== {}\n", file.display()))
            .and_then(|b| b.strip_suffix('\n'))
            .expect("a header and a final newline")
            .to_string();
        Fixture { dir, file, body }
    }

    fn judged(f: &Fixture, source: &str) -> Vec<crate::run::Failure> {
        let src = f.dir.path().join("toy.fib");
        std::fs::write(&src, source).expect("writable");
        let sut = build_expander(&src, f.dir.path(), &repo_root());
        check_group(
            &sut,
            &Group::new("toy", vec![f.file.clone()], Options::default()),
        )
    }

    #[test]
    fn a_tool_built_by_fibc_that_prints_what_the_oracle_prints_is_not_reported() {
        let f = fixture("build-faithful");
        let failures = judged(&f, &toy(&f.body, 0, false));
        assert!(
            failures.is_empty(),
            "{}",
            failures.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn a_tool_built_by_fibc_that_prints_one_character_differently_is_reported() {
        let f = fixture("build-character");
        let damaged = f.body.replacen("sym \"f\"", "sym \"g\"", 1);
        assert_ne!(damaged, f.body);
        let failures = judged(&f, &toy(&damaged, 0, false));
        assert_eq!(failures.len(), 1);
        let text = failures[0].to_string();
        assert!(text.contains("sym \\\"g\\\""), "{text}");
        assert!(text.contains("rust expander:"), "{text}");
    }

    #[test]
    fn a_tool_built_by_fibc_with_another_exit_status_or_that_traps_is_reported() {
        let f = fixture("build-status");
        let failures = judged(&f, &toy(&f.body, 1, false));
        assert_eq!(failures.len(), 1);
        assert!(failures[0]
            .to_string()
            .contains("exit status: rust expander 0, fibber expander 1"));
        let failures = judged(&f, &toy("", 0, true));
        assert_eq!(failures.len(), 1);
        let text = failures[0].to_string();
        assert!(text.contains("todo: expand"), "{text}");
    }

    /// A tool that prints each of its words on a line and then whether
    /// `lib/prelude.fib` can be read from where it runs.
    const ECHO: &str = "(defun main () -> i64
  (let ((all (args)))
    (do (loop ((i 0))
          (if (< i (count all)) (do (println (nth all i)) (recur (+ i 1))) ()))
        (match (read-file \"lib/prelude.fib\")
          (nil (println \"no prelude\"))
          ((some text) (println (if (> (count (str-chars text)) 1000) \"prelude read\" \"short\"))))
        0)))
";

    #[test]
    fn a_tool_built_by_fibc_gets_the_options_before_the_files_and_runs_in_the_repository() {
        let f = fixture("build-words");
        let src = f.dir.path().join("echo.fib");
        std::fs::write(&src, ECHO).expect("writable");
        let sut = build_expander(&src, f.dir.path(), &repo_root());
        let opts = Options {
            context: true,
            runner: fibref::expand_dump::RunnerKind::None,
            limits: fibref::expand_dump::LimitOverrides {
                steps: Some(7),
                ..Default::default()
            },
            ..Options::default()
        };
        let run = crate::run::run_tool(&sut, &opts, &[f.file.clone(), f.dir.path().join("b.fib")]);
        assert_eq!(run.outcome.status, 0, "{}", run.stderr);
        let text = String::from_utf8(run.outcome.stdout).expect("UTF-8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[..5],
            [
                "--context",
                "--no-runner",
                "--max-steps",
                "7",
                &f.file.display().to_string()
            ]
        );
        assert!(lines[5].ends_with("b.fib"), "{lines:?}");
        assert_eq!(
            lines[6], "prelude read",
            "run from the repository root: {lines:?}"
        );
    }

    #[test]
    fn the_source_is_expand_fib_when_it_exists_and_nothing_when_it_does_not() {
        let dir = TempDir::new("build-source");
        let root = dir.path();
        assert_eq!(expander_source(root, None), Ok(None));
        std::fs::create_dir_all(root.join("compiler")).expect("writable");
        std::fs::write(
            root.join("compiler/expand.fib"),
            "(defun main () -> i64 0)\n",
        )
        .expect("writable");
        assert_eq!(
            expander_source(root, None),
            Ok(Some(root.join("compiler/expand.fib")))
        );
    }

    #[test]
    fn a_named_source_that_is_not_a_file_is_an_error_and_not_a_skip() {
        let dir = TempDir::new("build-over");
        let missing = dir.path().join("nope.fib");
        assert!(expander_source(dir.path(), Some(missing)).is_err());
        let present = dir.path().join("here.fib");
        std::fs::write(&present, "").expect("writable");
        assert_eq!(
            expander_source(dir.path(), Some(present.clone())),
            Ok(Some(present))
        );
    }

    #[test]
    fn the_skip_message_names_the_file_and_says_that_nothing_was_compared() {
        let m = skip_message(Path::new("/r"));
        assert!(
            m.contains("SKIPPED") && m.contains("/r/compiler/expand.fib"),
            "{m}"
        );
        assert!(m.contains("nothing was compared"), "{m}");
    }
}
