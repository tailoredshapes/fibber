#!/bin/bash
# compiler/tests/expand/fuseinline/check.sh FIBC: the counted loops the expander writes for a fused `map` with its stage in line
# (compiler/expand/fuseinline.fib, docs/design/vec-performance.md item 2) are loops LLVM vectorises. kernels.fib is built at -O 0
# to LLVM IR, its `kern-` `hand-` and `closure-` functions are kept from being inlined into main (so each one's remarks are its own),
# and `opt -O2` runs with the loop vectoriser's remarks. Each `kern-` and `hand-` function must have a `Vectorized` remark and a
# vector fmul or fadd in its optimised body; `closure-` must have neither (a stage that is a function value stays a call: that is
# what the check would miss if it could not fail). Exit 1 on any difference. LLVM_BIN names opt (default: beside LLVM_LIBDIR).
# usage: check.sh FIBC     run from anywhere; FIB_LIB defaults to the tree's lib
fibc=${1:?usage: check.sh FIBC}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
bin=${LLVM_BIN:-$(dirname "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}")/bin}
out=$(mktemp -d "${TMPDIR:-/tmp}/fuseinline.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT
ulimit -v 16000000 2>/dev/null
timeout 300 "$fibc" build -O 0 "$here/kernels.fib" --emit llvm -o "$out/k.ll" || { echo "check.sh: build failed"; exit 1; }
python3 - "$out/k.ll" <<'PY' || exit 1
import re, sys
p = sys.argv[1]; out = []
for l in open(p):
    m = re.match(r'^define .*@"?(f\.(kern|hand|closure)-[^"(\s]+)"?\(', l)
    if m and l.rstrip().endswith('{'):
        l = re.sub(r'\)( [^)]*)?#(\d+) \{\s*$', lambda mm: ')' + (mm.group(1) or ' ') + 'noinline #' + mm.group(2) + ' {\n', l)
    out.append(l)
open(p, 'w').write(''.join(out))
PY
timeout 300 "$bin/opt" -S -passes='default<O2>' -pass-remarks-output="$out/k.yaml" "$out/k.ll" -o "$out/k.opt.ll" 2>/dev/null \
  || { echo "check.sh: opt failed ($bin/opt)"; exit 1; }
python3 - "$out/k.yaml" "$out/k.opt.ll" <<'PY'
import re, sys
remarks = open(sys.argv[1]).read().split('--- ')
vectorised = set()
for d in remarks:
    fn = re.search(r'Function:\s+(\S+)', d)
    if fn and 'Pass:            loop-vectorize' in d and re.search(r'Name:\s+Vectorized', d):
        vectorised.add(fn.group(1).strip("'\""))
bodies, cur = {}, None
for l in open(sys.argv[2]):
    m = re.match(r'^define .*@"?([^"(\s]+)"?\(', l)
    if m: cur = m.group(1); bodies[cur] = ''
    elif l.startswith('}'): cur = None
    elif cur: bodies[cur] += l
names = sorted(n for n in bodies if re.match(r'f\.(kern|hand|closure)-', n))
bad = 0
for n in names:
    vec_op = re.search(r'\bf(mul|add)( \w+)* <\d+ x double>', bodies[n]) is not None
    got = n in vectorised and vec_op
    want = not n.startswith('f.closure-')
    print(('ok   ' if got == want else 'FAIL ') + n + ': ' + ('vectorised' if got else 'not vectorised') + ('' if got == want else ' (wanted ' + ('vectorised' if want else 'not vectorised') + ')'))
    bad |= got != want
if len(names) != 8:
    print('FAIL expected 8 kernels, found %d' % len(names)); bad = 1
print('check.sh: %s' % ('every kernel as wanted' if not bad else 'DIFFERENCES'))
sys.exit(1 if bad else 0)
PY
