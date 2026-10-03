#!/bin/bash
# The batch integrator: judges several branches together, so the lead pays for one build and one gate instead of one for each.
#   scripts/batch.sh [--full] [--no-bench] [--keep] BRANCH..
# Makes a scratch branch (batch/TIMESTAMP) from main in a scratch worktree, cherry-picks each branch's commits (those not in main, oldest
# first, branches in the order given) and stops on a conflict, naming the commit and the branch. Then ONE gate (scripts/gate.sh --quick,
# or --full with the flag) and, if it passes, ONE bench compare (scripts/bench/quick.sh). If the gate fails it finds the offending branch by
# halving the batch: the gate again on the first half; failing, the offender is in it; passing, the first half is accepted and the search goes
# on in the second half on top of it. Builds are reused: stage 2 is cached by the tree's stamp (scripts/lib/stage2.sh), so a half
# that changes only cases or the tree it already built costs no build. Prints one report at the end. Exit 0 the batch is good, 1 it
# is not (the report names the branch), 2 setup or a conflict.
# Never touches main or any branch you name, never pushes: the scratch worktree and branch are deleted at the end (--keep keeps them).
# Environment: BATCH_BASE (default main); GATE_OUT (default ~/.cache/fibber-scratch/gate-batch); FIBC/SEED/LAIR_DIR as for gate.sh.
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
full=; bench=1; keep=
while [ $# -gt 0 ]; do
  case $1 in
    --full) full=--full ;; --no-bench) bench= ;; --keep) keep=1 ;;
    -*) echo "usage: scripts/batch.sh [--full] [--no-bench] [--keep] BRANCH.." >&2; exit 2 ;;
    *) break ;;
  esac
  shift
done
[ $# -ge 1 ] || { echo "usage: scripts/batch.sh [--full] [--no-bench] [--keep] BRANCH.." >&2; exit 2; }
branches=("$@")
base=${BATCH_BASE:-main}
export GATE_OUT=${GATE_OUT:-$HOME/.cache/fibber-scratch/gate-batch}
root=$repo
# shellcheck source=lib/stage2.sh
. "$here/lib/stage2.sh"
take_suite_lock

for b in "${branches[@]}"; do git -C "$repo" rev-parse --verify -q "$b^{commit}" > /dev/null || { echo "batch: no branch $b" >&2; exit 2; }; done
git -C "$repo" rev-parse --verify -q "$base^{commit}" > /dev/null || { echo "batch: no base $base" >&2; exit 2; }
stamp=$(date +%Y%m%d-%H%M%S)
wt=$HOME/.cache/fibber-scratch/batch-wt-$stamp
sb=batch/$stamp
git -C "$repo" worktree add -q -b "$sb" "$wt" "$base" || exit 2
cleanup() {
  if [ -z "$keep" ]; then git -C "$repo" worktree remove --force "$wt" > /dev/null 2>&1; git -C "$repo" branch -D "$sb" > /dev/null 2>&1
  else echo "kept: worktree $wt, branch $sb"; fi
}
trap cleanup EXIT
export GATE_ROOT=$wt
G() { git -C "$wt" "$@"; }

report=(); gates=0
say() { report+=("$*"); echo "batch: $*"; }

# apply_set BRANCH..: the scratch tree is the base with the commits of these branches on it. Returns 3 on a conflict (naming it), 0 else.
apply_set() {
  G cherry-pick --abort > /dev/null 2>&1; G reset -q --hard "$base"; G clean -qfd
  local b c
  for b in "$@"; do
    for c in $(git -C "$repo" rev-list --reverse "$base..$b"); do
      if ! G cherry-pick --empty=drop "$c" > /dev/null 2>&1; then
        if [ -n "$(G diff --name-only --diff-filter=U)" ]; then
          say "CONFLICT cherry-picking $(git -C "$repo" log -1 --format='%h %s' "$c") (branch $b): $(G diff --name-only --diff-filter=U | paste -sd' ')"
          G cherry-pick --abort > /dev/null 2>&1; return 3
        fi
        G cherry-pick --skip > /dev/null 2>&1
      fi
    done
  done
  return 0
}

# gate_set BRANCH..: apply, then one gate; 0 passes, 1 fails, 3 conflict. The gate's output goes to a log; the last lines are shown.
gate_set() {
  local n=$gates log; gates=$((gates + 1)); log=$GATE_OUT/batch-gate-$stamp-$gates.log
  apply_set "$@" || return 3
  local t0; t0=$(now)
  "$here/gate.sh" ${full:---quick} > "$log" 2>&1; local code=$?
  say "gate $gates on [$*]: $(tail -n 1 "$log") in $(elapsed "$t0" "$(now)") s ($(grep -E '^(build:)' "$log" | head -1)); log $log"
  return $((code != 0))
}

# bisect PREFIX_COUNT: the offender among branches[lo..hi), the first lo of them accepted; sets BAD.
BAD=
bisect() {
  local lo=$1 hi=$2 mid
  if [ $((hi - lo)) -eq 1 ]; then BAD=${branches[$lo]}; return; fi
  mid=$(( (lo + hi) / 2 ))
  if gate_set "${branches[@]:0:$mid}" && [ "$mid" -gt "$lo" ]; then bisect "$mid" "$hi"; else bisect "$lo" "$mid"; fi
}

result=0
gate_set "${branches[@]}"; code=$?
if [ "$code" -eq 3 ]; then
  echo "BATCH CONFLICT: $(printf '%s\n' "${report[@]}" | grep CONFLICT | head -1)"; exit 2
elif [ "$code" -eq 0 ]; then
  say "all ${#branches[@]} branches pass the gate together"
  if [ -n "$bench" ]; then
    t0=$(now); "$here/bench/quick.sh" > "$GATE_OUT/batch-bench-$stamp.log" 2>&1; bcode=$?
    say "bench compare (exit $bcode, $(elapsed "$t0" "$(now)") s): $(tail -n 1 "$GATE_OUT/batch-bench-$stamp.log"); log $GATE_OUT/batch-bench-$stamp.log"
    sed 's/^/  /' "$GATE_OUT/batch-bench-$stamp.log"
  fi
else
  say "the gate fails on the whole batch: halving"
  if [ "${#branches[@]}" -eq 1 ]; then BAD=${branches[0]}; else bisect 0 "${#branches[@]}"; fi
  say "OFFENDER: branch $BAD (the gate passes on what comes before it in the batch and fails with it)"
  result=1
fi

echo
echo "== batch report ($(( gates )) gate runs, base $base $(git -C "$repo" rev-parse --short "$base"))"
printf '  %s\n' "${report[@]}"
if [ "$result" -eq 0 ]; then echo "BATCH PASS: ${branches[*]}"; else echo "BATCH FAIL: offender $BAD; the others may be merged after it is fixed or left out"; fi
exit $result
