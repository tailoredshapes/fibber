#!/bin/bash
# emits lIR + LLVM IR + remarks for the shootout kernels
W=/tank/repos/tailoredshapes/fibber/.claude/worktrees/agent-a3c741e971f6bdfc6
S=/home/tmarsh/.cache/fibber-scratch/SIMD-V
cd $W
mkdir -p $S/k/sh
for b in mandelbrot spectral-norm n-body fannkuch-redux; do
  FIB_LIB=$W/lib $S/seed/fibc-0.1.5-linux-x86_64/bin/fibc emit scripts/shootout/$b/$b.fib -I compiler -I lib > $S/k/sh/$b.lir || { echo "emit $b failed"; continue; }
  $S/lairf emit-llvm $S/k/sh/$b.lir > $S/k/sh/$b.ll
  bash $S/k/opt.sh sh/$b.ll sh_$b
  echo "== $b remarks (user functions)"
  grep -v "fib\.\(core\|prelude\)" $S/k/o/sh_$b.remarks | cut -c1-260
done
