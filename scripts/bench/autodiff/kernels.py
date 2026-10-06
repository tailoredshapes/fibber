#!/usr/bin/env python3
"""NumPy (OpenBLAS) times for the same kernels as scripts/bench/autodiff/kernels.fib: best of 200, ms, single thread."""
import time
import numpy as np
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from mlp import make_w, hash_u, BATCH, INPUTS, HIDDEN, CLASSES, make_x


def best(name, f, runs=200):
    b = 1 << 62
    for _ in range(runs):
        s = time.perf_counter_ns()
        f()
        b = min(b, time.perf_counter_ns() - s)
    print(name, "best_ms", b / 1e6)


x = make_x()[:BATCH].copy()
w1 = make_w(INPUTS, HIDDEN, 2)
b1 = np.zeros(HIDDEN, np.float32)
w2 = make_w(HIDDEN, CLASSES, 3)
b2 = np.zeros(CLASSES, np.float32)
z1 = x @ w1 + b1
h = np.maximum(z1, 0)
dz1 = z1.copy()
dz2 = make_w(BATCH, CLASSES, 4)
best("gemm_x_w1_128x784x256", lambda: x @ w1)
best("gemm_x_w1_plus_bias_relu", lambda: np.maximum(x @ w1 + b1, 0))
best("gemm_xT_dz1_784x128x256", lambda: x.T @ dz1)
best("gemm_dz1_w1T_128x256x784", lambda: dz1 @ w1.T)
best("relu_mask_128x256", lambda: dz1 * (h > 0))
best("bias_grad_sum_axis0_128x256", lambda: dz1.sum(axis=0))
best("dense2_128x256x10", lambda: h @ w2 + b2)
best("dw2_256x128x10", lambda: h.T @ dz2)
best("sgd_update_4_tensors", lambda: [w1 - np.float32(0.1) * w1, b1 - np.float32(0.1) * b1, w2 - np.float32(0.1) * w2, b2 - np.float32(0.1) * b2])
