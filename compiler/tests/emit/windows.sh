#!/bin/bash
# The lowering of exclusive-view windows (compiler/emit/lower/window.fib, compiler/own/walk/call.fib `amp-pass`; package EV2) as the lIR shows it:
# in the loop of compiler/tests/emit/window-loop.fib an access of a window is an unsigned compare, an address and a load or a store, and a call of the
# library's own function only in the failing branch (a block `wino*`); a window write makes no private cell (the `fib.stack-init` of the function are the
# cells of the owner's lend and of the view, two, however long the loop), and the loop takes no count (no retain or release in its blocks).
# usage: windows.sh FIBC      (a stage 2; run anywhere, it changes into the repository root)
set -u
FIBC=${1:?fibc}
R=$(cd "$(dirname "$0")/../../.." && pwd); cd "$R" || exit 2
S=${WINDOWS_SCRATCH:-$HOME/.cache/fibber-scratch/windows-test}; mkdir -p "$S"
FIB_LIB=$R/lib "$FIBC" emit compiler/tests/emit/window-loop.fib > "$S/window-loop.ll" || { echo "windows: emit failed"; exit 2; }
awk '/^\(define internal tailcc \(f\.sweep /{on=1} on&&/^\(define/&&!/f\.sweep /{on=0} on' "$S/window-loop.ll" > "$S/sweep.ll"
[ -s "$S/sweep.ll" ] || { echo "windows: no f.sweep in the lIR"; exit 2; }
bad=0
inits=$(grep -c 'fib.stack-init' "$S/sweep.ll")
[ "$inits" -eq 2 ] || { echo "FAIL: $inits stack cells in sweep, want 2 (a private cell per access?)"; bad=1; }
checks=$(grep -c '(icmp ult' "$S/sweep.ll")
[ "$checks" -ge 2 ] || { echo "FAIL: $checks unsigned compares in sweep, want at least 2 (an inline access has one each)"; bad=1; }
stray=$(awk '/^  \(block /{b=$2} /call @f\.fib\.view\.win-/{ if (b !~ /^wino/) print b ": " $0 }' "$S/sweep.ll")
[ -z "$stray" ] || { echo "FAIL: a call of a window function outside a failing branch:"; echo "$stray"; bad=1; }
# the blocks of the loop are the ones between the loop header and the exit; the exit's own release of the view and retain of the array come after the loop
counts=$(awk '/^  \(block /{b=$2} /fib\.(retain|release)/{ if (b ~ /^(then|loop|winb|wino|ok)[0-9]*$/) print b ": " $0 }' "$S/sweep.ll")
[ -z "$counts" ] || { echo "FAIL: a count taken inside the loop:"; echo "$counts"; bad=1; }
[ $bad -eq 0 ] && echo "windows: the loop of window-loop.fib is inline (2 cells, $checks unsigned bound checks, no count, slow calls only in failing branches)"
exit $bad
