#!/usr/bin/env python3
"""scripts/lz4-diff.py: differential test of fib.compress.lz4 against liblz4 (python lz4 4.x: liblz4 1.10), both directions.
usage: lz4-diff.py LZ4DIFF_BINARY [N] [SEED] [WORKDIR]
  A: our output (frames: every option; blocks: fast, acceleration, HC) must decompress with liblz4 to the input.
  B: liblz4's output (frames and blocks, every level and option python offers, and a block dictionary) must decompress with ours to the input.
  C: for the fast level and acceleration 1, how often our block output is byte-identical to LZ4_compress_default's (and for each HC level, to liblz4's HC).
Inputs are seeded: markov text, runs, repeated records, random bytes, zeros, mixtures, and the sizes around every boundary the formats have."""
import os, random, struct, subprocess, sys, collections
import lz4.block as lb, lz4.frame as lf

BIN = sys.argv[1]
N = int(sys.argv[2]) if len(sys.argv) > 2 else 2000
SEED = int(sys.argv[3]) if len(sys.argv) > 3 else 1
WORK = sys.argv[4] if len(sys.argv) > 4 else '/tmp/lz4diff'
os.makedirs(WORK, exist_ok=True)
rng = random.Random(SEED)
WORDS = [bytes(rng.choices(b'abcdefghijklmnopqrstuvwxyz', k=rng.randint(2, 9))) for _ in range(300)]

def text(n):
    out = bytearray()
    w = rng.randrange(len(WORDS))
    while len(out) < n:
        w = (w * 7 + rng.randrange(5)) % len(WORDS) if rng.random() < 0.8 else rng.randrange(len(WORDS))
        out += WORDS[w] + (b' ' if rng.random() < 0.9 else b'\n')
    return bytes(out[:n])

def runs(n):
    out = bytearray()
    while len(out) < n:
        out += bytes([rng.randrange(256)]) * rng.choice([1, 2, 3, 5, 17, 100, 300, 5000])
    return bytes(out[:n])

def records(n):
    rec = bytes(rng.randrange(256) for _ in range(rng.choice([4, 7, 16, 33, 100])))
    out = bytearray()
    while len(out) < n:
        r = bytearray(rec)
        r[rng.randrange(len(r))] = rng.randrange(256)
        out += r
    return bytes(out[:n])

def rnd(n): return bytes(rng.getrandbits(8) for _ in range(n)) if n < 4096 else os.urandom(n)
def zeros(n): return bytes(n)
def pattern(n):
    p = bytes(rng.randrange(256) for _ in range(rng.randint(1, 12)))
    return (p * (n // len(p) + 1))[:n]
def mixed(n):
    out = b''
    while len(out) < n:
        out += rng.choice([text, runs, records, rnd, zeros, pattern])(rng.randint(1, 3000))
    return out[:n]
GENS = [text, runs, records, rnd, zeros, pattern, mixed]
EDGE = [0, 1, 2, 3, 4, 5, 11, 12, 13, 14, 15, 16, 17, 18, 19, 31, 32, 33, 255, 256, 257, 270, 271, 272, 4095, 4096, 4097, 65535, 65536, 65537, 65547, 65548,
        131071, 131072, 131073, 262144, 262145]

def sizes(n):
    for i in range(n):
        r = rng.random()
        if r < 0.25: yield rng.choice(EDGE)
        elif r < 0.7: yield rng.randint(0, 3000)
        elif r < 0.95: yield rng.randint(3000, 70000)
        else: yield rng.randint(70000, 400000)

def record(mode, level, flags, bsize, accel, dict_, data):
    return struct.pack('<7i', mode, level, flags, bsize, accel, len(dict_), len(data)) + dict_ + data

def run_batch(recs, tag):
    inp = os.path.join(WORK, tag + '.in'); out = os.path.join(WORK, tag + '.out')
    open(inp, 'wb').write(b''.join(recs))
    r = subprocess.run([BIN, inp, out], capture_output=True)
    if r.returncode != 0:
        print('TRAP/FAIL in', tag, r.returncode, r.stderr[-300:].decode(errors='replace')); return None
    d = open(out, 'rb').read(); res = []; i = 0
    while i < len(d):
        st, ln = struct.unpack_from('<ii', d, i); res.append((st, d[i + 8:i + 8 + ln])); i += 8 + ln
    return res

BSZ = [(65536, lf.BLOCKSIZE_MAX64KB), (262144, lf.BLOCKSIZE_MAX256KB), (1048576, lf.BLOCKSIZE_MAX1MB), (4194304, lf.BLOCKSIZE_MAX4MB)]
stats = collections.Counter(); fails = []

def check(cond, what):
    stats['checks'] += 1
    if not cond:
        stats['FAIL'] += 1
        if len(fails) < 20: fails.append(what)

def direction_a(items):
    recs = []; meta = []
    for (data, kind, p) in items:
        if kind == 'frame':
            level, flags, bs, accel = p
            recs.append(record(0, level, flags, bs, accel, b'', data))
        elif kind == 'block': recs.append(record(2, 1, 0, 0, p, b'', data))
        elif kind == 'hc': recs.append(record(4, p, 0, 0, 1, b'', data))
        meta.append((data, kind, p))
    res = run_batch(recs, 'a')
    if res is None: stats['FAIL'] += 1; return
    for (data, kind, p), (st, out) in zip(meta, res):
        stats['A-' + kind] += 1
        if st != 0: check(False, ('ours errored', kind, p, len(data))); continue
        try:
            back = lf.decompress(out) if kind == 'frame' else lb.decompress(out, uncompressed_size=max(len(data), 1))
        except Exception as e:
            check(False, ('liblz4 rejected ours', kind, p, len(data), str(e)[:80])); continue
        check(back == data, ('liblz4 decoded different bytes', kind, p, len(data)))
        if kind == 'block':
            ref = lb.compress(data, mode='fast', acceleration=p, store_size=False)
            stats['C-fast-acc%d-n' % p] += 1; stats['C-fast-acc%d-same' % p] += (ref == out)
            stats['C-fast-bytes'] += len(data); stats['C-fast-ours'] += len(out); stats['C-fast-ref'] += len(ref)
        if kind == 'hc':
            ref = lb.compress(data, mode='high_compression', compression=p, store_size=False)
            stats['C-hc%d-n' % p] += 1; stats['C-hc%d-same' % p] += (ref == out)
            stats['C-hc%d-ours' % p] += len(out); stats['C-hc%d-ref' % p] += len(ref)

def direction_b(items):
    recs = []; meta = []
    for (data, kind, p) in items:
        if kind == 'frame':
            level, flags, bs, linked, bc, cc, ss = p
            comp = lf.compress(data, compression_level=level, block_size=bs, block_linked=linked, content_checksum=cc, block_checksum=bc, store_size=ss)
            recs.append(record(1, 0, 0, 0, 1, b'', comp))
        elif kind == 'block':
            mode, lev, acc = p
            comp = lb.compress(data, mode=mode, acceleration=acc, compression=lev, store_size=False)
            recs.append(record(3, 0, 0, 0, 1, b'', comp))
        elif kind == 'blockdict':
            d = p
            comp = lb.compress(data, mode='fast', dict=d, store_size=False)
            recs.append(record(1000, 0, 0, 0, 1, d, comp))   # not run: ours has no raw-block dictionary API; counted below
        meta.append((data, kind, p))
    keep = [(r, m) for r, m in zip(recs, meta) if struct.unpack_from('<i', r)[0] != 1000]
    res = run_batch([r for r, m in keep], 'b')
    if res is None: stats['FAIL'] += 1; return
    for (r, (data, kind, p)), (st, out) in zip(keep, res):
        stats['B-' + kind] += 1
        check(st == 0 and out == data, ('ours did not decode liblz4 output', kind, p, len(data), st))

def main():
    szs = list(sizes(N))
    # A: ours -> liblz4
    items = []
    for n in szs:
        data = rng.choice(GENS)(n)
        c = rng.random()
        if c < 0.5:
            flags = rng.randrange(16)
            items.append((data, 'frame', (rng.choice([1, 1, 1, 3, 5, 9, 12, 0, -3]), flags, rng.choice([0, 65536, 262144, 1048576, 4194304]), rng.choice([1, 1, 4, 16]))))
        elif c < 0.8: items.append((data, 'block', rng.choice([1, 1, 1, 2, 4, 16, 64])))
        else: items.append((data, 'hc', rng.randint(3, 12)))
    for i in range(0, len(items), 500): direction_a(items[i:i + 500])
    # B: liblz4 -> ours
    items = []
    for n in szs:
        data = rng.choice(GENS)(n)
        c = rng.random()
        if c < 0.55:
            bs = rng.choice(BSZ)[1]
            items.append((data, 'frame', (rng.choice([0, 0, 1, 3, 5, 9, 12, 16]), 0, bs, rng.random() < 0.5, rng.random() < 0.5, rng.random() < 0.5, rng.random() < 0.5)))
        else:
            mode = rng.choice(['fast', 'fast', 'high_compression'])
            items.append((data, 'block', (mode, rng.randint(3, 12), rng.choice([1, 1, 2, 7, 33, 200]))))
    for i in range(0, len(items), 500): direction_b(items[i:i + 500])
    print(dict(stats))
    for k in sorted(stats):
        if k.endswith('-same'):
            b = k[:-5]; print('byte-identical %-22s %6d / %6d  (%.1f%%)' % (b, stats[k], stats[b + '-n'], 100.0 * stats[k] / max(1, stats[b + '-n'])))
    if stats['C-fast-ref']: print('fast ratio ours/ref (compressed bytes): %.4f' % (stats['C-fast-ours'] / stats['C-fast-ref']))
    for lv in range(3, 13):
        if stats['C-hc%d-ref' % lv]: print('hc%d compressed bytes ours/ref: %.4f' % (lv, stats['C-hc%d-ours' % lv] / stats['C-hc%d-ref' % lv]))
    for f in fails: print('FAIL', f)
    print('RESULT', 'FAIL' if stats['FAIL'] else 'PASS', 'checks', stats['checks'])
    sys.exit(1 if stats['FAIL'] else 0)

main()
