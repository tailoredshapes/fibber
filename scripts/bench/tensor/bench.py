#!/usr/bin/env python3
"""NumPy side of scripts/bench/tensor/bench.fib: same kernels, same inputs, same output format.

usage: bench.py KERNEL N RUNS      prints "t <ns>" per run (run 0 is the warmup) and "cs <checksum>"
Inputs are built outside the timed region; so is the checksum (sum of squares of every element,
accumulated in f64) and the destruction of the result."""
import sys
import time
import numpy as np


def gen(n, flavor, dt):
    i = np.arange(n, dtype=np.int64)
    if flavor == 0:
        v = ((i % 97) - 48) / 8.0
    elif flavor == 1:
        v = (((i * 7) % 89) - 44) / 16.0
    else:
        v = (((i * 5) % 83) - 41) / 32.0
    return v.astype(dt)


def make(dims, flavor, dt):
    n = 1
    for d in dims:
        n *= d
    return gen(n, flavor, dt).reshape(dims)


def cs(x):
    v = np.asarray(x).astype(np.float64).ravel()
    return float(np.dot(v, v))


def softmax(x):
    m = x.max(axis=-1, keepdims=True)
    e = np.exp(x - m)
    return e / e.sum(axis=-1, keepdims=True)


def layernorm(x, g, b):
    mu = x.mean(axis=-1, keepdims=True)
    xc = x - mu
    var = (xc * xc).mean(axis=-1, keepdims=True)
    return xc * (1.0 / np.sqrt(var + 0.00001)) * g + b


def relu(x):
    return np.where(x > 0, x, np.float32(0))


def setup(k, n):
    f64, f32 = np.float64, np.float32
    if k == "matmul64":
        a, b = make([n, n], 0, f64), make([n, n], 1, f64)
        return lambda: a @ b, cs
    if k == "matmul32":
        a, b = make([n, n], 0, f32), make([n, n], 1, f32)
        return lambda: a @ b, cs
    if k == "fma64":
        a, b, c = make([n], 0, f64), make([n], 1, f64), make([n], 2, f64)
        return lambda: a * b + c, cs
    if k == "fma32":
        a, b, c = make([n], 0, f32), make([n], 1, f32), make([n], 2, f32)
        return lambda: a * b + c, cs
    if k in ("sum", "sumfast", "mean", "max"):
        a = make([n], 1, f64)
        fn = {"sum": a.sum, "sumfast": a.sum, "mean": a.mean, "max": a.max}[k]
        return (lambda: fn()), float
    if k in ("sum0", "sum1", "max0", "max1"):
        a = make([n, n], 1, f64)
        axis = int(k[-1])
        if k.startswith("sum"):
            return (lambda: a.sum(axis=axis)), cs
        return (lambda: a.max(axis=axis)), cs
    if k == "bcast":
        a, row = make([n, n], 1, f64), make([n], 2, f64)
        return lambda: a + row, cs
    if k in ("attn", "attnfused"):
        # one head: scores = q k^T / sqrt(64), softmax over rows, times v; n is the sequence length, head dim 64, f32
        q, kk, v = make([n, 64], 1, f32), make([n, 64], 2, f32), make([n, 64], 0, f32)
        return lambda: softmax(q @ kk.T / np.float32(8.0)) @ v, cs
    if k in ("softmax", "softmaxfused"):
        a = make([n, n], 2, f64)
        return lambda: softmax(a), cs
    if k in ("layernorm", "layernormfused"):
        a, g, b = make([n, 1024], 1, f64), make([1024], 2, f64), make([1024], 0, f64)
        return lambda: layernorm(a, g, b), cs
    if k in ("mlp", "mlpfused"):
        x, w1, b1 = make([n, 784], 1, f32), make([784, 512], 2, f32), make([512], 0, f32)
        w2, b2 = make([512, 10], 2, f32), make([10], 1, f32)
        return lambda: relu(x @ w1 + b1) @ w2 + b2, cs
    raise SystemExit("unknown kernel " + k)


def main():
    k, n, runs = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    fn, check = setup(k, n)
    for r in range(runs):
        s = time.perf_counter_ns()
        v = fn()
        e = time.perf_counter_ns() - s
        print("t", e)
        if r == 0:
            print("cs", repr(check(v)))
        del v


main()
