#!/bin/bash
# scripts/mutant-obs.sh: planted faults for fib.log sinks (L2), fib.otel tracing (O1) and metrics (O2) (docs/design/observability-and-databases.md).
# Library only: copies lib/ and specs/ to a scratch directory, breaks ONE rule in the copy (a text replacement in one library file), and runs
# the spec that pins the rule with `F test`. The spec must FAIL under the mutant (a failed expectation, a trap or a timeout all count); a spec
# that still passes means the fault survived and the spec is a bad one. The unmutated copy is run first and must pass (else the script says so).
# usage: scripts/mutant-obs.sh MODE|all [REGEX]|list     (all runs every mode, or those whose name matches REGEX)
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
tp-v00-accepts-extra|otel-propagation-spec|otel/propagation.fib|(= version 0) (= n 55)|(= version 0) true
tp-version-ff-ok|otel-propagation-spec|otel/propagation.fib|(= version 255)|false
tp-zero-span-id-ok|otel-propagation-spec|otel/propagation.fib|(= sid 0))|false)
ts-duplicate-keys-ok|otel-propagation-spec|otel/propagation.fib|(= (count (set keys)) (count pairs))|true
ts-no-limit|otel-propagation-spec|otel/propagation.fib|(if (> (count parts) 32)|(if (> (count parts) 3200)
ids-zero-span-id|otel-propagation-spec|otel/ids.fib|(let [v (next-u64 g)] (if (= v 0) (next-u64 g) v))|(let [v (next-u64 g)] v)
span-parent-id-lost|otel-trace-spec|otel/tracer.fib|(match parent ((some pc) (. pc span-id)) (nil 0))|0
span-end-twice-exports-twice|otel-trace-spec|otel/tracer.fib|(when (compare-and-set! (. st ended) false true)|(when true
span-not-ended-on-trap|otel-trace-spec|otel/tracer.fib|(catch e (do (record-exception! sp e) (end-span! sp) (ex/throw e)))|(catch e (ex/throw e))
span-exception-not-recorded|otel-trace-spec|otel/tracer.fib|(catch e (do (record-exception! sp e) (end-span! sp) (ex/throw e)))|(catch e (do (end-span! sp) (ex/throw e)))
span-ok-not-final|otel-trace-spec|otel/tracer.fib|((StatusOk) d)|((StatusOk) (with d (status status)))
span-context-not-passed|otel-trace-spec|otel/tracer.fib|(defun span-in-context (c: Context s: Span) -> Context (context-with-span-context c (. s context)))|(defun span-in-context (c: Context s: Span) -> Context c)
span-flag-always-sampled|otel-trace-spec|otel/tracer.fib|flags (match decision ((SampleRecordAndSample) 1) (_ 0))|flags 1
sampler-parent-ignored|otel-trace-spec|otel/sampler.fib|(if (sampled-flag? p) SampleRecordAndSample SampleDrop)|SampleRecordAndSample
sampler-ratio-inverted|otel-trace-spec|otel/sampler.fib|(< (bit-and (. trace-id lo) 9223372036854775807) (. self threshold))|(>= (bit-and (. trace-id lo) 9223372036854775807) (. self threshold))
sampler-dropped-still-recorded|otel-trace-spec|otel/tracer.fib|((SampleDrop) (Span ctx nil tracer))|((SampleDrop) (Span ctx (some (begin-state p tracer name kind tid sid pc flags ts attrs links)) tracer))
simple-no-catch|otel-export-spec|otel/processor.fib|(ex/try (export-spans exporter spans) (catch e (Err (ex/ex-message e))))|(export-spans exporter spans)
batch-no-size-trigger|otel-export-spec|otel/processor.fib|due (or (>= n max) @wanted|due (or false @wanted
batch-no-time-trigger|otel-export-spec|otel/processor.fib|(>= (- now last) interval-ns)|false
batch-flush-no-wait|otel-export-spec|otel/processor.fib|(processor-flush (self) (do (wait-exported self)|(processor-flush (self) (do ()
batch-unbounded|otel-export-spec|otel/queue.fib|full (>= n (. q capacity))|full false
batch-drops-uncounted|otel-export-spec|otel/processor.fib|((QueueDropped) (do (bump! (. p dropped) 1) (when (= (. p overflow) DropOldest) ())))|((QueueDropped) ())
batch-order-reversed|otel-export-spec|otel/queue.fib|(vec (reverse newest-first))|newest-first
EOF
)
if [ "$MODE" = list ]; then echo "$TABLE" | cut -d'|' -f1; exit 0; fi
if [ "$MODE" = all ]; then
  rc=0
  for m in $(echo "$TABLE" | cut -d"|" -f1 | grep -E "${2:-.}"); do "$0" "$m" || rc=1; done
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
base=$(run); if ! echo "$base" | grep -q ", 0 fail, 0 trap, 0 timeout" || echo "$base" | grep -q "total: 0 scenarios"; then echo "mutant-obs $MODE: the unmutated spec does not pass" >&2; echo "$base" | tail -5 >&2; exit 2; fi
python3 - "$OUT/lib/fib/$FILE" "$OLD" "$NEW" <<'PY' || { echo "mutant-obs $MODE: the text to replace is not there exactly once" >&2; exit 2; }
import sys
p, old, new = sys.argv[1], sys.argv[2].replace('\\n', '\n'), sys.argv[3].replace('\\n', '\n')
s = open(p).read()
if s.count(old) != 1: sys.exit(1)
open(p, 'w').write(s.replace(old, new))
PY
out=$(run)
if echo "$out" | grep -q ", 0 fail, 0 trap, 0 timeout" && ! echo "$out" | grep -q "total: 0 scenarios"; then echo "mutant-obs $MODE: SURVIVED (the spec still passes)"; rm -rf "$OUT"; exit 1; fi
echo "mutant-obs $MODE: killed ($(echo "$out" | grep '^total' | sed 's/ in 1 spec.*//'))"
rm -rf "$OUT"
