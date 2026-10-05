;; Comparison harness for tsmarsh/sudoku, commit 743e4640, EPL-1.0.
;; Run with the original source and core.logic on the classpath, for example:
;; java -cp Clojure.jar:core.logic.jar:tsmarsh-sudoku/src clojure.main scripts/bench/sudoku.clj
(require '[sudoku.sudoku :as sudoku])

(def test-puzzle [0 0 6 3 1 7 8 0 0
                 3 0 0 0 0 0 0 0 1
                 0 0 7 0 0 0 3 0 0
                 7 0 0 9 0 6 0 0 8
                 0 6 0 0 0 0 0 4 0
                 8 0 0 5 0 4 0 0 7
                 0 0 1 0 0 0 5 0 0
                 4 0 0 0 0 0 0 0 6
                 0 0 8 7 5 1 2 0 0])
(def hard-puzzle [0 7 0 8 0 0 0 9 0
                 9 0 0 0 7 0 0 0 3
                 0 0 0 5 0 0 0 7 0
                 1 0 0 0 8 4 0 0 0
                 7 0 0 0 0 0 0 8 1
                 8 0 0 0 5 0 4 0 0
                 0 0 0 0 0 0 9 2 0
                 4 9 0 0 0 3 1 0 8
                 0 0 6 0 2 8 3 0 0])

(defn valid-solution? [puzzle solution]
  (let [units (concat
               (for [r (range 9)] (for [c (range 9)] (+ (* r 9) c)))
               (for [c (range 9)] (for [r (range 9)] (+ (* r 9) c)))
               (for [br (range 0 9 3) bc (range 0 9 3)]
                 (for [r (range br (+ br 3)) c (range bc (+ bc 3))]
                   (+ (* r 9) c))))]
    (and (= 81 (count solution))
         (every? (fn [[hint value]] (or (zero? hint) (= hint value)))
                 (map vector puzzle solution))
         (every? #(= (set (range 1 10)) (set (map solution %))) units))))

(defn check-answer [puzzle answer]
  (assert (= 1 (count answer)))
  (assert (valid-solution? puzzle (first answer))))

(defn measure [name puzzle]
  ;; Force both the result stream and each grid before stopping the clock.
  ;; Validation stays outside the timed region. Match sudoku.fib's 100 warmups
  ;; followed by 31 measured solves in the same process.
  (dotimes [_ 100]
    (check-answer puzzle (mapv vec (sudoku/sudokufd puzzle))))
  (let [times (mapv (fn [_]
                     (let [start (System/nanoTime)
                           answer (mapv vec (sudoku/sudokufd puzzle))
                           elapsed (- (System/nanoTime) start)]
                       (check-answer puzzle answer)
                       elapsed))
                   (range 31))]
    (println name "ns samples" times "median-after-warmup"
             (nth (sort times) 15))))

(measure "test" test-puzzle)
(measure "hard" hard-puzzle)
