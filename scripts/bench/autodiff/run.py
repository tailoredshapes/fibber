#!/usr/bin/env python3
"""Runs the end-to-end training benchmark (docs/shootout/autodiff.md) and prints the table.

  python3 scripts/bench/autodiff/run.py --fibc F --python VENV/bin/python [--cpu 3] [--steps 300] [--runs 5] [--out DIR]

Each configuration runs `--runs` times (default 5), one process per run, pinned to one CPU, one thread (OMP/OPENBLAS/MKL threads = 1), under
/tmp/fibsuite.lock; fibber under `ulimit -v 16000000`. Per run the program prints the per-step times; the table has the median over runs of the
median step time, the peak RSS of the process (wait4's ru_maxrss) with the RSS of the same program at 0 steps (data and parameters only), the
largest relative difference of the loss curve from the NumPy curve, and for fibber the allocations of one step from FIB_TRACE.
"""
import argparse, fcntl, os, resource, statistics, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))


def run(cmd, env, cpu, limit):
    def pre():
        os.sched_setaffinity(0, {cpu})
        if limit:
            resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env, preexec_fn=pre, text=True)
    out = p.stdout.read()
    _, status, ru = os.wait4(p.pid, 0)
    if status != 0:
        sys.exit("FAILED: %s (status %d)" % (cmd, status))
    return out, ru.ru_maxrss


def parse(out):
    losses, step = [], None
    for line in out.splitlines():
        w = line.split()
        if w and w[0] == "loss":
            losses.append(float(w[2]))
        elif w and w[0] == "step_ms":
            step = float(w[1])
    return losses, step


def trace_allocs(cmd, env, cpu):
    e = dict(env, FIB_TRACE="1")
    def pre():
        os.sched_setaffinity(0, {cpu})
    counts = []
    for n in ("1", "3"):
        c = cmd[:-1] + [n]
        p = subprocess.run(c, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, env=e, preexec_fn=pre, text=True)
        counts.append(sum(1 for l in p.stderr.splitlines() if l.startswith("A ")))
    return (counts[1] - counts[0]) // 2


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fibc", required=True)
    ap.add_argument("--python", required=True)
    ap.add_argument("--cpu", type=int, default=sorted(os.sched_getaffinity(0))[0])
    ap.add_argument("--steps", type=int, default=300)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--out", default=os.path.expanduser("~/.cache/fibber-scratch/ad/run"))
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    lock = open("/tmp/fibsuite.lock", "w")
    fcntl.flock(lock, fcntl.LOCK_EX)
    env = dict(os.environ, OMP_NUM_THREADS="1", OPENBLAS_NUM_THREADS="1", MKL_NUM_THREADS="1", BLIS_NUM_THREADS="1")
    root = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    exe = os.path.join(a.out, "mlp")
    subprocess.run([a.fibc, "build", os.path.join(HERE, "mlp.fib"), "-I", os.path.join(root, "lib"), "-o", exe],
                   env=dict(env, FIB_LIB=os.path.join(root, "lib")), check=True)
    rows = []
    for opt in ("sgd", "adam"):
        for name, mk, limit in (
            ("fibber tape (fib.autodiff)", lambda n: [exe, "tape", opt, str(n)], True),
            ("fibber hand-written backward", lambda n: [exe, "hand", opt, str(n)], True),
            ("NumPy hand-written backward", lambda n: [a.python, os.path.join(HERE, "mlp.py"), "numpy", opt, str(n)], False),
            ("PyTorch autograd", lambda n: [a.python, os.path.join(HERE, "mlp.py"), "torch", opt, str(n)], False),
            ("JAX jit(value_and_grad)", lambda n: [a.python, os.path.join(HERE, "mlp.py"), "jax", opt, str(n)], False),
        ):
            steps, rss, curve = [], [], None
            for _ in range(a.runs):
                out, kb = run(mk(a.steps), env, a.cpu, limit)
                losses, st = parse(out)
                steps.append(st)
                rss.append(kb)
                curve = losses
            _, kb0 = run(mk(0), env, a.cpu, limit)
            allocs = trace_allocs(mk(1), env, a.cpu) if limit else None
            rows.append((opt, name, statistics.median(steps), min(steps), max(steps), max(rss) / 1024, kb0 / 1024, curve, allocs))
            with open(os.path.join(a.out, "curve-%s-%s.txt" % (opt, name.split()[0] + name.split()[1])), "w") as f:
                f.write("\n".join("%.8g" % l for l in curve))
            print("done", opt, name, rows[-1][2], flush=True)
    ref = {o: next(r[7] for r in rows if r[0] == o and r[1].startswith("NumPy")) for o in ("sgd", "adam")}
    print("\n| optimiser | implementation | ms/step (median of %d) | min..max over runs | peak RSS MB | RSS MB at 0 steps | max rel. loss diff vs NumPy | allocations/step |" % a.runs)
    print("|---|---|---|---|---|---|---|---|")
    for opt, name, med, lo, hi, rss, rss0, curve, allocs in rows:
        d = max(abs(x - y) / max(abs(y), 1e-12) for x, y in zip(curve, ref[opt]))
        print("| %s | %s | %.3f | %.3f..%.3f | %.0f | %.0f | %.1e | %s |" % (opt, name, med, lo, hi, rss, rss0, d, allocs if allocs is not None else "not measured"))


if __name__ == "__main__":
    main()
