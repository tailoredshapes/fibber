#!/bin/bash
# The fib.sql differential test against HoneySQL (docs/design/observability-and-databases.md 5.6): every query of
# scripts/tests/sql/corpus.edn is formatted by real HoneySQL and by fib.sql, with no quoting, ANSI quoting and MySQL quoting, and the
# lines (SQL text and every parameter with its kind) must be identical.
# Usage: FIBC=/path/to/stage2 CLJ_CP=clojure.jar:spec.alpha.jar:core.specs.alpha.jar:honeysql.jar scripts/test-sql.sh
# The jars used for the design (2026-10-06): Clojure 1.12.0 (spec.alpha 0.5.238, core.specs.alpha 0.4.74) from repo1.maven.org, and
# HoneySQL 2.7.1479 from https://repo.clojars.org/com/github/seancorfield/honeysql/2.7.1479/honeysql-2.7.1479.jar
# (sha1 cbbe0d84b096e6db0adc177b20f16e1cde5eceee, sha256 b9c0c6c582fc1c78011afc519257d1c7e2a23911b618ab57e1b23494d0050290).
set -euo pipefail
cd "$(dirname "$0")/.."
fibc=${FIBC:?set FIBC to a stage 2 fibc}
cp=${CLJ_CP:?set CLJ_CP to the Clojure and HoneySQL jars, colon separated}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/fib-sql.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
python3 scripts/tests/sql/gen.py scripts/tests/sql/corpus.edn > "$scratch/diff.fib"
"$fibc" build "$scratch/diff.fib" -I lib -o "$scratch/diff"
"$scratch/diff" > "$scratch/fib.out"
java -cp "$cp" clojure.main scripts/tests/sql/oracle.clj scripts/tests/sql/corpus.edn > "$scratch/honeysql.out"
if diff "$scratch/honeysql.out" "$scratch/fib.out"; then
  echo "test-sql: $(wc -l < "$scratch/fib.out") lines identical ($(grep -c . scripts/tests/sql/corpus.edn) queries x 3 dialects)"
else
  echo "test-sql: FAILED (< HoneySQL, > fib.sql)"; exit 1
fi
