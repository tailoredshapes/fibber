#!/bin/bash
# The pthread keys of the runtime (rt/core.lir fib.task-key, fib.exc-key): glibc's pthread_key_t is 4 bytes, Darwin's is an unsigned long (8), so on Darwin
# pthread_key_create wrote 4 bytes past each i32 global. emit.os (darwin-keys) gives the Darwin runtime i64 globals, loads and key parameters; Linux keeps i32.
#   1. the lIR for arm64-apple-macosx, arm64-apple-ios has no i32 key global, load or parameter, and has the i64 ones;
#   2. the lIR for x86_64-unknown-linux-gnu and aarch64-unknown-linux-gnu keeps the i32 ones;
#   3. planted fault: the Linux text read as if it were Darwin's check fails (the check can fail).
# usage: a64-keys.sh    F=stage-2 fibc (FIBC also read). Exit 0 all hold, 1 a check failed, 2 no tools. Executed behaviour is checked on the Mac by cases 7870-7891.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "a64-keys: no fibc: set F" >&2; exit 2; }
export FIB_LIB=$root/lib
T=$(mktemp -d "${TMPDIR:-/tmp}/a64-keys.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
prog=compiler/tests/native/a64/o0-callee-pop.fib
wide() { grep -q 'global internal fib.task-key i64' "$1" && grep -q 'global internal fib.exc-key i64' "$1" && grep -q 'declare pthread_getspecific ptr (i64)' "$1" \
  && grep -q 'declare pthread_setspecific i32 (i64 ptr)' "$1" && ! grep -q 'load i32 @fib\.\(task\|exc\)-key' "$1"; }
narrow() { grep -q 'global internal fib.task-key i32' "$1" && grep -q 'global internal fib.exc-key i32' "$1" && grep -q 'declare pthread_getspecific ptr (i32)' "$1" \
  && grep -q 'load i32 @fib.task-key' "$1" && ! grep -q 'i64 @fib\.\(task\|exc\)-key' "$1"; }
for t in arm64-apple-macosx13.0.0 arm64-apple-ios17.0 aarch64-apple-darwin; do
  "$F" emit --exe --target "$t" -I cases/stdlib/support "$prog" > "$T/d.lir" 2>/dev/null && wide "$T/d.lir" && ok "$t: i64 key globals, loads and parameters" || no "$t: keys not 8 bytes"
done
for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  "$F" emit --exe --target "$t" -I cases/stdlib/support "$prog" > "$T/l.lir" 2>/dev/null && narrow "$T/l.lir" && ok "$t: i32 keys" || no "$t: keys not 4 bytes"
done
wide "$T/l.lir" && no "planted fault: Linux text passed the Darwin check" || ok "planted fault caught: the Linux text fails the Darwin check"
exit $bad
