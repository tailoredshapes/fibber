#!/bin/bash
# usage: exp.sh NAME FLAGS...   (strip flags; none = baseline).  Uses kni.ll, functions f.(sum|dot|axpy|map).  env PASSES, CPU pass through.
# builds o/NAME.exe, prints packed/scalar counts for the kernel functions and remarks
cd /home/tmarsh/.cache/fibber-scratch/SIMD-V/k
name=$1; shift
FRE=${FRE:-^f\.(sum|dot|axpy|map)}
python3 strip.py ${SRC:-kni.ll} o/$name.in.ll "$FRE" "$@" || exit 1
bash opt.sh o/$name.in.ll $name ${OPTARGS} >/dev/null || exit 1
/usr/lib/llvm-21/bin/llc -O2 -mcpu=${CPU:-raptorlake} -filetype=obj -relocation-model=pic o/$name.ll -o o/$name.o || exit 1
cc o/$name.o -o o/$name.exe -lm -lpthread || exit 1
# per-function vector counts from asm
python3 - "$name" <<'EOF'
import re,sys
n=sys.argv[1]
asm=open(f'o/{n}.s').read()
cur=None; res={}
for l in asm.split('\n'):
    m=re.match(r'^"?(f\.[\w.\-]+)"?:',l)
    if m: cur=m.group(1); res[cur]=[0,0]; continue
    if cur and re.match(r'\s+v?(add|sub|mul|div|fmadd\d*|fnmadd\d*|sqrt)p[sd]\b',l): res[cur][0]+=1
    if cur and re.match(r'\s+v?(add|sub|mul|div|fmadd\d*|fnmadd\d*|sqrt)s[sd]\b',l): res[cur][1]+=1
    if cur and re.match(r'\s+v?(padd|psub|pmul)[bwdq]\b',l): res[cur][0]+=1
for k,v in res.items():
    if re.match(r'f\.(sum|dot|axpy|map)',k): print(f'  {k}: packed={v[0]} scalar={v[1]}')
EOF
grep -E "Vectorized" o/$name.remarks | cut -c1-200
