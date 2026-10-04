#!/bin/bash
# Writes the GNU ld script DIR/libllvm-static.so: LLVM 21's static archives (the components the compiler's native.* modules call through LLVM-C: core
# orcjit native passes analysis irreader bitwriter, in llvm-config's dependency order) in a GROUP, then the system libraries they need, so that
#   fibc build PROGRAM -L DIR -l llvm-static
# links LLVM into the program (as `-L /usr/lib/llvm-21/lib -l LLVM-21` links the shared libLLVM-21.so). `fibc build` passes only -L and -l words to
# `cc` and gives no --start-group, hence the GROUP of the script (ld re-scans it until nothing is undefined); libstdc++ is named in the script
# because LLVM is C++ and `cc` does not link it. Prints the words to give fibc: `-L DIR -l llvm-static`.
# usage: scripts/llvm-static.sh DIR        LLVM_CONFIG names llvm-config (default llvm-config-21, else /usr/bin/llvm-config-21)
# Used by scripts/lib/stage2.sh (LLVM_LINK=static), scripts/package.sh and compiler/tests/llvm/build.sh.
set -eu
dir=${1:?usage: llvm-static.sh DIR}
lc=${LLVM_CONFIG:-$(command -v llvm-config-21 || echo /usr/bin/llvm-config-21)}
[ -x "$lc" ] || { echo "llvm-static: no llvm-config-21 (set LLVM_CONFIG)" >&2; exit 2; }
mkdir -p "$dir"
archives=$("$lc" --link-static --libfiles core orcjit native passes analysis irreader bitwriter | tr ' ' '\n' | grep -v '^$' | tr '\n' ' ')
[ -n "$archives" ] || { echo "llvm-static: llvm-config listed no archives (is the static LLVM 21 installed?)" >&2; exit 2; }
for a in $archives; do [ -f "$a" ] || { echo "llvm-static: missing archive $a" >&2; exit 2; }; done
libs=$("$lc" --link-static --system-libs | tr '\n' ' ')
printf 'GROUP ( %s AS_NEEDED ( -lstdc++ %s ) )\n' "$archives" "$libs" > "$dir/libllvm-static.so"
echo "-L $dir -l llvm-static"
