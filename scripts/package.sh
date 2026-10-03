#!/bin/bash
# Builds the release tarball of fibc, the compiler in fibber (stage 2), for linux x86_64.
# usage: FIBC=/path/to/seed/fibc scripts/package.sh [OUT]       (run from anywhere; OUT defaults to ./dist)
#   FIBC                 the seed: any fibc that can build compiler/fibc.fib (the Rust one, or an unpacked release). Required.
#   LLVM_SYS_211_PREFIX  LLVM 21 for building liblair.so (default /usr/lib/llvm-21)
#   LAIR_DIR             a directory that already holds a release-mode liblair.so: skips `cargo build --release -p lair`
#                        (for trying this script; a release is built without it)
# Steps, each of which stops the script if it fails:
#   1. VERSION and compiler/driver/version.fib agree (scripts/check-version.sh).
#   2. liblair.so in release mode.
#   3. Stage 2 (F) built by the seed; the stage check is the fixed point: F builds F3 and F3 emits the same lIR for compiler/fibc.fib as F (the seed's own emit is a note: its embedded prelude lags).
#   4. The shipped fibc: built by F3, with the rpath `$ORIGIN/../lib` and not the absolute one lair's link writes (a `cc` shim).
#   5. OUT/fibc-VERSION-linux-x86_64.tar.gz holding fibc-VERSION-linux-x86_64/{bin/fibc, lib/liblair.so, share/fibber/lib/, LICENSE,
#      README.txt}, and OUT/SHA256SUMS. The unpacked tree runs with no environment: the binary finds liblair.so by its rpath and the
#      library by its own location (compiler/expand/libdir.fib).
# The machine that runs fibc needs libc, libm, libstdc++, libgcc_s, libz and libzstd (what liblair.so links), and `cc` for `fibc build`.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
: "${FIBC:?FIBC must name a seed fibc}"
export LLVM_SYS_211_PREFIX=${LLVM_SYS_211_PREFIX:-/usr/lib/llvm-21}
out=$(mkdir -p "${1:-dist}" && cd "${1:-dist}" && pwd)
"$root/scripts/check-version.sh"
version=$(tr -d '[:space:]' < VERSION)
name=fibc-$version-linux-x86_64
work=$(mktemp -d "$out/work.XXXXXX")
trap 'rm -rf "$work"' EXIT

if [ -n "${LAIR_DIR:-}" ]; then
  lair=$(cd "$LAIR_DIR" && pwd)
else
  cargo build --release -p lair
  lair=$root/target/release
fi
[ -f "$lair/liblair.so" ] || { echo "package: no liblair.so in $lair" >&2; exit 1; }
cp "$lair/liblair.so" "$work/liblair.so"
lair=$work   # build against a copy, so the absolute rpath of the intermediate binaries names a directory that goes away with them

build() { # build SEED OUT [extra link arguments are in PATH's cc]
  LD_LIBRARY_PATH=$lair "$1" build compiler/fibc.fib -I compiler -I lib -L "$lair" -l lair -o "$2"
}
emit() { LD_LIBRARY_PATH=$lair "$1" emit -I compiler -I lib compiler/fibc.fib; }

echo "== stage 2 from the seed: $FIBC"
build "$FIBC" "$work/F"
echo "== stage check"
emit "$FIBC" > "$work/emit.seed"
emit "$work/F" > "$work/emit.F"
if cmp -s "$work/emit.seed" "$work/emit.F"; then echo "note: F emits the seed's lIR"; else echo "note: F emits different lIR from the seed (its embedded prelude lags this tree's)"; fi
build "$work/F" "$work/F3"
emit "$work/F3" > "$work/emit.F3"
cmp "$work/emit.F" "$work/emit.F3" || { echo "package: STAGE CHECK FAILED: F3 emit differs from F's (no fixed point)" >&2; exit 1; }
echo "stage check: F (built by the seed) and F3 (built by F) emit the same lIR ($(wc -c < "$work/emit.F") bytes)"

tree=$work/$name
mkdir -p "$tree/bin" "$tree/lib" "$tree/share/fibber"
REAL_CC=$(command -v cc) PATH="$work/shim:$PATH" build "$work/F3" "$tree/bin/fibc"
emit "$tree/bin/fibc" > "$work/emit.ship"
cmp "$work/emit.F" "$work/emit.ship" || { echo "package: the shipped fibc emits something else" >&2; exit 1; }
cp "$work/liblair.so" "$tree/lib/liblair.so"
# The seed's fibref (the frozen reference interpreter, which also serves `fibref lsp` to the editor extension) rides along when
# the seed has one beside its fibc: the bootstrapped tarball has no interpreter of its own.
seed_fibref=$(dirname "$FIBC")/fibref
if [ -x "$seed_fibref" ]; then cp "$seed_fibref" "$tree/bin/fibref"; fi
cp -r lib "$tree/share/fibber/lib"
cp LICENSE "$tree/LICENSE"
sed "s/@VERSION@/$version/g" > "$tree/README.txt" <<'README'
fibc @VERSION@: the fibber compiler, written in fibber (stage 2).

Layout
  bin/fibc                 the compiler
  bin/fibref               the frozen reference interpreter and `fibref lsp` for editors (when the seed had one)
  lib/liblair.so           the code generator (lair), found by the rpath $ORIGIN/../lib
  share/fibber/lib/        the standard library source, found beside bin/ by fibc itself

Use it where it is unpacked; no environment variable is needed:
  bin/fibc --version
  bin/fibc run hello.fib
  bin/fibc build hello.fib -o hello      (needs a C compiler, `cc`, to link)
Move the whole directory as one: bin/, lib/ and share/ stay side by side.
FIB_LIB and -I add module roots in front of the library as usual. `fibc help` lists the commands.

Needs libc, libm, libstdc++, libgcc_s, libz and libzstd (liblair.so contains LLVM); it does not need LLVM installed.
Licence: BSD 3-Clause, see LICENSE.
README
rm -f "$out/$name.tar.gz"
tar -C "$work" --owner=0 --group=0 --sort=name -czf "$out/$name.tar.gz" "$name"
(cd "$out" && sha256sum "$name.tar.gz" > SHA256SUMS)
echo "== done"
ls -l "$out/$name.tar.gz"
cat "$out/SHA256SUMS"
