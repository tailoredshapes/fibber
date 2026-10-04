;; regex-redux, the Computer Language Benchmarks Game, in Clojure on java.util.regex (as the published
;; entries are): re-pattern and a Matcher loop for the counts, Matcher.replaceAll for the substitutions.
;; Type hints on every interop call, zero reflection warnings; no arithmetic beyond the counts.
;; Run:  clojure.main -m regex-redux            (the file is regex_redux.clj, the namespace regex-redux)
;; The in-program time of the work (after the JVM and clojure.core are up) goes to stderr.
(ns regex-redux
  (:import (java.io InputStream)
           (java.nio.charset StandardCharsets)
           (java.util.regex Matcher Pattern)))

(set! *warn-on-reflection* true)

(def variants
  ["agggtaaa|tttaccct"
   "[cgt]gggtaaa|tttaccc[acg]"
   "a[act]ggtaaa|tttacc[agt]t"
   "ag[act]gtaaa|tttac[agt]ct"
   "agg[act]taaa|ttta[agt]cct"
   "aggg[acg]aaa|ttt[cgt]ccct"
   "agggt[cgt]aa|tt[acg]accct"
   "agggta[cgt]a|t[acg]taccct"
   "agggtaa[cgt]|[acg]ttaccct"])

(def substitutions
  [["tHa[Nt]" "<4>"]
   ["aND|caN|Ha[DS]|WaS" "<3>"]
   ["a[NSt]|BY" "<2>"]
   ["<[^>]*>" "|"]
   ["\\|[^|][^|]*\\|" "-"]])

(defn count-matches ^long [^Pattern p ^String s]
  (let [m (.matcher p s)]
    (loop [c 0]
      (if (.find m) (recur (inc c)) c))))

(defn replace-all ^String [^String pat ^String s ^String repl]
  (.replaceAll (.matcher (re-pattern pat) s) repl))

(defn -main [& _]
  (let [^String input (String. (.readAllBytes ^InputStream System/in) StandardCharsets/ISO_8859_1)
        t0 (System/nanoTime)
        ilen (.length input)
        ^String seq (replace-all ">.*\n|\n" input "")
        clen (.length seq)
        counts (mapv (fn [^String p] (count-matches (re-pattern p) seq)) variants)
        ^String result (reduce (fn [^String s [^String pat ^String repl]] (replace-all pat s repl)) seq substitutions)]
    (doseq [[p c] (map vector variants counts)]
      (println p c))
    (println)
    (println ilen)
    (println clen)
    (println (.length result))
    (binding [*out* *err*]
      (println (format "in-program: %.3f s" (/ (- (System/nanoTime) t0) 1e9))))))
