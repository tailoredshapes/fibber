#!/bin/bash
# scripts/tsan.sh: ThreadSanitizer over a fibber program, through the lIR (docs/design/parallelism.md 2.8, 3.8). No compiler change: the
# program is emitted as lIR with its runtime (`F emit`), turned into LLVM IR (`lairf emit-llvm`), every `define` gets `sanitize_thread`,
# `opt -passes=tsan` instruments it, `llc` makes an object and `gcc -fsanitize=thread` links it (GCC's libtsan has LLVM's ABI). The
# program runs, and each distinct report is summarised as a SIGNATURE (kind, the two accesses as `operation function`, no addresses).
#
#   scripts/tsan.sh [--any-result] [--check] PROGRAM.fib [args..]
#     without --check   print the signatures of this run (exit 0 unless the build failed or the program could not run)
#     --check           also compare them with the baseline (scripts/tsan-baseline.txt, or $TSAN_BASELINE): exit 1 when a signature is
#                       not in it (a NEW report) or the program exits with a code other than its own (TSAN's exit code 66 is not counted
#                       while the reports are known). Baseline lines: SIGNATURE <TAB> justification; `#` starts a comment.
#   scripts/tsan.sh --suite      the case set of scripts/tsan-suite.txt (one program per line, args after it) with --check, one summary line
#                                per program and a total; exit 1 when any program has a NEW report
#
# Environment (all optional)
#   F             the stage 2 fibc that emits the lIR (default: the gate's F for this tree: ~/.cache/fibber-scratch/gate-<tree name>/F)
#   TSAN_OUT      scratch (default ~/.cache/fibber-scratch/tsan): the lairf built from the tree, and one directory per program
#   TSAN_RUNS     runs of the instrumented binary (default 3): races depend on timing, so a report seen once counts; a clean run proves
#                 nothing about the other interleavings
#   TSAN_TIMEOUT  seconds per run (default 300)
#   TSAN_ULIMIT   `ulimit -v` for the instrumented run (default unlimited: TSAN reserves tens of terabytes of address space for its shadow
#                 memory, so the 16 GB cap of the other tools cannot apply; the real memory is a few times the program's)
#   TSAN_SED      a sed -E expression applied to the LLVM IR before instrumentation: a PLANTED fault (scripts/tsan-planted.sh)
#   FIB_LIB       the library directory (default: the tree's lib/)
#   MALLOC_ARENA_MAX is set to 2 for the runs.
#
# What a clean result means: no report in TSAN_RUNS runs of this program on this machine beyond the baseline. TSAN finds data races on
# NON-atomic memory (the two accesses are not ordered by a happens-before edge it can see). It cannot find ordering bugs among atomics (a
# weakened fence or memory order, a lost wakeup, an ABA): those need a model checker (parallelism.md 3.8). It is not a proof that the
# runtime is race free.
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
OUT=${TSAN_OUT:-$HOME/.cache/fibber-scratch/tsan}
RUNS=${TSAN_RUNS:-3}
TMO=${TSAN_TIMEOUT:-300}
BASE=${TSAN_BASELINE:-$root/scripts/tsan-baseline.txt}
F=${F:-$HOME/.cache/fibber-scratch/gate-$(basename "$root")/F}
LLVM=${LLVM_BIN:-/usr/lib/llvm-21/bin}
export FIB_LIB=${FIB_LIB:-$root/lib}
mkdir -p "$OUT/tmp"; export TMPDIR=$OUT/tmp

check=0; anyres=0
[ "${1:-}" = --any-result ] && { anyres=1; shift; }   # do not compare the printed result (a case whose answer is about the resident set)
case ${1:-} in
  --check) check=1; shift ;;
  --suite)
    suite=${TSAN_SUITE:-$root/scripts/tsan-suite.txt}; bad=0; n=0
    while read -r prog args; do
      case $prog in ''|'#'*) continue ;; esac
      flag=; case $prog in any-result:*) flag=--any-result; prog=${prog#any-result:} ;; esac
      n=$((n+1)); [ -e "$prog" ] || prog=$root/$prog
      # shellcheck disable=SC2086
      "$0" $flag --check "$prog" $args > "$OUT/suite-last.out" 2>&1; rc=$?
      echo "$([ $rc = 0 ] && echo ok || echo FAIL) $(basename "$prog") $args :: $(tail -1 "$OUT/suite-last.out")"
      [ $rc = 0 ] || { bad=$((bad+1)); sed 's/^/    /' "$OUT/suite-last.out" | grep -E 'NEW|build|exit' | head -5; }
    done < "$suite"
    echo "tsan suite: $n programs, $bad with a NEW report or a failure (baseline $BASE)"
    [ $bad = 0 ]; exit $? ;;
esac
prog=${1:?usage: scripts/tsan.sh [--check] PROGRAM.fib [args..] | --suite}; shift
[ -x "$F" ] || { echo "tsan: no stage 2 at $F: build one or set F" >&2; exit 2; }
[ -f "$prog" ] || { echo "tsan: no such program $prog" >&2; exit 2; }
for t in opt llc llvm-symbolizer; do [ -x "$LLVM/$t" ] || { echo "tsan: $LLVM/$t is missing" >&2; exit 2; }; done

# lairf: the tool that turns lIR into LLVM IR, built by F from the tree and cached by (F, the tree's sources).
stamp=$( { sha1sum < "$F"; (cd "$root" && find compiler lib -type f -not -path 'compiler/tests/*' | LC_ALL=C sort | xargs sha1sum | sha1sum); } | sha1sum | cut -c1-12)
lairf=$OUT/lairf-$stamp
if [ ! -x "$lairf" ]; then
  (cd "$root" && ulimit -v 16000000 && "$F" build compiler/lairf.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$lairf") > "$OUT/lairf.build" 2>&1 \
    || { echo "tsan: building lairf failed: $(tail -3 "$OUT/lairf.build")" >&2; exit 2; }
fi

name=$(basename "$prog" .fib | cut -c1-40)
w=$OUT/work/$name; mkdir -p "$w"; rm -f "$w"/run.*
# the case's `;; roots:` header names its support directories, relative to the file
incs=(); for r in $(sed -n 's/^;; roots:[ \t]*//p' "$prog" | head -1); do incs+=(-I "$(cd "$(dirname "$prog")" && cd "$r" 2>/dev/null && pwd)"); done
(ulimit -v 16000000; "$F" emit "${incs[@]}" "$prog" > "$w/p.lir") 2> "$w/emit.err" || { echo "tsan: emit failed: $(head -3 "$w/emit.err")" >&2; exit 2; }
(ulimit -v 16000000; "$lairf" emit-llvm "$w/p.lir" > "$w/p.ll") 2> "$w/llvm.err" || { echo "tsan: emit-llvm failed: $(head -3 "$w/llvm.err")" >&2; exit 2; }
[ -n "${TSAN_SED:-}" ] && { cp "$w/p.ll" "$w/p.pre.ll"; sed -i -E "$TSAN_SED" "$w/p.ll"; cmp -s "$w/p.ll" "$w/p.pre.ll" && { echo "tsan: TSAN_SED changed nothing" >&2; exit 2; }; }
sed -E 's/^(define [^{]*) \{$/\1 sanitize_thread {/' "$w/p.ll" > "$w/p.t.ll"
[ "$(grep -c '^define .*sanitize_thread' "$w/p.t.ll")" -gt 0 ] || { echo "tsan: no define was marked sanitize_thread" >&2; exit 2; }
"$LLVM/opt" -passes=tsan "$w/p.t.ll" -S -o "$w/p.tsan.ll" && "$LLVM/llc" -O1 -relocation-model=pic -filetype=obj "$w/p.tsan.ll" -o "$w/p.o" \
  && gcc -fsanitize=thread "$w/p.o" -o "$w/p.bin" -lm || { echo "tsan: instrumented build failed" >&2; exit 2; }

export MALLOC_ARENA_MAX=2
export TSAN_OPTIONS="exitcode=66 report_signal_unsafe=0 external_symbolizer_path=$LLVM/llvm-symbolizer"
codes=""
for i in $(seq 1 "$RUNS"); do
  (ulimit -v "${TSAN_ULIMIT:-unlimited}"; timeout "$TMO" "$w/p.bin" "$@" > "$w/run.$i.out" 2> "$w/run.$i.err"); codes="$codes $?"
done

# Signatures: one per report, "kind | op function | op function" (the two accesses of a race sorted; other kinds: kind | first frame).
sig() { awk '
  function emit() { if (kind == "") return;
    if (kind == "data race") { a = acc[1]; b = acc[2]; if (a > b) { t = a; a = b; b = t }; print kind " | " a " | " b (loc != "" ? " | " loc : "") }
    else print kind " | " acc[1]
    kind = "" }
  /^WARNING: ThreadSanitizer: / { emit(); kind = $0; sub(/^WARNING: ThreadSanitizer: /, "", kind); sub(/ \(pid=.*/, "", kind); n = 0; loc = ""; want = 0; acc[1] = acc[2] = ""; next }
  kind == "" { next }
  /^  (Previous )?([Aa]tomic )?([Ww]rite|[Rr]ead) of size/ { op = tolower($0); sub(/^ *previous /, "", op); sub(/^ +/, "", op); sub(/ of size.*/, "", op); want = 1; next }
  /^  Location is global/ { loc = $0; sub(/^ *Location is /, "", loc); sub(/ of size.*/, "", loc); sub(/ at 0x.*/, "", loc); next }
  /^    #0 / && kind != "data race" && acc[1] == "" { acc[1] = $2; next }
  /^    #0 / && want { n++; acc[n] = op " " $2; want = 0; next }
  /^SUMMARY/ { emit() }
  END { emit() }' "$@" | sed -E 's/\.[0-9]+( \||$)/.N\1/g' | sort | uniq -c | sed -E 's/^ *([0-9]+) /\1\t/'; }

cat "$w"/run.*.err 2>/dev/null > "$w/all.err"
sig "$w"/run.*.err > "$w/signatures.txt"
echo "program $prog  runs $RUNS  exit codes$codes  instrumented defines $(grep -c '^define .*sanitize_thread' "$w/p.t.ll")"
if [ -s "$w/signatures.txt" ]; then cat "$w/signatures.txt" | sed 's/^/report x/'; else echo "no reports"; fi
[ $check = 1 ] || exit 0

# Compare with the baseline.
new=0
while IFS=$'\t' read -r cnt s; do
  if grep -qxF -- "$s" <(sed -e '/^#/d' -e '/^$/d' "$BASE" 2>/dev/null | cut -f1); then echo "known x$cnt: $s"; else echo "NEW x$cnt: $s"; new=1; fi
done < "$w/signatures.txt"
# A run is a failure too when it timed out, or printed a result other than the header's `;; result:` (else 0: the kernels' answer), or
# (a case that expects a trap) did not exit nonzero. TSAN's own exit code is 66.
want=$(sed -n 's/^;; result:[ \t]*\(-\{0,1\}[0-9]*\).*/\1/p' "$prog" | head -1); want=${want:-0}
trapcase=0; grep -q '^;; expect:[ \t]*trap' "$prog" && trapcase=1
i=0
for c in $codes; do
  i=$((i+1)); got=$(tail -1 "$w/run.$i.out" 2>/dev/null)
  if [ "$c" = 124 ]; then echo "exit: run $i timed out after $TMO s"; new=1
  elif [ $trapcase = 1 ]; then [ "$c" != 0 ] || { echo "exit: run $i did not trap (code $c)"; new=1; }
  elif [ "$c" != 0 ] && [ "$c" != 66 ]; then echo "exit: run $i ended with code $c"; new=1
  elif [ $anyres = 0 ] && [ "$got" != "$want" ]; then echo "exit: run $i printed result '$got', the program's result is $want (header, else 0)"; new=1; fi
done
[ $new = 0 ] && echo "tsan: no report beyond the baseline ($(grep -vc '^#\|^$' "$BASE" 2>/dev/null) known) in $RUNS runs"
exit $new
