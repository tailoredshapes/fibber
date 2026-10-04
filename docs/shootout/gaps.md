# Shootout gaps

One section per gap found while writing the language-shootout programs.

## binary-trees: nullary enum variants appear to be heap-allocated

**What is missing.** A value of a nullary variant (`Leaf` of `(defenum Tree (Leaf) (Node l: Tree r: Tree))`) should need
no allocation (an immediate tag or a static singleton); the evidence says it costs one.

**Reproduction** (seed fibc 0.1.4, depth-18 tree, one build and one check, `/usr/bin/time -f '%e %U %M'`):

```clojure
;; enum version                                   ;; Option version
(defenum Tree (Leaf) (Node l: Tree r: Tree))      (defstruct Node (l: (Option Node) r: (Option Node)))
(defun make (d: i64) -> Tree                      (defun make (d: i64) -> Node
  (if (= d 0) (Node Leaf Leaf)                      (if (= d 0) (Node nil nil)
      (Node (make (- d 1)) (make (- d 1)))))            (Node (some (make (- d 1))) (some (make (- d 1))))))
```

Both build 524287 nodes and print the same count: enum 0.05 s elapsed, 66904 KB peak; Option 0.02 s, 26112 KB. At depth 21
(the full benchmark) the enum program took 13.3 s and 1.05 GB, the Option one 7.2-8.8 s and 0.39 GB. Half the nodes of a
complete tree are bottom nodes, each with two `Leaf` children, so about 2.0x the allocations of the Option version; the
memory ratio (2.6x) and time ratio (1.8-2.5x) match. I did not read the emitted lIR to confirm; the numbers are the
evidence.

**What Java/C use.** `null` / `NULL` children: no allocation for a leaf.

**Recommendation.** Compiler/runtime: represent a nullary variant of an enum with fields in other variants as a static
immortal object (or a tagged immediate pointer), never retained or released. Until then the idiomatic fibber program
for a tree is `(Option Node)` children, which is what binary-trees.fib uses; `binary-trees-enum.fib` keeps the
straightforward version for the comparison.

## binary-trees: a Java class cannot be named `binary-trees`

Not a fibber gap: the suite layout asks for a public class `<name>`, and `binary-trees` and `fannkuch-redux` are not
legal Java identifiers. binary-trees.java holds a non-public class `binarytrees`, run as `java -cp DIR binarytrees N`.
INFRA's run.sh needs a per-benchmark class name (suggestion: the directory name with `-` removed).

## binary-trees: Clojure deftype fields cannot be hinted with the type's own class

`(deftype Node [^Node l ^Node r])` fails with `ClassNotFoundException`; the hint goes on the use, `(check ^Node (.l t))`.
Not a fibber gap; recorded because it is the idiom a reader will look for.
