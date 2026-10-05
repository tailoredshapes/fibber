#!/bin/bash
# Test of scripts/fetch-seed.sh's per-platform SEED table (A64-1): builds two tiny "release" tarballs whose bin/fibc says which platform it is for, a SEED table
# with a row for each (file:// urls, so no network), and checks that each platform picks its own row, that the legacy unqualified rows are linux-x86_64's only,
# that a platform with no row fails with a message that names it, and that a wrong sha256 is refused. Each check has its planted fault.
# usage: scripts/tests/fetch-seed.sh      Exit 0 when every check holds.
set -u
root=$(cd "$(dirname "$0")/../.." && pwd)
T=$(mktemp -d "${TMPDIR:-/tmp}/fetch-seed.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
sum() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
for p in linux-x86_64 darwin-arm64; do
  mkdir -p "$T/pkg-$p/fibc-9/bin"; printf '#!/bin/sh\necho %s\n' "$p" > "$T/pkg-$p/fibc-9/bin/fibc"; chmod +x "$T/pkg-$p/fibc-9/bin/fibc"
  tar -C "$T/pkg-$p" -czf "$T/$p.tar.gz" fibc-9
done
lsum=$(sum "$T/linux-x86_64.tar.gz"); dsum=$(sum "$T/darwin-arm64.tar.gz")
printf 'url.linux-x86_64=file://%s/linux-x86_64.tar.gz\nsha256.linux-x86_64=%s\nurl.darwin-arm64=file://%s/darwin-arm64.tar.gz\nsha256.darwin-arm64=%s\n' "$T" "$lsum" "$T" "$dsum" > "$T/SEED.table"
printf 'url=file://%s/linux-x86_64.tar.gz\nsha256=%s\n' "$T" "$lsum" > "$T/SEED.legacy"
run() { # PLATFORM SEEDFILE DIR: prints what the fetched fibc says, or ERROR: and the message
  local out; out=$(FIB_PLATFORM=$1 "$root/scripts/fetch-seed.sh" "$3" "$2" 2> "$T/err") || { echo "ERROR: $(tr '\n' ' ' < "$T/err")"; return; }
  "$out"
}
r=$(run linux-x86_64 "$T/SEED.table" "$T/d1"); [ "$r" = linux-x86_64 ] && ok "table: linux-x86_64 takes its own row" || no "table, linux-x86_64: [$r]"
r=$(run darwin-arm64 "$T/SEED.table" "$T/d2"); [ "$r" = darwin-arm64 ] && ok "table: darwin-arm64 takes its own row" || no "table, darwin-arm64: [$r]"
r=$(run linux-x86_64 "$T/SEED.legacy" "$T/d3"); [ "$r" = linux-x86_64 ] && ok "legacy url=/sha256= rows are linux-x86_64's" || no "legacy, linux-x86_64: [$r]"
r=$(run darwin-arm64 "$T/SEED.legacy" "$T/d4"); case $r in ERROR:*darwin-arm64*) ok "legacy rows are not used for darwin-arm64 ($r)" ;; *) no "legacy file served darwin-arm64: [$r]" ;; esac
r=$(run linux-aarch64 "$T/SEED.table" "$T/d5"); case $r in ERROR:*"no seed for linux-aarch64"*) ok "a platform with no row fails and says which" ;; *) no "unknown platform: [$r]" ;; esac
sed "s/^sha256.darwin-arm64=.*/sha256.darwin-arm64=$lsum/" "$T/SEED.table" > "$T/SEED.wrong"
r=$(run darwin-arm64 "$T/SEED.wrong" "$T/d6"); case $r in ERROR:*"sha256 mismatch"*) ok "a wrong sha256 is refused" ;; *) no "planted wrong sha256 accepted: [$r]" ;; esac
echo "fetch-seed: $bad failure(s)"; exit $bad
