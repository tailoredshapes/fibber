#!/bin/bash
# compile-time.sh FIBC [N]: median seconds of `FIBC emit compiler/fibc.fib` (the compiler compiling itself to lIR) over N runs (default 3),
# and of `FIBC build scripts/bench/hello.fib`. Prints "emit-fibc SEC" and "build-hello SEC" lines for run.sh.
set -u
fibc=$1; n=${2:-3}
root=$(cd "$(dirname "$0")/../.." && pwd)
median() { sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
ulimit -v 16000000
t=()
for i in $(seq "$n"); do
  s=$(date +%s.%N)
  "$fibc" emit "$root/compiler/fibc.fib" -I "$root/compiler" -I "$root/lib" > "$tmp/out.ll" || exit 1
  e=$(date +%s.%N)
  t+=("$(echo "$e - $s" | bc -l)")
done
echo "emit-fibc $(printf '%s\n' "${t[@]}" | median)"
t=()
for i in $(seq "$n"); do
  s=$(date +%s.%N)
  "$fibc" build "$root/scripts/bench/hello.fib" -o "$tmp/hello" || exit 1
  e=$(date +%s.%N)
  t+=("$(echo "$e - $s" | bc -l)")
done
echo "build-hello $(printf '%s\n' "${t[@]}" | median)"
