#!/bin/bash
# The judge of compiler/emit/resume.fib: the hand-built builders of unit-resume.fib through emit.resume, byte for byte against
# compiler/tests/emit/resume.golden, then the unit checks (the whole-module checker accepts each result).
# resume.golden was recorded from the Rust resume.rs of tag seed-1 on 2026-10-05 (compiler/tests/emit/resume-oracle.rs, in git history,
# printed the same builders: 8 cases, 7196 bytes, equal to stage 2's), and is stage 2's own since: after an intended change of
# emit.resume run `resume.sh FIBC --update` and read the diff before committing. See docs/rust-legacy.md.
# usage: resume.sh FIBC [--update]    (scratch: RESUME_OUT; FIBC: a stage 2 fibc binary; run anywhere, it changes into the repository root)
set -euo pipefail
FIBC=${1:?fibc}; upd=0; [ "${2:-}" = "--update" ] && upd=1
S=${RESUME_OUT:-$HOME/.cache/fibber-scratch/E9r/oracle}
R=$(cd "$(dirname "$0")/../../.." && pwd)
mkdir -p "$S"
(cd "$R" && "$FIBC" build compiler/tests/emit/unit-resume.fib -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$S/resume-test")
"$S/resume-test" cases > "$S/fib.out"
if [ "$upd" -eq 1 ]; then cp "$S/fib.out" "$R/compiler/tests/emit/resume.golden"; echo "resume: golden updated"; fi
"$S/resume-test" > "$S/checks.out" || { tail -5 "$S/checks.out"; exit 1; }
cmp "$R/compiler/tests/emit/resume.golden" "$S/fib.out" && echo "resume: $(grep -c '^==' "$S/fib.out") cases byte for byte equal to resume.golden; $(grep -c '^ok' "$S/checks.out") checks ok"
