#!/bin/bash
# usage: build.sh IN.ll NAME  -> NAME (exe), NAME.opt.ll (optimised IR)
set -e
ulimit -v 16000000
L=/usr/lib/llvm-21/bin
cd /tank/data/fibber-scratch/lirfacts
timeout 600 $L/opt -S -passes="${PASSES:-default<O2>}" -mcpu=native "$1" -o "$2.opt.ll"
timeout 600 $L/llc -O2 -mcpu=native -filetype=obj -relocation-model=pic "$2.opt.ll" -o "$2.o"
cc "$2.o" -o "$2" -lz -l:libwebp.so.7 -lm -lpthread -ldl
