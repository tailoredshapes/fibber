#!/bin/bash
# Fetches the Unicode Character Database files the tables of fib.string.unicode and fib.regex.unicode are generated from, at the pinned
# version (16.0.0), and verifies each against specs/unicode-16.0.0.sha256 (ADR 0020: a script that downloads also checks a checksum).
# The gate never calls this: the generated tables are committed (lib/fib/string/unicode/data.fib, lib/fib/regex/unicode/scripts.fib).
# Usage: scripts/fetch-unicode.sh [DEST]   (default ~/.cache/fibber-scratch/tools/unicode-16.0.0); then
#        scripts/gen-unicode.py DEST       regenerates the tables; `git diff` must be empty.
set -eu
VERSION=16.0.0
here=$(cd "$(dirname "$0")" && pwd)
dest=${1:-$HOME/.cache/fibber-scratch/tools/unicode-$VERSION}
mkdir -p "$dest"
for f in UnicodeData.txt SpecialCasing.txt PropList.txt Scripts.txt; do
  [ -s "$dest/$f" ] || curl -sSL -m 120 -o "$dest/$f" "https://www.unicode.org/Public/$VERSION/ucd/$f"
done
(cd "$dest" && sha256sum -c --quiet "$here/../specs/unicode-$VERSION.sha256") && echo "Unicode $VERSION verified in $dest"
