#!/usr/bin/env python3
# Generates the operations of the fib.decimal differential test (scripts/decimal-diff/run.sh): lines `OP ARG..`, the decimals as BigDecimal text, rounding modes as
# 0 Up, 1 Down, 2 Ceiling, 3 Floor, 4 HalfUp, 5 HalfDown, 6 HalfEven. Fixed seed. Operations that Java refuses (a zero divisor, a non-terminating exact
# division, Unnecessary that must round) are not generated: fibber traps on them where Java throws, and a trap is not a value to compare.
# Usage: gen.py SEED COUNT
import random, struct, sys

seed, count = int(sys.argv[1]), int(sys.argv[2])
r = random.Random(seed)

def digits(n, nonzero_first=True):
    s = ''.join(r.choice('0123456789') for _ in range(n))
    if nonzero_first and s and s[0] == '0':
        s = r.choice('123456789') + s[1:]
    return s

def dec(allow_zero=True):
    k = r.random()
    if allow_zero and k < 0.05:
        return r.choice(['0', '0.0', '0.00', '-0', '0E+3', '0.000'])
    sign = '-' if r.random() < 0.4 else ''
    ndig = r.choice([1, 1, 2, 3, 5, 8, 12, 20, 34, 40])
    mant = digits(ndig)
    frac = r.choice([0, 0, 1, 2, 3, 6, 10, 20])
    if frac and frac < len(mant):
        mant = mant[:-frac] + '.' + mant[-frac:]
    elif frac:
        mant = '0.' + '0' * (frac - len(mant)) + mant
    if r.random() < 0.15:
        mant += 'E' + r.choice(['+', '-', '']) + str(r.randint(0, 12))
    return sign + mant

def nonzero():
    while True:
        d = dec(False)
        if any(c in '123456789' for c in d.split('E')[0]):
            return d

def f64bits():
    k = r.random()
    if k < 0.2: x = r.choice([0.1, 0.5, 1.0, 100.0, 1e22, 1e-7, 123456.789, 5e-324, 1.7976931348623157e308, -2.5, 3.14159])
    elif k < 0.6: x = r.uniform(-1e6, 1e6)
    else: x = r.uniform(-1, 1) * 10.0 ** r.randint(-30, 30)
    return '%016x' % struct.unpack('>Q', struct.pack('>d', x))[0]

out = []
for _ in range(count):
    k = r.random()
    if k < 0.12: out.append('add %s %s' % (dec(), dec()))
    elif k < 0.22: out.append('sub %s %s' % (dec(), dec()))
    elif k < 0.32: out.append('mul %s %s' % (dec(), dec()))
    elif k < 0.38: out.append('cmp %s %s' % (dec(), dec()))
    elif k < 0.42: out.append('eq %s %s' % (dec(), dec()))
    elif k < 0.54: out.append('div %s %s %d %d' % (dec(), nonzero(), r.choice([1, 2, 3, 5, 7, 16, 34, 34, 40]), r.randint(0, 6)))
    elif k < 0.60: out.append('divs %s %s %d %d' % (dec(), nonzero(), r.randint(-3, 12), r.randint(0, 6)))
    elif k < 0.63:
        d = '%s%d' % (r.choice(['', '', '-']), r.randint(1, 10 ** 6))
        out.append('divx %s %s' % (dec(), '%d' % (2 ** r.randint(0, 6) * 5 ** r.randint(0, 5))))
    elif k < 0.72: out.append('setscale %s %d %d' % (dec(), r.randint(-4, 25), r.randint(0, 6)))
    elif k < 0.80: out.append('round %s %d %d' % (dec(), r.randint(0, 30), r.randint(0, 6)))
    elif k < 0.84: out.append('pow %s %d' % (dec(), r.randint(0, 6)))
    elif k < 0.87: out.append('strip %s' % dec())
    elif k < 0.89: out.append('plain %s' % dec())
    elif k < 0.91: out.append('prec %s' % dec())
    elif k < 0.93: out.append('str %s' % dec())
    elif k < 0.95: out.append('f64 %s' % f64bits())
    elif k < 0.97: out.append('valof %s' % f64bits())
    elif k < 0.99: out.append('tof64 %s' % dec())
    else: out.append('toi64 %s' % (('-' if r.random() < 0.5 else '') + digits(r.randint(1, 15)) + ('.' + digits(r.randint(1, 6), False) if r.random() < 0.5 else '')))
print('\n'.join(out))
