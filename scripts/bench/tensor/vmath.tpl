;; Vector exp, log and tanh for f64 and f32 tensors (generated constants: scripts/bench/tensor/gen-vmath.py documents the derivation).
;;
;; Four lanes at a time on f64x4 (an f32 tensor is widened four lanes at a time, computed in f64 and rounded back, so a
;; result is within one rounding of the f64 result). No libm call; no table. Accuracy, measured against libm over a dense
;; sweep by cases 7080 and 7081 (the bounds asserted there are the contract):
;;   exp  : max error 1 ULP for f64 in [-708.396, 709.78]; below -708.396 the result is flushed to +0.0 (libm returns subnormals),
;;          above 709.7827 it is +inf; exp(NaN) = NaN, exp(-inf) = 0, exp(+inf) = +inf.
;;   log  : max error 2 ULP for f64 over the positive finite range (subnormal inputs included); log(0) = -inf, log(x < 0) = NaN
;;          (also -inf), log(+inf) = +inf, log(NaN) = NaN.
;;   tanh : max error 2 ULP for f64; tanh(+-inf) = +-1, tanh(+-0.0) = +-0.0 (sign kept), tanh(NaN) = NaN.
;; f32: correctly rounded to within 1 ULP of the f32 libm result in the same ranges (the f64 result is under half an f32 ULP away).
;; Not bit-for-bit libm, and a result may differ from the scalar libm in the last place.
(ns fib.tensor.vmath
  (:use fib.core fib.seq fib.coll fib.tensor.layout fib.tensor.storage))

(defun nan :private () -> f64 (bits->f64 9221120237041090560))
(defun inf :private () -> f64 (bits->f64 9218868437227405312))

;; 2^e for e in [-1022, 1023], by building the exponent field lane by lane
(defun pow2 :private (e: i64x4) -> f64x4
  (let ((b: i64x4 (shl (unchecked-add e (splat i64x4 1023)) 52)))
    (simd (bits->f64 (lane b 0)) (bits->f64 (lane b 1)) (bits->f64 (lane b 2)) (bits->f64 (lane b 3)))))

;; exp(x) = 2^k * exp(r), k = round(x log2 e), r = x - k ln2 (two fused steps), exp(r) a degree-13 Taylor polynomial on |r| <= ln2/2.
;; 2^k is applied as 2^(k -+ 1) and a factor 2 or 1/2 on the polynomial so that k = 1024 and k = -1022 stay representable.
(defun exp4 :private (x: f64x4) -> f64x4
  (let ((xc (simd/min (simd/max x (splat f64x4 -708.4)) (splat f64x4 709.8)))
        (kf (simd/round-even (* xc (splat f64x4 @LOG2E@))))
        (r1 (simd/fma kf (splat f64x4 (neg @LN2_HI@)) xc))
        (r (simd/fma kf (splat f64x4 (neg @LN2_LO@)) r1))
        (p @EXP_POLY@)
        (ki: i64x4 (simd/convert kf))
        (pos (simd/gt kf (splat f64x4 0.0)))
        (m (simd/blend pos (splat i64x4 -1) (splat i64x4 1)))
        (c (simd/blend pos (splat f64x4 2.0) (splat f64x4 0.5)))
        (y (* (* p c) (pow2 (unchecked-add ki m)))))
    (simd/blend (simd/lt x (splat f64x4 -708.396)) (splat f64x4 0.0)
                (simd/blend (simd/gt x (splat f64x4 709.782712893384)) (splat f64x4 (inf)) y))))

;; log(x) = e ln2 + log(m), x = m 2^e with m in [sqrt(1/2), sqrt 2) (integer bit trick on the exponent), log m = 2 s Q(s^2), s = (m-1)/(m+1).
;; Subnormal inputs are scaled by 2^54 first.
(defun log4 :private (x: f64x4) -> f64x4
  (let ((small (simd/lt x (splat f64x4 2.2250738585072014e-308)))
        (xs (simd/blend small (* x (splat f64x4 18014398509481984.0)) x))
        (adj (simd/blend small (splat f64x4 54.0) (splat f64x4 0.0)))
        (hx: i64x4 (simd (f64->bits (lane xs 0)) (f64->bits (lane xs 1)) (f64->bits (lane xs 2)) (f64->bits (lane xs 3))))
        (tmp (unchecked-subtract hx (splat i64x4 4604544271217802189)))
        (ki (sar tmp 52))
        (mb: i64x4 (unchecked-subtract hx (shl ki 52)))
        (m (simd (bits->f64 (lane mb 0)) (bits->f64 (lane mb 1)) (bits->f64 (lane mb 2)) (bits->f64 (lane mb 3))))
        (ef0: f64x4 (simd/convert ki))
        (ef (- ef0 adj))
        (f (- m (splat f64x4 1.0)))
        (s (fdiv f (+ m (splat f64x4 1.0))))
        (z (* s s))
        (q @LOG_POLY@)
        (s2 (* s (splat f64x4 2.0)))
        (logm (simd/fma (* s2 z) q s2))
        (y (simd/fma ef (splat f64x4 @LN2_HI@) (simd/fma ef (splat f64x4 @LN2_LO@) logm)))
        (zero (splat f64x4 0.0)))
    (simd/blend (simd/ne x x) x
                (simd/blend (simd/eq x zero) (splat f64x4 (- 0.0 (inf)))
                            (simd/blend (simd/lt x zero) (splat f64x4 (nan))
                                        (simd/blend (simd/eq x (splat f64x4 (inf))) (splat f64x4 (inf)) y))))))

;; tanh(x) = x + x z G(z), z = x^2, G a degree-13 fit of (tanh x - x)/x^3, for |x| < 0.55; else 1 - 2/(exp(2|x|) + 1) with the sign restored.
(defun tanh4 :private (x: f64x4) -> f64x4
  (let ((a (simd/abs x)) (zero (splat f64x4 0.0)) (one (splat f64x4 1.0))
        (z (* x x))
        (g @TANH_POLY@)
        (small (simd/fma (* x z) g x))
        (t (exp4 (* a (splat f64x4 2.0))))
        (th (- one (fdiv (splat f64x4 2.0) (+ t one))))
        (large (simd/blend (simd/lt x zero) (neg th) th)))
    (simd/blend (simd/eq x zero) x (simd/blend (simd/lt a (splat f64x4 @TANH_T@)) small large))))

(defun apply4 :private (op: i64 x: f64x4) -> f64x4
  (cond (= op 0) (exp4 x) (= op 1) (log4 x) :else (tanh4 x)))

(defun run-f64 :private (op: i64 x: (Tensor f64)) -> (Tensor f64)
  (let ((d (contiguous x)) (n (fib.tensor.storage/size d)) (off (array-get (. d meta) 0)) (src (. d buffer))
        (buf (cell (array (checked-size (shape d) 0.0) 0.0))))
    (do (loop ((i 0))
          (when (< i n)
            (if (<= (+ i 4) n)
                (let ((v: f64x4 (simd-load src (+ off i))))
                  (do (simd-store! &buf i (apply4 op v)) (recur (+ i 4))))
                (let ((v: f64x4 (simd-load-tail src (+ off i))))
                  (simd-store-tail! &buf i (apply4 op v))))))
        (from-array (shape d) @buf 0.0))))

(defun run-f32 :private (op: i64 x: (Tensor f32)) -> (Tensor f32)
  (let ((d (contiguous x)) (n (fib.tensor.storage/size d)) (off (array-get (. d meta) 0)) (src (. d buffer))
        (buf (cell (array (checked-size (shape d) 0.0f32) 0.0f32))))
    (do (loop ((i 0))
          (when (< i n)
            (if (<= (+ i 4) n)
                (let ((v: f32x4 (simd-load src (+ off i))) (w: f64x4 (simd/convert v)) (r: f32x4 (simd/convert (apply4 op w))))
                  (do (simd-store! &buf i r) (recur (+ i 4))))
                (let ((v: f32x4 (simd-load-tail src (+ off i))) (w: f64x4 (simd/convert v)) (r: f32x4 (simd/convert (apply4 op w))))
                  (simd-store-tail! &buf i r)))))
        (from-array (shape d) @buf 0.0f32))))

(defprotocol Transcendental
  (transcend (self op: i64 x: (Tensor Self)) -> (Tensor Self)))
(impl Transcendental f64 (transcend (self op x) (run-f64 op x)))
(impl Transcendental f32 (transcend (self op x) (run-f32 op x)))

(defun exp (x: (Tensor t)) :where ((Element t) (Transcendental t)) -> (Tensor t) (transcend (. x seed) 0 x))
(defun log (x: (Tensor t)) :where ((Element t) (Transcendental t)) -> (Tensor t) (transcend (. x seed) 1 x))
(defun tanh (x: (Tensor t)) :where ((Element t) (Transcendental t)) -> (Tensor t) (transcend (. x seed) 2 x))
