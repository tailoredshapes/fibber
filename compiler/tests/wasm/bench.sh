#!/bin/bash
# compiler/tests/wasm/bench.sh: the quick benchmarks (scripts/bench) native and as wasm32-wasi under node, median of RUNS, and whether the answers agree
# (docs/design/wasm.md 6). Not part of the gate: it measures. One row per program: name, native s, wasm s, wasm / native, same output (sha1 of stdout).
#   compiler/tests/wasm/bench.sh --fibc F [-n RUNS] [--runtime node|wasmtime] [NAME..]
# Needs WASI_SDK (or WASM_LD and WASI_SYSROOT) like `fibc build --target wasm32-wasi`. FIB_TARGET_FEATURES=-simd128 builds the wasm without simd128.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=; runs=3; runtime=node
while [ $# -gt 0 ]; do
  case $1 in
    --fibc) fibc=$2; shift ;;
    -n) runs=$2; shift ;;
    --runtime) runtime=$2; shift ;;
    *) break ;;
  esac
  shift
done
[ -x "$fibc" ] || { echo "usage: bench.sh --fibc F [-n RUNS] [NAME..]" >&2; exit 2; }
names=${*:-"num-f64 num-nbody vec-sort strings vec-index map-assoc-get vec-conj-pop set-conj lazy-fused"}
tmp=$(mktemp -d "$HOME/.cache/fibber-scratch/wasm-bench-XXXXXX")
trap 'rm -rf "$tmp"' EXIT
median() { sort -n | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}'; }
timeit() { # command...: seconds of one run, stdout to $tmp/out
  local s e
  s=$(date +%s.%N); "$@" > "$tmp/out" 2>/dev/null; e=$(date +%s.%N)
  echo "$e - $s" | bc -l
}
printf '%-16s %9s %9s %7s  %s\n' program native-s wasm-s ratio same-output
for n in $names; do
  src=$root/scripts/bench/$n.fib
  [ -f "$src" ] || { echo "$n: no such program" >&2; continue; }
  "$fibc" build "$src" -o "$tmp/$n.nat" -I "$root/lib" >/dev/null 2>"$tmp/err" || { echo "$n: native build failed: $(head -1 "$tmp/err")"; continue; }
  "$fibc" build "$src" -o "$tmp/$n.wasm" --target wasm32-wasi -I "$root/lib" >/dev/null 2>"$tmp/err" || { echo "$n: wasm build failed: $(head -1 "$tmp/err")"; continue; }
  "$tmp/$n.nat" > "$tmp/nat.out" 2>/dev/null
  if [ "$runtime" = wasmtime ]; then run=(wasmtime run -W tail-call=y,simd=y "$tmp/$n.wasm"); else run=(node --no-warnings --stack-size=50000 "$here/run.mjs" "$tmp/$n.wasm"); fi
  "${run[@]}" > "$tmp/wasm.out" 2>/dev/null
  same=no; cmp -s "$tmp/nat.out" "$tmp/wasm.out" && same=yes
  nt=$(for i in $(seq "$runs"); do timeit "$tmp/$n.nat"; done | median)
  wt=$(for i in $(seq "$runs"); do timeit "${run[@]}"; done | median)
  printf '%-16s %9.2f %9.2f %6.2fx  %s\n' "$n" "$nt" "$wt" "$(echo "$wt / $nt" | bc -l)" "$same"
done
