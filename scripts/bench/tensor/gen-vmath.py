# Generates lib/fib/tensor/vmath.fib from vmath.tpl: the polynomial constants (mpmath, 60 digits).
#   python3 scripts/bench/tensor/gen-vmath.py     (from the repository root)
import os
HERE = os.path.dirname(os.path.abspath(__file__))
import mpmath as mp
from math import factorial, log2, e, log
mp.mp.dps = 60

def d(x):  # shortest repr that round-trips, as a fibber f64 literal
    r = repr(float(x))
    if 'e' not in r and '.' not in r: r += '.0'
    return r

LOG2E = mp.mpf(1) / mp.log(2)
LN2 = mp.log(2)
hi = float(LN2)
# ln2_hi with 12 trailing zero bits so k*ln2_hi is exact for |k| < 2^12
import struct
bits = struct.unpack('<Q', struct.pack('<d', hi))[0] & ~((1 << 12) - 1)
LN2_HI = struct.unpack('<d', struct.pack('<Q', bits))[0]
LN2_LO = float(LN2 - mp.mpf(LN2_HI))

def horner(var, coeffs, acc='p'):
    # coeffs highest degree first; returns nested fma text
    s = d(coeffs[0]); s = '(splat f64x4 %s)' % s
    for c in coeffs[1:]:
        s = '(simd/fma %s %s (splat f64x4 %s))' % (s, var, d(c))
    return s

exp_c = [1.0 / factorial(n) if n > 1 else 1.0 for n in range(13, -1, -1)]
exp_c = [float(mp.mpf(1) / mp.factorial(n)) for n in range(13, -1, -1)]
exp_poly = horner('r', exp_c)

# log: log m = 2 s (1 + z/3 + z^2/5 + ...), z = s^2, |s| <= 0.1716 -> Q(z) = sum z^k/(2k+3), k = 0..9
log_c = [float(mp.mpf(1) / (2 * k + 3)) for k in range(9, -1, -1)]
log_poly = horner('z', log_c)

# tanh small: tanh x = x + x z G(z), z = x^2 in [0, T^2]
T = mp.mpf('0.55')
zmax = T * T
deg = 13
nodes = [zmax / 2 * (1 - mp.cos(mp.pi * (2 * i + 1) / (2 * (deg + 1)))) for i in range(deg + 1)]
def g(z):
    x = mp.sqrt(z)
    return (mp.tanh(x) - x) / (x ** 3) if z != 0 else mp.mpf(-1) / 3
A = mp.matrix(deg + 1, deg + 1)
b = mp.matrix(deg + 1, 1)
for i, zi in enumerate(nodes):
    for j in range(deg + 1):
        A[i, j] = zi ** j
    b[i] = g(zi)
sol = mp.lu_solve(A, b)
tanh_c = [float(sol[j]) for j in range(deg, -1, -1)]
tanh_poly = horner('z', tanh_c)
# measure the fit error over the interval
err = max(abs(sum(sol[j] * zz ** j for j in range(deg + 1)) - g(zz)) / abs(g(zz)) for zz in [zmax * k / 997 for k in range(1, 998)])
print('tanh fit max rel err', mp.nstr(err, 5))

TEMPLATE = open(os.path.join(HERE, 'vmath.tpl')).read()
out = (TEMPLATE
       .replace('@LOG2E@', d(LOG2E)).replace('@LN2_HI@', d(LN2_HI)).replace('@LN2_LO@', d(LN2_LO))
       .replace('@EXP_POLY@', exp_poly).replace('@LOG_POLY@', log_poly).replace('@TANH_POLY@', tanh_poly)
       .replace('@TANH_T@', d(T)))
open(os.path.join(HERE, '..', '..', '..', 'lib', 'fib', 'tensor', 'vmath.fib'), 'w').write(out)
print(LN2_HI, LN2_LO)
