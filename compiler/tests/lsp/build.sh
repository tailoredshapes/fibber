#!/bin/bash
# Builds the language server into OUT: compiler/lsp.fib (the tool), or with TARGET=fibc compiler/fibc.fib (`fibc lsp`).
# usage: build.sh OUT      FIBC names the fibc (default `fibc`); FIB_LIB the library (default lib/)
# REAL=1 builds the tree as it is, with the real lsp.json, lsp.text, lsp.analysis and lsp.scope (packages L0 to L2). Without it the stand-ins under
# standin/lsp/ replace those four files in a COPY of compiler/ (SCRATCH, default $TMPDIR or /tmp): a module is looked for beside the main file
# first, so `-I` cannot shadow compiler/lsp/*.fib for a main file that lives in compiler/; the copy is the one mechanism that works for both
# tools, and nothing under compiler/ itself is changed.
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
tree=$root
if [ "${REAL:-}" != 1 ]; then
  tree=${SCRATCH:-${TMPDIR:-/tmp}}/lsp-standin-tree
  rm -rf "$tree"; mkdir -p "$tree"
  cp -r "$root/compiler" "$tree/compiler"
  ln -s "$root/lib" "$tree/lib"
  cp "$root"/compiler/tests/lsp/standin/lsp/*.fib "$tree/compiler/lsp/"
fi
cd "$tree" || exit 2
ulimit -v "${ULIMIT_V:-16000000}"
"$fibc" build "$main" -I compiler -I lib -L "$llvmdir" -l LLVM-21 -o "$out"
