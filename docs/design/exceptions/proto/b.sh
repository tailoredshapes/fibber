#!/bin/bash
# usage: b.sh DIR/NAME  -> builds DIR/NAME.fib to DIR/NAME with the scratch F
ulimit -v 16000000
export FIB_LIB=/home/tmarsh/.cache/fibber-scratch/EXC/W/lib
cd /home/tmarsh/.cache/fibber-scratch/EXC
for n in "$@"; do
  ./F build "$n.fib" -o "$n" 2>&1 | head -8
done
