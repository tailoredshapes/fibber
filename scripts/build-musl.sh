#!/bin/bash
# scripts/build-musl.sh ARCH OUTDIR [TARBALL]: builds the pieces a static `fibc build --static` links against, from the pinned musl source.
#   ARCH     x86_64 or aarch64
#   OUTDIR   receives libc.a, crt1.o, crti.o, crtn.o, libgcc.a (the compiler support routines the generated code calls) and MUSL-LICENSE
#            (the layout `fibc build --static` reads: share/fibber/musl/ARCH/)
#   TARBALL  musl-1.2.5.tar.gz if you have it; else it is fetched (https://musl.libc.org/releases/) and checked
# Needs a C compiler for ARCH (gcc; aarch64-linux-gnu-gcc for aarch64 on an x86_64 host) and make. It is the only place the toolchain is
# needed: building fibber programs never needs it (docs/design/static-linking.md).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
MUSL_VERSION=1.2.5
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4
arch=${1:?usage: build-musl.sh x86_64|aarch64 OUTDIR [TARBALL]}; out=${2:?usage: build-musl.sh ARCH OUTDIR [TARBALL]}; tarball=${3:-}
case $arch in
  x86_64) cc=${MUSL_CC:-gcc} ;;
  aarch64) cc=${MUSL_CC:-aarch64-linux-gnu-gcc} ;;
  *) echo "build-musl: ARCH is x86_64 or aarch64" >&2; exit 2 ;;
esac
command -v "$cc" > /dev/null || { echo "build-musl: no C compiler $cc (set MUSL_CC)" >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/musl-build.XXXXXX"); trap 'rm -rf "$work"' EXIT
if [ -z "$tarball" ]; then
  tarball=$work/musl.tar.gz
  curl -fsSL -o "$tarball" "https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz"
fi
got=$(sha256sum "$tarball" | cut -d' ' -f1)
[ "$got" = "$MUSL_SHA256" ] || { echo "build-musl: $tarball has sha256 $got, expected $MUSL_SHA256" >&2; exit 1; }
tar -xzf "$tarball" -C "$work"
mkdir -p "$out"; out=$(cd "$out" && pwd)
(cd "$work/musl-$MUSL_VERSION" \
  && ./configure --target="$arch" --disable-shared --prefix="$work/inst" CC="$cc" AR="${cc%gcc}ar" RANLIB="${cc%gcc}ranlib" CFLAGS="-O2" > "$work/configure.log" 2>&1 \
  && make -j"$(nproc)" > "$work/make.log" 2>&1 && make install > /dev/null 2>&1) \
  || { echo "build-musl: musl did not build; logs in $work" >&2; trap - EXIT; exit 1; }
cp "$work/inst/lib/libc.a" "$work/inst/lib/crt1.o" "$work/inst/lib/crti.o" "$work/inst/lib/crtn.o" "$out/"
cp "$work/musl-$MUSL_VERSION/COPYRIGHT" "$out/MUSL-LICENSE"
libgcc=$("$cc" -print-libgcc-file-name)
cp "$libgcc" "$out/libgcc.a"
if [ "$arch" = x86_64 ]; then   # the start-up CPU check's glibc function, which musl lacks (rt/static/cpuid.c)
  "$cc" -O2 -c "$here/../rt/static/cpuid.c" -o "$out/fibshim.o"
fi
echo "built musl $MUSL_VERSION for $arch into $out"
