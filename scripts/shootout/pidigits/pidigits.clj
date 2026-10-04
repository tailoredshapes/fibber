;; pidigits: the streaming spigot of the Benchmarks Game with java.math.BigInteger through interop, as the published
;; Clojure entries do (type hints throughout, zero reflection warnings, primitive long counters in loop/recur).
;; Run: java -cp clojure.jar:core.specs.alpha.jar:spec.alpha.jar:DIR clojure.main -m pidigits N
;; The in-program time of the computation goes to standard error; the whole-process time is the caller's.
(ns pidigits
  (:import (java.math BigInteger)))

(set! *warn-on-reflection* true)

(defn -main [& args]
  (let [n (long (Long/parseLong (first args)))
        ^BigInteger two (BigInteger/valueOf 2)
        ^BigInteger three (BigInteger/valueOf 3)
        ^BigInteger four (BigInteger/valueOf 4)
        ^BigInteger ten BigInteger/TEN
        sb (StringBuilder.)
        t0 (System/nanoTime)]
    (loop [^BigInteger numer BigInteger/ONE
           ^BigInteger accum BigInteger/ZERO
           ^BigInteger denom BigInteger/ONE
           k 0
           i 0
           ^StringBuilder line (StringBuilder.)]
      (if (< i n)
        (let [k (inc k)
              ^BigInteger y2 (BigInteger/valueOf (inc (* k 2)))
              ^BigInteger accum (.multiply (.add accum (.multiply numer two)) y2)
              ^BigInteger numer (.multiply numer (BigInteger/valueOf k))
              ^BigInteger denom (.multiply denom y2)]
          (if (pos? (.compareTo numer accum))
            (recur numer accum denom k i line)
            (let [d (.intValue (.divide (.add (.multiply numer three) accum) denom))]
              (if (== d (.intValue (.divide (.add (.multiply numer four) accum) denom)))
                (let [i (inc i)]
                  (.append line (char (+ 48 d)))
                  (when (zero? (rem i 10))
                    (.append sb line)
                    (.append sb "\t:")
                    (.append sb i)
                    (.append sb "\n")
                    (.setLength line 0))
                  (recur (.multiply numer ten)
                         (.multiply (.subtract accum (.multiply denom (BigInteger/valueOf d))) ten)
                         denom k i line))
                (recur numer accum denom k i line)))))
        (do
          (when (pos? (.length line))
            (while (< (.length line) 10) (.append line \space))
            (.append sb line)
            (.append sb "\t:")
            (.append sb n)
            (.append sb "\n"))
          (binding [*out* *err*]
            (println (str "clj-compute-s " (/ (double (- (System/nanoTime) t0)) 1e9))))
          (print (str sb))
          (flush))))))
