#!/bin/bash
# Compiles the ThinLTO shim (compiler/llvm/thinlto.cpp: the module summary writer and the in-process thin link that FIB_LIRO_THIN=1 uses,
# over the C++ API of the LLVM fibc links) into DIR/fibthin.o and prints the `-L`/`-l` words that make `fibc build` link it whole, before
# the LLVM words (scripts/llvm-static.sh or `-l LLVM-21`):
#   Linux (GNU ld):  DIR/libfibthin.so is an ld script, INPUT of the object (an object named on a link line is always linked, an archive
#                    member only when something refers to it, and fibc refers to nothing in the shim) and libstdc++ as needed.
#   macOS (ld64):    DIR/libfibthin.a is the object itself: ld64 takes a file found by `-l` for what it is (an object is linked whole).
# fibc names no symbol of the shim (it registers its functions with LLVM when fibc starts; llvm.thinlto finds them), so a fibc linked
# without these words still builds and runs; only FIB_LIRO_THIN=1 then refuses.
# usage: scripts/thinlto-shim.sh DIR
#   CXX names the C++ compiler (default c++); LLVM_CONFIG names llvm-config (as scripts/llvm-static.sh). THINLTO_SHIM=off prints nothing.
# Used by mk/config.mk (LLVM_ARGS), mk/release.mk and scripts/lib/stage2.sh.
set -eu
dir=${1:?usage: thinlto-shim.sh DIR}
[ "${THINLTO_SHIM:-on}" = off ] && exit 0
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$dir"
dir=$(cd "$dir" && pwd)
if [ "$(uname -s)" = Darwin ]; then
  lc=${LLVM_CONFIG:-/opt/homebrew/opt/llvm@21/bin/llvm-config}
else
  lc=${LLVM_CONFIG:-$(command -v llvm-config-21 || echo /usr/bin/llvm-config-21)}
fi
[ -x "$lc" ] || { echo "thinlto-shim: no llvm-config at $lc (set LLVM_CONFIG, or THINLTO_SHIM=off)" >&2; exit 2; }
cxx=${CXX:-c++}
command -v "$cxx" > /dev/null || { echo "thinlto-shim: no C++ compiler $cxx (set CXX, or THINLTO_SHIM=off)" >&2; exit 2; }
# shellcheck disable=SC2046
"$cxx" $("$lc" --cxxflags) -O2 -fPIC -c "$root/compiler/llvm/thinlto.cpp" -o "$dir/fibthin.o.new" 2> "$dir/cxx.err" \
  || { cat "$dir/cxx.err" >&2; exit 2; }
mv "$dir/fibthin.o.new" "$dir/fibthin.o"
if [ "$(uname -s)" = Darwin ]; then
  cp "$dir/fibthin.o" "$dir/libfibthin.a"
  echo "-L $dir -l fibthin -l c++"
else
  printf 'INPUT ( %s AS_NEEDED ( -lstdc++ ) )\n' "$dir/fibthin.o" > "$dir/libfibthin.so"
  echo "-L $dir -l fibthin"
fi
