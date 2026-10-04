#!/bin/bash
# Builds compiler/tests/native/c-call.fib with a seed fibc and runs it: package C's test of native.call (the shim, the mixed-argument calls,
# the big stack, the traps and the mailbox). The JIT session of the module under test is lair's, through liblair.so, until native.api lands.
# usage: c-call.sh [OUT]        (from anywhere; FIBC names the fibc to build with, default `fibc`; LAIR_DIR the directory of liblair.so, default
#                               the lib/ next to FIBC's bin/; FIB_LIB the library, default the tree's lib/)
# Exit: 0 every check ok; 1 a check failed or the build failed; 2 no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "c-call: no fibc: set FIBC" >&2; exit 2; }
lairdir=${LAIR_DIR:-$(cd "$(dirname "$(command -v "$fibc" || echo "$fibc")")/../lib" && pwd)}
out=${1:-${TMPDIR:-/tmp}/c-call-$$}
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$lairdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
"$fibc" build compiler/tests/native/c-call.fib -I compiler -I compiler/tests/native -I lib -L "$lairdir" -l lair -o "$out" || { echo "c-call: FAILED to build" >&2; exit 1; }
"$out" all "$out"
