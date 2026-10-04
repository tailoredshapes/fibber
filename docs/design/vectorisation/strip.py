import sys, re
# usage: strip.py IN.ll OUT.ll FUNCREGEX FLAG..   flags: ovf bounds rc uniq noalias reassoc fast contract
inp, outp, fre = sys.argv[1], sys.argv[2], re.compile(sys.argv[3])
flags = set(sys.argv[4:])
TB_USED = set()
lines = open(inp).read().split('\n')
out = []
i = 0
ovf_re = re.compile(r'^(\s*)(%[\w.]+) = (?:tail )?call \{ (i\d+), i1 \} @llvm\.([su])(add|sub|mul)\.with\.overflow\.i\d+\((i\d+) ([^,]+), (i\d+) ([^)]+)\)')
while i < len(lines):
    l = lines[i]
    m = re.match(r'^define .*@"?([^"(\s]+)"?\(', l)
    if not (m and fre.search(m.group(1))):
        out.append(l); i += 1; continue
    # collect the function
    j = i
    while lines[j] != '}':
        j += 1
    body = lines[i:j + 1]
    i = j + 1
    txt = '\n'.join(body)
    if 'noalias' in flags:
        hdr = body[0]
        hdr2 = re.sub(r'\bptr (%p\d+)', r'ptr noalias \1', hdr)
        body[0] = hdr2
    if 'ovf' in flags:
        nb = []
        vals = {}
        for l in body:
            mm = ovf_re.match(l)
            if mm:
                ind, name, ty, sg, op, _, a, _, b = mm.groups()
                nb.append(f'{ind}{name}.v = {op} {ty} {a}, {b}')
                vals[name] = ty
            else:
                nb.append(l)
        body = []
        for l in nb:
            mm = re.match(r'^(\s*)(%[\w.]+) = extractvalue \{ (i\d+), i1 \} (%[\w.]+), (\d)$', l)
            if mm and mm.group(4) in vals:
                ind, nm, ty, src, idx = mm.groups()
                if idx == '0':
                    body.append(f'{ind}{nm} = add {ty} {src}.v, 0')
                else:
                    body.append(f'{ind}{nm} = or i1 false, false')
            else:
                body.append(l)
    if 'bounds' in flags:
        # a block named oob* contains only the trap-index call and unreachable; the branch into it gets a true condition
        oobs = set(re.findall(r'^(oob\w*):', '\n'.join(body), re.M))
        nb = []
        for k, l in enumerate(body):
            mm = re.match(r'^\s*br i1 (%[\w.]+), label %(\w+), label %(\w+)$', l)
            if mm and mm.group(3) in oobs:
                # the icmp defining the condition: make it true
                cond = mm.group(1)
                for q in range(len(nb) - 1, -1, -1):
                    if nb[q].lstrip().startswith(cond + ' = icmp'):
                        ind = re.match(r'^\s*', nb[q]).group(0)
                        nb[q] = f'{ind}{cond} = or i1 true, false'
                        break
            nb.append(l)
        body = nb
    if 'rc' in flags:
        body = [l for l in body if not re.search(r'call void @fib\.(retain|release)\(', l)]
    if 'uniq' in flags:
        nb = []
        for l in body:
            mm = re.match(r'^(\s*)(%[\w.]+) = call i1 @"fib\.unique\?"\(ptr [^)]*\)$', l)
            nb.append(f'{mm.group(1)}{mm.group(2)} = or i1 true, false' if mm else l)
        body = nb
    if 'uflag' in flags:
        nb = []
        k = 0
        pend = None
        for idx, l in enumerate(body):
            mm = re.match(r'^(\s*)(%[\w.]+) = call i1 @"fib\.unique\?"\((ptr [^)]*)\)$', l)
            nxt = body[idx + 1] if idx + 1 < len(body) else ''
            br = re.match(r'^\s*br i1 (%[\w.]+), label %(\w+), label %(\w+)$', nxt)
            if mm and br and br.group(1) == mm.group(2):
                k += 1
                x, a, c = mm.group(2), br.group(2), br.group(3)
                nb.append(f'  %uf{k} = load i1, ptr %uflag{k}')
                nb.append(f'  br i1 %uf{k}, label %{a}, label %chk{k}')
                nb.append(f'chk{k}:')
                nb.append(l)
                continue
            nb.append(l)
        body = nb
        # entry allocas and the store after the merge: the "written" labels follow in order
        nb = []
        wi = 0
        for l in body:
            nb.append(l)
            if l == 'entry:':
                for q in range(1, k + 1):
                    nb.append(f'  %uflag{q} = alloca i1')
                    nb.append(f'  store i1 false, ptr %uflag{q}')
            mm = re.match(r'^(written\w*):', l)
            if mm and wi < k:
                wi += 1
                nb.append(f'  store i1 true, ptr %uflag{wi}')
        body = nb
    if 'tbaa' in flags:
        cls = {}
        nb = []
        for l in body:
            mm = re.match(r'^\s*(%[\w.]+) = getelementptr %struct\.fib\.(array|cell)\.(\w+), ptr [^,]+, i32 0, i32 (\d)(?:, i64 .*)?$', l)
            if mm:
                nm, kind, ty, fld = mm.groups()
                if kind == 'array' and fld == '3': cls[nm] = 'len'
                elif kind == 'array' and fld == '4': cls[nm] = 'elem.' + ty
                elif kind == 'cell' and fld == '3': cls[nm] = 'cellptr'
            mm = re.match(r'^(\s*(?:%[\w.]+ = )?(?:load|store) .*?)(?:, ptr (%[\w.]+))(, align \d+)$', l)
            if mm and mm.group(2) in cls:
                l = f'{mm.group(1)}, ptr {mm.group(2)}{mm.group(3)}, !tbaa !{{TB:{cls[mm.group(2)]}}}'
            nb.append(l)
        body = nb
        TB_USED.update(re.findall(r'!\{TB:([\w.]+)\}', '\n'.join(body)))
    for f in ('reassoc', 'fast', 'contract'):
        if f in flags:
            body = [re.sub(r'= (fadd|fmul|fsub) ', rf'= \1 {f} ', l) for l in body]
    out.extend(body)
txt = '\n'.join(out)
if TB_USED:
    names = sorted(TB_USED)
    md = ['!9000 = !{!"fib tbaa root"}']
    ids = {}
    for k, n in enumerate(names):
        ids[n] = 9001 + 2 * k
        md.append(f'!{9001 + 2*k} = !{{!"{n}", !9000}}')
        md.append(f'!{9002 + 2*k} = !{{!{9001 + 2*k}, !{9001 + 2*k}, i64 0}}')
    txt = re.sub(r'!\{TB:([\w.]+)\}', lambda m: '!' + str(ids[m.group(1)] + 1), txt)
    txt += '\n' + '\n'.join(md) + '\n'
open(outp, 'w').write(txt)
