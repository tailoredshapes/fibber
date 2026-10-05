#!/bin/bash
# Builds the language server into OUT: compiler/lsp.fib (the tool), or with TARGET=fibc compiler/fibc.fib (`fibc lsp`).
# usage: build.sh OUT      FIBC names the fibc (default `fibc`); FIB_LIB the library (default lib/); run from anywhere
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
out=${1:?usage: build.sh OUT}
case $out in /*) ;; *) out=$PWD/$out ;; esac
export FIB_LIB=${FIB_LIB:-$root/lib}
llvmdir=${LLVM_LIBDIR:-$(llvm-config-21 --libdir 2>/dev/null || echo /usr/lib/llvm-21/lib)}
main=compiler/lsp.fib
[ "${TARGET:-}" = fibc ] && main=compiler/fibc.fib
cd "$root" || exit 2
ulimit -v "${ULIMIT_V:-16000000}"
"$fibc" build "$main" -I compiler -I lib -L "$llvmdir" -l LLVM-21 -o "$out"
