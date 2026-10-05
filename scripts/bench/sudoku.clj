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

(defn measure [name puzzle]
  (let [times (doall (for [_ (range 6)]
                       (let [start (System/nanoTime)
                             answer (sudoku/sudokufd puzzle)
                             elapsed (- (System/nanoTime) start)]
                         (assert (= 1 (count answer)))
                         elapsed)))]
    (println name "ns samples" times "median-after-warmup"
             (nth (sort (rest times)) 2))))

(measure "test" test-puzzle)
(measure "hard" hard-puzzle)
