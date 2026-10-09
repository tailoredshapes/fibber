#!/usr/bin/env python3
# scripts/bench/sched-matrix.py (SCHED-2): the interleaved median-of-N matrix of FIB_SPINNERS and friends over case 8645/8642, sched-fork rows, parallel-scale and lz4-bench; run from a directory with BINDIR/{c8645,c8642,sfork,pscale,lz4b} (docs/design/parallelism.md section 8).
# usage: h.py BINDIR WLIST ROWS [REPS]   e.g. h.py x86 4,8,16 c8645,c8642,storm,storm-main,wide,pfib,pmap,preduce,pforalloc 5
import os, subprocess, time, statistics, sys, json, platform, resource
bindir = sys.argv[1]; Ws = [int(x) for x in sys.argv[2].split(',')]; rows = sys.argv[3].split(','); REPS = int(sys.argv[4]) if len(sys.argv) > 4 else 5
INF = '1000000'
OLD = os.environ.get('OLD', 'x86')
CONFIGS = [  # name, env
 ('sp0', {'FIB_SPINNERS': '0'}), ('sp1', {'FIB_SPINNERS': '1'}), ('sp2(def)', {}), ('sp4', {'FIB_SPINNERS': '4'}),
 ('sp5', {'FIB_SPINNERS': '5'}), ('sp8', {'FIB_SPINNERS': '8'}), ('sp10', {'FIB_SPINNERS': '10'}), ('sp16', {'FIB_SPINNERS': '16'}),
 ('inf', {'FIB_SPINNERS': INF}),
 ('inf noyield', {'FIB_SPINNERS': INF, 'FIB_SPIN_YIELD_AFTER': '1000'}),
 ('sp2 noyield', {'FIB_SPINNERS': '2', 'FIB_SPIN_YIELD_AFTER': '1000'}),
 ('inf yield@8', {'FIB_SPINNERS': INF, 'FIB_SPIN_YIELD_AFTER': '8'}),
 ('sp4 noyield', {'FIB_SPINNERS': '4', 'FIB_SPIN_YIELD_AFTER': '1000'}),
 ('sp2 spin320', {'FIB_SPINNERS': '2', 'FIB_SPIN': '320'}), ('sp2 spin3200', {'FIB_SPINNERS': '2', 'FIB_SPIN': '3200'}),
 ('sp2 spin32000', {'FIB_SPINNERS': '2', 'FIB_SPIN': '32000'}),
 ('sp4 spin320', {'FIB_SPINNERS': '4', 'FIB_SPIN': '320'}), ('sp4 spin3200', {'FIB_SPINNERS': '4', 'FIB_SPIN': '3200'}),
 ('sp4 spin32000', {'FIB_SPINNERS': '4', 'FIB_SPIN': '32000'}),
 ('before', {'_BIN': OLD}), ('sp2 explicit', {'FIB_SPINNERS': '2'}),
 ('inf hint', {'FIB_SPINNERS': INF, 'FIB_SPIN_HINT': '1'}), ('sp2 hint', {'FIB_SPINNERS': '2', 'FIB_SPIN_HINT': '1'}),
 ('sp4 hint', {'FIB_SPINNERS': '4', 'FIB_SPIN_HINT': '1'}), ('sp8 hint', {'FIB_SPINNERS': '8', 'FIB_SPIN_HINT': '1'}),
 ('inf hint noyield', {'FIB_SPINNERS': INF, 'FIB_SPIN_HINT': '1', 'FIB_SPIN_YIELD_AFTER': '1000'}),
]
only = os.environ.get('CFGS')
if only: CONFIGS = [c for c in CONFIGS if c[0] in only.split(',')]
mac = platform.system() == 'Darwin'
def lim():
    if not mac: resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
def run(cmd, env, w, cwd=None):
    e = dict(os.environ, MALLOC_ARENA_MAX='2', FIB_THREADS=str(w))
    for k in ('FIB_SPINNERS', 'FIB_SPIN_YIELD_AFTER', 'FIB_SPIN', 'FIB_SPIN_HINT'): e.pop(k, None)
    e.update(env)
    t = time.perf_counter()
    p = subprocess.run(cmd, env=e, capture_output=True, text=True, cwd=cwd, preexec_fn=lim)
    return time.perf_counter() - t, p
med = statistics.median
def load(): return os.popen('uptime').read().strip()
SF = {'storm': 300000, 'storm-main': 300000, 'wide': 100000, 'pfib': 40}
LZ = {'lz4c': 'f', 'lz4d': 'fd'}
PS = {'pmap': 10000000, 'preduce': 1000000000, 'pforalloc': 10000000}
def cell(row, w, name, env0):
    env = dict(env0); bindir_ = env.pop('_BIN', bindir)
    if row in ('c8645', 'c8642'):
        dt, p = run(['./%s/%s' % (bindir_, row)], env, w)
        return dt * 1000 if p.returncode == 0 else None
    if row in SF:
        dt, p = run(['./%s/sfork' % bindir_, row, str(SF[row]), '1'], env, w)
        try: return int(p.stdout.split('us=')[1].split()[0]) / 1000.0
        except Exception: return None
    if row in PS:
        mode = 'pfor-alloc' if row == 'pforalloc' else row
        dt, p = run(['./%s/pscale' % bindir_, mode, str(w), str(PS[row])], env, w)
        calls = [int(l.split()[2]) for l in p.stdout.splitlines() if l.startswith('t call')]
        return med(calls) / 1000.0 if p.returncode == 0 and len(calls) == 3 else None
    if row in LZ:
        dt, p = run([os.path.expanduser('~/fibber-scratch/sched2/%s/lz4b' % bindir_), LZ[row], 'silesia/all', '1', '5', str(w), '1048576'], env, w, cwd=os.path.expanduser('~/fibber-scratch/lz4'))
        try: return float(p.stdout.split()[-2])
        except Exception: return None
    return None
skip = lambda row, name: name == 'sp0' and row != 'c8645'
print('machine', platform.node(), platform.machine(), 'cpus', os.cpu_count(), flush=True)
print('load before:', load(), flush=True)
res = {}
for rep in range(REPS):
    for row in rows:
        for w in Ws:
            for name, env in CONFIGS:
                if skip(row, name): continue
                res.setdefault((row, w, name), []).append(cell(row, w, name, env))
    print('rep', rep, load(), flush=True)
print('load after:', load(), flush=True)
print('| config | ' + ' | '.join('%s W=%d' % (r, w) for r in rows for w in Ws) + ' |')
for name, env in CONFIGS:
    cells = []
    for r in rows:
        for w in Ws:
            v = res.get((r, w, name))
            cells.append('-' if v is None else ('%.1f' % med(v) if all(x is not None for x in v) else 'n/a'))
    print('| %s | %s |' % (name, ' | '.join(cells)))
print('raw:', json.dumps({'|'.join(map(str, k)): v for k, v in res.items()}))
