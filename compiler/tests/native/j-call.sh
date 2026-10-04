#!/bin/bash
# Package J's gate 3: package C's checks (c-call.fib over a JIT session, c-call-orc.fib over ORC) with the DEFAULT host of native.call: no host-install,
# no liblair. c-call.fib is built with `lair.jit` changed to `native.api` and its call to the c-lair-host dropped; c-call-orc.fib with the lines that install a
# host over its own ORC session dropped (the default host is then made by native.call through native.api on the first call). The sessions of the modules
# under test are native.api's in the first, ORC's over the Rust lowering's LLVM text in the second.
# usage: j-call.sh [WORKDIR]        (FIBC names the fibc, default `fibc`; LAIR the Rust `lair`, default target/debug/lair of the repository; FIB_LIB the library;
#                                   LLVM_LIBDIR the directory of libLLVM-21.so)
# Exit: 0 both pass; 1 a check or a step failed; 2 a tool is missing.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
command -v "$fibc" >/dev/null 2>&1 || [ -x "$fibc" ] || { echo "j-call: no fibc: set FIBC" >&2; exit 2; }
lair=${LAIR:-/tank/repos/tailoredshapes/fibber/target/debug/lair}
[ -x "$lair" ] || lair=$(command -v lair) || { echo "j-call: no lair: set LAIR" >&2; exit 2; }
libdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
work=${1:-${TMPDIR:-/tmp}/j-call-$$}
mkdir -p "$work"
export FIB_LIB=${FIB_LIB:-$root/lib}
export LD_LIBRARY_PATH=$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
fail=0
# 1: c-call.fib over native.api
sed -e 's/\[lair\.jit :as jit\] \[c-lair-host :as host\]/[native.api :as jit]/' compiler/tests/native/c-call.fib > "$work/j-call.fib"
sed -i -e '/^(defun main () -> i64$/,/^    ((Ok _) (run))))$/c\' -e '(defun main () -> i64 (run))' "$work/j-call.fib"
grep -q 'native.api :as jit' "$work/j-call.fib" && grep -q '^(defun main () -> i64 (run))' "$work/j-call.fib" || { echo "j-call: c-call.fib changed shape" >&2; exit 1; }
"$fibc" build "$work/j-call.fib" -I compiler -I compiler/tests/native -I lib -L "$libdir" -l LLVM-21 -o "$work/j-call" || { echo "j-call: FAILED to build c-call" >&2; exit 1; }
"$work/j-call" all "$work/j-call" > "$work/call.out"; rc=$?
echo "c-call over native.api, default host: exit $rc, $(grep -c '^ok ' "$work/call.out") ok, $(grep -c '^FAIL' "$work/call.out") FAIL"
[ "$rc" = 0 ] || { grep -v '^ok ' "$work/call.out" | head; fail=1; }
# 2: c-call-orc.fib with no host-install
grep -v -e '(h (session \[shim mailbox\] "host"))' -e '(i (call/host-install (lookup-result h)))' compiler/tests/native/c-call-orc.fib > "$work/j-call-orc.fib"
grep -q '(call/host-install' "$work/j-call-orc.fib" && { echo "j-call: c-call-orc.fib changed shape" >&2; exit 1; }
"$fibc" build "$work/j-call-orc.fib" -I compiler -I compiler/tests/native -I lib -L "$libdir" -l LLVM-21 -o "$work/j-call-orc" || { echo "j-call: FAILED to build c-call-orc" >&2; exit 1; }
for n in module hooked; do
  "$work/j-call-orc" print $n > "$work/$n.lir" || exit 1
  "$lair" emit-llvm "$work/$n.lir" > "$work/$n.ll" || { echo "j-call: lair emit-llvm failed on $n" >&2; exit 1; }
done
: > "$work/shim.ll"; : > "$work/mailbox.ll"
"$work/j-call-orc" all "$work/j-call-orc" "$work" > "$work/orc.out"; rc=$?
echo "c-call-orc, default host: exit $rc, $(grep -c '^ok ' "$work/orc.out") ok, $(grep -c '^FAIL' "$work/orc.out") FAIL"
[ "$rc" = 0 ] || { grep -v '^ok ' "$work/orc.out" | head; fail=1; }
exit $fail
