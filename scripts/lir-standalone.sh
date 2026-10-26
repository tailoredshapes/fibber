#!/bin/bash
# scripts/lir-standalone.sh: the paths bound for the lIR repository (scripts/lir-paths.txt) need nothing of fibber's compiler. It copies the
# tracked files of those paths alone into a scratch tree (as they are in the working tree, or `git archive REV` of them with --rev), and there:
#   1. the plant: a copy of compiler/native/lower.fib that also uses `syntax.form` (fibber's reader, not in the path set) must FAIL to build,
#      naming the module, so the check can fail;
#   2. compiler/lairf.fib, lir2c.fib and lir2js.fib build with the seed compiler and the library the seed ships (FIB_LIB unset, `-I compiler`
#      of the scratch tree only, FIB_LIRO_CACHE=off so no object built earlier stands in): a `use` or `require` of any fibber module outside the path set is an unresolved module, a failed build;
#   3. the built lairf passes lair's own cases: `lairf --old-spelling cases cases/lir -j N`, every case.
# The split plan's step 5 (owner decision 2026-10-26: lIR and lair get their own repository); build/tools/sh-lir-standalone.ok runs it.
# usage: scripts/lir-standalone.sh SEED_FIBC [--rev REV] [--jobs N]   (OUT the scratch, default $TMPDIR/lir-standalone; LLVM_LIBDIR)
# exit 0 when all three hold, 1 when one does not, 2 on a usage or setup error.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/.." && pwd)
seed=${1:-}; shift || true
rev=""; jobs=4
while [ $# -gt 0 ]; do
  case "$1" in
    --rev) rev=$2; shift 2 ;;
    --jobs) jobs=$2; shift 2 ;;
    *) echo "lir-standalone: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -x "$seed" ] || { echo "lir-standalone: give the seed fibc (make print-seed)" >&2; exit 2; }
seed=$(cd "$(dirname "$seed")" && pwd)/$(basename "$seed")
lib=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
out=${OUT:-${TMPDIR:-/tmp}/lir-standalone}; rm -rf "$out"; mkdir -p "$out/tree" "$out/plant"
paths=$(grep -v '^[[:space:]]*#' "$root/scripts/lir-paths.txt" | grep -v '^[[:space:]]*$')
# the tree: only the listed paths (a pathspec that names nothing is an error: the list must stay true)
for p in $paths; do
  [ -n "$(git -C "$root" ls-files -- "$p")" ] || { echo "lir-standalone: scripts/lir-paths.txt names $p, which holds no tracked file" >&2; exit 2; }
done
if [ -n "$rev" ]; then
  # shellcheck disable=SC2086
  git -C "$root" archive "$rev" -- $paths | tar -x -C "$out/tree" || { echo "lir-standalone: git archive $rev failed" >&2; exit 2; }
else
  # shellcheck disable=SC2086
  git -C "$root" ls-files -z -- $paths | (cd "$root" && xargs -0 tar -cf -) | tar -x -C "$out/tree" || { echo "lir-standalone: copy failed" >&2; exit 2; }
fi
echo "tree: $(find "$out/tree" -type f | wc -l | tr -d ' ') files from $(echo "$paths" | wc -l | tr -d ' ') paths${rev:+ at $rev}"
# build NAME DIR: the seed builds compiler/NAME.fib of DIR with nothing but DIR/compiler and the seed's own library
build() {
  (cd "$2" && ulimit -v 16000000 && env -u FIB_LIB -u FIB_PATH FIB_LIRO_CACHE=off timeout 900 "$seed" build "compiler/$1.fib" -I compiler -L "$lib" -l LLVM-21 -o "$out/$1") > "$out/$1.log" 2>&1
}
fail=0
# 1. the plant
cp -R "$out/tree/." "$out/plant/"
awk '!done && /\(:use / { sub(/\(:use /, "(:use syntax.form "); done = 1 } { print }' \
  "$out/tree/compiler/native/lower.fib" > "$out/plant/compiler/native/lower.fib"
cmp -s "$out/tree/compiler/native/lower.fib" "$out/plant/compiler/native/lower.fib" && { echo "lir-standalone: the plant did not change native/lower.fib" >&2; exit 2; }
if (cd "$out/plant" && ulimit -v 16000000 && env -u FIB_LIB -u FIB_PATH FIB_LIRO_CACHE=off timeout 900 "$seed" build compiler/lairf.fib -I compiler -L "$lib" -l LLVM-21 -o "$out/lairf-planted") > "$out/plant.log" 2>&1; then
  echo "FAIL plant: lairf built although native/lower.fib requires syntax.form"; fail=1
elif grep -q 'syntax.form' "$out/plant.log"; then
  echo "plant: refused as it must be: $(grep -m1 'syntax.form' "$out/plant.log" | cut -c1-160)"
else
  echo "FAIL plant: the build failed without naming syntax.form:"; tail -n 5 "$out/plant.log"; fail=1
fi
# 2. the three tools from the tree
for t in lairf lir2c lir2js; do
  t0=$(date +%s)
  if build "$t" "$out/tree"; then echo "build $t: exit 0, $(( $(date +%s) - t0 )) s"; else echo "FAIL build $t:"; tail -n 8 "$out/$t.log"; fail=1; fi
done
# 3. lair's cases with the lairf the seed built
if [ -x "$out/lairf" ]; then
  res=$(cd "$out/tree" && ulimit -v 16000000 && LD_LIBRARY_PATH=$lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH} timeout 900 "$out/lairf" --old-spelling cases cases/lir -j "$jobs" 2>&1); st=$?
  printf '%s\n' "$res" > "$out/cases.log"
  total=$(printf '%s\n' "$res" | grep '^total:' || true)
  case "$st:$total" in
    0:*", 0 fail"*) echo "cases: $total" ;;
    *) echo "FAIL cases (status $st): ${total:-no total line}"; printf '%s\n' "$res" | grep -Ev '^(PASS|ok)' | tail -n 10; fail=1 ;;
  esac
fi
[ "$fail" = 0 ] && echo "PASS lir-standalone" || echo "FAIL lir-standalone (logs: $out)"
exit "$fail"
