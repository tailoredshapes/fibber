#!/bin/bash
# bld.sh OUTNAME SRC.fib CPU BITS [-I dir..]: a static binary of SRC for CPU at vector width BITS (BITS 0: the CPU's own width)
S=/home/tmarsh/.cache/fibber-scratch/avx512
T=/tank/repos/tailoredshapes/fibber/.claude/worktrees/agent-aaea0e0e4c75e9c11
export LD_LIBRARY_PATH=/usr/lib/llvm-21/lib FIB_LIB=${LIBDIR:-$T/lib}
[ "${CASEKIND:-}" = 1 ] || export FIB_EMIT_EXE=1
ulimit -v 16000000
out=$1; src=$2; cpu=$3; bits=$4; shift 4
mkdir -p $S/obj $S/bin
export FIB_TARGET_CPU=$cpu
[ "$bits" != 0 ] && export FIB_VECTOR_BITS=$bits
${F:-$S/s2} emit -I ${LIBDIR:-$T/lib} "$@" $src > $S/obj/$out.lir 2> $S/obj/$out.err || { echo "emit failed $out"; head -5 $S/obj/$out.err; exit 1; }
[ "${P512:-}" = 1 ] && sed -i "1s/.*/(target (cpu \"$cpu\") (features \"-prefer-256-bit\"))/" $S/obj/$out.lir
${L:-$S/L} build $S/obj/$out.lir -o $S/obj/$out.o -O 2 --emit obj 2>> $S/obj/$out.err || { echo "lair failed $out"; head -5 $S/obj/$out.err; exit 1; }
gcc -static $S/obj/$out.o -o $S/bin/$out -lm -lpthread 2>> $S/obj/$out.err || { echo "link failed $out"; head -5 $S/obj/$out.err; exit 1; }
echo "built $out ($cpu bits=$bits): $(head -1 $S/obj/$out.lir)"
