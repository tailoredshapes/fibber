#!/usr/bin/env python3
"""xform.py IN.ll OUT.ll : converts the calls of user functions (@f.* and @l.*) to user
functions and through closure pointers into `invoke` with one cleanup landing pad per
function (which calls the external @fib_cleanup_stub and resumes), and prints static
counts for a hidden-Result lowering (calls that would need a branch)."""
import re, sys
src = open(sys.argv[1]).read().split('\n')
USER = re.compile(r'^@(f|l)\.')
defs = {}  # name -> (start, end)
i = 0
while i < len(src):
    m = re.match(r'^define .*?@([^\s(]+)\(', src[i])
    if m:
        j = i
        while src[j] != '}': j += 1
        defs[m.group(1)] = (i, j)
        i = j
    i += 1
# ---- static may-throw closure (for the hidden-Result count)
TRAPS = ('fib.trap-c', 'fib.trap', 'fib.trap-at', 'fib.trap-at-c', 'fib.trap-bytes', 'fib.trap-hooked')
callre = re.compile(r'\bcall\b[^@%\n]*?(?:[@%][^\s(]+)?\(?')
def callees(name):
    s, e = defs[name]
    out = []
    for ln in src[s:e]:
        if re.search(r'\b(call|invoke)\b', ln) and 'llvm.' not in ln.split('(')[0] and ' = phi' not in ln:
            m = re.search(r'\b(?:call|invoke)\b[^@%]*?([@%][\w.\-$]+)\(', ln)
            out.append(m.group(1) if m else '?')
    return out
cal = {n: callees(n) for n in defs}
mt = {n for n in defs if any(c[1:] in TRAPS for c in cal[n])}
changed = True
while changed:
    changed = False
    for n in defs:
        if n not in mt and any((c.startswith('@') and c[1:] in mt) or c.startswith('%') for c in cal[n]):
            mt.add(n); changed = True
tot_calls = tot_branch = tot_ins = 0
userfns = [n for n in defs if USER.match('@' + n)]
for n in userfns:
    s, e = defs[n]
    tot_ins += sum(1 for ln in src[s:e] if re.match(r'^\s+(%\S+ = )?[a-z]', ln))
    for c in cal[n]:
        if c.startswith('@') and c[1:].startswith('llvm.'): continue
        tot_calls += 1
        if c.startswith('%') or (c.startswith('@') and c[1:] in mt and USER.match(c)):
            tot_branch += 1
# user-to-user + closure calls: the branch count (library helpers fib.* that may trap inline are counted as they are calls to may-throw functions)
tot_branch_all = 0
for n in userfns:
    for c in cal[n]:
        if c.startswith('%') or (c.startswith('@') and c[1:] in mt): tot_branch_all += 1
print(f'user functions {len(userfns)}, user instructions {tot_ins}, call sites in user functions {tot_calls}, '
      f'user->user or closure calls {tot_branch}, calls to any may-throw function or closure {tot_branch_all}, '
      f'may-throw functions {len(mt)} of {len(defs)} defined', file=sys.stderr)
# ---- invoke transform
out = []
ninv = 0
k = 0
while k < len(src):
    ln = src[k]
    m = re.match(r'^define .*?@([^\s(]+)\(', ln)
    if not (m and USER.match('@' + m.group(1))):
        out.append(ln); k += 1; continue
    name = m.group(1)
    s, e = defs[name]
    body = src[s:e + 1]
    hdr = re.sub(r' \{$', ' personality ptr @__gxx_personality_v0 {', body[0])
    res = [hdr]
    cur = 'entry'
    last = {}     # orig label -> last segment label
    n = 0
    for bi, b in enumerate(body[1:-1], 1):
        nxt = body[bi + 1] if bi + 1 < len(body) else ''
        lm = re.match(r'^([\w.\-$]+):', b)
        if lm:
            last[cur] = cur if cur not in last else last[cur]
            cur = lm.group(1); res.append(b); continue
        cm = re.match(r'^(\s*)(%[\w.\-$]+ = )?(tail )?call (.*?)([@%][\w.\-$]+)\((.*)\)(.*)$', b)
        tailpos = re.match(r'^\s+ret ', nxt) is not None   # a call in tail position: the frame is dead, no cleanup is owed
        if cm and 'musttail' not in b and 'llvm.' not in cm.group(5) and not tailpos:
            callee = cm.group(5)
            if callee.startswith('%') or USER.match(callee):
                n += 1; ninv += 1
                seg = f'cont.{n}'
                res.append(f'{cm.group(1)}{cm.group(2) or ""}invoke {cm.group(4)}{callee}({cm.group(6)}) to label %{seg} unwind label %lpad{cm.group(7)}')
                res.append(f'{seg}:')
                last[cur] = seg   # the segment that ends the original block (updated by later splits)
                cur = seg
                continue
        res.append(b)
    # the label a successor's phi must name: the last segment of each original block
    # (recompute: walk again)
    res2 = []
    cur = 'entry'; seglast = {}; orig = 'entry'
    lastof = {}
    for b in res:
        lm = re.match(r'^([\w.\-$]+):', b)
        if lm:
            if lm.group(1).startswith('cont.'): lastof[orig] = lm.group(1)
            else: orig = lm.group(1); lastof.setdefault(orig, orig)
    for b in res:
        if ' = phi ' in b:
            b = re.sub(r'\[ ([^,\]]+), %([\w.\-$]+) \]', lambda mm: f'[ {mm.group(1)}, %{lastof.get(mm.group(2), mm.group(2))} ]', b)
        res2.append(b)
    if n:
        res2 += ['lpad:', '  %lp = landingpad { ptr, i32 } cleanup', '  call void @fib_cleanup_stub()', '  resume { ptr, i32 } %lp']
    else:
        res2[0] = body[0]
    res2.append('}')
    out += res2
    k = e + 1
out.append('declare void @fib_cleanup_stub()')
out.append('declare i32 @__gxx_personality_v0(...)')
open(sys.argv[2], 'w').write('\n'.join(out) + '\n')
print(f'invokes {ninv}', file=sys.stderr)
