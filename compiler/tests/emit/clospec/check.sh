#!/bin/bash
# compiler/tests/emit/clospec/check.sh FIBC: a function literal or a named function passed to a `defun` that calls it is called directly
# in the callee's specialisation, and a let-bound literal whose binding is only ever called is called directly (compiler/emit/lower/
# spec.fib, docs/design/vectorisation.md 7 item 4), so LLVM puts it in line.
# kernels.fib is run (its answers are fixed below) and built at -O 0 to LLVM IR; its `spec-` and `closure-` functions are kept from being
# inlined into main (so each one's body is its own), and `opt -O2` runs. Each `spec-` and `direct-` function must have no indirect call in
# its optimised body or in any function of the program it calls (`f.`, `m.`, `l.`), at any depth, and a `spec-` function no call of a
# literal's code (`l.`) in its own body either (LLVM decides whether to put in line a literal holding a loop of its own: `direct-inner`
# is not asked that); `closure-unknown` and `closure-let-kept` must have an indirect call (a closure the specialisation cannot see, and
# a let-bound literal that is also kept in a Vec, stay one: what the check would miss if it could not fail). Exit 1 on any difference. LLVM_BIN names opt (default: beside LLVM_LIBDIR).
# usage: check.sh FIBC     run from anywhere; FIB_LIB defaults to the tree's lib
fibc=${1:?usage: check.sh FIBC}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
bin=${LLVM_BIN:-$(dirname "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}")/bin}
out=$(mktemp -d "${TMPDIR:-/tmp}/clospec.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT
ulimit -v 16000000 2>/dev/null
got=$(timeout 300 "$fibc" run "$here/kernels.fib") || { echo "check.sh: run failed"; exit 1; }
want=$'9900.0\n22.5\n5050.0\n4950.0\n22.5\n9900.0\n14850.0\n9901.0\n0'
[ "$got" = "$want" ] || { echo "check.sh: kernels.fib answered"; echo "$got"; echo "check.sh: wanted"; echo "$want"; exit 1; }
timeout 300 "$fibc" build -O 0 "$here/kernels.fib" --emit llvm -o "$out/k.ll" || { echo "check.sh: build failed"; exit 1; }
python3 - "$out/k.ll" <<'PY' || exit 1
import re, sys
p = sys.argv[1]; out = []
for l in open(p):
    m = re.match(r'^define .*@"?(f\.(spec|direct|closure)-[^"(\s]+)"?\(', l)
    if m and l.rstrip().endswith('{'):
        l = re.sub(r'\)( [^)]*)?#(\d+) \{\s*$', lambda mm: ')' + (mm.group(1) or ' ') + 'noinline #' + mm.group(2) + ' {\n', l)
    out.append(l)
open(p, 'w').write(''.join(out))
PY
timeout 300 "$bin/opt" -S -passes='default<O2>' "$out/k.ll" -o "$out/k.opt.ll" 2>/dev/null || { echo "check.sh: opt failed ($bin/opt)"; exit 1; }
python3 - "$out/k.opt.ll" <<'PY'
import re, sys
bodies, cur = {}, None
for l in open(sys.argv[1]):
    m = re.match(r'^define .*@"?([^"(\s]+)"?\(', l)
    if m: cur = m.group(1); bodies[cur] = ''
    elif l.startswith('}'): cur = None
    elif cur: bodies[cur] += l
names = sorted(n for n in bodies if re.match(r'f\.(spec|direct|closure)-', n))
def callees(b):
    return set(m.group(1) for m in re.finditer(r'\bcall [^@%]*@"?([fml]\.[^"(\s]+)"?\(', b))
def indirect(n):
    seen, work = set(), [n]
    while work:
        f = work.pop()
        if f in seen or f not in bodies: continue
        seen.add(f)
        if re.search(r'\bcall [^@%(]*%[\w.]+\(', bodies[f]): return f
        work.extend(callees(bodies[f]))
    return None
bad = 0
for n in names:
    via = indirect(n)
    lits = sorted(c for c in callees(bodies[n]) if c.startswith('l.'))
    if n.startswith('f.spec-'):
        ok = via is None and not lits
        why = ('an indirect call in ' + via if via else '') + ((' ' if via else '') + 'a call of ' + lits[0] if lits else '')
        print(('ok   ' if ok else 'FAIL ') + n + ': ' + ('direct, the literal in line' if ok else why + ' (wanted direct, no literal called)'))
    elif n.startswith('f.direct-'):
        ok = via is None
        print(('ok   ' if ok else 'FAIL ') + n + ': ' + ('direct' if ok else 'an indirect call in ' + via + ' (wanted direct)'))
    else:
        ok = via is not None
        print(('ok   ' if ok else 'FAIL ') + n + ': ' + ('an indirect call' if ok else 'direct (wanted an indirect call)'))
    bad |= not ok
if len(names) != 8:
    print('FAIL expected 8 functions, found %d' % len(names)); bad = 1
print('check.sh: %s' % ('every function as wanted' if not bad else 'DIFFERENCES'))
sys.exit(1 if bad else 0)
PY
