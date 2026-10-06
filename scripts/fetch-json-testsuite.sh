#!/bin/bash
# Fetches the JSONTestSuite (https://github.com/nst/JSONTestSuite, Nicolas Seriot) at the pinned commit and verifies every test_parsing file against
# specs/json-testsuite.sha256 (the checksum list is committed; the files are not). The gate never calls this: it is offline and uses
# specs/json-testsuite-subset/ . Usage: scripts/fetch-json-testsuite.sh [DEST]   (default ~/.cache/fibber-scratch/tools/JSONTestSuite)
set -eu
COMMIT=1ef36fa01286573e846ac449e8683f8833c5b26a
here=$(cd "$(dirname "$0")" && pwd)
dest=${1:-$HOME/.cache/fibber-scratch/tools/JSONTestSuite}
if [ ! -d "$dest/test_parsing" ]; then
  mkdir -p "$dest"
  tmp=$(mktemp -d)
  curl -sSL -m 300 -o "$tmp/jts.tar.gz" "https://codeload.github.com/nst/JSONTestSuite/tar.gz/$COMMIT"
  tar xzf "$tmp/jts.tar.gz" -C "$tmp" "JSONTestSuite-$COMMIT/test_parsing"
  mv "$tmp/JSONTestSuite-$COMMIT/test_parsing" "$dest/test_parsing"
  rm -rf "$tmp"
fi
(cd "$dest/test_parsing" && sha256sum -c --quiet "$here/../specs/json-testsuite.sha256") && echo "JSONTestSuite $COMMIT verified: $(ls "$dest/test_parsing" | wc -l) files in $dest/test_parsing"
