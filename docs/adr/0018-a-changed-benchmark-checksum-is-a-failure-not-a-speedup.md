# 0018. A changed benchmark checksum is a failure, not a speedup

Status: accepted
Date: 2026-10-06
Source: scripts/bench/quick.sh ("a wrong answer is not a speedup"); CLAUDE.md ("Performance cycle": "a changed checksum (a wrong answer is a failure, not a speedup)");
memory: "Specialising is fine ... measure first".

## Context

Every optimisation of the compiler, the runtime or the library is judged by a benchmark, and a wrong program is usually a fast one: a
loop that exits early, a result that is not used, a specialisation that drops a case. `scripts/bench/quick.sh` runs ten programs, takes the
median of three runs, and compares both the time and a checksum (the first 12 hex digits of the sha1 of what the program printed) with
`scripts/bench/baseline.tsv`. The rule that a changed answer fails whatever the time was a line in a script header and a sentence in
CLAUDE.md.

## Decision

1. Every row of `scripts/bench/baseline.tsv` is `name`, a median in seconds and a checksum of 12 lower-case hex digits, in tabs; no name
   appears twice; the checksum is never `UNSTABLE` (a program whose output differs between runs has no checksum to hold); every
   name is a program `scripts/bench/NAME.fib`. At least ten rows.
2. `quick.sh` keeps the rule in code: a checksum that differs from the baseline's is a `WRONG ANSWER` that sets the failure flag and decides
   before the timing is looked at; a run whose output differs from the previous run's is `UNSTABLE`; a failure flag ends with exit status 1
   (a regression alone is 3).

## Consequences

- `--record` on a tree whose answers are wrong would record the wrong checksum: the file is only as right as the tree it was recorded
  from (its header says so). This ADR keeps the file well-formed, not right.
- A benchmark added to the quick set needs a row, a source and a checksum.

## Governance

```fibber fitness
(defun tab-split (s: str) -> (Vec str)
  (loop ((i 0) (out []))
    (match (str-find s "\t" i)
      (nil (conj out (str-slice s i (str-len s))))
      ((some k) (recur (+ k 1) (conj out (str-slice s i k)))))))

;; The data lines of the baseline as (line, fields).
(defun baseline-lines (repo: Repo) -> (Vec NumLine)
  (filterv (fn (n: NumLine) (and (not (starts-with? (. n text) "#")) (not (= (. n text) ""))))
           (numbered (unwrap-or (file-text repo "scripts/bench/baseline.tsv") ""))))

(defun hex-char? (b: i64) -> bool (or (and (>= b 48) (<= b 57)) (and (>= b 97) (<= b 102))))

(defun hex12? (s: str) -> bool
  (and (= (str-len s) 12)
       (loop ((i 0)) (cond (>= i 12) true (hex-char? (sext i64 (str-byte-at s i))) (recur (+ i 1)) :else false))))

(defun row-faults (n: NumLine) -> (Vec Finding)
  (let ((fs (tab-split (. n text))))
    (cond (not (= (count fs) 3)) [(Finding "scripts/bench/baseline.tsv" (. n n) "is not name, seconds and checksum separated by tabs")]
          (not (hex12? (nth fs 2))) [(Finding "scripts/bench/baseline.tsv" (. n n) (str (nth fs 0) ": the checksum is not 12 hex digits (" (nth fs 2) ")"))]
          (not (is-number? (nth fs 1))) [(Finding "scripts/bench/baseline.tsv" (. n n) (str (nth fs 0) ": the median is not a number"))]
          :else [])))

(defun is-number? (s: str) -> bool (some? (parse-decimal s)))

(defun names-of (repo: Repo) -> (Vec str) (mapv (fn (n: NumLine) (nth (tab-split (. n text)) 0)) (baseline-lines repo)))

(rule "every row of the baseline is name, seconds and a 12-hex checksum, and no name is twice"
  (into (reduce (fn (acc: (Vec Finding) n: NumLine) (into acc (row-faults n))) [] (baseline-lines repo))
        (mapv (fn (n: str) (Finding "scripts/bench/baseline.tsv" 0 (str n " is in the baseline twice")))
              (filterv (fn (n: str) (> (count (filterv (fn (m: str) (= m n)) (names-of repo))) 1)) (names-of repo))))
  (plant "scripts/bench/baseline.tsv" "\nvec-index\t0.70\tUNSTABLE\n")
  (plant "scripts/bench/baseline.tsv" "\nzz-new\t0.50\t\n"))

(rule "every name of the baseline is a program of scripts/bench, and there are at least ten rows"
  (into (missing repo (mapv (fn (n: str) (str "scripts/bench/" n ".fib")) (names-of repo)))
        (if (< (count (names-of repo)) 10) [(Finding "scripts/bench/baseline.tsv" 0 "fewer than 10 rows")] []))
  (plant "scripts/bench/baseline.tsv" "\nzz-no-such-bench\t0.50\tc5bfa6762bdb\n")
  (plant-remove "scripts/bench/baseline.tsv"))

(rule "quick.sh decides a wrong answer before the timing and fails on it, and flags an unstable output"
  (into (must-contain repo "scripts/bench/quick.sh" "if [ \"$sum\" != \"$bs\" ]; then v=\"WRONG ANSWER (checksum $sum, baseline $bs)\"; bad=1")
        (into (must-contain repo "scripts/bench/quick.sh" "sum=UNSTABLE")
              (into (must-contain repo "scripts/bench/quick.sh" "s=$(sha1sum < \"$work/stdout\" | cut -c1-12)")
                    (must-contain repo "scripts/bench/quick.sh" "[ \"$bad\" -ne 0 ] && { echo \"BENCH FAIL: a wrong answer or a build failure\"; exit 1; }"))))
  (plant-file "scripts/bench/quick.sh" "#!/bin/bash\n")
  (plant-remove "scripts/bench/quick.sh"))
```

### What this does not check

That the recorded checksums are the right answers (they are what the tree printed when recorded); that `quick.sh` behaves as its text says
(it takes minutes and a quiet machine: the performance cycle runs it, the gate does not); the other benchmarks of `scripts/bench`
(`run.sh`, the shootout) that have no checksum column; timings (the 10% threshold is the script's, not a rule here).
