#!/usr/bin/env python3
"""Writes the edge-case corpus of scripts/json-floats.sh: lines `TEXT HEXBITS`, the bits being Python's float(TEXT), which is correctly rounded.

Cases: the boundaries of the double range; for random doubles d (uniform bits, subnormals, powers of two, near-integers) the exact decimal text of the
halfway point between d and its successor, and that text nudged one unit in its 40th digit either way (the three decide round-half-even and the
sticky bit); 17-significant-digit ties; short texts with large exponents; texts with 19 to 800 digits.
usage: json-floats.py N > corpus.txt   (fixed seed)"""
import random, struct, sys
from decimal import Decimal, getcontext
from fractions import Fraction

getcontext().prec = 1200
rnd = random.Random(20261006)


def bits(x):
    return struct.unpack('<Q', struct.pack('<d', x))[0]


def frombits(b):
    return struct.unpack('<d', struct.pack('<Q', b))[0]


def emit(text):
    try:
        x = float(text)
    except OverflowError:
        return
    if x != x or x in (float('inf'), float('-inf')):
        return
    print(text, '%016x' % (bits(x) if bits(x) < 2 ** 63 else bits(x) - 2 ** 64 if False else bits(x)))


def signed(b):
    return b - 2 ** 64 if b >= 2 ** 63 else b


def emit(text):  # noqa: F811  (the bits are printed as a signed hex-free decimal word: the fibber side reads 16 hex digits into an i64)
    try:
        x = float(text)
    except OverflowError:
        return
    if x == float('inf') or x == float('-inf'):
        return
    print(text, '%016x' % bits(x))


def midpoint_text(d):
    nxt = frombits(bits(d) + 1)
    m = (Fraction(d) + Fraction(nxt)) / 2
    dec = Decimal(m.numerator) / Decimal(m.denominator)  # exact: the denominator is a power of two and the precision is ample
    return dec


def main(n):
    for t in ['5e-324', '2.4703282292062327e-324', '2.4703282292062328e-324', '2.2250738585072014e-308', '2.2250738585072011e-308',
              '1.7976931348623157e308', '1.7976931348623158e308', '9007199254740993', '9007199254740992.5', '4.9406564584124654e-324',
              '1e22', '1e23', '8.41e21', '1.5e300', '0.000001', '123456789012345678', '0.1', '0.3', '2.5e-5', '7.0e-10']:
        emit(t)
    for _ in range(n):
        kind = rnd.randrange(5)
        if kind == 0:
            b = rnd.getrandbits(63)
        elif kind == 1:
            b = rnd.getrandbits(52)  # subnormal
        elif kind == 2:
            b = (rnd.randrange(1, 2046) << 52)  # a power of two
        elif kind == 3:
            b = (1023 + rnd.randrange(-60, 60)) << 52 | rnd.getrandbits(52)
        else:
            b = rnd.getrandbits(63)
        d = frombits(b)
        if d != d or d == float('inf') or b >= 0x7FEFFFFFFFFFFFFF:
            continue
        m = midpoint_text(d)
        s = format(m, 'f') if abs(m.adjusted()) < 400 else str(m)
        emit(('%s' % m).replace('E+', 'e').replace('E-', 'e-') if 'E' in str(m) else str(m))
        ex = m.adjusted()
        for nudge in (1, -1):
            digits = m.as_tuple()
            nm = m + nudge * Decimal(10) ** (ex - 40)
            emit(str(nm).replace('E+', 'e').replace('E-', 'e-'))
        # a 17-digit text of d itself and of its midpoint rounded to 17 digits
        emit(repr(d))
        emit('%.17g' % d)
        emit('%.16e' % d)
    for _ in range(n // 4):
        digits = ''.join(rnd.choice('0123456789') for _ in range(rnd.choice([19, 20, 25, 40, 100, 800])))
        emit('0.' + digits + 'e%d' % rnd.randrange(-300, 300))
        emit(digits[:1] + '.' + digits[1:] + 'e%d' % rnd.randrange(-320, 310))


main(int(sys.argv[1]) if len(sys.argv) > 1 else 20000)
