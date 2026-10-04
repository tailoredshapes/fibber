# reference output of pidigits: Machin pi = 16 atan(1/5) - 4 atan(1/239) in integers; usage: python3 reference.py N | md5sum
import sys
sys.set_int_max_str_digits(0)
n=int(sys.argv[1])
# Machin: pi = 16 atan(1/5) - 4 atan(1/239) with guard digits, integer arithmetic
g=20
one=10**(n+g)
def atan_inv(x):
    s=t=one//x; k=1; x2=x*x
    while t:
        t//=x2; k+=2
        s+= -(t//k) if (k//2)%2 else t//k
    return s
pi=(16*atan_inv(5)-4*atan_inv(239))//10**g
d=str(pi)[:n]
out=[]
for i in range(0,n,10):
    chunk=d[i:i+10]
    out.append(chunk.ljust(10)+"\t:%d\n"%min(i+10,n))
sys.stdout.write(''.join(out))
