#!/bin/bash
# The Rust is frozen as the seed (tag seed-1); the data its tests read has moved on. Replaces cases/, lib/, spec/ and compiler/ with the
# seed's, and the one Rust test file that counts the rows of the stdlib table in spec/ (its count follows the spec), so that the
# Rust job judges the Rust against the data of its own time. The bootstrap reader compares compiler/ too: newer syntax belongs
# to the stage2 job, not the frozen Rust oracle. Needs the tag: check out with fetch-depth 0 and fetch-tags.
set -eu
cd "$(dirname "$0")/.."
git rev-parse --verify -q 'seed-1^{commit}' > /dev/null || { echo "ci-seed-overlay: no tag seed-1 (fetch tags)" >&2; exit 1; }
rm -rf cases lib spec compiler
git checkout seed-1 -- cases lib spec compiler crates/fibref/tests/stdlib_table/rows.rs
echo "overlaid seed-1: cases lib spec compiler crates/fibref/tests/stdlib_table/rows.rs"
