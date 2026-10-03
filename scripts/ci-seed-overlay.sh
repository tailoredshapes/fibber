#!/bin/bash
# The Rust is frozen as the seed (tag seed-1); the data its tests read has moved on. Replaces cases/, lib/ and spec/ with the
# seed's, and the one Rust test file that counts the rows of the stdlib table in spec/ (its count follows the spec), so that the
# Rust job judges the Rust against the data of its own time. Needs the tag: check out with fetch-depth 0 and fetch-tags.
set -eu
cd "$(dirname "$0")/.."
git rev-parse --verify -q 'seed-1^{commit}' > /dev/null || { echo "ci-seed-overlay: no tag seed-1 (fetch tags)" >&2; exit 1; }
rm -rf cases lib spec
git checkout seed-1 -- cases lib spec crates/fibref/tests/stdlib_table/rows.rs
echo "overlaid seed-1: cases lib spec crates/fibref/tests/stdlib_table/rows.rs"
