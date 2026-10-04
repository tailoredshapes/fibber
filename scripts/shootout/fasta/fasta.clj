;; fasta (Computer Language Benchmarks Game), Clojure 1.12. Primitive math with unchecked arithmetic (the LCG never overflows a long),
;; a byte-array output buffer, loop/recur, type hints with no reflection. The in-program time goes to stderr.
(ns fasta
  (:gen-class))

(set! *warn-on-reflection* true)
(set! *unchecked-math* :warn-on-boxed)

(def ^:const IM 139968)
(def ^:const IA 3877)
(def ^:const IC 29573)
(def ^:const CAP 65536)

(def ^"[B" alu
  (.getBytes (str "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG"
                  "GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA"
                  "CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT"
                  "ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA"
                  "GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG"
                  "AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC"
                  "AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA")))

(defn- flush-buf [^java.io.OutputStream out ^bytes buf ^longs pos]
  (.write out buf 0 (int (aget pos 0)))
  (aset pos 0 0))

(defn- put-str [^String s ^bytes buf ^longs pos]
  (let [p (aget pos 0)
        n (.length s)]
    (loop [i 0]
      (when (< i n)
        (aset buf (+ p i) (byte (int (.charAt s (int i)))))
        (recur (inc i))))
    (aset pos 0 (+ p n))))

(defn- repeat-fasta [^java.io.OutputStream out ^bytes buf ^longs pos ^long n]
  (let [m (alength alu)]
    (loop [left n k 0]
      (when (> left 0)
        (let [w (min left 60)
              p (aget pos 0)]
          (loop [j 0]
            (when (< j w)
              (aset buf (+ p j) (aget alu (rem (+ k j) m)))
              (recur (inc j))))
          (aset buf (+ p w) (byte 10))
          (aset pos 0 (+ p w 1))
          (when (> (aget pos 0) (- CAP 61)) (flush-buf out buf pos))
          (recur (- left w) (rem (+ k w) m)))))))

(defn- cumulative ^doubles [^doubles p]
  (let [c (double-array (alength p))]
    (loop [i 0 acc 0.0]
      (when (< i (alength p))
        (let [a (+ acc (aget p i))]
          (aset c i a)
          (recur (inc i) a))))
    c))

(defn- random-fasta [^java.io.OutputStream out ^bytes buf ^longs pos ^doubles cum ^bytes syms n seed]
  (let [m (dec (alength cum))]
    (loop [left (long n) last (long seed)]
      (if (> left 0)
        (let [w (min left 60)
              p (aget pos 0)
              last2 (long (loop [j 0 l last]
                      (if (< j w)
                        (let [l2 (rem (+ (* l IA) IC) IM)
                              r (/ (double l2) 139968.0)
                              i (loop [i 0] (if (and (< i m) (>= r (aget cum i))) (recur (inc i)) i))]
                          (aset buf (+ p j) (aget syms i))
                          (recur (inc j) l2))
                        l)))]
          (aset buf (+ p w) (byte 10))
          (aset pos 0 (+ p w 1))
          (when (> (aget pos 0) (- CAP 61)) (flush-buf out buf pos))
          (recur (- left w) last2))
        last))))

(defn -main [& args]
  (let [n (if (seq args) (Long/parseLong (first args)) 1000)
        t0 (System/nanoTime)
        out ^java.io.OutputStream System/out
        buf (byte-array CAP)
        pos (long-array 1)
        iub-s (.getBytes "acgtBDHKMNRSVWY")
        iub-c (cumulative (double-array [0.27 0.12 0.12 0.27 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02]))
        hs-s (.getBytes "acgt")
        hs-c (cumulative (double-array [0.3029549426680 0.1979883004921 0.1975473066391 0.3015094502008]))]
    (put-str ">ONE Homo sapiens alu\n" buf pos)
    (repeat-fasta out buf pos (* n 2))
    (put-str ">TWO IUB ambiguity codes\n" buf pos)
    (let [s (random-fasta out buf pos iub-c iub-s (* n 3) 42)]
      (put-str ">THREE Homo sapiens frequency\n" buf pos)
      (random-fasta out buf pos hs-c hs-s (* n 5) s))
    (flush-buf out buf pos)
    (.flush out)
    (binding [*out* *err*]
      (println (format "in-program %.3f s" (/ (- (System/nanoTime) t0) 1e9))))))
