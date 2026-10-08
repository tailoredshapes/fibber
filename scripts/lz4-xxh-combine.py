import struct, random
# xxh32 is not combinable: find two 16-byte inputs A1 != A2 with the same xxh32 and show xxh32(A1||B) != xxh32(A2||B)
P1, P2, P3, P4, P5 = 2654435761, 2246822519, 3266489917, 668265263, 374761393
M = 0xFFFFFFFF
def rotl(x, n): return ((x << n) | (x >> (32 - n))) & M
def xxh32(b, seed=0):
    n = len(b); i = 0
    if n >= 16:
        v = [(seed + P1 + P2) & M, (seed + P2) & M, seed, (seed - P1) & M]
        while i + 16 <= n:
            for k in range(4):
                x = struct.unpack_from('<I', b, i + 4 * k)[0]
                v[k] = (rotl((v[k] + x * P2) & M, 13) * P1) & M
            i += 16
        h = (rotl(v[0], 1) + rotl(v[1], 7) + rotl(v[2], 12) + rotl(v[3], 18)) & M
    else:
        h = (seed + P5) & M
    h = (h + n) & M
    while i + 4 <= n:
        h = (rotl((h + struct.unpack_from('<I', b, i)[0] * P3) & M, 17) * P4) & M; i += 4
    while i < n:
        h = (rotl((h + b[i] * P5) & M, 11) * P1) & M; i += 1
    h ^= h >> 15; h = (h * P2) & M; h ^= h >> 13; h = (h * P3) & M; h ^= h >> 16
    return h
assert xxh32(b"") == 0x02CC5D05 and xxh32(b"Nobody inspects the spammish repetition") == 0xE2293B2F
rnd = random.Random(1)
seen = {}
B = bytes(range(64))
tries = 0
while True:
    a = rnd.randbytes(16); h = xxh32(a); tries += 1
    if h in seen and seen[h] != a:
        a1 = seen[h]; a2 = a; break
    seen[h] = a
print("collision after", tries, "inputs: xxh32(A1) = xxh32(A2) =", hex(h))
print("xxh32(A1||B) =", hex(xxh32(a1 + B)), " xxh32(A2||B) =", hex(xxh32(a2 + B)), " equal:", xxh32(a1 + B) == xxh32(a2 + B))
# and the same for 2 stripes and a long B, to show that it is the lane states that matter
print("with B of 4096 bytes:", hex(xxh32(a1 + rnd.randbytes(4096))), "(different B per call: only to show it runs)")
