#!/bin/bash
# compiler/tests/emit/nomatch/check.sh FIBC: the block a match falls into when no clause matched is `unreachable`, not a trap, when the
# checker proved the clauses exhaustive (types.exhaust; compiler/emit/lower/cx.fib `lcx-no-match`, docs/design/lir-facts.md 9 item 1),
# and a refutable let keeps its own trap. nomatch.fib is run (it answers 0) and emitted as lIR: the functions `f.area` (an enum),
# `f.guarded` (a guarded clause before exhaustive ones) and `f.opt` (an Option and a bool) must not hold the text `no match clause matched`
# and must end a block in `(unreachable)`; `f.pick` (a refutable let) must still call fib.trap with `let: pattern does not match`. The stage 2 before
# the change fails the first three (each trapped). Exit 1 on any difference.
# usage: check.sh FIBC     run from anywhere; FIB_LIB defaults to the tree's lib
fibc=${1:?usage: check.sh FIBC}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
out=$(mktemp -d "${TMPDIR:-/tmp}/nomatch.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT
ulimit -v 16000000 2>/dev/null
got=$(timeout 300 "$fibc" run "$here/nomatch.fib"; echo "exit $?")
[ "$got" = $'0\nexit 0' ] || { echo "check.sh: nomatch.fib answered: $got"; exit 1; }
timeout 300 "$fibc" emit "$here/nomatch.fib" > "$out/n.lir" 2> "$out/err" || { echo "check.sh: emit failed"; cat "$out/err"; exit 1; }
python3 - "$out/n.lir" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
defs = {}
for m in re.finditer(r'\(define [^(]*\((f\.[\w.?!-]+) ', text):
    start = j = m.start(); depth = 0
    while j < len(text):
        c = text[j]
        if c == '"':
            j += 1
            while text[j] != '"': j += 2 if text[j] == '\\' else 1
        elif c == '(': depth += 1
        elif c == ')':
            depth -= 1
            if depth == 0: break
        j += 1
    defs.setdefault(m.group(1), text[start:j + 1])
bad = 0
for name in ['f.area', 'f.guarded', 'f.opt']:
    body = defs.get(name)
    if body is None:
        print('FAIL %s: not emitted' % name); bad += 1; continue
    if 'no match clause matched' in body:
        print('FAIL %s: the exhaustive match still traps' % name); bad += 1
    elif '(unreachable)' not in body:
        print('FAIL %s: no unreachable block' % name); bad += 1
    else:
        print('ok %s' % name)
# the message is a string constant: `(constant internal str.N {..} {.. ([K x i8] (i8 108) ..)})`
def const_text(name):
    m = re.search(r'\(constant internal ' + re.escape(name) + r' .*?\[\d+ x i8\] ((?:\(i8 -?\d+\) ?)+)', text)
    return ''.join(chr(int(b) % 256) for b in re.findall(r'\(i8 (-?\d+)\)', m.group(1))) if m else ''
traps = re.findall(r'\(call @fib\.trap @(str\.\d+)\)', defs.get('f.pick', ''))
if any(const_text(t).startswith('let: pattern does not match') for t in traps): print('ok f.pick')
else: print('FAIL f.pick: the refutable let lost its trap'); bad += 1
sys.exit(1 if bad else 0)
PY
