;; mandelbrot: the Benchmarks Game's, N x N pixels, 50 iterations, P4 bitmap on stdout.
;; Primitive double and long math in loop/recur; no wrapping arithmetic is needed (the byte is built in a long and narrowed).
(ns mandelbrot
  (:import (java.io BufferedOutputStream OutputStream)))

(set! *warn-on-reflection* true)

(defn- inside? [^double cr ^double ci]
  (loop [i 0 zr 0.0 zi 0.0 tr 0.0 ti 0.0]
    (if (or (>= i 50) (> (+ tr ti) 4.0))
      (<= (+ tr ti) 4.0)
      (let [zi2 (+ (* (* 2.0 zr) zi) ci)
            zr2 (+ (- tr ti) cr)]
        (recur (inc i) zr2 zi2 (* zr2 zr2) (* zi2 zi2))))))

(defn- pack ^long [^long n ^long bx ^double ci]
  (loop [k 0 acc 0]
    (if (< k 8)
      (let [x (+ (* bx 8) k)
            bit (if (and (< x n) (inside? (- (/ (* 2.0 (double x)) (double n)) 1.5) ci)) 1 0)]
        (recur (inc k) (bit-or (bit-shift-left acc 1) (long bit))))
      acc)))

(defn -main [& args]
  (let [n (long (Long/parseLong (first args)))
        w (quot (+ n 7) 8)
        ^bytes row (byte-array w)
        ^OutputStream out (BufferedOutputStream. System/out 65536)
        t0 (System/nanoTime)]
    (.write out (.getBytes (str "P4\n" n " " n "\n")))
    (loop [y 0]
      (when (< y n)
        (let [ci (- (/ (* 2.0 (double y)) (double n)) 1.0)]
          (loop [bx 0]
            (when (< bx w)
              (aset row bx (unchecked-byte (pack n bx ci)))
              (recur (inc bx))))
          (.write out row 0 (int w)))
        (recur (inc y))))
    (.flush out)
    (binding [*out* *err*]
      (println (format "in-program %.3f s" (/ (- (System/nanoTime) t0) 1e9))))))
