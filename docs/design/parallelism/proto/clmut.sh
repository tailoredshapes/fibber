#!/bin/bash
# clmut.sh: plant faults in the lIR Chase-Lev deque and show the stress test fails (dup > 0 or lost > 0); 20 runs each
cd /home/tmarsh/.cache/fibber-scratch/par/w
L=/home/tmarsh/.cache/fibber-scratch/SIMD2-P0/lairf
run() { for i in $(seq 1 20); do timeout 60 $L run -O 2 $1 2>&1 | tail -1; done | sort | uniq -c | sort -rn | head -4; }
echo "== original"; run cl.lir
# M1: steal takes the item without the cmpxchg on top (a plain store of top+1)
python3 - <<'EOF'
s = open('cl.lir').read()
a = s.index('(DEFINE (steal I64)')
b = s.index('(DEFINE (take VOID)')
steal = s[a:b]
steal2 = steal.replace('(r (CMPXCHG SEQ_CST MONOTONIC (GLOBAL top) t (ADD t (I64 1)))))\n      (RET (SELECT (EXTRACTVALUE r 1) x (I64 0))))))',
                       ')\n      (ATOMIC-STORE MONOTONIC (ADD t (I64 1)) (GLOBAL top))\n      (RET x))))')
assert steal != steal2
open('cl.m1.lir', 'w').write(s[:a] + steal2 + s[b:])
# M2: pop's fence seq_cst weakened to release (the owner's bottom store may be seen after the thief's top load)
open('cl.m2.lir', 'w').write(s.replace('(ATOMIC-STORE MONOTONIC b (GLOBAL bottom))\n      (FENCE SEQ_CST)', '(ATOMIC-STORE MONOTONIC b (GLOBAL bottom))\n      (FENCE RELEASE)'))
# M3: pop of the last element does not race for it (no cmpxchg)
a = s.index('(BLOCK last')
b = s.index('(DEFINE (steal I64)')
open('cl.m3.lir', 'w').write(s[:a] + '(BLOCK last\n    (ATOMIC-STORE MONOTONIC (ADD b (I64 1)) (GLOBAL bottom))\n    (RET x)))\n\n' + s[b:])
# M4: steal's seq_cst fence between the loads of top and bottom removed
a = s.index('(DEFINE (steal I64)')
open('cl.m4.lir', 'w').write(s[:a] + s[a:].replace('(FENCE SEQ_CST)', '', 1))
EOF
for m in m1 m2 m3 m4; do echo "== mutant $m"; $L check cl.$m.lir 2>&1 | head -3; run cl.$m.lir; done
