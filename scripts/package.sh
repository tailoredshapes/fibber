#!/bin/bash
# Builds the release tarball of fibc, since MAKE-1 a wrapper over `make release` (mk/release.mk, docs/design/build.md): the steps are targets under
# build/release/ (F built by the seed, the stage check, the shipped binary with no rpath, its checks in scripts/lib/check-shipped.sh, the tree,
# the --version and hello run in an empty environment), then OUT/fibc-VERSION-PLATFORM.tar.gz and OUT/SHA256SUMS.
# usage: FIBC=/path/to/seed/fibc scripts/package.sh [OUT]       (OUT defaults to ./dist)
#   FIBC                 the seed (BUILDER of the Makefile; default: the one SEED names, fetched under build/seed/)
#   LLVM_LINK            static (default) or shared (RELEASE_LLVM_LINK of the Makefile: shared is for trying only, the binary is not a release)
#   WITH_MUSL=1          adds share/fibber/musl/ARCH/ (MUSL_TARBALL names a local musl-1.2.5.tar.gz)
#   FIB_TARGET_CPU       the baseline CPU (RELEASE_CPU; default x86-64-v3 on Linux, apple-m1 on a Mac: docs/adr/0008)
# The version check is a target of its own ("$root/scripts/check-version.sh" is the recipe of build/version.ok, which the release depends on).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
make=make; [ "$(uname -s)" = Darwin ] && command -v gmake > /dev/null 2>&1 && make=gmake
args=(DIST="${1:-dist}")
[ -n "${FIBC:-}" ] && args+=(BUILDER="$FIBC")
[ -n "${LLVM_LINK:-}" ] && args+=(RELEASE_LLVM_LINK="$LLVM_LINK")
[ -n "${WITH_MUSL:-}" ] && args+=(WITH_MUSL="$WITH_MUSL")
[ -n "${MUSL_TARBALL:-}" ] && args+=(MUSL_TARBALL="$MUSL_TARBALL")
[ -n "${FIB_TARGET_CPU:-}" ] && args+=(RELEASE_CPU="$FIB_TARGET_CPU")
exec "$make" release "${args[@]}"
