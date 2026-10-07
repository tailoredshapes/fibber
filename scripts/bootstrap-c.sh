#!/bin/bash
# scripts/bootstrap-c.sh: fibber on a machine with a C compiler and no fibber (docs/design/lir2c.md section 6). Given `fibc.c`, the compiler's
# own C (`fibc build --emit c compiler/fibc.fib -I compiler -I lib -o fibc.c` on a machine that has a fibber), it compiles the compiler with
# cc, then builds the tree's compiler with it (stage 2 of this tree) and checks the fixed point: the two compilers emit the same lIR for
# compiler/fibc.fib. The compiler links the LLVM library (its JIT runs macros, its `build` generates code), so the machine needs
# libLLVM (the shared library, e.g. Ubuntu's libllvm21: a library, not a compiler) beside cc: no clang, no seed binary.
#   scripts/bootstrap-c.sh FIBC_C OUTDIR [--cc CC] [--llvm-lib DIR] [--llvm-name NAME]
# Prints each step with its time; exit 0 when the fixed point holds.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/.." && pwd)
src=${1:?fibc.c}; out=${2:?OUTDIR}; shift 2
cc=cc; llvmlib=""; llvmname=""
while [ $# -gt 0 ]; do
  case $1 in --cc) cc=$2; shift ;; --llvm-lib) llvmlib=$2; shift ;; --llvm-name) llvmname=$2; shift ;; esac; shift
done
if [ -z "$llvmlib" ]; then for d in /usr/lib/llvm-21/lib /usr/lib/llvm21/lib /usr/lib/x86_64-linux-gnu /usr/lib/aarch64-linux-gnu /usr/lib; do
  [ -e "$d/libLLVM-21.so" ] || [ -e "$d/libLLVM.so.21.1" ] || [ -e "$d/libLLVM.so" ] && { llvmlib=$d; break; }; done; fi
if [ -z "$llvmname" ]; then if [ -e "$llvmlib/libLLVM-21.so" ]; then llvmname=LLVM-21; elif [ -e "$llvmlib/libLLVM.so" ]; then llvmname=LLVM; else llvmname=":libLLVM.so.21.1"; fi; fi
mkdir -p "$out"; export FIB_LIB=$root/lib TMPDIR=$out
t0=$(date +%s)
echo "bootstrap: $cc $(${cc} --version | head -1); LLVM library $llvmname in $llvmlib"
"$cc" -std=gnu11 -O2 -ffp-contract=off -fno-math-errno -fno-delete-null-pointer-checks -w -o "$out/fibc-c" "$src" -L "$llvmlib" "-l$llvmname" -Wl,-rpath,"$llvmlib" -lm -lpthread \
  || { echo "bootstrap: cc could not compile $src"; exit 1; }
t1=$(date +%s); echo "bootstrap: fibc.c ($(wc -c < "$src") bytes) compiled in $((t1 - t0)) s: $out/fibc-c ($(wc -c < "$out/fibc-c") bytes)"
"$out/fibc-c" --version || { echo "bootstrap: fibc-c does not run"; exit 1; }
(cd "$root" && "$out/fibc-c" build compiler/fibc.fib -I compiler -I lib -L "$llvmlib" "-l$llvmname" -o "$out/F3") || { echo "bootstrap: fibc-c could not build the tree's compiler"; exit 1; }
t2=$(date +%s); echo "bootstrap: fibc-c built compiler/fibc.fib (LLVM, stage 2 of this tree) in $((t2 - t1)) s: $out/F3"
(cd "$root" && "$out/fibc-c" emit -I compiler -I lib compiler/fibc.fib > "$out/emit.c") || exit 1
(cd "$root" && "$out/F3" emit -I compiler -I lib compiler/fibc.fib > "$out/emit.3") || exit 1
t3=$(date +%s)
if cmp -s "$out/emit.c" "$out/emit.3"; then echo "bootstrap: OK, the fixed point holds: fibc-c and F3 emit the same lIR for compiler/fibc.fib ($(wc -c < "$out/emit.c") bytes, $((t3 - t2)) s for both emits)"; exit 0
else echo "bootstrap: FAIL, the two emits differ"; cmp "$out/emit.c" "$out/emit.3"; exit 1; fi
