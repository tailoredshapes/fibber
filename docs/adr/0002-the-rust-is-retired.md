# 0002. The Rust is retired

Status: accepted
Date: 2026-10-05
Source: ROADMAP.md stage 10; commit `847f785` (delete crates/, Cargo files, the Rust CI jobs); docs/rust-legacy.md; tag `seed-1`.

## Context

Until 2026-10-05 the project had two implementations of the language: the Rust tools (`fibref`, `fibc`, `lair`, `lir`, `fibgen`) and
the compiler in fibber (stage 2). The Rust ones were the oracles: a pass of stage 2 was correct when it printed what the Rust printed. They
also kept every build, CI job and script tied to `cargo` and an LLVM binding crate.

## Decision

The Rust is retired. The tree has no `crates/`, no Cargo files and no Rust toolchain in the build, the gate, the release scripts or CI.
Stage 2 is built by a released seed (`SEED`) and is the only compiler. What the Rust oracles pinned is pinned by recorded outputs: the
golden suites of `compiler/tests/golden/`, one per pass tool, and by the specs and cases. The Rust lives in git (tag `seed-1`).

## Consequences

- A build needs a seed `fibc` and LLVM 21; nothing else. A new platform needs a release of the seed for it.
- A regression the Rust would have caught by disagreeing is caught only if a golden, a case or a spec says so.
- Goldens are recorded, not independent: re-recording one accepts the change. The Rust's comparison is gone.
- The Rust ports still owed (`fibref`'s interpreter, `fibgen`, `fibc lsp`) are backlog, not dependencies.

## Governance

```fibber fitness
(rule "there is no crates directory and no Cargo or Rust-toolchain file"
  (present repo ["crates" "crates/**" "Cargo.toml" "Cargo.lock" "rust-toolchain" "rust-toolchain.toml" "clippy.toml" "rustfmt.toml"])
  (plant-file "crates/fibc/src/main.rs" "fn main() {}\n"))

(rule "the gate does not call cargo or name crates/"
  (into (grep-live repo ["scripts/gate.sh" "scripts/batch.sh" "scripts/ci-stage2.sh" "scripts/lib/*.sh"] "cargo")
        (grep-live repo ["scripts/gate.sh" "scripts/batch.sh" "scripts/ci-stage2.sh" "scripts/lib/*.sh"] "crates/"))
  (plant "scripts/gate.sh" "\ncargo build --release\n"))

(rule "the release scripts do not call cargo or name crates/"
  (into (grep-live repo ["scripts/package.sh" "scripts/fetch-seed.sh" "scripts/check-version.sh"] "cargo")
        (grep-live repo ["scripts/package.sh" "scripts/fetch-seed.sh" "scripts/check-version.sh"] "crates/"))
  (plant "scripts/package.sh" "\ncrates/lair/build.sh\n"))

(rule "CI (GitLab and GitHub) does not call cargo or name crates/"
  (into (grep-live repo [".gitlab-ci.yml" ".github/workflows/*.yml"] "cargo")
        (grep-live repo [".gitlab-ci.yml" ".github/workflows/*.yml"] "crates/"))
  (plant ".gitlab-ci.yml" "\n  script: cargo test\n"))

(rule "the pass tools keep their golden suites (one .golden per suite, at least sixteen)"
  (at-least repo ["compiler/tests/golden/*.golden"] 16)
  (plant-remove "compiler/tests/golden/reader-dump.golden"))

(rule "every golden suite has the list of its inputs"
  (mapv (fn (g: str) (Finding g 0 "has no .files list of inputs next to it"))
        (filterv (fn (g: str) (not (member? (names-matching repo ["compiler/tests/golden/*.files"]) (str (str-slice g 0 (- (str-len g) 7)) ".files"))))
                 (names-matching repo ["compiler/tests/golden/*.golden"])))
  (plant-remove "compiler/tests/golden/own-taken.files"))
```

### What this does not check

That the goldens are right (they are recordings: a re-record passes this); that the seed in `SEED` exists or builds stage 2 (the
release workflow and the gate do); that the history can still bring the Rust back (`docs/rust-legacy.md`); prose that mentions
`cargo` (README, ROADMAP, docs record history and are not scanned). Scripts under `scripts/` other than the ones named are not scanned:
a mutant script may still name something Rust-shaped, and that is not a build dependency.
