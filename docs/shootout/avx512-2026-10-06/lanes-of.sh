#!/bin/bash
# lanes-of.sh COMPILER CPU..: what (native-lanes f64) and (native-lanes f32) are for each CPU name (emit: the lane counts show in the lIR)
S=/home/tmarsh/.cache/fibber-scratch/avx512
T=/tank/repos/tailoredshapes/fibber/.claude/worktrees/agent-aaea0e0e4c75e9c11
export LD_LIBRARY_PATH=/usr/lib/llvm-21/lib FIB_LIB=$T/lib FIB_EMIT_EXE=1
cd $T
c=$1; shift
for cpu in "$@"; do
  echo "$cpu: $(FIB_TARGET_CPU=$cpu $c emit $S/t4.fib | grep -o '<[0-9]* x [a-z]*>' | sort -u | tr '\n' ' ')"
done
echo "host (unset): $(unset FIB_EMIT_EXE; $c run $S/t4.fib | tr '\n' ' ')"
echo "host, FIB_VECTOR_BITS=512: $(unset FIB_EMIT_EXE; FIB_VECTOR_BITS=512 $c run $S/t4.fib | tr '\n' ' ')"
