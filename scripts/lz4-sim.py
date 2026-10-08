import lz4.block as lb, random, sys
M64 = (1 << 64) - 1
def h4(d, i, bits): return ((int.from_bytes(d[i:i+4], 'little') * 2654435761) & 0xffffffff) >> (32 - bits)
def h5(d, i, bits): return ((((int.from_bytes(d[i:i+8].ljust(8, b'\0'), 'little') << 24) & M64) * 889523592379) & M64) >> (64 - bits)

def seq(out, d, anchor, ip, off, ml):
    lit = ip - anchor
    tok = (min(lit, 15) << 4) | (min(ml - 4, 15) if ml else 0)
    out.append(tok)
    if lit >= 15:
        r = lit - 15
        while r >= 255: out.append(255); r -= 255
        out.append(r)
    out.extend(d[anchor:ip])
    if ml:
        out.extend([off & 255, off >> 8])
        if ml - 4 >= 15:
            r = ml - 4 - 15
            while r >= 255: out.append(255); r -= 255
            out.append(r)

def compress(d, hashf, bits, accel=1):
    n = len(d); out = bytearray()
    if n < 13: seq(out, d, 0, n, 0, 0); return bytes(out)
    table = {}
    mflimit1 = n - 12 + 1; matchlimit = n - 5
    anchor = 0; ip = 1
    table[hashf(d, 0, bits)] = 0
    while True:
        # search
        step = 1; nb = accel << 6; fwd = ip
        found = False
        while True:
            ip = fwd; fwd += step; step = nb >> 6; nb += 1
            if fwd > mflimit1: break
            h = hashf(d, ip, bits)
            cand = table.get(h, 0)   # an empty slot is index 0 (liblz4 zeroes the table)
            table[h] = ip
            if ip - cand <= 65535 and cand < ip and d[cand:cand+4] == d[ip:ip+4]:
                found = True; break
        if not found: break
        while ip > anchor and cand > 0 and d[ip-1] == d[cand-1]: ip -= 1; cand -= 1
        # match length
        ml = 4
        while ip + ml < matchlimit and d[ip+ml] == d[cand+ml]: ml += 1
        seq(out, d, anchor, ip, ip - cand, ml)
        ip += ml; anchor = ip
        if ip >= mflimit1: break
        table[hashf(d, ip - 2, bits)] = ip - 2
        # immediate test
        while True:
            h = hashf(d, ip, bits); cand = table.get(h, 0); table[h] = ip
            if ip - cand <= 65535 and cand < ip and d[cand:cand+4] == d[ip:ip+4]:
                while ip > anchor and cand > 0 and d[ip-1] == d[cand-1]: ip -= 1; cand -= 1
                ml = 4
                while ip + ml < matchlimit and d[ip+ml] == d[cand+ml]: ml += 1
                seq(out, d, anchor, ip, ip - cand, ml)
                ip += ml; anchor = ip
                if ip >= mflimit1: break
                table[hashf(d, ip - 2, bits)] = ip - 2
            else:
                ip += 1; break
        if ip >= mflimit1: break
    seq(out, d, anchor, n, 0, 0)
    return bytes(out)

rng = random.Random(3)
WORDS = [b'the', b'of', b'and', b'compression', b'block', b'frame', b'literal', b'match']
def text(n):
    o = b''
    while len(o) < n: o += rng.choice(WORDS) + b' '
    return o[:n]
cases = []
for n in range(13, 200):
    cases.append(text(n)); cases.append(bytes(rng.randrange(4) for _ in range(n)))
for name, hf, bits in [('h4-13', h4, 13), ('h5-13', h5, 13), ('h5-12', h5, 12), ('h4-12', h4, 12)]:
    bad = 0; first = None
    for d in cases:
        ref = lb.compress(d, mode='fast', acceleration=1, store_size=False)
        mine = compress(d, hf, bits)
        if mine != ref:
            bad += 1
            if first is None or len(d) < len(first[0]): first = (d, mine, ref)
    print(name, bad, 'of', len(cases))
    if first and len(sys.argv) > 1 and sys.argv[1] == name: print(first[0], '\n mine', first[1].hex(), '\n ref ', first[2].hex())
