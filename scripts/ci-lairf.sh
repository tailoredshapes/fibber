#!/bin/bash
# lairf (lair in fibber, compiler/lairf.fib) against the Rust lair, as CI runs it while the Rust lair exists (until stage 10): builds lairf, then the
# five compare scripts of compiler/tests/native/ over a small corpus: every file of cases/lir and 30 programs emitted from cases/ownership (every 5th that
# the emitter accepts). Exit 0 only when every unit of every script is SAME.
# usage: scripts/ci-lairf.sh OUT        (from the repository root or anywhere)
#   FIBC        the fibc that builds lairf and emits the programs (any seed or stage 2)
#   SEED_LIB    the directory with the liblair.so of a seed made before the flip (it loads it to compile); unset for a stage 2
#   LAIR        the Rust lair binary (the oracle); required
#   LLVM_LIBDIR LLVM 21's lib directory (default /usr/lib/llvm-21/lib): lairf links its libLLVM-21.so (shared) and finds it by rpath
#   LEVELS      the optimisation levels of compare-obj.sh (default: 0 2, to keep CI short; the script's own default is 0 1 2 3)
#   JOBS        units at a time (at most 4)
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
out=$(mkdir -p "${1:?usage: ci-lairf.sh OUT}" && cd "$1" && pwd)
: "${FIBC:?FIBC names the fibc that builds lairf}"
: "${LAIR:?LAIR names the Rust lair}"
libdir=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
export FIB_LIB=$root/lib TMPDIR=$out/tmp LEVELS=${LEVELS:-0 2} JOBS=${JOBS:-4}
mkdir -p "$TMPDIR"
fibc() { if [ -n "${SEED_LIB:-}" ]; then LD_LIBRARY_PATH=$SEED_LIB "$FIBC" "$@"; else "$FIBC" "$@"; fi; }

echo "== build lairf"
fibc build compiler/lairf.fib -I compiler -I lib -L "$libdir" -l LLVM-21 -o "$out/lairf" || { echo "ci-lairf: lairf did not build"; exit 1; }
if ldd "$out/lairf" | grep -q lair; then echo "ci-lairf: lairf links a liblair"; exit 1; fi
export LAIRF=$out/lairf

echo "== emit 30 programs from cases/ownership"
corpus=$out/corpus; rm -rf "$corpus"; mkdir -p "$corpus"
n=0; i=0
for f in $(find cases/ownership -maxdepth 1 -name '*.fib' | LC_ALL=C sort); do
  i=$((i + 1)); [ $((i % 5)) -eq 0 ] || continue
  b=$(basename "$f" .fib)
  if fibc emit -I compiler -I lib "$f" > "$corpus/$b.lir" 2> /dev/null && [ -s "$corpus/$b.lir" ]; then n=$((n + 1)); else rm -f "$corpus/$b.lir"; fi
  [ "$n" -lt 30 ] || break
done
echo "emitted $n programs"
[ "$n" -gt 0 ] || { echo "ci-lairf: no program was emitted"; exit 1; }

bad=0
t=compiler/tests/native
for s in ast check llvm obj; do
  echo "== compare-$s"
  "$t/compare-$s.sh" cases/lir "$corpus" | grep -v '^SAME'
  [ "${PIPESTATUS[0]}" -eq 0 ] || bad=1
done
echo "== compare-cases"
"$t/compare-cases.sh" | grep -v '^SAME'
[ "${PIPESTATUS[0]}" -eq 0 ] || bad=1
if [ "$bad" -eq 0 ]; then echo "ci-lairf: ok"; else echo "ci-lairf: FAILED"; fi
exit "$bad"
