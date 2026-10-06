#!/usr/bin/env python3
"""Checks `HEXBITS TEXT` lines (from `json-floats print N`) against Python: the text must read back (float(TEXT)) to the same bits, and must be the
shortest round-trip text, digit for digit and exponent for exponent, that repr() gives (both are the closest shortest decimal). usage: json-floats-check.py < lines"""
import struct, sys
from decimal import Decimal

def norm(text):
    d = Decimal(text)
    sign, digits, exp = d.as_tuple()
    digits = list(digits)
    while len(digits) > 1 and digits[-1] == 0:
        digits.pop(); exp += 1
    return (sign, tuple(digits), exp) if digits != [0] else (sign, (0,), 0)

n = bad = bad_rt = 0
for line in sys.stdin:
    hx, text = line.split()
    bits = int(hx, 16)
    x = struct.unpack('<d', struct.pack('<Q', bits))[0]
    n += 1
    if float(text) != x or struct.unpack('<Q', struct.pack('<d', float(text)))[0] != bits:
        bad_rt += 1
        if bad_rt < 10: print('NOT ROUND TRIP', hx, text)
        continue
    if norm(text) != norm(repr(x)):
        bad += 1
        if bad < 10: print('NOT SHORTEST/CLOSEST', hx, text, 'python', repr(x))
    if not any(c in text for c in '.eE'):
        bad += 1
        if bad < 10: print('NOT FLOAT SHAPED', text)
print('printed doubles checked:', n, 'round-trip failures', bad_rt, 'shortest mismatches', bad)
sys.exit(1 if bad or bad_rt else 0)
