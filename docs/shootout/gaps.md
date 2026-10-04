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

# Gaps the shootout programs hit

One section per gap: what is missing, the smallest reproduction with the exact error, what the Java/C version uses, the
recommendation. (Appended by each benchmark's agent; this part is BIG, the BigInt library and pidigits.)

## A symbol cannot end in `'`: `+'`, `inc'` and the rest of Clojure's auto-promoting names cannot be written

Missing: the names `+'` `-'` `*'` `inc'` `dec'` of spec/stdlib.md §4.9 (tranche 5, the BigInt rows). The reader ends a symbol at
`'` (spec/syntax.md §1.1: a symbol is a maximal run of characters that are not whitespace, `( ) [ ] { } " ; ' ` , @ ~ \`), so
`+'` reads as the symbol `+` and then a quote.

Reproduction (`q1.fib`):

```clojure
(ns main)
(defun inc' (a: i64) -> i64 (+ a 1))
(defun main () -> i64 (inc' 4))
```

```
rejected:
q1.fib:2:13: (a: i64) is not a parameter
```

Clojure and Java: Clojure's `+'` is an ordinary symbol (the reader's quote is a macro character only at the start of a
token); Java has no counterpart (`BigInteger.add`). The Clojure pidigits entry in this suite uses interop and does not need them.

Recommendation: a language change in the reader: a `'` inside a symbol (not at its start) is a constituent, as Clojure's.
The Rust lexer, `compiler/syntax/lexer.fib` and spec/syntax.md §1.1 change together (mirror-pending). Until then the library
offers `bigint` and `(+ (bigint a) (bigint b))`, and the rows keep tranche 5. Auto-promotion (a `Long` when it fits) stays a
deviation either way (§5 T5).

## No widening multiply, no wrapping arithmetic, no `clz`: the BigInt limb is 31 bits in an `i64`

Missing: an unsigned 64 by 64 to 128 bit multiply (`mulhi`, as Rust's `u64::widening_mul` or C's `__int128`), the wrapping
`unchecked-add` family of spec §4.3 (tranche 3, "needs a new builtin L10"), and a count-leading-zeros primitive.

Effect: arithmetic traps on overflow, so a limb product must fit a signed `i64`. `lib/fib/bigint` therefore uses 31-bit limbs
((2^31-1)^2 + 2(2^31-1) < 2^62): 3% more limbs than 32-bit ones (`BigInteger` has 32-bit limbs in `int`s with `long` products, so
the two are alike here) and four times as many limb products as GMP's 64-bit limbs. This is the main reason the library cannot
approach the C column: its limb loops cost 1 to 6 ns per limb where GMP's cost a fraction of one. The bit length of a limb
is `popcount` of the limb with its top bit smeared down (5 shifts), where Java uses `Integer.numberOfLeadingZeros`.

Reproduction: `(* 4294967295 4294967295)` is `trap: integer overflow in * at i64` for a 32-bit limb product with a carry added.

Recommendation: library or builtin: `(mul-hi a b)` and `(mul-lo a b)` (or a `i128` type), `clz`, `ctz`, and the `unchecked-*` family;
then the limb can be 64 bits and the inner loops are those of GMP's `mpn_mul_1`/`mpn_addmul_1`.

## An owned `(Array i64)` parameter cannot be written in place by the callee

Missing: a way for a library function to take an array it may overwrite when the caller passes the only reference
(spec/types §2.5: the count of a callee's owned parameter is two, so `array-set!` on a parameter copies, as the comment on the
prelude's `vnodes-up` says). `lib/fib/bigint` builds each result in a `(cell (array n 0))` it owns (`&c` in/out) and trims a high
zero limb with `array-pop!`; the operands are never reused, so `numer = numer * 10` in the spigot allocates a fresh array each time.
Java's `BigInteger` is immutable too (and allocates as well); GMP's `mpz_mul_ui(numer, numer, 10)` is in place.

Recommendation: language change or library idiom: a consuming `(mul-small! &a m)` over an `&a: (Array i64)` in/out parameter works
today for the library's own loops (the in/out parameter is the callee's), but a program that holds a `BigInt` struct cannot call
it without a mutable field. A `BigInt` that is a record of one `(Cell (Array i64))` would, at the price of a mutable type in the API.
Not done: it breaks the value semantics Clojure programs expect.

## The runtime's allocator trims the heap on every free of a large array

Found by `strace -c ./pidigits-fib 5000`: 32276 `brk` calls (98.96% of the system calls) and 1.7 s of system time at N=10000
(user 4.8 s), because each BigInt operation allocates and frees an array of 60 to 120 KB, and glibc's `free` of the top chunk
returns the pages to the kernel (`M_TRIM_THRESHOLD`), which `malloc` then faults in again. With
`MALLOC_TRIM_THRESHOLD_=4000000000 MALLOC_TOP_PAD_=268435456 MALLOC_MMAP_THRESHOLD_=4000000000` the same run (N=5000) goes from
1.34 s elapsed (0.18 s system) to 1.00 s (0.00 s system).

Recommendation: runtime change (`fib.rt`, `rt/*.lir`): call `mallopt(M_TRIM_THRESHOLD, ...)` and `mallopt(M_MMAP_THRESHOLD, ...)` at start
(Java's and GMP's allocators do not hand pages back on each free), or a size-class free list for arrays. Every benchmark that
allocates and frees mid-size arrays in a loop pays this.

One section per gap found while writing the shootout programs: what is missing, the smallest reproduction, what the
Java/C version uses, and a recommendation.

## fannkuch-redux: no mutable array that a callee can write without a copy

**Missing.** `(MArray t)`, `long-array`, `aget`, `aset` (spec/stdlib.md §2.11, rows `aget aset alength long-array`) are specified
but not in the library or the v0.1.4 seed:

```
$ fibc build t.fib -I lib      ; (defun f (p: (MArray i64)) -> i64 (aget p 0))  (defun main () -> i64 (f (long-array 4)))
rejected:
t.fib:1:15: unknown type MArray
t.fib:2:38: unbound name long-array
```

The program therefore uses the builtin value form: `(cell (array n 0))`, `@c`, `(array-get @c i)`, `(array-set! &c i x)`, and
passes the arrays to helpers as `&p: (Array i64)` in/out parameters. That is legal safe fibber, but see the next gap.

**Java/C use** `int[]` / `int a[32]`, written in place from any callee.

**Recommendation.** Library: land `MArray` with `long-array aget aset alength` as specified (the benchmark would then read like
the Clojure twin).

## fannkuch-redux: forwarding an `&` parameter to a callee copies the array on every call

**Reproduction.** `(defun rev (&p: (Array i64) k: i64) -> unit ...(array-set! &p ...))` called from
`(defun flips (&p: (Array i64)) ...  (rev &p k) ...)`. `fibc explain` says `@20:17 (reverse-prefix ..) call &p: acquire`
(whereas a call from a function that owns the cell says `&p: move in`, cases/ownership/261). `acquire` retains the array for
the callee's private cell, so the callee's first `array-set!` finds it shared and copies it (`fib.array-slice`, `fib.array-alloc`
in the profile: 30% of the run at N=10, 0.55 s against 0.39 s after inlining the callee by hand).

**Java/C use** a plain array reference; no copy.

**Recommendation.** Language/compiler change: treat `&p` of an `&` parameter like `&c` of a cell (move the content in, store the
result back) when nothing else can read `p` during the call. Until then the benchmark inlines the loop (a local rewrite).

What the shootout programs needed and the language or library did not have. One section per gap, appended by
each benchmark's agent. (Each benchmark worktree started this file on its own: merge the sections.)

## spectral-norm: `MArray` (`double-array`, `aget`, `aset`) is not in the library

**Missing.** `unknown type MArray` (stdlib §2.11, tranche 3, specified, not landed).

**What Java and C use.** `double[]`.

**What was done.** Two versions. `spectral-norm.fib` keeps its vectors as `(Array f64)` (read with `array-get`) and
builds each product in a `(Cell (Array f64))` written in place by `array-set!` through `&`: safe, no `unsafe`.
`spectral-norm-vec.fib` is the same with `(Vec f64)`, `nth` and `assoc`, the Clojure-shaped way. The Vec version is
about 5 times slower (the profile is in README.md): `nth` on a `Vec` is a call that walks the trie.

**Recommendation.** Land `MArray`; then `aget` on it should compile to the same load as `array-get`. Independently,
`nth` on a `(Vec a)` with a known-small count (the tail only, count <= 32) or a hoisted loop-invariant `v` would
close the gap for programs that use persistent vectors for numerics.

## spectral-norm: `sqrt` is not a builtin

**Missing.** `(math/sqrt x)`: `unbound name math/sqrt` (stdlib §4.16 says builtin; not landed).

**What Java and C use.** `Math.sqrt` / libm `sqrt`.

**What was done.** `sqrt-newton` in the program: 60 Newton steps from 1.0 on the final ratio, one call, so the time
is not distorted. It converges to within an ulp, far inside the 9 printed decimals; the three-way md5 agreement
at both sizes is the check.

**Recommendation.** The builtin (`llvm.sqrt.f64`), as the n-body section of this file (when merged) says.

## spectral-norm: `format` / `printf` with `%.9f` is not landed

**Missing.** `unbound name printf`; `str` of an `f64` is the shortest round-trip text.

**Reproduction.** `(ns main) (defun main () -> i64 (do (printf "%.9f\n" 0.5) 0))`: `unbound name printf`.

**What Java and C use.** `String.format("%.9f", x)` / `printf("%.9f\n", x)`.

**What was done.** `fmt9` in the program: rounds the shortest-repr text of a value in [1, 10) to 9 decimals, half
up, as Java does; it runs once. It is a local string function, not a library one: a general `format-f64` belongs in
the library (see n-body's section when merged, `fib.fmt/format-f64`).

**Recommendation.** The `%f` directive of the `format` macro with precision, written in fibber.

## spectral-norm: integer `/` yields a Ratio, so `(i+j)(i+j+1)/2` needs `quot`

Not a gap, a note for ports: `(/ a b)` on `i64` is a `(Ratio i64)`; `quot` (or `shr` for a power of two) is the
integer division Java's `/` is.
