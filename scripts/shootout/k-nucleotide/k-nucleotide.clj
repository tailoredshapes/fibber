;; k-nucleotide, Clojure 1.12 twin: the same algorithm as k-nucleotide.java (stdin as bytes, the THREE sequence as 2-bit codes in a
;; byte array, every k-mer of length 1, 2, 3, 4, 6, 12, 18 counted in a java.util.HashMap keyed by the k-mer as a long).
;; Run as a script: clojure.main k-nucleotide.clj (the file name has a hyphen, so there is no -m form). *unchecked-math* is on
;; (the Java version has no overflow checks either); *warn-on-reflection* is on and the file loads with zero warnings.
;; In-program time: the whole of -main (reading, counting, printing) is timed with System/nanoTime and printed to stderr.
(set! *warn-on-reflection* true)
(set! *unchecked-math* true)
(import '(java.util HashMap ArrayList Arrays Locale Map$Entry) '(java.io FileInputStream FileDescriptor InputStream))

(defn count-kmers ^HashMap [^bytes sq ^long n ^long k]
  (let [m (HashMap.)
        mask (dec (bit-shift-left 1 (* 2 k)))]
    (loop [i 0 key 0]
      (when (< i n)
        (let [key (bit-and (bit-or (bit-shift-left key 2) (aget sq i)) mask)]
          (when (>= i (dec k))
            (let [kk (Long/valueOf key)
                  ^Integer c (.get m kk)]
              (.put m kk (Integer/valueOf (if c (inc (.intValue c)) 1)))))
          (recur (inc i) key))))
    m))

(defn decode ^String [^long key ^long k]
  (let [cs (char-array k)]
    (dotimes [i k]
      (aset cs (- (dec k) i) (.charAt "ACGT" (bit-and (unsigned-bit-shift-right key (* 2 i)) 3))))
    (String. cs)))

(defn encode ^long [^String s]
  (loop [i 0 key 0]
    (if (< i (.length s)) (recur (inc i) (+ (* key 4) (.indexOf "ACGT" (int (.charAt s i))))) key)))

(defn frequencies! [^bytes sq ^long n ^long k ^StringBuilder out]
  (let [^HashMap m (count-kmers sq n k)
        total (inc (- n k))
        es (sort-by (fn [[kk v]] [(- (long v)) (long kk)])
                    (map (fn [^Map$Entry e] [(.getKey e) (.getValue e)]) (.entrySet m)))]
    (doseq [[kk v] es]
      (.append out (decode (long kk) k)) (.append out \space)
      (let [v (quot (+ (* (long v) 200000) total) (* 2 total)) fp (rem v 1000)] ; exact integer percent, half up (as the other twins)
        (.append out (str (quot v 1000) "." (cond (< fp 10) "00" (< fp 100) "0" :else "") fp)))
      (.append out \newline))
    (.append out \newline)))

(defn count-of! [^bytes sq ^long n ^String pat ^StringBuilder out]
  (let [^HashMap m (count-kmers sq n (.length pat))
        ^Integer c (.get m (Long/valueOf (encode pat)))]
    (.append out (if c (.intValue c) 0)) (.append out \tab) (.append out pat) (.append out \newline)))

(defn -main [& _]
  (let [t0 (System/nanoTime)
        tbl (byte-array 256 (byte -1))
        _ (dotimes [i 4]
            (aset tbl (int (.charAt "ACGT" i)) (byte i))
            (aset tbl (int (.charAt "acgt" i)) (byte i)))
        want (.getBytes ">THREE")
        in (FileInputStream. FileDescriptor/in)
        b (byte-array 65536)
        chunks (ArrayList.)
        st (long-array 4) ; state: 0 mode, 1 hp, 2 ok, 3 total residues
        _ (loop []
            (let [len (if (< (aget st 0) 3) (.read in b 0 65536) -1)]
              (when (pos? len)
                (let [tmp (byte-array len)
                      j (loop [i 0 mode (aget st 0) hp (aget st 1) ok (aget st 2) j 0]
                          (if (and (< i len) (< mode 3))
                            (let [c (bit-and (aget b i) 255)]
                              (cond
                                (== mode 1) (if (== c 10)
                                              (recur (inc i) (if (and (== ok 1) (>= hp 6)) 2 0) 0 ok j)
                                              (recur (inc i) mode (inc hp) (if (and (< hp 6) (not (== c (aget want hp)))) 0 ok) j))
                                (== c 62) (if (== mode 2)
                                            (recur (inc i) 3 hp ok j)
                                            (recur (inc i) 1 1 1 j))
                                (and (== mode 2) (>= (aget tbl c) 0)) (do (aset tmp j (aget tbl c)) (recur (inc i) mode hp ok (inc j)))
                                :else (recur (inc i) mode hp ok j)))
                            (do (aset st 0 mode) (aset st 1 hp) (aset st 2 (long ok)) j)))]
                  (when (pos? j) (.add chunks (Arrays/copyOf tmp (int j))))
                  (aset st 3 (+ (aget st 3) (long j)))
                  (recur)))))
        total (aget st 3)
        sq (byte-array total)
        _ (loop [i 0 off 0]
            (when (< i (.size chunks))
              (let [^bytes c (.get chunks i)]
                (System/arraycopy c 0 sq off (alength c))
                (recur (inc i) (+ off (alength c))))))
        out (StringBuilder.)]
    (frequencies! sq total 1 out)
    (frequencies! sq total 2 out)
    (doseq [p ["GGT" "GGTA" "GGTATT" "GGTATTTTAATT" "GGTATTTTAATTTATAGT"]] (count-of! sq total p out))
    (print (str out))
    (flush)
    (binding [*out* *err*] (println (str "in-program-ms " (quot (- (System/nanoTime) t0) 1000000))))))

(-main)
