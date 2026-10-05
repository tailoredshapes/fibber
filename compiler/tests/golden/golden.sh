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
# Environment: GOLDEN_OUT scratch (default ~/.cache/fibber-scratch/golden), FIB_LIB is set to lib/ of this tree.
# Prints `ok SUITE (N inputs)` or `FAIL SUITE: first differing input` for each suite; exit 0 only if none failed; 2 for a usage error.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
. "$here/suites.sh"
show=; showf=; update=0; rescan=0; only=(); tools=; fibc=${FIBC:-}
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
  for t in read expand types own explain emit lairf; do
    "$fibc" build "compiler/$t.fib" -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$tools/$t" || { echo "golden: could not build $t" >&2; exit 2; }
  done
fi

# block TOOL OPTIONS FILE: one input's block (the `#### FILE` line, the output, the `status N` line) in $out/block
block() {
  { echo "#### $3"; (ulimit -v 8000000; timeout 120 "$tools/$1" $2 "$3" 2>&1; echo "status $?") | sed "s|$root|<root>|g"; } > "$out/block"
}

if [ -n "$show" ]; then   # --show SUITE FILE: stage 2's output for one input of a suite
  while IFS='|' read -r name tool opts _rust _globs; do
    if [ "$name" = "$show" ]; then block "$tool" "$opts" "$showf"; cat "$out/block"; exit 0; fi
  done < <(suite_table)
  echo "golden: no suite $show" >&2; exit 2
fi
bad=0
while IFS='|' read -r name tool opts _rust globs; do
  [ -z "$name" ] && continue
  if [ ${#only[@]} -gt 0 ] && [[ " ${only[*]} " != *" $name "* ]]; then continue; fi
  files=$here/$name.files; gold=$here/$name.golden
  if [ "$rescan" -eq 1 ]; then
    # the input list: every file the suite's globs match now, in byte order (a new input joins the suite on --update)
    # shellcheck disable=SC2086
    ls -1 $globs 2>/dev/null | LC_ALL=C sort > "$files"
  fi
  [ -f "$files" ] && [ -f "$gold" ] || { [ "$rescan" -eq 1 ] || { echo "FAIL $name: no golden ($name.golden)"; bad=1; continue; }; }
  new=$out/$name.new; : > "$new"
  while read -r f; do block "$tool" "$opts" "$f"; shape "$name" "$out/block" "$f" >> "$new"; done < "$files"
  if [ "$update" -eq 1 ]; then cp "$new" "$gold"; echo "updated $name ($(wc -l < "$files") inputs)"; continue; fi
  if cmp -s "$new" "$gold"; then echo "ok $name ($(wc -l < "$files") inputs)"
  else
    echo "FAIL $name: $(diff "$gold" "$new" | grep -m1 '^[<>]' | cut -c1-160)"; diff "$gold" "$new" | head -8; bad=1
  fi
done < <(suite_table)
exit $bad
