#!/bin/bash
# scripts/mutant-deps.sh: the planted faults of the package manager (docs/design/packages.md §7). Copies compiler/ and lib/ of this tree to a
# scratch directory, applies ONE fault, builds a stage 2 from the copy (nothing in the tree changes) and runs compiler/tests/deps/run.sh
# with it: the run must FAIL (a check that fails, or a build error). A fault under which every check passes means the rule has no test.
#
# usage: scripts/mutant-deps.sh [FAULT..]        (default: all)
#   lock-ignored     resolution does not pin the locked commits: every tag is asked of the remote again    (locked: builds with no remote)
#   conflict-silent  a second request at another commit is dropped (first wins) instead of a CONFLICT      (conflict: refused)
#   sha-unverified   a checkout's commit is not compared with the locked sha                               (tamper: another commit)
#   files-unverified a checkout's changed files are not looked for                                         (tamper: build refused)
#   offline-ignored  --offline fetches anyway                                                              (offline, cold cache: refused)
#   order-reversed   the libraries' roots come in reverse lock order                                       (path: ..)
# environment: FIBC (the stage 2 or seed that builds the mutant; required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-deps).
# exit: 0 when every fault was caught, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FAULTS=("$@")
[ ${#FAULTS[@]} -gt 0 ] || FAULTS=(lock-ignored conflict-silent sha-unverified files-unverified offline-ignored order-reversed)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-deps}
FIBC=${FIBC:?mutant-deps: set FIBC to a fibc that builds stage 2}
root=$R; GATE_OUT=$OUT; mkdir -p "$OUT"
. "$R/scripts/lib/stage2.sh"; llvm_link_args || exit 2   # LLVM_ARGS: how a stage 2 links LLVM (LLVM_LINK, LLVM_LIBDIR)
ulimit -v 16000000

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error (the source changed under the fault)
  cp "$1" "$1.orig"; perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-deps: pattern not found in $1: $2" >&2; exit 2; }
  rm -f "$1.orig"
}
plant() {
  local t=$1/compiler/pkg
  case $2 in
    lock-ignored)     sub "$t/project-mode.fib" 's/\(pins \(match lock \(nil \[\]\) \(\(some l\) \(\. l libs\)\)\)\)/(pins [])/' ;;
    conflict-silent)  sub "$t/resolve.fib" 's/\(if same \(Ok nil\) \(Err \(conflict r l \(request-where o r\)\)\)\)/(Ok nil)/' ;;
    sha-unverified)   sub "$t/git.fib" 's/\(not \(= \(nth ids 0\) sha\)\) \(Err/false (Err/' ;;
    files-unverified) sub "$t/git.fib" 's/\(Err \(str dir ": the checkout\x27s files were changed: " \(x\/first-line changed\)\)\)/(Ok (nth ids 1))/' ;;
    offline-ignored)  sub "$t/git.fib" 's/offline \(Err \(str sha/false (Err (str sha/' ;;
    order-reversed)   sub "$t/modules.fib" 's/\(project-roots p test\) libs\)\)/(project-roots p test) (vec (reverse libs))))/' ;;
    *) echo "mutant-deps: no fault $2" >&2; exit 2 ;;
  esac
}

survived=0
for f in "${FAULTS[@]}"; do
  t=$OUT/$f; rm -rf "$t"; mkdir -p "$t"; cp -r "$R/compiler" "$R/lib" "$t/"
  plant "$t" "$f"
  if ! (cd "$t" && FIB_LIB=$t/lib "$FIBC" build compiler/fibc.fib -I compiler -I lib "${LLVM_ARGS[@]}" -o "$t/F") > "$t/build.log" 2>&1; then
    echo "killed    $f (stage 2 does not build: $t/build.log)"; continue
  fi
  if (cd "$R" && FIB_LIB=$R/lib "$R/compiler/tests/deps/run.sh" "$t/F") > "$t/run.log" 2>&1; then
    echo "SURVIVED  $f: every check passed ($t/run.log)"; survived=1
  else
    echo "killed    $f: $(grep -c '^FAIL' "$t/run.log") checks failed, first: $(grep -m1 '^FAIL' "$t/run.log" | cut -c1-120)"
  fi
done
[ $survived = 0 ] && echo "mutant-deps: every fault was caught" || echo "mutant-deps: a fault SURVIVED"
exit $survived
