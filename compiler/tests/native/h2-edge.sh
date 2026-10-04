#!/bin/bash
# The reader's edge inputs (compiler/tests/native/h2-edge/): unicode columns, comments, commas, CRLF, nesting at and past the limit, strings and vector
# types that fail, and a parse error before a structural or a lexical one (the Rust reads every form before it parses any, so the later error wins).
# Each file is compared with the Rust lair twice: `dump-ast` and `check`, byte for byte. Exit 0 only when every file is SAME.
# usage: h2-edge.sh        run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf
here=$(cd "$(dirname "$0")" && pwd)
bad=0
for n in ast check; do
  "$here/compare-$n.sh" -j 4 "$here/h2-edge" | grep -v '^SAME'
  [ "${PIPESTATUS[0]}" -eq 0 ] || bad=1
done
exit $bad
