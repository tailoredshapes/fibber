;; fannkuch-redux, single-threaded: the same rotate-and-count algorithm as the fibber, Java and C versions.
;; Primitive long math (checked arithmetic is kept: nothing here overflows), int-array-free: long-array with ^longs hints.
(ns fannkuch-redux
  (:gen-class))

(set! *warn-on-reflection* true)
(set! *unchecked-math* :warn-on-boxed)

(defn fannkuch [^long n]
  (let [p (long-array n)
        p1 (long-array n)
        cnt (long-array n)]
    (dotimes [i n] (aset p1 i i))
    (loop [r n pc 0 maxf 0 sum 0]
      (let [r (long (loop [r r] (if (== r 1) r (do (aset cnt (dec r) r) (recur (dec r))))))]
        (dotimes [i n] (aset p i (aget p1 i)))
        (let [flips (long (loop [flips 0]
                            (let [k (aget p 0)]
                              (if (== k 0)
                                flips
                                (do (loop [i 0 j k]
                                      (when (< i j)
                                        (let [t (aget p i)]
                                          (aset p i (aget p j))
                                          (aset p j t)
                                          (recur (inc i) (dec j)))))
                                    (recur (inc flips)))))))
              maxf (if (> flips maxf) flips maxf)
              sum (if (even? pc) (+ sum flips) (- sum flips))
              r (long (loop [r 1]
                        (if (== r n)
                          n
                          (let [p0 (aget p1 0)]
                            (loop [i 0] (when (< i r) (aset p1 i (aget p1 (inc i))) (recur (inc i))))
                            (aset p1 r p0)
                            (aset cnt r (dec (aget cnt r)))
                            (if (> (aget cnt r) 0) r (recur (inc r)))))))]
          (if (== r n)
            [sum maxf]
            (recur r (inc pc) maxf sum)))))))

(defn -main [& args]
  (let [n (if (seq args) (Long/parseLong (first args)) 7)
        t0 (System/nanoTime)
        [sum maxf] (fannkuch n)]
    (println sum)
    (println (str "Pfannkuchen(" n ") = " maxf))
    (binding [*out* *err*]
      (println (str "in-program-seconds " (/ (- (System/nanoTime) t0) 1e9))))))

;; run as a script: clojure.main fannkuch-redux.clj N
(when (seq *command-line-args*) (apply -main *command-line-args*))
