#!/bin/bash
# cltsan.sh: the lIR deque and its mutants under ThreadSanitizer (lairf emit-llvm -> sanitize_thread -> opt tsan -> llc -> gcc -fsanitize=thread)
cd /home/tmarsh/.cache/fibber-scratch/par/w
ulimit -v unlimited
for m in "" .m1 .m2 .m3 .m4; do
  /home/tmarsh/.cache/fibber-scratch/EXC/lairf emit-llvm cl$m.lir > clt$m.ll || exit 1
  sed -E 's/^(define [^{]*) \{$/\1 sanitize_thread {/' clt$m.ll > clt$m.s.ll
  /usr/lib/llvm-21/bin/opt -passes=tsan clt$m.s.ll -S -o clt$m.t.ll && /usr/lib/llvm-21/bin/llc -O1 -relocation-model=pic -filetype=obj clt$m.t.ll -o clt$m.o && gcc -fsanitize=thread clt$m.o -o clt$m.exe -lm || exit 1
  echo "== tsan cl$m: $(timeout 120 ./clt$m.exe 2>&1 | grep -E 'SUMMARY|^dup' | sed 's/(BuildId.*//; s#/home/[^ ]*/##' | sort | uniq -c | tr '\n' ';')"
done
