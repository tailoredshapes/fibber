#!/bin/bash
# scripts/lib/lir-manifest.sh: sourced by scripts/sync-lir.sh and scripts/lir-drift.sh. The manifest of fibber's pinned copy of the lir repository
# (the file LIR at the root): one `MODE SHA1 PATH` line per file under LIR's path= lines, MODE 100755 or 100644, SHA1 git's blob id of the
# contents, sorted by PATH (LC_ALL=C). Its sha256 is LIR's tree=. The same manifest comes from a commit of the lir repository (git ls-tree)
# and from fibber's working tree (git hash-object of each file, tracked or untracked but not ignored), so the two are compared offline.

# lir_field LIRFILE NAME: the value of the first NAME= line
lir_field() { sed -n "s/^$2=//p" "$1" | sed -n 1p; }
# lir_paths LIRFILE: the path= values, one per line
lir_paths() { sed -n 's/^path=//p' "$1"; }
# lir_sha256: the sha256 of standard input, 64 hex digits
lir_sha256() {
  if command -v sha256sum > /dev/null 2>&1; then sha256sum | cut -d' ' -f1; else shasum -a 256 | cut -d' ' -f1; fi
}
# lir_manifest_commit REPO COMMIT PATH..: the manifest of the paths at COMMIT of the git repository REPO
lir_manifest_commit() {
  local repo=$1 commit=$2; shift 2
  git -C "$repo" ls-tree -r "$commit" -- "$@" | awk -F'\t' '{ split($1, a, " "); print a[1], a[3], $2 }' | LC_ALL=C sort -k3
}
# lir_manifest_tree ROOT PATH..: the manifest of the paths as they are in the working tree of ROOT (a file deleted but still tracked is absent)
lir_manifest_tree() {
  local root=$1; shift
  (
    cd "$root" || exit 2
    git ls-files -co --exclude-standard -- "$@" | LC_ALL=C sort -u | while IFS= read -r f; do
      [ -f "$f" ] || continue
      if [ -x "$f" ]; then mode=100755; else mode=100644; fi
      printf '%s %s %s\n' "$mode" "$(git hash-object -- "$f")" "$f"
    done
  ) | LC_ALL=C sort -k3
}
