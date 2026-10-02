#!/bin/bash
# The judge of package E6 (objects, cells, dynamic dispatch) on its edge programs, compiler/tests/emit/{objects,cells,dyn}-*.fib: the
# types, statics and functions `fibc emit-dump` (the Rust, the oracle) prints against what the fibber tool prints, byte for byte. A
# program whose lowering reaches a stub of another package is PENDING (the tool says `todo:`), never a pass; the exit status is 0 only
# when every file is the same. Run from the repository root.
# usage: objects.sh FIBC TOOL [FILE..]     (TOOL: `fibc build compiler/emit.fib -I compiler -I lib -o TOOL`; FIB_LIB=lib)
FIBC=${1:?fibc}; TOOL=${2:?tool}; shift 2
files=("$@"); [ ${#files[@]} -eq 0 ] && files=(compiler/tests/emit/objects-*.fib compiler/tests/emit/cells-*.fib compiler/tests/emit/dyn-*.fib)
export CMP_OUT=${TMPDIR:-$HOME/.cache/fibber-scratch}/objects-sh.$$
out=$(compiler/tests/types/compare.sh -j 4 -c emit-dump -o "--sections types,statics,fns" "$FIBC" "$TOOL" "${files[@]}")
echo "$out"; compiler/tests/emit/causes.sh "$CMP_OUT"; rm -rf "$CMP_OUT"
echo "$out" | head -1 | grep -q "different 0$"
