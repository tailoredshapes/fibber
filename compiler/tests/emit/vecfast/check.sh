#!/bin/bash
# compiler/tests/emit/vecfast/check.sh FIBC: the element read, write and append of a Vec are small fast paths that LLVM puts in line, with
# the rest out of line (lib/prelude.fib: vec-nth and vec-nth-slow, vec-assoc and vec-assoc-slow, vec-conj and vec-conj-slow; lair marks a
# function whose name ends `-slow`, or whose instance name has `-slow.`, `noinline`: compiler/native/lower.fib `slow-path?`).
# kernels.fib is run (its answers are fixed below) and built whole-program to optimised LLVM IR. Exit 1 unless: each of the three slow
# paths is defined with `noinline`; no function calls vec-nth or vec-assoc of a Vec (`f.fib.prelude.vec-nth.`, `vec-assoc.`, or the
# Reducible nth or Assoc assoc method of Vec, which only call them): every read and write is in line; and the loops that append call
# vec-conj-slow (the slow path stays a call). The stage 2 of f748147c fails: there is no slow path and every nth is a call of the method.
# usage: check.sh FIBC     run from anywhere; FIB_LIB defaults to the tree's lib
fibc=${1:?usage: check.sh FIBC}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
out=$(mktemp -d "${TMPDIR:-/tmp}/vecfast.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT
ulimit -v 16000000 2>/dev/null
got=$(cd "$root" && timeout 300 "$fibc" run "$here/kernels.fib") || { echo "check.sh: run failed"; exit 1; }
want=$'504500\n3000\n499500\n0'
[ "$got" = "$want" ] || { echo "check.sh: kernels.fib answered"; echo "$got"; echo "check.sh: wanted"; echo "$want"; exit 1; }
(cd "$root" && timeout 300 "$fibc" build --whole-program "$here/kernels.fib" --emit llvm -o "$out/k.ll") || { echo "check.sh: build failed"; exit 1; }
python3 - "$out/k.ll" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
groups = dict(re.findall(r'^attributes #(\d+) = \{([^}]*)\}', text, re.M))
defs = {}
for m in re.finditer(r'^define [^@]*@"?([^"(\s]+)"?\([^\n]*?#(\d+) \{', text, re.M):
    defs[m.group(1)] = groups.get(m.group(2), '')
bad = 0
for slow in ('f.fib.prelude.vec-nth-slow.', 'f.fib.prelude.vec-assoc-slow.', 'f.fib.prelude.vec-conj-slow.'):
    found = [n for n in defs if n.startswith(slow)]
    if not found: print('check.sh: no definition of ' + slow + '*'); bad = 1
    for n in found:
        if 'noinline' not in defs[n].split(): print('check.sh: ' + n + ' is not noinline'); bad = 1
calls = re.findall(r'\bcall [^@%]*@"?([^"(\s]+)"?\(', text)
inline = [c for c in calls if re.match(r'f\.fib\.prelude\.vec-(nth|assoc)\.|m\.fib\.seq\.protocols\.Reducible\.nth\.\$fib\.prelude/Vec|m\.fib\.coll\.protocols\.Assoc\.assoc\.\$fib\.prelude/Vec', c)]
for c in sorted(set(inline)): print('check.sh: a call of ' + c + ' (not in line)'); bad = 1
if not any(c.startswith('f.fib.prelude.vec-conj-slow.') for c in calls): print('check.sh: no call of vec-conj-slow'); bad = 1
sys.exit(bad)
PY
