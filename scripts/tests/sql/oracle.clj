;; The HoneySQL side of the fib.sql differential test (scripts/test-sql.sh): formats every query of corpus.edn in three ways (no quoting,
;; ANSI quoting as PostgreSQL and SQLite use, MySQL's backticks) and prints one line per result: the SQL, then each parameter as
;; `| kind:value`, the same text the fibber side prints.
(require '[honey.sql :as sql] '[clojure.string :as s] '[clojure.edn :as edn])

(defn param-text [p]
  (cond (nil? p) "nil" (string? p) (str "s:" p) (integer? p) (str "i:" p) (float? p) (str "f:" p)
        (boolean? p) (str "b:" p) :else (str "?:" p)))

(defn line [[q & ps]] (s/join " " (cons q (map #(str "| " (param-text %)) ps))))

(doseq [text (remove s/blank? (s/split-lines (slurp (first *command-line-args*))))]
  (let [q (edn/read-string text)]
    (doseq [opts [{} {:quoted true} {:dialect :mysql}]]
      (println (try (line (sql/format q opts)) (catch Exception e (str "ERROR " (.getMessage e))))))))
