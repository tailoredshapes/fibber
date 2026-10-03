#!/bin/sh
# Fails if the file VERSION at the repository root and the constant in compiler/driver/version.fib disagree.
# usage: scripts/check-version.sh      (from anywhere)
root=$(cd "$(dirname "$0")/.." && pwd)
file=$(tr -d '[:space:]' < "$root/VERSION")
const=$(sed -n 's/^(def version: str "\([^"]*\)")$/\1/p' "$root/compiler/driver/version.fib")
if [ -z "$file" ] || [ -z "$const" ]; then
  echo "check-version: cannot read the version (VERSION: [$file], version.fib: [$const])" >&2
  exit 2
fi
if [ "$file" != "$const" ]; then
  echo "check-version: VERSION says $file but compiler/driver/version.fib says $const" >&2
  exit 1
fi
echo "version $file"
