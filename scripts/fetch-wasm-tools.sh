#!/bin/bash
# Fetches the WebAssembly toolchain of `fibc build --target wasm32-wasi` into scratch (docs/design/wasm.md 3, ADR 0020): a wasi-sdk (wasm-ld, the WASI sysroot with wasi-libc,
# compiler-rt's builtins) and wasmtime (a second runtime for the tests; node's node:wasi is the first). Each archive is checked against the sha256 written here, which is the
# `digest` GitHub publishes for the release asset (checked on 2026-10-07); a changed or substituted download fails and is removed. Nothing is installed system-wide.
#   scripts/fetch-wasm-tools.sh [DEST]      default ~/.cache/fibber-scratch/tools/wasm
# Then:  export WASI_SDK=DEST/wasi-sdk-34.0-x86_64-linux     (and WASMTIME=DEST/wasmtime-v49.0.2-x86_64-linux/wasmtime for the optional second runtime)
# Linux x86-64 only: the other hosts' archives are listed in the release pages of the two projects; add their sha256 here to use them.
set -eu
dest=${1:-$HOME/.cache/fibber-scratch/tools/wasm}
SDK=wasi-sdk-34.0-x86_64-linux
SDK_SHA256=b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4
WT=wasmtime-v49.0.2-x86_64-linux
WT_SHA256=a4d6e9e3a5a60f527cf7793d674c48930c80c2e8977995b8a275cad3254b9322
case $(uname -s)-$(uname -m) in Linux-x86_64) ;; *) echo "fetch-wasm-tools: only Linux x86-64 archives are pinned here" >&2; exit 2 ;; esac
mkdir -p "$dest"
fetch() { # NAME URL SHA256 EXT
  local name=$1 url=$2 sha=$3 ext=$4
  [ -d "$dest/$name" ] && { echo "$name is in $dest"; return 0; }
  curl -sSL -m 900 -o "$dest/$name.$ext" "$url"
  if ! echo "$sha  $dest/$name.$ext" | sha256sum -c --quiet; then rm -f "$dest/$name.$ext"; echo "fetch-wasm-tools: $name does not match its sha256" >&2; exit 1; fi
  case $ext in tar.gz) tar xzf "$dest/$name.$ext" -C "$dest" ;; tar.xz) tar xJf "$dest/$name.$ext" -C "$dest" ;; esac
  echo "$name verified ($sha) and unpacked in $dest"
}
fetch "$SDK" "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/$SDK.tar.gz" "$SDK_SHA256" tar.gz
fetch "$WT" "https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.2/$WT.tar.xz" "$WT_SHA256" tar.xz
echo "WASI_SDK=$dest/$SDK"
