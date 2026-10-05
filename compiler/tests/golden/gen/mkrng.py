import sys
M = (1 << 64) - 1
def sg(x): return x - (1 << 64) if x >= 1 << 63 else x
class R:
    def __init__(s, seed): s.s = seed & M
    def nxt(s):
        s.s = (s.s + 0x9E3779B97F4A7C15) & M
        z = s.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M
        return z ^ (z >> 31)
    def below(s, n): return 0 if n == 0 else s.nxt() % n
    def rng(s, lo, hi): return lo + s.below(max(hi - lo + 1, 1))
lines = ['# seed (signed i64 bits of the u64), then: next x5 (signed); below 7 x8; below 1000003 x8; range -3 9 x8; chance 60 x8 (1/0); weighted [3 0 5 1] x8',
         '# computed by compiler/tests/golden/gen/mkrng.py from the SplitMix64 algorithm of crates/fibgen/src/rng.rs (seed-1); the corpus manifests are the check against the Rust binary itself']
for seed in (0, 1, 42, 1409, 123456789, M, 1 << 63):
    r = R(seed); a = [str(sg(r.nxt())) for _ in range(5)]
    r = R(seed); b = [str(r.below(7)) for _ in range(8)]
    r = R(seed); c = [str(r.below(1000003)) for _ in range(8)]
    r = R(seed); d = [str(r.rng(-3, 9)) for _ in range(8)]
    r = R(seed); e = [str(1 if r.below(100) < 60 else 0) for _ in range(8)]
    r = R(seed); f = []
    for _ in range(8):
        x = r.below(9); i = 0; w = [3, 0, 5, 1]
        while x >= w[i]: x -= w[i]; i += 1
        f.append(str(i))
    lines.append('\t'.join([str(sg(seed)), ' '.join(a), ' '.join(b), ' '.join(c), ' '.join(d), ' '.join(e), ' '.join(f)]))
open(sys.argv[1], 'w').write('\n'.join(lines) + '\n')
