#!/bin/bash
# Planted faults in the server's own source: each mutant is a copy of compiler/ with one wrong line, built and replayed (server.sh); the replay must FAIL.
# A replay that passes a mutant cannot see that fault. The comparator's own faults (a range, a dropped diagnostic, a code, the framing) are
# `server.sh OUT planted`; these are faults in the code the tests are about.
# usage: mutants.sh [NAME..]    FIBC names the fibc; SCRATCH the directory the copies go in (default $TMPDIR or /tmp)
# mutants: code (-32601 becomes -32600), range (a diagnostic's start is one byte late), definition (a definition is found one byte late),
#          filter (completion keeps only the library's names: the buffer's own items are dropped), initialize (the hover capability is lost)
# Exit: 0 every mutant was caught; 1 one was not, or did not build; 2 usage.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
scratch=${SCRATCH:-${TMPDIR:-/tmp}}/lsp-mutants
names=("$@")
[ ${#names[@]} -gt 0 ] || names=(code range definition filter initialize)
mutate() { # mutate NAME FILE FROM TO  (the copy's FILE must contain FROM exactly once)
  local f=$scratch/$1/compiler/$2
  [ "$(grep -cF -- "$3" "$f")" = 1 ] || { echo "mutants: $1: '$3' is not once in $2" >&2; return 1; }
  python3 - "$f" "$3" "$4" <<'EOF'
import sys
p, a, b = sys.argv[1:4]
s = open(p).read()
open(p, 'w').write(s.replace(a, b, 1))
EOF
}
bad=0
for n in "${names[@]}"; do
  rm -rf "${scratch:?}/$n"; mkdir -p "$scratch/$n"
  cp -r "$root/compiler" "$scratch/$n/compiler"
  ln -s "$root/lib" "$scratch/$n/lib"
  case $n in
    code) mutate $n lsp/server.fib '(def err-method-not-found: i64 -32601)' '(def err-method-not-found: i64 -32600)' ;;
    range) mutate $n lsp/server.fib '(range-json (tx/diag-range src d))' '(range-json (tx/diag-range src (LsDiag (+ (. d start) 1) (. d end) (. d message))))' ;;
    definition) mutate $n lsp/definition.fib '(Pair i (+ i len))' '(Pair (+ i 1) (+ i len))' ;;
    filter) mutate $n lsp/features.fib '(into (into (into (into locals own) globals) core) macros)' '(into (into (into globals core) macros) [])' ;;
    initialize) mutate $n lsp/server.fib '(Pair "hoverProvider" (JBool true))' '(Pair "hoverProvider" (JBool false))' ;;
    *) echo "mutants: no mutant $n" >&2; exit 2 ;;
  esac || { bad=1; continue; }
  if ! FIB_LIB=$root/lib "$scratch/$n/compiler/tests/lsp/build.sh" "$scratch/$n/serve" > "$scratch/$n/build.log" 2>&1; then
    echo "mutants: $n did not build:"; head -5 "$scratch/$n/build.log"; bad=1; continue
  fi
  if SERVER="$scratch/$n/serve" FIB_LIB=$root/lib "$here/server.sh" > "$scratch/$n/run.log" 2>&1; then
    echo "mutants: $n was NOT caught"; bad=1
  else
    echo "mutant $n caught: $(grep -m1 -A1 '^FAIL' "$scratch/$n/run.log" | tr '\n' ' ' | cut -c1-220)"
  fi
done
exit $bad
