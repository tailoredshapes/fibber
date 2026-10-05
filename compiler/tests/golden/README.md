# compiler/tests/golden

Golden outputs of the passes of the compiler in fibber, for the days after the Rust went (RR1, ROADMAP stage 10).

Until then each pass was judged by a compare script that ran the Rust oracle (`fibref read|expand|types|own|explain`,
`fibc emit-dump`, `lair dump-ast|check|emit-llvm`) and the stage 2 tool on the same inputs and required the same bytes. The
Rust is in git history (tag `seed-1`; `docs/rust-legacy.md`). What the oracles said on 2026-10-05 is recorded here.

| File | What |
|------|------|
| `suites.sh` | the suites: stage 2 tool, options, input globs (and which Rust command recorded each) |
| `golden.sh` | the check; `--update` regenerates from stage 2 alone (`--rescan` also takes new inputs); `--show SUITE FILE` prints one output |
| `NAME.files`, `NAME.golden` | the inputs of suite NAME and the recorded output and exit status of each (a SHA-256 for `reader-dump` and `expand-porter`, whose text would be 100 MB) |
| `RECORDED.txt` | per suite: inputs, recorded, left out; and each input left out |
| `record-from-rust.sh` | historical: how they were recorded; needs the Rust binaries, which are not in the tree |
| `audit/` | reference outputs of the Rust memory audit, for the port of `fibref` (see `audit/README.md`); not a check |

An input is in a suite only if the Rust and stage 2 printed the same bytes and exit status on the day of recording. Where they did
not (a prelude that moved, a language the seed does not know, a pass that is ahead of the Rust: `compiler/mirror-pending/`) there is
no Rust-recorded golden, and the input is listed in `RECORDED.txt`. After an intended change of a pass: `golden.sh --update`, read
the diff in git, commit it with the change. Run `compiler/tests/golden/golden.sh --fibc F` (F: a stage 2 fibc; it builds the tools
into `$GOLDEN_OUT/tools`) or `--tools DIR` for tools you have built; exit 0 only if every suite is `ok`.

Other goldens of the same origin: `compiler/tests/emit/resume.golden` (`emit/resume.sh`), `compiler/tests/own/golden/*.txt`
(`own/golden.sh`; the ten of them that no longer agreed with stage 2 on that day, because the prelude and the ownership pass had moved,
were re-recorded from stage 2 and carry no Rust confirmation), `compiler/tests/types/*/NAME.out`.
