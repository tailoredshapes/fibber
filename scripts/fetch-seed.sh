#!/bin/bash
# Downloads the seed fibc named by the file SEED, verifies its sha256 and unpacks it. Prints the path of its bin/fibc on standard output
# (everything else goes to standard error), so:  FIBC=$(scripts/fetch-seed.sh seed-dir) scripts/package.sh
# usage: scripts/fetch-seed.sh DIR [SEEDFILE]        SEEDFILE defaults to the SEED at the repository root
# SEED holds two lines: `url=...` (a release tarball; any curl URL) and `sha256=...` (64 hex digits). A seed is the tarball of an earlier
# release: the Rust tools (v0.0.x, scripts/package-rust.sh) or a stage 2 (scripts/package.sh); both have bin/fibc.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dir=${1:?usage: fetch-seed.sh DIR [SEEDFILE]}
seed=${2:-$root/SEED}
url=$(sed -n 's/^url=//p' "$seed")
want=$(sed -n 's/^sha256=//p' "$seed")
if [ -z "$url" ]; then echo "fetch-seed: no url= line in $seed" >&2; exit 2; fi
case $want in
  [0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]*) ;;
  *) echo "fetch-seed: $seed has no sha256 yet ([$want]); release the seed, then record its sha256" >&2; exit 2 ;;
esac
if [ "${#want}" -ne 64 ]; then echo "fetch-seed: sha256 in $seed is not 64 hex digits" >&2; exit 2; fi
mkdir -p "$dir"
tarball=$dir/seed.tar.gz
# A private repository's release assets need credentials: with gh, ask GitHub for the asset of a github.com release URL
# (OWNER/REPO/releases/download/TAG/FILE); otherwise (a public URL, any other host) curl does it.
case $url in
  https://github.com/*/releases/download/*) gh_ok=1 ;;
  *) gh_ok= ;;
esac
if [ -n "$gh_ok" ] && command -v gh >/dev/null 2>&1; then
  rest=${url#https://github.com/}
  repo=${rest%%/releases/*}; rest=${rest#*/releases/download/}; tag=${rest%%/*}; file=${rest#*/}
  rm -f "$tarball"
  gh release download "$tag" -R "$repo" -p "$file" -O "$tarball" >&2
else
  curl -fsSL --retry 3 -o "$tarball" "$url" >&2
fi
got=$(sha256sum "$tarball" | cut -d' ' -f1)
if [ "$got" != "$want" ]; then
  echo "fetch-seed: sha256 mismatch for $url: got $got, SEED says $want" >&2
  rm -f "$tarball"
  exit 1
fi
tar -xzf "$tarball" -C "$dir"
fibc=$(find "$dir" -path '*/bin/fibc' -type f | head -n 1)
if [ -z "$fibc" ]; then echo "fetch-seed: no bin/fibc in the tarball" >&2; exit 1; fi
cd "$(dirname "$fibc")" && echo "$PWD/fibc"
