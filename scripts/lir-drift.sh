#!/bin/bash
# scripts/lir-drift.sh [ROOT]: fibber's copy of the lir repository is the commit LIR names, unchanged. The manifest of the paths of LIR (path=) as
# they are in ROOT's working tree (every file, tracked or untracked but not ignored: scripts/lib/lir-manifest.sh) must hash to LIR's tree=; an
# edited, added, removed or re-moded file is a failure. The copy is read-only: a change goes to the lir repository first, then
# scripts/sync-lir.sh brings it here. Offline: no lir checkout is needed. Then the plants, so the check can fail: the same manifest with one
# file's contents changed by a byte, with an extra file, and with a file missing must each hash to something other than tree=.
# build/tools/sh-lir-drift.ok runs it (mk/tools.mk, the quick gate).
# exit 0 when the copy matches and the plants are caught, 1 otherwise, 2 on a setup error.
set -eu -o pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "${1:-$here/..}" && pwd)
. "$here/lib/lir-manifest.sh"
lir=$root/LIR
[ -f "$lir" ] || { echo "lir-drift: no $lir" >&2; exit 2; }
want=$(lir_field "$lir" tree); commit=$(lir_field "$lir" commit); paths=$(lir_paths "$lir")
[ "${#want}" = 64 ] && [ -n "$paths" ] || { echo "lir-drift: $lir has no tree= of 64 hex digits or no path=" >&2; exit 2; }
# shellcheck disable=SC2086
manifest=$(lir_manifest_tree "$root" $paths)
[ -n "$manifest" ] || { echo "lir-drift: no file under LIR's paths" >&2; exit 2; }
got=$(printf '%s\n' "$manifest" | lir_sha256)
n=$(printf '%s\n' "$manifest" | wc -l | tr -d ' ')
fail=0
if [ "$got" = "$want" ]; then
  echo "lir-drift: $n files equal the lir repository at $commit (tree $want)"
else
  fail=1
  echo "FAIL lir-drift: the copy of the lir repository differs from LIR (tree $want at $commit; the working tree's is $got)"
  # shellcheck disable=SC2086
  dirty=$(git -C "$root" status --short -- $paths || true)
  [ -z "$dirty" ] || { echo "  uncommitted under LIR's paths:"; printf '%s\n' "$dirty" | sed 's/^/    /'; }
  since=$(git -C "$root" log -1 --format=%H -- LIR 2> /dev/null || true)
  if [ -n "$since" ]; then
    # shellcheck disable=SC2086
    commits=$(git -C "$root" log --format='%h %s' "$since..HEAD" -- $paths || true)
    [ -z "$commits" ] || { echo "  commits since LIR last changed that touch its paths:"; printf '%s\n' "$commits" | sed 's/^/    /'; }
  fi
  echo "  the copy is read-only: change the lir repository, then scripts/sync-lir.sh --repo DIR COMMIT (or scripts/sync-lir.sh to put the pin back)"
fi
# the plants: each altered manifest must not hash to tree= (the comparison sees contents, extra files and missing files)
first=$(printf '%s\n' "$manifest" | sed -n 1p)
f=${first#* * }
byte=$( { cat "$root/$f"; printf 'x'; } | git hash-object --stdin)
plant_edit=$(printf '%s\n' "$manifest" | awk -v s="$byte" 'NR == 1 { $2 = s } { print }')
plant_extra=$(printf '%s\n%s\n' "$manifest" "100644 $byte ${f%/*}/planted-extra.fib" | LC_ALL=C sort -k3)
plant_missing=$(printf '%s\n' "$manifest" | sed 1d)
for p in edit extra missing; do
  case $p in edit) m=$plant_edit ;; extra) m=$plant_extra ;; missing) m=$plant_missing ;; esac
  h=$(printf '%s\n' "$m" | lir_sha256)
  if [ "$h" = "$got" ]; then echo "FAIL lir-drift plant $p: the planted manifest hashes as the real one"; fail=1; fi
done
[ "$fail" = 1 ] || echo "lir-drift plants: an edited byte ($f), an extra file and a missing file each change the hash"
exit "$fail"
