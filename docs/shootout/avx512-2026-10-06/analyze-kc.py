import csv, os, sys, statistics
S = '/home/tmarsh/.cache/fibber-scratch/avx512/'
for m in ('spr', 'zen4'):
    R = S + 'results-%s/results/r2/' % m
    rows = {}
    for kc in (64, 128, 256, 384, 512):
        for r in csv.DictReader(open(R + 'kc%d.tsv' % kc), delimiter='\t'):
            rows.setdefault(r['label'], {})[kc] = (float(r['fib_ms']), r['numpy_openblas_ms'])
    print('###', m, '(ms; matmul GFLOPS in brackets)')
    print('| kernel | kc64 | kc128 | kc256 | kc384 | kc512 | OpenBLAS |')
    print('|---|---|---|---|---|---|---|')
    import re
    for l, d in rows.items():
        mm = re.match(r'matmul (f32|f64) (\d+)', l)
        cells = []
        for kc in (64, 128, 256, 384, 512):
            ms = d[kc][0]
            cells.append('%.2f' % ms + (' (%.0f)' % (2.0 * int(mm.group(2)) ** 3 / (ms * 1e6)) if mm else ''))
        ob = d[128][1]
        print('| %s | %s | %s |' % (l, ' | '.join(cells), ob if ob != 'None' else '-'))
    ps = {}
    for line in open(R + 'poly.txt'):
        w = line.split()
        ps.setdefault(w[0], []).append(float(w[1]))
    print('poly', {k: statistics.median(v) for k, v in ps.items()}, {k: v for k, v in ps.items()})
    print()
