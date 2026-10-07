#!/bin/bash
# The differential test of fib.regex against java.util.regex (package LIB-2): generated patterns (the shared subset: classes, groups, quantifiers, anchors, the
# inline flags i s m x, single-digit backreferences, \x{..}, and \p{..} of fib.regex.unicode) over generated texts (ASCII, line terminators, accented letters,
# Greek, Arabic-Indic digits, a supplementary character), find and matches; Java's answer (the leftmost match, its code point start, every group) must equal
# fibber's. Fixed seeds: the same run is the same lines. Usage: scripts/regex-diff/run.sh [SEED [COUNT [--unicode]]]; needs a JDK (java Ref.java) and an F
# (FIBC=path, default `fibc`). Prints the counts: lines, agreed, budget (searches that spent the backtracking budget, not compared), disagreements (listed).
set -u
here=$(cd "$(dirname "$0")" && pwd)
seed=${1:-1}; count=${2:-2000}; uni=${3:-}
fibc=${FIBC:-fibc}
java=${JAVA:-java}
tmp=${TMPDIR:-/tmp}/regex-diff.$$
mkdir -p "$tmp"
python3 "$here/gen.py" "$seed" "$count" $uni > "$tmp/cases.txt"
"$java" "$here/Ref.java" "$tmp/cases.txt" "$tmp/java.out" || { echo "java failed"; exit 2; }
"$fibc" run "$here/harness.fib" -- "$tmp/cases.txt" > "$tmp/fib.raw" || { echo "fibc failed"; exit 2; }
sed '$d' "$tmp/fib.raw" > "$tmp/fib.out"      # the last line is main's result
lines=$(wc -l < "$tmp/cases.txt")
python3 - "$tmp/cases.txt" "$tmp/java.out" "$tmp/fib.out" <<'EOF'
import sys
cases = open(sys.argv[1]).read().split('\n')
java = open(sys.argv[2]).read().split('\n')
fib = open(sys.argv[3]).read().split('\n')
agree = budget = bad = 0
kinds = {'ERR': 0, 'NO': 0, 'match': 0}
shown = 0
for i, c in enumerate(cases):
    if not c: continue
    j, f = java[i], fib[i] if i < len(fib) else '<missing>'
    if f == 'BUDGET': budget += 1; continue
    if j == f:
        agree += 1
        kinds['ERR' if j == 'ERR' else 'NO' if j == 'NO' else 'match'] += 1
        continue
    bad += 1
    if shown < 40:
        shown += 1
        mode, p, t = c.split(' ')
        d = lambda h: '' if h == '-' else bytes.fromhex(h).decode()
        print('DISAGREE %s pattern %r text %r\n   java: %s\n   fib:  %s' % (mode, d(p), d(t), j, f))
print('lines %d agreed %d (both refuse %d, no match %d, match %d) budget %d disagreements %d' % (agree + budget + bad, agree, kinds['ERR'], kinds['NO'], kinds['match'], budget, bad))
sys.exit(1 if bad else 0)
EOF
rc=$?
rm -rf "$tmp"
exit $rc
