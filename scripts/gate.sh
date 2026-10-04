#!/bin/bash
# The gate: one command that says whether a tree is good. Builds stage 2, (full) checks the fixed point, runs the three case directories
# with `cases -j N`, compares the cases that do not pass with scripts/ci-stage2.expected (scripts/ci-stage2.sh does that part, as CI runs
# it), prints a timing line for each stage and ends with PASS or FAIL (exit 0 or 1; 2 for a usage or setup error).
#   scripts/gate.sh [--quick|--full] [-j N]
#   --quick (default)  stage 2, ownership, modules and a deterministic sample of about 100 stdlib cases (every expected failure among them)
#   --full             stage 2 built by the seed, the fixed point (F builds F3, both emit the same lIR), and all three directories in full
#   -j N               cases run at a time (default 12; the machine has 28 cores and 61 GB, and no more than about 12 heavy jobs at once)
# Environment (scripts/lib/stage2.sh has the whole list): FIBC or SEED = the fibc that builds stage 2 (CI gives the seed); without
# them a previous F of the same prelude builds it (the line `build:` says which); GATE_OUT = scratch (F and case output are kept there,
# and a build is skipped when compiler/ and lib/ are what the cached F was built from); LLVM_LINK = shared (default) or static: how F links
# LLVM; LAIR_DIR = where the liblair.so of a pre-flip builder is (a builder that is a stage 2 needs none).
# Takes /tmp/fibsuite.lock for its whole run, so a benchmark (scripts/bench/quick.sh) never overlaps it.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${GATE_ROOT:-$(cd "$here/.." && pwd)}   # GATE_ROOT: judge another checkout of the repository (scripts/batch.sh)
mode=quick; jobs=${GATE_JOBS:-12}
while [ $# -gt 0 ]; do
  case $1 in
    --quick) mode=quick ;; --full) mode=full ;;
    -j) jobs=${2:?-j needs a number}; shift ;;
    -j*) jobs=${1#-j} ;;
    *) echo "usage: scripts/gate.sh [--quick|--full] [-j N]" >&2; exit 2 ;;
  esac
  shift
done
case $jobs in ''|*[!0-9]*|0) echo "gate: -j wants a whole number of at least 1" >&2; exit 2 ;; esac
# shellcheck source=lib/stage2.sh
. "$here/lib/stage2.sh"
take_suite_lock
cd "$root" || exit 2
bad=0; t_all=$(now); timings=()
note() { echo "[$(elapsed "$t_all" "$(now)") s] $*"; }
fail() { bad=1; note "$*"; }

[ "$mode" = full ] && [ -z "${FIBC:-}${SEED:-}" ] && GATE_FRESH=1   # CI's check is the seed's: the previous F is for quick runs
echo "gate: $mode, -j $jobs, tree $(tree_stamp), scratch $GATE_OUT"

t0=$(now)
if stage2_ensure; then F=$GATE_OUT/F; timings+=("build $(elapsed "$t0" "$(now)") s"); else
  echo "GATE FAIL: stage 2 did not build"; exit 1
fi
note "stage build done"

if [ "$mode" = full ]; then
  t0=$(now)
  if stage3_check; then timings+=("fixed point $(elapsed "$t0" "$(now)") s"); else fail "fixed point FAILED"; timings+=("fixed point FAILED $(elapsed "$t0" "$(now)") s"); fi
fi

# The sample of the quick gate: every 10th stdlib case by name (the order of `ls`, bytes), and the expected failures among them, so a
# sample still shows the harness reporting one. A case is a *.fib or a directory with a main.fib; its name is what `--only` matches.
sample=
if [ "$mode" = quick ]; then
  sample=$GATE_OUT/sample.txt
  names=$(cd cases/stdlib && for e in $(LC_ALL=C ls); do
            if [ -f "$e" ]; then case $e in *.fib) echo "$e" ;; esac; elif [ -f "$e/main.fib" ]; then echo "$e"; fi; done)
  step=$(( $(echo "$names" | wc -l) / 100 )); [ "$step" -lt 1 ] && step=1
  { echo "$names" | awk -v k="$step" 'NR % k == 0'
    sed -n 's|^cases/stdlib/\([^ ]*\) .*|\1|p' scripts/ci-stage2.expected | sed 's|/main.fib$||'; } | LC_ALL=C sort -u > "$sample"
  echo "gate: stdlib sample of $(wc -l < "$sample") of $(echo "$names" | wc -l) cases (every ${step}th, plus the expected failures)"
fi

t0=$(now)
out=$GATE_OUT/cases; rm -rf "$out"; mkdir -p "$out"
export CI_STAGE2_OUT=$out CI_STAGE2_JOBS=$jobs
if [ -n "$sample" ]; then export CI_STAGE2_ONLY=$sample; else unset CI_STAGE2_ONLY; fi
"$here/ci-stage2.sh" "$F" > "$out/ci-stage2.log" 2>&1
code=$?
cat "$out/ci-stage2.log"
dirs=$(awk '/^== / { d=$4; sub("cases/", "", d) } /^exit [0-9]+ after/ { printf "%s%s %s (exit %s)", sep, d, $4, $2; sep=", " }' "$out/ci-stage2.log")
timings+=("cases: $dirs")
if [ "$code" -ne 0 ]; then fail "cases: ci-stage2.sh failed (the non-passing set differs from scripts/ci-stage2.expected, or a directory timed out)"; fi
timings+=("cases total $(elapsed "$t0" "$(now)") s")

echo
echo "== timing"
for l in "${timings[@]}"; do echo "  $l"; done
echo "  total $(elapsed "$t_all" "$(now)") s"
if [ "$bad" -eq 0 ]; then echo "GATE PASS ($mode)"; exit 0; fi
echo "GATE FAIL ($mode)"; exit 1
