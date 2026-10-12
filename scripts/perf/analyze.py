#!/usr/bin/env python3
"""analyze.py PROF.json SCRIPT.txt [phase-bounds...] : per-phase leaf/inclusive symbol tables from perf script output."""
import sys, json, re, collections
prof = json.load(open(sys.argv[1]))
mono0 = prof['mono0']
# phase ends from stderr "time X N ms" lines (reported when the phase ends)
ends = []
for t, l in prof['stderr']:
    m = re.match(r'time (\S+) (\d+) ms', l)
    if m:
        ends.append((m.group(1), t))
phases = []
prev = 0.0
for name, t in ends:
    phases.append((name, prev, t))
    prev = t
phases.append(('link+tail', prev, 1e9))
hdr = re.compile(r'^(\S+)\s+(\d+\.\d+):')
samples = []
cur = None
for line in open(sys.argv[2]):
    m = hdr.match(line)
    if m:
        if cur: samples.append(cur)
        cur = [float(m.group(2)) - mono0, m.group(1), []]
    elif line.startswith('\t') and cur is not None:
        s = line.strip()
        mm = re.match(r'[0-9a-f]+ (.*) \((.*)\)$', s)
        if mm: cur[2].append((mm.group(1), mm.group(2)))
if cur: samples.append(cur)
samples = [s for s in samples if s[1] != 'python3' and s[1] != 'perf']
print(f"samples (excluding python): {len(samples)}")
topn = 20
for name, a, b in phases:
    ss = [s for s in samples if a <= s[0] < b]
    if not ss: continue
    leaf = collections.Counter()
    incl = collections.Counter()
    dso = collections.Counter()
    for t, comm, fr in ss:
        if not fr: continue
        sym, d = fr[0]
        if d == '[unknown]' and sym == '[unknown]': sym, d = '[kernel]', '[kernel]'
        leaf[sym] += 1
        dso[d.split('/')[-1]] += 1
        for sy in set(f[0] for f in fr): incl[sy] += 1
    n = len(ss)
    print(f"\n== {name}: {a:.1f}-{min(b, prof['wall']):.1f}s, {n} samples (~{n/199:.1f} s CPU)")
    print("  by object: " + ", ".join(f"{k} {100*v/n:.0f}%" for k, v in dso.most_common(5)))
    print("  top leaf symbols:")
    for k, v in leaf.most_common(topn): print(f"    {100*v/n:5.1f}%  {k[:90]}")
    print("  top inclusive (callchain) symbols:")
    for k, v in incl.most_common(topn + 4): print(f"    {100*v/n:5.1f}%  {k[:90]}")
