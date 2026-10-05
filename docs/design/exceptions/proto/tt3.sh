#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/EXC/c
bash proto.sh tt3 > /dev/null 2>&1
/usr/bin/time -f 'proto: 20000 trapping tasks: %e s wall, %U user, %S sys, maxrss %M KB' ./tt3.proto > /dev/null 2> tt3.err
tail -1 tt3.err
FIB_TRACE=1 ./tt3.proto 2> tt3.trace > /dev/null
echo "A lines $(grep -c '^A ' tt3.trace)  F lines $(grep -c '^F ' tt3.trace)"
