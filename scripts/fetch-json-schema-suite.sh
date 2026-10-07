#!/bin/bash
# Fetches the JSON-Schema-Test-Suite (https://github.com/json-schema-org/JSON-Schema-Test-Suite) at the pinned commit, after checking the
# archive's sha256 (ADR 0020). The gate never calls this: specs/json-schema-spec.fib is offline. Usage: scripts/fetch-json-schema-suite.sh [DEST]
# (default ~/.cache/fibber-scratch/tools/JSON-Schema-Test-Suite)
set -eu
COMMIT=5b0ee1613e45fcc2bddac00e07c19cd49b00d8a8
SHA256=baf7510ec75fb87311b8272cd04bab51459449f7c5f12befdc55a41ec19de8bf
dest=${1:-$HOME/.cache/fibber-scratch/tools/JSON-Schema-Test-Suite}
if [ ! -f "$dest/.commit-$COMMIT" ]; then
  tmp=$(mktemp -d)
  curl -sSL -m 300 -o "$tmp/suite.tar.gz" "https://codeload.github.com/json-schema-org/JSON-Schema-Test-Suite/tar.gz/$COMMIT"
  echo "$SHA256  $tmp/suite.tar.gz" | sha256sum -c --quiet || { echo "fetch-json-schema-suite: sha256 mismatch for $COMMIT" >&2; rm -rf "$tmp"; exit 1; }
  rm -rf "$dest"; mkdir -p "$dest"
  tar xzf "$tmp/suite.tar.gz" -C "$dest" --strip-components=1
  touch "$dest/.commit-$COMMIT"
  rm -rf "$tmp"
fi
echo "JSON-Schema-Test-Suite $COMMIT in $dest"
