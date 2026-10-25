import sys, re, subprocess, collections
# usage: loopchecks.py OPT.ll FUNCREGEX   per loop of the matching functions (after optimisation): the checks left in its own blocks
# (blocks of an inner loop count for the inner loop only).
ll, fre = sys.argv[1], re.compile(sys.argv[2])
txt = open(ll).read()
nomatch = set(re.findall(r'^(@[\w.]+) = .*c"no match clause matched\\00"', txt, re.M))
nthr = set(re.findall(r'^(@[\w.]+) = .*c"nth: index out of range\\00"', txt, re.M))
# functions -> blocks
funcs = {}
cur = None
for l in txt.split('\n'):
    m = re.match(r'^define .*@"?([^"(\s]+)"?\(', l)
    if m:
        cur = m.group(1); funcs[cur] = collections.OrderedDict(); blk = 'entry'; funcs[cur][blk] = []; continue
    if cur is None: continue
    if l == '}': cur = None; continue
    m = re.match(r'^([\w.\-]+):', l)
    if m: blk = m.group(1); funcs[cur][blk] = []; continue
    funcs[cur][blk].append(l)
def kind(blocks, b):
    ls = blocks.get(b, [])
    for l in ls[:3]:
        m = re.search(r'call fastcc void @fib\.(trap[-\w]*)\(([^)]*)\)', l)
        if m:
            fn, arg = m.groups(); a = arg.split()[-1] if arg else ''
            if fn == 'trap-index': return 'bounds'
            if a in nthr: return 'nth'
            if a in nomatch: return 'nomatch'
            if fn == 'trap-c': return 'ovf?'
            return 'trap'
    return None
loopinfo = subprocess.run(['/usr/lib/llvm-21/bin/opt', '-passes=print<loops>', '-disable-output', ll], capture_output=True, text=True).stderr
loops = collections.defaultdict(list)
for l in loopinfo.split('\n'):
    m = re.match(r"Loop info for function '(.*)':", l)
    if m: f = m.group(1); continue
    m = re.match(r'\s*Loop at depth (\d+) containing: (.*)', l)
    if m and fre.search(f):
        bl = [re.sub(r'<.*', '', x).lstrip('%').strip('"') for x in m.group(2).split(',')]
        loops[f].append((int(m.group(1)), bl))
for f, ls in loops.items():
    blocks = funcs.get(f, {})
    print(f'== {f}')
    for d, bl in ls:
        inner = set()
        for d2, bl2 in ls:
            if d2 > d and set(bl2) < set(bl): inner |= set(bl2)
        own = [b for b in bl if b not in inner]
        c = collections.Counter()
        callees = collections.Counter()
        for b in own:
            for l in blocks.get(b, []):
                if '.with.overflow' in l and '= ' in l and 'call' in l: c['ovf'] += 1
                m = re.match(r'\s*br i1 [^,]+, label %([\w.\-]+), label %([\w.\-]+)', l)
                if m:
                    for t in m.groups():
                        k = kind(blocks, t)
                        if k: c['br->' + k] += 1
                m = re.search(r'switch i32 [^,]+, label %([\w.\-]+)', l)
                if m and kind(blocks, m.group(1)): c['sw->' + kind(blocks, m.group(1))] += 1
                if re.search(r'atomicrmw|load atomic|cmpxchg', l): c['atomic'] += 1
                m = re.search(r'call [^@]*@"?([^"(\s]+)"?\(', l)
                if m and not m.group(1).startswith(('llvm.', 'fib.trap')): callees[m.group(1)] += 1
                if re.search(r'\bphi\b', l): c['phi'] += 1
                if re.search(r'= fadd|= fmul', l): c['fp'] += 1
        hdr = bl[0]
        print(f'  depth {d} {hdr} ({len(own)} own blocks): ' + ' '.join(f'{k}={v}' for k, v in sorted(c.items())) +
              ('  calls: ' + ', '.join(f'{k[:50]}x{v}' for k, v in callees.items()) if callees else ''))
