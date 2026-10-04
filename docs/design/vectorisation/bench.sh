#!/bin/bash
# usage: bench.sh BENCH TAG FLAGS..  : strips flags in every f.* function of sh/BENCH.ll, optimises, links, runs the small size 3 times
# (md5 of stdout is printed), prints best elapsed ms and the number of vectorized loops. env FRE overrides the function regex.
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
b=$1; tag=$2; shift 2
NN=${NN:-}
case $b in mandelbrot) n=2000;; spectral-norm) n=1000;; n-body) n=500000;; fannkuch-redux) n=10;; esac
[ -n "$NN" ] && n=$NN
python3 strip.py ${SRC:-sh/$b.ll} o/${b}_$tag.in.ll "${FRE:-^f\.}" "$@" || exit 1
bash opt.sh o/${b}_$tag.in.ll ${b}_$tag ${OPTARGS} | sed "s/^/  /"
/usr/lib/llvm-21/bin/llc -O2 -mcpu=${CPU:-raptorlake} -filetype=obj -relocation-model=pic o/${b}_$tag.ll -o o/${b}_$tag.o || exit 1
cc o/${b}_$tag.o -o o/${b}_$tag.exe -lm -lpthread || exit 1
best=999999
for r in 1 2 3; do
  flock /tmp/fibsuite.lock /usr/bin/time -f %e -o o/${b}_$tag.t ./o/${b}_$tag.exe $n > o/${b}_$tag.out
  t=$(awk '{printf "%d", $1*1000}' o/${b}_$tag.t)
  [ $t -lt $best ] && best=$t
done
sum=$(md5sum < o/${b}_$tag.out | cut -c1-32)
echo "$b $tag n=$n md5=$sum best_ms=$best  (vectorized loops: $(grep -c '^Vectorized' o/${b}_$tag.remarks))"
