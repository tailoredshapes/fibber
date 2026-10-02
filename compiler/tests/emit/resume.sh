#!/bin/bash
# The judge of compiler/emit/resume.fib: the hand-built builders of unit-resume.fib through emit.resume,
# against the same builders through the Rust resume.rs, byte for byte (spec/method.md: the Rust is the
# oracle). The Rust runs from a scratch crate of copies of ir.rs, value.rs and resume.rs, so this needs no
# LLVM and no cargo, only rustc. The copy of resume.rs has the two hash tables whose iteration order reaches
# the text made ordered (HashSet -> BTreeSet, the edge_loads HashMap -> BTreeMap), the intended behaviour
# that emit.resume implements (its header: ORDER); once resume.rs is ordered the seds change nothing.
# usage: resume.sh FIBC LIBLAIR_DIR [SCRATCH]    (FIBC: a fibc binary; LIBLAIR_DIR: the directory of liblair.so)
set -euo pipefail
FIBC=${1:?fibc}; LAIR=${2:?dir of liblair.so}; S=${3:-$HOME/.cache/fibber-scratch/E9r/oracle}
R=$(cd "$(dirname "$0")/../../.." && pwd)
mkdir -p "$S/src"
cp "$R"/crates/fibc/src/{ir,value,resume}.rs "$S/src/"
cp "$R/compiler/tests/emit/resume-oracle.rs" "$S/src/main.rs"
sed -i -e 's/HashSet/BTreeSet/g' -e '/let mut edge_loads/ s/HashMap/BTreeMap/g' \
    -e 's/^use std::collections::{HashMap, BTreeSet};/use std::collections::{BTreeMap, BTreeSet, HashMap};/' "$S/src/resume.rs"
rustc --edition 2021 -A warnings -o "$S/oracle" "$S/src/main.rs"
"$S/oracle" > "$S/rust.out"
(cd "$R" && "$FIBC" build compiler/tests/emit/unit-resume.fib -I compiler -I lib -L "$LAIR" -l lair -o "$S/resume-test")
"$S/resume-test" cases > "$S/fib.out"
"$S/resume-test" > "$S/checks.out" || { tail -5 "$S/checks.out"; exit 1; }
cmp "$S/rust.out" "$S/fib.out" && echo "resume: $(grep -c '^==' "$S/rust.out") cases byte for byte equal to the Rust; $(grep -c '^ok' "$S/checks.out") checks ok"
