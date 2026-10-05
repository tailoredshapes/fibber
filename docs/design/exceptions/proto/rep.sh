#!/bin/bash
# usage: rep.sh : 7 runs of the happy path of every built binary, median and min ns
cd /home/tmarsh/.cache/fibber-scratch/EXC/a
for alloc in 1 0; do
  for mode in plain invoke result flag jmp; do
    n=chain_${mode}_$alloc
    v=$(for i in 1 2 3 4 5 6 7; do ./$n 20000000 10 | awk '/happy/ {print $2}'; done | sort -n | tr '\n' ' ')
    echo "alloc=$alloc $mode: sorted ns/call: $v"
  done
done
