#!/bin/bash
# pvw.sh SRC.fib CPU..: zmm/ymm counts of the assembly for each CPU, with LLVM's default width and with -prefer-256-bit
S=/home/tmarsh/.cache/fibber-scratch/avx512
T=/tank/repos/tailoredshapes/fibber/.claude/worktrees/agent-aaea0e0e4c75e9c11
export LD_LIBRARY_PATH=/usr/lib/llvm-21/lib FIB_LIB=$T/lib FIB_EMIT_EXE=1
src=$1; shift
b=$(basename $src .fib)
for cpu in "$@"; do
  FIB_TARGET_CPU=$cpu $S/s2 emit -I $T/lib $src > $S/out/$b-$cpu.lir
  sed '1s/.*/(target (cpu "'$cpu'") (features "-prefer-256-bit"))/' $S/out/$b-$cpu.lir > $S/out/$b-$cpu-p512.lir
  for v in "" -p512; do
    FIB_TARGET_CPU=$cpu $S/L build $S/out/$b-$cpu$v.lir -o $S/out/$b-$cpu$v.s -O 2 --emit asm 2>&1 | head -2
    echo "$b $cpu$v: zmm=$(grep -c zmm $S/out/$b-$cpu$v.s) ymm=$(grep -c ymm $S/out/$b-$cpu$v.s)"
  done
done
