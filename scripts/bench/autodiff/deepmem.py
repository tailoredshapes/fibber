#!/usr/bin/env python3
"""Peak RSS of one training step of the 12 x 1024 MLP (scripts/bench/autodiff/deep.fib) in fibber and PyTorch (docs/shootout/autodiff.md).
  python3 deepmem.py --fibc F --python VENV/bin/python [--cpu 3]
Each program runs at 0 steps (parameters and data) and at 1 step; wait4's ru_maxrss of each, and the difference, are printed; 3 runs, the median.
"""
import argparse, os, resource, statistics, subprocess, sys

TORCH = r'''
import sys, torch
torch.set_num_threads(1)
steps = int(sys.argv[1])
L, W, R = 12, 1024, 256
def hash_u(i, j, salt):
    import numpy as np
    i = np.arange(i, dtype=np.int64)[:, None]; j = np.arange(j, dtype=np.int64)[None, :]
    h = ((7919 * i + 104729 * j + 1299709 * salt + 12345) * 2654435761) % 4294967296
    return ((h >> 7) % 1000000).astype(np.float64) / 1000000.0
import numpy as np
ps = []
for l in range(L):
    w = torch.from_numpy(((hash_u(W, W, l + 2) - 0.5) * 2.0 * np.sqrt(6.0 / 1024.0)).astype(np.float32)).requires_grad_()
    ps += [w, torch.zeros(W, requires_grad=True)]
x = torch.from_numpy(((hash_u(R, W, 1) - 0.5) * 2.0).astype(np.float32))
if steps > 0:
    h = x
    for l in range(L):
        h = torch.relu(h @ ps[2 * l] + ps[2 * l + 1])
    loss = (h * h).mean()
    loss.backward()
    print("loss", loss.item())
'''


def run(cmd, cpu, limit):
    def pre():
        os.sched_setaffinity(0, {cpu})
        if limit:
            resource.setrlimit(resource.RLIMIT_AS, (16000000 * 1024, 16000000 * 1024))
    env = dict(os.environ, OMP_NUM_THREADS="1", OPENBLAS_NUM_THREADS="1", MKL_NUM_THREADS="1")
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env, preexec_fn=pre, text=True)
    out = p.stdout.read()
    _, st, ru = os.wait4(p.pid, 0)
    if st != 0:
        sys.exit("FAILED %s" % cmd)
    return ru.ru_maxrss / 1024.0, out.strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fibc", required=True)
    ap.add_argument("--python", required=True)
    ap.add_argument("--cpu", type=int, default=3)
    a = ap.parse_args()
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.abspath(os.path.join(here, "..", "..", ".."))
    out = os.path.expanduser("~/.cache/fibber-scratch/ad/run")
    os.makedirs(out, exist_ok=True)
    exe = os.path.join(out, "deep")
    subprocess.run([a.fibc, "build", os.path.join(here, "deep.fib"), "-I", os.path.join(root, "lib"), "-o", exe], check=True,
                   env=dict(os.environ, FIB_LIB=os.path.join(root, "lib")))
    tp = os.path.join(out, "deep_torch.py")
    open(tp, "w").write(TORCH)
    print("| implementation | peak RSS MB, 0 steps | peak RSS MB, 1 step | the step needs (MB) | output |")
    print("|---|---|---|---|---|")
    for name, mk, limit in (("fibber tape", lambda n: [exe, str(n)], True), ("PyTorch autograd", lambda n: [a.python, tp, str(n)], False)):
        r0 = [run(mk(0), a.cpu, limit)[0] for _ in range(3)]
        r1 = [run(mk(1), a.cpu, limit) for _ in range(3)]
        m0, m1 = statistics.median(r0), statistics.median(x[0] for x in r1)
        print("| %s | %.0f | %.0f | %.0f | %s |" % (name, m0, m1, m1 - m0, r1[0][1]))


if __name__ == "__main__":
    main()
