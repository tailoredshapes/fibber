#!/bin/bash
# HISTORICAL, run once on 2026-10-05 (RR1, retiring the Rust): recorded the goldens of this directory from the Rust oracles of tag seed-1,
# which are not in the tree any more (docs/rust-legacy.md says how to build them: git checkout seed-1, cargo build -p fibref -p fibc).
# For each suite of suites.sh and each input: the Rust command and the stage 2 tool both run; the input joins the suite (NAME.files) and the
# Rust's output is its golden block only if the two outputs and exit statuses are equal byte for byte. The others are listed in
# RECORDED.txt: stage 2 and the Rust disagreed on them (a language the seed does not know, a prelude that moved), so no Rust-recorded golden exists.
# usage: record-from-rust.sh RUSTBINDIR TOOLSDIR [SUITE..]      (from anywhere; inputs one at a time, each under timeout 60 and ulimit -v)
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
rs=${1:?RUSTBINDIR (fibref, fibc of the seed-1 build)}; tools=${2:?TOOLSDIR}; shift 2
. "$here/suites.sh"
want=" $* "; unset FIB_LIB; export FIB_TARGET_CPU=x86-64-v2
renice -n 10 $$ > /dev/null 2>&1
cd "$root" || exit 2
tmp=${TMPDIR:-/tmp}; : > "$here/RECORDED.txt"
blk() { # blk FILE CMD..: the block of an input; the command's output with the tree's path made <root>
  local f=$1; shift
  echo "#### $f"
  (ulimit -v 8000000; timeout 60 "$@" 2>&1; echo "status $?") | sed "s|$root|<root>|g"
}
while IFS='|' read -r name tool opts rcmd globs; do
  [ -z "$name" ] && continue
  if [ "$want" != "  " ] && [[ "$want" != *" $name "* ]]; then continue; fi
  : > "$here/$name.files"; : > "$here/$name.golden"; left=(); n=0
  # shellcheck disable=SC2086
  for f in $(ls -1 $globs 2>/dev/null | LC_ALL=C sort); do
    n=$((n + 1))
    read -r bin sub <<< "$rcmd"
    blk "$f" "$rs/$bin" $sub $opts "$f" > "$tmp/r.out"
    FIB_LIB=$root/lib blk "$f" "$tools/$tool" $opts "$f" > "$tmp/t.out"
    if cmp -s "$tmp/r.out" "$tmp/t.out"; then echo "$f" >> "$here/$name.files"; shape "$name" "$tmp/r.out" "$f" >> "$here/$name.golden"; else left+=("$f"); fi
  done
  echo "$name: $n inputs, $((n - ${#left[@]})) recorded, ${#left[@]} left out" | tee -a "$here/RECORDED.txt"
  for f in "${left[@]}"; do echo "  left out: $f" >> "$here/RECORDED.txt"; done
done < <(suite_table)
