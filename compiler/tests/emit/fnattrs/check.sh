#!/bin/bash
# compiler/tests/emit/fnattrs/check.sh FIBC: `:noinline` and `:cold` after a defun's parameters (spec/syntax.md 3.1) are the lIR words of the
# same names on each instance of the function (spec/lir.md 4.3), and lair makes them LLVM's function attributes. prog.fib is run (its answer
# is fixed below), emitted as lIR and built whole-program to optimised LLVM IR. Exit 1 unless: the define of cold-path carries `noinline cold`
# and keep-out `noinline` only, in the lIR and in the LLVM; plain-slow, whose name ends `-slow` and which asks for nothing, carries neither
# (in the LLVM it is put in line, so it is not defined at all). A stage 2 that reads the marks as the body fails (`unbound name ->`); one
# whose lair marks names ending `-slow` fails on plain-slow.
# usage: check.sh FIBC     run from anywhere; FIB_LIB defaults to the tree's lib
fibc=${1:?usage: check.sh FIBC}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
out=$(mktemp -d "${TMPDIR:-/tmp}/fnattrs.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT
ulimit -v 16000000 2>/dev/null
got=$(cd "$root" && timeout 300 "$fibc" run "$here/prog.fib") || { echo "check.sh: run failed"; echo "$got"; exit 1; }
want=$'3\n0'
[ "$got" = "$want" ] || { echo "check.sh: prog.fib answered"; echo "$got"; echo "check.sh: wanted"; echo "$want"; exit 1; }
(cd "$root" && timeout 300 "$fibc" emit "$here/prog.fib" > "$out/p.lir") || { echo "check.sh: emit failed"; exit 1; }
(cd "$root" && timeout 300 "$fibc" build --whole-program "$here/prog.fib" --emit llvm -o "$out/p.ll") || { echo "check.sh: build failed"; exit 1; }
python3 - "$out/p.lir" "$out/p.ll" <<'PY'
import re, sys
lir, ll = open(sys.argv[1]).read(), open(sys.argv[2]).read()
want = {'f.cold-path': {'noinline', 'cold'}, 'f.keep-out': {'noinline'}, 'f.plain-slow': set()}
bad = 0
for name, attrs in want.items():
    m = re.search(r'^\(DEFINE ([^(]*)\(' + re.escape(name) + r' ', lir, re.M)
    if not m: print('check.sh: no lIR define of ' + name); bad = 1; continue
    words = set(m.group(1).split()) & {'NOINLINE', 'COLD'}
    if words != {a.upper() for a in attrs}: print('check.sh: lIR define of %s has %s, wanted %s' % (name, sorted(words), sorted(attrs))); bad = 1
groups = dict(re.findall(r'^attributes #(\d+) = \{([^}]*)\}', ll, re.M))
for name, attrs in want.items():
    m = re.search(r'^define [^@]*@"?' + re.escape(name) + r'"?\([^\n]*?(?:#(\d+) )?\{', ll, re.M)
    if not m:
        if attrs: print('check.sh: no LLVM define of ' + name); bad = 1
        continue
    words = set(groups.get(m.group(1) or '', '').split()) & {'noinline', 'cold'}
    if words != attrs: print('check.sh: LLVM define of %s has %s, wanted %s' % (name, sorted(words), sorted(attrs))); bad = 1
sys.exit(bad)
PY
