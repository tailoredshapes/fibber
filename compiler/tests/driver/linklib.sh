#!/bin/bash
# The native libraries a module declares (`(extern f .. :lib "NAME")`), linked only when reached and by the mode of the build (docs/design/static-linking.md 3).
#   1. a program that requires a module declaring a library it never calls links and runs, though the library exists nowhere (the planted fault: if the
#      declaration forced a link, this build fails);
#   2. a program that calls it fails with a message that names the module AND the library, dynamic and static;
#   3. a real library (built here from C with gcc: libfibtest.a and .so) links dynamically (-lfibtest, found with -L and an rpath), bundled (`--link fibtest=static`)
#      and in a fully static musl executable (`--static`, when the musl pieces are found), and the program gets 42 from it each time;
#   4. a mode that is not static or dynamic, and a dynamic library in a --static build, are refused with a message.
# usage: linklib.sh   F=stage-2 fibc (FIBC also read). Exit: 0 all hold; 1 a check failed; 2 no fibc.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}
command -v "$F" > /dev/null 2>&1 || [ -x "$F" ] || { echo "linklib: no fibc: set F" >&2; exit 2; }
export FIB_LIB=$root/lib
T=$(mktemp -d "${TMPDIR:-/tmp}/linklib.XXXXXX"); trap 'rm -rf "$T"' EXIT
D=compiler/tests/driver/linklib; bad=0
ok() { echo "ok   $1"; }
no() { echo "FAIL $1"; bad=1; }
check() { # check NAME COMMAND..: passes when the command succeeds
  local n=$1; shift; if "$@" > "$T/out" 2>&1; then ok "$n"; else no "$n"; sed 's/^/     /' "$T/out" | head -5; fi; }
fails_with() { # fails_with NAME TEXT.. -- COMMAND..: passes when the command fails and its output holds every TEXT
  local n=$1; shift; local texts=(); while [ "$1" != -- ]; do texts+=("$1"); shift; done; shift
  if "$@" > "$T/out" 2>&1; then no "$n (it succeeded)"; return; fi
  for t in "${texts[@]}"; do grep -qF -- "$t" "$T/out" || { no "$n (no '$t' in the message)"; sed 's/^/     /' "$T/out" | head -5; return; }; done; ok "$n"; }

check "1 a module that declares a missing library, required and not called, links and runs" \
  bash -c "'$F' build $D/requires-missing.fib -I $D -o $T/rm && $T/rm"
fails_with "2a calling it names the module and the library (dynamic)" "fakelib" "fibtest-missing" -- "$F" build $D/uses-missing.fib -I $D -o "$T/um"
fails_with "2b calling it names the module and the library (--link static)" "fakelib" "fibtest-missing" -- "$F" build --link fibtest-missing=static $D/uses-missing.fib -I $D -o "$T/um"
fails_with "4a a mode that is neither static nor dynamic" "static or dynamic" -- "$F" build --link-mode bogus $D/uses-missing.fib -I $D -o "$T/um"
fails_with "4b dynamic in a --static build" "no dynamic loader" -- "$F" build --static --link fibtest-missing=dynamic $D/uses-missing.fib -I $D -o "$T/um"

if command -v gcc > /dev/null 2>&1; then
  mkdir -p "$T/lib"; printf 'int fibtest_answer(void) { return 42; }\n' > "$T/lib/a.c"
  gcc -O1 -fPIC -c "$T/lib/a.c" -o "$T/lib/a.o" && ar rcs "$T/lib/libfibtest.a" "$T/lib/a.o" && gcc -shared -o "$T/lib/libfibtest.so" "$T/lib/a.o" || { echo "linklib: gcc failed"; exit 1; }
  check "3a a real library, dynamic: -lfibtest by the declaration" bash -c "'$F' build $D/uses-answer.fib -I $D -L $T/lib -o $T/d && $T/d && { ldd $T/d 2>/dev/null || otool -L $T/d; } | grep -q libfibtest"
  check "3b a real library bundled: --link fibtest=static has no dependency on it" bash -c "'$F' build --link fibtest=static $D/uses-answer.fib -I $D -L $T/lib -o $T/s && $T/s && ! { ldd $T/s 2>/dev/null || otool -L $T/s; } | grep -q libfibtest"
  check "3c --link-mode static does the same" bash -c "'$F' build --link-mode static $D/uses-answer.fib -I $D -L $T/lib -o $T/s2 && $T/s2"
  if "$F" build --static $D/requires-missing.fib -I $D -o "$T/probe" > "$T/out" 2>&1; then
    check "3d a real library in a fully static musl executable" bash -c "'$F' build --static $D/uses-answer.fib -I $D -L $T/lib -o $T/ss && $T/ss && ! ldd $T/ss 2>/dev/null | grep -q 'libc'" # linux-only: a fully static musl executable (3d is skipped where there are no musl pieces)
  else echo "skip 3d (no musl pieces: $(head -1 "$T/out" | cut -c1-100))"; fi
else echo "skip 3 (no gcc to build the test library)"; fi
[ $bad -eq 0 ] && echo "linklib: all checks hold" || echo "linklib: FAILED"; exit $bad
