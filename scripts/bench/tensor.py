#!/usr/bin/env python3
"""Verified tensor workloads and GEMM tuning against local NumPy (never installed)."""
import argparse
import contextlib
import fcntl
import io
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import shutil
import subprocess
import time

for variable in ("OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "MKL_NUM_THREADS", "BLIS_NUM_THREADS"):
    os.environ[variable] = "1"
ROOT = Path(__file__).resolve().parents[2]


def specifications(suite):
    jobs = []
    for dtype in ("f32", "f64"):
        if suite == "tune":
            for shape in ((31, 17, 29), (128, 128, 128), (257, 129, 65), (512, 512, 512)):
                for kind in range(3):
                    for cache in (32, 64):
                        for inner in (64, 128):
                            for panels in (False, True):
                                jobs.append(dict(dtype=dtype, op="config", shape=shape, config=(kind, cache, inner), panels=panels))
            continue
        for n in (31, 4096, 1048576, 8388608):
            for op in ("add", "sum", "dot", "axpby"):
                jobs.append(dict(dtype=dtype, op=op, shape=(n,)))
        for shape in ((31, 17, 29), (128, 128, 128), (257, 129, 65), (512, 512, 512)):
            for op in ("mmul", "mmul-transpose", "pack"):
                jobs.append(dict(dtype=dtype, op=op, shape=shape))
        for shape in ((129, 257), (33, 17)):
            jobs.append(dict(dtype=dtype, op="row-broadcast", shape=shape))
    for i, job in enumerate(jobs):
        job["id"] = f"j{i}"
    return jobs


def helper(dtype):
    zero, cast = ("0.0f32", "float") if dtype == "f32" else ("0.0", "double")
    eight, sixteen = ("8.0f32", "16.0f32") if dtype == "f32" else ("8.0", "16.0")
    return f"""
(defun make-{dtype} (dims: (Vec i64) flavor: i64) -> (t/Tensor {dtype})
  (let ((n (loop ((i 0) (n 1)) (if (< i (count dims)) (recur (+ i 1) (* n (nth dims i))) n)))
        (buf (cell (array n {zero}))))
    (do (dotimes (i n)
          (array-set! &buf i (if (= flavor 0) (fdiv ({cast} (- (rem i 97) 48)) {eight})
                                 (fdiv ({cast} (- (rem (* i 7) 89) 44)) {sixteen}))))
        (t/from-array dims @buf {zero}))))
"""


def workload(job, reference):
    dtype, op, shape = job["dtype"], job["op"], job["shape"]
    suffix = "f32" if dtype == "f32" else ""
    dims = lambda values: "[" + " ".join(map(str, values)) + "]"
    if len(shape) == 3:
        m, k, n = shape
        a = (k, m) if op in ("mmul-transpose", "pack") else (m, k)
        b = (k, n)
    else:
        a, b = shape, (shape[-1],) if op == "row-broadcast" else shape
    expressions = {"add": "(t/add a b)", "sum": "(t/sum-fast a)", "dot": "(t/dot-fast a b)",
                   "row-broadcast": "(t/add a b)", "mmul": "(t/mmul a b)",
                   "mmul-transpose": "(t/mmul (t/transpose a) b)"}
    if op == "axpby":
        alpha, beta = "0.5" + suffix, "-0.25" + suffix
        expression = f"(t/add (t/scale {alpha} a) (t/scale {beta} b))" if reference else f"(t/axpby {alpha} a {beta} b)"
    elif op == "pack":
        expression = "(t/contiguous (t/transpose a))" if reference else f"(g{dtype[-2:]}/pack-matrix (t/transpose a))"
    elif op == "config":
        kind, cache, inner = job["config"]
        expression = f"(g{dtype[-2:]}/multiply-mode {kind} {cache} {inner} {str(job.get('panels', False)).lower()} a b)"
    else:
        expression = expressions[op]
    checksum = "result" if op in ("sum", "dot") else "(t/sum-fast result)"
    return f"""
(defun {job['id']} () -> unit
  (let ((a (make-{dtype} {dims(a)} 0)) (b (make-{dtype} {dims(b)} 1)))
    (dotimes (i 6)
      (let ((start (sys-clock-now)) (result {expression}) (elapsed (- (sys-clock-now) start)))
        (println "{job['id']} " elapsed " " {checksum})))))
"""


def fibber(args, jobs):
    measurements = {}
    for batch in range(0, len(jobs), 16):
        chunk = jobs[batch:batch + 16]
        source, executable = args.output / f"bench-{batch}.fib", args.output / f"bench-{batch}"
        imports = " [fib.tensor.gemm-f32 :as g32] [fib.tensor.gemm-f64 :as g64]" if not args.reference else ""
        text = f"(ns main (:require [fib.tensor :as t]{imports}))\n"
        text += helper("f32") + helper("f64")
        text += "".join(workload(job, args.reference) for job in chunk)
        text += "\n(defun main () -> i64\n  (do\n" + "".join(f"    ({j['id']})\n" for j in chunk) + "    0))\n"
        source.write_text(text)
        roots = (["-I", str(args.library)] if args.library else []) + ["-I", str(ROOT / "lib")]
        subprocess.run([args.compiler, "build", str(source), *roots, "-o", str(executable)], check=True, cwd=ROOT)
        run = subprocess.run([str(executable)], check=True, capture_output=True, text=True)
        samples = {job["id"]: [] for job in chunk}
        for line in run.stdout.splitlines():
            name, ns, checksum = line.split()
            samples[name].append((int(ns), float(checksum)))
        for job in chunk:
            values = samples[job["id"]]
            if len(values) != 6 or any(not math.isfinite(v[1]) for v in values):
                raise RuntimeError(f"missing or nonfinite samples: {job}")
            measurements[job["id"]] = dict(ms=statistics.median(v[0] for v in values[1:]) / 1e6,
                                           checksums=[v[1] for v in values], samples_ns=[v[0] for v in values])
    return measurements


def operands(np, job):
    shape, op = job["shape"], job["op"]
    if len(shape) == 3:
        m, k, n = shape
        ashape = (k, m) if op in ("mmul-transpose", "pack") else (m, k)
        bshape = (k, n)
    else:
        ashape = shape
        bshape = (shape[-1],) if op == "row-broadcast" else shape
    dtype = np.float32 if job["dtype"] == "f32" else np.float64
    a = ((np.arange(math.prod(ashape)) % 97 - 48) / 8).astype(dtype).reshape(ashape)
    b = (((np.arange(math.prod(bshape)) * 7) % 89 - 44) / 16).astype(dtype).reshape(bshape)
    return a, b


def numpy_measurements(jobs, native):
    try:
        import numpy as np
    except ImportError:
        raise RuntimeError("NumPy is required for the expanded suite's independent answer checks")
    timings = {}
    for job in jobs:
        a, b = operands(np, job)
        op = job["op"]
        alpha, beta = a.dtype.type(0.5), a.dtype.type(-0.25)
        operations = {"add": lambda: a + b, "sum": lambda: a.sum(), "dot": lambda: a @ b,
                      "axpby": lambda: alpha * a + beta * b,
                      "row-broadcast": lambda: a + b, "mmul": lambda: a @ b,
                      "config": lambda: a @ b, "mmul-transpose": lambda: a.T @ b,
                      "pack": lambda: np.ascontiguousarray(a.T)}
        samples, expected = [], None
        for _ in range(6):
            start = time.perf_counter_ns()
            result = operations[op]()
            samples.append(time.perf_counter_ns() - start)
            expected = float(np.sum(result, dtype=np.float64))
            del result
        tolerance = dict(rel_tol=5e-5, abs_tol=0.02) if job["dtype"] == "f32" else dict(rel_tol=1e-10, abs_tol=1e-8)
        for checksum in native[job["id"]]["checksums"]:
            if not math.isclose(checksum, expected, **tolerance):
                raise RuntimeError(f"wrong answer: {job}, Fibber {checksum}, NumPy {expected}")
        timings[job["id"]] = dict(ms=statistics.median(samples[1:]) / 1e6, expected=expected,
                                    tolerance=tolerance, samples_ns=samples)
    configuration = io.StringIO()
    with contextlib.redirect_stdout(configuration):
        np.show_config()
    return dict(version=np.__version__, configuration=configuration.getvalue()), timings


def library_digest(root):
    digest = hashlib.sha256()
    paths = [root / "fib/tensor.fib", *sorted((root / "fib/tensor").rglob("*.fib"))]
    for path in paths:
        digest.update(str(path.relative_to(root)).encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", required=True)
    parser.add_argument("--output", type=Path, default=Path("/tmp/fibber-tensor-bench"))
    parser.add_argument("--suite", choices=("expanded", "tune"), default="expanded")
    parser.add_argument("--library", type=Path, help="alternate library root for a preserved baseline")
    parser.add_argument("--reference", action="store_true", help="use unfused arithmetic and generic copying for baseline compatibility")
    parser.add_argument("--cpu", type=int, help="pin to this CPU; Linux defaults to the first allowed CPU")
    args = parser.parse_args()
    args.compiler = shutil.which(args.compiler) or args.compiler
    if hasattr(os, "sched_getaffinity"):
        allowed = os.sched_getaffinity(0)
        cpu = min(allowed) if args.cpu is None else args.cpu
        os.sched_setaffinity(0, {cpu})
    args.output = args.output.resolve()
    args.library = args.library.resolve() if args.library else None
    args.output.mkdir(parents=True, exist_ok=True)
    jobs = specifications(args.suite)
    with open("/tmp/fibsuite.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        native = fibber(args, jobs)
        numpy, timings = numpy_measurements(jobs, native)
    print(f"NumPy {numpy['version']}; one BLAS thread; warmup + median of 5; answers checked")
    print("dtype operation        shape/config                        Fibber ms    NumPy ms")
    for job in jobs:
        label = "x".join(map(str, job["shape"])) + (f"/{job['config']}/p{int(job.get('panels', False))}" if "config" in job else "")
        print(f"{job['dtype']:5} {job['op']:16} {label:35} {native[job['id']]['ms']:10.4f} {timings[job['id']]['ms']:11.4f}")
    record = dict(numpy=numpy, platform=platform.platform(), blas_threads=1, samples=5,
                  compiler=subprocess.check_output([args.compiler, "--version"], text=True).strip(),
                  compiler_path=str(Path(args.compiler).resolve()),
                  compiler_sha256=hashlib.sha256(Path(args.compiler).read_bytes()).hexdigest(),
                  tensor_library_sha256=library_digest(args.library or ROOT / "lib"),
                  library_path=str(args.library or ROOT / "lib"), suite=args.suite,
                  target_cpu=os.environ.get("FIB_TARGET_CPU", "host"), reference=args.reference,
                  affinity=sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
                  jobs=[dict(**job, fibber=native[job["id"]], numpy=timings[job["id"]]) for job in jobs])
    (args.output / "results.json").write_text(json.dumps(record, indent=2) + "\n")


if __name__ == "__main__":
    main()
