#!/bin/bash
# Cross-compiles the compiler itself (compiler/fibc.fib, stage 2) and lairf to Mach-O arm64 objects on this x86 machine and links them on an Apple Silicon Mac
# against Homebrew's LLVM 21 (keg-only: /opt/homebrew/opt/llvm@21), under the Mac's ~/fibber-a64-scratch only. Leaves ~/fibber-a64-scratch/fibc there (a native
# stage 2 for the Mac: the first seed for `scripts/mac-check.sh`). Builds nothing from LLVM; the Mac needs `brew install llvm@21` and Xcode's command line tools.
# usage: a64-fibc-on-mac.sh [--host HOST]       F=stage-2 fibc, L=lairf, LD_LIBRARY_PATH reaching libLLVM-21 (this machine's)
# Exit: 0 linked and `fibc --version` ran on the Mac; 1 a step failed; 2 no Mac or no tools.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
host=192.168.7.254; triple=${TRIPLE:-arm64-apple-macosx13.0.0}; llvm=${MAC_LLVM:-/opt/homebrew/opt/llvm@21}
[ "${1:-}" = --host ] && host=$2
: "${F:?F names the stage-2 fibc}" "${L:?L names lairf}"
export FIB_LIB=$root/lib
T=$(mktemp -d "${TMPDIR:-/tmp}/a64f.XXXXXX"); trap 'rm -rf "$T"' EXIT
ssh -o BatchMode=yes -o ConnectTimeout=10 "$host" 'mkdir -p ~/fibber-a64-scratch' || { echo "a64-fibc-on-mac: no ssh to $host" >&2; exit 2; }
for tool in fibc lairf; do
  "$F" emit --exe --target "$triple" -I compiler -I lib "compiler/$tool.fib" > "$T/$tool.lir" || { echo "FAIL emit $tool"; exit 1; }
  "$L" build "$T/$tool.lir" --target "$triple" -o "$T/$tool.o" -O 2 --emit obj || { echo "FAIL object $tool"; exit 1; }
  scp -q "$T/$tool.o" "$host:fibber-a64-scratch/$tool-mac.o" || exit 2
  ssh -o BatchMode=yes "$host" "cd ~/fibber-a64-scratch && cc $tool-mac.o -o $tool -L$llvm/lib -lLLVM-21 -Wl,-rpath,$llvm/lib -lm -lpthread" || { echo "FAIL link $tool on the Mac"; exit 1; }
  echo "linked $tool on the Mac"
done
ssh -o BatchMode=yes "$host" 'cd ~/fibber-a64-scratch && ./fibc --version'
