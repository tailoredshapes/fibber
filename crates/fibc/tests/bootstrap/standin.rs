//! A stand-in for the reader tool, to show the harness can fail.
//!
//! The stand-in is a shell script that replays, for each file named on
//! its command line, a section and a status it was given beforehand: the
//! oracle's own, or the oracle's with one kind of damage, in the dump mode
//! or in the print mode (`--print` first). Run through the same pipeline
//! as the real tool (batches, the process wrapper, the byte comparison,
//! the localization of a failure to a file, the mode flag) it must pass
//! when faithful and fail, in the right way and naming the mode, when
//! damaged.

use std::path::{Path, PathBuf};

use super::check::{check_files, Failure};
use super::compare::{Difference, Outcome};
use super::tool::{oracle, run_tool_within, Mode, Tool, BATCH, TIMED_OUT};

/// What the stand-in gets wrong; the number is the index of the input
/// whose replay is damaged. A damage is made in one mode only (the one
/// [`replay_tool`] is given); the other mode is replayed faithfully.
#[derive(Clone, Copy, Debug)]
pub enum Damage {
    /// Nothing: a tool that agrees with the oracle everywhere.
    Faithful,
    /// One character of the first line below the header is changed: a
    /// digit of the first node's column in the dump, the first character
    /// of the first form in the printed text.
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

/// Replays `sh script [--print] FILE..`: the mode is `print` when the
/// first argument is `--print` (which is not a file) and `dump` otherwise;
/// for each file, its section in that mode (the `== FILE` header and the
/// dump or the printed forms), then the largest status. It logs how many
/// files each run was given, in a log for each mode.
const SCRIPT: &str = "\
here=$(dirname \"$0\")
mode=dump
if [ \"$1\" = \"--print\" ]; then mode=print; shift; fi
echo \"$#\" >> \"$here/calls-$mode.log\"
status=0
for f in \"$@\"; do
  b=$(basename \"$f\")
  if [ -e \"$here/$b.$mode.crash\" ]; then kill -ABRT $$; fi
  cat \"$here/$b.$mode.section\"
  s=$(cat \"$here/$b.$mode.status\")
  if [ \"$s\" -gt \"$status\" ]; then status=$s; fi
done
exit $status
";

/// A script that drops the `--print` flag and replays the dump: a tool
/// that does not know the print mode, next to the replay it wraps.
const IGNORES_THE_FLAG: &str = "\
here=$(dirname \"$0\")
if [ \"$1\" = \"--print\" ]; then shift; fi
exec sh \"$here/replay.sh\" \"$@\"
";

/// Writes a stand-in for `files` into `dir`, with `damage` in `damaged`
/// mode only, and returns it.
pub fn replay_tool(dir: &Path, files: &[PathBuf], damage: Damage, damaged: Mode) -> Tool {
    std::fs::create_dir_all(dir).expect("writable");
    let script = dir.join("replay.sh");
    std::fs::write(&script, SCRIPT).expect("writable");
    for (i, file) in files.iter().enumerate() {
        let name = file.file_name().expect("a file name").to_string_lossy();
        for mode in Mode::ALL {
            let mut replay = oracle(std::slice::from_ref(file), mode);
            if mode == damaged {
                damage_replay(dir, &name, mode, damage, (i, files.len()), &mut replay);
            }
            std::fs::write(dir.join(format!("{name}.{mode}.section")), &replay.stdout)
                .expect("writable");
            std::fs::write(
                dir.join(format!("{name}.{mode}.status")),
                replay.status.to_string(),
            )
            .expect("writable");
        }
    }
    Tool::script(&script)
}

/// Applies `damage` to the replay of input `at.0` of `at.1`, if it is
/// the input the damage names.
fn damage_replay(
    dir: &Path,
    name: &str,
    mode: Mode,
    damage: Damage,
    at: (usize, usize),
    replay: &mut Outcome,
) {
    let (i, total) = at;
    match damage {
        Damage::Character(n) if n == i => match mode {
            Mode::Dump => bump_first_column(&mut replay.stdout),
            Mode::Print => change_first_form(&mut replay.stdout),
        },
        Damage::MissingLine(n) if n == i => drop_last_line(&mut replay.stdout),
        Damage::Status(n) if n == i => replay.status = (replay.status + 1) % 3,
        Damage::TrailingNewline if i + 1 == total => {
            assert_eq!(replay.stdout.pop(), Some(b'\n'));
        }
        Damage::Crash(n) if n == i => {
            std::fs::write(dir.join(format!("{name}.{mode}.crash")), "").expect("writable");
        }
        _ => {}
    }
}

/// Changes the first character of the first line below the header (an
/// ASCII character in the test's inputs) to a different one.
fn change_first_form(text: &mut [u8]) {
    let header_end = text
        .iter()
        .position(|b| *b == b'\n')
        .expect("a header line");
    let first = &mut text[header_end + 1];
    assert!(first.is_ascii(), "an ASCII character");
    *first ^= 1;
}

/// Changes the first digit of the column of the first node (the text
/// after the first `:` below the header).
fn bump_first_column(text: &mut [u8]) {
    let header_end = text
        .iter()
        .position(|b| *b == b'\n')
        .expect("a header line");
    let colon = header_end
        + text[header_end..]
            .iter()
            .position(|b| *b == b':')
            .expect("a position");
    let digit = &mut text[colon + 1];
    assert!(digit.is_ascii_digit(), "a digit follows the colon");
    *digit = b'0' + (*digit - b'0' + 1) % 10;
}

fn drop_last_line(text: &mut Vec<u8>) {
    assert_eq!(text.pop(), Some(b'\n'));
    let start = text.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    text.truncate(start);
}

/// How many files each run of the stand-in in `tool_dir` was given in `mode`.
fn calls(tool_dir: &Path, mode: Mode) -> Vec<usize> {
    let log =
        std::fs::read_to_string(tool_dir.join(format!("calls-{mode}.log"))).unwrap_or_default();
    log.lines().filter_map(|l| l.parse().ok()).collect()
}

/// Inputs for the plumbing test: valid, erroneous and unreadable files.
fn write_inputs(dir: &Path, count: usize) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).expect("writable");
    (0..count)
        .map(|i| {
            let path = dir.join(format!("in-{i:03}.fib"));
            let text = match i % 4 {
                0 => format!("(f {i} \"é\")\n"),
                1 => format!("(f {i}\n"),
                2 => format!("[{i}] )"),
                _ => format!("{{:k {i}}} 'x"),
            };
            std::fs::write(&path, text).expect("writable");
            path
        })
        .collect()
}

/// A tool that runs `real` and changes one thing in the second line of
/// what it prints: a digit of the first position in the dump, the first
/// character of the first form in print mode. It is the real reader (or
/// printer) with one fault, to show that the pipeline can fail on the
/// real thing in either mode.
fn corrupted(dir: &Path, real: &Tool) -> Tool {
    std::fs::create_dir_all(dir).expect("writable");
    let quoted = |p: &Path| {
        let text = p.to_str().expect("test paths are UTF-8").to_string();
        assert!(!text.contains('\''), "{text}");
        format!("'{text}'")
    };
    let command: Vec<String> = std::iter::once(&real.program)
        .chain(&real.leading)
        .map(|p| quoted(p))
        .collect();
    let script = dir.join("corrupt.sh");
    let text = format!(
        "here=$(dirname \"$0\")\n{} \"$@\" > \"$here/out.txt\"\nstatus=$?\n\
         if [ \"$1\" = --print ]; then sed '2s/^./Z/' \"$here/out.txt\"\n\
         else sed '2s/ \\([0-9]*\\):/ 9\\1:/' \"$here/out.txt\"; fi\nexit $status\n",
        command.join(" ")
    );
    std::fs::write(&script, text).expect("writable");
    Tool::script(&script)
}

/// Runs the pipeline, in each mode, on three small inputs through `real`
/// with one fault and says whether every one is reported, at the line
/// that was changed, in the mode that has the fault.
pub fn fault_in_the_real_tool_is_noticed(dir: &Path, real: &Tool) -> Result<(), String> {
    let inputs = dir.join("canary-inputs");
    std::fs::create_dir_all(&inputs).expect("writable");
    let files: Vec<PathBuf> = ["(a 1)\n", "x", "(b [c])"]
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let path = inputs.join(format!("canary-{i}.fib"));
            std::fs::write(&path, text).expect("writable");
            path
        })
        .collect();
    let tool = corrupted(&dir.join("canary-tool"), real);
    for mode in Mode::ALL {
        let failures = check_files(&tool, mode, &files);
        let at_line_2 = |f: &Failure| {
            f.mode == mode && matches!(f.differences[..], [Difference::Line { number: 2, .. }])
        };
        if failures.len() != files.len() || !failures.iter().all(at_line_2) {
            let shown: String = failures.iter().map(|f| f.to_string()).collect();
            return Err(format!(
                "a reader with one fault was reported in {mode} mode on {} of {} inputs, \
                 not all at line 2:\n{shown}",
                failures.len(),
                files.len()
            ));
        }
    }
    Ok(())
}

fn only(failures: &[Failure]) -> &Failure {
    assert_eq!(failures.len(), 1, "{}", failures.len());
    &failures[0]
}

/// The failures of a stand-in damaged in `mode`, run in `mode`.
fn damaged(root: &Path, files: &[PathBuf], damage: Damage, mode: Mode) -> Vec<Failure> {
    let tool = replay_tool(&root.join(format!("damaged-{mode}")), files, damage, mode);
    check_files(&tool, mode, files)
}

/// The faithful tool passes in both modes, over two batches and no more
/// than [`BATCH`] files per process; a missing readable file is replayed
/// as the oracle prints it too.
fn faithful_passes(root: &Path, files: &[PathBuf]) {
    let mut with_unreadable = files.to_vec();
    with_unreadable.push(root.join("no-such-file.fib"));
    let dir = root.join("faithful");
    let tool = replay_tool(&dir, &with_unreadable, Damage::Faithful, Mode::Dump);
    for mode in Mode::ALL {
        let failures = check_files(&tool, mode, &with_unreadable);
        assert!(failures.is_empty(), "{}", failures[0]);
        assert_eq!(
            calls(&dir, mode),
            vec![BATCH, with_unreadable.len() - BATCH],
            "{mode}"
        );
    }
}

fn damage_is_found_and_localized(root: &Path, files: &[PathBuf]) {
    let late = BATCH + 20;
    // The status of a run is the largest of its files', so a wrong one
    // shows where it raises that: among files that all read.
    let readable: Vec<PathBuf> = files.iter().step_by(4).cloned().collect();
    for mode in Mode::ALL {
        let f = damaged(root, files, Damage::Character(late), mode);
        let found = only(&f);
        assert_eq!(found.mode, mode);
        assert!(found.subject.ends_with("in-120.fib"), "{}", found.subject);
        assert!(matches!(
            found.differences[..],
            [Difference::Line { number: 2, .. }]
        ));

        let f = damaged(root, files, Damage::MissingLine(3), mode);
        assert!(only(&f).subject.ends_with("in-003.fib"));
        assert!(matches!(
            only(&f).differences[..],
            [Difference::Line { actual: None, .. }]
        ));

        let f = damaged(root, &readable, Damage::Status(5), mode);
        assert!(only(&f).subject.ends_with("in-020.fib"));
        assert!(matches!(
            only(&f).differences[..],
            [Difference::Status { .. }]
        ));

        let f = damaged(root, files, Damage::TrailingNewline, mode);
        assert!(only(&f).subject.ends_with("in-139.fib"));
        assert!(matches!(
            only(&f).differences[..],
            [Difference::FinalNewline { .. }]
        ));

        let f = damaged(root, files, Damage::Crash(7), mode);
        assert!(only(&f).subject.ends_with("in-007.fib"));
        assert!(matches!(
            only(&f).differences[..],
            [
                Difference::Line { .. },
                Difference::Status { actual: 134, .. }
            ]
        ));
    }
}

/// A damage made in one mode is reported in that mode and not in the
/// other, and a tool that ignores `--print` fails in print mode alone.
fn modes_are_judged_apart(root: &Path, files: &[PathBuf]) {
    for (bad, good) in [(Mode::Dump, Mode::Print), (Mode::Print, Mode::Dump)] {
        let dir = root.join(format!("apart-{bad}"));
        let tool = replay_tool(&dir, files, Damage::Character(2), bad);
        let found = check_files(&tool, bad, files);
        assert_eq!(only(&found).mode, bad);
        assert!(check_files(&tool, good, files).is_empty(), "{good} mode");
    }
    let dir = root.join("ignores-the-flag");
    replay_tool(&dir, files, Damage::Faithful, Mode::Dump);
    std::fs::write(dir.join("ignore.sh"), IGNORES_THE_FLAG).expect("writable");
    let blind = Tool::script(&dir.join("ignore.sh"));
    assert!(check_files(&blind, Mode::Dump, files).is_empty());
    let found = check_files(&blind, Mode::Print, files);
    assert!(!found.is_empty());
    assert!(found.iter().all(|f| f.mode == Mode::Print), "{}", found[0]);
    assert!(found[0].to_string().starts_with("DIFFERENT in print mode:"));
}

fn broken_tools_fail(root: &Path, files: &[PathBuf]) {
    for mode in Mode::ALL {
        let none = check_files(&Tool::program(Path::new("true")), mode, &files[..5]);
        assert!(
            !none.is_empty(),
            "a tool that prints nothing must fail: {mode}"
        );
        let absent = Tool::program(&root.join("no-such-tool"));
        let missing = check_files(&absent, mode, &files[..5]);
        assert!(
            !missing.is_empty(),
            "a tool that cannot start must fail: {mode}"
        );
    }
    let slow = Tool {
        program: PathBuf::from("sleep"),
        leading: vec![PathBuf::from("30")],
    };
    let limit = std::time::Duration::from_millis(300);
    let run = run_tool_within(&slow, Mode::Dump, &[], limit);
    assert_eq!(run.outcome.status, TIMED_OUT);
    assert!(run.stderr.contains("timed out"), "{}", run.stderr);
}

/// A tool runs with no core dump and at most 4 GiB of address space.
fn limits_apply(files: &[PathBuf]) {
    let shell = Tool {
        program: PathBuf::from("sh"),
        leading: vec![PathBuf::from("-c"), PathBuf::from("ulimit -v; ulimit -c")],
    };
    let limit = std::time::Duration::from_secs(20);
    let run = run_tool_within(&shell, Mode::Dump, &files[..0], limit);
    let text = String::from_utf8_lossy(&run.outcome.stdout).into_owned();
    let mut lines = text.lines();
    let memory: u64 = lines.next().unwrap_or("").parse().expect(&text);
    assert!(memory <= 4_194_304, "{text}");
    assert_eq!(lines.next(), Some("0"), "{text}");
}

/// One test, not several: it starts processes, and the test binary
/// starts at most a few at a time.
#[test]
fn the_pipeline_passes_a_faithful_tool_and_fails_a_damaged_one() {
    let dir = crate::tmp::TempDir::new("standin");
    let files = write_inputs(&dir.path().join("inputs"), 140);
    faithful_passes(dir.path(), &files);
    damage_is_found_and_localized(dir.path(), &files);
    modes_are_judged_apart(dir.path(), &files);
    broken_tools_fail(dir.path(), &files);
    limits_apply(&files);
}
