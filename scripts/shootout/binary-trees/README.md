# binary-trees

The Computer Language Benchmarks Game's binary-trees. Depth N: build and check one stretch tree of depth N+1 (then drop
it), build a long-lived tree of depth N, then for each depth d = 4, 6, .. N build and check `2^(N-d+4)` trees of depth d
(each dropped after its check), and finally check the long-lived tree. Output is the game's text, tab-separated.

Sizes: small 16, full 21 (`sizes.txt`; md5 of stdout). The full output at 21 begins `stretch tree of depth 22  check:
8388607` and ends `long lived tree of depth 21  check: 4194303`, the game's published values.

## Rules followed

- Every language allocates every node (no pooling, no arena): Java `new`, C `malloc`/`free` (one `free` walk per tree),
  Clojure `deftype`, fibber a `defstruct`. Fibber and C free explicitly or by ownership; Java and Clojure by the GC.
- Fibber: `(defstruct Node (l: (Option Node) r: (Option Node)))`, a leaf has `nil` children as Java's has `null`: the
  same number of allocations in all four. No `unsafe`, no raw pointers. N comes from `(args)`.
- Java: a final `Node` with two fields, recursion, default JVM flags. The class is `binarytrees` (a hyphen is not legal
  in a Java identifier) inside `binary-trees.java`; it is not public so javac accepts it. Run `java -cp DIR binarytrees N`.
- C: `gcc -O3 -march=native`, glibc malloc.
- Clojure: `deftype Node [l r]`, hints on the uses (`^Node (.l t)`; a deftype field cannot be hinted with its own
  class), `^long` primitive functions, checked arithmetic (the values stay far below 2^63), zero reflection warnings
  (`*warn-on-reflection*` is on; stderr is empty but for the in-program seconds). Run `clojure.main binary-trees.clj N`.
  The in-program seconds of the work are printed to stderr.

## Variants

`binary-trees-enum.fib` is the first, straightforward version: `(defenum Tree (Leaf) (Node l: Tree r: Tree))`, a leaf is
`(Node Leaf Leaf)`. Same output. At 21 it took 13.3 s and 1.05 GB (one run) against 7.2-8.8 s and 0.39 GB for the Option
version, which is consistent with each `Leaf` being a heap allocation (docs/shootout/gaps.md, "nullary enum variants").
The program of record is the Option version, which is also the exact structural twin of the Java one.

## Gaps hit

See docs/shootout/gaps.md: nullary variants allocated. Nothing was missing to write the program.
