#!/bin/bash
# The differential test of fib.decimal against java.math.BigDecimal (package LIB-2): COUNT seeded random operations (add subtract multiply compareTo equals divide
# with a MathContext, with a scale, and exact, setScale, round, pow, stripTrailingZeros, toPlainString, precision, parse and print, the exact and the shortest
# conversions from double, the conversions to double and long), each result compared as text. Usage: scripts/decimal-diff/run.sh [SEED [COUNT]];
# needs a JDK (java Ref.java) and an F (FIBC=path, default `fibc`). Prints the number of operations and of disagreements (the first ones listed).
set -u
here=$(cd "$(dirname "$0")" && pwd)
seed=${1:-1}; count=${2:-100000}
fibc=${FIBC:-fibc}
java=${JAVA:-java}
tmp=${TMPDIR:-/tmp}/decimal-diff.$$
mkdir -p "$tmp"
python3 "$here/gen.py" "$seed" "$count" > "$tmp/ops.txt"
"$java" "$here/Ref.java" "$tmp/ops.txt" "$tmp/java.out" || { echo "java failed"; exit 2; }
"$fibc" run "$here/harness.fib" -- "$tmp/ops.txt" > "$tmp/fib.raw" || { echo "fibc failed (a trap ends the run: the last line of $tmp/fib.raw names where)"; tail -3 "$tmp/fib.raw"; exit 2; }
sed '$d' "$tmp/fib.raw" > "$tmp/fib.out"
python3 - "$tmp/ops.txt" "$tmp/java.out" "$tmp/fib.out" <<'EOF'
import sys
ops = open(sys.argv[1]).read().split('\n')
java = open(sys.argv[2]).read().split('\n')
fib = open(sys.argv[3]).read().split('\n')
bad = n = 0
for i, o in enumerate(ops):
    if not o: continue
    n += 1
    if java[i] != fib[i]:
        bad += 1
        if bad <= 25: print('DISAGREE %s\n   java: %s\n   fib:  %s' % (o, java[i], fib[i]))
print('operations %d disagreements %d' % (n, bad))
sys.exit(1 if bad else 0)
EOF
rc=$?
rm -rf "$tmp"
exit $rc
