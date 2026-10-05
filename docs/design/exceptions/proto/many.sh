#!/bin/bash
for p in "$@"; do
  echo "### $p"
  bash /home/tmarsh/.cache/fibber-scratch/EXC/a/prog.sh $p 2>&1 | tail -8
done
