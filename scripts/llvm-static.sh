#!/bin/bash
# Prepares LLVM 21's static archives (the components the compiler's native.* modules call through LLVM-C: core orcjit native passes analysis irreader
# bitwriter, plus the x86 and aarch64 backends the target initialisation of compiler/llvm/target.fib names, in llvm-config's dependency order) so that
#   fibc build PROGRAM <the words this script prints>
# links LLVM into the program (as `-L /usr/lib/llvm-21/lib -l LLVM-21` links the shared libLLVM-21.so). `fibc build` passes only -L and -l words to
# `cc` and gives no --start-group, so:
#   Linux (GNU ld):  DIR/libllvm-static.so is an ld script: the archives in a GROUP (ld re-scans it until nothing is undefined), then the system libraries
#                    they need; libstdc++ is named in it because LLVM is C++ and `cc` does not link it. Prints `-L DIR -l llvm-static`.
#   macOS (ld64):    ld64 resolves archive members in any order, so no GROUP is needed, but `-l` takes one library: DIR/libllvm-static.a is the archives and
#                    Homebrew's static libzstd.a merged by Apple's `libtool -static` (so that no /opt/homebrew dylib is referenced); the system libraries
#                    (libc++, libz, libxml2, from /usr/lib) are named on the command line. Prints `-L DIR -l llvm-static -l c++ -l z -l xml2`.
# usage: scripts/llvm-static.sh DIR
#   LLVM_CONFIG names llvm-config (Linux: llvm-config-21, else /usr/bin/llvm-config-21; macOS: /opt/homebrew/opt/llvm@21/bin/llvm-config)
#   ZSTD_A (macOS) names the static libzstd.a (default: $(brew --prefix zstd)/lib/libzstd.a, else /opt/homebrew/opt/zstd/lib/libzstd.a)
# Used by scripts/lib/stage2.sh (LLVM_LINK=static), scripts/package.sh and compiler/tests/llvm/build.sh.
set -eu
dir=${1:?usage: llvm-static.sh DIR}
mkdir -p "$dir"
comps="core orcjit native passes analysis irreader bitwriter x86 aarch64"
if [ "$(uname -s)" = Darwin ]; then
  lc=${LLVM_CONFIG:-/opt/homebrew/opt/llvm@21/bin/llvm-config}
  [ -x "$lc" ] || { echo "llvm-static: no llvm-config at $lc (brew install llvm@21; set LLVM_CONFIG)" >&2; exit 2; }
  zstd=${ZSTD_A:-$(brew --prefix zstd 2>/dev/null || echo /opt/homebrew/opt/zstd)/lib/libzstd.a}
  [ -f "$zstd" ] || { echo "llvm-static: no static libzstd.a at $zstd (set ZSTD_A)" >&2; exit 2; }
  # shellcheck disable=SC2086
  archives=$("$lc" --link-static --libfiles $comps | tr ' ' '\n' | grep -v '^$' | tr '\n' ' ')
  [ -n "$archives" ] || { echo "llvm-static: llvm-config listed no archives" >&2; exit 2; }
  for a in $archives; do [ -f "$a" ] || { echo "llvm-static: missing archive $a" >&2; exit 2; }; done
  # shellcheck disable=SC2086
  libtool -static -o "$dir/libllvm-static.a" $archives "$zstd" 2> "$dir/libtool.err" || { cat "$dir/libtool.err" >&2; exit 2; }
  rm -f "$dir/libllvm-static.dylib"
  echo "-L $dir -l llvm-static -l c++ -l z -l xml2"
  exit 0
fi
lc=${LLVM_CONFIG:-$(command -v llvm-config-21 || echo /usr/bin/llvm-config-21)}
[ -x "$lc" ] || { echo "llvm-static: no llvm-config-21 (set LLVM_CONFIG)" >&2; exit 2; }
# shellcheck disable=SC2086
archives=$("$lc" --link-static --libfiles $comps | tr ' ' '\n' | grep -v '^$' | tr '\n' ' ')
[ -n "$archives" ] || { echo "llvm-static: llvm-config listed no archives (is the static LLVM 21 installed?)" >&2; exit 2; }
for a in $archives; do [ -f "$a" ] || { echo "llvm-static: missing archive $a" >&2; exit 2; }; done
libs=$("$lc" --link-static --system-libs | tr '\n' ' ')
printf 'GROUP ( %s AS_NEEDED ( -lstdc++ %s ) )\n' "$archives" "$libs" > "$dir/libllvm-static.so"
echo "-L $dir -l llvm-static"
