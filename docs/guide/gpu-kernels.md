---
examples: required
---

# GPU kernels

GPU kernels are an experimental opt-in via `defkernel`, with a restricted
kernel language. `--emit ptx` targets CUDA and `--emit wgsl` targets WebGPU.
Host programs need a device driver from a separate repository; `fib.gpu.sim`
checks memory/transfer protocols on the CPU and does not execute kernels.
Atomics, shuffles and pinned-transfer contracts are documented in sections
12–14 of [the GPU design](../design/gpu.md). The CPU tensor API is independent
of a driver and is useful as a reference for checking GPU results:

```fib run
(ns main (:require [fib.tensor :as t]))
(defun main () -> i64 (do (println (t/sum (t/tensor [1.0 2.0 3.0]))) 0))
```

```text out
6.0
0
```
