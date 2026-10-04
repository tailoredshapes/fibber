#!/bin/bash
# Builds compiler/tests/llvm/spike.fib with an `fibc` that needs no liblair.so, linking LLVM either shared or static.
# usage: build.sh shared|static OUT        (from the repository root; FIBC names the fibc to build with, default `fibc`; FIB_LIB the library)
#   LLVM_CONFIG  llvm-config (default llvm-config-21 on PATH or /usr/bin/llvm-config-21)
#
# fibc's link step is `cc OBJ -o OUT -lm -lpthread` then `-L DIR -Xlinker -rpath -Xlinker DIR` for each -L, then `-lNAME` for each -l
# (crates/lair/src/aot/link.rs `args`, compiler/driver/native.fib). Only -L and -l words reach `cc`, so:
#   shared: `-L LLVMLIBDIR -l LLVM-21`, the libLLVM-21.so of the machine (the rpath makes the executable find it).
#   static: LLVM's static archives, in a GNU ld linker script that fibc's `-l` can name. llvm-config lists the archives of the
#           components lair needs (core, orcjit, native, passes, analysis, irreader, bitwriter: every LLVM-C function the lair crate and
#           the spike call) in dependency order; the script is a `GROUP` of them (ld re-scans the group until nothing is undefined)
#           followed by the system libraries LLVM needs, and libstdc++ (LLVM is C++ and `cc` does not link it). The script is written to
#           $OUT.static/libllvm-static.so, and `-L $OUT.static -l llvm-static` finds it as it would a library.
set -eu
mode=${1:?usage: build.sh shared|static OUT}
out=${2:?usage: build.sh shared|static OUT}
fibc=${FIBC:-fibc}
lc=${LLVM_CONFIG:-$(command -v llvm-config-21 || echo /usr/bin/llvm-config-21)}
libdir=$("$lc" --libdir)
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
case $mode in
  shared)
    "$fibc" build -I "$root/compiler" "$root/compiler/tests/llvm/spike.fib" -L "$libdir" -l LLVM-21 -o "$out" ;;
  static)
    d=$out.static; LLVM_CONFIG=$lc "$root/scripts/llvm-static.sh" "$d" > /dev/null
    "$fibc" build -I "$root/compiler" "$root/compiler/tests/llvm/spike.fib" -L "$d" -l llvm-static -o "$out" ;;
  *) echo "mode is shared or static" >&2; exit 2 ;;
esac
