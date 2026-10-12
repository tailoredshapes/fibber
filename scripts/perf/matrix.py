#!/usr/bin/env python3
"""matrix.py CONFIGS... : for each config LABEL build F 3 times (wall/user/RSS/size/sha) then time `emit` 3 times.
Config syntax: LABEL:KEY=VAL,KEY=VAL (whole = FIB_LIRO_CACHE=off with no partition).  Results appended to results.jsonl."""
import sys, os, time, subprocess, resource, json, hashlib, re, threading

P = os.environ.get('PERF_SCRATCH', os.path.expanduser('~/.cache/fibber-scratch/perf'))
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BUILDER = os.environ.get('BUILDER', ROOT + '/build/F')
REPS = int(os.environ.get('REPS', '3'))
EREPS = int(os.environ.get('EREPS', '3'))
FR = os.environ.get('FROZEN', P + '/frozen')
BUILD = ['build', FR + '/compiler/fibc.fib', '-I', FR + '/compiler', '-I', FR + '/lib', '-L', '/usr/lib/llvm-21/lib', '-l', 'LLVM-21']


def run(cmd, env, outpath=None):
    """Run cmd; return dict with wall/user/sys/maxrss(from rusage of this child only via wait4)/stderr."""
    e = dict(os.environ)
    e.update(env)
    t0 = time.monotonic()
    p = subprocess.Popen(cmd, cwd=ROOT, env=e, stdout=open(outpath, 'wb') if outpath else subprocess.DEVNULL,
                         stderr=subprocess.PIPE, text=True)
    lines = []
    th = threading.Thread(target=lambda: [lines.append((round(time.monotonic() - t0, 2), l.rstrip())) for l in p.stderr])
    th.start()
    _, status, ru = os.wait4(p.pid, 0)
    th.join()
    wall = time.monotonic() - t0
    p.returncode = os.waitstatus_to_exitcode(status)
    return dict(rc=p.returncode, wall=round(wall, 2), user=round(ru.ru_utime, 2), sys=round(ru.ru_stime, 2),
                maxrss_mib=round(ru.ru_maxrss / 1024), load=os.getloadavg()[0], stderr=lines)


def sha(path):
    return hashlib.sha256(open(path, 'rb').read()).hexdigest()[:16]


def phases(lines):
    d = {}
    for t, l in lines:
        m = re.match(r'time (\S+) (\d+) ms', l)
        if m and m.group(1) in ('front', 'lir.emit', 'lir.check', 'link', 'llvm.lower', 'llvm.optimize', 'llvm.codegen') and m.group(1) not in d:
            d[m.group(1)] = int(m.group(2))
    units = [int(m.group(1)) for t, l in lines for m in [re.match(r'time liro.unit (\d+) ms', l)] if m]
    d['units_ms'] = sorted(units)
    return d


for cfg in sys.argv[1:]:
    label, _, envs = cfg.partition(':')
    env = dict(kv.split('=', 1) for kv in envs.split(',') if kv)
    env.update({'FIB_LIRO_CACHE': 'off', 'FIB_TIME': '1', 'FIB_LIRO_STATS': '1'})
    res = dict(label=label, env=env, builds=[], emits=[])
    shas = []
    for rep in range(REPS):
        os.makedirs(f'{P}/o{rep}', exist_ok=True)
        out = f'{P}/o{rep}/F-{label}'
        r = run([BUILDER] + BUILD + ['-o', out], env)
        r['phases'] = phases(r.pop('stderr'))
        r['size'] = os.path.getsize(out) if os.path.exists(out) else None
        r['sha'] = sha(out) if os.path.exists(out) else None
        shas.append(r['sha'])
        res['builds'].append(r)
        print(label, 'build', rep, {k: v for k, v in r.items() if k != 'phases'}, flush=True)
        if rep > 0:
            os.remove(out)
    res['deterministic'] = len(set(shas)) == 1
    f = f'{P}/o0/F-{label}'
    for rep in range(EREPS):
        r = run([f, 'emit', FR + '/compiler/fibc.fib', '-I', FR + '/compiler', '-I', FR + '/lib'], {}, outpath=f'{P}/emit-{label}.lir')
        r.pop('stderr')
        r['md5'] = hashlib.md5(open(f'{P}/emit-{label}.lir', 'rb').read()).hexdigest()
        res['emits'].append(r)
        print(label, 'emit', rep, r, flush=True)
    open(P + '/results.jsonl', 'a').write(json.dumps(res) + '\n')
