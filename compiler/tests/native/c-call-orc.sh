#!/bin/bash
# package C's test of native.call over ORC, with no liblair: builds compiler/tests/native/c-call-orc.fib linked with libLLVM, makes the LLVM IR of the
# shim, the mailbox and the modules under test with `lair emit-llvm` (the Rust lowering, until fibber's own exists), runs the checks of c-checks.fib.
# usage: c-call-orc.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; LAIR the Rust `lair`, default target/debug/lair of the repository or
#                                       `lair` on the PATH; LLVM_CONFIG llvm-config, default llvm-config-21; FIB_LIB the library)
# Exit: 0 every check ok; 1 a check or a step failed; 2 a tool is missing.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "c-call-orc: no fibc: set FIBC" >&2; exit 2; }
lair=${LAIR:-$root/target/debug/lair}
[ -x "$lair" ] || lair=$(command -v lair) || { echo "c-call-orc: no lair: set LAIR" >&2; exit 2; }
lc=${LLVM_CONFIG:-$(command -v llvm-config-21 || echo /usr/bin/llvm-config-21)}
libdir=$("$lc" --libdir) || exit 2
work=${1:-${TMPDIR:-/tmp}/c-call-orc-$$}
mkdir -p "$work"
export FIB_LIB=${FIB_LIB:-$root/lib}
cd "$root" || exit 2
"$fibc" build compiler/tests/native/c-call-orc.fib -I compiler -I compiler/tests/native -I lib -L "$libdir" -l LLVM-21 -o "$work/c-call-orc" || { echo "c-call-orc: FAILED to build" >&2; exit 1; }
for n in shim mailbox module hooked; do
  "$work/c-call-orc" print $n > "$work/$n.lir" || exit 1
  "$lair" emit-llvm "$work/$n.lir" > "$work/$n.ll" || { echo "c-call-orc: lair emit-llvm failed on $n" >&2; exit 1; }
done
"$work/c-call-orc" all "$work/c-call-orc" "$work"
