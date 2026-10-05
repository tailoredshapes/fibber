#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/EXC/a
ssh -o ConnectTimeout=10 192.168.7.254 'mkdir -p ~/fibber-a64-scratch/exc' || exit 1
scp -q jit.ll 192.168.7.254:fibber-a64-scratch/exc/jit.ll
ssh 192.168.7.254 'cd ~/fibber-a64-scratch/exc && uname -m && L=/opt/homebrew/opt/llvm@21/bin && $L/lli --version | head -2 &&
 for k in orc mcjit; do echo "-- lli --jit-kind=$k"; $L/lli -jit-kind=$k -load=/usr/lib/libc++.1.dylib jit.ll 2>&1 | head -5; echo "exit=${PIPESTATUS[0]}"; done;
 echo "-- AOT: llc + cc"; $L/llc -filetype=obj jit.ll -o jit.o && c++ jit.o -o jit.bin && ./jit.bin; echo "AOT exit=$?"; rm -f jit.o jit.bin'
