(ns _selftest)
(set! *warn-on-reflection* true)
(set! *unchecked-math* :warn-on-boxed)
(defn -main [& args]
  (let [n (long (Long/parseLong (first args)))
        t0 (System/nanoTime)
        acc (loop [i 0 acc 0]
              (if (>= i n) acc (recur (inc i) (+ acc (rem (* i i) 1000003)))))]
    (binding [*out* *err*] (println (str "clj-compute-s " (/ (double (- (System/nanoTime) t0)) 1e9))))
    (println acc)
    (println "done")))
