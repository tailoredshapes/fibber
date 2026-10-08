import lz4.block as lb, sys
# the generator of specs/compress-lz4-hc-spec.fib, and liblz4's HC output for it: (size, FNV-1a of the output) per input and level
def gen(seed, n, mode):
    x = seed
    def nxt():
        nonlocal x
        x = (x * 1103515245 + 12345) & 0x7fffffff
        return x >> 8
    out = bytearray()
    copy_p, run_p, maxd = [(7, 50, 3000), (3, 40, 60000), (11, 9, 300), (5, 300, 65000), (2, 90, 1000), (16, 12, 40000)][mode]
    while len(out) < n:
        r = nxt()
        if len(out) > 20 and r % copy_p == 0:
            d = 1 + nxt() % min(len(out), maxd)
            l = 4 + nxt() % 40
            for _ in range(l):
                out.append(out[-d])
        elif r % run_p == 0:
            b = 97 + nxt() % 13
            l = 20 + nxt() % 600
            out += bytes([b]) * l
        else:
            out.append(97 + r % 13)
    return bytes(out[:n])
def fnv(b):
    h = 2166136261
    for c in b:
        h = ((h ^ c) * 16777619) & 0xFFFFFFFF
    return h
cases = [(1, 30000, 0), (2, 80000, 1), (3, 150000, 2), (4, 60000, 3), (5, 200000, 4), (6, 100000, 5), (7, 12345, 0), (8, 70001, 3)]
levels = [3, 5, 9, 10, 11, 12]
print("[" + " ".join("[%d %d %d]" % c for c in cases) + "]")
rows = []
for (s, n, m) in cases:
    d = gen(s, n, m)
    row = []
    for lv in levels:
        z = lb.compress(d, mode='high_compression', compression=lv, store_size=False)
        row.append("%d %d" % (len(z), fnv(z)))
    rows.append("[" + " ".join(row) + "]")
print("[" + "\n ".join(rows) + "]")
