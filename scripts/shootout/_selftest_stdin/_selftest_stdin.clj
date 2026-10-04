(ns _selftest_stdin)
(set! *warn-on-reflection* true)
(defn -main [& args]
  (let [^java.io.InputStream in System/in
        buf (byte-array 65536)
        t0 (System/nanoTime)
        total (loop [total 0]
                (let [n (.read in buf)]
                  (if (pos? n) (recur (+ total n)) total)))]
    (binding [*out* *err*] (println (str "clj-compute-s " (/ (double (- (System/nanoTime) t0)) 1e9))))
    (println total)
    (println "done")))
