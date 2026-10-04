#!/bin/bash
# usage: opt.sh IN.ll OUTPREFIX [extra opt args]   -> OUTPREFIX.ll (optimised), OUTPREFIX.yaml (remarks), OUTPREFIX.s (asm)
in=$1; out=$2; shift 2
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
CPU=${CPU:-raptorlake}
/usr/lib/llvm-21/bin/opt -S -passes="${PASSES:-default<O2>}" -mcpu=$CPU -pass-remarks-output=o/$out.yaml "$@" $in -o o/$out.ll || exit 1
/usr/lib/llvm-21/bin/llc -O2 -mcpu=$CPU o/$out.ll -o o/$out.s
python3 remarks.py o/$out.yaml > o/$out.remarks
echo "packed fp (ps/pd, ymm): $(grep -cE 'v(add|mul|sub|div|fmadd[0-9]+|sqrt)p[sd]|vfmadd[0-9]+p[sd]' o/$out.s)  scalar fp: $(grep -cE 'v?(add|mul|sub|div|fmadd[0-9]*|sqrt)s[sd]\b' o/$out.s)"
