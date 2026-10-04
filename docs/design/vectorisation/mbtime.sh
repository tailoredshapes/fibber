#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
for prog in ./o/mandelbrot_base.exe ./mb_c_O2 ./mb_avx; do
  best=999999
  for r in 1 2 3; do
    flock /tmp/fibsuite.lock /usr/bin/time -f %e -o o/mb.t $prog 4000 > /dev/null
    t=$(awk '{printf "%d", $1*1000}' o/mb.t)
    [ $t -lt $best ] && best=$t
  done
  echo "mandelbrot N=4000 $prog best_ms=$best"
done
