import sys, re
# usage: redirect.py IN.ll OUT.ll SPEC..   SPEC = FUNCREGEX:ORDINALS (comma list, 1-based count of nth calls in that function)
# clones f.fib.prelude.vec-nth.T as vec-nth.T.nc and points the chosen calls at the clone (strip the clone afterwards)
inp, outp, specs = sys.argv[1], sys.argv[2], sys.argv[3:]
txt = open(inp).read()
lines = txt.split('\n')
nth_call = re.compile(r'call tailcc (\w+) @(?:"m\.fib\.seq\.protocols\.Reducible\.nth\.\$fib\.prelude/Vec\.t0_\.(\w+)"|f\.fib\.prelude\.vec-nth\.(\w+))\(')
out = []; cur = None; k = 0; hits = 0; clones = set()
sp = [(re.compile(s.split(':')[0]), set(int(x) for x in s.split(':')[1].split(','))) for s in specs]
for l in lines:
    m = re.match(r'^define .*@"?([^"(\s]+)"?\(', l)
    if m: cur = m.group(1); k = 0
    mm = nth_call.search(l)
    if mm and cur:
        k += 1
        for fre, ords in sp:
            if fre.fullmatch(cur) and k in ords:
                t = mm.group(2) or mm.group(3)
                l = l[:mm.start()] + f'call tailcc {mm.group(1)} @f.fib.prelude.vec-nth.{t}.nc(' + l[mm.end():]
                clones.add(t); hits += 1
    out.append(l)
txt = '\n'.join(out)
for t in clones:
    m = re.search(r'^define[^\n]*@f\.fib\.prelude\.vec-nth\.' + t + r'\(.*?^}', txt, re.M | re.S)
    txt += '\n' + m.group(0).replace(f'@f.fib.prelude.vec-nth.{t}(', f'@f.fib.prelude.vec-nth.{t}.nc(', 1) + '\n'
print(f'{hits} call sites redirected', file=sys.stderr)
open(outp, 'w').write(txt)
