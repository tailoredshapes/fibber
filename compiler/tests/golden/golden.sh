#!/bin/bash
# The golden checks of the passes of compiler/ (spec/bootstrap.md): each suite of suites.sh runs a pass tool built from stage 2 on its inputs
# and compares the output and exit status of every input with the golden recorded in this directory (NAME.golden, inputs in NAME.files).
# The goldens were first recorded from the Rust oracles of tag seed-1 (see docs/rust-legacy.md), kept only for the inputs on which stage 2
# and the Rust agreed byte for byte on the day they were recorded (RECORDED.txt says how many and which were left out). They are now stage
# 2's own: after an intended change of a pass, regenerate with --update and read the diff in git before committing it.
# usage: compiler/tests/golden/golden.sh [--update [--rescan]] [--only SUITE..] [--tools DIR | --fibc F]
#        compiler/tests/golden/golden.sh --show SUITE FILE [--tools DIR | --fibc F]     stage 2's output for one input
#   --tools DIR  a directory holding the pass tools (read expand types own explain emit lairf), built from compiler/TOOL.fib with a stage 2 fibc
#   --fibc F     (default: FIBC, else the gate's F) builds the tools it needs into $GOLDEN_OUT/tools first (about a minute)
# --update keeps each suite's list of inputs (the ones the Rust agreed on); --rescan takes every file the suite's globs match now, and so
# adds the inputs stage 2 alone has judged.
# Environment: GOLDEN_JOBS suites (and tool builds) at a time, default 4 (the gate's GATE_SLOTS limit the heavy ones); GOLDEN_OUT scratch (default ~/.cache/fibber-scratch/golden), FIB_LIB is set to lib/ of this tree.
# Prints `ok SUITE (N inputs)` or `FAIL SUITE: first differing input` for each suite; exit 0 only if none failed; 2 for a usage error.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
. "$here/suites.sh"
. "$here/../../../scripts/lib/slots.sh"   # under the gate (GATE_SLOTS) a tool build and a suite each hold a slot; alone nothing is limited
jobs=${GOLDEN_JOBS:-4}; show=; showf=; update=0; rescan=0; only=(); tools=; fibc=${FIBC:-}
while [ $# -gt 0 ]; do
  case $1 in
    --update) update=1 ;;
    --rescan) rescan=1 ;;
    --only) shift; while [ $# -gt 0 ] && [ "${1#--}" = "$1" ]; do only+=("$1"); shift; done; continue ;;
    --tools) tools=${2:?--tools DIR}; shift ;;
    --fibc) fibc=${2:?--fibc F}; shift ;;
    --show) show=${2:?--show SUITE FILE}; showf=${3:?--show SUITE FILE}; shift 2 ;;
    *) echo "usage: golden.sh [--update [--rescan]] [--only SUITE..] [--tools DIR | --fibc F]" >&2; exit 2 ;;
  esac
  shift
done
out=${GOLDEN_OUT:-$HOME/.cache/fibber-scratch/golden}; mkdir -p "$out/tmp"
export TMPDIR=$out/tmp FIB_LIB=$root/lib FIB_TARGET_CPU=x86-64-v2
cd "$root" || exit 2
if [ -z "$tools" ]; then
  [ -z "$fibc" ] && fibc=$HOME/.cache/fibber-scratch/gate-$(basename "$root")/F
  [ -x "$fibc" ] || { echo "golden: no fibc to build the tools with: give --tools DIR or --fibc F (or FIBC)" >&2; exit 2; }
  tools=$out/tools; mkdir -p "$tools"
  stamp=$(sha1sum < "$fibc" | cut -c1-16)-$(cat "$root"/compiler/*/*.fib "$root"/lib/prelude.fib | sha1sum | cut -c1-16)   # the tools are rebuilt when the fibc or the sources change
  if [ "$(cat "$tools/.stamp" 2>/dev/null)" != "$stamp" ]; then
    pids=()
    for t in read expand types own explain emit lairf; do
      "$root/scripts/lib/slots.sh" "$fibc" build "compiler/$t.fib" -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$tools/$t" &
      pids+=($!)
      if [ ${#pids[@]} -ge "$jobs" ]; then wait "${pids[0]}" || { echo "golden: could not build a tool" >&2; exit 2; }; pids=("${pids[@]:1}"); fi
    done
    for p in "${pids[@]}"; do wait "$p" || { echo "golden: could not build a tool" >&2; exit 2; }; done
    echo "$stamp" > "$tools/.stamp"
  fi
fi

# block TOOL OPTIONS FILE BLOCKFILE: one input's block (the `#### FILE` line, the output, the `status N` line) in BLOCKFILE
block() {
  { echo "#### $3"; (ulimit -v 8000000; timeout 120 "$tools/$1" $2 "$3" 2>&1; echo "status $?") | sed "s|$root|<root>|g"; } > "$4"
}

if [ -n "$show" ]; then   # --show SUITE FILE: stage 2's output for one input of a suite
  while IFS='|' read -r name tool opts _rust _globs; do
    if [ "$name" = "$show" ]; then block "$tool" "$opts" "$showf" "$out/block"; cat "$out/block"; exit 0; fi
  done < <(suite_table)
  echo "golden: no suite $show" >&2; exit 2
fi
# run_suite NAME TOOL OPTIONS GLOBS: one suite; prints its lines, returns 1 if it failed (run in the background, $jobs at a time)
run_suite() {
  local name=$1 tool=$2 opts=$3 globs=$4 files=$here/$1.files gold=$here/$1.golden new=$out/$1.new f
  if [ "$rescan" -eq 1 ]; then
    # the input list: every file the suite's globs match now, in byte order (a new input joins the suite on --update)
    # shellcheck disable=SC2086
    ls -1 $globs 2>/dev/null | LC_ALL=C sort > "$files"
  fi
  [ -f "$files" ] && [ -f "$gold" ] || { [ "$rescan" -eq 1 ] || { echo "FAIL $name: no golden ($name.golden)"; return 1; }; }
  : > "$new"
  while read -r f; do block "$tool" "$opts" "$f" "$out/block.$name"; shape "$name" "$out/block.$name" "$f" >> "$new"; done < "$files"
  if [ "$update" -eq 1 ]; then cp "$new" "$gold"; echo "updated $name ($(wc -l < "$files") inputs)"; return 0; fi
  if cmp -s "$new" "$gold"; then echo "ok $name ($(wc -l < "$files") inputs)"; return 0; fi
  echo "FAIL $name: $(diff "$gold" "$new" | grep -m1 '^[<>]' | cut -c1-160)"; diff "$gold" "$new" | head -8; return 1
}

bad=0; names=(); running=0
while IFS='|' read -r name tool opts _rust globs; do
  [ -z "$name" ] && continue
  if [ ${#only[@]} -gt 0 ] && [[ " ${only[*]} " != *" $name "* ]]; then continue; fi
  names+=("$name")
  ( slot_acquire; run_suite "$name" "$tool" "$opts" "$globs" > "$out/res.$name"; echo $? > "$out/res.$name.status"; slot_release ) &
  running=$((running + 1))
  while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do sleep 0.1; done
done < <(suite_table)
wait
for name in "${names[@]}"; do cat "$out/res.$name"; [ "$(cat "$out/res.$name.status")" = 0 ] || bad=1; done
exit $bad
