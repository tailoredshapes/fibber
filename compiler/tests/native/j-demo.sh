#!/bin/bash
# Package J's gate 2: compiler/jit-demo.fib's `basic` mode with native.api instead of lair.jit (and native.call instead of lair.call, its host the DEFAULT one,
# made through native.api): builds the demo as it is (over liblair.so) and with its `lair.jit` and `lair.call` requires swapped, runs both, and checks (1) the two
# outputs are the same line for line, (2) the hook part of the native one is the golden of crates/fibc/tests/capi/basic.rs (c-jit-demo.hooks.golden), (3) the
# executables the demo builds run with the statuses 42 and 7. `lair.fibm` and `lair.expand` of the demo stay over liblair.so (they are not J's), so the native
# build links liblair.so and libLLVM-21.so both.
# usage: j-demo.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; LAIR_DIR the directory of liblair.so, default the lib/ next to it; LLVM_LIBDIR the directory of
#                                  libLLVM-21.so; FIB_LIB the library, default the tree's lib/)
# Exit: 0 all three hold; 1 one does not; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "j-demo: no fibc: set FIBC" >&2; exit 2; }
lairdir=${LAIR_DIR:-$(cd "$(dirname "$(command -v "$fibc" || echo "$fibc")")/../lib" && pwd)}
llvmdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
work=${1:-${TMPDIR:-/tmp}/j-demo-$$}
mkdir -p "$work/a" "$work/b"
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$lairdir:$llvmdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
sed -e 's/\[lair\.jit :as jit\] \[lair\.call :as call\]/[native.api :as jit] [native.call :as call]/' compiler/jit-demo.fib > "$work/jit-demo-native.fib"
grep -q 'native.api :as jit' "$work/jit-demo-native.fib" || { echo "j-demo: the requires to swap are not in jit-demo.fib" >&2; exit 1; }
# lair.fibm and lair.expand take a session and a mailbox of the types of lair.jit and lair.call: shadow copies of them with the same swap, first on the path
mkdir -p "$work/shadow/lair"
for m in fibm expand; do
  sed -e 's/\[lair\.jit :as jit\]/[native.api :as jit]/' -e 's/\[lair\.call :as call\]/[native.call :as call]/' compiler/lair/$m.fib > "$work/shadow/lair/$m.fib"
  grep -q 'native\.' "$work/shadow/lair/$m.fib" || { echo "j-demo: lair/$m.fib has no require to swap" >&2; exit 1; }
done
"$fibc" build compiler/jit-demo.fib -I compiler -I compiler/tests/native -I lib -L "$lairdir" -l lair -o "$work/demo-lair" || exit 1
"$fibc" build "$work/jit-demo-native.fib" -I "$work/shadow" -I compiler -I compiler/tests/native -I lib -L "$lairdir" -l lair -L "$llvmdir" -l LLVM-21 -o "$work/demo-native" || exit 1
"$work/demo-lair" basic "$work/a" | sed "s|$work/a|DIR|g" > "$work/lair.out"
"$work/demo-native" basic "$work/b" | sed "s|$work/b|DIR|g" > "$work/native.out"
fail=0
if cmp -s "$work/lair.out" "$work/native.out"; then echo "same: $(wc -l < "$work/native.out") lines, native.api and lair.jit"; else
  echo "DIFFERENT:"; diff "$work/lair.out" "$work/native.out" | head -20; fail=1; fi
tail -n "$(wc -l < "$here/c-jit-demo.hooks.golden")" "$work/native.out" > "$work/hooks.out"
if cmp -s "$work/hooks.out" "$here/c-jit-demo.hooks.golden"; then echo "golden: the hook part is EXPECTED_HOOKS"; else
  echo "GOLDEN DIFFERS:"; diff "$here/c-jit-demo.hooks.golden" "$work/hooks.out" | head -20; fail=1; fi
for e in "hello 42" "exit7 7"; do
  set -- $e; "$work/b/$1" >/dev/null 2>&1; st=$?
  if [ "$st" = "$2" ]; then echo "executable $1: status $st"; else echo "executable $1: status $st, want $2"; fail=1; fi
done
exit $fail
