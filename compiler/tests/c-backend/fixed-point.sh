#!/bin/bash
# The fixed point through C (docs/design/lir2c.md section 4): F builds the compiler through the C route into F_c, and `F_c emit
# compiler/fibc.fib` must equal `F emit compiler/fibc.fib` (the LLVM-built F's): byte for byte, the strongest test of the printer.
#   compiler/tests/c-backend/fixed-point.sh --fibc F [--cc CC] [-O N] OUTDIR
# Environment: LLVM_LIB (default /usr/lib/llvm-21/lib). Prints the sizes and times; exit 0 when the two emits are identical.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
fibc=""; cc=${FIB_CC:-gcc}; opt=2; out=""
while [ $# -gt 0 ]; do
  case $1 in --fibc) fibc=$2; shift ;; --cc) cc=$2; shift ;; -O) opt=$2; shift ;; *) out=$1 ;; esac; shift
done
: "${fibc:?--fibc F}" "${out:?OUTDIR}"; mkdir -p "$out"; llvm=${LLVM_LIB:-/usr/lib/llvm-21/lib}
cd "$root" || exit 2
export FIB_LIB=$root/lib TMPDIR=$out FIB_CC_KEEP=1
t0=$(date +%s)
"$fibc" --via c --cc "$cc" build compiler/fibc.fib -I compiler -I lib -O "$opt" -L "$llvm" -l LLVM-21 -o "$out/F_c" > "$out/build.log" 2>&1 || { echo "FAIL: F_c did not build:"; tail -20 "$out/build.log"; exit 1; }
t1=$(date +%s)
echo "F_c built through $cc -O$opt in $((t1 - t0)) s: $(wc -c < "$out/F_c.c") bytes of C, $(wc -l < "$out/F_c.c") lines; F_c is $(wc -c < "$out/F_c") bytes (F: $(wc -c < "$fibc"))"
"$fibc" emit -I compiler -I lib compiler/fibc.fib > "$out/emit.F" 2> "$out/emit.F.err" || { echo "FAIL: F emit failed"; exit 1; }
t2=$(date +%s)
"$out/F_c" emit -I compiler -I lib compiler/fibc.fib > "$out/emit.F_c" 2> "$out/emit.F_c.err" || { echo "FAIL: F_c emit failed:"; tail -5 "$out/emit.F_c.err"; exit 1; }
t3=$(date +%s)
echo "F emit: $((t2 - t1)) s, $(wc -c < "$out/emit.F") bytes; F_c emit: $((t3 - t2)) s, $(wc -c < "$out/emit.F_c") bytes"
if cmp -s "$out/emit.F" "$out/emit.F_c"; then echo "OK: F_c emit compiler/fibc.fib == F emit compiler/fibc.fib ($(sha1sum < "$out/emit.F" | cut -c1-16))"; exit 0
else echo "FAIL: the emits differ:"; cmp "$out/emit.F" "$out/emit.F_c"; diff "$out/emit.F" "$out/emit.F_c" | head -10; exit 1; fi
