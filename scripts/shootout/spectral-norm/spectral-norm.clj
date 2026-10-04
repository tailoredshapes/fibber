(ns spectral-norm)
(set! *warn-on-reflection* true)
(set! *unchecked-math* :warn-on-boxed)

;; Same algorithm and summation order as the Java twin. int arithmetic is exact here (no wrapping needed for n <= 5500),
;; *unchecked-math* only removes the overflow checks the Java version does not have either.
(defmacro a [i j]
  `(/ 1.0 (double (+ (quot (* (+ ~i ~j) (+ ~i ~j 1)) 2) ~i 1))))

(defn mul-av [^long n ^doubles v ^doubles av]
  (loop [i 0]
    (when (< i n)
      (aset av i (double (loop [j 0 s 0.0]
                   (if (< j n) (recur (inc j) (+ s (* (double (a i j)) (aget v j)))) s))))
      (recur (inc i)))))

(defn mul-atv [^long n ^doubles v ^doubles atv]
  (loop [i 0]
    (when (< i n)
      (aset atv i (double (loop [j 0 s 0.0]
                    (if (< j n) (recur (inc j) (+ s (* (double (a j i)) (aget v j)))) s))))
      (recur (inc i)))))

(defn mul-atav [^long n ^doubles v ^doubles tmp ^doubles atav]
  (mul-av n v tmp)
  (mul-atv n tmp atav))

(defn -main [& args]
  (let [n (long (Long/parseLong (first args)))
        u (double-array n 1.0)
        v (double-array n)
        tmp (double-array n)
        t0 (System/nanoTime)]
    (dotimes [_ 10]
      (mul-atav n u tmp v)
      (mul-atav n v tmp u))
    (let [[vbv vv] (loop [i 0 vbv 0.0 vv 0.0]
                     (if (< i n)
                       (recur (inc i) (+ vbv (* (aget u i) (aget v i))) (+ vv (* (aget v i) (aget v i))))
                       [vbv vv]))]
      (binding [*out* *err*]
        (println (format "in-program: %.3f s" (/ (- (System/nanoTime) t0) 1e9))))
      (println (format "%.9f" (Math/sqrt (/ (double vbv) (double vv))))))))

(apply -main *command-line-args*)
