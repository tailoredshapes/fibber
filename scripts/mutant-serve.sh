#!/bin/bash
# scripts/mutant-serve.sh: planted faults for the development loop (docs/design/dev-loop.md sections 8 and 9: `fibc serve`, `fibc repl`). Copies compiler/ and lib/ to
# a scratch directory, breaks ONE rule in the copy, builds a fibc from the copy and runs the test that pins the rule against it: the test must FAIL. A test that still
# passes under its mutant is a bad test.
# usage: scripts/mutant-serve.sh MODE
#   MODE                 what is broken                                                              test that must fail
#   stale-cache          the key of a module ignores its text (an edited file is not seen)           serve
#   no-dependents        the key of a module ignores the modules before it (a dependent is not       serve
#                        expanded or checked again when what it uses changes)
#   no-supervisor        the worker is not run under the supervisor (a trap ends the server)         serve
#   repl-trap-kills      the REPL runs an expression in its own process, not in a child              repl
#   redefine-keeps-old   a redefinition does not replace the earlier definition                      repl
#   redefine-unchecked   a definition is accepted without checking the program of them all           repl
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-serve-MODE).
# exit: 0 when the test failed under the mutant, 1 when it passed (the mutant survived), 2 for a setup error. That the tests pass UNmutated is shown by running them.
set -uo pipefail
MODE=${1:?usage: mutant-serve.sh stale-cache|no-dependents|no-supervisor|repl-trap-kills|redefine-keeps-old|redefine-unchecked}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-serve-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-serve: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
rm -rf "$OUT"; mkdir -p "$OUT"
cp -r "$R/compiler" "$R/lib" "$OUT/"
patch() { # patch FILE OLD NEW: the text must be there exactly once
  python3 - "$OUT/$1" "$2" "$3" <<'EOF' || { echo "mutant-serve: the text to change is not in $1 (the source moved: update the mutant)" >&2; exit 2; }
import sys
p, old, new = sys.argv[1:4]
s = open(p).read()
if s.count(old) != 1: sys.exit(1)
open(p, 'w').write(s.replace(old, new))
EOF
}
case $MODE in
  stale-cache)        patch compiler/serve/incr.fib '(hash-combine (hash-combine (hash-combine prev (hash text)) (str-len text)) (hash ns))' '(hash-combine (hash-combine prev 0) (hash ns))'; TEST=serve ;;
  no-dependents)      patch compiler/serve/incr.fib '(hash-combine (hash-combine (hash-combine prev (hash text)) (str-len text)) (hash ns))' '(hash-combine (hash-combine (hash-combine 0 (hash text)) (str-len text)) (hash ns))'; TEST=serve ;;
  no-supervisor)      patch compiler/serve/server.fib '(loop ((n 0))
    (let ((pid (sk/fork-process)))' '(loop ((n 0))
    (let ((pid 0))'; TEST=serve ;;
  repl-trap-kills)    patch compiler/serve/repl.fib '(let ((pid (sk/fork-process)))
    (cond (< pid 0) -1' '(let ((pid 0))
    (cond (< pid 0) -1'; TEST=repl ;;
  redefine-keeps-old) patch compiler/serve/replcore.fib '(into (filterv (fn (d: Def) (not (some? (first (filterv (fn (n: Def) (= (. n key) (. d key))) news))))) olds) news))' '(into olds news))'; TEST=repl ;;
  redefine-unchecked) patch compiler/serve/replcore.fib '((Err ds) (if (empty? ds)
                      (accept s clauses defs news (. f stats))' '((Err ds) (if true
                      (accept s clauses defs news (. f stats))'; TEST=repl ;;
  *) echo "mutant-serve: unknown mode $MODE" >&2; exit 2 ;;
esac
cd "$OUT" || exit 2
echo "mutant-serve: building the $MODE mutant ..."
( ulimit -v 16000000; "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F" ) > "$OUT/build.log" 2>&1 \
  || { echo "mutant-serve: the mutant does not build (see $OUT/build.log)"; tail -5 "$OUT/build.log"; exit 2; }
cd "$R" || exit 2
if [ "$TEST" = serve ]; then FIB_LIB="$R/lib" "$R/compiler/tests/serve/run.sh" "$OUT/F" > "$OUT/test.log" 2>&1; else "$R/compiler/tests/repl/run.sh" "$OUT/F" > "$OUT/test.log" 2>&1; fi
rc=$?
grep -E '^FAIL' "$OUT/test.log" | head -5 || true  # pipe-ok: a short failure listing
if [ $rc -ne 0 ]; then echo "mutant-serve: $MODE KILLED by the $TEST test (exit $rc)"; exit 0; fi
echo "mutant-serve: $MODE SURVIVED: the $TEST test passed under the mutant"; exit 1
