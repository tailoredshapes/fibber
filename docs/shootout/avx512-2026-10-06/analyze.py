#!/usr/bin/env python3
"""analyze.py RESULTS_DIR_OF_MACHINE: markdown tables of one machine's results (tensor, shootout, autodiff, checks)."""
import csv, os, statistics, sys, re

R = sys.argv[1]
if os.path.isdir(os.path.join(R, "results")):
    R = os.path.join(R, "results")

def tsv(name):
    p = os.path.join(R, name)
    if not os.path.exists(p):
        return []
    with open(p) as f:
        return list(csv.DictReader(f, delimiter="\t"))

def fl(x):
    try:
        return float(x)
    except Exception:
        return None

print("### Cases on this machine (each built for 256 and 512 bits; 0 is a pass)")
rows = [l.rstrip("\n").split("\t") for l in open(os.path.join(R, "cases.tsv"))] if os.path.exists(os.path.join(R, "cases.tsv")) else []
ok = [r for r in rows if len(r) > 1 and r[1].strip() == "0"]
print("%d of %d case programs print 0; failures: %s" % (len(ok), len(rows), [r for r in rows if not (len(r) > 1 and r[1].strip() == "0")]))
print()
print("### Checksums of the benchmark kernels at 256 and 512 bits (small sizes)")
for r in tsv("checksums.tsv") if False else []:
    pass
cs = [l.rstrip("\n").split("\t") for l in open(os.path.join(R, "checksums.tsv"))] if os.path.exists(os.path.join(R, "checksums.tsv")) else []
print("| kernel | 256 | 512 | |\n|---|---|---|---|")
for r in cs:
    print("| %s | %s | %s | %s |" % tuple(r[:4]))
print()

print("### Tensor table (median of 5, one thread, ms)")
t256, t512 = tsv("tensor-256.tsv"), tsv("tensor-512.tsv")
d512 = {r["label"]: r for r in t512}
kc = {}
for k in (64, 256):
    kc[k] = {r["label"]: r for r in tsv("tensor-512-kc%d.tsv" % k)}
print("| kernel | fib 256 | fib 512 | 512/256 time | NumPy OpenBLAS | NumPy ref-ish | 512 vs OpenBLAS | GFLOPS 256 | GFLOPS 512 | kc64 | kc256 | cs ok |")
print("|---|---|---|---|---|---|---|---|---|---|---|---|")
for r in t256:
    l = r["label"]; a = fl(r["fib_ms"]); r5 = d512.get(l); b = fl(r5["fib_ms"]) if r5 else None
    npy = fl(r["numpy_ms"]); ob = fl(r["numpy_openblas_ms"])
    m = re.match(r"matmul (f32|f64) (\d+)", l)
    g = lambda ms: "%.1f" % (2.0 * int(m.group(2)) ** 3 / (ms * 1e6)) if (m and ms) else "-"
    kcs = ["%.2f" % fl(kc[k][l]["fib_ms"]) if l in kc[k] and fl(kc[k][l]["fib_ms"]) else "-" for k in (64, 256)]
    print("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s/%s |" % (
        l, "%.2f" % a if a else "-", "%.2f" % b if b else "-", "%.2f" % (b / a) if a and b else "-",
        "%.2f" % ob if ob else "-", "%.2f" % npy if npy else "-", "%.2f" % (b / ob) if b and ob else "-",
        g(a), g(b), kcs[0], kcs[1], r["checksum_ok"], r5["checksum_ok"] if r5 else "-"))
print()

print("### Shootout (seconds, median of 5, one thread)")
so = tsv("shootout.tsv")
by = {}
for r in so:
    by.setdefault(r["bench"], {})[r["variant"]] = r
print("| bench | simd 256 | simd 512 | 256/512 speedup | scalar (LLVM default) | scalar -prefer-256-bit | speedup | status |")
print("|---|---|---|---|---|---|---|---|")
for b, d in by.items():
    def v(k):
        x = d.get(k)
        return fl(x["elapsed_s"]) if x and x["status"] == "ok" else None
    s256, s512 = v("simd-256"), v("simd-512")
    sc, sp = v("scalar"), v("scalar-p512")
    if v("simd") is not None:
        s256 = v("simd")
    st = ",".join("%s:%s" % (k, x["status"]) for k, x in d.items() if x["status"] != "ok")
    f = lambda x: "%.2f" % x if x else "-"
    print("| %s | %s | %s | %s | %s | %s | %s | %s |" % (b, f(s256), f(s512), "%.2fx" % (s256 / s512) if s256 and s512 else "-",
                                                         f(sc), f(sp), "%.2fx" % (sc / sp) if sc and sp else "-", st or "ok"))
print()

print("### Autodiff MLP step (ms per step, median over runs of the median step)")
ad = os.path.join(R, "autodiff")
def steps(prefix):
    out = []
    if not os.path.isdir(ad):
        return out
    for fn in sorted(os.listdir(ad)):
        if fn.startswith(prefix):
            for l in open(os.path.join(ad, fn)):
                if l.startswith("step_ms"):
                    out.append(float(l.split()[1]))
    return out
print("| optimiser | fib 256 | fib 512 | 512/256 | NumPy OpenBLAS | fib 512 / NumPy |")
print("|---|---|---|---|---|---|")
for opt in ("sgd", "adam"):
    a, b, n = steps("tape-%s-256-" % opt), steps("tape-%s-512-" % opt), steps("numpy-%s-" % opt)
    h = steps("hand-%s-256-" % opt), steps("hand-%s-512-" % opt)
    med = lambda x: statistics.median(x) if x else None
    f = lambda x: "%.3f" % x if x else "-"
    print("| %s (tape) | %s | %s | %s | %s | %s |" % (opt, f(med(a)), f(med(b)), "%.2f" % (med(b) / med(a)) if a and b else "-", f(med(n)), "%.2f" % (med(b) / med(n)) if b and n else "-"))
    print("| %s (hand) | %s | %s | %s | | |" % (opt, f(med(h[0])), f(med(h[1])), "%.2f" % (med(h[1]) / med(h[0])) if h[0] and h[1] else "-"))
