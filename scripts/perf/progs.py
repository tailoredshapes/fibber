#!/usr/bin/env python3
"""Build time (3x) and run time (3x) of benchmark programs under several unit modes, with the whole-program-built F."""
import os, sys, subprocess, time, resource, statistics as st, json, shutil

P = os.environ.get('PERF_SCRATCH', os.path.expanduser('~/.cache/fibber-scratch/perf'))
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
F = os.environ.get('FIBC', ROOT + '/build/F')
PROGS = sys.argv[1].split(',')
MODES = {
    'whole': dict(FIB_LIRO_CACHE='off'),
    'units-serial-cold': dict(FIB_LIRO_CACHE='COLD'),
    'units-par8-off': dict(FIB_LIRO_CACHE='off', FIB_LIRO_PARTITION='1', FIB_LIRO_JOBS='8'),
    'import40-par8': dict(FIB_LIRO_CACHE='off', FIB_LIRO_PARTITION='1', FIB_LIRO_JOBS='8', FIB_LIRO_IMPORT='40'),
    'import80-par8': dict(FIB_LIRO_CACHE='off', FIB_LIRO_PARTITION='1', FIB_LIRO_JOBS='8', FIB_LIRO_IMPORT='80'),
    'import80-serial-cold': dict(FIB_LIRO_CACHE='COLD', FIB_LIRO_IMPORT='80'),
}


def timed(cmd, env, **kw):
    e = dict(os.environ)
    e.update(env)
    t0 = time.monotonic()
    p = subprocess.Popen(cmd, cwd=ROOT, env=e, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **kw)
    _, status, ru = os.wait4(p.pid, 0)
    return time.monotonic() - t0, ru.ru_utime + ru.ru_stime, ru.ru_maxrss // 1024


for prog in PROGS:
    for mode, env in MODES.items():
        env = dict(env)
        walls, cpus, rss = [], [], []
        for rep in range(3):
            if env.get('FIB_LIRO_CACHE') == 'COLD':
                d = f'{P}/pcache'
                shutil.rmtree(d, ignore_errors=True)
                env['FIB_LIRO_CACHE'] = d
                cold = True
            else:
                cold = False
            out = f'{P}/prog-{prog}-{mode}'
            w, c, r = timed([F, 'build', f'scripts/bench/{prog}.fib', '-o', out], env)
            if cold:
                env['FIB_LIRO_CACHE'] = 'COLD'
            walls.append(w); cpus.append(c); rss.append(r)
        runs = []
        for rep in range(3):
            w, c, r = timed([out], {})
            runs.append(w)
        print(f"{prog:12s} {mode:20s} build wall min/med {min(walls):6.2f}/{st.median(walls):6.2f} cpu med {st.median(cpus):6.1f} "
              f"rss {max(rss)} MiB  size {os.path.getsize(out)}  run min/med {min(runs):5.2f}/{st.median(runs):5.2f} load {os.getloadavg()[0]:.1f}", flush=True)
