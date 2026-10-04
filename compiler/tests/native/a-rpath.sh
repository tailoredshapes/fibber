#!/bin/bash
# The relocatable rpath (package A, gate 4): scripts/package.sh builds the shipped fibc with a `cc` shim that turns the absolute rpath of lair's link into
# `$ORIGIN/../lib`; the port must write the command the shim understands. This script takes the shim from scripts/package.sh itself (the text between
# `cat > "$work/shim/cc" <<'SHIM'` and `SHIM`), builds a program against a shared library with `lair build -L LIB -l NAME` and with `lairf build ...`, once
# plainly and once with the shim first on PATH, and shows for each tool: the RUNPATH of the plain build is the library's absolute directory, the RUNPATH of
# the shimmed build is `$ORIGIN/../lib`, the two tools write the same RUNPATH, and the shimmed executable run from a copied tree (bin/ and lib/) without
# LD_LIBRARY_PATH exits with the library's answer (42).
# usage: a-rpath.sh        run from the repository root: LAIR=target/debug/lair LAIRF=path/to/lairf      Exit: 0 all hold, 1 one failed, 2 usage
set -u
LAIR=${LAIR:-target/debug/lair}
[ -x "$LAIR" ] && [ -n "${LAIRF:-}" ] && [ -x "$LAIRF" ] || { echo "a-rpath: set LAIR and LAIRF to executables" >&2; exit 2; }
command -v readelf >/dev/null || { echo "a-rpath: no readelf" >&2; exit 2; }
export FIB_TARGET_CPU=${FIB_TARGET_CPU:-x86-64-v2}
T=$(mktemp -d "${TMPDIR:-$HOME/.cache/fibber-scratch}/a-rpath.XXXXXX")
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/lib" "$T/shim"
echo 'int lairtest_triple(int n) { return 3 * n; }' > "$T/triple.c"
cc -shared -fPIC -o "$T/lib/liblairtest.so" "$T/triple.c" || exit 2
cat > "$T/p.lir" <<'EOF'
(declare lairtest_triple i32 (i32))
(define (main i32) ()
  (block entry (ret (call @lairtest_triple (i32 14)))))
EOF
sed -n "/^cat > \"\$work\/shim\/cc\" <<'SHIM'\$/,/^SHIM\$/p" scripts/package.sh | sed '1d;$d' > "$T/shim/cc"
[ -s "$T/shim/cc" ] || { echo "a-rpath: could not extract the shim from scripts/package.sh" >&2; exit 2; }
chmod +x "$T/shim/cc"
REAL_CC=$(command -v cc); export REAL_CC

fail=0
check() { # NAME CONDITION-RESULT(0 holds)
  if [ "$2" = 0 ]; then echo "PASS $1"; else echo "FAIL $1"; fail=1; fi
}
runpath() { readelf -d "$1" | sed -n 's/.*RUNPATH.*\[\(.*\)\].*/\1/p'; }

for tool in rust fib; do
  bin=$LAIR; [ $tool = fib ] && bin=$LAIRF
  "$bin" build "$T/p.lir" -o "$T/plain.$tool" -O 2 -L "$T/lib" -l lairtest 2> "$T/plain.$tool.err"; check "$tool plain build" $?
  PATH="$T/shim:$PATH" "$bin" build "$T/p.lir" -o "$T/shim.$tool" -O 2 -L "$T/lib" -l lairtest 2> "$T/shim.$tool.err"; check "$tool shimmed build" $?
  [ "$(runpath "$T/plain.$tool")" = "$(cd "$T/lib" && pwd -P)" ]; check "$tool plain RUNPATH is the absolute library directory" $?
  [ "$(runpath "$T/shim.$tool")" = '$ORIGIN/../lib' ]; check "$tool shimmed RUNPATH is \$ORIGIN/../lib" $?
  env -u LD_LIBRARY_PATH "$T/plain.$tool"; [ $? = 42 ]; check "$tool plain executable finds the library without LD_LIBRARY_PATH (exit 42)" $?
  mkdir -p "$T/pkg.$tool/bin" "$T/pkg.$tool/lib"
  cp "$T/shim.$tool" "$T/pkg.$tool/bin/prog"; cp "$T/lib/liblairtest.so" "$T/pkg.$tool/lib/"
  env -u LD_LIBRARY_PATH "$T/pkg.$tool/bin/prog"; [ $? = 42 ]; check "$tool shimmed executable from a copied tree finds lib/ by \$ORIGIN (exit 42)" $?
done
[ "$(runpath "$T/shim.rust")" = "$(runpath "$T/shim.fib")" ]; check "the shimmed RUNPATH is the same in both tools" $?
[ "$(runpath "$T/plain.rust" | sed "s#$T#TMP#")" = "$(runpath "$T/plain.fib" | sed "s#$T#TMP#")" ]; check "the plain RUNPATH is the same in both tools" $?
cmp -s "$T/shim.rust" "$T/shim.fib"; check "the shimmed executables are byte-identical" $?
cmp -s "$T/plain.rust" "$T/plain.fib"; check "the plain executables are byte-identical" $?
if [ "$fail" = 0 ]; then echo "a-rpath: all hold"; else echo "a-rpath: FAILED"; fi
exit $fail
