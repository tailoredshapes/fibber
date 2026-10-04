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

What the shootout programs needed and the language or library did not have. One section per gap, appended by
each benchmark's agent.

## n-body: `sqrt` is not a builtin (stdlib §4.10, §7 L11)

**Missing.** `(math/sqrt x)`: the spec makes it a builtin of `fib.math`; neither the compiler nor the library has it
(`ROADMAP.md` "Performance" item 6: "`sqrt` is not landed (nbody had to avoid it)").

**Reproduction.** `(ns main) (defun main () -> i64 (do (println (sqrt 2.0)) 0))`: `unbound name sqrt`.

**What Java and C use.** `Math.sqrt` and libm `sqrt`, which the JIT and gcc turn into one `sqrtsd`.

**What was done.** `lib/fib/math.fib` (`fib.math/sqrt`), a library function over libm's `sqrt` through an `extern` in
`lib/fib/math/libm.fib`, called inside `unsafe` in the library only (the benchmark has no `unsafe`). Case
`cases/stdlib/4204`. Libm's `sqrt` is correctly rounded, so the answer is the one a builtin gives. The cost of it is a
call per `sqrt` where Java and C inline the instruction (ten per step).

**Recommendation.** The builtin of §7 L11 (`llvm.sqrt.f64` in the emitter, `f64::sqrt` in the interpreter), then delete
`lib/fib/math.fib`'s `unsafe` and make `sqrt` the builtin: language change, compiler package.

## n-body: `format` / `%f` is not landed (stdlib §4.14)

**Missing.** `(format "%.9f" x)`: `unbound name format`. `str` of an `f64` is the shortest round-trip text (`1.0E21`),
which is not `%.9f`.

**Reproduction.** `(ns main) (defun main () -> i64 (do (println (format "%.9f" 0.5)) 0))`.

**What Java and C use.** `String.format("%.9f", x)` and `printf("%.9f", x)`.

**What was done.** `lib/fib/fmt.fib` (`fib.fmt/format-f64 x digits`), over glibc's `strfromd` (a fixed-arity
`snprintf` of one double: an `extern` declared with `:varargs` and called with the extra argument is rejected,
`snprintf takes 3 argument(s), got 4`, and the runtime already declares `snprintf` with another signature). Case
`cases/stdlib/4205`. It rounds the exact binary value as C does; Java rounds the shortest decimal text half up, so
they can differ on a value whose shortest text ends in a 5 exactly at the last place: the md5 of the three programs
agree at every size recorded.

**Recommendation.** The `%f` directive with precision in the `format` macro (§4.14, Q29), written in fibber without
libc (exact decimal expansion of the double with integer arithmetic) so the interpreter and the compiled program agree
on every platform.

## n-body: a read through a cell retains and releases the array (finding for the compiler package, not a gap)

**Missing.** `MArray`, `double-array`, `aget`, `aset` (stdlib §2.11, tranche 3) are not in the released library
(`unknown type MArray`). The program holds its 35 doubles in an `(Array f64)` inside a `cell` and writes with
`array-set!` through `&` parameters, the in-place path of the prelude itself.

**Evidence of the cost.** `objdump -d` of the compiled program shows, around every `(array-get @a i)`, a retain
(`incq (%r15)` behind two flag tests) and a release (`dec`) of the array's count, and around every `array-set!` a
uniqueness test then the store. About 17 reads per pair of bodies, ten pairs per step. The functional form
(`array-with`, the array a loop variable) is worse, 1.0 s per million steps against 0.39 s: `explain` shows the
parameter `borrowed`, so every `array-with` copies (`fib.array-slice`), and `:owned` is refused on a `defun`
parameter (`:owned is not a parameter`). The version kept reads through a borrowed parameter in a kernel that returns
the new values in a struct, then writes through `&a`: 0.30 s per million steps.

**What Java and C use.** `double[]` or fields: a load.

**Recommendation.** (1) Do not retain around a read of `@cell` when the result is a scalar (a borrow for the length of
the load); (2) accept `:owned` on `defun` parameters so `array-with` in a loop is in place; (3) land `MArray`.
Compiler and library packages.

## Tooling note: the Rust `fibref` of the tree does not read the tree's library

`cargo build -p fibref` at this commit builds a `fibref` whose embedded prelude lacks `array-push!`, `array-take!`:
`lib/prelude.fib:94:19: unbound name array-push!`. After the flip the interpreter side of "both tools" is no longer
runnable from this tree; the cases 4204 and 4205 were run with the compiled tool only.
