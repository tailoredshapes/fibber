#!/bin/bash
cd /home/tmarsh/.cache/fibber-scratch/EXC/a
ssh -o ConnectTimeout=10 "${FIB_MAC_HOST:?set FIB_MAC_HOST}" 'mkdir -p ~/fibber-a64-scratch/exc' || exit 1
scp -q jit.ll $FIB_MAC_HOST:fibber-a64-scratch/exc/jit.ll
ssh "$FIB_MAC_HOST" 'cd ~/fibber-a64-scratch/exc && uname -m && L=/opt/homebrew/opt/llvm@21/bin && $L/lli --version | head -2 &&
 for k in orc mcjit; do echo "-- lli --jit-kind=$k"; $L/lli -jit-kind=$k -load=/usr/lib/libc++.1.dylib jit.ll 2>&1 | head -5; echo "exit=${PIPESTATUS[0]}"; done;
 echo "-- AOT: llc + cc"; $L/llc -filetype=obj jit.ll -o jit.o && c++ jit.o -o jit.bin && ./jit.bin; echo "AOT exit=$?"; rm -f jit.o jit.bin'
