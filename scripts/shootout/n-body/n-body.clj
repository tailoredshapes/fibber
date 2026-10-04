;; n-body, the Benchmarks Game. All five bodies in one primitive double array, body i at 7i .. 7i+6:
;; x y z vx vy vz mass. Doubles need no unchecked-math (the only integer arithmetic is the index, in primitive
;; longs); zero reflection warnings. Run as a script, `clojure.main n-body.clj N` (the file name has a hyphen, so
;; `-m` cannot find the namespace); the last form calls -main.
(ns nbody)
(set! *warn-on-reflection* true)
(set! *unchecked-math* :warn-on-boxed)

(def ^:const solar-mass (* 4.0 Math/PI Math/PI))
(def ^:const days-per-year 365.24)

(defn- body! [^doubles a i x y z vx vy vz m]
  (let [o (* 7 (long i))]
    (aset a o (double x)) (aset a (+ o 1) (double y)) (aset a (+ o 2) (double z))
    (aset a (+ o 3) (* (double vx) days-per-year)) (aset a (+ o 4) (* (double vy) days-per-year))
    (aset a (+ o 5) (* (double vz) days-per-year)) (aset a (+ o 6) (* (double m) solar-mass))))

(defn- advance! [^doubles a ^double dt]
  (loop [i 0]
    (when (< i 5)
      (let [oi (* 7 i)]
        (loop [j (inc i)]
          (when (< j 5)
            (let [oj (* 7 j)
                  dx (- (aget a oi) (aget a oj))
                  dy (- (aget a (+ oi 1)) (aget a (+ oj 1)))
                  dz (- (aget a (+ oi 2)) (aget a (+ oj 2)))
                  d2 (+ (+ (* dx dx) (* dy dy)) (* dz dz))
                  mag (/ dt (* d2 (Math/sqrt d2)))
                  mi (aget a (+ oi 6))
                  mj (aget a (+ oj 6))]
              (aset a (+ oi 3) (- (aget a (+ oi 3)) (* (* dx mj) mag)))
              (aset a (+ oi 4) (- (aget a (+ oi 4)) (* (* dy mj) mag)))
              (aset a (+ oi 5) (- (aget a (+ oi 5)) (* (* dz mj) mag)))
              (aset a (+ oj 3) (+ (aget a (+ oj 3)) (* (* dx mi) mag)))
              (aset a (+ oj 4) (+ (aget a (+ oj 4)) (* (* dy mi) mag)))
              (aset a (+ oj 5) (+ (aget a (+ oj 5)) (* (* dz mi) mag)))
              (recur (inc j))))))
      (recur (inc i))))
  (loop [i 0]
    (when (< i 5)
      (let [o (* 7 i)]
        (aset a o (+ (aget a o) (* dt (aget a (+ o 3)))))
        (aset a (+ o 1) (+ (aget a (+ o 1)) (* dt (aget a (+ o 4)))))
        (aset a (+ o 2) (+ (aget a (+ o 2)) (* dt (aget a (+ o 5)))))
        (recur (inc i))))))

(defn- energy ^double [^doubles a]
  (loop [i 0 e 0.0]
    (if (< i 5)
      (let [oi (* 7 i)
            vx (aget a (+ oi 3)) vy (aget a (+ oi 4)) vz (aget a (+ oi 5)) mi (aget a (+ oi 6))
            e1 (+ e (* (* 0.5 mi) (+ (+ (* vx vx) (* vy vy)) (* vz vz))))
            e2 (double (loop [j (inc i) e (double e1)]
                 (if (< j 5)
                   (let [oj (* 7 j)
                         dx (- (aget a oi) (aget a oj))
                         dy (- (aget a (+ oi 1)) (aget a (+ oj 1)))
                         dz (- (aget a (+ oi 2)) (aget a (+ oj 2)))]
                     (recur (inc j) (- e (/ (* mi (aget a (+ oj 6))) (Math/sqrt (+ (+ (* dx dx) (* dy dy)) (* dz dz)))))))
                   e)))]
        (recur (inc i) e2))
      e)))

(defn- show ^String [^double x] (String/format java.util.Locale/ROOT "%.9f" (to-array [x])))

(defn -main [& args]
  (let [n (Long/parseLong (first args))
        a (double-array 35)]
    (body! a 0 0.0 0.0 0.0 0.0 0.0 0.0 1.0)
    (body! a 1 4.84143144246472090e+00 -1.16032004402742839e+00 -1.03622044471123109e-01
           1.66007664274403694e-03 7.69901118419740425e-03 -6.90460016972063023e-05 9.54791938424326609e-04)
    (body! a 2 8.34336671824457987e+00 4.12479856412430479e+00 -4.03523417114321381e-01
           -2.76742510726862411e-03 4.99852801234917238e-03 2.30417297573763929e-05 2.85885980666130812e-04)
    (body! a 3 1.28943695621391310e+01 -1.51111514016986312e+01 -2.23307578892655734e-01
           2.96460137564761618e-03 2.37847173959480950e-03 -2.96589568540237556e-05 4.36624404335156298e-05)
    (body! a 4 1.53796971148509165e+01 -2.59193146099879641e+01 1.79258772950371181e-01
           2.68067772490389322e-03 1.62824170038242295e-03 -9.51592254519715870e-05 5.15138902046611451e-05)
    (let [px (loop [i 0 s 0.0] (if (< i 5) (recur (inc i) (+ s (* (aget a (+ (* 7 i) 3)) (aget a (+ (* 7 i) 6))))) s))
          py (loop [i 0 s 0.0] (if (< i 5) (recur (inc i) (+ s (* (aget a (+ (* 7 i) 4)) (aget a (+ (* 7 i) 6))))) s))
          pz (loop [i 0 s 0.0] (if (< i 5) (recur (inc i) (+ s (* (aget a (+ (* 7 i) 5)) (aget a (+ (* 7 i) 6))))) s))]
      (aset a 3 (/ (- (double px)) solar-mass))
      (aset a 4 (/ (- (double py)) solar-mass))
      (aset a 5 (/ (- (double pz)) solar-mass)))
    (println (show (energy a)))
    (let [t0 (System/nanoTime)]
      (loop [k 0] (when (< k n) (advance! a 0.01) (recur (inc k))))
      (binding [*out* *err*]
        (println (str "in-program " (/ (- (System/nanoTime) t0) 1e9) " s"))))
    (println (show (energy a)))))

(apply -main *command-line-args*)
