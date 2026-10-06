#!/usr/bin/env python3
"""The Python side of the end-to-end training benchmark (docs/shootout/autodiff.md): the same MLP 784-256-10, the same generated data
and initial weights as scripts/bench/autodiff/mlp.fib, in NumPy (hand-written gradients), PyTorch CPU (autograd) and JAX (jit of value_and_grad).

  OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 MKL_NUM_THREADS=1 python3 mlp.py FRAMEWORK OPT STEPS
  FRAMEWORK numpy | torch | jax      OPT sgd (lr 0.1) | adam (lr 0.001, 0.9, 0.999, 1e-8)
Prints `loss STEP VALUE` per step, `step_ms MEDIAN MEAN MIN` over the steps after 10 warm-up steps, and `checksum` (sum |w1|, f64), like the
fibber program. Nothing is downloaded: the data are integer hashes ((7919 n + 104729 j + 1299709 salt + 12345) * 2654435761) mod 2^32.
"""
import os, sys, time, statistics
import numpy as np

BATCH, SAMPLES, INPUTS, HIDDEN, CLASSES = 128, 4096, 784, 256, 10


def hash_u(n, j, salt):
    h = ((7919 * n + 104729 * j + 1299709 * salt + 12345) * 2654435761) % 4294967296
    return ((h >> 7) % 1000000).astype(np.float64) / 1000000.0


def make_x():
    n = np.arange(SAMPLES, dtype=np.int64)[:, None]
    j = np.arange(INPUTS, dtype=np.int64)[None, :]
    c = n % 10
    sig = ((j * (c + 3) + c * 17) % 11).astype(np.float64) / 10.0 - 0.5
    return (0.5 * (hash_u(n, j, 1) - 0.5) + 0.8 * sig).astype(np.float32)


def make_w(rows, cols, salt):
    a = 2.0 * np.sqrt(6.0 / rows)
    i = np.arange(rows, dtype=np.int64)[:, None]
    j = np.arange(cols, dtype=np.int64)[None, :]
    return ((hash_u(i, j, salt) - 0.5) * a).astype(np.float32)


def labels_of(b):
    return (np.arange(BATCH, dtype=np.int64) + b * BATCH) % 10


def init():
    return [make_w(INPUTS, HIDDEN, 2), np.zeros(HIDDEN, np.float32), make_w(HIDDEN, CLASSES, 3), np.zeros(CLASSES, np.float32)]


class Adam:
    def __init__(self, ps):
        self.m = [np.zeros_like(p) for p in ps]
        self.v = [np.zeros_like(p) for p in ps]
        self.n = 0

    def step(self, ps, gs, lr=0.001, b1=0.9, b2=0.999, eps=1e-8):
        self.n += 1
        c1, c2 = 1 - b1 ** self.n, 1 - b2 ** self.n
        for i, (p, g) in enumerate(zip(ps, gs)):
            self.m[i] = b1 * self.m[i] + (1 - b1) * g
            self.v[i] = b2 * self.v[i] + (1 - b2) * g * g
            p -= (lr / c1) * self.m[i] / (np.sqrt(self.v[i]) / np.sqrt(c2) + eps)


def run_numpy(opt, steps, X):
    ps = init()
    ad = Adam(ps) if opt == "adam" else None
    out = []
    for s in range(steps):
        b = s % 32
        xb = X[b * BATCH:(b + 1) * BATCH]
        lab = labels_of(b)
        t0 = time.perf_counter_ns()
        w1, b1, w2, b2 = ps
        h = np.maximum(xb @ w1 + b1, 0)
        o = h @ w2 + b2
        e = np.exp(o - o.max(axis=1, keepdims=True))
        p = e / e.sum(axis=1, keepdims=True)
        loss = -np.log(p[np.arange(BATCH), lab]).mean()
        dz2 = p.copy()
        dz2[np.arange(BATCH), lab] -= 1.0
        dz2 *= np.float32(1.0 / BATCH)
        gs = [None, None, h.T @ dz2, dz2.sum(axis=0)]
        dz1 = (dz2 @ w2.T) * (h > 0)
        gs[0] = xb.T @ dz1
        gs[1] = dz1.sum(axis=0)
        if ad:
            ad.step(ps, gs)
        else:
            for p_, g in zip(ps, gs):
                p_ -= np.float32(0.1) * g
        out.append((float(loss), time.perf_counter_ns() - t0))
    return out, float(np.abs(ps[0].astype(np.float64)).sum())


def run_torch(opt, steps, X):
    import torch
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    ps = [torch.from_numpy(p.copy()).requires_grad_() for p in init()]
    o = torch.optim.Adam(ps, lr=0.001) if opt == "adam" else torch.optim.SGD(ps, lr=0.1)
    Xt = torch.from_numpy(X)
    out = []
    for s in range(steps):
        b = s % 32
        xb = Xt[b * BATCH:(b + 1) * BATCH]
        lab = torch.from_numpy(labels_of(b))
        t0 = time.perf_counter_ns()
        o.zero_grad()
        h = torch.relu(xb @ ps[0] + ps[1])
        loss = torch.nn.functional.cross_entropy(h @ ps[2] + ps[3], lab)
        loss.backward()
        o.step()
        out.append((float(loss.item()), time.perf_counter_ns() - t0))
    return out, float(ps[0].detach().double().abs().sum())


def run_jax(opt, steps, X):
    import jax, jax.numpy as jnp
    ps = [jnp.asarray(p) for p in init()]
    Xj = jnp.asarray(X)

    def loss_fn(ps, xb, lab):
        h = jax.nn.relu(xb @ ps[0] + ps[1])
        lo = h @ ps[2] + ps[3]
        lp = jax.nn.log_softmax(lo)
        return -jnp.mean(lp[jnp.arange(BATCH), lab])

    @jax.jit
    def step_sgd(ps, xb, lab):
        l, g = jax.value_and_grad(loss_fn)(ps, xb, lab)
        return l, [p - 0.1 * q for p, q in zip(ps, g)]

    @jax.jit
    def step_adam(ps, m, v, n, xb, lab):
        l, g = jax.value_and_grad(loss_fn)(ps, xb, lab)
        n = n + 1.0
        c1, c2 = 1 - 0.9 ** n, 1 - 0.999 ** n
        m = [0.9 * a + 0.1 * q for a, q in zip(m, g)]
        v = [0.999 * a + 0.001 * q * q for a, q in zip(v, g)]
        ps = [p - (0.001 / c1) * a / (jnp.sqrt(c) / jnp.sqrt(c2) + 1e-8) for p, a, c in zip(ps, m, v)]
        return l, ps, m, v, n

    m = [jnp.zeros_like(p) for p in ps]
    v = [jnp.zeros_like(p) for p in ps]
    n = jnp.float32(0.0)
    # compile outside the timing (jit): one call on the first batch with throwaway copies, as every framework's warm-up
    xb0, lab0 = Xj[0:BATCH], jnp.asarray(labels_of(0))
    if opt == "adam":
        jax.block_until_ready(step_adam(ps, m, v, n, xb0, lab0))
    else:
        jax.block_until_ready(step_sgd(ps, xb0, lab0))
    out = []
    for s in range(steps):
        b = s % 32
        xb = Xj[b * BATCH:(b + 1) * BATCH]
        lab = jnp.asarray(labels_of(b))
        t0 = time.perf_counter_ns()
        if opt == "adam":
            l, ps, m, v, n = step_adam(ps, m, v, n, xb, lab)
        else:
            l, ps = step_sgd(ps, xb, lab)
        jax.block_until_ready(ps)
        out.append((float(l), time.perf_counter_ns() - t0))
    return out, float(np.abs(np.asarray(ps[0], dtype=np.float64)).sum())


def main():
    fw, opt, steps = sys.argv[1], sys.argv[2], int(sys.argv[3])
    X = make_x()
    out, check = {"numpy": run_numpy, "torch": run_torch, "jax": run_jax}[fw](opt, steps, X)
    for s, (l, _) in enumerate(out):
        print("loss", s, "%.8g" % l)
    ts = [t for _, t in out][10:] if steps > 10 else [t for _, t in out]
    if ts:
        print("step_ms", statistics.median(ts) / 1e6, statistics.mean(ts) / 1e6, min(ts) / 1e6)
    else:
        print("step_ms 0 0 0")
    print("checksum", "%.8g" % check)


if __name__ == "__main__":
    main()
