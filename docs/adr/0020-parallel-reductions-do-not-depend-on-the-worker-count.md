# 0020. Parallel reductions do not depend on the worker count

Status: accepted
Date: 2026-10-06
Source: docs/design/parallelism.md 3.5 (P-det); docs/design/decisions-2026-10-04.md, "Parallelism" ("a fixed-tree deterministic reduction as the DEFAULT
(identical bits for any worker count, measured free)"); lib/fib/parallel/reduce.fib; cases 7609 to 7611; scripts/mutant-parallel.sh.

## Context

A floating point sum depends on the order of its additions. A parallel reduction that splits the work by the number of workers and
combines in whatever order is cheapest gives different bits on a laptop with 8 cores and a server with 64, and a different answer for
the same program on the same machine when the quota changes. Python's tools ship that as normal. The owner's line was "best in class
parallelism", and the decision was that the **default** reduction is deterministic: the indices are cut into chunks of a fixed grain,
each chunk is reduced by the sequential kernel, and the chunk results are combined in a fixed balanced pairwise tree. Neither the grain nor the
tree depends on the number of workers, and the sequential path runs the same tree. The faster order-free mode is opt in (`fast`).

## Decision

1. The default grain of a reduction, `reduce-grain` in `fib.parallel.chunks`, is a constant: its form mentions neither the number of
   workers, nor `cpu-count`, nor a `Par`.
2. `par` (the default `Par`) has `fast` false; `pfold-chunks` combines with the fixed tree unless `fast`, and `fold-grain` falls back to `(reduce-grain)`.
3. The cases that pin the property stay: identical bits for 1 to 28 workers (7609), the tree's shape (7610), the scan (7611), and the
   mutant script that breaks the library two ways (`reduce-grain-by-workers`: a grain taken from the worker count; `reduce-left-combine`: a left-to-right combine).

## Consequences

- A change to the default grain that reads the machine fails `fibc adr` before it reaches a case.
- Choosing speed over bits is explicit: `(with (par) (fast true))`, a `Par` with `fast` true.
- `pmap` (an order-preserving map) is not a reduction and may chunk by the worker count; only the combine of a reduction must not.

## Governance

```fibber fitness
(defun grain-forms (repo: Repo) -> (Vec Top)
  (reduce (fn (acc: (Vec Top) f: SrcFile) (into acc (filterv (fn (t: Top) (= (. t name) "reduce-grain")) (. f tops))))
          [] (select repo ["lib/fib/parallel/chunks.fib"])))

(defun worker-atoms () -> (Vec str) ["workers" "cpu-count" "Par" "par" "online-cpus" "available-cpus" "max-workers" "grain"])

(rule "reduce-grain is a constant: it mentions no worker count"
  (into (if (empty? (grain-forms repo)) [(Finding "lib/fib/parallel/chunks.fib" 0 "no reduce-grain")] [])
        (reduce (fn (acc: (Vec Finding) t: Top)
                  (let ((as (mapv atom-text (filterv (fn (a: Node) (not (string-atom? a))) (atoms-of (. t node))))))
                    (into acc (mapv (fn (w: str) (Finding "lib/fib/parallel/chunks.fib" (. t line) (str "reduce-grain mentions " w)))
                                    (filterv (fn (w: str) (member? as w)) (worker-atoms))))))
                [] (grain-forms repo)))
  (plant "lib/fib/parallel/chunks.fib" "\n(defun reduce-grain () -> i64 (* 8192 (cpu-count)))\n")
  (plant-remove "lib/fib/parallel/chunks.fib"))

(rule "the default Par is not fast, the default combine is the tree, the default fold grain is reduce-grain"
  (into (must-contain repo "lib/fib/parallel/chunks.fib" "(defun par () -> Par (Par (cpu-count) 0 false))")
        (into (must-contain repo "lib/fib/parallel/reduce.fib" "(if (. p fast) (left-combine combine init parts) (tree-combine combine init parts))")
              (must-contain repo "lib/fib/parallel/reduce.fib" ":else (reduce-grain)")))
  (plant-file "lib/fib/parallel/chunks.fib" "(ns fib.parallel.chunks)\n(defun par () -> Par (Par (cpu-count) 0 true))\n")
  (plant-file "lib/fib/parallel/reduce.fib" "(ns fib.parallel.reduce)\n"))

(rule "the cases that pin it are in the tree with their worker counts, and the mutant script is too"
  (into (must-contain repo "cases/stdlib/7609-parallel-preduce-of-f64-has-identical-bits-for-every-worker-count.fib" "[1 2 3 4 7 8 16 28]")
        (into (must-contain repo "cases/stdlib/7610-parallel-fold-combines-chunk-results-in-a-fixed-balanced-pairwise-tree.fib" "grain")
              (into (missing repo ["cases/stdlib/7611-parallel-pscan-is-the-prefix-fold-and-the-same-for-every-worker-count.fib"])
                    (into (must-contain repo "scripts/mutant-parallel.sh" "reduce-grain-by-workers)") (must-contain repo "scripts/mutant-parallel.sh" "reduce-left-combine)")))))
  (plant-file "cases/stdlib/7609-parallel-preduce-of-f64-has-identical-bits-for-every-worker-count.fib" "(ns main)\n")
  (plant-remove "scripts/mutant-parallel.sh"))
```

### What this does not check

That the reduction really gives the same bits (cases 7609 to 7611 run in the gate, and the mutant script, which is run by hand, breaks it two
ways); that `pmap` and `pfor` keep order and run every index once (cases 7603 to 7605); that tensor kernels, which parallelise
themselves above a size threshold, reduce deterministically (their own cases); refcount contention and the races TSAN found
(`scripts/tsan.sh`); the rest of the parallelism decisions (tiers, a pool, channels), which are design, not yet built.
