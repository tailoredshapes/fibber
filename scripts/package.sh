#!/bin/bash
# Builds the release tarball of fibc, the compiler in fibber (stage 2), for the platform it runs on: linux-x86_64 or darwin-arm64 (Apple silicon macOS).
# usage: FIBC=/path/to/seed/fibc scripts/package.sh [OUT]       (run from anywhere; OUT defaults to ./dist)
#   FIBC                 the seed: any fibc that can build compiler/fibc.fib (the Rust one, or an unpacked release). Required. A seed made before the
#                        flip (v0.1.x) compiles with its own liblair.so: it is found in LAIR_DIR, else in ../lib of the seed's bin/ (a release layout).
#   LLVM_LINK            static (default) or shared: how the compiler links LLVM 21. static is the release: the archives of `llvm-config-21
#                        --link-static` (LLVM 21 dev, with its static libraries, must be installed; on macOS Homebrew's llvm@21 and zstd), through the
#                        library scripts/llvm-static.sh makes. shared (`-L /usr/lib/llvm-21/lib -l LLVM-21`, on macOS /opt/homebrew/opt/llvm@21/lib; LLVM_LIBDIR for another directory) is for trying this script
#                        only: the result needs the LLVM shared library and the checks of the shipped binary that forbid it are skipped.
#   LAIR_DIR             the directory with the seed's liblair.so (see FIBC); no stage 2 of this tree needs one
# Steps, each of which stops the script if it fails:
#   1. VERSION and compiler/driver/version.fib agree (scripts/check-version.sh).
#   2. Stage 2 (F) built by the seed; the stage check is the fixed point: F builds F3 and F3 emits the same lIR for compiler/fibc.fib as F (the seed's own emit is a note: its embedded prelude lags).
#   3. The shipped fibc: built by F3 and linked to LLVM statically (macOS: the `libtool`-merged archive of llvm-static.sh). `fibc build` writes an absolute rpath for every -L directory, here a directory of
#      this script's scratch that goes away with it; a `cc` shim drops it, so that bin/fibc has no RUNPATH at all. Checked on Linux: no RUNPATH/RPATH, no NEEDED
#      liblair or libLLVM, and `ldd` shows only libc, libm, libstdc++, libgcc_s, libz and libzstd (and the loader); no AVX in the code. Checked on macOS
#      (`otool`): no LC_RPATH, and every dylib it loads is under /usr/lib or /System (nothing of Homebrew), and the signature (`codesign -v`: ld64 signs ad hoc).
#   4. OUT/fibc-VERSION-PLATFORM.tar.gz holding fibc-VERSION-PLATFORM/{bin/fibc, share/fibber/lib/, LICENSE, README.txt} (no lib/: the
#      code generator is in bin/fibc), and OUT/SHA256SUMS. The unpacked tree runs with no environment: the compiler finds the library by its own location
#      (compiler/expand/libdir.fib).
# The machine that runs fibc needs (Linux) libc, libm, libstdc++, libgcc_s, libz and libzstd, (macOS) only the system libraries, and `cc` for `fibc build`; not LLVM.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
: "${FIBC:?FIBC must name a seed fibc}"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)  plat=linux-x86_64;  default_cpu=x86-64-v2; llvm_prefix=/usr/lib/llvm-21; llvm_libdir=/usr/lib/llvm-21/lib ;;
  Darwin-arm64)  plat=darwin-arm64;  default_cpu=apple-m1;  llvm_prefix=/opt/homebrew/opt/llvm@21; llvm_libdir=/opt/homebrew/opt/llvm@21/lib ;;
  *) echo "package: no release platform for $(uname -s)-$(uname -m) (linux-x86_64 and darwin-arm64)" >&2; exit 2 ;;
esac
export LLVM_SYS_211_PREFIX=${LLVM_SYS_211_PREFIX:-$llvm_prefix}
# lair generates code for the host CPU unless told otherwise; a release must run on machines other than the one that built it
# (a tarball built on a CI runner died with SIGILL on a desktop CPU), so everything this script builds targets a baseline:
# x86-64-v2 on x86, apple-m1 (the first Apple silicon: every later one runs its code) on a Mac.
export FIB_TARGET_CPU=${FIB_TARGET_CPU:-$default_cpu}
out=$(mkdir -p "${1:-dist}" && cd "${1:-dist}" && pwd)
"$root/scripts/check-version.sh"
version=$(tr -d '[:space:]' < VERSION)
name=fibc-$version-$plat
work=$(mktemp -d "$out/work.XXXXXX")
trap 'rm -rf "$work"' EXIT

LLVM_LINK=${LLVM_LINK:-static}
case $LLVM_LINK in
  static) read -r -a llvm_args < <("$root/scripts/llvm-static.sh" "$work/llvm") ;;
  shared) llvm_args=(-L "${LLVM_LIBDIR:-$llvm_libdir}" -l LLVM-21) ;;
  *) echo "package: LLVM_LINK is static or shared, not $LLVM_LINK" >&2; exit 2 ;;
esac
# The seed's own code generator, if it has one (a release before the flip): found by LD_LIBRARY_PATH when the seed compiles. The stages built here need none.
seed_lair=${LAIR_DIR:-$(dirname "$FIBC")/../lib}
seed_env=()
[ -f "$seed_lair/liblair.so" ] && seed_env=(env "LD_LIBRARY_PATH=$(cd "$seed_lair" && pwd)")

build() { # build FIBC OUT
  if [ "$1" = "$FIBC" ]; then ${seed_env[@]+"${seed_env[@]}"} "$1" build compiler/fibc.fib -I compiler -I lib "${llvm_args[@]}" -o "$2"
  else "$1" build compiler/fibc.fib -I compiler -I lib "${llvm_args[@]}" -o "$2"; fi
}
emit() { if [ "$1" = "$FIBC" ]; then ${seed_env[@]+"${seed_env[@]}"} "$1" emit -I compiler -I lib compiler/fibc.fib; else "$1" emit -I compiler -I lib compiler/fibc.fib; fi; }

echo "== stage 2 from the seed: $FIBC (LLVM $LLVM_LINK)"
build "$FIBC" "$work/F"
echo "== stage check"
emit "$FIBC" > "$work/emit.seed"
emit "$work/F" > "$work/emit.F"
if cmp -s "$work/emit.seed" "$work/emit.F"; then echo "note: F emits the seed's lIR"; else echo "note: F emits different lIR from the seed (its embedded prelude lags this tree's, or the seed is older than the flip)"; fi
build "$work/F" "$work/F3"
emit "$work/F3" > "$work/emit.F3"
cmp "$work/emit.F" "$work/emit.F3" || { echo "package: STAGE CHECK FAILED: F3 emit differs from F's (no fixed point)" >&2; exit 1; }
echo "stage check: F (built by the seed) and F3 (built by F) emit the same lIR ($(wc -c < "$work/emit.F") bytes)"

echo "== the shipped binary: LLVM $LLVM_LINK, no rpath"
mkdir "$work/shim"
cat > "$work/shim/cc" <<'SHIM'
#!/bin/sh
# Drops the rpath that `fibc build` writes for each -L directory (`-Xlinker -rpath -Xlinker DIR`): the one here is a scratch directory of the
# package script, and the shipped binary needs no library of its own (LLVM is inside it).
n=$#
while [ "$n" -gt 0 ]; do
  a=$1; shift; n=$((n - 1))
  if [ "$a" = -Xlinker ] && [ "$n" -ge 3 ] && [ "$1" = -rpath ]; then shift; shift; shift; n=$((n - 3)); continue; fi
  set -- "$@" "$a"
done
exec "$REAL_CC" "$@"
SHIM
chmod +x "$work/shim/cc"

tree=$work/$name
mkdir -p "$tree/bin" "$tree/share/fibber"
REAL_CC=$(command -v cc) PATH="$work/shim:$PATH" build "$work/F3" "$tree/bin/fibc"
if [ "$LLVM_LINK" = static ] && [ "$plat" = darwin-arm64 ]; then
  if otool -l "$tree/bin/fibc" | grep -q LC_RPATH; then
    echo "package: the shipped fibc has an LC_RPATH (the cc shim did not take):" >&2; otool -l "$tree/bin/fibc" | grep -A2 LC_RPATH >&2; exit 1
  fi
  stray=$(otool -L "$tree/bin/fibc" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/)' || true)
  if [ -n "$stray" ]; then echo "package: the shipped fibc loads libraries outside /usr/lib and /System: $stray" >&2; exit 1; fi
  codesign -v "$tree/bin/fibc" || { echo "package: the shipped fibc has no valid signature" >&2; exit 1; }
  echo "shipped fibc: no LC_RPATH, loads: $(otool -L "$tree/bin/fibc" | tail -n +2 | awk '{print $1}' | tr '\n' ' ')"
elif [ "$LLVM_LINK" = static ]; then
  if readelf -d "$tree/bin/fibc" | grep -q -E 'RUNPATH|RPATH'; then
    echo "package: the shipped fibc has an RUNPATH/RPATH (the cc shim did not take, or a library is still found by one):" >&2
    readelf -d "$tree/bin/fibc" | grep -E 'RUNPATH|RPATH' >&2; exit 1
  fi
  if readelf -d "$tree/bin/fibc" | grep NEEDED | grep -q -E 'liblair|libLLVM'; then
    echo "package: the shipped fibc needs liblair or libLLVM:" >&2; readelf -d "$tree/bin/fibc" | grep NEEDED >&2; exit 1
  fi
  extra=$(ldd "$tree/bin/fibc" | awk '{print $1}' | sed 's|.*/||' | grep -v -E '^(linux-vdso|ld-linux)|^(libc|libm|libstdc\+\+|libgcc_s|libz|libzstd)\.' || true)
  if [ -n "$extra" ]; then echo "package: ldd shows libraries beyond libc, libm, libstdc++, libgcc_s, libz, libzstd: $extra" >&2; exit 1; fi
  echo "shipped fibc: no RUNPATH, NEEDED: $(readelf -d "$tree/bin/fibc" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p' | tr '\n' ' ')"
else
  echo "note: LLVM_LINK=shared: this binary needs the LLVM shared library; it is not a release"
fi
# No AVX in the code generated for x86-64-v2, by function: LLVM's static archives carry BLAKE3's AVX2 and AVX-512 kernels, which blake3 picks at run time
# by cpuid (`_llvm_blake3_*_avx2`, `_avx512`), so those and only those are allowed to use ymm/zmm.
if [ "$plat" = linux-x86_64 ] && command -v objdump >/dev/null 2>&1 && [ "$FIB_TARGET_CPU" = x86-64-v2 ]; then
  avx=$(objdump -d --no-show-raw-insn "$tree/bin/fibc" | awk '/^[0-9a-f]+ <.*>:$/ { fn = $2 } /(ymm|zmm)[0-9]/ { c[fn]++ } END { for (f in c) print f }' \
        | grep -v -E '^<_llvm_blake3_[a-z_]*(avx2|avx512)>:$' || true)
  if [ -n "$avx" ]; then
    echo "package: the shipped fibc uses AVX registers although it was built for $FIB_TARGET_CPU (FIB_TARGET_CPU did not take), in: $(echo "$avx" | head -n 5 | tr '\n' ' ')" >&2; exit 1
  fi
fi
emit "$tree/bin/fibc" > "$work/emit.ship"
cmp "$work/emit.F" "$work/emit.ship" || { echo "package: the shipped fibc emits something else" >&2; exit 1; }
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
  share/fibber/lib/        the standard library source, found beside bin/ by fibc itself

Use it where it is unpacked; no environment variable is needed:
  bin/fibc --version
  bin/fibc run hello.fib
  bin/fibc build hello.fib -o hello      (needs a C compiler, `cc`, to link)
Move the whole directory as one: bin/ and share/ stay side by side.
FIB_LIB and -I add module roots in front of the library as usual. `fibc help` lists the commands.

Needs only the system libraries (on Linux libc, libm, libstdc++, libgcc_s, libz and libzstd; on macOS the ones under /usr/lib); the code generator, LLVM, is inside bin/fibc,
so it does not need LLVM installed.
Licence: BSD 3-Clause, see LICENSE.
README
rm -f "$out/$name.tar.gz"
if tar --version 2>/dev/null | grep -q 'GNU tar'; then tar -C "$work" --owner=0 --group=0 --sort=name -czf "$out/$name.tar.gz" "$name"
else tar -C "$work" --uid 0 --gid 0 -czf "$out/$name.tar.gz" "$name"; fi   # bsdtar (macOS) has no --sort
(cd "$out" && { sha256sum "$name.tar.gz" 2>/dev/null || shasum -a 256 "$name.tar.gz"; } > SHA256SUMS)
echo "== done"
ls -l "$out/$name.tar.gz"
cat "$out/SHA256SUMS"
