#!/usr/bin/env python3
# compiler/tests/live/gen-load.py THREADS SWAPS: a live session script in which THREADS workers spawned in the session call `handler`
# in a loop while the main thread redefines it SWAPS times (alternating two bodies); at the end the workers are stopped and joined and
# the session prints the calls made and the answers that were of neither body (`; calls N`, `; bad N`).
import sys
threads = int(sys.argv[1]) if len(sys.argv) > 1 else 8
swaps = int(sys.argv[2]) if len(sys.argv) > 2 else 40
print("; under load: workers spawned in the session call handler while it is redefined")
print("(def stop: (Atom bool) (atom false))")
print("(def tasks: (Atom (Vec (Task i64))) (atom []))")
print("(def bad: (Atom i64) (atom 0))")
print("(defun handler (x: i64) -> i64 (+ x 1))")
print("(defun worker () -> i64 (loop ((n 0)) (if @stop n (let ((r (handler n))) (do (when (and (not (= r (+ n 1))) (not (= r (+ n 2)))) (swap! bad inc)) (recur (+ n 1)))))))")
for i in range(threads):
    print("(swap! tasks conj (spawn (fn () -> i64 (worker))))")
for i in range(swaps):
    print("(defun handler (x: i64) -> i64 (+ x %d))" % (2 if i % 2 == 0 else 1))
print("(reset! stop true)")
print("(println (str \"; calls \" (reduce + 0 (mapv (fn (t: (Task i64)) -> i64 (join t)) @tasks))))")
print("(println (str \"; bad \" @bad))")
