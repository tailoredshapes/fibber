#!/bin/sh
# usage: run.sh file.lir  -> verify, compile, run
f=$1; b=${f%.lir}
export RUST_BACKTRACE=0; L=/home/user/liar/target/release/lair
echo "== $f"
timeout 60 $L --verify "$f" -o "$b.exe" 2>&1 | head -20; rc=$?
if [ -x "$b.exe" ] && [ "$b.exe" -nt "$f" ]; then timeout 10 "./$b.exe"; echo "[exit $?]"; fi
