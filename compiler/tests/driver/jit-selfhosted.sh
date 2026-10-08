#!/bin/bash
# DARWIN-3: the gate's F is built by the SEED; a compiler built by a release (F3 here: built by F) runs its own `main` on a 256 MiB stack thread and keeps the first thread parked
# (docs/design/stack.md), and the program it runs with `fibc run` (the JIT) lives in that process. A thread count that read the operating system saw the compiler's threads
# (macOS: `threads 2` for a program with one; cases 8354 and 8642 failed under F3's JIT and passed under the seed-built F). The seed-built gate could not see this class of bug, so:
#   1. F and F3 print the same for a program that counts its threads alone (1), with a task sleeping (2) and after the join: `1 2 8`, with the stack thread and with FIB_STACK_MB=0;
#   2. the pool, stack, process and thread cases run through F3's JIT (`F3 cases ... --only`) and all pass.
# usage: jit-selfhosted.sh F F3        (F3: a compiler built by F: build/F3 of the gate). Exit 0 all hold, 1 a check failed, 2 no tools.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root" || exit 2
# shellcheck disable=SC1091
[ -f scripts/portable/env.sh ] && . scripts/portable/env.sh
F=${1:?F}; F3=${2:?F3}
[ -x "$F" ] && [ -x "$F3" ] || { echo "jit-selfhosted: no F or F3" >&2; exit 2; }
export FIB_LIB=$root/lib
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
prog=compiler/tests/driver/jit/threads.fib
for c in "$F" "$F3"; do
  for mb in default 0; do
    if [ $mb = 0 ]; then out=$(FIB_STACK_MB=0 "$c" run "$prog" 2>&1 | tr '\n' ' '); else out=$("$c" run "$prog" 2>&1 | tr '\n' ' '); fi
    if [ "$out" = "1 2 8 102 " ]; then ok "$(basename "$c") run, stack $mb: 1 2 8 (102)"; else no "$(basename "$c") run, stack $mb: [$out], want [1 2 8 102 ]"; fi
  done
done
only="7531 7532 8350 8351 8352 8353 8354 8640 8641 8642 8643 8644 8645 8646 8647 8648 912 919 8741 8780"
# shellcheck disable=SC2086
res=$("$F3" cases cases/stdlib --only $only -j 4 2>&1 | tail -n 1)
case $res in
  *" 0 fail, 0 pending, 0 header error"*) ok "F3's JIT: $res" ;;
  *) no "F3's JIT: $res"
     # shellcheck disable=SC2086
     "$F3" cases cases/stdlib --only $only -j 4 2>&1 | grep -E "FAIL|HEADER" | cut -c1-200 ;;
esac
echo "jit-selfhosted: $bad failure(s)"; exit $bad
