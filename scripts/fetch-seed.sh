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
# A seed cache (FIB_SEED_CACHE, a directory the CI user can read: DIR/SHA256/seed.tar.gz) is tried first, so a runner without GitHub
# credentials, or without a network, still has its seed; a download is stored there when the directory is writable.
cached=
if [ -n "${FIB_SEED_CACHE:-}" ] && [ -f "$FIB_SEED_CACHE/$want/seed.tar.gz" ]; then
  cp "$FIB_SEED_CACHE/$want/seed.tar.gz" "$tarball" && cached=1
fi
if [ -z "$cached" ]; then
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
fi
got=$(sha256sum "$tarball" | cut -d' ' -f1)
if [ "$got" != "$want" ]; then
  echo "fetch-seed: sha256 mismatch for $url: got $got, SEED says $want" >&2
  rm -f "$tarball"
  exit 1
fi
if [ -z "$cached" ] && [ -n "${FIB_SEED_CACHE:-}" ] && mkdir -p "$FIB_SEED_CACHE/$want" 2>/dev/null; then
  cp "$tarball" "$FIB_SEED_CACHE/$want/seed.tar.gz" 2>/dev/null && chmod -R a+rX "$FIB_SEED_CACHE/$want" 2>/dev/null || true
fi
tar -xzf "$tarball" -C "$dir"
fibc=$(find "$dir" -path '*/bin/fibc' -type f | head -n 1)
if [ -z "$fibc" ]; then echo "fetch-seed: no bin/fibc in the tarball" >&2; exit 1; fi
# A seed compiler must read the library it is going to compile, not the one it was released with (a newer tree may use a prelude
# function the seed's library lacks, and an installed compiler prefers its bundled library): the tree's lib/ replaces the bundled one.
bundled=$(cd "$(dirname "$fibc")/.." && pwd)/share/fibber/lib
if [ -d "$bundled" ] && [ -d "$root/lib" ]; then rm -rf "$bundled" && cp -r "$root/lib" "$bundled"; fi
cd "$(dirname "$fibc")" && echo "$PWD/fibc"
