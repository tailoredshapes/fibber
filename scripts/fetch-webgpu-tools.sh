#!/bin/bash
# scripts/fetch-webgpu-tools.sh: the tools of the WebGPU kernel target (docs/design/webgpu.md 8; ADR 0020: every download has a recorded checksum and is verified).
# Fetches into DIR (default ~/.cache/fibber-scratch/tools), nothing installed on the system:
#   wgpu-native/   the wgpu-native release (libwgpu_native.so, webgpu.h, wgpu.h): the native WebGPU driver fib-gpu-webgpu links it (WGPU_NATIVE_DIR)
#   naga/bin/naga  naga-cli, the WGSL validator and translator (built from the crates.io tarball with cargo; the WGSL oracle of compiler/tests/native/wgsl-emit.sh)
#   webgpu-node/   the `webgpu` npm package (Dawn's node bindings: navigator.gpu under node, the second validator; its own install step fetches Dawn's prebuilt binary)
# usage: fetch-webgpu-tools.sh [DIR] [wgpu-native|naga|node]..  (default: all three)
set -euo pipefail
DIR=${1:-$HOME/.cache/fibber-scratch/tools}; shift || true
WANT=${*:-"wgpu-native naga node"}
WGPU_VER=v29.0.1.1
WGPU_URL=https://github.com/gfx-rs/wgpu-native/releases/download/$WGPU_VER/wgpu-linux-x86_64-release.zip
WGPU_SHA=95a4d90c071005a98d03eab348beaa6b07e16eb00d1dcdb9f8348f75eb97ec5a
NAGA_VER=30.0.1
NAGA_URL=https://static.crates.io/crates/naga-cli/naga-cli-$NAGA_VER.crate
NAGA_SHA=45283c11b8b1da1936214eefc9caf9c1bc74ae5c090023eaea84ba317a194c8c
NODE_VER=0.6.2
NODE_URL=https://registry.npmjs.org/webgpu/-/webgpu-$NODE_VER.tgz
NODE_SHA=77a6be6513b9f7c2e94425620ce603efb38cdc5d0393e1b31a1b0c8479330ffd
mkdir -p "$DIR"
fetch() { # fetch URL FILE SHA
  [ -f "$2" ] || curl -sfL -m 600 -o "$2" "$1"
  echo "$3  $2" | sha256sum -c - >/dev/null || { echo "fetch-webgpu-tools: checksum of $2 differs from the recorded one" >&2; rm -f "$2"; exit 1; }
}
for what in $WANT; do
  case $what in
    wgpu-native)
      mkdir -p "$DIR/wgpu-native"; cd "$DIR/wgpu-native"
      fetch "$WGPU_URL" wgpu-linux-x86_64-release.zip "$WGPU_SHA"
      [ -f lib/libwgpu_native.so ] || python3 -c "import zipfile; zipfile.ZipFile('wgpu-linux-x86_64-release.zip').extractall('.')"
      echo "$WGPU_VER" > VERSION; echo "wgpu-native $WGPU_VER: $DIR/wgpu-native/lib/libwgpu_native.so" ;;
    naga)
      mkdir -p "$DIR/naga/src"; cd "$DIR/naga/src"
      fetch "$NAGA_URL" naga-cli-$NAGA_VER.crate "$NAGA_SHA"
      [ -d naga-cli-$NAGA_VER ] || tar xzf naga-cli-$NAGA_VER.crate
      [ -x "$DIR/naga/bin/naga" ] || CARGO_TARGET_DIR="$DIR/naga/target" nice "$HOME/.cargo/bin/cargo" install --path naga-cli-$NAGA_VER --root "$DIR/naga" --locked -j 4 >/dev/null 2>&1 \
        || CARGO_TARGET_DIR="$DIR/naga/target" nice "$HOME/.cargo/bin/cargo" install --path naga-cli-$NAGA_VER --root "$DIR/naga" -j 4 2>&1 | tail -3
      echo "naga-cli $NAGA_VER: $("$DIR/naga/bin/naga" --version 2>&1) at $DIR/naga/bin/naga" ;;
    node)
      mkdir -p "$DIR/webgpu-node"; cd "$DIR/webgpu-node"
      [ -f webgpu-$NODE_VER.tgz ] || curl -sfL -m 600 -o webgpu-$NODE_VER.tgz "$NODE_URL"
      if [ -z "$NODE_SHA" ]; then sha256sum webgpu-$NODE_VER.tgz | tee webgpu-$NODE_VER.tgz.sha256; else echo "$NODE_SHA  webgpu-$NODE_VER.tgz" | sha256sum -c - >/dev/null || { echo "fetch-webgpu-tools: checksum of webgpu-$NODE_VER.tgz differs" >&2; exit 1; }; fi
      [ -d node_modules/webgpu ] || { echo '{"name":"fib-webgpu-tools","private":true}' > package.json; npm install --no-audit --no-fund "./webgpu-$NODE_VER.tgz" 2>&1 | tail -2; }
      echo "webgpu (Dawn for node) $NODE_VER: $DIR/webgpu-node/node_modules/webgpu" ;;
    *) echo "fetch-webgpu-tools: unknown tool $what" >&2; exit 2 ;;
  esac
done
