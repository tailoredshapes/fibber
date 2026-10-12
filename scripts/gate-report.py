#!/usr/bin/env python3
"""scripts/gate-report.py: what a gate run cost, and whether it cost too much (docs/design/build.md, the gate budget).

From the stamps under the build directory (a stamp holds its elapsed seconds, its mtime is when it ended; so are the `X.secs` files of the
products that are no stamp: F, F3, the emitted lIR, the case shards) it prints wall seconds, summed seconds, average parallelism, the
critical path, the longest stamps and the idle time, and compares them with scripts/gate-budget.tsv.

  gate-report.py --build build --mode full|quick --stamps S.. [--t0 EPOCH] [--jobs N] [--baseline FILE]   the report
  gate-report.py ... --write-baseline        rewrite the rows of the mode from the last complete run (make gate-baseline)

WARN lines never fail the gate; with GATE_BUDGET_STRICT=1 they exit 3.
"""
import argparse, json, os, subprocess, sys, time

WALL_SLACK = 1.15        # wall over the baseline by more than 15%
STAMP_SLACK = 1.5        # a stamp over its recorded time by more than 50% ...
STAMP_FLOOR = 5          # ... and by more than 5 s
PARTIAL = 0.9            # a run that did less than 90% of the baseline's work has no wall to compare


def read_secs(path):
    try:
        text = open(path).read().strip()
    except OSError:
        return None
    return int(text) if text.isdigit() else None


def collect(build, stamps, t0):
    """name -> (start, end, secs, ran) for the stamps of the mode and every X.secs under build."""
    files = {}
    for s in stamps:
        if s.endswith('.ok'):
            files[os.path.relpath(s, build)[:-3]] = s
    for d, _, names in os.walk(build):
        for n in names:
            if n.endswith('.secs'):
                p = os.path.join(d, n)
                files.setdefault(os.path.relpath(p, build)[:-5], p)
    nodes = {}
    for name, p in files.items():
        if name + '.0' in files:    # the slowest of a directory's shards, kept beside the shards: not another run
            continue
        secs = read_secs(p)
        if secs is None:
            continue
        end = os.path.getmtime(p)
        nodes[name] = (end - secs, end, secs, t0 is None or end >= t0)
    return nodes


def critical_path(nodes):
    """The chain of nodes that ends last, each starting when the one before ended (within 1 s of the clock's resolution)."""
    ran = {k: v for k, v in nodes.items() if v[3] and v[2] > 0}    # a stamp of 0 s is a check, not a wait anybody saw
    if not ran:
        return []
    cur = max(ran, key=lambda k: ran[k][1])
    chain = [cur]
    while True:
        start = ran[cur][0]
        before = [k for k in ran if k != cur and ran[k][1] <= start + 1.0 and k not in chain]
        if not before:
            return chain[::-1]
        cur = max(before, key=lambda k: (ran[k][1], ran[k][2]))
        chain.append(cur)


def idle(nodes, t0, t1, jobs):
    """Seconds in [t0, t1) with fewer than jobs/4 nodes running, and the longest such stretch as (from, to)."""
    ran = [v for v in nodes.values() if v[3]]
    floor = max(1.0, jobs / 4.0)
    total, best, cur_from = 0, (0, 0), None
    n = int(t1 - t0 + 0.999)
    for t in range(n + 1):
        low = t < n and sum(1 for v in ran if v[0] <= t0 + t + 0.5 < v[1]) < floor
        total += low
        if low and cur_from is None:
            cur_from = t
        elif not low and cur_from is not None:
            best = (cur_from, t) if t - cur_from > best[1] - best[0] else best
            cur_from = None
    return total, best, floor


def read_baseline(path, mode):
    rows, meta = {}, ''
    try:
        lines = open(path).read().splitlines()
    except OSError:
        return rows, meta
    for ln in lines:
        if ln.startswith('#meta\t%s\t' % mode):
            meta = ln.split('\t', 2)[2]
        elif ln and not ln.startswith('#'):
            m, name, secs = ln.split('\t')
            if m == mode:
                rows[name] = int(secs)
    return rows, meta


def compare(nodes, wall, base, complete_enough):
    warns = []
    if not base:
        return warns
    if '@wall' in base and complete_enough and wall > base['@wall'] * WALL_SLACK:
        warns.append('wall %d s is %d%% over the baseline %d s' % (wall, 100 * (wall - base['@wall']) / base['@wall'], base['@wall']))
    for name in sorted(nodes):
        _, _, secs, ran = nodes[name]
        if not ran:
            continue
        if name not in base:
            warns.append('%s (%d s) has no baseline entry' % (name, secs))
        elif secs > base[name] * STAMP_SLACK and secs - base[name] > STAMP_FLOOR:
            warns.append('%s took %d s, baseline %d s' % (name, secs, base[name]))
    return warns


def describe(nodes, chain, t0):
    out, prev_end = [], t0
    for k in chain:
        s, e, secs, _ = nodes[k]
        wait = s - prev_end
        out.append('    %s %d s%s' % (k, secs, ('  (+%d s before it started)' % wait) if wait >= 2 else ''))
        prev_end = e
    return out


def write_baseline(path, mode, record, nodes, args):
    ran = {k: v for k, v in nodes.items() if v[3]}
    if record['cached']:
        sys.exit('gate-report: not a clean run: %d stamps were current and did not run (%s ...); make clean-build, then run the gate on a quiet box'
                 % (len(record['cached']), ', '.join(record['cached'][:3])))
    keep = [ln for ln in (open(path).read().splitlines() if os.path.exists(path) else [])
            if ln and not ln.startswith('#meta\t%s\t' % mode) and not (not ln.startswith('#') and ln.split('\t')[0] == mode)]
    head = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True).stdout.strip()
    meta = 'date=%s commit=%s jobs=%d wall=%d summed=%d' % (time.strftime('%Y-%m-%d'), head, record['jobs'], record['wall'], record['summed'])
    keep.append('#meta\t%s\t%s' % (mode, meta))
    keep.append('%s\t@wall\t%d' % (mode, record['wall']))
    keep += ['%s\t%s\t%d' % (mode, k, ran[k][2]) for k in sorted(ran)]
    header = ['# scripts/gate-budget.tsv: the cost of the gate when it was last accepted (docs/design/build.md, the gate budget).',
              '# mode, stamp (build/NAME.ok or build/NAME.secs), seconds. Rewritten by `make gate-baseline` from a clean quiet run: a reviewed change.']
    body = [ln for ln in keep if not ln.startswith('# ')]
    open(path, 'w').write('\n'.join(header + sorted(body, key=lambda ln: (not ln.startswith('#'), ln))) + '\n')
    print('gate-report: baseline rows of %s rewritten in %s (%s)' % (mode, path, meta))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--build', default='build'); ap.add_argument('--mode', default='full')
    ap.add_argument('--stamps', nargs='*', default=[]); ap.add_argument('--t0', type=float)
    ap.add_argument('--jobs', type=int, default=1); ap.add_argument('--baseline', default='scripts/gate-budget.tsv')
    ap.add_argument('--write-baseline', action='store_true')
    args = ap.parse_args()
    runfile = os.path.join(args.build, 'gate-%s.run' % args.mode)
    if args.write_baseline:
        record = json.load(open(runfile))
        write_baseline(args.baseline, args.mode, record, {k: tuple(v) for k, v in record['nodes'].items()}, args)
        return 0
    t0 = args.t0
    nodes = collect(args.build, args.stamps, t0)
    ran = {k: v for k, v in nodes.items() if v[3]}
    if not ran:
        print('== budget\n  no stamp ran in this invocation (everything was current): nothing to measure')
        return 0
    start = t0 if t0 is not None else min(v[0] for v in ran.values())
    end = max(v[1] for v in ran.values())
    wall = max(1, round((time.time() if t0 is not None else end) - start))
    summed = sum(v[2] for v in ran.values())
    base, meta = read_baseline(args.baseline, args.mode)
    cached = sorted(k for k, v in nodes.items() if not v[3] and (os.path.join(args.build, k + '.ok') in args.stamps))
    json.dump({'t0': start, 'jobs': args.jobs, 'wall': wall, 'summed': summed, 'cached': cached, 'nodes': nodes}, open(runfile, 'w'))
    done = sum(base.get(k, 0) for k in ran)
    enough = bool(base) and done >= PARTIAL * sum(v for k, v in base.items() if k != '@wall')
    print('== budget (%s, -j%d)' % (args.mode, args.jobs))
    print('  wall %d s, summed %d s over %d stamps, average parallelism %.1f%s' % (wall, summed, len(ran), summed / float(wall),
          ('; %d stamps were current and did not run' % len(cached)) if cached else ''))
    chain = critical_path(nodes)
    print('  critical path, by start and finish time (%d s of the wall):' % sum(nodes[k][2] for k in chain))
    print('\n'.join(describe(nodes, chain, start)))
    print('  longest: ' + ', '.join('%s %d s' % (k, ran[k][2]) for k in sorted(ran, key=lambda k: -ran[k][2])[:5]))
    total, best, floor = idle(nodes, start, end, args.jobs)
    print('  idle: %d s of %d with fewer than %d running; longest stretch +%d s to +%d s' % (total, end - start, floor, best[0], best[1]))
    warns = compare(nodes, wall, base, enough)
    if base and not enough:
        print('  baseline (%s): partial run, wall not compared' % meta)
    elif not base:
        print('  no baseline rows for %s in %s: make gate-baseline' % (args.mode, args.baseline))
    else:
        print('  baseline: wall %d s (%s)' % (base.get('@wall', 0), meta))
    for w in warns:
        print('  WARN budget: ' + w)
    return 3 if warns and os.environ.get('GATE_BUDGET_STRICT') == '1' else 0


if __name__ == '__main__':
    sys.exit(main())
