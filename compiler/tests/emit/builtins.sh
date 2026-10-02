#!/bin/bash
# The judge of the builtins (package E8, emit.lower.builtins): for each compiler/tests/emit/builtin-NAME.fib (one tiny program per
# arm of Rust's `Cx::builtin`, and a few variants that take another branch of an arm: a counted element, a unit element, a dyn), the
# functions `fibc emit-dump --sections fns` (the Rust, the oracle) prints against what the emitter tool compiler/emit.fib prints,
# byte for byte. A file passes only when (1) the Rust accepts and lowers it (else the test would compare two refusals), (2) the
# two texts are equal and (3) the text holds a mark of the arm (so a program that stopped reaching its builtin fails instead of
# passing on an unrelated lowering). The first difference names the arm: the file is the arm. A tool that reports a stub reached
# (`todo: PACKAGE NAME`, exit 70, or a trap, exit 134) is PENDING, never a pass. Run from the repository root with FIB_LIB set.
# usage: builtins.sh FIBC TOOL [FILE..]    (TOOL: `fibc build compiler/emit.fib -I compiler -I lib -o TOOL`)
FIBC=${1:?fibc}; TOOL=${2:?tool}; shift 2
here=$(dirname "$0")
[ $# -eq 0 ] && set -- "$here"/builtin-*.fib
same=0; diff=0; pending=0; bad=0
T=${TMPDIR:-$HOME/.cache/fibber-scratch}/builtins-sh.$$; mkdir -p "$T"

# The text an arm must leave in the functions (grep -F), by file name without `builtin-` and `.fib`.
mark() {
  case "$1" in
    alloc) echo '@fib.alloc-check';; free) echo '(call @free';; args) echo '@fib.args-count';;
    array) echo '@fib.array-alloc';; array-counted) echo 'fillr';; array-len) echo '(load i64';;
    array-get) echo 'oob';; array-get-counted) echo '@fib.retain';; array-get-unit) echo '@fib.trap-index';;
    array-with) echo '@fib.array-slice';; array-with-counted) echo '@fib.release';;
    array-copy) echo '@fib.array-slice';; array-copy-counted) echo '@trace.';;
    array-set|array-set-counted) echo '@fib.unique?';;
    atom|reset|swap) echo '%struct.fib.atom';; cell|set) echo '%struct.fib.cell';;
    bits-f32|bits-f64|f32-bits|f64-bits) echo 'bitcast';; char-to-i32) echo 'zext';; i32-to-char) echo '@fib.i32-to-char';;
    gensym) echo '@fibm.gensym-hook';; struct-p|enum-p) echo '.v6';;
    struct-fields|struct-params|struct-field-types|enum-params|enum-variants) echo '.v9';;
    spawn|join) echo '@fib.spawn';; not) echo '(xor';; trap) echo '@fib.trap';;
    weak|weak-dyn) echo '@fib.weak';; raw) echo '(load i64';; raw-dyn) echo '(extractvalue';;
    raw-retained|release-raw) echo '@fib.release';; ptr-plus) echo '(getelementptr i8';;
    load-i8) echo '(load i8';; load-i16) echo '(load i16';; load-i32) echo '(load i32';; load-i64) echo '(load i64';;
    load-ptr) echo '(load ptr';; store-*) echo '(store ';;
    read-file) echo '@fib.read-file';; write-file) echo '@fib.write-file';; starts-with) echo '@fib.str-starts-with';;
    str-from-bytes) echo '@fib.str-from-array';; str-*) echo "@fib.$1";;
    *) echo '';;
  esac
}

for f in "$@"; do
  n=$(basename "$f" .fib); n=${n#builtin-}
  timeout 120 "$FIBC" emit-dump --sections fns "$f" > "$T/rust" 2> "$T/rust.err"; rc=$?
  if [ $rc -ne 0 ] || ! grep -q '^(define' "$T/rust"; then bad=$((bad+1)); echo "BAD $n: the Rust does not lower it (exit $rc)"; continue; fi
  (ulimit -c 0 -v 4000000; timeout 120 "$TOOL" --sections fns "$f" > "$T/fib" 2> "$T/fib.err"); rc=$?
  if [ $rc -eq 70 ] || [ $rc -eq 134 ]; then pending=$((pending+1)); echo "pending $n: $(grep -h -o 'todo: [A-Za-z0-9]* [a-z0-9-]*' "$T/fib" "$T/fib.err" | head -1)"; continue; fi
  m=$(mark "$n")
  if ! cmp -s "$T/rust" "$T/fib"; then diff=$((diff+1)); echo "DIFFERENT $n"; diff "$T/rust" "$T/fib" | head -6
  elif [ -n "$m" ] && ! grep -qF -- "$m" "$T/fib"; then bad=$((bad+1)); echo "BAD $n: the text has no '$m'"
  else same=$((same+1)); fi
done
rm -rf "$T"
echo "same $same, different $diff, pending $pending, bad $bad"
[ $diff -eq 0 ] && [ $pending -eq 0 ] && [ $bad -eq 0 ]
