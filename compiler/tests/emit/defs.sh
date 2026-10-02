#!/bin/bash
# The judge of the constant `def`s (package E10, emit.defs.*): for each compiler/tests/emit/def-NAME.fib (one tiny program per type
# shape of a def: scalar, str, struct, enum, array, Option, closure, quote and map), the sections `statics,defs,fns` of
# `fibc emit-dump` (the Rust, the oracle) against what the emitter tool compiler/emit.fib prints, byte for byte. A file passes only
# when (1) the Rust accepts and lowers it, (2) the two texts are equal and (3) the text holds every mark of the shape (so a
# program whose defs stopped being evaluated fails instead of passing on an empty text). The tool reads the values out of the
# JIT's memory, so it is built linked to liblair. A tool that reports a stub reached (`todo: PACKAGE NAME`, exit 70, or a trap,
# exit 134) is PENDING, never a pass. Run from the repository root with FIB_LIB set.
# usage: defs.sh FIBC TOOL [FILE..]
#   TOOL: `fibc build compiler/emit.fib -I compiler -I lib -L target/debug -l lair -o TOOL`
FIBC=${1:?fibc}; TOOL=${2:?tool}; shift 2
here=$(dirname "$0")
[ $# -eq 0 ] && set -- "$here"/def-*.fib
same=0; diff=0; pending=0; bad=0
T=${TMPDIR:-$HOME/.cache/fibber-scratch}/defs-sh.$$; mkdir -p "$T"

# What the text must hold (grep -F, each line a mark), by file name without `def-` and `.fib`.
marks() {
  case "$1" in
    scalar) printf '%s\n' '(float 0.10000000149011612)' '(i16 300)' '(i32 -70000)' '(i8 -3)' '(double 1.5e-7)' '(i64 9223372036854775807)';;
    str) printf '%s\n' '(i8 195)' '(i8 169)' '(i8 92)' '(i8 34)';;
    struct) printf '%s\n' '(constant internal def.1 %struct.o.Inner' '(float 0.5)' '(i1 1)' '(double 2.25)' '@str.0 @def.1';;
    enum) printf '%s\n' '%struct.o.Shape.v3' '%struct.o.Shape.v2' '%struct.o.Shape.v0' '%struct.o.Shape.v1';;
    array) printf '%s\n' '[4 x i64]' '[3 x i8]' '[3 x i1]' '[3 x double]' '(double 1e16)' '[3 x ptr]';;
    option) printf '%s\n' 'Option.i64_.v0' 'Option.i64_.v1' 'Option.$fib.builtin/Option.i64__.v1' '(ptr null)';;
    closure) printf '%s\n' '@clo.0' '@clo.1' '%struct.o.Op';;
    quote) printf '%s\n' 'Map.i64.str_.v1' '[3 x ptr]';;
    *) echo '';;
  esac
}

for f in "$@"; do
  n=$(basename "$f" .fib); n=${n#def-}
  timeout 120 "$FIBC" emit-dump --sections statics,defs,fns "$f" > "$T/rust" 2> "$T/rust.err"; rc=$?
  if [ $rc -ne 0 ] || ! grep -q '^(define' "$T/rust"; then bad=$((bad+1)); echo "BAD $n: the Rust does not lower it (exit $rc)"; continue; fi
  (ulimit -c 0 -v 4000000; timeout 120 "$TOOL" --sections statics,defs,fns "$f" > "$T/fib" 2> "$T/fib.err"); rc=$?
  if [ $rc -eq 70 ] || [ $rc -eq 134 ]; then pending=$((pending+1)); echo "pending $n: $(grep -h -o 'todo: [A-Za-z0-9]* [a-z0-9-]*' "$T/fib" "$T/fib.err" | head -1)"; continue; fi
  if ! cmp -s "$T/rust" "$T/fib"; then diff=$((diff+1)); echo "DIFFERENT $n"; diff "$T/rust" "$T/fib" | head -6 | cut -c1-200; continue; fi
  miss=""
  while IFS= read -r m; do
    [ -n "$m" ] && ! grep -qF -- "$m" "$T/fib" && miss="$miss '$m'"
  done < <(marks "$n")
  if [ -n "$miss" ]; then bad=$((bad+1)); echo "BAD $n: the text has none of$miss"; else same=$((same+1)); fi
done
rm -rf "$T"
echo "same $same, different $diff, pending $pending, bad $bad"
[ $diff -eq 0 ] && [ $pending -eq 0 ] && [ $bad -eq 0 ]
