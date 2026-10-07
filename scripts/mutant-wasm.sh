#!/bin/bash
# Mutation check of the wasm32-wasi build (docs/design/wasm.md 7): each mutant plants one fault in the compiler, builds a stage 2 with it, runs compiler/tests/wasm/wasm.sh
# (the checks of the 32-bit target, node, wasm-ld, wasi-libc), and passes only if that run FAILS. A mutant that survives means a check is missing. The sources are restored
# after every mutant (and on exit), so the tree is clean afterwards; run it on a clean tree.
#   scripts/mutant-wasm.sh BUILDER [OUTDIR]       BUILDER: a fibc that builds compiler/fibc.fib (a seed or a stage 2). ONLY=name runs one mutant.
# Needs what wasm.sh needs (node, WASI_SDK or WASM_LD and WASI_SYSROOT); without them it says so and exits 77 (skipped).
# Exit: 0 every mutant was killed, 1 one survived, 2 the plant did not apply or the build failed, 77 skipped.
#   memory-grow       the linker's memory limit is 64 MiB, not 4 GiB less a page: `grow` allocates 768 MiB
#   fd-write          a `size_t` of more than 64 KiB is cut to the largest `i32` (the length of a `write`): `write` prints 200000 bytes
#   tail-call         the row's features lose `+tail-call`: `tail` runs ten million tail calls in node's own stack
#   simd-lane-order   the 16-lane pieces of a wide mask are put together in the reverse order: `movemask`
#   pointer-width     a pointer is 8 bytes in the layouts of a 4-byte target: `vec-of-objects` (cases 8251 and 8252)
#   no-saturation     a `size_t` that does not fit an `i32` becomes its low 32 bits: `oversize` asks for 2^33 + 4096 bytes
set -u
builder=${1:?usage: mutant-wasm.sh BUILDER [OUTDIR]}
root=$(cd "$(dirname "$0")/.." && pwd)
out=${2:-$HOME/.cache/fibber-scratch/mutant-wasm}
mkdir -p "$out/tmp"
export FIB_LIB=$root/lib TMPDIR=$out/tmp
ulimit -v 16000000
command -v node >/dev/null || { echo "skip: node is not installed"; exit 77; }
if [ -z "${WASI_SDK:-}" ]; then for d in "$HOME"/.cache/fibber-scratch/tools/wasm/wasi-sdk-*; do [ -d "$d" ] && WASI_SDK=$d; done; export WASI_SDK; fi
if [ -z "${WASI_SDK:-}" ] && ! command -v wasm-ld >/dev/null && [ -z "${WASM_LD:-}" ]; then echo "skip: no wasm-ld (set WASI_SDK)"; exit 77; fi
files="compiler/native/linkwasm.fib compiler/native/lower/calls.fib compiler/types/targets.fib compiler/emit/lower/simdfn.fib compiler/emit/layout.fib"
for f in $files; do mkdir -p "$out/orig/$(dirname "$f")"; cp "$root/$f" "$out/orig/$f"; done
restore() { for f in $files; do cp "$out/orig/$f" "$root/$f"; done; }
trap restore EXIT
survived=0
plant() { # FILE OLD NEW: the text OLD (once at least) becomes NEW; the file must change
  python3 - "$root/$1" "$2" "$3" <<'EOF'
import sys
path, old, new = sys.argv[1:4]
s = open(path).read()
if old not in s:
    sys.exit(3)
open(path, "w").write(s.replace(old, new, 1))
EOF
}
mutant() { # NAME FILE OLD NEW
  local name=$1; shift; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  plant "$@" || { echo "PLANT-FAILED $name: the text to change is not in $2"; exit 2; }
  (cd "$root" && "$builder" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$out/Fm") > "$out/$name.build" 2>&1 \
    || { echo "BUILD-FAILED $name: $(tail -2 "$out/$name.build" | tr '\n' ' ')"; exit 2; }
  local res; res=$(bash "$root/compiler/tests/wasm/wasm.sh" "$out/Fm" 2>&1)
  if echo "$res" | tail -1 | grep -q " 0 failed"; then echo "SURVIVED $name: $(echo "$res" | tail -1)"; survived=1
  else echo "killed   $name: $(echo "$res" | grep '^FAIL' | head -2 | tr '\n' ';')"; fi
}
mutant memory-grow compiler/native/linkwasm.fib '4294901760' '67108864'
mutant fd-write compiler/native/lower/calls.fib '(core/const-int i64t 4294967295 false)))' '(core/const-int i64t 65535 false)))'
mutant tail-call compiler/types/targets.fib '"+simd128,+tail-call" false 1 0 1 1 "none" "wasm" "wasm-ld" "WASI_SYSROOT"' '"+simd128" false 1 0 1 1 "none" "wasm" "wasm-ld" "WASI_SYSROOT"'
mutant simd-lane-order compiler/emit/lower/simdfn.fib '(range (* 16 k) (* 16 (+ k 1)))' '(range (* 16 (- (quot n 16) (+ k 1))) (* 16 (- (quot n 16) k)))'
mutant pointer-width compiler/emit/layout.fib '((LirPtr) (Pair pb pb)) ((LirRaw) (Pair pb pb))' '((LirPtr) (Pair 8 8)) ((LirRaw) (Pair 8 8))'
mutant no-saturation compiler/native/lower/calls.fib '(b-select (. fx b) big (core/const-int i32t 4294967295 false) low)' '(b-select (. fx b) big low low)'
[ $survived -eq 0 ] && echo "mutant-wasm: every mutant was killed"
exit $survived
