#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
for b in spectral-norm n-body mandelbrot fannkuch-redux; do
  bash bench.sh $b base 2>&1 | grep -v "^  packed"
  bash bench.sh $b ovf ovf 2>&1 | grep -v "^  packed"
  bash bench.sh $b ovf_bounds ovf bounds 2>&1 | grep -v "^  packed"
  bash bench.sh $b reassoc reassoc 2>&1 | grep -v "^  packed"
  bash bench.sh $b all ovf bounds rc uniq reassoc noalias 2>&1 | grep -v "^  packed"
  bash bench.sh $b all_nr ovf bounds rc uniq noalias 2>&1 | grep -v "^  packed"
  PASSES='default<O3>' bash bench.sh $b O3 2>&1 | grep -v "^  packed"
  echo "-- $b packed/scalar fp in the binary: base $(grep -cE '\sv?(add|mul|sub|div)p[sd]|vfmadd[0-9]+p[sd]' o/${b}_base.s)/$(grep -cE '\sv?(add|mul|sub|div|fmadd[0-9]*)s[sd]' o/${b}_base.s)  all $(grep -cE '\sv?(add|mul|sub|div)p[sd]|vfmadd[0-9]+p[sd]' o/${b}_all.s)/$(grep -cE '\sv?(add|mul|sub|div|fmadd[0-9]*)s[sd]' o/${b}_all.s)"
done
