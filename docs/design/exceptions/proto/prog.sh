#!/bin/bash
# usage: prog.sh NAME : builds scripts/bench/NAME.fib as plain and as invoke variants (same opt/llc pipeline), prints sizes and times
ulimit -v 16000000
export FIB_LIB=/home/tmarsh/.cache/fibber-scratch/EXC/W/lib
cd /home/tmarsh/.cache/fibber-scratch/EXC
mkdir -p p; cd p
L=/usr/lib/llvm-21/bin
n=$1
../F emit ../W/scripts/bench/$n.fib > $n.lir || exit 1
../lairf emit-llvm $n.lir > $n.ll || exit 1
python3 ../a/xform.py $n.ll $n.inv.ll || exit 1
echo 'void fib_cleanup_stub(void) __attribute__((noinline)); void fib_cleanup_stub(void) { __asm__ volatile(""); }' > stub.c
gcc -O2 -c stub.c -o stub.o
for v in $n $n.inv; do
  $L/opt -passes='default<O2>' $v.ll -S -o $v.opt.ll || exit 1
  $L/llc -O2 -relocation-model=pic -filetype=obj $v.opt.ll -o $v.o || exit 1
  g++ $v.o stub.o -o $v.bin -lm -lpthread || exit 1
  echo "$v: $($L/llvm-size -A $v.o | awk '/^\.text/ {t+=$2} /eh_frame/ {e=$2} /gcc_except_table/ {g=$2} END {printf "text=%d eh_frame=%d except_table=%d", t, e, g}')"
done
for r in 1 2 3 4 5; do
  for v in $n $n.inv; do
    /usr/bin/time -f "$v %U" ./$v.bin > /dev/null 2>> times.$n.txt
  done
done
for v in $n $n.inv; do echo "$v user s sorted: $(grep "^$v " times.$n.txt | awk '{print $2}' | sort -n | tr '\n' ' ')"; done
rm -f times.$n.txt
