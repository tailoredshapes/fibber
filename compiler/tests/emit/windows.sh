#!/bin/bash
# The lowering of exclusive-view windows (compiler/emit/lower/window.fib, compiler/own/walk/call.fib `amp-pass`; package EV2) as the lIR shows it:
# in the loop of compiler/tests/emit/window-loop.fib an access of a window is an unsigned compare, an address and a load or a store, and a call of the
# library's own function only in the failing branch (a block `wino*`); a window write makes no private cell (the `fib.stack-init` of the function are the
# cells of the owner's lend and of the view, two, however long the loop), and the loop takes no count (no retain or release in its blocks).
# usage: windows.sh FIBC      (a stage 2; run anywhere, it changes into the repository root)
set -euo pipefail
FIBC=${1:?fibc}
R=$(cd "$(dirname "$0")/../../.." && pwd); cd "$R" || exit 2
S=${WINDOWS_SCRATCH:-$HOME/.cache/fibber-scratch/windows-test}; mkdir -p "$S"
FIB_LIB=$R/lib "$FIBC" emit compiler/tests/emit/window-loop.fib > "$S/window-loop.ll" || { echo "windows: emit failed"; exit 2; }
"$FIBC" build compiler/tests/emit/inspect.fib -I compiler -I lib -o "$S/inspect"
"$S/inspect" windows "$S/window-loop.ll" f.sweep
