#!/bin/bash
# The gate: one command that says whether a tree is good. Builds stage 2, (full) checks the fixed point, runs the three case directories
# with `cases -j N`, compares the cases that do not pass with scripts/ci-stage2.expected (scripts/ci-stage2.sh does that part, as CI runs
# it), prints a timing line for each stage and ends with PASS or FAIL (exit 0 or 1; 2 for a usage or setup error).
#   scripts/gate.sh [--quick|--full] [--incremental] [-j N]
#   --quick (default)  stage 2, the tool skeletons (scripts/tools.sh), ownership, modules and a deterministic sample of about 100 stdlib cases (every expected failure among them)
#   GATE_SPECS=1       quick gate: also run `fibc test specs` (the full gate always does)
#   --full             stage 2 built by the seed, the fixed point (F builds F3, both emit the same lIR), the golden checks of the passes
#                      (compiler/tests/golden/golden.sh), the tool tests (scripts/tools.sh: language server, fibref and fibgen ports), and all three directories in full
#   --incremental      the inner loop's gate: a stage that passed before, on inputs that have not changed since (a stamp of the files it depends on,
#                      under $GATE_OUT/stamps, written when a stage passes), is not run again and says "skipped (unchanged since PASS at STAMP)". Not
#                      for CI or a release: without the flag every stage runs, whatever passed before.
#   -j N               cases run at a time inside each shard (default 4: a shard compiles one case at a time, the runs overlap)
# The stages that need only F run side by side, one process group each, sharing a budget of GATE_BUDGET slots (default the number of cores, at most 16; the machine has
# 28 cores and 61 GB, and others use it): every heavy process (a compile, a shard of cases, a unit program) holds one slot while it runs
# (scripts/lib/slots.sh). GATE_SHARDS (default GATE_BUDGET) is how many processes the stdlib cases are dealt out to. Each stage's log is
# kept under $GATE_OUT and its output printed in a fixed order at the end, whichever finished first; the lines `[T s] stage X done` that
# appear while it runs are progress.
# Environment (scripts/lib/stage2.sh has the whole list): FIBC or SEED = the fibc that builds stage 2 (CI gives the seed); without
# them a previous F of the same prelude builds it (the line `build:` says which); GATE_OUT = scratch (F and case output are kept there,
# and a build is skipped when compiler/ and lib/ are what the cached F was built from); LLVM_LINK = shared (default) or static: how F links
# LLVM.
# Takes /tmp/fibsuite.lock for its whole run, so a benchmark (scripts/bench/quick.sh) never overlaps it.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${GATE_ROOT:-$(cd "$here/.." && pwd)}   # GATE_ROOT: judge another checkout of the repository (scripts/batch.sh)
mode=quick; jobs=${GATE_JOBS:-4}; incremental=0
while [ $# -gt 0 ]; do
  case $1 in
    --quick) mode=quick ;; --full) mode=full ;; --incremental) incremental=1 ;;
    -j) jobs=${2:?-j needs a number}; shift ;;
    -j*) jobs=${1#-j} ;;
    *) echo "usage: scripts/gate.sh [--quick|--full] [--incremental] [-j N]" >&2; exit 2 ;;
  esac
  shift
done
case $jobs in ''|*[!0-9]*|0) echo "gate: -j wants a whole number of at least 1" >&2; exit 2 ;; esac
cores=$(nproc 2> /dev/null || echo 16); [ "$cores" -gt 16 ] && cores=16
budget=${GATE_BUDGET:-$cores}; shards=${GATE_SHARDS:-$budget}
case $budget$shards in *[!0-9]*|'') echo "gate: GATE_BUDGET and GATE_SHARDS want whole numbers" >&2; exit 2 ;; esac
[ "$budget" -ge 1 ] && [ "$shards" -ge 1 ] || { echo "gate: GATE_BUDGET and GATE_SHARDS want at least 1" >&2; exit 2; }
# shellcheck source=lib/stage2.sh
. "$here/lib/stage2.sh"
# shellcheck source=lib/slots.sh
. "$here/lib/slots.sh"
take_suite_lock
cd "$root" || exit 2
bad=0; t_all=$(now); timings=()
note() { echo "[$(elapsed "$t_all" "$(now)") s] $*"; }
fail() { bad=1; note "$*"; }

[ "$mode" = full ] && [ -z "${FIBC:-}${SEED:-}" ] && GATE_FRESH=1   # CI's check is the seed's: the previous F is for quick runs
echo "gate: $mode, -j $jobs, budget $budget, shards $shards, tree $(tree_stamp), scratch $GATE_OUT"

t0=$(now)
if stage2_ensure; then F=$GATE_OUT/F; timings+=("build $(elapsed "$t0" "$(now)") s"); else
  echo "GATE FAIL: stage 2 did not build"; exit 1
fi
note "stage build done"

# ---- the stages. Each is a function run in its own background process: it writes, under $sd, NAME.t (timing lines), NAME.out (what to show:
# the tails of failing logs, the case tables), NAME.fail (failure notes) and NAME.ok (when it passed; its stamp is then kept).
sd=$GATE_OUT/stage; rm -rf "$sd"; mkdir -p "$sd" "$GATE_OUT/stamps"
slots_init "$GATE_OUT/slots" "$budget"

# dep_stamp PATH..: one hash of the files under the paths (what an incremental run compares)
dep_stamp() { (cd "$root" && find "$@" -type f 2> /dev/null | LC_ALL=C sort | xargs sha1sum 2> /dev/null | sha1sum | cut -c1-16); }
# skipped NAME PATH..: true (and a timing line) when --incremental and the stage passed on these very inputs
skipped() {
  local name=$1; shift
  stamp=$(dep_stamp "$@")-$mode; [ "$incremental" -eq 1 ] || return 1
  [ "$(cat "$GATE_OUT/stamps/$name.$mode" 2> /dev/null)" = "$stamp" ] || return 1
  echo "$name skipped (unchanged since PASS at $stamp)" > "$sd/$name.t"; touch "$sd/$name.ok"; return 0
}
# stage_done NAME: a stage that has passed (its .ok exists) keeps its stamp
stage_done() { [ -f "$sd/$1.ok" ] && echo "$stamp" > "$GATE_OUT/stamps/$1.$mode"; echo "[$(elapsed "$t_all" "$(now)") s] stage $1 done ($([ -f "$sd/$1.ok" ] && echo ok || echo FAILED))"; }
sfail() { echo "[$(elapsed "$t_all" "$(now)") s] $2" >> "$sd/$1.fail"; }

stage_fixed() {
  skipped fixed compiler lib && return
  local t0; t0=$(now)
  if stage3_check > "$sd/fixed.out" 2>&1; then echo "fixed point $(elapsed "$t0" "$(now)") s" > "$sd/fixed.t"; touch "$sd/fixed.ok"
  else sfail fixed "fixed point FAILED"; echo "fixed point FAILED $(elapsed "$t0" "$(now)") s" > "$sd/fixed.t"; fi
}

# The golden checks of the passes (compiler/tests/golden: reader, expander, type checker, ownership checker, emitter, lair), full gate only.
stage_golden() {
  skipped golden compiler lib && return
  local t0; t0=$(now)
  if GOLDEN_JOBS=$budget GOLDEN_OUT=$GATE_OUT/golden "$root/compiler/tests/golden/golden.sh" --fibc "$F" > "$GATE_OUT/golden.log" 2>&1; then
    echo "golden $(grep -c '^ok' "$GATE_OUT/golden.log") suites ok $(elapsed "$t0" "$(now)") s" > "$sd/golden.t"; touch "$sd/golden.ok"
  else tail -n 20 "$GATE_OUT/golden.log" > "$sd/golden.out"; sfail golden "golden checks FAILED (log: $GATE_OUT/golden.log)"
    echo "golden FAILED $(elapsed "$t0" "$(now)") s" > "$sd/golden.t"; fi
}

# The tool tests (scripts/tools.sh): the skeletons of the fibref and fibgen ports in the quick gate; in the full gate also the language server's unit
# programs, replay and hardening cases, the fibref heap programs, and the fibgen port's rng check, pipelines comparison and planted faults. One
# timing line for each script; a failing one fails the gate.
stage_tools() {
  skipped tools compiler lib scripts docs spec specs cases && return
  local t0 l; t0=$(now)
  if "$here/tools.sh" "--$mode" "$F" "$GATE_OUT/tools" > "$GATE_OUT/tools.log" 2>&1; then touch "$sd/tools.ok"; else tail -n 40 "$GATE_OUT/tools.log" > "$sd/tools.out"; fi
  while IFS= read -r l; do case $l in ok\ *|FAIL\ *) echo "tools $l" >> "$sd/tools.t" ;; esac; done < "$GATE_OUT/tools.log"
  [ -f "$sd/tools.ok" ] || sfail tools "tools FAILED (log: $GATE_OUT/tools.log)"
  echo "tools total $(elapsed "$t0" "$(now)") s" >> "$sd/tools.t"
}

# The executable ADRs (docs/adr, compiler/adr.fib, docs/design/executable-adrs.md), full gate only: about fifteen seconds with the 20 ADRs of today (build of the tool, one
# program for every block; the checks of 0014 and 0016 read the 2,200 case files). --strict: an accepted ADR with no check that runs is a failure too. Failures are not compared with
# scripts/ci-stage2.expected: a violated decision is a regression.
stage_adr() {
  skipped adr compiler lib docs spec scripts cases specs rt .github editors SEED VERSION && return
  local t0; t0=$(now)
  if "$SLOTS_SH" "$F" build compiler/adr.fib -I compiler -I lib -o "$GATE_OUT/adr" > "$GATE_OUT/adr.log" 2>&1 \
     && FIBC="$F" FIB_LIB="$root/lib" "$SLOTS_SH" "$GATE_OUT/adr" --strict >> "$GATE_OUT/adr.log" 2>&1; then
    echo "adr $(grep -E '^[0-9]+ ADRs:' "$GATE_OUT/adr.log" | head -1) $(elapsed "$t0" "$(now)") s" > "$sd/adr.t"; touch "$sd/adr.ok"
  else tail -n 30 "$GATE_OUT/adr.log" > "$sd/adr.out"; sfail adr "executable ADRs FAILED (log: $GATE_OUT/adr.log)"; echo "adr FAILED $(elapsed "$t0" "$(now)") s" > "$sd/adr.t"; fi
}

# The sample of the quick gate: every 10th stdlib case by name (the order of `ls`, bytes), and the expected failures among them, so a
# sample still shows the harness reporting one. A case is a *.fib or a directory with a main.fib; its name is what `--only` matches.
make_sample() {
  sample=$GATE_OUT/sample.txt
  local names step
  names=$(cd cases/stdlib && for e in $(LC_ALL=C ls); do
            if [ -f "$e" ]; then case $e in *.fib) echo "$e" ;; esac; elif [ -f "$e/main.fib" ]; then echo "$e"; fi; done)
  step=$(( $(echo "$names" | wc -l) / 100 )); [ "$step" -lt 1 ] && step=1
  { echo "$names" | awk -v k="$step" 'NR % k == 0'
    sed -n 's|^cases/stdlib/\([^ ]*\) .*|\1|p' scripts/ci-stage2.expected | sed 's|/main.fib$||'; } | LC_ALL=C sort -u > "$sample"
  echo "gate: stdlib sample of $(wc -l < "$sample") of $(echo "$names" | wc -l) cases (every ${step}th, plus the expected failures)"
}

stage_cases() {
  skipped "cases" compiler lib cases scripts spec && return
  local t0 out code dirs d n got; t0=$(now)
  out=$GATE_OUT/cases; rm -rf "$out"; mkdir -p "$out"
  export CI_STAGE2_OUT=$out CI_STAGE2_JOBS=$jobs CI_STAGE2_SHARDS=$shards
  if [ -n "$sample" ]; then export CI_STAGE2_ONLY=$sample; else unset CI_STAGE2_ONLY; fi
  "$here/ci-stage2.sh" "$F" > "$out/ci-stage2.log" 2>&1; code=$?
  cp "$out/ci-stage2.log" "$sd/cases.out"
  dirs=$(awk '/^== / { d=$4; sub("cases/", "", d) } /^exit [0-9]+ after/ { printf "%s%s %s (exit %s)", sep, d, $4, $2; sep=", " }' "$out/ci-stage2.log")
  echo "cases: $dirs" > "$sd/cases.t"
  [ "$code" -ne 0 ] && sfail cases "cases: ci-stage2.sh failed (the non-passing set differs from scripts/ci-stage2.expected, or a directory timed out)"
  echo "cases total $(elapsed "$t0" "$(now)") s" >> "$sd/cases.t"
  # The case-count floor (scripts/case-floor.expected): a directory that runs fewer cases than its floor fails the gate; the stdlib floor applies to
  # the full run only (the quick gate runs a sample).
  while read -r d n; do
    case $d in ''|'#'*) continue ;; esac
    [ "$d" = stdlib ] && [ -n "$sample" ] && continue
    got=$(awk -v want="cases/$d" '/^== / { cur=$4 } /^[0-9]+ cases:/ { if (cur == want) { print $1; exit } }' "$out/ci-stage2.log")
    if [ -z "$got" ] || [ "$got" -lt "$n" ]; then
      sfail cases "cases: cases/$d ran ${got:-no} cases, the floor is $n (scripts/case-floor.expected): a merge may have lost cases"
    fi
  done < "$here/case-floor.expected"
  [ -f "$sd/cases.fail" ] || touch "$sd/cases.ok"
}

# The specs (fib.test, docs/design/test-harness.md 6.5): `F test specs`, deterministic (--seed 1), every scenario must hold. Not compared with
# scripts/ci-stage2.expected: a failing spec is a regression, not a known gap. Full gate, or GATE_SPECS=1 for a quick one; skipped when there is no specs/.
stage_specs() {
  skipped specs compiler lib specs && return
  local t0; t0=$(now)
  if "$F" test specs -j "$jobs" --seed 1 > "$GATE_OUT/specs.log" 2>&1; then
    echo "specs $(grep '^total:' "$GATE_OUT/specs.log" | sed 's/^total: //; s/ (seed 1)//') $(elapsed "$t0" "$(now)") s" > "$sd/specs.t"; touch "$sd/specs.ok"
  else tail -n 30 "$GATE_OUT/specs.log" > "$sd/specs.out"; sfail specs "specs FAILED (log: $GATE_OUT/specs.log)"; echo "specs FAILED $(elapsed "$t0" "$(now)") s" > "$sd/specs.t"; fi
}

# The static stage (docs/design/static-linking.md 4), full gate or GATE_STATIC=1: `fibc build --static` against the musl pieces (scripts/build-musl.sh; FIB_MUSL_DIR names them, or
# they are beside the compiler). Only when the pieces are there, and reported as skipped, with the reason, when they are not: the toolchain is no dependency of the gate.
# It runs the link-declaration checks (compiler/tests/driver/linklib.sh), cases/ownership and cases/modules as static executables (`FIB_STATIC_CASES=1`: each case is built
# with --static and run), and the stdlib cases of the thread, atom, task and link blocks. Not compared with scripts/ci-stage2.expected: every one must pass.
stage_static() {
  skipped static compiler lib cases scripts && return
  local t0 d log; t0=$(now); log=$GATE_OUT/static.log
  if [ ! -r "${FIB_MUSL_DIR:-/nonexistent}/x86_64/libc.a" ] && [ ! -r "${FIB_MUSL_DIR:-/nonexistent}/libc.a" ]; then
    echo "static SKIPPED: no musl pieces (set FIB_MUSL_DIR to a directory built by scripts/build-musl.sh x86_64 DIR/x86_64): the static build was not tested" > "$sd/static.t"; touch "$sd/static.ok"; return
  fi
  : > "$log"
  F="$F" "$here/../compiler/tests/driver/linklib.sh" >> "$log" 2>&1 || sfail static "static: linklib.sh FAILED"
  for d in ownership modules; do
    FIB_STATIC=1 FIB_STATIC_CASES=1 "$SLOTS_SH" "$F" cases cases/$d -j "$jobs" >> "$log" 2>&1 || sfail static "static: cases/$d as static executables FAILED"
  done
  FIB_STATIC=1 FIB_STATIC_CASES=1 "$SLOTS_SH" "$F" cases cases/stdlib --only 655- 665- 1709- 2228- 2650- 4005- 6106- 6222- 7304- 8000- 8001- 8002- 8003- 8060- 8061- -j "$jobs" >> "$log" 2>&1 \
    || sfail static "static: stdlib cases as static executables FAILED"
  echo "static $(grep -E '^[0-9]+ cases:' "$log" | awk '{p+=$3; f+=$5} END {print p " pass, " f " fail"}') (log: $log) $(elapsed "$t0" "$(now)") s" > "$sd/static.t"
  if [ -f "$sd/static.fail" ]; then tail -n 20 "$log" > "$sd/static.out"; else touch "$sd/static.ok"; fi
}

sample=
[ "$mode" = quick ] && make_sample
# the order of the report
order=()
[ "$mode" = full ] && order+=(fixed golden)
order+=(tools)
[ "$mode" = full ] && order+=(adr)
order+=(cases)
if [ "$mode" = full ] || [ "${GATE_STATIC:-0}" = 1 ]; then order+=(static); fi
if { [ "$mode" = full ] || [ "${GATE_SPECS:-0}" = 1 ]; } && [ -d specs ]; then order+=(specs); fi
# The stages with the longest chains first (the fixed point is one build after another, golden builds its tools and then runs its suites, the cases
# are shards), a moment before the many small scripts of tools, so that the slots they ask for are theirs and the wall time is not a late
# straggler's: the report below is the same whichever order they start in.
for s in fixed golden cases static tools adr specs; do
  [[ " ${order[*]} " == *" $s "* ]] || continue
  ( stage_"$s"; stage_done "$s" ) &
  [ "$s" = cases ] && sleep 3
done
wait

# ---- the report, in a fixed order
for s in "${order[@]}"; do
  [ -f "$sd/$s.out" ] && cat "$sd/$s.out"
  [ -f "$sd/$s.fail" ] && { bad=1; cat "$sd/$s.fail"; }
  [ -f "$sd/$s.ok" ] || bad=1
  [ -f "$sd/$s.t" ] || echo "$s FAILED (no result)" > "$sd/$s.t"
  while IFS= read -r l; do timings+=("$l"); done < "$sd/$s.t"
done

echo
echo "== timing"
for l in "${timings[@]}"; do echo "  $l"; done
echo "  total $(elapsed "$t_all" "$(now)") s"
if [ "$bad" -eq 0 ]; then echo "GATE PASS ($mode)"; exit 0; fi
echo "GATE FAIL ($mode)"; exit 1
