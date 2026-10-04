# The language shootout

The ten programs of the Computer Language Benchmarks Game, written in fibber, Java, Clojure and C, timed
under one protocol. The report is [docs/shootout.md](../../docs/shootout.md).

```
FIBC=/path/to/bin/fibc scripts/shootout/run.sh [NAME..] [--size small|full|small,full] [--langs fib,java,clj,c]
                                                [-n RUNS] [--out FILE]
scripts/shootout/run.sh --render rows.tsv       # print the table of an earlier run
scripts/shootout/report.sh [run.sh arguments]   # run, then rewrite the results section of docs/shootout.md
```

Needs `/usr/bin/time`, `flock`, `javac`/`java` (JDK), `gcc`, the Clojure 1.12 jars (`SHOOT_CLJ`, default
`~/.cache/fibber-scratch/B4-D/clj`: `clojure-1.12.0.jar`, `core.specs.alpha-0.4.74.jar`, `spec.alpha-0.5.238.jar`)
and a `fibc` (`FIBC`, else `bin/fibc`, else the one on `PATH`; `scripts/fetch-seed.sh DIR` unpacks the seed
release; `FIB_LIB` defaults to this tree's `lib/`, and a seed needs `LD_LIBRARY_PATH=<seed>/lib`).

## A benchmark directory

`scripts/shootout/<name>/` holds `<name>.fib`, `<name>.java` (public class `<name>`, `main(String[])` reads N from
`args[0]`), `<name>.clj` (first form `(ns <ns>)`, with `(defn -main [& args] ..)`), `<name>.c` (reads N from
`argv[1]`), `sizes.txt` and `README.md`. `run.sh` runs every directory that has a `sizes.txt`; names that start
with `_` are the self-tests and run only when named. All four programs get N as their only argument.

`sizes.txt`: one line per size, the size word first, then `N <number>` and the md5 of the correct stdout, in
that order or any other (tokens are told apart: 32 hex digits is the md5, digits are N):

```
small N 1000 0123456789abcdef0123456789abcdef
full N 25000000 fedcba9876543210fedcba9876543210
```

A line `stdin fasta` marks a benchmark that reads the fasta output on stdin (`k-nucleotide`, `reverse-complement`
and `regex-redux` are always treated so). Its N is then the fasta size: `run.sh` generates the input with the
Java fasta twin (`scripts/shootout/fasta/fasta.java`, `java fasta N`) once, caches it as
`$SHOOT_SCRATCH/input/fasta-N.txt` with its md5 beside it (`.md5`), compares that md5 with the one in the fasta
directory's own `sizes.txt` when that has the same N, and feeds it to every language on stdin. The benchmark is
still given N as `args[0]`; it may ignore it.

## The protocol

- Every timed run takes `flock /tmp/fibsuite.lock` (the machine is shared; `SHOOT_NOLOCK=1` turns it off), so
  timing runs never overlap. Builds are not locked.
- `/usr/bin/time -f '%e %U %M'`: elapsed wall seconds (10 ms resolution), user CPU seconds, peak RSS in KB. The
  median run by elapsed time is reported, with its own user time and RSS: 5 runs at `small`, 3 at `full`
  (`-n` overrides). A run limit of `SHOOT_TIMEOUT` seconds (default 3600) applies.
- Fibber runs under `ulimit -v 16000000`. Java and Clojure run with default flags (`SHOOT_JAVA_FLAGS` adds the
  same flags to both); C is `gcc -O3 -march=native`; fibber is `fibc build X.fib -I lib`.
- Every run's stdout is hashed and compared with the md5 in `sizes.txt`. A mismatch, a non-zero exit or a missing
  md5 makes the row FAIL: the table shows FAIL and no number, `run.sh` exits 1. A missing source or a failed build
  is `n/a` (not a failure of the run), with the reason in the TSV.
- Baselines: hello world in each language (fibber `scripts/bench/hello.fib`, a Java `Hello`, a C `puts`, and
  `clojure.main -e nil` for Clojure, which includes loading `clojure.core`), same protocol, in the `_hello` rows.

## Clojure's two numbers

The Clojure column is `wall (in-program)`. Wall is the whole process, including the JVM start-up and loading
Clojure, as the Benchmarks Game reports it; the ratio column `fib/clj (wall)` uses it. The in-program time is the
computation proper: `-main` prints a line to **stderr** whose last number is the seconds, taken with
`System/nanoTime` around the work only, for example

```clojure
(binding [*out* *err*] (println (str "clj-compute-s " (/ (double (- (System/nanoTime) t0)) 1e9))))
```

`run.sh` takes the last number on the last stderr line that has a digit (seconds; `ms` right after it means
milliseconds). `fib/clj (in-prog)` divides fibber's elapsed time by that. A `-main` that prints no such line
gets an empty in-program cell. Reflection or boxed-math warnings on stderr are noted in the TSV. The program is
run as `java -cp JARS:DIR clojure.main -m <ns> N`, with the `.clj` copied to the path `<ns>` implies (`-` becomes
`_`).

## Output

Table columns: elapsed / user / RSS MB for each language, then fibber/Java, fibber/Clojure (wall),
fibber/Clojure (in-program), fibber/C elapsed ratios (below 1 means fibber is faster). Raw rows, tab separated,
in `--out` (default `$SHOOT_SCRATCH/rows.tsv`, `SHOOT_SCRATCH` defaults to `~/.cache/fibber-shootout`):
`bench size n lang status elapsed_s user_s rss_kb prog_s md5 note`, status `ok`, `FAIL` or `n/a`.

## The self-tests

`_selftest` (a loop and a checksum) and `_selftest_stdin` (counts the bytes on stdin) are the stand-ins that
exercise the runner in all four languages. `_selftest_bad` is `_selftest` with the Java twin planted to print a
wrong number: it must come out as a FAIL row for Java and make `run.sh` exit 1:

```
scripts/shootout/run.sh _selftest _selftest_bad _selftest_stdin --size small,full -n 3
```

`_selftest_stdin` needs a fasta twin to generate its input: point `SHOOT_FASTA_DIR` at any directory holding a
`fasta.java` (class `fasta`, prints N bytes) until the real one lands.
