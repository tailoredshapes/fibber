#!/bin/bash
# The drift test of compiler/emit/runtime.fib (plan G4): regenerate it from rt/*.lir with gen-runtime.fib and compare
# the result with the committed file byte for byte, then run unit-runtime.fib (each def against its .lir file, the declarations).
# usage: runtime.sh FIBC [SCRATCH]      (FIBC: a fibc binary; run anywhere, it changes into the repository root)
set -euo pipefail
FIBC=${1:?fibc}; S=${2:-$HOME/.cache/fibber-scratch/E3/runtime}
R=$(cd "$(dirname "$0")/../../.." && pwd); cd "$R"; mkdir -p "$S"
"$FIBC" build compiler/tests/emit/gen-runtime.fib -I compiler -I lib -o "$S/gen-runtime"
"$S/gen-runtime" rt > "$S/runtime.fib"
cmp "$S/runtime.fib" compiler/emit/runtime.fib || { echo "runtime.fib has drifted from rt: regenerate it"; exit 1; }
"$FIBC" build compiler/tests/emit/unit-runtime.fib -I compiler -I lib -o "$S/unit-runtime"
"$S/unit-runtime" > "$S/unit.out" || { grep -v '^ok' "$S/unit.out" | head; exit 1; }
echo "runtime: regenerated runtime.fib equals the committed file; $(grep -c '^ok' "$S/unit.out") checks ok"
