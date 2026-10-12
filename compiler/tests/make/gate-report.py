#!/usr/bin/env python3
"""compiler/tests/make/gate-report.py: the gate's cost report (scripts/gate-report.py) on a fixture build directory with chosen mtimes.

A run of the fixture: F (0-100 s), F.lir, F3 (100-150 s), F3.lir, a long tool (100-300 s), two short ones, all ending at known times.
Each check prints `ok N ..` or `FAIL N ..`; exit 0 when all hold. The checks can fail: the planted ones (a slow stamp, a new stamp, a slow
wall) must produce the WARN, the unchanged run must produce none, and a strict run must exit 3.
"""
import os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, '..', '..', '..', 'scripts', 'gate-report.py')
T0 = 1_000_000_000
BAD = []


def check(label, cond, detail=''):
    print(('ok   ' if cond else 'FAIL ') + label + ('' if cond else '  ' + detail))
    if not cond:
        BAD.append(label)


def put(build, name, start, secs):
    """a stamp or .secs file `name` that took `secs` and ended at start+secs (seconds after T0)."""
    p = os.path.join(build, name)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, 'w').write('%d\n' % secs)
    os.utime(p, (T0 + start + secs, T0 + start + secs))


def fixture(build, overrides=None, extra=()):
    rows = {'F.secs': (0, 100), 'F.lir.secs': (100, 20), 'F3.secs': (100, 50), 'F3.lir.secs': (150, 20),
            'tools/long.ok': (100, 200), 'tools/short-a.ok': (100, 10), 'tools/short-b.ok': (110, 10), 'fixed-point.ok': (170, 0)}
    rows.update(overrides or {})
    for name in extra:
        rows[name] = (100, 8)
    for name, (start, secs) in rows.items():
        put(build, name, start, secs)
    return [os.path.join(build, n) for n in rows if n.endswith('.ok')]


def baseline(path, wall=300, **more):
    base = {'F': 100, 'F.lir': 20, 'F3': 50, 'F3.lir': 20, 'tools/long': 200, 'tools/short-a': 10, 'tools/short-b': 10, 'fixed-point': 0}
    base.update(more)
    with open(path, 'w') as f:
        f.write('#meta\tfull\tdate=2026-01-01 commit=fixture jobs=4 wall=%d summed=0\n' % wall)
        f.write('full\t@wall\t%d\n' % wall)
        for k, v in base.items():
            f.write('full\t%s\t%d\n' % (k, v))


def report(build, bl, stamps, now=300, extra_env=None, args=()):
    env = dict(os.environ, **(extra_env or {}))
    env.pop('GATE_BUDGET_STRICT', None)
    env.update(extra_env or {})
    code = 'import sys, time; sys.argv = %r; time.time = lambda: %d; exec(open(%r).read())' % (
        ['gate-report.py', '--build', build, '--mode', 'full', '--t0', str(T0), '--jobs', '4', '--baseline', bl, '--stamps'] + stamps + list(args), T0 + now, SCRIPT)
    r = subprocess.run([sys.executable, '-c', code], capture_output=True, text=True, env=env)
    return r.returncode, r.stdout + r.stderr


def main():
    with tempfile.TemporaryDirectory() as d:
        build, bl = os.path.join(d, 'build'), os.path.join(d, 'budget.tsv')
        stamps = fixture(build)
        baseline(bl)
        rc, out = report(build, bl, stamps)
        check('1 an unchanged run: exit 0, no WARN', rc == 0 and 'WARN' not in out, out)
        check('2 wall, summed seconds and parallelism are printed', 'wall 300 s, summed 410 s over 8 stamps, average parallelism 1.4' in out, out)
        check('3 the critical path is F, then the long tool', 'critical path' in out and out.index('    F 100 s') < out.index('tools/long 200 s'), out)
        check('4 the longest are listed, longest first', 'longest: tools/long 200 s, F 100 s, F3 50 s' in out, out)
        check('5 idle time is counted (1 running below the floor of 4/4=1: none)', 'idle: 0 s of 300 with fewer than 1 running' in out, out)

        fixture(build, {'tools/short-a.ok': (100, 40)})
        rc, out = report(build, bl, stamps)
        check('6 a planted slow stamp (10 s -> 40 s) warns, by name', rc == 0 and 'WARN budget: tools/short-a took 40 s, baseline 10 s' in out, out)
        check('6b ... and only that one', out.count('WARN') == 1, out)
        fixture(build, {'tools/short-a.ok': (100, 14)})
        rc, out = report(build, bl, stamps)
        check('7 +4 s on 10 s (40%, under 5 s) does not warn', 'WARN' not in out, out)
        fixture(build, {'tools/short-a.ok': (100, 16)})
        rc, out = report(build, bl, stamps)
        check('7b +6 s on 10 s (60%, over 5 s) warns', 'tools/short-a took 16 s' in out, out)
        fixture(build, {'tools/short-a.ok': (100, 10)})

        fixture(build, extra=['tools/brand-new.ok'])
        rc, out = report(build, bl, stamps + [os.path.join(build, 'tools/brand-new.ok')])
        check('8 a stamp with no baseline entry warns', 'WARN budget: tools/brand-new (8 s) has no baseline entry' in out, out)
        os.remove(os.path.join(build, 'tools/brand-new.ok'))

        rc, out = report(build, bl, stamps, now=300 + 46)
        check('9 wall 15% over the baseline warns', 'WARN budget: wall 346 s is 15% over' in out or 'WARN budget: wall 346 s is 15% over the baseline 300 s' in out, out)
        rc, out = report(build, bl, stamps, now=300 + 44)
        check('9b wall under 15% over does not warn', 'WARN' not in out, out)

        baseline(bl)
        fixture(build, {'tools/short-a.ok': (100, 40)})
        rc, out = report(build, bl, stamps, extra_env={'GATE_BUDGET_STRICT': '1'})
        check('10 GATE_BUDGET_STRICT=1 exits 3 on a WARN', rc == 3, 'exit %d: %s' % (rc, out))
        fixture(build, {'tools/short-a.ok': (100, 10)})
        rc, out = report(build, bl, stamps, extra_env={'GATE_BUDGET_STRICT': '1'})
        check('10b ... and 0 when there is none', rc == 0, 'exit %d: %s' % (rc, out))

        os.utime(os.path.join(build, 'tools/short-b.ok'), (T0 - 50, T0 - 50))
        rc, out = report(build, bl, stamps)
        check('11 a stamp older than the start did not run: not counted, said so', 'summed 400 s over 7 stamps' in out and '1 stamps were current' in out, out)
        fixture(build)

        idle_build = os.path.join(d, 'idle')
        put(idle_build, 'tools/a.ok', 0, 50)
        put(idle_build, 'tools/b.ok', 150, 50)
        rc, out = report(idle_build, bl, [os.path.join(idle_build, 'tools/a.ok'), os.path.join(idle_build, 'tools/b.ok')], now=200,
                         )
        check('12 idle: the 100 s between two stamps (floor 1) is counted', 'idle: 100 s of 200 with fewer than 1 running' in out, out)
        check('12b ... with its longest stretch', 'longest stretch +50 s to +150 s' in out, out)

        stamps = fixture(build)
        baseline(bl)
        rc, out = report(build, bl, stamps)
        rc2 = subprocess.run([sys.executable, SCRIPT, '--build', build, '--mode', 'full', '--baseline', bl, '--write-baseline'], capture_output=True, text=True)
        check('13 --write-baseline from a complete run rewrites the rows', rc2.returncode == 0 and 'full\ttools/long\t200' in open(bl).read(), rc2.stdout + rc2.stderr)
        rc, out = report(build, bl, stamps)
        check('13b ... and the next report has no WARN', 'WARN' not in out, out)
    print('%d failed' % len(BAD) if BAD else 'all hold')
    return 1 if BAD else 0


if __name__ == '__main__':
    sys.exit(main())
