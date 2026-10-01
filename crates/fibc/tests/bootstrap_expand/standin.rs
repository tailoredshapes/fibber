//! A stand-in for the expander tool, to show the harness can fail.
//!
//! The stand-in is a shell script that replays, for each file named on
//! its command line, a section and a status it was given beforehand: the
//! oracle's own, or the oracle's with one kind of damage. Run through the
//! same pipeline as the real tool (batches, the process wrapper, the
//! words before the files, the directory, the byte comparison, the
//! localization of a failure to a file) it must pass when faithful and
//! fail, in the right way, when damaged.

use std::path::{Path, PathBuf};

use fibref::expand_dump::Options;

use crate::compare::Outcome;
use crate::run::oracle;
use crate::tool::Tool;

/// What the stand-in gets wrong; the number is the index of the file whose
/// replay is damaged.
#[derive(Clone, Copy, Debug)]
pub enum Damage {
    /// Nothing: a tool that agrees with the oracle everywhere.
    Faithful,
    /// The first character of the first line below the module header is
    /// changed.
    Character(usize),
    /// The last line of the file's section is missing.
    MissingLine(usize),
    /// The tool's exit status is one more (mod 3) than the oracle's.
    Status(usize),
    /// The whole output ends without its final newline.
    TrailingNewline,
    /// The tool is killed by a signal when it reaches the file.
    Crash(usize),
}

/// Replays `sh script WORD.. FILE..`: for each argument that is a file it
/// has a section for (kept under the file's whole path, each `/` a `%`),
/// the section, then the largest status; it logs the words of each run,
/// and the directory it ran in.
const SCRIPT: &str = "\
here=$(dirname \"$0\")
printf '%s\\n' \"$*\" >> \"$here/words.log\"
pwd >> \"$here/cwd.log\"
status=0
for a in \"$@\"; do
  b=$(printf '%s' \"$a\" | tr '/' '%')
  if [ -e \"$here/$b.section\" ]; then
    if [ -e \"$here/$b.crash\" ]; then kill -ABRT $$; fi
    cat \"$here/$b.section\"
    s=$(cat \"$here/$b.status\")
    if [ \"$s\" -gt \"$status\" ]; then status=$s; fi
  fi
done
exit $status
";

/// The name a file's replay is kept under: its whole path, `/` as `%`.
fn key(file: &Path) -> String {
    file.to_string_lossy().replace('/', "%")
}

/// Writes a stand-in for `files` under `opts` into `dir`, with `damage`,
/// and returns it.
pub fn replay_tool(dir: &Path, files: &[PathBuf], opts: &Options, damage: Damage) -> Tool {
    std::fs::create_dir_all(dir).expect("writable");
    let script = dir.join("replay.sh");
    std::fs::write(&script, SCRIPT).expect("writable");
    for (i, file) in files.iter().enumerate() {
        let name = key(file);
        let mut replay = oracle(std::slice::from_ref(file), opts);
        damage_replay(dir, &name, damage, (i, files.len()), &mut replay);
        std::fs::write(dir.join(format!("{name}.section")), &replay.stdout).expect("writable");
        std::fs::write(
            dir.join(format!("{name}.status")),
            replay.status.to_string(),
        )
        .expect("writable");
    }
    Tool::script(&script)
}

fn damage_replay(dir: &Path, name: &str, damage: Damage, at: (usize, usize), replay: &mut Outcome) {
    let (i, total) = at;
    match damage {
        Damage::Character(n) if n == i => change_third_line(&mut replay.stdout),
        Damage::MissingLine(n) if n == i => drop_last_line(&mut replay.stdout),
        Damage::Status(n) if n == i => replay.status = (replay.status + 1) % 3,
        Damage::TrailingNewline if i + 1 == total => {
            assert_eq!(replay.stdout.pop(), Some(b'\n'));
        }
        Damage::Crash(n) if n == i => {
            std::fs::write(dir.join(format!("{name}.crash")), "").expect("writable");
        }
        _ => {}
    }
}

/// Changes the first character of the third line (the first below the
/// header and the module line) to a different one.
fn change_third_line(text: &mut [u8]) {
    let mut starts = text
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .map(|(i, _)| i + 1);
    let third = starts.nth(1).expect("a section of three lines");
    assert!(text[third].is_ascii(), "an ASCII character");
    text[third] ^= 1;
}

fn drop_last_line(text: &mut Vec<u8>) {
    text.pop();
    while text.last().is_some_and(|b| *b != b'\n') {
        text.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{check_group, Group, Sut};
    use crate::tmp::TempDir;
    use fibref::expand_dump::RunnerKind;

    /// Five small programs: a macro-free one, one that defines a macro,
    /// one with an expansion error, one that does not read, a module
    /// program, in files of distinct names.
    fn files(dir: &Path) -> Vec<PathBuf> {
        let write = |name: &str, text: &str| {
            std::fs::write(dir.join(name), text).expect("writable");
            dir.join(name)
        };
        write("util.fib", "(ns util)\n(defun one () -> i64 1)\n");
        vec![
            write("a.fib", "(defun f (x: i64) -> i64 (when x 1 2))\n"),
            write(
                "b.fib",
                "(defmacro m (x) `(do ,x))\n(defun main () -> i64 (m 1))\n",
            ),
            write("c.fib", "(defun f () -> i64 1)\n(f)\n"),
            write("d.fib", "(a\n"),
            write(
                "e.fib",
                "(ns main (:use util))\n(defun main () -> i64 (one))\n",
            ),
        ]
    }

    fn group(files: Vec<PathBuf>, opts: Options) -> Group {
        Group::new("stand-in", files, opts)
    }

    fn sut(dir: &TempDir, files: &[PathBuf], opts: &Options, damage: Damage) -> Sut {
        Sut {
            tool: replay_tool(&dir.path().join("tool"), files, opts, damage),
            cwd: dir.path().to_path_buf(),
        }
    }

    fn options() -> Vec<Options> {
        vec![
            Options::default(),
            Options {
                context: true,
                runner: RunnerKind::None,
                ..Options::default()
            },
        ]
    }

    #[test]
    fn a_faithful_tool_passes_whatever_the_words_and_gets_them_and_the_directory() {
        let dir = TempDir::new("standin-faithful");
        let fs = files(dir.path());
        for opts in options() {
            let s = sut(&dir, &fs, &opts, Damage::Faithful);
            assert!(check_group(&s, &group(fs.clone(), opts)).is_empty());
        }
        let log = |name: &str| {
            std::fs::read_to_string(dir.path().join("tool").join(name)).expect("logged")
        };
        let words = log("words.log");
        let last = words.lines().last().expect("a run");
        assert!(last.starts_with("--context --no-runner "), "{last}");
        assert!(last.ends_with("e.fib"), "{last}");
        let cwd = log("cwd.log");
        let want = dir.path().canonicalize().expect("exists");
        assert!(
            cwd.lines().all(|l| std::path::Path::new(l) == want),
            "{cwd}"
        );
    }

    #[test]
    fn each_kind_of_damage_is_reported_and_localized_to_its_file() {
        let dir = TempDir::new("standin-damage");
        let fs = files(dir.path());
        let opts = Options::default();
        let damages = [
            (Damage::Character(0), "a.fib", "output line 3 differs"),
            (Damage::MissingLine(1), "b.fib", "output line"),
            (
                Damage::Status(2),
                "c.fib",
                "exit status: rust expander 1, fibber expander 2",
            ),
            (
                Damage::Crash(4),
                "e.fib",
                "exit status: rust expander 0, fibber expander 134",
            ),
        ];
        for (damage, file, what) in damages {
            let s = sut(&dir, &fs, &opts, damage);
            let failures = check_group(&s, &group(fs.clone(), opts.clone()));
            assert_eq!(
                failures.len(),
                1,
                "{damage:?}: {}",
                failures.iter().map(|f| f.to_string()).collect::<String>()
            );
            let text = failures[0].to_string();
            assert!(text.contains(file), "{damage:?}: {text}");
            assert!(text.contains(what), "{damage:?}: {text}");
        }
    }

    #[test]
    fn a_missing_final_newline_is_reported() {
        let dir = TempDir::new("standin-newline");
        let fs = files(dir.path());
        let opts = Options::default();
        let s = sut(&dir, &fs, &opts, Damage::TrailingNewline);
        let failures = check_group(&s, &group(fs.clone(), opts));
        assert!(!failures.is_empty());
        assert!(
            failures[0].to_string().contains("ends with a newline"),
            "{}",
            failures[0]
        );
    }

    #[test]
    fn a_batch_is_localized_to_the_one_file_that_differs() {
        let dir = TempDir::new("standin-batch");
        let many: Vec<PathBuf> = (0..250)
            .map(|i| {
                let path = dir.path().join(format!("f{i:03}.fib"));
                std::fs::write(&path, format!("(defun f{i} () -> i64 {i})\n")).expect("writable");
                path
            })
            .collect();
        let opts = Options::default();
        let s = sut(&dir, &many, &opts, Damage::Character(150));
        let failures = check_group(&s, &group(many.clone(), opts));
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0].subject.ends_with("f150.fib"),
            "{}",
            failures[0].subject
        );
        let runs = std::fs::read_to_string(dir.path().join("tool/words.log")).expect("logged");
        assert!(
            runs.lines().count() >= 3 + 100,
            "three batches, and the bad one again by file"
        );
    }

    #[test]
    fn a_tool_that_knows_none_of_the_files_prints_nothing_and_is_reported() {
        let dir = TempDir::new("standin-empty");
        let fs = files(dir.path());
        let other = vec![dir.path().join("zzz.fib")];
        std::fs::write(&other[0], "(f)\n").expect("writable");
        let opts = Options::default();
        let s = sut(&dir, &fs, &opts, Damage::Faithful);
        let failures = check_group(&s, &group(other, opts));
        assert_eq!(failures.len(), 1);
    }
}
