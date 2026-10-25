import sys, re
# usage: lfstrip.py IN.ll OUT.ll FUNCREGEX FLAG..
# Ceiling rewrites of fibber's unoptimised IR (fibc build -O 0 --emit llvm), in the style of docs/design/vectorisation/strip.py.
#   bounds   branch into a block that calls fib.trap-index goes straight to the in-bounds side          (unsound)
#   nthrange branch into a block that calls fib.trap("nth: index out of range") never taken             (unsound)
#   nomatch  branch into a block that calls fib.trap-c("no match clause matched") never taken           (sound: exhaustive match)
#   ovf      every s{add,sub,mul}.with.overflow becomes a plain wrapping op, overflow bit false           (unsound)
#   ovfnsw   the same, the op carries nsw                                                                   (unsound as a blanket rule)
#   ovfnuw   the same, the op carries nuw nsw                                                               (unsound as a blanket rule)
#   range    !range [0, 2^60) on loads of an array length and of a Vec's cnt                                (sound: allocation bound)
#   vecinv   in vec-nth: llvm.assume(len(tail) <= cnt)                                                       (sound: Vec invariant)
#   tbaa     struct-path-free TBAA tags on array length, array elements per type, Vec fields                (argued, vectorisation.md 6)
#   inv      !invariant.load on loads of a Vec's tag and fields                                             (Vec fields are immutable; see the doc)
#   inl      alwaysinline on vec-nth and Reducible.nth                                                      (not a fact; separates inlining)
#   reassoc  reassoc on fadd/fmul/fsub                                                                      (opt-in semantics change)
inp, outp, fre = sys.argv[1], sys.argv[2], re.compile(sys.argv[3])
flags = set(sys.argv[4:])
src = open(inp).read()
lines = src.split('\n')

# string constants that name the trap kinds
nomatch_strs = set(re.findall(r'^(@[\w.]+) = .*c"no match clause matched\\00"', src, re.M))
nth_strs = set(re.findall(r'^(@[\w.]+) = .*c"nth: index out of range\\00"', src, re.M))

ovf_re = re.compile(r'^(\s*)(%[\w.]+) = (?:tail )?call \{ (i\d+), i1 \} @llvm\.([su])(add|sub|mul)\.with\.overflow\.i\d+\((i\d+) ([^,]+), (i\d+) ([^)]+)\)')
gep_re = re.compile(r'^\s*(%[\w.]+) = getelementptr (%[\w."$/]+), ptr [^,]+, i32 0, i32 (\d+)(?:, i64 .*)?$')
TB_USED = set()
out = []
i = 0
need_assume = False
need_launder = [False]
need_invg = [False]
need_scope = [False]


def trap_kind(body, label):
    """The kind of trap a block named label is, if its first instruction is a trap call."""
    for k, l in enumerate(body):
        if l.startswith(label + ':'):
            nxt = body[k + 1].strip() if k + 1 < len(body) else ''
            m = re.match(r'call void @fib\.(trap[-\w]*)\(([^)]*)\)', nxt)
            if not m:
                return None
            fn, arg = m.groups()
            if fn == 'trap-index':
                return 'bounds'
            a = arg.split()[-1] if arg else ''
            if fn == 'trap' and a in nth_strs:
                return 'nthrange'
            if fn == 'trap-c' and a in nomatch_strs:
                return 'nomatch'
            return None
    return None


while i < len(lines):
    l = lines[i]
    m = re.match(r'^define .*@"?([^"(\s]+)"?\(', l)
    if not (m and fre.search(m.group(1))):
        out.append(l); i += 1; continue
    fname = m.group(1)
    j = i
    while lines[j] != '}':
        j += 1
    body = lines[i:j + 1]
    i = j + 1
    if 'inl' in flags and (re.match(r'f\.fib\.prelude\.vec-nth\.', fname) or 'Reducible.nth.$fib.prelude/Vec' in fname):
        body[0] = re.sub(r' (#\d+) \{$', r' alwaysinline \1 {', body[0])
    strip_kinds = flags & {'bounds', 'nthrange', 'nomatch'}
    if strip_kinds:
        nb = []
        for l in body:
            mm = re.match(r'^(\s*)br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$', l)
            if mm:
                ind, c, a, b = mm.groups()
                ka, kb = trap_kind(body, a), trap_kind(body, b)
                if kb in strip_kinds:
                    l = f'{ind}br label %{a}'
                elif ka in strip_kinds:
                    l = f'{ind}br label %{b}'
            nb.append(l)
        body = nb
    ov = flags & {'ovf', 'ovfnsw', 'ovfnuw'}
    if ov:
        fl = {'ovf': '', 'ovfnsw': 'nsw ', 'ovfnuw': 'nuw nsw '}[sorted(ov)[0]]
        nb = []
        vals = {}
        for l in body:
            mm = ovf_re.match(l)
            if mm:
                ind, name, ty, sg, op, _, a, _, b = mm.groups()
                nb.append(f'{ind}{name}.v = {op} {fl}{ty} {a}, {b}')
                vals[name] = ty
            else:
                nb.append(l)
        body = []
        for l in nb:
            mm = re.match(r'^(\s*)(%[\w.]+) = extractvalue \{ (i\d+), i1 \} (%[\w.]+), (\d)$', l)
            if mm and mm.group(4) in vals:
                ind, nm, ty, src_, idx = mm.groups()
                body.append(f'{ind}{nm} = add {ty} {src_}.v, 0' if idx == '0' else f'{ind}{nm} = or i1 false, false')
            else:
                body.append(l)
    if 'range' in flags or 'tbaa' in flags or 'inv' in flags:
        cls = {}
        nb = []
        for l in body:
            mm = gep_re.match(l)
            if mm:
                nm, st, fld = mm.groups()
                if st.startswith('%struct.fib.array.'):
                    if fld == '3': cls[nm] = 'len'
                    elif fld == '4': cls[nm] = 'elem.' + st[len('%struct.fib.array.'):]
                elif st.startswith('%"struct.o.$fib.prelude/Vec.') and st.endswith('.v1"'):
                    cls[nm] = {'4': 'vcnt', '5': 'vshift', '6': 'vroot', '7': 'vtail'}.get(fld)
                elif st.startswith('%"struct.o.$fib.prelude/Vec.') and fld == '3':
                    cls[nm] = 'vtag'
                elif st == '%struct.fib.hdr' and 'tbaahdr' in flags:
                    cls[nm] = 'hdr' + fld
            mm = re.match(r'^(\s*(?:%[\w.]+ = )?(load|store) .*?)(?:, ptr (%[\w.]+))(, align \d+)$', l)
            if mm and cls.get(mm.group(3)):
                c = cls[mm.group(3)]
                extra = ''
                if 'range' in flags and mm.group(2) == 'load' and c in ('len', 'vcnt') and ' = load i64,' in l:
                    extra += ', !range !8999'
                if 'inv' in flags and mm.group(2) == 'load' and c in ('vcnt', 'vshift', 'vroot', 'vtail', 'vtag'):
                    extra += ', !invariant.load !8998'
                if 'tbaa' in flags:
                    extra += f', !tbaa !{{TB:{c}}}'
                    TB_USED.add(c)
                l = f'{mm.group(1)}, ptr {mm.group(3)}{mm.group(4)}{extra}'
            nb.append(l)
        body = nb
    if 'invg' in flags:
        # !invariant.group on loads of a Vec's tag and fields; a reused shell is laundered first (its reuse block and the phi edge from it)
        cls = {}
        reuse = {}
        for l in body:
            mm = re.match(r'^\s*%[\w.]+ = phi ptr (.*)$', l)
            if mm:
                for v, b in re.findall(r'\[ (%[\w.]+), %(reuse\d+) \]', mm.group(1)):
                    reuse[b] = v
        nb = []
        blk = None
        for l in body:
            mm = re.match(r'^([\w.\-]+):', l)
            if mm:
                blk = mm.group(1); nb.append(l)
                if blk in reuse:
                    nb.append(f'  %lnd.{blk} = call ptr @llvm.launder.invariant.group.p0(ptr {reuse[blk]})')
                    need_launder[0] = True
                continue
            if blk in reuse:
                l = re.sub(re.escape(reuse[blk]) + r'\b', f'%lnd.{blk}', l)
            mm = re.match(r'^\s*%[\w.]+ = phi ptr (.*)$', l)
            if mm:
                for b, v in reuse.items():
                    l = l.replace(f'[ {v}, %{b} ]', f'[ %lnd.{b}, %{b} ]')
            mm = gep_re.match(l)
            if mm:
                nm, st, fld = mm.groups()
                if st.startswith('%"struct.o.$fib.prelude/Vec.') and (st.endswith('.v1"') and fld in '4567' or fld == '3'):
                    cls[nm] = True
            mm = re.match(r'^(\s*%[\w.]+ = load .*?, ptr (%[\w.]+), align \d+)$', l)
            if mm and cls.get(mm.group(2)) and blk not in reuse:
                l = mm.group(1) + ', !invariant.group !8998'
                need_invg[0] = True
            nb.append(l)
        body = nb
    if 'scoped' in flags:
        # one alias scope for the fields of immutable objects (a Vec's tag and fields): loads of them are in the scope; every store and
        # call outside a reuse or steal block is declared not to touch it
        cls = {}
        nb = []
        blk = None
        for l in body:
            mm = re.match(r'^([\w.\-]+):', l)
            if mm:
                blk = mm.group(1); nb.append(l); continue
            mm = gep_re.match(l)
            if mm:
                nm, st, fld = mm.groups()
                if st.startswith('%"struct.o.$fib.prelude/Vec.') and (st.endswith('.v1"') and fld in '4567' or fld == '3'):
                    cls[nm] = True
            exempt = blk is not None and re.match(r'(reuse|steal)\d+$', blk)
            mm = re.match(r'^(\s*%[\w.]+ = load .*?, ptr (%[\w.]+), align \d+)((?:, ![\w.]+ ![\d]+)*)$', l)
            if mm and cls.get(mm.group(2)) and not exempt:
                l = mm.group(1) + mm.group(3) + ', !alias.scope !8995'
            elif not exempt and re.match(r'^\s*store ', l) and '!noalias' not in l:
                l = l + ', !noalias !8995'
            elif not exempt and re.match(r'^\s*(%[\w.]+ = )?(tail |musttail )?call ', l) and '@llvm.' not in l and '!noalias' not in l:
                l = l + ', !noalias !8995'
            nb.append(l)
        body = nb
        need_scope[0] = True
    if 'vecinv' in flags and re.match(r'f\.fib\.prelude\.vec-nth\.', fname):
        nb = []
        done = False
        for l in body:
            nb.append(l)
            mm = re.match(r'^\s*(%[\w.]+) = call \{ i64, i1 \} @llvm\.ssub\.with\.overflow\.i64\(i64 (%[\w.]+), i64 (%[\w.]+)\)', l)
            if mm and not done:
                done = True
                nb.insert(len(nb) - 1, f'  %vinv = icmp sle i64 {mm.group(3)}, {mm.group(2)}')
                nb.insert(len(nb) - 1, '  call void @llvm.assume(i1 %vinv)')
                need_assume = True
        body = nb
    if 'uniq' in flags:
        body = [re.sub(r'^(\s*)(%[\w.]+) = call i1 @"fib\.unique\?"\(ptr [^)]*\)$', r'\1\2 = or i1 true, false', l) for l in body]
    if 'reassoc' in flags:
        body = [re.sub(r'= (fadd|fmul|fsub) ', r'= \1 reassoc ', l) for l in body]
    out.extend(body)
txt = '\n'.join(out)
md = []
if need_scope[0]:
    md += ['!8993 = distinct !{!8993, !"fib imm domain"}', '!8994 = distinct !{!8994, !8993, !"fib imm"}', '!8995 = !{!8994}']
if need_launder[0] and 'declare ptr @llvm.launder.invariant.group.p0' not in txt:
    md.append('declare ptr @llvm.launder.invariant.group.p0(ptr)')
if 'inv' in flags or need_invg[0]:
    md.append('!8998 = !{}')
if 'range' in flags:
    md.append('!8999 = !{i64 0, i64 1152921504606846976}')
if TB_USED:
    md.append('!9000 = !{!"fib tbaa root"}')
    ids = {}
    # stable ids, so that two passes agree
    ALLN = ['len', 'vcnt', 'vshift', 'vroot', 'vtail', 'vtag', 'hdr0', 'hdr1', 'hdr2'] + sorted(n for n in TB_USED if n.startswith('elem.'))
    ELEMS = ['elem.' + t for t in ('i8', 'i16', 'i32', 'i64', 'double', 'float', 'ptr', 'i1')]
    ALLN = ALLN[:9] + ELEMS + [n for n in ALLN[9:] if n not in ELEMS]
    for k, n in enumerate(ALLN):
        ids[n] = 9001 + 2 * k
        md.append(f'!{9001 + 2*k} = !{{!"{n}", !9000}}')
        md.append(f'!{9002 + 2*k} = !{{!{9001 + 2*k}, !{9001 + 2*k}, i64 0}}')
    txt = re.sub(r'!\{TB:([\w.]+)\}', lambda m: '!' + str(ids[m.group(1)] + 1), txt)
if need_assume and 'declare void @llvm.assume' not in txt:
    md.append('declare void @llvm.assume(i1)')
md = [m for m in md if not re.search('^' + re.escape(m.split(' = ')[0]) + ' = ', txt, re.M)]
txt += '\n' + '\n'.join(md) + '\n'
open(outp, 'w').write(txt)
