#!/bin/bash
# scripts/sync-lir.sh [--repo DIR] [COMMIT]: copies the paths that the file LIR names (path= lines) from COMMIT of the lir repository into
# fibber, exactly (each path is removed, then `git archive COMMIT -- PATHS` is unpacked over the root), and rewrites LIR's commit= (the full
# sha) and tree= (the sha256 of the copied files' manifest: scripts/lib/lir-manifest.sh). It then checks the copy against tree= and prints
# what changed. Nothing is staged: review `git status` and add LIR and the paths.
#   --repo DIR  a checkout (or bare clone) of the lir repository; COMMIT is any revision of it. Without --repo, LIR's url= is fetched into
#               FIB_LIR_CACHE (default ~/.cache/fibber-scratch/lir.git, a bare repository, every branch).
#   COMMIT      default: the commit LIR already names (puts the pinned copy back)
# A commit that no remote branch of DIR contains is copied with a warning: push it to the lir repository before fibber's commit is pushed.
# exit 0 when the copy matches, 1 when it does not, 2 on a usage or setup error.
set -eu -o pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/lib/lir-manifest.sh"
lir=$root/LIR
repo=""; commit=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo=${2:?--repo DIR}; shift 2 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    -*) echo "sync-lir: unknown option $1" >&2; exit 2 ;;
    *) [ -z "$commit" ] || { echo "sync-lir: one COMMIT only" >&2; exit 2; }; commit=$1; shift ;;
  esac
done
[ -f "$lir" ] || { echo "sync-lir: no $lir" >&2; exit 2; }
url=$(lir_field "$lir" url); [ -n "$commit" ] || commit=$(lir_field "$lir" commit)
paths=$(lir_paths "$lir")
[ -n "$paths" ] && [ -n "$commit" ] || { echo "sync-lir: $lir names no path= or no commit=" >&2; exit 2; }
if [ -z "$repo" ]; then
  repo=${FIB_LIR_CACHE:-$HOME/.cache/fibber-scratch/lir.git}
  [ -d "$repo" ] || git init -q --bare "$repo"
  echo "sync-lir: fetching $url into $repo" >&2
  git -C "$repo" fetch -q "$url" '+refs/heads/*:refs/heads/*' || { echo "sync-lir: fetch of $url failed" >&2; exit 2; }
fi
full=$(git -C "$repo" rev-parse --verify -q "$commit^{commit}") || { echo "sync-lir: $repo has no commit $commit" >&2; exit 2; }
for p in $paths; do
  [ -n "$(git -C "$repo" ls-tree "$full" -- "$p")" ] || { echo "sync-lir: $full of $repo has no $p (LIR's path= lines)" >&2; exit 2; }
done
remote=$(git -C "$repo" branch -r --contains "$full" 2> /dev/null || true)
if [ -z "$remote" ] && [ -n "$(git -C "$repo" remote)" ]; then
  echo "sync-lir: warning: no remote branch of $repo contains $full; push it before fibber's commit is pushed" >&2
fi
# shellcheck disable=SC2086
want=$(lir_manifest_commit "$repo" "$full" $paths | lir_sha256)
for p in $paths; do rm -rf "${root:?}/$p"; done
# shellcheck disable=SC2086
git -C "$repo" archive "$full" -- $paths | tar -x -C "$root"
awk -v c="$full" -v t="$want" '/^commit=/ { print "commit=" c; next } /^tree=/ { print "tree=" t; next } { print }' "$lir" > "$lir.new"
mv "$lir.new" "$lir"
# shellcheck disable=SC2086
got=$(lir_manifest_tree "$root" $paths | lir_sha256)
echo "sync-lir: $full ($(lir_manifest_commit "$repo" "$full" $paths | wc -l | tr -d ' ') files), tree $want"
# shellcheck disable=SC2086
changed=$(git -C "$root" status --short -- LIR $paths)
if [ -n "$changed" ]; then printf '%s\n' "$changed"; else echo "sync-lir: no change"; fi
if [ "$got" != "$want" ]; then
  echo "sync-lir: FAIL: the working tree's copy hashes to $got, not $want (an ignored or untracked file under the paths?)" >&2; exit 1
fi
# The copied files moved lines, so the reference pages that cite them (docs/reference/environment.md) are regenerated here, not
# found by the gate's docs stage three syncs later.
if command -v make > /dev/null && [ -f Makefile ]; then make doc-reference > /dev/null 2>&1 || echo "sync-lir: make doc-reference failed; run it by hand" >&2; fi
