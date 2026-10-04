;; binary-trees, the Computer Language Benchmarks Game: allocate and walk complete binary trees, no pooling.
;; usage: clojure.main binary-trees.clj N
;; A deftype with final fields (the Java Node), type hints, loop/recur over primitive longs, zero reflection warnings.
;; Arithmetic is Clojure's checked long arithmetic (the sums stay far below 2^63, so unchecked would change nothing).
;; The in-program seconds of the work go to stderr.
(set! *warn-on-reflection* true)
(ns binary-trees)

;; a field cannot be hinted with its own class: the hint goes on the use, below
(deftype Node [l r])

(defn make ^Node [^long d]
  (if (zero? d)
    (Node. nil nil)
    (Node. (make (dec d)) (make (dec d)))))

(defn check ^long [^Node t]
  (if (nil? (.l t))
    1
    (+ 1 (check ^Node (.l t)) (check ^Node (.r t)))))

(defn run [^long n]
  (let [min-depth 4
        max-depth (max (+ min-depth 2) n)
        stretch (inc max-depth)]
    (println (str "stretch tree of depth " stretch "\t check: " (check (make stretch))))
    (let [long-lived (make max-depth)]
      (loop [d min-depth]
        (when (<= d max-depth)
          (let [iters (bit-shift-left 1 (+ (- max-depth d) min-depth))
                sum (loop [i 0 s 0]
                      (if (< i iters) (recur (inc i) (+ s (check (make d)))) s))]
            (println (str iters "\t trees of depth " d "\t check: " sum))
            (recur (+ d 2)))))
      (println (str "long lived tree of depth " max-depth "\t check: " (check long-lived))))))

(defn -main [& args]
  (let [n (if (seq args) (Long/parseLong (first args)) 10)
        t0 (System/nanoTime)]
    (run n)
    (.println System/err (str "in-program-seconds " (/ (- (System/nanoTime) t0) 1e9)))))

(when (seq *command-line-args*) (apply -main *command-line-args*))
