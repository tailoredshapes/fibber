#!/usr/bin/env python3
"""Drive bench.fib (fib.tensor) against bench.py (NumPy): same inputs, checksums compared, median of 5.

  python3 scripts/bench/tensor/run.py --fibc F [--only PREFIX..] [--small] [--tsv out.tsv] [--openblas DIR]

Every process runs under `flock /tmp/fibsuite.lock` and `ulimit -v 16000000`, with OMP/OPENBLAS/MKL/BLIS
threads forced to 1 (fib.tensor has no tasks, so no all-threads table is made for it). A process builds its
inputs, runs the kernel RUNS=6 times (run 0 is the warmup, dropped) and prints every time; the table reports
the median of the 5 timed runs. Checksums are sums of squares of the result accumulated in f64 (scalar
kernels: the value). Tolerance: relative 1e-9 for f64 results and 1e-4 for f32 results (the f32 matmul sums in
a different order from BLAS); a mismatch is printed as FAIL and the row is not trusted.
--small runs reduced sizes once, for checking the two programs agree. --openblas DIR (a directory holding a
libblas.so.3 that is OpenBLAS, put first on LD_LIBRARY_PATH) adds a third column: the same NumPy on OpenBLAS."""
import argparse
import os
import statistics
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
# (kernel, n, small n, label, rtol)
SUITE = [
    ("matmul64", 512, 128, "matmul f64 512", 1e-9),
    ("matmul64", 1024, 128, "matmul f64 1024", 1e-9),
    ("matmul64", 2048, 128, "matmul f64 2048", 1e-9),
    ("matmul32", 512, 128, "matmul f32 512", 1e-4),
    ("matmul32", 1024, 128, "matmul f32 1024", 1e-4),
    ("matmul32", 2048, 128, "matmul f32 2048", 1e-4),
    ("fma64", 10_000_000, 100_000, "a*b+c f64 1e7", 1e-9),
    ("fma64", 100_000_000, 100_000, "a*b+c f64 1e8", 1e-9),
    ("fma32", 10_000_000, 100_000, "a*b+c f32 1e7", 1e-4),
    ("sum", 100_000_000, 100_000, "sum f64 1e8 (ordered t/sum)", 1e-9),
    ("sumfast", 100_000_000, 100_000, "sum f64 1e8 (t/sum-fast)", 1e-9),
    ("mean", 100_000_000, 100_000, "mean f64 1e8", 1e-9),
    ("max", 100_000_000, 100_000, "max f64 1e8", 1e-9),
    ("expvec", 10_000_000, 100_000, "exp f64 1e7 (t/exp)", 1e-9),
    ("logvec", 10_000_000, 100_000, "log f64 1e7 (t/log)", 1e-9),
    ("tanhvec", 10_000_000, 100_000, "tanh f64 1e7 (t/tanh)", 1e-9),
    ("expvec32", 10_000_000, 100_000, "exp f32 1e7 (t/exp)", 1e-4),
    ("sum0", 4096, 256, "sum axis 0, 4096x4096", 1e-9),
    ("sum1", 4096, 256, "sum axis 1, 4096x4096", 1e-9),
    ("max0", 4096, 256, "max axis 0, 4096x4096", 1e-9),
    ("max1", 4096, 256, "max axis 1, 4096x4096", 1e-9),
    ("bcast", 4096, 256, "broadcast add 4096x4096 + row", 1e-9),
    ("softmax", 4096, 256, "softmax rows 4096x4096 f64", 1e-9),
    ("layernorm", 4096, 256, "layernorm 4096x1024 f64", 1e-9),
    ("mlp", 256, 32, "MLP f32 b256 784-512-10", 1e-4),
    ("mlpfused", 256, 32, "MLP f32 b256 784-512-10 (t/dense)", 1e-4),
    ("softmaxfused", 4096, 256, "softmax rows 4096x4096 f64 (t/softmax)", 1e-9),
    ("layernormfused", 4096, 256, "layernorm 4096x1024 f64 (t/layernorm)", 1e-9),
    ("attn", 1024, 128, "attention f32 n=1024 d=64 (composed)", 1e-4),
    ("attnfused", 1024, 128, "attention f32 n=1024 d=64 (t/softmax)", 1e-4),
]


def run(cmd, env_extra, runs):
    env = dict(os.environ)
    for v in ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS", "BLIS_NUM_THREADS"):
        env[v] = "1"
    env.update(env_extra)
    shell = "ulimit -v 16000000; exec flock /tmp/fibsuite.lock " + " ".join(cmd)
    p = subprocess.run(["bash", "-c", shell], env=env, capture_output=True, text=True)
    if p.returncode != 0:
        return None, None, p.stderr.strip()[-300:] or "exit %d" % p.returncode
    ts = [int(l.split()[1]) for l in p.stdout.splitlines() if l.startswith("t ")]
    cs = [float(l.split()[1]) for l in p.stdout.splitlines() if l.startswith("cs ")]
    if len(ts) != runs or len(cs) != 1:
        return None, None, "bad output: " + p.stdout[-200:]
    return statistics.median(ts[1:]) / 1e6, cs[0], ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fibc", required=True, help="stage 2 compiler; FIB_LIB is taken from the environment")
    ap.add_argument("--only", nargs="*", default=[])
    ap.add_argument("--small", action="store_true")
    ap.add_argument("--tsv")
    ap.add_argument("--openblas")
    ap.add_argument("--bin", default=os.path.expanduser("~/.cache/fibber-scratch/tp1/bench"))
    a = ap.parse_args()
    env = dict(os.environ)
    root = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    env.setdefault("FIB_LIB", os.path.join(root, "lib"))
    subprocess.run([a.fibc, "build", os.path.join(HERE, "bench.fib"), "-I", HERE, "-I", os.path.join(root, "lib"), "-o", a.bin],
                   env=env, check=True)
    runs = 2 if a.small else 6
    rows, bad = [], 0
    for k, n, ns, label, rtol in SUITE:
        if a.only and not any(label.startswith(o) or k.startswith(o) for o in a.only):
            continue
        n = ns if a.small else n
        args = [k, str(n), str(runs)]
        fm, fc, fe = run([a.bin] + args, {}, runs)
        nm, nc, ne = run(["python3", os.path.join(HERE, "bench.py")] + args, {}, runs)
        ob = None
        if a.openblas and k.startswith("matmul") or (a.openblas and k in ("mlp", "mlpfused", "attn", "attnfused")):
            ob, oc, oe = run(["python3", os.path.join(HERE, "bench.py")] + args, {"LD_LIBRARY_PATH": a.openblas}, runs)
        ok = fm is not None and nm is not None and abs(fc - nc) <= rtol * max(abs(fc), abs(nc)) + 1e-300
        if not ok:
            bad += 1
        rows.append((label, fm, nm, ob, ok, fc, nc, fe or ne))
        print("%-34s fib %10s ms  numpy %10s ms  openblas %10s  ratio %6s  %s  cs fib=%s numpy=%s %s" % (
            label, "%.3f" % fm if fm else "-", "%.3f" % nm if nm else "-", "%.3f" % ob if ob else "-",
            "%.2f" % (fm / nm) if fm and nm else "-", "ok" if ok else "FAIL", fc, nc, fe or ne), flush=True)
    if a.tsv:
        with open(a.tsv, "w") as f:
            f.write("label\tfib_ms\tnumpy_ms\tnumpy_openblas_ms\tchecksum_ok\tfib_cs\tnumpy_cs\n")
            for r in rows:
                f.write("\t".join(str(x) for x in (r[0], r[1], r[2], r[3], r[4], r[5], r[6])) + "\n")
    sys.exit(1 if bad else 0)


main()
