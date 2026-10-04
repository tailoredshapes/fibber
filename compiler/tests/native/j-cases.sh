#!/bin/bash
# Package J's gate 1: the JIT path of `lair cases` over the five directories of cases/lir, through native.jit (drivers/j-run-cases.fib), against the Rust
# `lair cases` on the same directories, line for line (rows, per-directory counts, total) and in exit status.
# usage: j-cases.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; LAIR the Rust `lair`, default target/debug/lair of the main checkout; FIB_LIB the
#                                    library; LLVM_LIBDIR the directory of libLLVM-21.so; CASES the directories, default the five of cases/lir)
# Exit: 0 identical; 1 different or a step failed; 2 a tool is missing.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "j-cases: no fibc: set FIBC" >&2; exit 2; }
lair=${LAIR:-/tank/repos/tailoredshapes/fibber/target/debug/lair}
[ -x "$lair" ] || lair=$(command -v lair) || { echo "j-cases: no lair: set LAIR" >&2; exit 2; }
libdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
work=${1:-${TMPDIR:-/tmp}/j-cases-$$}
mkdir -p "$work"
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
dirs=${CASES:-"cases/lir/adversarial cases/lir/audit cases/lir/instr cases/lir/mapping cases/lir/verify"}
"$fibc" build compiler/tests/native/drivers/j-run-cases.fib -I compiler -I lib -L "$libdir" -l LLVM-21 -o "$work/j-run-cases" || { echo "j-cases: FAILED to build" >&2; exit 1; }
"$work/j-run-cases" cases $dirs > "$work/fibber.txt"; rf=$?
"$lair" cases $dirs > "$work/rust.txt"; rr=$?
if cmp -s "$work/fibber.txt" "$work/rust.txt" && [ "$rf" = "$rr" ]; then
  echo "same: $(tail -1 "$work/fibber.txt"); $(wc -l < "$work/fibber.txt") lines, exit $rf in both"; exit 0
fi
echo "DIFFERENT (exit fibber $rf, rust $rr):"; diff "$work/rust.txt" "$work/fibber.txt" | head -20; exit 1
