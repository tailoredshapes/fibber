#!/bin/bash
# compiler/jit-demo.fib's `basic` mode through native.call instead of lair.call (package C's gate): builds the demo as it is and with its
# `lair.call` require changed to `native.call`, runs both, and checks (1) the two outputs are the same line for line, (2) the hook part of the
# native one is the golden of crates/fibc/tests/capi/basic.rs (EXPECTED_HOOKS, c-jit-demo.hooks.golden), (3) the executables the demo builds
# run with the statuses 42 and 7. The sessions are still lair.jit's (liblair.so) until native.api lands; what changes is who calls the
# addresses and runs the mailbox.
# usage: c-jit-demo.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; LAIR_DIR the directory of liblair.so, default the lib/ next to it;
#                                       FIB_LIB the library, default the tree's lib/)
# Exit: 0 all three hold; 1 one does not; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "c-jit-demo: no fibc: set FIBC" >&2; exit 2; }
lairdir=${LAIR_DIR:-$(cd "$(dirname "$(command -v "$fibc" || echo "$fibc")")/../lib" && pwd)}
work=${1:-${TMPDIR:-/tmp}/c-jit-demo-$$}
mkdir -p "$work/a" "$work/b"
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$lairdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
sed 's/\[lair\.call :as call\]/[native.call :as call]/' compiler/jit-demo.fib > "$work/jit-demo-native.fib"
grep -q 'native.call :as call' "$work/jit-demo-native.fib" || { echo "c-jit-demo: the require to swap is not in jit-demo.fib" >&2; exit 1; }
"$fibc" build compiler/jit-demo.fib -I compiler -I lib -L "$lairdir" -l lair -o "$work/demo-lair" || exit 1
"$fibc" build "$work/jit-demo-native.fib" -I compiler -I lib -L "$lairdir" -l lair -o "$work/demo-native" || exit 1
"$work/demo-lair" basic "$work/a" | sed "s|$work/a|DIR|g" > "$work/lair.out"
"$work/demo-native" basic "$work/b" | sed "s|$work/b|DIR|g" > "$work/native.out"
fail=0
if cmp -s "$work/lair.out" "$work/native.out"; then echo "same: $(wc -l < "$work/native.out") lines, native.call and lair.call"; else
  echo "DIFFERENT:"; diff "$work/lair.out" "$work/native.out" | head -20; fail=1; fi
tail -n "$(wc -l < "$here/c-jit-demo.hooks.golden")" "$work/native.out" > "$work/hooks.out"
if cmp -s "$work/hooks.out" "$here/c-jit-demo.hooks.golden"; then echo "golden: the hook part is EXPECTED_HOOKS"; else
  echo "GOLDEN DIFFERS:"; diff "$here/c-jit-demo.hooks.golden" "$work/hooks.out" | head -20; fail=1; fi
for e in "hello 42" "exit7 7"; do
  set -- $e; "$work/b/$1" >/dev/null 2>&1; st=$?
  if [ "$st" = "$2" ]; then echo "executable $1: status $st"; else echo "executable $1: status $st, want $2"; fail=1; fi
done
exit $fail
