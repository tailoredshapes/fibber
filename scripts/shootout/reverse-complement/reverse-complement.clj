;; reverse-complement, Clojure 1.12 twin: the same algorithm as reverse-complement.java (stdin in 64 KB blocks, the complemented
;; residues of a sequence kept as a list of byte arrays, written back to front in 60-column lines through a 64 KB buffer).
;; Run as a script: clojure.main reverse-complement.clj (the file name has a hyphen, so there is no -m form). *unchecked-math* is on
;; (the Java version has no overflow checks either); *warn-on-reflection* is on and the file loads with zero warnings.
;; In-program time: the whole of -main is timed with System/nanoTime and printed to stderr.
(set! *warn-on-reflection* true)
(set! *unchecked-math* true)
(import '(java.util ArrayList Arrays) '(java.io FileInputStream FileOutputStream FileDescriptor))

(def ^"[B" out (byte-array 65536))
(def ^"[J" st (long-array 1)) ; the write position of `out`
(def ^FileOutputStream os (FileOutputStream. FileDescriptor/out))

(defn put! [^long b]
  (when (== (aget st 0) 65536)
    (.write os out 0 65536)
    (aset st 0 0))
  (aset out (aget st 0) (byte b))
  (aset st 0 (inc (aget st 0))))

(defn emit! [^ArrayList chunks]
  (loop [k (dec (.size chunks)) col 0]
    (if (>= k 0)
      (let [^bytes c (.get chunks k)
            col (long (loop [i (dec (alength c)) col col]
                  (if (>= i 0)
                    (do (put! (aget c i))
                        (if (== (inc col) 60)
                          (do (put! 10) (recur (dec i) 0))
                          (recur (dec i) (inc col))))
                    col)))]
        (recur (dec k) col))
      (when (> col 0) (put! 10)))))

(defn -main [& _]
  (let [t0 (System/nanoTime)
        tbl (byte-array 256)
        from "ACGTUMRWSYKVHDBN"
        to "TGCAAKYWSRMBDHVN"
        _ (dotimes [i 16]
            (aset tbl (int (.charAt from i)) (byte (int (.charAt to i))))
            (aset tbl (+ (int (.charAt from i)) 32) (byte (int (.charAt to i)))))
        in (FileInputStream. FileDescriptor/in)
        b (byte-array 65536)
        chunks (volatile! (ArrayList.))
        mode (long-array 1)]
    (loop []
      (let [n (.read in b 0 65536)]
        (when (pos? n)
          (let [tmp (byte-array n)
                j (loop [i 0 j 0]
                    (if (< i n)
                      (let [c (bit-and (aget b i) 255)]
                        (cond
                          (== (aget mode 0) 1) (do (put! c) (when (== c 10) (aset mode 0 0)) (recur (inc i) j))
                          (== c 10) (recur (inc i) j)
                          (== c 62) (let [j (long j)]
                                      (when (pos? j) (.add ^ArrayList @chunks (Arrays/copyOf tmp (int j))))
                                      (emit! @chunks)
                                      (vreset! chunks (ArrayList.))
                                      (aset mode 0 1)
                                      (put! c)
                                      (recur (inc i) 0))
                          :else (do (aset tmp j (aget tbl c)) (recur (inc i) (inc j)))))
                      j))]
            (when (pos? j) (.add ^ArrayList @chunks (Arrays/copyOf tmp (int j)))))
          (recur))))
    (emit! @chunks)
    (.write os out 0 (int (aget st 0)))
    (.flush os)
    (binding [*out* *err*] (println (str "in-program-ms " (quot (- (System/nanoTime) t0) 1000000))))))

(-main)
