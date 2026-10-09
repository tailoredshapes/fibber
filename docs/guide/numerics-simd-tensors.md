---
examples: required
---

# Numerics, SIMD and tensors

`fib.tensor` provides typed dense tensors, strided views, broadcasting,
reductions, GEMM and dense-layer operations. Ordered reductions preserve order;
explicit fast variants permit reassociation. See the [tensor API](../../lib/fib/tensor/README.md).
`fib.simd` exposes lanes; native widths depend on the target. `simd/fma` needs
actual fused multiply-add, while `simd/muladd` is portable. `fib.autodiff`
provides reverse-mode tape operations and optimizers over tensors; compiler AD
is still a proposal. Measurements in the shootout describe their dated
hardware and workload, not universal performance guarantees.

```fib run
(ns main (:require [fib.tensor :as t]))
(defun main () -> i64 (do (println (t/sum (t/tensor [1.0 2.0 3.0]))) 0))
```

```text out
6.0
0
```
