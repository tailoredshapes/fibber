# 0004. The library chooses FMA by `(has-fma)`, not by lane count

Status: accepted
Date: 2026-10-05
Source: CLAUDE.md ("Vector fma"); lib/fib/tensor/linalg.fib; docs/design/aarch64.md (A64-2); docs/shootout/aarch64.md.

## Context

`simd/fma` is exact: on a target without a hardware fused multiply-add it is a libm call per lane (200 ms against 6 ms for a 512 matrix
product). `simd/muladd` is fused only where the target has FMA. The number of lanes of a native vector does not say whether the target
has FMA: an x86-64-v3 CPU has 256-bit vectors and FMA, an x86-64 one without AVX has neither, and every aarch64 CPU has FMA with
128-bit vectors. Choosing a kernel by `(native-lanes f64)` is right on one machine and a 30 times slowdown on the next.

## Decision

Library code that picks a fused kernel asks `(has-fma)`, a constant of the target that the compiler folds. It never infers FMA from
`native-lanes`, from a CPU name or from a vector width. The fused kernels are reached only through the dispatch in `fib.tensor.linalg`.

## Consequences

- One source builds a fast kernel on x86-64-v3 and aarch64 and a safe one on x86-64 and x86-64-v2 (what a release ships).
- A new kernel family gets a `(has-fma)` test at its dispatch, in one place.
- A lane-count test next to an `fma` is treated as a defect, even when it happens to be right on the machine that wrote it.

## Governance

```fibber fitness
(rule "no top-level form in lib/ mentions native-lanes and simd/fma together"
  (forms-mentioning repo ["lib/**.fib"] ["native-lanes" "simd/fma"])
  (plant "lib/fib/tensor/linalg.fib" "\n(defun pick-kernel (a: f64) -> f64 (if (> (native-lanes f64) 2) (simd/fma a a a) a))\n"))

(rule "... nor native-lanes and simd/muladd, nor native-lanes and fma"
  (into (forms-mentioning repo ["lib/**.fib"] ["native-lanes" "simd/muladd"]) (forms-mentioning repo ["lib/**.fib"] ["native-lanes" "fma"]))
  (plant "lib/fib/tensor/linalg.fib" "\n(defun pick-kernel (a: f64) -> f64 (if (> (native-lanes f64) 2) (simd/muladd a a a) a))\n"))

(rule "only the dispatch in fib.tensor.linalg (and the kernels themselves) require the fused kernel modules"
  (requires-into repo ["lib/**.fib" "!lib/fib/tensor/linalg.fib" "!lib/fib/tensor/gemm-fma-*.fib"] "fib.tensor.gemm-fma" [])
  (plant-file "lib/fib/tensor/zz-plant.fib" "(ns fib.tensor.zz-plant (:require [fib.tensor.gemm-fma-f64 :as g]))\n"))

(rule "every call of a fused kernel in linalg sits on a line that tests (has-fma)"
  (filterv (fn (f: Finding) (not (some? (str-find (. f text) "has-fma" 0))))
           (into (grep-live repo ["lib/fib/tensor/linalg.fib"] "(gf32/") (grep-live repo ["lib/fib/tensor/linalg.fib"] "(gf64/")))
  (plant "lib/fib/tensor/linalg.fib" "\n(defun pick-kernel (a b) (gf64/multiply a b))\n"))

(rule "the dispatch does use (has-fma) (the rules above would pass on a library that never chose)"
  (must-contain repo "lib/fib/tensor/linalg.fib" "(if (has-fma)")
  (plant-file "lib/fib/tensor/linalg.fib" ""))

(rule "the primitive the decision rests on is in the spec (a row of spec/types.md names has-fma)"
  (must-contain repo "spec/types.md" "has-fma")
  (plant-file "spec/types.md" "# types\n"))
```

### What this does not check

That `(has-fma)` is true where the hardware has FMA (case `7077`/`7078` and `scripts/mac-check.sh` on a Mac do); that the fused kernels
are correct or fast (the shootout tables); code outside `lib/` (a user program may pick as it likes); a lane-count test that is not
in the same top-level form as an `fma` name (a helper `fast-lanes?` called from the dispatch would not be seen: the rule reads one
form at a time); an `fma` reached through an alias the rule does not list (`s/fma` is not matched: only `simd/fma`, `simd/muladd` and
`fma`).
