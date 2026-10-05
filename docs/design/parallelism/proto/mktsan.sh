#!/bin/bash
# mktsan.sh PROG.fib OUT [patch]: build PROG with ThreadSanitizer through emit -> lairf emit-llvm -> opt tsan -> llc -> gcc
# patch = make the fib.mt flag atomic (monotonic) so its (benign) race does not hide the others
set -e
P=/home/tmarsh/.cache/fibber-scratch/par
src=$1; out=$2
$P/fb emit $src > $out.lir
/home/tmarsh/.cache/fibber-scratch/SIMD2-P0/lairf emit-llvm $out.lir > $out.ll
if [ "${3:-}" = patch ]; then
  sed -i -E 's/^(\s*%[0-9a-z.]+ = )load i32, ptr @fib\.mt, align 4/\1load atomic i32, ptr @fib.mt monotonic, align 4/; s/^(\s*)store i32 1, ptr @fib\.mt, align 4/\1store atomic i32 1, ptr @fib.mt monotonic, align 4/' $out.ll
fi
sed -E 's/^(define [^{]*) \{$/\1 sanitize_thread {/' $out.ll > $out.t.ll
/usr/lib/llvm-21/bin/opt -passes=tsan $out.t.ll -S -o $out.tsan.ll
/usr/lib/llvm-21/bin/llc -O1 -relocation-model=pic -filetype=obj $out.tsan.ll -o $out.tsan.o
gcc -fsanitize=thread $out.tsan.o -o $out -lm
