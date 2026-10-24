import sys, subprocess, bisect, collections
exe, samples = sys.argv[1], sys.argv[2]
syms = []
for l in subprocess.run(['nm', '-C', '--defined-only', exe], capture_output=True, text=True).stdout.split('\n'):
    p = l.split(' ', 2)
    if len(p) == 3 and p[1] in 'tTwW':
        syms.append((int(p[0], 16), p[2]))
syms.sort(); addrs = [a for a, _ in syms]
c = collections.Counter(); tot = 0
for l in open(samples):
    a = int(l, 16); tot += 1
    k = bisect.bisect_right(addrs, a) - 1
    c[syms[k][1] if k >= 0 and a - addrs[k] < 1 << 20 else '?'] += 1
for s, n in c.most_common(int(sys.argv[3]) if len(sys.argv) > 3 else 30):
    print(f'{100*n/tot:5.1f}% {n:6d} {s}')
print('total', tot)
