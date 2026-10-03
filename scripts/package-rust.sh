#!/bin/bash
# Packages the Rust tools (the 0.0.x line, the seed of the bootstrap) as OUT/fibc-VERSION-linux-x86_64.tar.gz and OUT/SHA256SUMS.
# usage: scripts/package-rust.sh VERSION [OUT]       VERSION like 0.0.1 (no leading v); OUT defaults to ./dist
#   LLVM_SYS_211_PREFIX  LLVM 21 (default /usr/lib/llvm-21)
# The tarball holds fibc-VERSION-linux-x86_64/{bin/fibc, bin/fibref, lib/liblair.so, LICENSE, README.txt}. The Rust tools embed the library
# (`lib/`) and link lair statically; liblair.so is there because `fibc build compiler/fibc.fib -L DIR -l lair` (the stage-2 build) links it.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
version=${1:?usage: package-rust.sh VERSION [OUT]}
export LLVM_SYS_211_PREFIX=${LLVM_SYS_211_PREFIX:-/usr/lib/llvm-21}
out=$(mkdir -p "${2:-dist}" && cd "${2:-dist}" && pwd)
name=fibc-$version-linux-x86_64
work=$(mktemp -d "$out/work.XXXXXX")
trap 'rm -rf "$work"' EXIT
cargo build --release -p fibc -p fibref -p lair
tree=$work/$name
mkdir -p "$tree/bin" "$tree/lib"
cp target/release/fibc target/release/fibref "$tree/bin/"
cp target/release/liblair.so "$tree/lib/"
cp LICENSE "$tree/LICENSE"
sed "s/@VERSION@/$version/g" > "$tree/README.txt" <<'README'
fibber's Rust tools @VERSION@ (the 0.0.x line, the seed that builds the compiler written in fibber).

  bin/fibc      the compiler in Rust (stage 1): run, build, emit, explain, cases, gen
  bin/fibref    the reference interpreter and memory audit
  lib/liblair.so  lair's C interface, for `fibc build FILE -L lib -l lair`

The standard library is inside the executables. Licence: BSD 3-Clause, see LICENSE.
README
rm -f "$out/$name.tar.gz"
tar -C "$work" --owner=0 --group=0 --sort=name -czf "$out/$name.tar.gz" "$name"
(cd "$out" && sha256sum "$name.tar.gz" > SHA256SUMS)
ls -l "$out/$name.tar.gz"
cat "$out/SHA256SUMS"
