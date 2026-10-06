# Writes the fibber side of the fib.sql differential test: one program that formats every query of corpus.edn with the three dialects of
# oracle.clj and prints the same lines. The corpus is Clojure data that is also fibber source for the `sql` macro.
import sys
lines = [l.strip() for l in open(sys.argv[1]) if l.strip()]
out = ['(ns main (:use fib.sql) (:require [fib.db.core :as db]))',
       '(defun param-text (d: Datum) -> str',
       '  (match d ((DNil) "nil") ((DStr s) (str "s:" s)) ((DInt i) (str "i:" i)) ((DFloat f) (str "f:" f)) ((DBool b) (str "b:" b)) (_ "?:")))',
       '(defun show (q: db/Query) -> unit',
       '  (println (reduce (fn (acc: str d: Datum) (str acc " | " (param-text d))) (. q sql) (. q params))))',
       '(defun main () -> i64', '  (do']
for l in lines:
    for d in [':none', ':ansi', ':mysql']:
        out.append('    (show (format-with (sql %s) %s))' % (l, d))
out.append('    0))')
print('\n'.join(out))
