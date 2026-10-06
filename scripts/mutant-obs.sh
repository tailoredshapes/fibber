#!/bin/bash
# scripts/mutant-obs.sh: planted faults for fib.log sinks (L2), fib.otel tracing (O1) and metrics (O2) (docs/design/observability-and-databases.md).
# Library only: copies lib/ and specs/ to a scratch directory, breaks ONE rule in the copy (a text replacement in one library file), and runs
# the spec that pins the rule with `F test`. The spec must FAIL under the mutant (a failed expectation, a trap or a timeout all count); a spec
# that still passes means the fault survived and the spec is a bad one. The unmutated copy is run first and must pass (else the script says so).
# usage: scripts/mutant-obs.sh MODE|all|list
# environment: FIBC (a stage 2 with the `test` command, e.g. the F of scripts/gate.sh; required), MUT_OUT (scratch, default
#   ~/.cache/fibber-scratch/mutant-obs-MODE). Runs under `ulimit -v 16000000` and MALLOC_ARENA_MAX=2.
# exit: 0 when the spec failed under the mutant, 1 when it survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-obs.sh MODE|all|list}
R=$(cd "$(dirname "$0")/.." && pwd)
# MODE | spec | file under lib/fib | text to find | text to put in its place     (one per line; the find text must occur exactly once)
TABLE=$(cat <<'EOF'
file-no-rotate|log-sinks-spec|log/file.fib|due (and (> (. s max-bytes) 0)|due (and false
file-no-shift|log-sinks-spec|log/file.fib|(do (loop [i (- keep 1)]|(do (loop [i 0]
file-no-lock|log-sinks-spec|log/file.fib|(do (lock! (. s busy))\n      (let [r (ex/try|(do (let [r (ex/try
file-lock-leaks|log-sinks-spec|log/file.fib|(do (reset! (. s busy) false) r))))|(do r))))
async-unbounded|log-sinks-spec|log/async.fib|full (>= n (. s capacity))|full false
async-close-no-drain|log-sinks-spec|log/async.fib|(do (join (. self worker)) (sink-close (. self inner)))|(sink-close (. self inner))
async-flush-noop|log-sinks-spec|log/async.fib|(sink-flush (self) (do (drain-wait self) (sink-flush (. self inner))))|(sink-flush (self) (sink-flush (. self inner)))
async-drops-uncounted|log-sinks-spec|log/async.fib|(do (bump! (. s dropped) 1) (Ok ()))|(Ok ())
async-worker-uncaught|log-sinks-spec|log/async.fib|(match (ex/try (sink-write inner r) (catch e (Err (ex/ex-message e))))|(match (sink-write inner r)
async-oldest-policy-keeps-oldest|log-sinks-spec|log/async.fib|(QLink r (without-oldest @(. s queue)))|@(. s queue)
fanout-no-catch|log-sinks-spec|log/route.fib|(match (ex/try (f k) (catch e (Err (ex/ex-message e))))|(match (f k)
fanout-stops-at-first|log-sinks-spec|log/route.fib|((Err m) (Pair (if (some? (. acc fst)) (. acc fst) (some m)) (+ (. acc snd) 1)))))|((Err m) (trap m))))
filter-off-by-one|log-sinks-spec|log/route.fib|(if (>= (. r level) (. self min))|(if (> (. r level) (. self min))
env-spec-ignores-root|log-sinks-spec|log/core.fib|(if (some? (str-find entry "=" 0)) root (unwrap-or (parse-level entry) root))|root
EOF
)
if [ "$MODE" = list ]; then echo "$TABLE" | cut -d'|' -f1; exit 0; fi
if [ "$MODE" = all ]; then
  rc=0
  for m in $(echo "$TABLE" | cut -d'|' -f1); do "$0" "$m" || rc=1; done
  exit $rc
fi
ROW=$(echo "$TABLE" | grep "^$MODE|") || { echo "mutant-obs: unknown mode $MODE" >&2; exit 2; }
IFS='|' read -r _ SPEC FILE OLD NEW <<< "$ROW"
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-obs-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-obs: set FIBC to an executable stage 2 (it needs the test command)" >&2; exit 2; }
unset LD_LIBRARY_PATH
rm -rf "$OUT"; mkdir -p "$OUT"
cp -r "$R/lib" "$R/specs" "$OUT/"
ulimit -v 16000000; export MALLOC_ARENA_MAX=2
run() { (cd "$OUT" && timeout 300 "$FIBC" test "specs/$SPEC.fib" -I lib -I specs --seed 1 2>&1); }
base=$(run); if ! echo "$base" | grep -q ", 0 fail, 0 trap, 0 timeout"; then echo "mutant-obs $MODE: the unmutated spec does not pass" >&2; echo "$base" | tail -5 >&2; exit 2; fi
python3 - "$OUT/lib/fib/$FILE" "$OLD" "$NEW" <<'PY' || { echo "mutant-obs $MODE: the text to replace is not there exactly once" >&2; exit 2; }
import sys
p, old, new = sys.argv[1], sys.argv[2].replace('\\n', '\n'), sys.argv[3].replace('\\n', '\n')
s = open(p).read()
if s.count(old) != 1: sys.exit(1)
open(p, 'w').write(s.replace(old, new))
PY
out=$(run)
if echo "$out" | grep -q ", 0 fail, 0 trap, 0 timeout"; then echo "mutant-obs $MODE: SURVIVED (the spec still passes)"; rm -rf "$OUT"; exit 1; fi
echo "mutant-obs $MODE: killed ($(echo "$out" | grep '^total' | sed 's/ in 1 spec.*//'))"
rm -rf "$OUT"
