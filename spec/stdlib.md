# The standard library (M7)

Status: **Proposed**. The owner asked on 2026-10-01 for a library that "steals Clojure's, or as
close to it", improves its inconsistencies freely, is as ergonomic as Clojure and has the run-time
performance of Rust, and said that no benchmarking or optimisation comes before the library is
viable (ROADMAP M7). This page is the design: the abstractions (§2), the naming rules (§3), every
name of Clojure's core, set, string, walk, data and math namespaces with its verdict and its
fibber signature (§4), the deviations (§5), where the code lives (§6), the language and compiler
changes it needs (§7), the order of work and how each step is judged (§8), and what the owner
still has to decide, each with a recommendation (§9), and the review record (§10). Nothing here is **Decided**
until the owner signs it; method.md applies throughout: a function is done when a test that can fail says so.

How this page was made. One agent inventoried Clojure (673 names, 71 sharp edges); three wrote
independent designs (zero-cost first, Clojure fidelity first, type-system realism first); a fifth judged
them (the scoring is in the design record, `synth-notes.md`) and wrote this page. Three critics
(ergonomics, compilability, consistency) then checked it and raised 66 findings; a further agent revised
the page, and §10 records every finding with its verdict and what changed. A claim about what the
compiler does cites an item of Appendix A, **[R]** where it matters: a program that was run with `fibc`
and `fibref` in the session that wrote this page or in the review (A10), its output quoted. A claim that is a
design record's and was not re-run, or a prediction, says so (**[H]** for a hypothesis). Nothing was measured
for run-time speed; the only such numbers are counts of heap objects, indirect calls and count operations, which
are properties of the design that can be read off `FIB_TRACE=1` and `fibc emit` (ROADMAP M7 rule 5 allows no
timing yet); the one timing on the page is the cost of compiling, which §6.4 needs.

## 1. Principles

The owner's aim, and the rules of ROADMAP §M7, as this page applies them:

| # | Principle | What it means for a function |
|---|---|---|
| P1 | Clojure's names, shapes and argument order first | A name keeps Clojure's meaning unless §5 says otherwise and gives the reason. Sequence functions take the sequence last, collection functions take the collection first (§3). |
| P2 | Zero-cost by construction | Everything generic is monomorphised and every protocol call is static (compiler.md §7). A sequence function returns a small struct that remembers its source and its function (a *recipe*), never a lazy cell and never a copy; an adaptor allocates nothing per element except what it hands on (until C5 a struct or an `(Option scalar)` is a heap object: `zip`'s `Pair`, `keep`'s `Option`, a `Map` walk's `Pair`, [R] A10 alloc), and only the materialisers (`vec`, `into`, `set`, `sort`, `group-by`) allocate per element by their signature. |
| P3 | Persistent in the API, in place when unique | `conj`, `assoc`, `update` keep Clojure's value semantics. Where the ownership checker proves the collection unique they update it in place, so a loop of `assoc`s is Clojure's transient without a transient API. There is none. Whether this holds is a test (§2.5), not a belief. |
| P4 | Unboxed elements, a real hash | A `(Vec i64)` stores `i64`s. The hash that types §2.12 fixes (an integer hashes to itself, FNV-1a for text) is replaced before the HAMT's speed is judged (§9 Q15). |
| P5 | Types, not truthiness | `nil` is `Option`'s empty variant and nothing else. A function that is undefined on some input of its type returns `Option` (`first`, `get`, `peek`, `find-first`, `parse-long`, `index-of`); one whose precondition is a programmer error traps (`nth`, `pop`, `assoc` past the end), with a message and a position. There are no exceptions. |
| P6 | One way | A protocol exists where the monomorphiser can dispatch on it (§2.3); a run-time predicate (`vector?`, `seq?`, `instance?`) is a type, so it is not offered. |
| P7 | No hidden global state | No dynamic vars, no global random generator (an `Rng` value), no metadata, no top-level `atom` (§9 Q23). |
| P8 | Ships on today's compiler | Each tranche of §8 is written in the form that compiles today. The compiler changes of §7 make the same source faster or the same names terser; they do not change what a function means, except the syntax and checker items E3, E4, E5, E8, L14 and L19, which the owner signs separately. |
| P9 | Every function has an executable test | Cases run both ways (method rule 6), generated programs against a model (rule 5), differential tests against a naive reference written in fibber (§8). The compiler (M6) is the first customer: what stage 2 needs lands first. |

Two non-goals. The library does not try to be a Clojure interpreter: no `eval`, no reflection, no
arbitrary-precision numbers, no ratios, no Java interop (§4.17). It does not hide cost: `count` says
in §4 whether it is O(1) or a walk, and re-traversing a recipe re-runs it (§2.1).

## 2. The core abstractions

Code blocks are marked. `ran:` means the block (or the module it is cut from) was compiled and run
with `fibc` and with `fibref` and the audit was clean (Appendix A, A7); `proposed:` means the
compiler does not accept it today, and the error it gives is quoted where it matters.

### 2.1 Iteration: one push protocol, one pull protocol for lockstep walks

A collection is anything that can feed its elements to a function. The protocol has one required
method, `each-while`: internal iteration with early exit by a `bool`. This is `reduce` (Clojure's
`IReduce`) made the primitive, and it is what Rust calls `try_fold`. The bodies of `nth`, `last` and
`to-vec` are abbreviated here; the module that ran has them ([R] A10 nth).

```lisp
;; ran: fib.seq
(defprotocol (Reducible s e)
  (each-while (self k: (fn (e) bool) :borrow) -> bool)   ; false from k stops the walk; the result says whether it ran to the end
  (size (self) -> i64                                    ; default: a walk; O(1) where the source knows
    (let ((n (cell 0)))
      (do (each-while self (fn (x) (do (set! n (+ @n 1)) true)))
          @n)))
  (nth (self i: i64) -> e ..)                            ; default: a walk that traps past the end; O(1) on Vec Array Range Slice
  (last (self) -> (Option e) ..)                         ; default: a walk
  (to-vec (self) -> (Vec e) ..))                         ; default: a push loop; a Vec returns itself
(defenum (Step a) (More v: a) (Done v: a))                ; Clojure's `reduced`: the step of `reduce-while`
(defstruct (Pair a b) (fst: a snd: b))                    ; the tuple the library needs (§7 L3)
```

Rules:

1. **Sources** implement `Reducible` with one `impl`: `Vec`, `List`, `Array`, `Option` (zero or one
   element, so `(map inc nil)` is empty and `nil` flows through a pipeline), `Map` (yields
   `(Pair k v)`), `Set`, `Range`, `Iterate`, `Repeat`, `Cycle`, `Chars`, and every adaptor below.
   **`str` is not a source** (§2.9); `(chars s)` is.
2. **Adaptors** (`map filter remove keep take drop take-while drop-while mapcat concat
   map-indexed reductions partition interleave zip ...`) are functions that build, in O(1), a struct
   holding the source and the parameters, and implement `Reducible` over it. They are *recipes*:
   traversing one twice runs it twice, calling `f` again. Nothing runs until a consumer walks it,
   and the adaptor itself allocates nothing per element; what it hands to the next stage can be an
   object until C5 (`zip` yields a `Pair` and `keep` an `Option`: +1003 and +1002 objects over 1000
   elements against +4 for `zip-with` and +2 for `map`; `(first v)` in a loop +1000 against `(nth v 0)`
   +0; a `Map` walk +2000 in the prototype, the prelude's `Entry` and the `Pair` it is re-wrapped in,
   [R] A10 alloc).
3. **Consumers** (`reduce first last any? every? find-first count empty? into vec set sort ...`) are
   loops over `each-while`. `first`, `find-first`, `any?`, `every?`, `take`, `take-while`, `empty?`
   stop the source through the `bool`, so `(take 5 (iterate inc 0))`, `(first (filter p (iterate inc 0)))` and
   `(any? p (iterate inc 0))` terminate. Clojure's `reduced` is `(reduced x)`, a `(Done x)` that
   `reduce-while` reads; it allocates a heap enum per step until unboxed enums land (§7 C5), so the
   plain consumers use the `bool` channel.
4. **`count` is `size`**: O(1) for `Vec Map Set Array Range Option` and for size-preserving
   adaptors (`map`, `concat` of sized sources), a walk for the rest. `Range` has a closed-form size
   ([R] A10 rangesize). `(count (map f c))` does **not** call `f` (Clojure's does): a recipe runs its
   functions when a consumer walks it, and `size` is not a walk, so a pipeline that is counted and then
   summed runs `f` once or twice depending on the consumer (A10 count). `empty?` stops at the first
   element the recipe produces: O(1) on a source that holds its elements, and `(empty? (filter p c))`
   calls `p` until an element passes (100,000 calls for a predicate that never does, A10 count).
   **`nth`, `last` and `to-vec` are methods with a walking default**, so `(nth (filter p c) 3)` works in
   O(n) as Clojure's does and `(nth v 3)` is O(1); there is no `Indexable` protocol (§2.3). A generic
   `vec` cannot special-case a type, which is why `to-vec` is a method: `(vec v)` of a `Vec` is the
   identity, and `(vec (vec (vec v)))` of a 1000-element `Vec` allocates nothing beyond `v` (A10 nth).
5. **Lockstep walks** (`zip`, `zip-with`, `interleave`, three-argument `map`) need the second
   operand one element at a time, which push cannot give. Their second operand implements
   `Cursable`, whose cursor never allocates per element:

```lisp
;; ran: fib.seq.cursor
(defprotocol (Cursor c e) (advance! (self) -> bool) (current (self) -> e))
(defprotocol (Cursable s c) (cursor (self) -> c))
```

   `Vec` and `Range` are `Cursable` in the prototype; `Array`, `List` and `Chars` follow the same pattern.
   **An adaptor is not `Cursable`**, so a call works or fails by argument order: `(zip xs (map inc xs))` is
   `no implementation of Cursable for (Mapped (Vec i64) i64 i64)`, `(zip (map inc xs) xs)` runs, and
   `(zip-with (fn (b a) (- b a)) (rest xs) xs)`, the adjacent-differences idiom, runs only with the
   recipe first ([R] A10 cur). The cursor of `(Mapped c e b)` would be a struct over the cursor of `c`, and
   the impl that says so is rejected by the rule that an impl's context names only variables of its head,
   `type variable k is not a parameter of the impl head` (A10 cur-impl; types §3.3, the Paterson
   condition). §7 L16 lifts the rule for a variable that a constraint determines; then `Mapped`, `Dropped`,
   `Taken` and `Zipped` have cursors (one `advance!` each) and `Filtered` and `Mapcat` still do not (a pull
   over them must buffer). Until then the operand that is not a plain source goes first, the second is a
   `Vec`, `Range` or `Array`, or the call writes `(vec c)` for it.
6. **A user type joins with one `impl`**, and gets every adaptor and consumer: a binary search tree
   is a source in six lines (`t2.fib`, [R] A7).
7. **Transducers** are values of `(Xf a b)`, a factory of per-traversal steppers (a step function and a flush). Only the two
   element types appear in the type, so one `Xf` value serves two accumulator types (the case the
   `let`-does-not-generalise rule breaks for accumulator-typed encodings, §7 L5):

```lisp
;; ran: fib.xf
(defstruct (Stepper a b) (step: (fn (a (fn (b) bool)) bool) flush: (fn ((fn (b) bool)) bool)))
(defstruct (Xf a b) (make: (fn () (Stepper a b))))
(defun xmap (f: (fn (a) b)) -> (Xf a b) (Xf (fn () (Stepper (fn (x k) (k (f x))) (fn (k) true)))))
(defun xf-comp (x: (Xf a b) y: (Xf b c)) -> (Xf a c)           ; reads left to right, as Clojure's transducer `comp` appears to
  (Xf (fn () (let ((sx ((. x make))) (sy ((. y make))))
               (Stepper (fn (e k) ((. sx step) e (fn (b) ((. sy step) b k))))
                        (fn (k) (if ((. sx flush) (fn (b) ((. sy step) b k))) ((. sy flush) k) false)))))))
(defun transduce (xf: (Xf a b) f: (fn (r b) r) init: r c: c) :where ((Reducible c a)) -> r
  (let ((st ((. xf make))) (acc (cell init)))
    (let ((k (fn (y) (do (set! acc (f @acc y)) true))))
      (do (if (each-while c (fn (x) ((. st step) x k))) ((. st flush) k) false)
          @acc))))
(defun into-xf (to: t xf: (Xf a b) c: c) :where ((Collection t b) (Reducible c a)) -> t
  (transduce xf (fn (acc y) (conj acc y)) to c))
```

   A stepper has a **flush**, run once after the last element, because a stateful transducer holds
   elements back: `(into-xf [] (xpartition-all 2) [1 2 3 4 5])` is `[[1 2] [3 4] [5]]` and the composition
   with `xmap` on either side agrees ([R] A10 xf; without the flush the last group is lost, which the first
   version of this page did not see). `transduce` and the three-argument `into` work today (A8). `sequence` and `eduction`, which are
   recipes, do not: a stored closure that is handed a closure makes that closure escape, and an
   `each-while` whose `k` is declared `:borrow` may not let it (`implementation of
   Reducible/each-while for (Xformed c a b) makes parameter k escape; the protocol declares it
   :borrow`, A8). They wait for closure types (§7 C1), where the step is a static call. The
   function-name form is `xf`: `(xf (map f) (filter p))` composes left to right; `comp` stays
   function composition, right to left (§5 row 44).

**Why push, with a pull cursor beside it.** Three designs were measured. Pull with a mutable cursor
and `current`/`advance!` (design "fidelity") has 3 indirect calls and 3 retain pairs per element
today; push (designs "zero" and "realistic") has 6 and 6 ([R] A1, A3), and neither allocates per
element. With static closures (§2.2) the **pull** chain reaches 0 and 0 (A2), but it runs the user's
function twice for every element that survives a `filter` (a counting `map` then `filter even?` over
1000 elements calls `f` 1500 times: `filter` tests `current` and the reducer reads it again, [R] A10
d1c), which is the weakness charged to pull below. The **push** chain reaches 0 and 0 only if a protocol
method may carry its own visitor type with a bound, and no program of the review could declare that
(§2.2, A10 push): it is a prediction that needs §7 L16, not a measurement, and §9 Q14 rests on it. Push
wins on what is not a speed question: a source is one `loop` (a tree, a HAMT, `mapcat`, `cycle` are one-liners, where a cursor
needs an explicit stack); early exit is a return value; each user function runs exactly once per
element (a pull cursor needs a slot cell per adaptor to avoid re-running `f` behind a `filter`: design "fidelity" has one, and design "zero" counted 15 calls of `f` for 10 elements without it, both from the design record and not re-run here); and no
`(Option e)` is made per element, which is a heap object when `e` is a scalar ([R] A5). Pull wins
where two sources advance together, so that is the one place it is offered. A pull `seq` of
`(Option cursor)` would be free for object-typed cursors (a null pointer), but the recursion idiom
it serves (`(rest s)` as a loop variable) needs an unbounded type for a recipe; recursion walks a
`List`, a `Slice`, or uses `loop`. The idiom is not merely slow but a hazard: a function that recurses
on `(rest c)` for any `Reducible c` calls itself at `(Dropped c e)`, then at `(Dropped (Dropped c e) e)`,
without end (polymorphic recursion): `fibc` exhausts memory (`memory allocation of 577136 bytes failed`
under `ulimit -v 4000000`) and `fibref` returns 3 ([R] A10 poly), a stage-1 divergence that §7 B4 closes
by making both tools reject it with a message that names the recursion.

**Materialising** is explicit and the only per-element cost: `vec`, `into`, `set`, `zipmap`,
`sort`, `reverse`, `group-by`, `frequencies`. The bulk ones build in one function
over a growable buffer and assemble the trie once, about n/32 allocations instead of 2n (§2.5); `vec` of a
`Vec` is the identity.
Each adaptor has an eager spelling only where Clojure has one (`mapv`, `filterv`, aliases of
`(vec (map ..))`).

**Infinite and lazy.** `iterate repeat cycle` are ordinary sources that only an
early-exiting consumer ends. Memoised laziness, for a recursive definition such as the Fibonacci
numbers, is a separate library type `LSeq` (`fib.lazy`, tranche 5): a head and a `(Lazy a)` forced
once. Fibber has no GC to make a cell per element cheap, so it is not the default.

**A recipe is a type.** Each adaptor is a distinct struct, which has three consequences. (1) *Joins.*
`(if flag (filter p v) v)` is `cannot unify (Vec i64) with (Filtered (Vec i64) i64)`, and two different
adaptors in the arms of an `if` or `match` fail the same way; `cond->` over a sequence is the same case
([R] A10 join). The remedy is `(seq-of c)`, a `(dyn (Reducible e))` that erases the type at the cost of one
heap object and an indirect call per visit; it type-checks and runs (`pipeline` in A10 join), and the cost
is in the name. (2) *Threads.* A recipe holds a closure, and a closure built outside a task cannot be sent:
`value of type (fn :local (i64) i64) cannot be shared between threads: closure capture r, field f of
Mapped` (A10 col). An adaptor struct therefore carries a colour parameter for each stored closure,
`(defstruct (Mapped c e b k :colour) (src: c f: (fn k (e) b)))`, and then `(spawn (fn () (sum r)))` runs
(12 under both tools, A10 col); §4's signatures name the data parameters only. (3) *Borrows.* A recipe
stores its source, so a library function whose parameter is `:borrow` cannot pipeline it: `parameter v of g
is declared :borrow but escapes` (A10 bw). Collection parameters of library `defun`s are left to inference
(owned).

### 2.2 Closures and the higher-order story

A function argument has type `(fn (A) R)`. The compiler decides how it is called, and today the
answer is expensive; the design says what must change and what the library looks like when it does.

**Today [R] A1.** `(->> v (map f) (filter p) (reduce g 0))` over a 1000-element `Vec` allocates 4
heap objects in all (two recipe structs, two stored closures), none per element, but the loop makes
**6 indirect calls and 6 retain/release pairs per element**: a closure type has no identity, so no
specialisation can name the callee, and a call through a closure hands every argument over owned, so
the caller retains first. `fibc` also compiles at LLVM level 0 (`JitOptions::default()`), and at
`-O 2` five indirect call sites remain on the chain, besides the object drop hook (A1): LLVM cannot see through a closure object.

**The target, emulated [R] A2.** Give each closure its own type and the same chain is made of direct
calls. Written with functor structs, `reduce` and every step have **0 indirect calls, 0 retains and
0 releases**, and at `-O 2` the one indirect call left in the module is the object drop hook. That chain
is a **pull** chain (cursors), not the push design of §2.1. Written as push, with the visitor a functor and
`each-while` a method with its own type variable bounded by `Fn`, the protocol cannot be declared: a bound
on a method variable is `a method is (name (self qual* x: T qual*) -> type)`, no bound is `no
implementation of Fn1 for k; add (Fn1 k) to the :where of the impl`, and the visitor as a protocol
parameter is `type variable k is not a parameter of the impl head` ([R] A10 push). So the 0-and-0 of push
is a prediction that waits for §7 L16 (impl and method contexts that may name a determined variable);
until then the measured numbers are those of A1 and A2.

```lisp
;; proposed (notation: `args` stands for the parameter types). Today `(impl (Fn1 a b) (fn (a) b) ..)` is
;;   "an instance head is a type constructor applied to distinct variables" (A6)
(defprotocol (Fn f args r)                       ; built in: every (fn ..) literal is an instance, with its captures as fields
  (call (self args) -> r))
(defstruct (Mapped c f) (src: c f: f))           ; one more type parameter per stored closure
(defun map (f: f c: c) :where ((Fn f (e) b) (Reducible c e)) -> (Mapped c f))
```

The library is written once, with function-typed fields, in the form that compiles today. When C1
lands, each adaptor struct gains one type parameter per closure it stores and each `impl` one `Fn`
bound. A `(fn ..)` literal passed to `map` is the same text, but the rewrite is **not invisible**: it
changes where a program type-checks. Today `(reduce + 0 (if flag (map inc v) (map dec v)))` checks (both arms
are `(Mapped (Vec i64) i64 i64)`, result 12), and with a type per closure the arms are `(Mapped (Vec i64)
Inc)` and `(Mapped (Vec i64) Dec)`: `cannot unify (Mapped (Vec i64) Dec) with (Mapped (Vec i64) Inc)`
([R] A10 c1emul). Every join point (an `if`, a `match`, a `loop` accumulator, a vector of recipes, a struct
field) is affected. C1 therefore includes an **erasure rule**: where two closure types meet at a join
the checker unifies them at the erased `(fn (A) R)` of that parameter (a heap closure) and the caller sees
the erased type; `(seq-of ..)` is the explicit form for recipes. `(fn (A) R)` stays the type of a closure
that is stored or passed dynamically, and is an instance of `Fn`.

**What is not adopted.** Fusion by macro (`->>` rewriting a literal pipeline into one loop, as two
designs did) is **not** part of the library: it fuses only a pipeline visible at one call site,
needs the pipeline's functions to be literals, costs one JIT module per distinct macro under `fibc`
(80 to 95 ms each, A9), and is a Rust-side expander feature that stage 2 would have to re-implement.
Callable structs (a functor per function) work but are not ergonomic ([R] A2); they are the
emulation, not the API.

**Conventions the library follows now.** A consumer's function parameter is `:borrow` (it is called,
never stored), so a literal passed to it is a stack object; an adaptor's function parameter is owned
(it is stored), so its literal is a heap closure, one per adaptor per pipeline, not per element. A
function that hands its parameter to an adaptor (`sort-by`'s key, which `map` stores) cannot declare it
`:borrow` either: `parameter f of sort-by-with is declared :borrow but escapes` (A10 sortby).
Combinators (`comp partial complement constantly fnil juxt identity`) return closures and so
allocate one heap closure per call, not per element.

### 2.3 The protocol hierarchy

| Protocol | Methods | Required by | Instances |
|---|---|---|---|
| `Reducible s e` | `each-while`, `size`, `nth` (traps out of range), `last`, `to-vec` | every sequence function and consumer | `Vec List Array Option Map Set Range Iterate Repeat Cycle Chars Slice Queue SortedMap SortedSet Reversed Repeatedly FRange Matches Lines`, every adaptor, user types |
| `Cursable s c`, `Cursor c e` | `cursor`; `advance!`, `current` | `zip interleave map/3 zipmap` (second operand) | `Vec Range Array Slice`; their cursors; an adaptor only after §7 L16 |
| `Lookup s k v` | `get -> (Option v)` | `get update find select-keys` | `Map`; `Vec` (key `i64`); `SortedMap`; `Slice`; user types |
| `Assoc s k v` | `assoc` | `assoc update assoc-in` | `Map`; `Vec` (index below the count); `SortedMap` |
| `Dissoc s k` | `dissoc` | `dissoc disj` | `Map Set SortedMap SortedSet` |
| `Keyed s k` | `contains?` | `contains?` | `Map Set SortedMap SortedSet` (not `Vec` or `Slice`: the index-versus-element wart) |
| `Collection s e` | `conj` | `into conj` (`vec`, `set` and `merge` are `Reducible` plus `Hash`/`Eq`) | `Vec` (end), `List` (front), `Set`, `Map` (of `Pair`), `Queue` (back), `SortedMap` (of `Pair`), `SortedSet` |
| `Emptyable` | `empty` | `empty select-keys assoc-in` | `Vec Map Set List Queue SortedMap SortedSet` |
| `Stack s e` | `peek -> (Option e)`, `pop` | `peek pop` | `Vec` (end), `List` (front), `Queue` (front) |
| `Reversible s e` | `each-while-rev` | `rseq` | `Vec Array Range Slice SortedMap SortedSet` |
| `KeyReducible m k v` | `each-kv-while` | `reduce-kv` | `Map`, `Vec` (index, element) |
| `Eq Ord Hash Show` | built in; `derive` | `= < hash str` | scalars, `str`, `Option`, `List`; **added**: `Vec Map Set Pair Triple` (§2.7); `Ord` of a `Map` or `Set` is not offered (no defined meaning) |
| `Debug` | `debug -> str` | `pr prn pr-str` | as `Show`; strings quoted (§2.7) |
| `Num`, `Bits` | built in (types §2.12) | `+ - * / bit-and ...` | integer and float types |
| `Unit t` | `zero`, `one` (take a value of the type as witness) | `inc dec abs zero? sum` | `i8..i64`, `f32`, `f64` |
| `ToByte ToShort ToInt ToLong ToFloat ToDouble ToChar` | `byte short int long float double char` | the conversions | numeric types |

The prelude's `Countable`, `Indexable`, `Traversable`, `Iter` and `Seq` protocols and its `for-each`, `map`,
`iter`, `collect`, `filter-iter` are replaced by `Reducible` and the functions of §4 (of the 191 case files of
`cases/ownership`, one uses `Countable` and two use `filter-iter`; §6.5 and §8.3 give every prelude name and the
migration). `Associative` splits into `Lookup`, `Assoc` and `Dissoc` so that a vector can be looked up, and a set
can be dissociated, without pretending to be the other; the prelude's `Entry` struct gives way to `Pair`, the one
tuple type (§9 Q31); `dissoc` and `disj` are one `Dissoc`, so `(disj m k)` on a
`Map` type-checks (§5 row 79). Protocols that §4 names only in a signature are defined with their module:
`Sorted s k v` (`subseq`, `fib.sorted`), `Walkable n` (`fib.walk`), `Diffable t` (`fib.data`), `Pretty a`
(`pprint`), and `Send a`, which is the compiler's.

```lisp
;; ran: fib.coll
(defprotocol (Lookup s k v)  (get (self key: k) -> (Option v)))
(defprotocol (Assoc s k v)   (assoc (self key: k val: v) -> Self))
(defprotocol (Dissoc s k)    (dissoc (self key: k) -> Self))
(defprotocol (Keyed s k)     (contains? (self key: k) -> bool))
(defprotocol Emptyable       (empty (self) -> Self))
(defprotocol (Stack s e)     (peek (self) -> (Option e)) (pop (self) -> Self))
(impl (Lookup i64 a) (Vec a) (get (self i) (if (if (>= i 0) (< i (vec-count self)) false) (some (vec-nth self i)) nil)))
```

Protocol methods carry Clojure's names. A name whose Clojure arities differ by a default value
(`get`, `nth`, `reduce`, `sort`, `update`, `sort-by`) has its shortest arity as the method or function and
the others as clauses of the same name once arity overloading exists (§7 L1); until then they are `get-or`, `nth-or`, `reduce1`, `sort-with`, `update-or`, `sort-by-with`, `range-by` and `zip-with`
(§4). **Those stand-ins are deleted in the commit that lands L1** (P6: one way, no aliases), and the cases that
used them are ported in the same commit; `sorted-map-with` and `sorted-set-with` stay, as Clojure's `-by` is
renamed (§3 N4). **[R]** A protocol method may not share a name with a `defun` clause today (`f is already
defined`), so L1 must say that a method and clauses of other arities may coexist.

**Defaults.** A default method written once is specialised per type. `Reducible.size`, `nth`, `last` and
`to-vec` are the examples, overridden where the source knows better: by `Vec` ([R] A10 nth, rangesize) and, in
the same way, by `Array`, `Range`, `Option` and `Slice`; `Ord`'s `<=`, `>`, `>=` and `Eq`'s `!=` are the prelude's.

**Instances are global facts** (syntax §5): the library's `impl Eq (Vec a)` is visible to every
program, which is what lets a `Vec` be a map key.

### 2.4 `nil`, `Option` and `empty`

`nil` is `Option`'s empty variant. `Option` is itself a `Reducible` of size 0 or 1, so absence composes
with sequence functions. The punning Clojure does is replaced by rules:

| Clojure idiom | Here | Rule |
|---|---|---|
| `(get m k)`, `(first c)`, `(peek s)`, `(find m k)`, `(last c)` | `(Option v)` | a miss is `nil`, never a trap; absent and `nil`-valued cannot be confused |
| `(get m k d)`, `(nth c i d)` | `(unwrap-or (get m k) d)`; clause of `get`, `nth` (§7 L1) | |
| `(or (get m k) d)` | `(unwrap-or (get m k) d)` | `or` takes `bool` only ([R] `cannot unify (Option i64) with bool`, A6) |
| the sure case | `(unwrap o)`, `(nth c 0)` | `unwrap` traps `unwrap: nil`; there is no `first!` family: `!` means "mutates through `&` or a cell" |
| `(some pred c)` | `(any? p c)` -> `bool`; `(find-map f c)` -> `(Option b)`; `(find-first p c)` | `some` is `Option`'s constructor and pattern |
| `(if-let [x e] a b)` | `(if-let [x e] a b)` over `Option`; the pattern may be any pattern, refutable ones included | expands to `(match e ((some x) a) (_ b))`, so `(if-let ([a b] o) ..)` falls to the else on a shorter vector (the macro of today expands to `(nil b)` and rejects it: `non-exhaustive match: missing (some [])`, [R] A10 iflet); Clojure's `[a b :as all]` is spelled `([a b] :as all)` |
| `(when-let ..)` | statement: the body is `unit` | |
| `(map f nil)`, `(count nil)` | empty recipe, 0 | `Option` is `Reducible` |
| `(conj nil x)`, `(get nil k)` | type errors | `nil` is not a `Collection` or a `Lookup`; `(first nil)` is `nil` and `(count nil)` is 0, because an `Option` is `Reducible` (A10 nth) |
| `(update m k f)` | `f` receives the value; a missing key traps `update: no key`; `(update-or m k f d)` supplies a default; `(update-opt m k f)` hands `f` the `(Option v)` | `(update m :n inc)` is Clojure's text and, like Clojure's, fails on a missing key (an NPE there, a trap here); Clojure's `(update m w (fnil inc 0))` is `(update-or m w inc 0)`, or literally `(update-opt m w (fnil inc 0))` |
| `(some-> o f g)` | each step takes the unwrapped value and returns an `Option`; the first `nil` ends it | a plain function is `(fn (x) (some (g x)))`; the bind is `(and-then f o)` |
| `(empty c)` | `Emptyable`: the empty collection of `c`'s type | a method with a `self`, so it type-checks today |

An `Option` of an object type is a null pointer and allocates nothing; an `Option` of a scalar, and
every enum with a payload, is a heap object today ([R] A5: 200 allocations for 100 calls). `get` on
a `(Map i64 i64)` therefore allocates once per lookup until §7 C5 lands; it is a cost of the
representation, not of the API, so the API does not bend to it. `Show` of an `Option` is `nil` or `(some x)`
(the literal's own syntax; `Debug` quotes a string inside), never the bare `x`: an `(Option (Option a))` stays
readable.

### 2.5 Mutation and uniqueness

The API is persistent. Two things make it fast, and neither is a transient API.

**Bulk operations build in one function.** `vec`, `into`, `set`, `zipmap`, `sort`, `group-by` and
`frequencies` hold their accumulator in a local `cell` and fill an array they alone own, so the unique
write path applies and nothing is copied: the merge of `sort` over 1000 elements spends 2 objects on its
buffers ([R] A7). Building the `Vec`'s trie from a flat buffer in one pass needs the trie's private nodes,
so `vec` and `into` live in the same module as `Vec`: about n/32 allocations instead of the 2.09 per
element that a `conj` loop costs today (**[H]**: the design record's bulk builder filled a flat array with
14 objects for 1000 elements; assembling the trie from it was not written).

**A loop of `conj` or `assoc` is in place iff the accumulator is unique.** Today it never is
([R] A4):

| Program (1000 updates) | Heap objects |
|---|---|
| `(conj acc i)` in a `loop`; `(set! c (conj @c i))`; `(push! &v i)` | 2094 |
| `(assoc acc i i)`, `(map-put! &m i i)` on a `Map` (design record; the compilability critic re-ran it) | 7935 |
| `(array-set! &c i i)` on an array in a `let` cell | 1 |
| `(bump acc k)` where `bump` writes its array parameter through a local cell (`u1`) | 1001 arrays for 1000 calls |

The cause, from `fibref explain`: `params: a owned (rule 1: stored)`, `calls: (cell ..) arg 1 a:
retain`, and in the caller `(bump ..) arg 1 acc: retain`, `recur ... release [acc] (old loop value)`.
A variable's last use is a retain followed by a release, never a move, so the callee sees count 2 and
`array-set!` copies; and `&` copy-in acquires (syntax §3.13), so the first write of every `&` call
copies. Library code cannot work around either: a helper `builder-push! &buf x` would copy the buffer on
every call. The changes, in order, are §7 C2 (a last use moves), C3 (an `&x` that is the only mention
of `x` shares without acquiring), C4 (`array-push!` with capacity, `array-update!` that moves a slot
out, an uninitialised array). **They do not suffice for the existing `Vec`, because the blocker is not only
the caller's retain: a field of an owned, unique shell is never uniquely owned by the callee.** `bump`
that matches an owned `(E1 n arr)` and writes `arr` through a cell copies the array each time: four chained
calls on unique temporaries cost 10 objects, one call 4 ([R] A10 own), and `explain` says `arr derived-of e`,
`(cell ..) arg 1 arr: retain`, `release [e] (exit)`. The same with a method that takes `self` `:owned` on a
struct with an array field, in a `loop` of 1000 puts: **2002 objects** (one shell and one array copy per
call; `explain`: `self owned (not inferred)`, `(put ..) call arg 1 b: retain`, `release [b] (exit)`).
What works today is a struct **held in a cell**: 1000 `set-field! &c n ..` allocate nothing (A10 own). So C2
to C4 fix the caller's side only, and §7 C7 is needed beside them: a `match` or field read on an owned shell
that is dead afterwards moves the payload out (no retain, no release of the field with the shell). With C2, C3,
C4 and C7 the existing persistent code is in place when unique, with no new API: `(conj v x)` takes `self`
`:owned` and writes the tail array. That removes the copies of the tail and of the path; the `Vec` header is
still one object per `conj`, because an enum has no `set-field!`. Reaching zero per `conj` needs `Vec` to be a
struct with a tail of spare capacity, held in a cell and updated through `set-field!`, which is what works
today; that changes a layout that `fibc` (`rt/vec.lir`, `lower/pattern.rs`, `macros/abi.rs`) and `fibref`
(`eval/vecs.rs`) share. The review moves that decision **before tranche 3** (§9 Q18): the struct `Vec` is the
route the evidence supports, and C7 is the route that keeps the enum.

**What is tested.** The zero-cost claims are tests with counts (§8): a three-stage pipeline over n and
over 10n elements allocates the same number of objects (passes today: +4 for 1000, A1), a bulk `vec` of
n allocates at most n/32 + c, a `conj` loop of n allocates at most n + c once C2 to C4 and C7 land (the
header, one object per `conj`) and at most c once `Vec` is a struct (**both fail today**, 2094; they are in
the `open` list of the harness, which prints them as failing, never as pending). The harness has no object
count today: a case header is `spec expect result audit error trap`; §7 H1 adds `allocs` (a maximum) and the
count lines it reads are the `A` lines of `fibc run --trace`, which the review ran (A10 t1r: 2099 for the
chain of A1 through `fibc run --trace`).

### 2.6 Destructuring and binding

What exists [R] A6: `match` takes literals, `_`, names, `(Variant p ..)`, `nil`, `(some p)`, vector
patterns with `& rest` and `:as`, struct patterns, guards; `let` takes irrefutable patterns (a struct
pattern, `[& r]`); `fn`, `defun` and `loop` take symbols only; `(let [a 1] ..)` is `malformed let`.

| Clojure | Here today | Proposed (§7) |
|---|---|---|
| `(let [[a b] v] ..)` | `match` with `[a b]`; a refutable `let` pattern is rejected | refutable `let` pattern traps `let: pattern does not match` (L8) |
| `(fn [[k v]] ..)`, `(defn f [{:keys [a]}] ..)` | `a fn parameter is sym or sym: type` | irrefutable patterns in `fn`, `defun`, `loop` parameters, as sugar for a `let` (L7) |
| `{:keys [a b]}` | `(. s a)` reads a field and `(with s (a v))` replaces one (§4.2); no punning today | the pattern `{:keys [a b]}` on a struct; a `(Map keyword v)` uses `get` (L9) |
| `[k v]` on a map entry | `(Pair k v)`, the one tuple type | tuple-like structs (`Pair`, `Triple`) accept vector patterns (L3b) |
| `(case x 1 :a (2 3) :b)` | `match` with literals | or-patterns `(or p q)` (L6) |
| `(let [a 1 b 2] ..)`, `(loop [i 0] ..)` | `(let ((a 1) (b 2)) ..)` | bracket forms accepted beside the parenthesised ones (E3) |
| `(cond t1 e1 t2 e2)` | `(cond (t1 e1) ..)` | Clojure's flat pairs (E4) |

A pattern states its shape: `[a b]` matches exactly two elements, `[a b & r]` binds a new `Vec`
(empty, never `nil`), there is no default inside a pattern (the default is `unwrap-or`), and a
refutable pattern in a binding position is an explicit trap, as `nth` is. `{:strs ..}`, `{:syms ..}`
and `:or` are not offered (§4.2).

### 2.7 Equality, order, hash, print

* `=` is structural on **one** static type. `(= 1 1.0)` is a compile error ([R] `cannot unify f64
  with i64`), not `false`; there is no `==`. `(= [1] (list 1))` is a type error; cross-collection
  equality is spelled `(seq= a b)`. Floats follow IEEE (`(= nan nan)` false, `(= 0.0 -0.0)` true).
* `Eq Hash Show Debug` exist for `Vec Map Set Pair Triple Option List`, `Ord` for `Vec Pair Triple Option List`
  (not for `Map` and `Set`: an order over an iteration order that §2.7 leaves unspecified has no meaning), and
  `derive` for user types, which today derives `Eq Ord Hash Show` only (`(derive Debug P)` is `cannot derive
  Debug: only Eq, Ord, Hash and Show`, [R] A10 dd; §7 L17). Today `(= [1 2] [1 2])` is `no implementation of
  Eq for (Vec i64)` ([R] A6); the prototype's instances make it work, and a `Vec` a map key (A7). `Ord` on sequences is lexicographic, a shorter
  one first on a tie (Clojure compares length first; §5 row 16); `Eq` and `Ord` agree (`(compare a b)`
  is 0 exactly when `(= a b)`, with one exception below).
* `compare` returns `-1`, `0` or `1` as an `i64`. A comparator, wherever one is taken (`sort-with`,
  `sorted-map-with`), is `(fn (e e) i64)` returning a negative, zero or positive number, as Clojure's;
  a `bool` predicate is **not** a comparator (Clojure accepts one and collapses "equal" elements).
  `(comparator less?)` converts. **`compare` on `f64` is a total order**: a NaN is greater than every number and
  equal to itself, and `-0.0` equals `0.0`; so `compare` and `=` agree on every value except NaN
  (`(= nan nan)` is false, `(compare nan nan)` is 0), `<` stays IEEE (false against a NaN), and `sort` of
  floats is deterministic: `(sort [3.0 nan 1.0 2.0 nan 0.5])` and the same elements in another order both give
  `[0.5 1.0 2.0 3.0 NaN NaN]` ([R] A10 nan). One generic definition does it, `(!= a a)` holding only for a
  NaN. `max` and `min` return NaN when either argument is NaN, as Clojure's do (A10 nan); the first version of
  this page said the NaN loses, which no definition over `<` gives. The law tests of §8.1 test `compare` for
  totality over NaN and for agreement with `=` everywhere else.
* `Hash f64` hashes `-0.0` as `0.0` and one NaN, so `(get (assoc m 0.0 1) -0.0)` finds the key
  ([R] it is `nil` today, A6). A NaN key is never found, as in Clojure. **The combiners of today trap.**
  `derive Hash` and the prelude's `Hash (List a)` fold with `h*31 + hash x`, and a struct of two strings or a
  list of three is `trap: integer overflow in * at i64` under both tools ([R] A10 hash), so a `Map` keyed by
  a two-string struct or a `(Vec str)`, the bread and butter of a symbol table, fails in tranche 1 as soon as
  the keys are long enough. `hash-combine` is therefore a **rotate-and-xor mixer built from `shl`, `shr`,
  `bit-or` and `bit-xor`, which never trap and exist today**: it hashes a 30-element `Vec` and a `Vec` of
  strings without a trap and tells `["ab" "c"]` from `["a" "bc"]` and `["a" "b"]` from `["b" "a"]` (A10
  hash); `derive Hash` and every `Hash` instance of the library use it, and `hash-unordered-coll` folds the
  elements' hashes with `bit-xor`, which is commutative and does not trap, so `Hash (Map k v)` and `Hash (Set k)`
  are order independent. Both ship in tranche 0 and 1 (§8.2); the multiplicative finaliser that needs a wrapping
  multiply (§7 L10) replaces the rotate-xor in tranche 3, in one commit with the new integer hash (§9 Q15).
* **`Show` is the display form and `Debug` the readable form**, with fixed meanings at every depth. A
  string shows as itself and debugs quoted and escaped; a collection shows its elements with `Show`
  (`[a b]`) and debugs them with `Debug` (`["a" "b"]`). The text of every type of §2.3, in one place: a `Vec` and
  every recipe (`Range`, `Mapped`, `Filtered`, `Taken`, and the rest: one line over `show-seq` each) print
  `[a b]`; a `Pair` and a `Triple` print `[a b]` too (Clojure's entry is a vector), `Debug` `["a" 1]`; a
  `Set` prints `#{a b}`; a `Map` prints `{k v, k v}`; an `Option` prints `nil` or `(some x)`; a struct made by
  `derive Show` prints `(Name f1 f2)`; a recipe's `Eq` is `(seq= a b)`, not `=` ([R] A10 showseq; the
  first version said every type has a `Show`, which was false for recipes). `str`, `println`, `print` use `Show`; `pr`,
  `prn`, `pr-str`, `dbg` use `Debug`. Clojure's `(str ["a"])` is `["a"]` and its `println` of the
  same is `[a]`; here `str` and `println` agree (§5 rows 29 and 41).
* **Map and set iteration order is unspecified**: the HAMT's order, a function of the hash and the insertion
  history, the same in the interpreter and the compiled program (the hash is part of the spec). **`Show` and
  `Debug` of a `Map` and of a `Set` do not print that order: they print the entries in the order of the `Show`
  text of their keys** (so `10` precedes `2`; text order, not numeric), which does not depend on the hash, so
  the change of the integer hash (§9 Q15) leaves every printed map unchanged, and a map inserted in two
  orders prints the same text (`up` and `dn` of A10 showmap, twelve integer keys). Tests compare with `=`; a
  numerically ordered print is a `SortedMap` (`fib.sorted`).

### 2.8 Numbers

Arithmetic is the builtin binary `Num` method on one type; the variadic spellings are macros that
fold (`(+ a b c)` is `(+ (+ a b) c)`, `(< a b c)` is a short-circuit chain). `(+)` has no type, so it is
a compile error: use `(sum c)`. Integers divide toward zero, `rem` has the dividend's sign, overflow
traps at every width, float division is IEEE (types §2.12). Literals have one type, so `inc`, `dec`,
`abs`, `zero?`, `pos?`, `neg?`, `even?`, `odd?`, `mod` take a `Unit` witness (`(one x)`), which makes
them generic over every numeric type ([R] A7: design "zero"'s `num1`). Conversions are checked:
`(long x)`, `(int x)`, `(byte x)`, `(double x)`, `(char n)` dispatch on the argument type and trap when
the value does not fit; `(trunc i8 x)` keeps the low bits; `(fptosi i64 x)` saturates. `unchecked-*`
wrap at the operand's width and need a new builtin (§7 L10); `clojure.math` is `fib.math`, with the
IEEE-exact functions (`sqrt floor ceil rint copysign`) as builtins and the transcendental ones written
in fibber over `f64->bits`, so the interpreter and the compiled program agree to the bit (§7 L11).
`round` is half up, as Java's `Math.round` that Clojure's wraps. `rand` and `shuffle` take an `Rng`. An integer
literal does not adopt a float type: `(* 2 1.5)` is `cannot unify f64 with i64` ([R] A6), which numeric code
meets constantly; §9 Q24 asks whether a literal may adopt one.

### 2.9 Strings

A `str` is immutable UTF-8. **It is not a collection**: `(count "é")` would be 2 or 1 depending on the
unit, and `(subs s 0 (count s))` would then slice wrongly without trapping. So `count`, `first`,
`map` on a `str` are type errors ([R] `no implementation of Reducible for str`, A7), and the three
units are three names: `(str-len s)` bytes in O(1); `(count (chars s))` characters (Unicode scalars)
in O(n); `(str/chars s)` the `Reducible char` view. **Every offset is a byte offset**: `subs`,
`index-of`, `str/last-index-of` and the prelude's `str-slice` agree, so an `index-of` result feeds
`subs`; an offset that splits a character traps `str-slice [a, b) splits a character`. `str/split`
keeps trailing empties and round-trips with `str/join`; `str/replace` takes a literal, `replace-re` a
`Regex` and a `$n` template, `replace-with` a function; `upper-case` and `lower-case` are Unicode full
case mappings, `upper` and `lower` on a `char` the simple one-to-one mapping. `(str a b ..)` is a macro
over `Show`; `format` and `printf` are macros that check each directive against its argument's type at
compile time and need a literal format string. `index-of` returns `(Option i64)`, never -1. **`Chars` is a decoder over one byte array**: `(count (chars s))`
of a 1000-character string allocates 2 objects, where the prelude's `str-chars` (a `conj` loop of
`(Vec char)`) allocates 2095 ([R] A10 chars), and the two agree on `"aé€😀z"`. Every string function of §4.7 is
built on `str-bytes`, which allocates a fresh array per call (100 objects for 100 calls, A10 str), so a
tokenizer that calls `str/index-of` from an offset copies the string each time; §7 L18 adds `(str-byte-at s i)`
and `(str-find s pat from)`, which allocate nothing. `parse-double` has the grammar `[+-]? (digits ['.' digits*]
| '.' digits) ([eE] [+-]? digits)?` and the names `NaN`, `Infinity` and `-Infinity` (**[H]**: the run of A10 pd
covers the decimal part only); the grammar is checked in
fibber before `strtod` is called, because `strtod` accepts hex, a leading space, `inf` and trailing junk, and
`fibref` rejects hex (`unsupported: hex float`) where `fibc` prints 16.0, so the tools disagree without the
check ([R] A10 pd: the check followed by `strtod` gives the same eleven answers under both tools).

### 2.10 Errors and effects

`trap` is the only abort. A recoverable failure is a value: `(Result a e)`, with `Ok` and `Err`, in the
prelude (it is `compiler/util/result.fib` today, which is deleted in the commit that adds `Result`, §6.2), and
`(try-let ((x e) (y f)) body)` binds each `Ok` payload in turn and yields the first `Err` as the value of the
whole expression. **It is not `try!`**: the language has no early return, so a macro that leaves the enclosing
function cannot be written (`(return (Err e))` is `unbound name return`, [R] A10 try), and `try-let` is a block
macro over nested `match` that runs today, 106 under both tools (A10 try). Parsers return `Option` (`parse-long`) or `Result` (`regex`). `ex-info`, `try`,
`catch`, `finally` have no target; `finally`'s non-memory cleanup (closing a handle) needs a scope-exit
hook (§7 L12), after which `with-open` is a macro over it. Randomness takes an `Rng` value (a seeded
xoshiro with its state in cells; one per task); time and the environment are functions of `fib.sys`.
`println` is a macro over `Show` (several arguments are joined with a space, [R] A10 pm); the prelude's
one-`str` function of that name is replaced in value position by a **function twin generic over `Show`**, and
so are `str`, `print`, `prn` and `pr`: `(run! println [1 2 3])` and `(map str xs)` work, which a macro alone
cannot do (`unbound name str`), because a macro and a function of one name coexist (head position expands,
[R] A6, A10 twin).

## 3. Argument order and naming

| # | Rule | Examples |
|---|---|---|
| N1 | A function over a **sequence** takes the sequence **last**; a function over a **collection value** takes it **first**. So `->>` threads sequences and `->` threads collections. | `(map f c)`, `(take n c)`, `(reduce f init c)`; `(assoc m k v)`, `(get m k)`, `(conj c x)`, `(update m k f)`, `(subvec v a b)` |
| N2 | The function argument of a sequence function comes first, so a literal `fn` reads before the data. | `(filter p c)`, `(group-by f c)`, `(sort-by key c)` |
| N3 | The exceptions Clojure has are kept and listed: `into` is collection-first for its target; `str/join` and `str/split` take the separator and the string in Clojure's two orders; `reduce-kv` (`f init m`) and `set/select` (`p s`) take the collection last, as Clojure's do; `includes?` takes the element first (`(includes? x c)`) so that `->>` threads the source; `str/includes? s sub` is the string function and takes the string first. | `(into to c)`, `(str/join sep c)`, `(str/split s sep)`, `(reduce-kv f init m)` |
| N4 | `-by` takes a **key function** (`sort-by group-by partition-by`); `-key` is Clojure's `max-key`/`min-key`; `-with` takes a **comparator or combiner** (`sorted-map-with merge-with`; the stand-ins `sort-with` and `sort-by-with` until L1). | `(sort-by count c)`, `(sort-by-with count desc c)`, `(merge-with + a b)` |
| N5 | A `?` suffix is a `bool` result; a `!` suffix means the function writes through an `&` parameter, a cell or an `Atom` (`push! swap! reset! set!`), or runs only for its effect on each element (`run!`, Clojure's). A persistent function never ends in `!`. | `empty?`, `contains?`, `push!` |
| N6 | `->` in a name is a conversion `from->to`, as the builtins have it (`char->i32`); the library's conversions that Clojure names `long`, `int`, `double` keep those names. The builtin casts that are one instruction name the target first (`(trunc i8 x)`, `(zext i64 b)`, `(fptosi i64 x)`): two conventions, because the first is a name and the second a type argument. | `(long x)`, `(char->str c)` |
| N7 | A function that returns a recipe is named for what it does; an eager twin exists only where Clojure has it (`mapv filterv`). Materialisers are `vec set into zipmap sort reverse`. | `(vec (map f c))` |
| N8 | Types are CamelCase; an adaptor's struct is named for the adaptor in the past participle where its verb has one (`Mapped Filtered Taken Dropped Kept`) and by the noun otherwise (`Cat Mapcat Iterate Repeat Cycle Reductions Distinct ZipWith TreeSeq`). §4 writes `(Mapped c e b)`, the data parameters, which is what compiles today; §2.2 writes `(Mapped c f)`, the form after C1, and a colour parameter follows either (§2.1). Modules are `fib.x`, required with an alias (`str/`, `set/`, `math/`). | |
| N9 | A name that the prelude's `join` (the task wait) or core forms use is never redefined unqualified; the library's `join` is always `str/join`. | |
| N10 | In the signature column of §4: parameters in order, `->` result, ` \| ` introduces the protocol constraints (`Reducible c e`), lower-case letters are type variables, `;` separates the signatures of the arities of one name. `c` is a source, `s` a keyed collection, `e` an element. | |
| N11 | An `Option` function takes the function first and the `Option` last, as a sequence function does, so `->>` threads it: `(map-opt f o)`, `(and-then f o)`. | `(->> o (map-opt inc) (and-then half))` |

## 4. The function table

Every name of the survey has one row: 673 names of `clojure.core` 1.12, `clojure.set`,
`clojure.string`, `clojure.walk`, `clojure.data` and `clojure.math`, grouped by the module that holds
it, plus 43 names that Clojure lacks and the library adds (marked `(new)`). Columns:

* **Verdict**: `keep` (same name, same meaning), `adapt` (same name, changed meaning or type, stated),
  `rename` (another name), `replace` (another construct, not a function of that name), `omit` (not
  offered, with the reason), `new`. Over the survey's names: keep 138, adapt 206,
  rename 47, replace 82, omit 200. The survey's own counts were keep 149,
  adapt 185, rename 49, replace 87, omit 203; the 33 verdicts that changed are listed in the design
  record (`synth-notes.md`), and the review changed four more, from `keep` to `adapt` (`swap!`, `conj`, `comp`,
  `def`, §10). The counts are those of a script that parses these tables (`tablecount.py` of the review), which
  the executable `stdlib_table` of §8.1 replaces.
* **Fibber**: the spelling. `macro`, `core form`, `reader`, `pattern` say what kind of thing it is.
* **Signature**: in the notation of §3 N10; each arity of an overloaded name is one signature, separated
  by `;`. Where an arity needs L1 (§7), the stand-in the first tranches use is named in the note.
* **T**: the tranche of §8 that delivers it (names that exist today are `1`).
* **Note**: what differs from Clojure, and where it matters, the cost.

Offered over the survey's names by tranche: T1 155, T2 56, T3 84, T4 76, T5 20.

### 4.1 Builtins (compiler primitives, syntax §4.3)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `aget` | rename | `(array-get a i)` | `(Array a) i64 -> a` | 1 | builtin; traps out of range |
| `aset` | rename | `(array-set! &a i x)` | `&(Array a) i64 a -> unit` | 1 | builtin; in place only when `a` is unique, else it copies |
| `alength` | rename | `(array-len a)` | `(Array a) -> i64` | 1 | builtin |
| `aclone` | rename | `(array-copy a lo hi)` | `(Array a) i64 i64 -> (Array a)` | 1 | builtin |
| `make-array` | adapt | `(array n x)` | `i64 a -> (Array a)` | 1 | builtin; an initial element is required (§7 C4 adds an uninitialised array for the library) |
| `+` | adapt | `(+ a b ..)` | `t t -> t \| Num t` | 1 | the builtin is binary on one type, overflow traps; a macro folds more arguments, `(+)` is a compile error (use `sum`) |
| `-` | adapt | `(- a b ..) (- a)` | `t t -> t \| Num t; t -> t \| Num t` | 1 | binary builtin, `neg` for one argument; macro folds |
| `*` | adapt | `(* a b ..)` | `t t -> t \| Num t` | 1 | as `+`; `product` for a collection |
| `/` | adapt | `(/ a b)` | `t t -> t \| Num t` | 1 | integers truncate toward zero and trap on zero; no ratios |
| `rem` | keep | `(rem a b)` | `t t -> t \| Num t` | 1 | builtin; the sign of the dividend |
| `<` | adapt | `(< a b ..)` | `t t -> bool \| Ord t` | 1 | builtin `Ord` method on any ordered type; a macro chains more arguments |
| `>` | adapt | `(> a b ..)` | `t t -> bool \| Ord t` | 1 | as `<` |
| `<=` | adapt | `(<= a b ..)` | `t t -> bool \| Ord t` | 1 | IEEE at floats |
| `>=` | adapt | `(>= a b ..)` | `t t -> bool \| Ord t` | 1 | IEEE at floats |
| `bit-and` | keep | `(bit-and a b ..)` | `t t -> t \| Bits t` | 1 | builtin, binary; a macro folds more |
| `bit-or` | keep | `(bit-or a b ..)` | `t t -> t \| Bits t` | 1 | as `bit-and` |
| `bit-xor` | keep | `(bit-xor a b ..)` | `t t -> t \| Bits t` | 1 | as `bit-and` |
| `bit-not` | keep | `(bit-not a)` | `t -> t \| Bits t` | 1 | builtin |
| `bit-shift-left` | rename | `(shl a n)` | `t t -> t \| Bits t` | 1 | builtin; the distance is taken modulo the width |
| `bit-shift-right` | rename | `(sar a n)` | `t t -> t \| Bits t` | 1 | builtin, arithmetic |
| `unsigned-bit-shift-right` | rename | `(shr a n)` | `t t -> t \| Bits t` | 1 | builtin, logical |
| `Long/bitCount` | rename | `(popcount a)` | `t -> t \| Bits t` | 1 | builtin |
| `=` | adapt | `(= a b ..)` | `t t -> bool \| Eq t` | 1 | one type, structural; a type error across numeric types; a macro chains more arguments |
| `not=` | rename | `(!= a b)` | `t t -> bool \| Eq t` | 1 | builtin `Eq` method |
| `not` | keep | `(not b)` | `bool -> bool` | 1 | builtin; no truthiness |
| `atom` | keep | `(atom v)` | `a -> (Atom a) \| Send a` | 1 | builtin; an atom of a type that is not `Send` is a compile error |
| `swap!` | adapt | `(swap! a f arg ..)` | `(Atom a) (fn (a ..) a) .. -> a` | 1 | a prelude macro over the builtin: `(swap! a + 5)` is `(swap! a (fn (x) (+ x 5)))`, and the macro and the builtin coexist ([R] A10 swap: 30 under both tools); returns the new value; `f` may run more than once |
| `reset!` | keep | `(reset! a v)` | `(Atom a) a -> unit` | 1 | builtin |
| `deref` | keep | `@x` | `(Atom a) -> a` | 1 | builtin |
| `set!` | rename | `(set! c v)` | `(Cell a) a -> unit` | 1 | builtin; also `(set! (. obj field) v)` |

### 4.2 Core forms, macros and reader syntax

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `doseq` | keep | `(doseq [x xs :when p] body)` | macro | 2 | runs for effect, same modifiers |
| `if` | keep | `(if c a b)` | core form | 1 | `bool` test, both arms (syntax §3.4) |
| `if-not` | adapt | `(if-not c a b)` | macro | 2 | `(if (not c) a b)` |
| `if-let` | adapt | `(if-let [x e] a b)` | macro `(Option a)` | 1 | spelled `(if-let (x e) a b)` today, the bracket form needs §7 E3; the binding is any pattern over an `Option`, refutable ones included: the macro expands to `(match e ((some x) a) (_ b))` (§2.4) |
| `when` | keep | `(when c body ..)` | macro | 1 | the body is `unit` (the else is `()`) |
| `when-not` | adapt | `(when-not c body ..) (unless c body ..)` | macro | 2 | `unless` is the prelude's name, kept as a deprecated alias for the cases that use it and removed when they are ported (P6: one way) |
| `when-let` | adapt | `(when-let [x e] body ..)` | macro | 1 | spelled `(when-let (x e) ..)` today (bracket form: §7 E3); over `Option`; the body is `unit` |
| `cond` | adapt | `(cond t1 e1 t2 e2 :else d)` | macro | 2 | Clojure's flat pairs replace the prelude's `(cond (t e) ..)` (§7 E4); falling off the end traps `cond: no clause matched`; `:else` or `true` ends it |
| `condp` | adapt | `(condp pred x t1 r1 t2 r2 default)` | macro | 3 | over `cond`; no clause traps |
| `case` | adapt | `(case x lit r (lit1 lit2) r2 default)` | macro | 3 | expands to `match` with literal patterns and or-patterns (§7 L6); no match is a compile error without a default |
| `and` | adapt | `(and a b ..)` | macro | 1 | `bool` only, short-circuit |
| `or` | adapt | `(or a b ..)` | macro | 1 | `bool` only; the default idiom is `(unwrap-or o d)` |
| `do` | keep | `(do e ..)` | core form | 1 | `(do)` is `()` |
| `let` | adapt | `(let [a 1 b 2] body)` | core form | 2 | the bracket form is sugar over the parenthesised pairs (§7 E3); patterns allowed, refutable ones trap (§7 L8) |
| `loop` | keep | `(loop [i 0 acc x] ..)` | core form | 2 | as `let`: bracket sugar |
| `recur` | keep | `(recur ..)` | core form | 1 | tail position inside `loop` only |
| `fn` | adapt | `(fn [x y] body)` | core form | 2 | bracket parameters are sugar; patterns allowed in parameters (§7 L7); no arity overloading in `fn` |
| `def` | adapt | `(def name: T v)` | core form | 1 | immortal; today the initialiser is a literal: `(def ok: (Set i64) (set [1 3]))` is `def ok: initialiser is not a constant expression`, as is `(def v: i64 (f 2))` and `(def counter: (Atom i64) (atom 0))`, while `(def v: (Vec i64) [1 2 3])` and `{1 2}` run ([R] A10 def). §7 L15 evaluates a total library constructor of constants (`set vec hash-set hash-map zipmap`) once before `main`, in module order; an `atom` or `cell` is not allowed (P7, §9 Q23) |
| `defmacro` | keep | `(defmacro name (params) body)` | core form | 1 | rest parameter `...` |
| `dotimes` | keep | `(dotimes [i n] body)` | macro | 1 | prelude macro, spelled `(dotimes (i n) ..)` today; the bracket form needs §7 E3 |
| `for` | adapt | `(for [x xs :when p :let [y e] y ys] body)` | macro | 2 | a recipe (`Mapped`/`Mapcat`/`Filtered` chain), not a `Vec`; `:when`, `:while`, `:let` |
| `while` | keep | `(while c body ..)` | macro | 1 | `bool` test |
| `doto` | keep | `(doto x (f a) g)` | macro | 1 | prelude macro |
| `with` (new) | new | `(with e (field v) ..)` | macro `s -> s` | 2 | a copy of the struct `e` with the named fields replaced, `e` untouched: Clojure's `assoc` on a record, and Rust's `S { f: v, ..e }`; it expands to a cell and `set-field!`, which runs today, in a module that does not itself define `concat` (B2): `(with db (users ..) (n 3))` gives 4 3 3 2 under both tools ([R] A10 with); `update-in` over records is `with` nested; a unique `e` can reuse its object (§2.5, C7) |
| `comment` | keep | `(comment ..)` | macro | 3 | expands to `()` |
| `time` | adapt | `(time e)` | macro | 4 | prints the elapsed time to stderr, returns `e`'s value; needs `now-ns` |
| `assert` | keep | `(assert c) (assert c msg)` | macro | 1 | prelude macro; always on, traps |
| `var` | keep | `(var name)` | core form | 1 | the definition a name resolves to, not a Var object |
| `quote` | keep | `'x` | core form | 1 | the value is a `Form` |
| `ns` | keep | `(ns a.b (:require [c.d :as d]) (:use e))` | core form | 1 | no `:refer`, no `:import` |
| `defprotocol` | keep | `(defprotocol (P self det*) (m (self x: T) -> R))` | core form | 1 | static dispatch |
| `->` | keep | `(-> x f (g a))` | macro | 1 | prelude macro |
| `->>` | keep | `(->> x f (g a))` | macro | 1 | prelude macro |
| `as->` | keep | `(as-> x v (f a v) ..)` | macro | 2 |  |
| `cond->` | adapt | `(cond-> x t1 f1 t2 f2)` | macro | 3 | `bool` tests, flat pairs |
| `cond->>` | adapt | `(cond->> x t1 f1 t2 f2)` | macro | 3 | as `cond->` |
| `some->` | adapt | `(some-> o f g)` | macro `(Option a)` | 3 | each step takes the unwrapped value and returns an `(Option b)`; stops at the first `nil`; a plain function is wrapped as `(fn (x) (some (f x)))` |
| `some->>` | adapt | `(some->> o f g)` | macro `(Option a)` | 3 | as `some->`, threading last |
| `partial` | adapt | `(partial f a)` | macro `(fn (a b) c) a -> (fn (b) c)` | 2 | leaves one parameter free |
| `partial2` (new) | new | `(partial2 f a)` | macro `(fn (a b c) d) a -> (fn (b c) d)` | 2 | leaves two parameters free; a macro cannot see `f`'s arity, so the name carries it |
| `comp` | adapt | `(comp f g ..)` | `(fn (b) c) (fn (a) b) -> (fn (a) c)` | 1 | right to left; a macro nests for more; transducers compose with `xf`, not `comp` (§5 row 44) |
| `juxt` | adapt | `(juxt f g)` | `(fn (a) b) (fn (a) c) -> (fn (a) (Pair b c))` | 3 | two or three functions give `Pair`/`Triple`; for more, `(map (fn (f) (f x)) fs)` gives a recipe of the one result type |
| `complement` | keep | `(complement p)` | `(fn (a) bool) -> (fn (a) bool)` | 2 |  |
| `constantly` | adapt | `(constantly x)` | `b -> (fn (a) b)` | 2 | unary |
| `fnil` | adapt | `(fnil f d)` | `(fn (a) b) a -> (fn ((Option a)) b)` | 1 | the `Option` is what `update-opt` hands over; `(update-or m k f d)` is the short form |
| `memoize` | adapt | `(memoize f)` | `(fn (a) b) -> (fn (a) b) \| Hash a, Eq a` | 4 | keeps an `(Atom (Map a b))`; `f` must be sendable |
| `every-pred` | adapt | `(every-pred p q)` | `(fn (a) bool) (fn (a) bool) -> (fn (a) bool)` | 3 |  |
| `some-fn` | adapt | `(some-fn f g)` | `(fn (a) (Option b)) (fn (a) (Option b)) -> (fn (a) (Option b))` | 3 | the first `some` |
| `completing` | adapt | `(completing f fin)` | `(fn (a e) a) (fn (a) r) -> (Completing a e r)` | 5 | with transducers |
| `cat` | adapt | `(cat)` | `-> (CatXf)` | 5 | the transducer form of `mapcat identity`; with transducers |
| `halt-when` | adapt | `(halt-when p)` | `(fn (e) bool) -> (HaltXf e)` | 5 | with transducers |
| `:keys` | adapt | `{:keys [x y]}` | pattern | 3 | struct field punning in `let`/`fn`/`match` (§7 L9); a `(Map keyword v)` uses `get` |
| `:as` | keep | `(pat :as v)` | pattern | 1 | syntax §3.6 |
| `&` | keep | `[a b & r]` | pattern | 1 | `r` is a new `Vec` |
| `_` | keep | `_` | pattern | 1 | the wildcard |
| `'x` | keep | `'x` | reader | 1 | syntax §1.2 |
| ``x` | adapt | ``x` | reader | 1 | quasiquote without qualification and without `x#` |
| `~x` | rename | `,x` | reader | 1 | the comma, when it touches a form |
| `~@x` | rename | `,@x` | reader | 1 | with a `(Vec Form)` |
| `@x` | keep | `@x` | reader | 1 | cells, atoms and weak references |
| `#'x` | rename | `(var x)` | core form | 1 |  |
| `#_` | keep | `#_` | reader | 1 | syntax §1.1 |
| `#(...)` | adapt | `#(f % %2)` | reader | 2 | reads as `(fn (%1 %2) (f %1 %2))`; `%` is `%1`; no `%&`; nesting is an error (§7 E8) |
| `#{...}` | adapt | `#{a b}` | reader | 2 | reads as `(hash-set a b)` (§7 E8) |
| `(:k m)` | adapt | `(:k x)` | checker | 2 | a keyword that unifies with `(fn (S) T)` elaborates to `(fn (x) (. x k))` when `S` is a struct with that field and to `get` when `S` is a `(Map keyword v)`; `(map :name ps)`, `(sort-by :age ps)`, `(group-by :dept ps)` are the commonest Clojure lines (§7 L14, §9 Q11) |

### 4.3 `fib.core` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `str` | adapt | `(str a ..)` | macro `a ... -> str \| Show a` | 1 | concatenates `(show a)`; `(str)` is `""`; a Rust prelude macro (§6.3) |
| `subs` | adapt | `(subs s a b) (subs s a)` | `str i64 i64 -> str; str i64 -> str` | 1 | BYTE offsets, pairing with `str-len` and `index-of`; traps when an offset splits a character |
| `name` | adapt | `(name k)` | `keyword -> str` | 4 | keywords are flat; no namespace part |
| `keyword` | adapt | `(keyword s)` | `str -> keyword` | 4 | interns at run time |
| `gensym` | keep | `(gensym) (gensym prefix)` | `-> str; str -> str` | 1 | builtin; macro time only (syntax §3.16) |
| `char` | adapt | `(char n)` | `t -> char \| ToChar t` | 2 | checked: traps on a non-scalar; `i32` and `i64` |
| `parse-long` | adapt | `(parse-long s)` | `str -> (Option i64)` | 1 | `nil` on any malformed input, never a trap |
| `parse-double` | adapt | `(parse-double s)` | `str -> (Option f64)` | 3 | correctly rounded through `strtod`, after the grammar of §2.9 is checked in fibber: hex, a leading space, `inf` and trailing junk give `nil` under both tools ([R] A10 pd) |
| `parse-boolean` | adapt | `(parse-boolean s)` | `str -> (Option bool)` | 3 |  |
| `Integer/parseInt` | rename | `(parse-i32 s) (parse-i32 s radix)` | `str -> (Option i32); str i64 -> (Option i32)` | 3 |  |
| `Long/parseLong` | rename | `(parse-long s) (parse-i64 s radix)` | `str -> (Option i64); str i64 -> (Option i64)` | 3 |  |
| `Double/parseDouble` | rename | `(parse-double s)` | `str -> (Option f64)` | 3 |  |
| `inc` | keep | `(inc x)` | `t -> t \| Num t, Unit t` | 1 | any numeric type; traps on overflow |
| `dec` | keep | `(dec x)` | `t -> t \| Num t, Unit t` | 1 |  |
| `mod` | keep | `(mod a b)` | `t t -> t \| Num t, Ord t, Unit t` | 1 | the sign of the divisor; traps on zero |
| `max` | adapt | `(max a b ..)` | `t t -> t \| Ord t` | 1 | any ordered type; a macro folds more; a NaN argument gives NaN, as Clojure's ([R] A10 nan) |
| `min` | adapt | `(min a b ..)` | `t t -> t \| Ord t` | 1 | as `max` |
| `abs` | adapt | `(abs x)` | `t -> t \| Num t, Ord t, Unit t` | 1 | traps on the minimum of an integer type |
| `unchecked-add` | adapt | `(unchecked-add a b)` | `t t -> t \| Bits t` | 3 | wrapping, at the operand's width (§7 L10) |
| `unchecked-subtract` | adapt | `(unchecked-subtract a b)` | `t t -> t \| Bits t` | 3 | wrapping |
| `unchecked-multiply` | adapt | `(unchecked-multiply a b)` | `t t -> t \| Bits t` | 3 | wrapping; hash mixers and generators need it |
| `unchecked-negate` | adapt | `(unchecked-negate a)` | `t -> t \| Bits t` | 3 | wrapping |
| `unchecked-inc` | adapt | `(unchecked-inc a)` | `t -> t \| Bits t` | 3 | wrapping |
| `unchecked-dec` | adapt | `(unchecked-dec a)` | `t -> t \| Bits t` | 3 | wrapping |
| `byte` | adapt | `(byte x)` | `t -> i8 \| ToByte t` | 2 | checked: traps when the value does not fit; `(trunc i8 x)` wraps |
| `short` | adapt | `(short x)` | `t -> i16 \| ToShort t` | 2 | checked |
| `int` | adapt | `(int x)` | `t -> i32 \| ToInt t` | 2 | checked; floats truncate toward zero and trap out of range |
| `long` | adapt | `(long x)` | `t -> i64 \| ToLong t` | 2 | checked; `(sext i64 x)` widens without a check |
| `float` | adapt | `(float x)` | `t -> f32 \| ToFloat t` | 2 |  |
| `double` | adapt | `(double x)` | `t -> f64 \| ToDouble t` | 2 |  |
| `bit-and-not` | keep | `(bit-and-not a b)` | `t t -> t \| Bits t` | 3 | library over `bit-and` and `bit-not` |
| `bit-clear` | keep | `(bit-clear a n)` | `t t -> t \| Bits t` | 3 | library |
| `bit-set` | keep | `(bit-set a n)` | `t t -> t \| Bits t` | 3 | library |
| `bit-flip` | keep | `(bit-flip a n)` | `t t -> t \| Bits t` | 3 | library |
| `bit-test` | keep | `(bit-test a n)` | `t t -> bool \| Bits t` | 3 | library |
| `rand` | adapt | `(rand rng)` | `Rng -> f64` | 4 | an explicit generator, `fib.random` |
| `rand-int` | adapt | `(rand-int rng n)` | `Rng i64 -> i64` | 4 |  |
| `zero?` | keep | `(zero? x)` | `t -> bool \| Eq t, Unit t` | 1 |  |
| `pos?` | keep | `(pos? x)` | `t -> bool \| Ord t, Unit t` | 1 |  |
| `neg?` | keep | `(neg? x)` | `t -> bool \| Ord t, Unit t` | 1 |  |
| `even?` | keep | `(even? n)` | `t -> bool \| Bits t, Unit t` | 1 | any integer width |
| `odd?` | keep | `(odd? n)` | `t -> bool \| Bits t, Unit t` | 1 |  |
| `NaN?` | keep | `(NaN? x)` | `f64 -> bool` | 3 |  |
| `infinite?` | keep | `(infinite? x)` | `f64 -> bool` | 3 |  |
| `Long/MAX_VALUE` | rename | `i64-max` | `i64` | 1 | a `def`; also `i64-min`, `i32-max`, `i32-min`, ... |
| `Long/MIN_VALUE` | rename | `i64-min` | `i64` | 1 | a `def` |
| `Double/MAX_VALUE` | rename | `f64-max` | `f64` | 3 | a `def`; also `f64-epsilon`, `f64-min-positive`, `f64-inf`, `f64-nan` |
| `identical?` | rename | `(same? a b)` | `a a -> bool` | 3 | pointer equality on objects, value equality on scalars; observes sharing, never equality |
| `compare` | adapt | `(compare a b)` | `t t -> i64 \| Ord t` | 1 | -1, 0 or 1; a total order on `f64` (NaN greatest and equal to itself), so it agrees with `=` except at NaN; consistent with `<` otherwise; lexicographic on collections (§2.7) |
| `hash` | keep | `(hash x)` | `t -> i64 \| Hash t` | 1 | builtin method; instances for collections are library; `f64` hashes `-0.0` as `0.0` (§9 Q7) |
| `hash-combine` | adapt | `(hash-combine h x)` | `i64 i64 -> i64` | 1 | rotate and xor from `shl shr bit-or bit-xor`: never traps, exists today ([R] A10 hash); a multiplicative mixer replaces it with L10 (T3) |
| `hash-ordered-coll` | keep | `(hash-ordered-coll c)` | `c -> i64 \| Reducible c e, Hash e` | 1 | for `Vec` and `List`, over `hash-combine` |
| `hash-unordered-coll` | keep | `(hash-unordered-coll c)` | `c -> i64 \| Reducible c e, Hash e` | 1 | commutative (an xor of the elements' hashes); for `Map` and `Set` |
| `mix-collection-hash` | keep | `(mix-collection-hash h n)` | `i64 i64 -> i64` | 3 | the finaliser |
| `nil?` | keep | `(nil? o)` | `(Option a) -> bool` | 1 |  |
| `some?` | keep | `(some? o)` | `(Option a) -> bool` | 1 |  |
| `identity` | keep | `(identity x)` | `a -> a` | 1 |  |
| `distinct?` | adapt | `(distinct? c)` | `c -> bool \| Reducible c e, Hash e, Eq e` | 3 |  |
| `defn` | rename | `(defun name (x: T ..) -> R body)` | core form | 1 | `defun`; overloaded by arity with `(defun name (:arity (x: T) -> R body) (:arity (x: T y: T) -> R body))`, each clause with its own `-> R` and `:where` (§7 L1) |
| `36rZZ 2r1010` | adapt | `0x1F 0b1010` | reader | 3 | other bases through `(parse-i64 s radix)` |
| `Result` (new) | new | `(defenum (Result a e) (Ok v: a) (Err e: e))` | type | 1 | the prelude gets the `Result` of `compiler/util/result.fib`, which is deleted in the same commit (two types of one name cannot exchange values, §6.2); `(try-let ..)` threads `Err` |
| `unwrap` (new) | new | `(unwrap o)` | `(Option a) -> a` | 1 | traps `unwrap: nil` |
| `unwrap-or` (new) | new | `(unwrap-or o d)` | `(Option a) a -> a` | 1 | in the prelude today |
| `map-opt` (new) | new | `(map-opt f o)` | `(fn (a) b) (Option a) -> (Option b)` | 1 | function first, `Option` last (§3 N11) |
| `and-then` (new) | new | `(and-then f o)` | `(fn (a) (Option b)) (Option a) -> (Option b)` | 1 | the bind of `some->`; function first, as `map-opt` |
| `try-let` (new) | new | `(try-let ((x e) ..) body)` | macro `(Result a e) .. -> (Result b e)` | 2 | binds each `Ok` payload in turn; the first `Err` is the value; a block macro over `match`, because the language has no early return (§2.10, [R] A10 try) |
| `Pair` (new) | new | `(Pair a b)` | struct `(fst: a snd: b)` | 1 | the one tuple type: a `Map` yields it and `conj`s it, `zip` and `juxt` give it, so `(into {} (zip ks vs))` and `(into {} (map (juxt f g) xs))` work ([R] A10 pair); `derive`s `Eq Ord Hash Show`, shows `[a b]` (§2.7); patterns `(Pair a b)` and, once §7 L3b lands, `[a b]` |
| `Triple` (new) | new | `(Triple a b c)` | struct `(fst: a snd: b thd: c)` | 1 | as `Pair` |
| `Step` (new) | new | `(Step a)` | enum `(More v: a) (Done v: a)` | 1 | the step result of `reduce-while` |
| `Unit` (new) | new | `(zero x) (one x)` | protocol `t -> t` | 1 | the witness `inc`, `abs` and `zero?` need, since a literal has one type; static form after §7 L4 |
| `neg` (new) | new | `(neg x)` | `t -> t \| Num t` | 1 | builtin |
| `ipow` (new) | new | `(ipow x n)` | `t i64 -> t \| Num t, Unit t` | 3 | exact integer power; traps on overflow |

### 4.4 `fib.seq` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `seq` | adapt | `(seq c)` | `c -> (Option c) \| Reducible c e` | 1 | `nil` when empty, else `(some c)`: keeps `(if-let (s (seq xs)) ..)`; stops after one element; `(if (seq xs) ..)` is a type error (`cannot unify (Option (Vec i64)) with bool`): the test is `(not (empty? xs))` |
| `vec` | keep | `(vec c)` | `c -> (Vec e) \| Reducible c e` | 1 | the `to-vec` method: a `Vec` argument is returned as is (no allocation, [R] A10 nth), any other source builds into one buffer |
| `list` | keep | `(list a ..)` | macro `a ... -> (List a)` | 1 | prelude macro, kept; expands to `Cons`/`Empty` |
| `list*` | adapt | `(list* a .. l)` | macro `a ... (List a) -> (List a)` | 3 | nested `cons`; the last argument is a `List` |
| `cons` | adapt | `(cons x l)` | `a (List a) -> (List a)` | 2 | a function once the `List` variants are renamed `Empty`/`Cons` (§5 row 22) |
| `hash-map` | adapt | `{k v ..} or (hash-map k v ..)` | macro `k v ... -> (Map k v) \| Hash k, Eq k` | 2 | an odd count is a compile error; duplicate literal keys are an error (§9 Q5) |
| `hash-set` | adapt | `#{a ..} or (hash-set a ..)` | macro `a ... -> (Set a) \| Hash a, Eq a` | 2 | the reader reads `#{..}` as `(hash-set ..)` (§7 E8) |
| `range` | adapt | `(range n) (range a b) (range a b s)` | `i64 -> Range; i64 i64 -> Range; i64 i64 i64 -> Range` | 1 | overloaded by arity (§7 L1); exact; step 0 traps; `(range)` is `(iterate inc 0)`; floats: `frange` |
| `repeat` | adapt | `(repeat x) (repeat n x)` | `a -> (Repeat a); i64 a -> (Taken (Repeat a) a)` | 2 | overloaded by arity |
| `repeatedly` | adapt | `(repeatedly f) (repeatedly n f)` | `(fn () a) -> (Repeatedly a); i64 (fn () a) -> (Taken (Repeatedly a) a)` | 3 | `f` runs once per element produced, at each traversal |
| `iterate` | keep | `(iterate f x)` | `(fn (a) a) a -> (Iterate a)` | 2 | infinite source; a consumer that stops ends it |
| `cycle` | keep | `(cycle c)` | `c -> (Cycle c e) \| Reducible c e` | 2 | an empty source ends at once |
| `lazy-seq` | adapt | `(lazy-seq body)` | macro `(LSeq a) -> (Lazy a)` | 5 | the one memoising sequence, for recursive definitions; `fib.lazy` |
| `concat` | adapt | `(concat a b ..)` | `c1 c2 -> (Cat c1 c2 e) \| Reducible c1 e, Reducible c2 e` | 1 | two-arity function; macro nests for more |
| `interleave` | adapt | `(interleave a b ..)` | `c1 c2 -> (Interleaved c1 c2 e) \| Cursable c1 k1, Cursor k1 e, Cursable c2 k2, Cursor k2 e` | 2 | truncates to the shorter; both operands must be `Cursable` (not an adaptor until L16: `(vec c)` first); `zip-strict` traps on a mismatch |
| `interpose` | keep | `(interpose sep c)` | `e c -> (Interposed c e) \| Reducible c e` | 2 |  |
| `tree-seq` | adapt | `(tree-seq branch? children root)` | `(fn (n) bool) (fn (n) d) n -> (TreeSeq n d) \| Reducible d n` | 3 | typed over one node type; depth first, no explicit stack in the caller |
| `re-seq` | adapt | `(re-seq re s)` | `Regex str -> (Matches)` | 4 | `Reducible Match`; `fib.regex` |
| `line-seq` | adapt | `(lines-of r)` | `Reader -> (Lines)` | 5 | needs a scope-exit hook to close the handle (§7 L12); `fib.io` |
| `iteration` | adapt | `(iteration step init)` | `(fn (k) (Option (Pair v k))) k -> (Iteration k v)` | 5 | `fib.seq` |
| `first` | adapt | `(first c)` | `c -> (Option e) \| Reducible c e` | 1 | `nil` on empty; use `(unwrap (first c))` or `(nth c 0)` for the sure case |
| `second` | adapt | `(second c)` | `c -> (Option e) \| Reducible c e` | 2 |  |
| `last` | adapt | `(last c)` | `c -> (Option e) \| Reducible c e` | 1 | the `last` method: a walk by default, O(1) on `Vec` ([R] A10 nth) and, by the same override in the instance, on `Array Range Slice` |
| `butlast` | adapt | `(butlast c)` | `c -> (DroppedLast c e) \| Reducible c e` | 3 | empty when short, never `nil` |
| `rest` | adapt | `(rest c)` | `c -> (Dropped c e) \| Reducible c e` | 2 | `(drop 1 c)`; empty when empty; recursion on `(rest c)` over a generic `c` is polymorphic recursion and is rejected (§7 B4); recursion walks a `List` or uses `loop` |
| `nth` | adapt | `(nth c i) (nth c i d)` | `c i64 -> e \| Reducible c e; c i64 e -> e` | 1 | the `nth` method: O(1) on `Vec` ([R] A10 nth) and, by the same override, on `Array Range Slice`, a walk on any other source, so `(nth (filter p c) 3)` works (A10 nth); traps out of range as Clojure throws; the 3-arity is `nth-or` until L1 |
| `take` | keep | `(take n c)` | `i64 c -> (Taken c e) \| Reducible c e` | 1 | stops the source at n |
| `take-while` | keep | `(take-while p c)` | `(fn (e) bool) c -> (TakenWhile c e) \| Reducible c e` | 1 |  |
| `take-nth` | adapt | `(take-nth n c)` | `i64 c -> (TakenNth c e) \| Reducible c e` | 3 | n <= 0 traps |
| `take-last` | adapt | `(take-last n c)` | `i64 c -> (Vec e) \| Reducible c e` | 3 | materialises |
| `drop` | keep | `(drop n c)` | `i64 c -> (Dropped c e) \| Reducible c e` | 1 |  |
| `drop-while` | keep | `(drop-while p c)` | `(fn (e) bool) c -> (DroppedWhile c e) \| Reducible c e` | 2 |  |
| `drop-last` | adapt | `(drop-last c) (drop-last n c)` | `c -> (DroppedLast c e); i64 c -> (DroppedLast c e) \| Reducible c e` | 3 | overloaded by arity; holds n elements back |
| `split-at` | adapt | `(split-at n c)` | `i64 c -> (Pair (Vec e) (Vec e)) \| Reducible c e` | 3 | one pass |
| `split-with` | adapt | `(split-with p c)` | `(fn (e) bool) c -> (Pair (Vec e) (Vec e)) \| Reducible c e` | 3 | one pass; `p` runs once per element up to and including the first failure |
| `subvec` | adapt | `(subvec v a b) (subvec v a)` | `(Vec e) i64 i64 -> (Slice e)` | 3 | an O(1) view holding a count on `v`; `(vec (subvec ..))` copies; indexes checked at the call |
| `peek` | adapt | `(peek s)` | `s -> (Option e) \| Stack s e` | 2 | the end `conj` adds at: a `Vec`'s last, a `List`'s first |
| `pop` | adapt | `(pop s)` | `s -> s \| Stack s e` | 2 | traps when empty; in place when unique |
| `rseq` | adapt | `(rseq c)` | `c -> (Reversed c e) \| Reversible c e` | 3 | `Vec`, `Range`, sorted collections |
| `count` | adapt | `(count c)` | `c -> i64 \| Reducible c e` | 1 | `size`: O(1) for `Vec` `Map` `Set` `Array` `Range` `Option` and size-preserving adaptors, a walk otherwise; does not call the function of a `map` (§2.1 rule 4); not defined on `str` (§2.9) |
| `bounded-count` | keep | `(bounded-count n c)` | `i64 c -> i64 \| Reducible c e` | 3 | stops after n elements |
| `empty?` | adapt | `(empty? c)` | `c -> bool \| Reducible c e` | 1 | stops at the first element the recipe produces (§2.1 rule 4) |
| `not-empty` | adapt | `(not-empty c)` | `c -> (Option c) \| Reducible c e` | 2 | same as `seq` |
| `map` | adapt | `(map f c) (map f c1 c2) (map f c1 c2 c3)` | `(fn (e) b) c -> (Mapped c e b) \| Reducible c e; (fn (a b) r) c1 c2 -> (ZipWith c1 c2 a b k r) \| Reducible c1 a, Cursable c2 k, Cursor k b` | 1 | overloaded by arity (L1; `zip-with` until then); returns a re-runnable recipe, not a `Vec`; the second operand is `Cursable`, so a `Vec`, `Range` or `Array`, not an adaptor, until L16; `(map f)` is the transducer |
| `mapv` | adapt | `(mapv f c)` | `(fn (e) b) c -> (Vec b) \| Reducible c e` | 2 | alias of `(vec (map f c))` |
| `mapcat` | adapt | `(mapcat f c)` | `(fn (e) d) c -> (Mapcat c e d b) \| Reducible c e, Reducible d b` | 1 | flattening is free in the push model |
| `map-indexed` | adapt | `(map-indexed f c)` | `(fn (i64 e) b) c -> (MapIndexed c e b) \| Reducible c e` | 2 |  |
| `keep` | adapt | `(keep f c)` | `(fn (e) (Option b)) c -> (Kept c e b) \| Reducible c e` | 2 | `f` returns an `Option`; `nil`s are dropped, `some`s unwrapped |
| `keep-indexed` | adapt | `(keep-indexed f c)` | `(fn (i64 e) (Option b)) c -> (KeptIndexed c e b) \| Reducible c e` | 3 |  |
| `reverse` | adapt | `(reverse c)` | `c -> (Vec e) \| Reducible c e` | 1 | eager; `rseq` is the O(1) view |
| `sort` | adapt | `(sort c) (sort cmp c)` | `c -> (Vec e) \| Reducible c e, Ord e; (fn (e e) i64) c -> (Vec e) \| Reducible c e` | 1 | stable merge sort; one comparator convention: negative, zero, positive; `(sort-with cmp c)` is the 2-arity until L1; sorting a `Vec` copies it once (the `vec` inside is the identity), 2096 objects for 1000 elements against 4190 before ([R] A10 sort) |
| `sort-by` | adapt | `(sort-by key c) (sort-by key cmp c)` | `(fn (e) k) c -> (Vec e) \| Reducible c e, Ord k; (fn (e) k) (fn (k k) i64) c -> (Vec e) \| Reducible c e` | 1 | the key is computed once per element (decorate, sort, undecorate: 5 calls for 5 elements, [R] A10 sortby); `(sort-by val desc m)` is Clojure's `(sort-by val > m)`; `sort-by-with` is the 3-arity until L1 |
| `sort-by-with` (new) | new | `(sort-by-with key cmp c)` | `(fn (e) k) (fn (k k) i64) c -> (Vec e) \| Reducible c e` | 1 | the 3-arity of `sort-by` until L1; stable; `key` is stored, so it is not `:borrow` |
| `desc` (new) | new | `(desc a b)` | `t t -> i64 \| Ord t` | 1 | `(compare b a)`: the descending comparator, a function value (`(sort-with desc xs)`); `compare` is the ascending one |
| `shuffle` | adapt | `(shuffle rng c)` | `Rng c -> (Vec e) \| Reducible c e` | 4 | `fib.random`; no global state |
| `distinct` | keep | `(distinct c)` | `c -> (Distinct c e) \| Reducible c e, Hash e, Eq e` | 2 | first occurrences, in order |
| `dedupe` | keep | `(dedupe c)` | `c -> (Deduped c e) \| Reducible c e, Eq e` | 3 |  |
| `replace` | adapt | `(replace smap c)` | `(Map e e) c -> (Replaced c e) \| Reducible c e, Hash e, Eq e` | 3 | the vector-as-map case is dropped |
| `partition` | adapt | `(partition n c) (partition n step c) (partition n step pad c)` | `i64 c -> (Partitioned c e); i64 i64 c -> (Partitioned c e); i64 i64 d c -> (Partitioned c e) \| Reducible c e, Reducible d e` | 2 | yields `(Vec e)`; drops an incomplete last group, as Clojure's does (`partition-all` keeps it) |
| `partition-all` | adapt | `(partition-all n c) (partition-all n step c)` | `i64 c -> (Partitioned c e); i64 i64 c -> (Partitioned c e) \| Reducible c e` | 2 | yields `(Vec e)` |
| `partition-by` | keep | `(partition-by f c)` | `(fn (e) k) c -> (PartitionedBy c e k) \| Reducible c e, Eq k` | 2 | yields `(Vec e)` |
| `group-by` | adapt | `(group-by f c)` | `(fn (e) k) c -> (Map k (Vec e)) \| Reducible c e, Hash k, Eq k` | 1 | one pass; each group in encounter order |
| `frequencies` | keep | `(frequencies c)` | `c -> (Map e i64) \| Reducible c e, Hash e, Eq e` | 1 |  |
| `zipmap` | keep | `(zipmap ks vs)` | `c1 c2 -> (Map k v) \| Reducible c1 k, Cursable c2 k2, Cursor k2 v, Hash k, Eq k` | 2 | truncates to the shorter |
| `reduce` | adapt | `(reduce f init c) (reduce f c)` | `(fn (a e) a) a c -> a \| Reducible c e; (fn (e e) e) c -> (Option e) \| Reducible c e` | 1 | the 2-arity has no initial value, so an empty `c` gives `nil` (`reduce1` of the prototype) |
| `reduce-kv` | keep | `(reduce-kv f init m)` | `(fn (a k v) a) a m -> a \| KeyReducible m k v` | 2 | `Map` (entries) and `Vec` (index, element); collection last (§3 N3); the walk passes `k` and `v` as two arguments, so it builds no `Pair` |
| `reductions` | adapt | `(reductions f init c) (reductions f c)` | `(fn (a e) a) a c -> (Reductions c e a) \| Reducible c e` | 2 | emits `init` first; the 2-arity seeds with the first element |
| `transduce` | adapt | `(transduce xf f init c)` | `Xf (fn (a b) a) a c -> a` | 3 | transducers are values of `(Xf a b)`, a factory of steppers with a flush (§2.1) |
| `xf` (new) | new | `(xf t1 t2 ..)` | macro `(Xf a b) (Xf b c) -> (Xf a c)` | 3 | composes transducers left to right, `(xf-comp x y)` nested; `comp` stays function composition (§5 row 44) |
| `run!` | keep | `(run! f c)` | `(fn (e) r) c -> unit \| Reducible c e` | 1 | replaces the prelude's `for-each`; `f` may return any `r`, which is dropped (monomorphised, no cost): `(run! (fn (x) (swap! a + x)) xs)` is 6 under both tools ([R] A10 run); over a literal `(range a b)` and a literal one-parameter `fn` it expands to the counting loop that the prelude's `for-each` macro makes today (§6.3) |
| `some` | rename | `(any? p c) (find-map f c)` | `(fn (e) bool) c -> bool; (fn (e) (Option b)) c -> (Option b)` | 1 | `some` is `Option`'s constructor; `find-map` is the value form |
| `every?` | keep | `(every? p c)` | `(fn (e) bool) c -> bool \| Reducible c e` | 1 |  |
| `not-any?` | rename | `(none? p c)` | `(fn (e) bool) c -> bool \| Reducible c e` | 1 |  |
| `not-every?` | keep | `(not-every? p c)` | `(fn (e) bool) c -> bool \| Reducible c e` | 2 |  |
| `filter` | adapt | `(filter p c)` | `(fn (e) bool) c -> (Filtered c e) \| Reducible c e` | 1 | `p` returns `bool` |
| `filterv` | adapt | `(filterv p c)` | `(fn (e) bool) c -> (Vec e) \| Reducible c e` | 2 | alias of `(vec (filter p c))` |
| `remove` | keep | `(remove p c)` | `(fn (e) bool) c -> (Filtered c e) \| Reducible c e` | 1 |  |
| `max-key` | adapt | `(max-key k c)` | `(fn (e) k) c -> (Option e) \| Reducible c e, Ord k` | 2 | ties: the last wins, as Clojure's; `min-key` the mirror |
| `min-key` | adapt | `(min-key k c)` | `(fn (e) k) c -> (Option e) \| Reducible c e, Ord k` | 2 | ties: the last wins, as Clojure's |
| `rand-nth` | adapt | `(rand-nth rng c)` | `Rng c -> (Option e) \| Reducible c e` | 4 | `fib.random` |
| `random-sample` | adapt | `(random-sample rng p c)` | `Rng f64 c -> (Sampled c e) \| Reducible c e` | 4 |  |
| `sequence` | adapt | `(sequence xf c)` | `Xf c -> (XfApplied xf c)` | 5 |  |
| `eduction` | adapt | `(eduction xf c)` | `Xf c -> (XfApplied xf c)` | 5 |  |
| `any?` | adapt | `(any? p c)` | `(fn (e) bool) c -> bool \| Reducible c e` | 1 | takes a predicate (Clojure's `any?` is constantly true) |
| `reduced` | adapt | `(reduced x)` | `a -> (Step a)` | 1 | `(Done x)`; `reduce-while` reads it |
| `find-first` (new) | new | `(find-first p c)` | `(fn (e) bool) c -> (Option e) \| Reducible c e` | 1 | the value form of `any?`; not a Clojure name |
| `zip` (new) | new | `(zip a b)` | `c1 c2 -> (Zipped c1 c2) \| Reducible c1 x, Cursable c2 k, Cursor k y` | 2 | yields `(Pair x y)` (one object per element until C5); truncates to the shorter; the second operand is `Cursable` |
| `zip-with` (new) | new | `(zip-with f a b)` | `(fn (x y) r) c1 c2 -> (ZipWith ..)` | 2 | the 3-arity of `map` until L1, then deleted; no `Pair` per element: +4 objects over 1000 elements ([R] A10 alloc) |
| `seq-of` (new) | new | `(seq-of c)` | `c -> (dyn (Reducible e)) \| Reducible c e` | 2 | the type-erased source for a join (`(if flag (seq-of (filter p v)) (seq-of v))`, [R] A10 join) and for a field of unknown source type; one heap object and an indirect call per visit |
| `seq=` (new) | new | `(seq= a b)` | `c1 c2 -> bool \| Reducible c1 e, Reducible c2 e, Eq e` | 2 | equality across `Reducible`s: `(seq= (map inc [1 2]) [2 3])` is true ([R] A10 showseq); `=` is on one type |
| `zip-strict` (new) | new | `(zip-strict a b)` | `c1 c2 -> (Zipped c1 c2)` | 3 | traps on a length mismatch |
| `sum` (new) | new | `(sum c)` | `c -> e \| Reducible c e, Num e, Unit e` | 1 | `(reduce + 0 c)`; needs a static `zero` for an empty generic `e` (§7 L4); `i64` until then |
| `product` (new) | new | `(product c)` | `c -> e \| Reducible c e, Num e, Unit e` | 2 |  |
| `reduce-while` (new) | new | `(reduce-while f init c)` | `(fn (a e) (Step a)) a c -> a \| Reducible c e` | 1 | Clojure's `reduced`; boxed `Step` allocates per step until §7 C5 |
| `frange` (new) | new | `(frange a b s)` | `f64 f64 f64 -> (FRange)` | 3 | `a + i*s` for `i < ceil((b-a)/s)`, no accumulation; step 0 traps |
| `nth-or` (new) | new | `(nth-or c i d)` | `c i64 e -> e \| Reducible c e` | 1 | the 3-arity of `nth` until L1, then deleted |
| `reduce1` (new) | new | `(reduce1 f c)` | `(fn (e e) e) c -> (Option e) \| Reducible c e` | 1 | the 2-arity of `reduce` until L1, then deleted; `(unwrap (reduce1 max xs))` is Clojure's `(apply max xs)` |
| `sort-with` (new) | new | `(sort-with cmp c)` | `(fn (e e) i64) c -> (Vec e) \| Reducible c e` | 1 | the comparator form of `sort` (until L1, then deleted); stable |
| `range-by` (new) | new | `(range-by a b s)` | `i64 i64 i64 -> Range` | 1 | the 3-arity of `range` until L1, then deleted; `(range a b)` is today's macro |

### 4.5 `fib.coll` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `find` | adapt | `(find m k)` | `s k -> (Option (Pair k v)) \| Lookup s k v` | 2 | returns the entry, a `Pair` |
| `select-keys` | keep | `(select-keys m ks)` | `s c -> s \| Lookup s k v, Assoc s k v, Emptyable s, Reducible c k` | 1 | absent keys are skipped |
| `conj` | adapt | `(conj c x ..)` | `c e -> c \| Collection c e` | 1 | one rule per type: `List` front, `Vec` end, `Set` anywhere, `Map` takes a `Pair`; `(conj nil x)` is a type error; macro for more than one `x` |
| `assoc` | adapt | `(assoc m k v ..)` | `s k v -> s \| Assoc s k v` | 1 | `Map` and `Vec` (index below the count; else traps); macro for more pairs |
| `dissoc` | keep | `(dissoc m k ..)` | `s k -> s \| Dissoc s k` | 1 | `Map` and `Set`; macro for more keys |
| `get` | adapt | `(get c k) (get c k d)` | `s k -> (Option v) \| Lookup s k v; s k v -> v \| Lookup s k v` | 1 | absent and `nil`-valued differ by construction; overloaded by arity |
| `get-in` | adapt | `(get-in m [k1 k2 ..]) (get-in m [k1 ..] d)` | macro `m k1 .. -> (Option v); m k1 .. v -> v` | 3 | the path is a literal vector, one `get` per level, so each level has its own key type |
| `assoc-in` | adapt | `(assoc-in m [k1 k2 ..] v)` | macro `m k1 .. v -> m \| Lookup, Assoc, Emptyable per level` | 3 | a missing level is `(empty ..)` of the level's type; needs static `empty` (§7 L4) |
| `update` | adapt | `(update m k f)` | `s k (fn (v) v) -> s \| Lookup s k v, Assoc s k v` | 1 | `f` sees the value; a missing key traps `update: no key`, as Clojure's NPE; `(update m k f arg ..)` is a macro form for extra arguments; `(update m k inc)` compiles, where the first version of this page rejected it (`cannot unify (fn :send (i64) i64) with (fn ((Option a)) a)`); the three-argument form `(update m k f d)` is `update-or` until L1 ([R] A10 update) |
| `update-or` (new) | new | `(update-or m k f d)` | `s k (fn (v) v) v -> s \| Lookup s k v, Assoc s k v` | 1 | `f` sees `d` when the key is missing: Clojure's `(update m w (fnil inc 0))` is `(update-or m w inc 0)`; deleted when L1 lands (the 4-arity of `update`) |
| `update-opt` (new) | new | `(update-opt m k f)` | `s k (fn ((Option v)) v) -> s \| Lookup s k v, Assoc s k v` | 1 | `f` sees `(Option v)` as Clojure's sees `nil`: `(update-opt m w (fnil inc 0))` is the literal port |
| `update-in` | adapt | `(update-in m [k1 ..] f)` | macro `m k1 .. (fn (v) v) -> m` | 3 | typed per level like `get-in`; `update` at each level, so a missing key traps; `assoc-in` creates levels; the empty path is a compile error |
| `update-keys` | adapt | `(update-keys m f)` | `(Map k v) (fn (k) k2) -> (Map k2 v) \| Hash k2, Eq k2` | 3 | two keys mapping to one trap `update-keys: duplicate key` |
| `update-vals` | keep | `(update-vals m f)` | `(Map k v) (fn (v) w) -> (Map k w) \| Hash k, Eq k` | 3 | the value type may change |
| `contains?` | adapt | `(contains? c k)` | `s k -> bool \| Keyed s k` | 1 | `Map` (key) and `Set` (member) only; a `Vec` has `(some? (get v i))` and `includes?` |
| `into` | adapt | `(into to c) (into to xf c)` | `t c -> t \| Collection t e, Reducible c e` | 1 | the 3-arity takes a transducer (tranche 5); a bulk path builds in one buffer |
| `empty` | adapt | `(empty c)` | `s -> s \| Emptyable s` | 2 | `Vec`, `Map`, `Set`, `List`; takes a value, so no static method is needed |
| `merge` | adapt | `(merge m1 m2 ..)` | `(Map k v) (Map k v) -> (Map k v) \| Hash k, Eq k` | 1 | macro nests for more; never `nil` |
| `merge-with` | adapt | `(merge-with f m1 m2 ..)` | `(fn (v v) v) (Map k v) (Map k v) -> (Map k v) \| Hash k, Eq k` | 2 | `f` combines a conflict (old, new) |
| `keys` | adapt | `(keys m)` | `m -> (Mapped m (Pair k v) k) \| Reducible m (Pair k v)` | 1 | an empty recipe, not `nil` |
| `vals` | adapt | `(vals m)` | `m -> (Mapped m (Pair k v) v) \| Reducible m (Pair k v)` | 1 |  |
| `key` | keep | `(key p)` | `(Pair k v) -> k` | 1 | the map element is a `Pair`; `key` and `val` read it (a function, not a field: `(. p fst)`) |
| `val` | keep | `(val p)` | `(Pair k v) -> v` | 1 |  |
| `disj` | keep | `(disj s x ..)` | `s k -> s \| Dissoc s k` | 1 |  |
| `set` | keep | `(set c)` | `c -> (Set e) \| Reducible c e, Hash e, Eq e` | 1 |  |
| `includes?` (new) | new | `(includes? x c)` | `e c -> bool \| Reducible c e, Eq e` | 1 | element test for a `Vec`, `List`, or any source; element first, so `->>` threads the source (§3 N3) |
| `Slice` (new) | new | `(Slice e)` | struct `(v: (Vec e) lo: i64 hi: i64)` | 3 | the O(1) view `subvec` returns; `Reducible` (with an O(1) `nth`), `Lookup` |
| `get-or` (new) | new | `(get-or c k d)` | `s k v -> v \| Lookup s k v` | 1 | the 3-arity of `get` until L1 (§7), then deleted; `(get m k 0)` is `get takes 2 argument(s), got 3` until then (the error says `use get-or`, §7 D1) |

### 4.6 `fib.sorted`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `subseq` | adapt | `(subseq sc test key)` | `s Cmp k -> (Range) \| Sorted s k v` | 5 | `Cmp` is `(defenum Cmp Lt Le Gt Ge)`, not a function value |
| `rsubseq` | adapt | `(rsubseq sc test key)` | `s Cmp k -> (Reversed ..) \| Sorted s k v` | 5 |  |
| `sorted-map` | adapt | `(sorted-map k v ..)` | macro `k v ... -> (SortedMap k v) \| Ord k` | 3 | a B-tree; one key type, so no run-time failure |
| `sorted-map-by` | rename | `(sorted-map-with cmp k v ..)` | `(fn (k k) i64) k v ... -> (SortedMap k v)` | 3 | `-with` takes the comparator |
| `sorted-set` | adapt | `(sorted-set a ..)` | macro `a ... -> (SortedSet a) \| Ord a` | 3 | a B-tree |
| `sorted-set-by` | rename | `(sorted-set-with cmp a ..)` | `(fn (a a) i64) a ... -> (SortedSet a)` | 3 | "equal under the order" means the same element |
| `comparator` | adapt | `(comparator less?)` | `(fn (a a) bool) -> (fn (a a) i64)` | 3 |  |
| `clojure.lang.PersistentQueue/EMPTY` | adapt | `(queue)` | `-> (Queue a)` | 3 | `conj` at the back, `peek` and `pop` at the front; `Collection`, `Stack`, `Reducible` |

### 4.7 `fib.string` (alias `str`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.string/blank?` | keep | `(str/blank? s)` | `str -> bool` | 1 | empty or whitespace only |
| `clojure.string/capitalize` | keep | `(str/capitalize s)` | `str -> str` | 4 |  |
| `clojure.string/ends-with?` | keep | `(str/ends-with? s suffix)` | `str str -> bool` | 1 |  |
| `clojure.string/escape` | adapt | `(str/escape s f)` | `str (fn (char) (Option str)) -> str` | 4 |  |
| `clojure.string/includes?` | keep | `(str/includes? s sub)` | `str str -> bool` | 1 |  |
| `clojure.string/index-of` | adapt | `(str/index-of s sub) (str/index-of s sub from)` | `str str -> (Option i64); str str i64 -> (Option i64)` | 1 | byte offset; never -1 |
| `clojure.string/join` | adapt | `(str/join c) (str/join sep c)` | `c -> str \| Reducible c e, Show e; str c -> str \| Reducible c e, Show e` | 1 | always qualified: an unqualified `join` is the task-wait builtin |
| `clojure.string/last-index-of` | adapt | `(str/last-index-of s sub)` | `str str -> (Option i64)` | 3 | byte offset |
| `clojure.string/lower-case` | keep | `(str/lower-case s)` | `str -> str` | 4 | Unicode full case mapping, locale independent |
| `clojure.string/replace` | adapt | `(str/replace s from to)` | `str str str -> str` | 3 | literal to literal; `replace-re` and `replace-with` are the other two |
| `clojure.string/replace-first` | adapt | `(str/replace-first s from to)` | `str str str -> str` | 3 |  |
| `clojure.string/reverse` | adapt | `(str/reverse s)` | `str -> str` | 4 | by Unicode scalar value, not by grapheme |
| `clojure.string/split` | adapt | `(str/split s sep) (str/split s sep limit)` | `str str -> (Vec str); str str i64 -> (Vec str)` | 1 | keeps trailing empties (round-trips with `join`); `sep` is a literal string, `split-re` takes a `Regex` |
| `clojure.string/split-lines` | keep | `(str/split-lines s)` | `str -> (Vec str)` | 1 | splits at LF and CRLF |
| `clojure.string/starts-with?` | keep | `(str/starts-with? s prefix)` | `str str -> bool` | 1 | the prelude builtin `starts-with?`, re-exported: one function with two names for the 2 cases that use it, not two implementations |
| `clojure.string/trim` | keep | `(str/trim s)` | `str -> str` | 1 | Unicode whitespace |
| `clojure.string/trim-newline` | keep | `(str/trim-newline s)` | `str -> str` | 3 |  |
| `clojure.string/triml` | keep | `(str/triml s)` | `str -> str` | 1 |  |
| `clojure.string/trimr` | keep | `(str/trimr s)` | `str -> str` | 1 |  |
| `clojure.string/upper-case` | keep | `(str/upper-case s)` | `str -> str` | 4 | Unicode full case mapping |
| `chars` (new) | new | `(str/chars s)` | `str -> Chars` | 1 | a `Reducible char` view, a decoder over one byte array (2 objects for 1000 characters, [R] A10 chars); `str` is not a collection, so `count` of a `str` is a type error |
| `str-len` (new) | new | `(str-len s)` | `str -> i64` | 1 | builtin: bytes, O(1) |
| `str-byte-at` (new) | new | `(str-byte-at s i)` | `str i64 -> i8` | 1 | builtin (§7 L18): one byte, no allocation, traps out of range; `str-bytes` allocates an array per call |
| `str-find` (new) | new | `(str-find s pat from)` | `str str i64 -> (Option i64)` | 1 | builtin (§7 L18): the byte offset of the first match at or after `from`, no allocation; `str/index-of` is built on it |
| `split-re` (new) | new | `(str/split-re s re)` | `str Regex -> (Vec str)` | 4 |  |
| `replace-re` (new) | new | `(str/replace-re s re template)` | `str Regex str -> str` | 4 | `$n` in the template |
| `replace-with` (new) | new | `(str/replace-with s re f)` | `str Regex (fn (Match) str) -> str` | 4 |  |

### 4.8 `fib.char`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `Character/isDigit` | rename | `(digit? c)` | `char -> bool` | 1 | ASCII digits |
| `Character/isLetter` | rename | `(letter? c)` | `char -> bool` | 4 | Unicode letters (needs a table) |
| `Character/isLetterOrDigit` | rename | `(alphanumeric? c)` | `char -> bool` | 4 |  |
| `Character/isWhitespace` | rename | `(whitespace? c)` | `char -> bool` | 1 | the Unicode `White_Space` property |
| `Character/isUpperCase` | rename | `(upper? c)` | `char -> bool` | 4 |  |
| `Character/isLowerCase` | rename | `(lower? c)` | `char -> bool` | 4 |  |
| `Character/toUpperCase` | rename | `(upper c)` | `char -> char` | 4 | the simple one-to-one mapping |
| `Character/toLowerCase` | rename | `(lower c)` | `char -> char` | 4 |  |
| `Character/digit` | rename | `(digit-value c radix)` | `char i64 -> (Option i64)` | 3 |  |
| `Character/forDigit` | rename | `(digit-char n radix)` | `i64 i64 -> (Option char)` | 3 |  |

### 4.9 `fib.regex`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `re-pattern` | adapt | `(regex s)` | `str -> (Result Regex str)` | 4 | a non-backtracking engine, so no pattern can hang a call |
| `#"regex"` | adapt | `(re "..")` | macro `str -> Regex` | 4 | checked at compile time; no reader literal (§9 Q9) |
| `re-find` | adapt | `(re-find re s)` | `Regex str -> (Option Match)` | 4 | `Match` has `whole`, `groups` (`(Vec (Option str))`) and byte offsets, whatever the pattern |
| `re-matches` | adapt | `(re-matches re s)` | `Regex str -> (Option Match)` | 4 | the whole string must match |

### 4.10 `fib.math` (alias `math`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.math/E` | keep | `math/e` | `f64` | 3 | a `def` |
| `clojure.math/PI` | keep | `math/pi` | `f64` | 3 | a `def` |
| `clojure.math/sqrt` | keep | `(math/sqrt x)` | `f64 -> f64` | 3 | IEEE-exact, so a builtin both tools agree on (§7 L11); `f32` too |
| `clojure.math/cbrt` | keep | `(math/cbrt x)` | `f64 -> f64` | 4 | written in fibber over `f64->bits`, so the interpreter and the compiler agree bit for bit |
| `clojure.math/pow` | keep | `(math/pow x y)` | `f64 f64 -> f64` | 4 | in fibber (fdlibm-style); `ipow` is the exact integer power |
| `clojure.math/exp` | keep | `(math/exp x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/expm1` | keep | `(math/expm1 x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/log` | keep | `(math/log x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/log10` | keep | `(math/log10 x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/log1p` | keep | `(math/log1p x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/sin` | keep | `(math/sin x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/cos` | keep | `(math/cos x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/tan` | keep | `(math/tan x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/asin` | keep | `(math/asin x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/acos` | keep | `(math/acos x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/atan` | keep | `(math/atan x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/atan2` | keep | `(math/atan2 y x)` | `f64 f64 -> f64` | 4 | Clojure's argument order (y first) |
| `clojure.math/sinh` | keep | `(math/sinh x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/cosh` | keep | `(math/cosh x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/tanh` | keep | `(math/tanh x)` | `f64 -> f64` | 4 | in fibber |
| `clojure.math/hypot` | keep | `(math/hypot x y)` | `f64 f64 -> f64` | 4 | in fibber |
| `clojure.math/floor` | keep | `(math/floor x)` | `f64 -> f64` | 3 | exact; a builtin candidate; `floor-i64` returns an integer |
| `clojure.math/ceil` | keep | `(math/ceil x)` | `f64 -> f64` | 3 | exact |
| `clojure.math/rint` | keep | `(math/rint x)` | `f64 -> f64` | 3 | half to even |
| `clojure.math/round` | adapt | `(math/round x)` | `f64 -> i64` | 3 | half up (toward positive infinity) as Java's, saturating; NaN is 0 |
| `clojure.math/signum` | keep | `(math/signum x)` | `f64 -> f64` | 3 |  |
| `clojure.math/to-degrees` | keep | `(math/to-degrees x)` | `f64 -> f64` | 4 |  |
| `clojure.math/to-radians` | keep | `(math/to-radians x)` | `f64 -> f64` | 4 |  |
| `clojure.math/floor-div` | keep | `(math/floor-div a b)` | `t t -> t \| Bits t, Num t` | 3 | traps on zero and on min / -1 |
| `clojure.math/floor-mod` | keep | `(mod a b)` | `t t -> t \| Num t, Ord t, Unit t` | 3 | alias of `mod` |
| `clojure.math/to-int-exact` | adapt | `(int x)` | `i64 -> i32` | 2 | the checked narrowing |
| `clojure.math/copy-sign` | keep | `(math/copy-sign x s)` | `f64 f64 -> f64` | 4 | from the bit casts |
| `clojure.math/get-exponent` | keep | `(math/get-exponent x)` | `f64 -> i64` | 4 | from `f64->bits` |
| `clojure.math/ulp` | keep | `(math/ulp x)` | `f64 -> f64` | 4 |  |
| `clojure.math/next-after` | keep | `(math/next-after x d)` | `f64 f64 -> f64` | 4 |  |
| `clojure.math/next-up` | keep | `(math/next-up x)` | `f64 -> f64` | 4 |  |
| `clojure.math/next-down` | keep | `(math/next-down x)` | `f64 -> f64` | 4 |  |
| `clojure.math/scalb` | keep | `(math/scalb x n)` | `f64 i64 -> f64` | 4 |  |
| `clojure.math/IEEE-remainder` | keep | `(math/ieee-remainder x y)` | `f64 f64 -> f64` | 4 |  |
| `clojure.math/random` | adapt | `(rand rng)` | `Rng -> f64` | 4 | takes an `Rng`; see `rand` |

### 4.11 `fib.set` (alias `set`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.set/union` | adapt | `(set/union a b)` | `(Set e) (Set e) -> (Set e) \| Hash e, Eq e` | 3 | the larger set is extended; macro nests for more |
| `clojure.set/intersection` | adapt | `(set/intersection a b)` | `(Set e) (Set e) -> (Set e) \| Hash e, Eq e` | 3 |  |
| `clojure.set/difference` | adapt | `(set/difference a b)` | `(Set e) (Set e) -> (Set e) \| Hash e, Eq e` | 3 |  |
| `clojure.set/select` | adapt | `(set/select p s)` | `(fn (e) bool) (Set e) -> (Set e) \| Hash e, Eq e` | 3 | `(set (filter p s))` is the general form |
| `clojure.set/project` | adapt | `(set/project xrel ks)` | `(Set (Map k v)) c -> (Set (Map k v)) \| Reducible c k` | 5 | needs `Hash (Map k v)` |
| `clojure.set/rename-keys` | adapt | `(set/rename-keys m kmap)` | `(Map k v) (Map k k) -> (Map k v) \| Hash k, Eq k` | 5 | a collision traps |
| `clojure.set/rename` | adapt | `(set/rename xrel kmap)` | `(Set (Map k v)) (Map k k) -> (Set (Map k v))` | 5 |  |
| `clojure.set/index` | adapt | `(set/index xrel ks)` | `(Set (Map k v)) c -> (Map (Map k v) (Set (Map k v)))` | 5 |  |
| `clojure.set/map-invert` | adapt | `(set/map-invert m)` | `(Map k v) -> (Map v k) \| Hash v, Eq v` | 3 | a collision traps |
| `clojure.set/join` | adapt | `(set/join xrel yrel)` | `(Set (Map k v)) (Set (Map k v)) -> (Set (Map k v))` | 5 |  |
| `clojure.set/subset?` | keep | `(set/subset? a b)` | `(Set e) (Set e) -> bool \| Hash e, Eq e` | 3 |  |
| `clojure.set/superset?` | keep | `(set/superset? a b)` | `(Set e) (Set e) -> bool \| Hash e, Eq e` | 3 |  |

### 4.12 `fib.walk` (alias `walk`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.walk/walk` | adapt | `(walk/walk inner outer form)` | `(fn (n) n) (fn (n) n) n -> n \| Walkable n` | 4 | one node type per call; `Form` and any user tree |
| `clojure.walk/postwalk` | adapt | `(walk/postwalk f form)` | `(fn (n) n) n -> n \| Walkable n` | 4 |  |
| `clojure.walk/prewalk` | adapt | `(walk/prewalk f form)` | `(fn (n) n) n -> n \| Walkable n` | 4 |  |
| `clojure.walk/keywordize-keys` | adapt | `(walk/keywordize-keys m)` | `(Map str v) -> (Map keyword v)` | 4 | one level |
| `clojure.walk/stringify-keys` | adapt | `(walk/stringify-keys m)` | `(Map keyword v) -> (Map str v)` | 4 | one level |
| `clojure.walk/prewalk-replace` | adapt | `(walk/prewalk-replace smap form)` | `(Map Form Form) Form -> Form` | 4 | on `Form` |
| `clojure.walk/postwalk-replace` | adapt | `(walk/postwalk-replace smap form)` | `(Map Form Form) Form -> Form` | 4 |  |

### 4.13 `fib.data` (alias `data`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.data/diff` | adapt | `(data/diff a b)` | `t t -> (Diff t) \| Diffable t` | 5 | a `Diff` struct (only-a, only-b, both), not a triple; for `Map`, `Set`, `Vec` |

### 4.14 `fib.print` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `println` | adapt | `(println a ..)` | macro `a ... -> unit \| Show a` | 1 | one `Show` argument or several, separated by a space ([R] A10 pm); in value position it is a function generic over `Show`, as are `str print prn pr` (`(run! println xs)`, `(map str xs)`, [R] A10 twin) |
| `print` | adapt | `(print a ..)` | macro `a ... -> unit \| Show a` | 1 | no newline |
| `pr` | adapt | `(pr a ..)` | macro `a ... -> unit \| Debug a` | 2 | strings quoted and escaped |
| `prn` | adapt | `(prn a ..)` | macro `a ... -> unit \| Debug a` | 1 | `pr` and a newline |
| `print-str` | rename | `(str a ..)` | - | 1 | `str` over `Show` |
| `pr-str` | adapt | `(debug-str a ..)` | macro `a ... -> str \| Debug a` | 2 |  |
| `flush` | keep | `(flush)` | `-> unit` | 4 | the prelude writes unbuffered, so a no-op until buffering exists |
| `printf` | adapt | `(printf "fmt" a ..)` | macro `-> unit` | 4 | the directives are checked against the argument types at compile time |
| `format` | adapt | `(format "fmt" a ..)` | macro `-> str` | 4 | `%s` over `Show`, `%d` an integer, `%f` a float, `%x`; the format string must be a literal; locale independent |
| `pprint` | adapt | `(pprint x)` | `a -> unit \| Pretty a` | 5 | long tail |
| `read-line` | adapt | `(read-line)` | `-> (Option str)` | 4 | `nil` at end of input; `fib.io` |
| `slurp` | rename | `(read-file path)` | `str -> (Option str)` | 1 | builtin |
| `spit` | rename | `(write-file path s)` | `str str -> bool` | 1 | builtin, exists today |

### 4.15 `fib.async`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `swap-vals!` | adapt | `(swap-vals! a f)` | `(Atom a) (fn (a) a) -> (Pair a a)` | 4 | old and new |
| `reset-vals!` | adapt | `(reset-vals! a v)` | `(Atom a) a -> (Pair a a)` | 4 |  |
| `compare-and-set!` | adapt | `(compare-and-set! a old new)` | `(Atom a) a a -> bool \| Eq a` | 4 | compares by value, objects included, through `Eq`; `same?` observes identity |
| `delay` | adapt | `(delay e)` | macro `a -> (Delay a)` | 4 | a closure and a `Cell`; forced once |
| `force` | adapt | `(force d)` | `(Delay a) -> a` | 4 |  |
| `promise` | adapt | `(promise)` | `-> (Promise a) \| Send a` | 5 | an `Atom` and a wait; needs a blocking wait primitive |
| `deliver` | adapt | `(deliver p v)` | `(Promise a) a -> bool` | 5 | true when this call delivered |
| `future` | rename | `(spawn (fn () body))` | `(fn :send () a) -> (Task a)` | 1 | builtin; read with `(join t)` |
| `future-call` | rename | `(spawn f)` | `(fn :send () a) -> (Task a)` | 1 | builtin |
| `future-done?` | adapt | `(done? t)` | `(Task a) -> bool` | 5 |  |
| `pmap` | keep | `(pmap f c)` | `(fn :send (a) b) c -> (Vec b) \| Reducible c a` | 1 | eager; one task per element until a pool exists |

### 4.16 `fib.sys`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `Thread/sleep` | rename | `(sleep ms)` | `i64 -> unit` | 4 |  |
| `System/currentTimeMillis` | rename | `(now-ms)` | `-> i64` | 4 |  |
| `System/nanoTime` | rename | `(nano-time)` | `-> i64` | 4 | monotonic |
| `System/getenv` | rename | `(getenv name)` | `str -> (Option str)` | 4 |  |
| `System/exit` | rename | `(exit n)` | `i64 -> a` | 4 | or return `n` from `main` |
| `*command-line-args*` | rename | `(args)` | `-> (Vec str)` | 1 | builtin |
| `clojure-version` | rename | `(version)` | `-> str` | 4 |  |

### 4.17 Not offered

| Clojure name | Verdict | Instead | Reason |
|---|---|---|---|
| `vector` | replace | `[a b ..]` | the literal; `(map vector xs ys)` is `(zip xs ys)` |
| `vector-of` | omit | - | `(Vec i64)` is already unboxed (M7 rule 4); the element type is part of the type |
| `array-map` | omit | - | do not offer an order-preserving map that silently stops preserving; offer an explicit insertion-ordered map (`OrderedMap`) if wanted |
| `lazy-cat` | omit | - | `concat` over adaptors is already lazy |
| `file-seq` | omit | - | file-system walking belongs to an I/O library, not the sequence core |
| `iterator-seq` | omit | - | Java interop; fibber's `Reducible` protocol is the native thing |
| `enumeration-seq` | omit | - | Java interop |
| `xml-seq` | omit | - | dynamic xml map trees |
| `resultset-seq` | omit | - | Java interop |
| `seque` | omit | - | concurrency belongs to channels and `spawn`, not to sequences |
| `ffirst` | omit | - | needs nested sequential element types; `(first (first x))` spelled out |
| `fnext` | omit | - | alias of `second` |
| `nfirst` | omit | - | spell it out |
| `nnext` | omit | - | spell it out |
| `next` | replace | (rest c) and (empty? c) | `next` existed to return `nil`; the `Iter` method of that name is removed |
| `nthnext` | omit | - | `(drop n c)` and `empty?` |
| `nthrest` | omit | - | alias of `drop` |
| `splitv-at` | replace | (split-at n c) | `split-at` already returns `Vec`s |
| `flatten` | omit | - | needs heterogeneous nesting; a typed `flatten` is `(mapcat identity c)` one level, or a `Tree` enum walk |
| `partitionv` | replace | (partition n c) | `partition` yields `Vec`s |
| `partitionv-all` | replace | (partition-all n c) | `partition-all` yields `Vec`s |
| `dorun` | replace | (run! f c) | nothing is lazy-and-cached; a recipe runs when consumed |
| `doall` | replace | (vec c) | `vec` when a materialised result is wanted |
| `map-entry?` | omit | - | the entry is a `Pair`, a distinct struct; the question is a type |
| `sorted?` | omit | - | a type, `SortedMap`/`SortedSet` |
| `transient` | omit | - | replaced by in-place update on a unique collection (M7 rule 3, types 6.6): a loop of `assoc`s is already the transient |
| `persistent!` | omit | - | see transient |
| `conj!` | omit | - | see transient |
| `assoc!` | omit | - | see transient |
| `dissoc!` | omit | - | see transient |
| `disj!` | omit | - | see transient |
| `pop!` | omit | - | see transient |
| `defrecord` | replace | `defstruct` | `defstruct` plus `(derive Eq Name)` etc. (syntax 3.7); a struct is not a map: no extra keys, no dissoc of a field |
| `->Name` | replace | - | the struct constructor `(Point 1 2)` (syntax 3.7) |
| `map->Name` | replace | - | no map-to-struct conversion (fields are typed); `(Point ..)` with every field |
| `deftype` | replace | `defstruct` | `defstruct` with `(Cell T)` fields for mutable state (syntax 3.7) |
| `struct-map` | omit | - | legacy |
| `struct` | omit | - | legacy |
| `create-struct` | omit | - | legacy |
| `accessor` | omit | - | legacy |
| `object-array` | replace | `(array n x)` | `(array n x)` |
| `int-array` | replace | `(array n 0i32)` | `(array n 0i32)` with the typed literal; one `(Array T)` type for all |
| `to-array` | replace | `(vec c)` | `(vec c)` into a `(Vec a)`; arrays are for the library internals |
| `into-array` | replace | `(vec c)` | a typed `(Vec a)` from any `Reducible` |
| `to-array-2d` | omit | - | build a `(Array (Array a))` |
| `amap` | replace | `(mapv f a)` | `(mapv f a)` since `(Array a)` is `Reducible`; the in-place form is a library loop with `array-set!` |
| `areduce` | replace | `(reduce f init a)` | `(reduce f init a)` since `(Array a)` is `Reducible` |
| `defstruct` | omit | - | legacy; fibber's `defstruct` is the record form |
| `namespace` | omit | - | keywords are flat symbols; `:a/b` is spelled out |
| `symbol` | omit | - | symbols exist only as `Form`s in macros |
| `char?` | omit | - | static types, `char` is a type |
| `clojure.string/re-quote-replacement` | omit | - | if replacement strings are literal, no escaping is needed |
| `parse-uuid` | omit | - | a `Uuid` struct if wanted |
| `random-uuid` | omit | - | needs an `Rng` |
| `re-matcher` | omit | - | no stateful matcher; a `Reducible` `Matches` covers the loop |
| `re-groups` | omit | - | `Match` carries its groups |
| `quot` | replace | (/ a b) | the same rule on integers |
| `+'` | omit | - | needs arbitrary precision (not in the language); overflow traps |
| `-'` | omit | - | as +' |
| `*'` | omit | - | as +' |
| `inc'` | omit | - | as +' |
| `dec'` | omit | - | as +' |
| `unchecked-divide-int` | omit | - | one generic wrapping family at every width |
| `unchecked-remainder-int` | omit | - | as unchecked-divide-int |
| `unchecked-add-int` | omit | - | covered by the width-generic wrapping family |
| `unchecked-byte` | replace | (trunc i8 x) | keeps the low bits, never traps |
| `num` | omit | - | no boxed Number |
| `bigint` | omit | - | no arbitrary-precision type; a `BigInt` library is a long-tail item |
| `bigdec` | omit | - | as bigint |
| `biginteger` | omit | - | as bigint |
| `rationalize` | omit | - | no Ratio type |
| `numerator` | omit | - | no Ratio type |
| `denominator` | omit | - | no Ratio type |
| `==` | omit | - | `=` is one type-directed equality and cross-type comparison is a type error (p04); convert explicitly to compare an `i64` with an `f64` |
| `pos-int?` | omit | - | static types; `(pos? n)` |
| `neg-int?` | omit | - | static types |
| `nat-int?` | omit | - | static types; `(not (neg? n))` |
| `clojure.math/add-exact` | replace | (+ a b) | plain `+` already traps on overflow |
| `clojure.math/subtract-exact` | replace | (- a b) | plain `-` traps |
| `clojure.math/multiply-exact` | replace | (* a b) | plain `*` traps |
| `clojure.math/negate-exact` | replace | (neg a) | traps on the minimum |
| `clojure.math/increment-exact` | replace | (inc a) | traps |
| `clojure.math/decrement-exact` | replace | (dec a) | traps |
| `true?` | omit | - | values are `bool`; write the value |
| `false?` | omit | - | write `(not x)` |
| `boolean` | omit | - | no truthiness (syntax 2); `some?` for Options |
| `boolean?` | omit | - | static types |
| `seq?` | omit | - | static types; `Reducible` is a protocol |
| `coll?` | omit | - | protocol `Collection` constraint instead |
| `list?` | omit | - | static types |
| `vector?` | omit | - | static types |
| `map?` | omit | - | static types |
| `set?` | omit | - | static types |
| `string?` | omit | - | static types |
| `keyword?` | omit | - | static types |
| `symbol?` | omit | - | static types; `Form` has `Sym` |
| `number?` | omit | - | static types |
| `integer?` | omit | - | static types |
| `int?` | omit | - | static types |
| `float?` | omit | - | static types |
| `double?` | omit | - | static types |
| `decimal?` | omit | - | no BigDecimal |
| `ratio?` | omit | - | no Ratio |
| `rational?` | omit | - | no Ratio |
| `fn?` | omit | - | static types |
| `ifn?` | omit | - | only functions are callable (types 1.4) |
| `associative?` | omit | - | `Lookup` and `Assoc` protocol constraints |
| `sequential?` | omit | - | static types |
| `counted?` | omit | - | `Reducible.size` is O(1) where the source knows (§2.1) |
| `reversible?` | omit | - | a protocol (`Reversible`) if `rseq` is kept |
| `indexed?` | omit | - | `Reducible.nth` is O(1) where the source knows (§2.1) |
| `seqable?` | omit | - | `Reducible` constraint |
| `ident?` | omit | - | static types |
| `simple-ident?` | omit | - | static types |
| `qualified-ident?` | omit | - | static types |
| `simple-keyword?` | omit | - | static types |
| `qualified-keyword?` | omit | - | static types |
| `simple-symbol?` | omit | - | static types |
| `qualified-symbol?` | omit | - | static types |
| `uuid?` | omit | - | static types |
| `inst?` | omit | - | static types |
| `bytes?` | omit | - | static types |
| `class?` | omit | - | no reflection |
| `instance?` | omit | - | types are static; a `match` on an enum is the runtime test |
| `satisfies?` | omit | - | instances are checked at compile time (types 4) |
| `extends?` | omit | - | as satisfies? |
| `isa?` | omit | - | no hierarchies |
| `record?` | omit | - | static types |
| `var?` | omit | - | no Vars |
| `special-symbol?` | omit | - | no reflection |
| `reader-conditional?` | omit | - | no reader conditionals |
| `tagged-literal?` | omit | - | no tagged literals |
| `chunked-seq?` | omit | - | implementation detail |
| `type` | omit | - | no reflection |
| `class` | omit | - | no reflection |
| `cast` | omit | - | static types |
| `if-some` | omit | - | `if-let` over `Option` has exactly this meaning |
| `when-some` | omit | - | same as `when-let` over `Option` |
| `when-first` | omit | - | `(when-let (x (first c)) ..)` with `first` returning `(Option e)` |
| `letfn` | replace | - | named `fn` for self recursion (p31: factorial of 10 is 3628800) and top-level `defun`s for mutual recursion (syntax 3.1) |
| `defn-` | replace | - | the `:private` qualifier (syntax 5) |
| `declare` | omit | - | names are bound before any body is checked (syntax 3.1) |
| `defonce` | omit | - | no REPL redefinition model; `def` constants are evaluated once before `main` |
| `binding` | omit | - | no dynamic vars (`def` is immortal and immutable); pass the context as a parameter or struct |
| `with-redefs` | omit | - | no vars; tests pass fakes as arguments (protocols) |
| `locking` | omit | - | use an `Atom` or message passing |
| `eval` | omit | - | no run-time `eval`; macros run at expansion time on the JIT (syntax 3.16) |
| `macroexpand` | omit | - | a compiler flag (`fibc expand`) instead |
| `macroexpand-1` | omit | - | as macroexpand |
| `require` | replace | - | the `:require` clause of `ns` |
| `use` | replace | - | the `:use` clause of `ns` |
| `refer` | omit | - | only `:use` and qualified `alias/x` |
| `import` | omit | - | no Java |
| `in-ns` | omit | - | no REPL namespaces |
| `extend-type` | replace | `impl` | `impl` (global fact, one per protocol and head, syntax 3.10) |
| `extend-protocol` | replace | - | several `impl`s |
| `reify` | replace | - | a struct with a `dyn` value, `(dyn P e)` (syntax 3.10), or a closure |
| `proxy` | omit | - | Java |
| `definterface` | omit | - | Java |
| `gen-class` | omit | - | Java |
| `defmulti` | replace | `defprotocol` | `defprotocol` when dispatch is on a type, a `match` on an enum when the dispatch is on data |
| `defmethod` | replace | `impl` | `impl` or a `match` clause |
| `with-open` | replace | - | scope-exit release of the handle object (types 6.3), once a `Drop`-like hook exists for non-memory cleanup (section 4, L12); until then an explicit ... |
| `with-out-str` | replace | - | a writer argument; a string builder is §9 Q26 |
| `..` | omit | - | Java |
| `apply` | omit | - | needs dynamic arity and variadic callees; named folds replace the usual uses: `(apply + xs)` is `(sum xs)`, `(apply * xs)` `(product xs)`, `(apply max xs)` `(unwrap (reduce max xs))`, `(apply str xs)` `(str/join xs)`, `(apply concat xss)` `(mapcat identity xss)`, `(apply merge ms)` `(reduce merge ms)` (an `Option` of the map), `(apply merge-with f ms)` `(reduce (fn (a b) (merge-with f a b)) ms)`, `(apply map vector m)` (transpose) a nested `mapv` (needs a typed matrix, so no name); p05/p11 ... |
| `trampoline` | omit | - | real tail calls and loops (syntax 2, types 6.10); a mutually recursive pair in one module is a tail call |
| `memfn` | omit | - | Java |
| `bound-fn` | omit | - | no dynamic vars |
| `reduced?` | replace | `match` | `match` on `Step` |
| `unreduced` | replace | `match` | `match` on `Step` |
| `ensure-reduced` | replace | `Done` | `Done` |
| `volatile!` | replace | `(cell x)` | `(cell x)` (types 2.9) |
| `vswap!` | replace | `(set! c (f @c))` | `(set! c (f @c))` on a `Cell` |
| `vreset!` | replace | `(set! c v)` | `(set! c v)` |
| `volatile?` | omit | - | static types |
| `:strs` | omit | - | no map patterns |
| `:syms` | omit | - | no map patterns |
| `:or` | omit | - | with `get` returning `(Option v)`, `(unwrap-or (get m :a) 1)` |
| `add-watch` | omit | - | callbacks on state changes cannot hold borrowed values safely; explicit subscription through a channel |
| `remove-watch` | omit | - | see add-watch |
| `set-validator!` | omit | - | validate in the function given to `swap!` |
| `get-validator` | omit | - | see set-validator! |
| `delay?` | omit | - | static types |
| `realized?` | omit | - | a `Delay` has `forced?`; adaptors have no cells |
| `future-cancel` | omit | - | no cancellation; a cancel flag in an `Atom` that the task polls |
| `future-cancelled?` | omit | - | see future-cancel |
| `future?` | omit | - | static types |
| `pcalls` | replace | (plet ..) | `plet` (syntax §3.12) |
| `pvalues` | replace | (plet ..) | `plet` |
| `meta` | omit | - | no metadata (syntax 5: "fibber has no metadata maps"); a struct field carries the datum |
| `with-meta` | omit | - | see meta |
| `vary-meta` | omit | - | see meta |
| `alter-meta!` | omit | - | see meta |
| `reset-meta!` | omit | - | see meta |
| `^Type` | replace | `x: i64` | `x: i64` annotations, checked (syntax 1.5) |
| `println-str` | omit | - | `(str .. "\n")` |
| `prn-str` | omit | - | `(str (debug-str x) "\n")` |
| `newline` | omit | - | `(println "")` |
| `print-table` | omit | - | long tail, no heterogeneous rows |
| `tap>` | omit | - | `dbg` covers the use |
| `read-string` | replace | - | the reader of `fib.syntax` returning `Form` (spec/bootstrap.md); typed parsers `parse-long` etc. for data |
| `read` | omit | - | see read-string |
| `with-in-str` | omit | - | pass a reader parameter |
| `ex-info` | replace | (Err e) | an error struct or enum of the module in a `(Result a e)` |
| `ex-data` | replace | - | the fields of the error type |
| `ex-message` | replace | `(show err)` | `(show err)` via `Show` on the error type |
| `ex-cause` | replace | - | an error enum variant holding the cause |
| `throw` | replace | `(Err e)` | `(Err e)` for recoverable errors, `trap` for impossible states (syntax 4.3) |
| `try` | replace | `match` | `match` on a `Result`; there are no exceptions to catch; a trap ends the program (types 2.11) |
| `catch` | replace | - | a `Result` pattern |
| `finally` | replace | - | scope-exit release for memory (types 6.3); non-memory cleanup (flushing, closing) needs a `Drop`-like protocol, a gap |
| `Throwable->map` | omit | - | no stack traces |
| `ex-triage` | omit | - | Java |
| `clojure.walk/postwalk-demo` | omit | - | a debugging aid; `dbg` |
| `clojure.walk/prewalk-demo` | omit | - | a debugging aid |
| `clojure.walk/macroexpand-all` | omit | - | a compiler flag (`fibc expand FILE`) |
| `clojure.data/equality-partition` | omit | - | protocol internals |
| `clojure.data/diff-similar` | omit | - | protocol internals |
| `#"..."` | omit | - | no reader macro; see `re-pattern` |
| `^meta` | replace | `:private` | `:private` and `x: T` (syntax 5, 1.5) |
| `::kw` | omit | - | keywords are plain symbols of one flat scalar type; `:ns/name` is spelled out |
| `1N 1M 1/2` | omit | - | no such types; width suffixes `1i32` `2.5f32` instead (syntax 1.1) |
| `##Inf ##-Inf ##NaN` | omit | - | no NaN or infinity literal (types 2.12); `(/ 1.0 0.0)` and `(/ 0.0 0.0)` build them; named constants `f64-inf`, `f64-nan` are library `def`s |
| `#?(:clj ..)` | omit | - | one platform |
| `#inst "..."` | omit | - | no tagged literals |
| `print-method` | replace | - | an `impl Show` and `impl Debug` for the type |
| `*out*` | replace | `println` | `println` and `eprintln` write to fd 1 and fd 2 (prelude, syntax 4.5); a writer value passed as an argument for redirection |
| `*in*` | replace | - | a stdin handle value; `(read-line)` returns `(Option str)`; absent today |
| `*print-length*` | omit | - | options on the `Debug`/`Pretty` call, not dynamic state |
| `load-string` | omit | - | no run-time compiler |
| `load-file` | omit | - | no run-time compiler |
| `derive` | omit | - | hierarchies are not offered; keep fibber's `derive` |
| `char-escape-string` | omit | - | an implementation detail of `Debug` |
| `char-name-string` | omit | - | an implementation detail of `Debug` |
| `with-precision` | omit | - | no BigDecimal |
| `with-local-vars` | replace | `(cell x)` | `(cell x)` (syntax 3.11) |
| `replicate` | omit | - | see `repeat` |
| `destructure` | replace | - | patterns in `match` and `let` are checked by the compiler, there is no expansion to call; macro writers use `match` forms |
| `find-keyword` | omit | - | keywords are interned by the reader; run-time `(keyword s)` interns, a separate decision (see `keyword`) |
| `extend` | replace | `impl` | `impl` (syntax 3.10) |
| `refer-clojure` | omit | - | the prelude is always used; a local definition shadows a `:use`d name |
| `alias` | omit | - | only the `:require ... :as` clause |
| `with-bindings` | omit | - | no dynamic vars |
| `uri?` | omit | - | no URI type |
| `inst-ms` | omit | - | no date type in the core; a time library is a long-tail item |
| `test` | omit | - | tests are programs with executable verdicts (spec/method.md) |
| `requiring-resolve` | omit | - | no run-time name resolution |
| `read+string` | omit | - | see `read-string` |
| `remove-tap` | omit | - | see `tap>` |
| `print-simple` | omit | - | printer internals |
| `print-dup` | omit | - | printer internals |
| `reader-conditional` | omit | - | one platform |
| `tagged-literal` | omit | - | no tagged literals |
| `munge` | omit | - | compiler internals |
| `long-array` | replace | `(array n 0)` | `(array n 0)` (an `(Array i64)`), see `int-array` |
| `double-array` | replace | `(array n 0.0)` | `(array n 0.0)`, see `int-array` |
| `float-array` | replace | `(array n 0.0f32)` | `(array n 0.0f32)`, see `int-array` |
| `short-array` | replace | `(array n 0i16)` | `(array n 0i16)`, see `int-array` |
| `byte-array` | replace | `(array n 0i8)` | `(array n 0i8)`; the same `(Array i8)` that `str-bytes` returns (syntax 4.3), signed |
| `char-array` | replace | `(Array char)` | `(Array char)` of Unicode scalars, `(str-chars s)` returns a `(Vec char)` today |
| `boolean-array` | replace | `(array n false)` | `(array n false)` |
| `unchecked-subtract-int` | omit | - | covered by the width-generic wrapping family |
| `unchecked-multiply-int` | omit | - | covered by the width-generic wrapping family |
| `unchecked-negate-int` | omit | - | covered by the width-generic wrapping family |
| `unchecked-inc-int` | omit | - | covered by the width-generic wrapping family |
| `unchecked-dec-int` | omit | - | covered by the width-generic wrapping family |
| `unchecked-short` | replace | `(trunc i16 x)` | `(trunc i16 x)` |
| `unchecked-char` | replace | `(i32->char n)` | `(i32->char n)` traps on a non-scalar; a char is a scalar value, not 16 bits |
| `unchecked-int` | replace | `(trunc i32 x)` | `(trunc i32 x)` |
| `unchecked-long` | replace | `(fptosi i64 x)` | `(fptosi i64 x)` saturating for floats, `(sext i64 x)` for ints |
| `unchecked-float` | replace | `(fptrunc f32 x)` | `(fptrunc f32 x)` |
| `unchecked-double` | replace | `(fpext f64 x)` | `(fpext f64 x)`, `(sitofp f64 n)` |
| `add-tap` | omit | - | see `tap>` |
| `Math/sqrt` | replace | `clojure.math/sqrt` | `clojure.math/sqrt` |
| `Math/pow` | replace | `clojure.math/pow` | `clojure.math/pow` |
| `Math/abs` | replace | `abs` | `abs` |
| `Math/floor` | replace | `clojure.math/floor` | `clojure.math/floor` |
| `Math/ceil` | replace | `clojure.math/ceil` | `clojure.math/ceil` |
| `Math/round` | replace | `clojure.math/round` | `clojure.math/round` |
| `Math/random` | replace | `rand` | `rand` over an `Rng` |
| `Math/log` | replace | `clojure.math/log` | `clojure.math/log` |

## 5. Deviations from Clojure

Each row is a place where this library does not do what Clojure does, with the reason. `S` numbers
refer to the sharp edges of the survey (`stdlib/survey.md` §2 of the design record); every one of the
71 is answered by a row here or is marked "same" (kept as Clojure has it, stated) or "n/a" (the type
system makes it impossible).

| # | Clojure | Here | Reason | S |
|---|---|---|---|---|
| 1 | `nil` is a value of every type; sequence functions accept it | `nil` is `Option`'s empty variant; `(conj nil x)` and `(get nil k)` are type errors (an `Option` is not a `Collection` or a `Lookup`); `Option` itself is a 0-or-1 `Reducible`, so `(first nil)` is `nil` and `(count nil)` is 0 ([R] A10 nth) | typing | S03 S29 S28 |
| 2 | `first last second peek get find max-key` return `nil` when empty or absent | return `(Option e)`; `(unwrap o)` for the sure case; `nth` traps | typing; absent and `nil`-valued differ by construction | S09 S14 |
| 3 | `(some pred c)` | `(any? p c)` -> `bool`; `(find-map f c)` -> `(Option b)`; `(find-first p c)` | `some` constructs an `Option` | |
| 4 | truthiness: `or`/`and` return operands, `if` tests `nil`/`false` | `if`, `and`, `or`, `when` take `bool`; `(unwrap-or o d)` is `(or o d)` | typing | S27 |
| 5 | `when`, `when-let` return the body or `nil` | the body is `unit`; the value form is `(if-let [x e] (some body) nil)` | typing | S41 |
| 6 | `next`, and `rest` returning `()` or a seq | no `next`; `(rest c)` is `(drop 1 c)`, a recipe that is empty when empty | `next` exists to return `nil`; its name is also the removed `Iter` method | S04 |
| 7 | `contains?` on a vector tests an index | `contains?` is `Keyed`: `Map` and `Set` only; `includes?` tests an element, `(some? (get v i))` an index | the inconsistency ROADMAP rule 1 names | S02 |
| 8 | `(nth c i)` throws; `(get v i)` is `nil`; a vector is callable | `nth` traps `nth: index out of range`; `(get v i)` is `Option`; vectors, maps, sets and keywords are not callable | only functions are callable (types §1.4) | S60 S66 S36 |
| 9 | `count` realises a lazy seq; `(count "é")` counts UTF-16 units | `count` is `size` (O(1) or a walk, §2.1); `str` is not a collection: `str-len` bytes, `(count (chars s))` characters | no hidden unit | S18 |
| 10 | lazy seqs are cached, chunked, and retain their head; effects run ahead | recipes re-run at each traversal, run exactly when consumed and one element at a time; `LSeq` (tranche 5) is the memoising type | no GC to make a cell per element cheap | S11 S12 S19 S61 |
| 11 | `(reduce f c)` calls `(f)` when empty; `reduced` is a wrapper checked at each step | `(reduce f init c)`; `(reduce f c)` returns `(Option e)`; `reduced` is `(Done x)` read by `reduce-while` | no identity element for a generic `f`; typed | S64 |
| 12 | `(map f c1 c2)`, `zipmap`, `interleave` truncate silently | truncate to the shorter, stated; `zip-strict` traps on a mismatch; the second operand is `Cursable` (a `Vec`, `Range` or `Array`, not an adaptor, until §7 L16) | push cannot step two sources | S47 |
| 13 | `partition` drops an incomplete last group | same: kept, with `partition-all` the documented safe form | Clojure's meaning ported code relies on | S23 same |
| 14 | `(= 1 1.0)` is false, `(== 1 1.0)` true; `1` and `1.0` are different keys | `=` is on one type: a compile error across numeric types; no `==`; a map's key type is one type | no implicit widening | S05 S34 |
| 15 | `(= [1] '(1))` is true | a type error; `(seq= a b)` compares across `Reducible`s | equality on one type | S05 |
| 16 | `compare` throws across types and compares vectors by length first | `compare` returns -1/0/1 on one type; `Ord` on sequences is lexicographic; on `f64` it is a total order with NaN greatest and equal to itself (Clojure's `compare` is built on `<`, so it should say 0 against a NaN and `sort` of floats with a NaN would then depend on the input order there: **[H]**, not run) | `Ord` and `Eq` agree; `sort` must be deterministic | S56 |
| 17 | `sort` returns a seq and takes a function or a comparator | `sort` returns a stable `Vec`; a comparator is `(fn (e e) i64)`, never a predicate | one convention; a predicate collapses equal elements | S13 S44 |
| 18 | `max-key`/`min-key` return the last of equal keys | same, now `(Option e)` over a collection | stated | S14 same |
| 19 | `sorted-map-by`, `sorted-set-by` take a comparator | `sorted-map-with`, `sorted-set-with`; `-by` takes a key function | one suffix, one meaning | S26 S44 |
| 20 | `conj` front for a list, end for a vector; `(conj nil x)` is a list; `(conj m [k v])` | one rule per `Collection` instance; `(conj nil x)` is a type error; a map takes a `Pair` | stated per type | S06 same |
| 21 | `(assoc v i x)` appends at `i = count` | `Assoc` on `Vec`: `i < count`, else traps; `conj` appends | one operation, one meaning | S08 |
| 22 | `empty` returns `nil` for non-collections; `list`'s variants are Lisp's | `empty` is `Emptyable`, taking a value; the `List` variants `empty` and `cons` are renamed `Empty` and `Cons` and `cons` becomes a function; the `list` macro expands to qualified names | the library defines functions `empty` and `cons`, so the variants cannot keep the names; and the unhygienic `list` macro expands to bare `empty`: defining any function `empty` breaks `(list 1 2)` ([R] A6). A qualified expansion `(fib.prelude/cons 1 (fib.prelude/cons 2 fib.prelude/empty))`, in an expression and in a pattern, works beside a user `empty` and `cons` (31 under both tools, [R] A10 list); the rename is still needed for the library's own functions | S57 |
| 23 | `get-in`, `assoc-in`, `update-in` take a path vector of any keys | macros over a literal path, one `get` per level; a missing level is `(empty ..)` of that level's type | each level has its own key type | S21 |
| 24 | maps and vectors hold values of any types; `{:a 1 :b "x"}` | one key type and one value type; heterogeneous data is a struct, or `Val` of `fib.data` (tranche 5) | static types | S36 |
| 25 | `(:k m)`, `(m :k)` | not adopted yet; `(get m :k)`, `(. s k)`; a keyword in call position as a checker feature is §9 Q11 | needs the argument's type to choose | S36 |
| 26 | `vec` and `set` of a map give vectors of entries | the entry is a `(Pair k v)`, the library's one tuple type (a struct with `fst` and `snd`); `(key p)`, `(val p)`; `[k v]` patterns after L3b | no tuple vector | S07 S58 |
| 27 | `keys`, `vals`, `seq` of empty return `nil` | an empty recipe; `(seq c)` is `(Option c)`: `nil` when empty | one empty per type | S29 |
| 28 | the printed order of a map is its internal order, and flips at 9 entries | iteration order is the HAMT's, unspecified; `Show` and `Debug` print in the order of the `Show` text of the keys, which does not depend on the hash; tests compare with `=`; `fib.sorted` is the numerically ordered type | no implementation detail to depend on; the hash changes once (Q15) | S15 S70 |
| 29 | `println`, `prn`, `str`, `pr-str` disagree about nested strings; `str` of a lazy seq prints an object header | `Show` (display: `str println print`) and `Debug` (readable: `pr prn pr-str dbg`), each the same at every depth; every type has a `Show` | two protocols, fixed meanings | S16 S17 |
| 30 | `merge` accepts `nil`; `(merge)` is `nil` | `(merge m1 m2 ..)` on `Map`s of one type, never `nil` | typing | S20 |
| 31 | `into` takes its behaviour from the target, and bad input is a run-time error | `(into to c)` needs `(Collection t e)` and `(Reducible c e)`: a bad element type is a compile error | typing | S10 |
| 32 | `update-keys` collisions keep the last | `update-keys: duplicate key` traps; `(into (map-empty) (map ..))` is the lossy-by-rule path | no silent data loss | |
| 33 | `range` accumulates floats and repeats forever on step 0 | `range` is integer and exact, a zero step traps; `frange` computes `a + i*s` for `i < ceil((b-a)/s)` | determinism | S22 |
| 34 | `+` mixes `long` and `double`, overflows to `BigInt` with `+'`, `(/ 7 2)` is `7/2`, `(+)` is 0 | one type; overflow traps; integers divide toward zero; no ratios or `BigInt`; `(+)` is a compile error, `(sum c)` is the identity form | types §2.12 | S30 S31 |
| 35 | `round` is half up on `Math.round` | the same, saturating, NaN is 0; `rint` is half to even; conversions are checked (`int`, `long`) | stated | S32 |
| 36 | `identical?` and `compare-and-set!` use reference equality on boxed numbers | `same?` observes sharing on objects and compares scalars by value; `compare-and-set!` compares by `Eq` | no boxed-number surprise | S35 |
| 37 | `rand`, `shuffle`, `rand-nth` use a global generator | they take an `Rng` | no global mutable state | S50 |
| 38 | `#(+ % 1)` with `%&`, no nesting, `->` surprises | reader sugar for `fn` with `%`, `%1` to `%9`; no `%&`; nesting is an error (E8) | the biggest ergonomic gap, with the gotchas removed | S37 |
| 39 | `case` has its own list and no-match rules | a macro over `match` with literals and or-patterns; no match is a compile error unless a default is given | exhaustiveness is checked, never an exception | S39 |
| 40 | `cond`, `condp` and `case` disagree about no match | all trap or fail to compile; `cond` falls off the end with `cond: no clause matched`; `cond` is Clojure's flat pairs (E4) | one rule | S40 |
| 41 | `(str ["a"])` is `["a"]` and `(println ["a"])` is `[a]` | `str` and `println` are both `Show`: `[a]`; `(debug-str ["a"])` is `["a"]` | one rule for display | S16 S17 |
| 42 | `some->` stops at `nil`, `cond->` never passes its test the value | `some->` steps return `Option`; `cond->` takes `(test form)` pairs of `bool` | typing | S42 |
| 43 | `swap!` may run its function more than once | same, stated (syntax §3.11); the function is `:borrow` | | S43 same |
| 44 | `(comp (map f) (filter p))` composes transducers; function `comp` right to left | `(xf (map f) (filter p))` composes `Xf` values left to right; `comp` is function composition: `(comp (xmap inc) (xfilter odd?))` is `cannot unify (Xf i64 i64) with (fn (a) b)` ([R] A10 comp), and the error says `xf` (§7 D1); after C1 `Fn` is a protocol and `comp` may dispatch on `Fn` and `Xf`, giving Clojure's text, which impl heads on function types forbid today | the two never share a spelling today | S63 |
| 45 | `reduce`, `into`, `transduce` over a transducer value that is shared between accumulators fail on typed encodings | `Xf` is a factory of steps: one value serves two accumulator types ([R] A8); `sequence`/`eduction` wait for closure types | `let` does not generalise (types §2.4) | S63 |
| 46 | `mapv`, `filterv` and no siblings | kept as aliases of `(vec (map ..))` and `(vec (filter ..))`; no others | readers of Clojure code | S25 |
| 47 | the `-by`, `-key` and `-with` zoo | `-by` key function, `-key` Clojure's `max-key`/`min-key`, `-with` comparator or combiner | one suffix, one meaning | S26 |
| 48 | `doseq`, `for`, `run!`, `dorun`, `doall` | `doseq` and `run!` for effect, `for` a recipe, `vec` materialises; `dorun` and `doall` vanish; a recipe dropped unconsumed is a warning (§7 L13) | no lazy cells | S19 |
| 49 | duplicate keys: an error in a literal, silence in a constructor | the expander rejects duplicate **literal** keys; a computed duplicate: the last wins | stated (§9 Q5) | S46 |
| 50 | `subvec` shares and retains its parent | the same: a `Slice` holds a count on its parent; `(vec (subvec ..))` copies | stated | S48 same |
| 51 | `peek`/`pop` mean different ends | `Stack`: `Vec` end, `List` front, `Queue` front; `pop` of an empty collection traps; `peek` gives `nil` | stated per type | S49 same |
| 52 | `split-with` walks the prefix twice | one pass, `p` runs once per element up to and including the first failure | | S71 |
| 53 | `clojure.string/split` drops trailing empties and takes a regex; `replace` interprets `$` by match type | `str/split` keeps trailing empties and takes a literal, `split-re` a `Regex`; `replace`, `replace-re`, `replace-with` are three functions | round-trip with `join`; no overloading on a dynamic type | S53 S54 |
| 54 | `index-of` returns `nil`, the Java method -1 | `(Option i64)`, a byte offset; never -1 | | S55 |
| 55 | `upper-case` of a string and of a char differ | `str/upper-case` is the full mapping, `upper` on a `char` the simple one | different functions of different types | S67 |
| 56 | regex results change shape with the pattern; backtracking | `re-find` returns `(Option Match)` with `whole` and `groups` always; the engine is non-backtracking | no pathological hang | S52 |
| 57 | `format` is `String.format` with its failure modes | a macro that checks directives against argument types at compile time; a literal format string | | S51 |
| 58 | `defrecord`, a record is a map until it is not | `defstruct`; a struct is not a map; field update is `(with r (f v))` (§4.2, [R] A10 with) | typing | S62 |
| 59 | `list?`, `seq?`, `coll?`, `counted?` disagree | a protocol constraint in the signature; no run-time predicate | P6 | S59 |
| 60 | recursion and primitive locals | loop variables are typed; `recur` only in `loop` | n/a | S69 |
| 61 | `sorted-set-by` with a mixed key type throws at insertion | one key type with `Ord`: cannot occur | n/a | S45 |
| 62 | `transient`, `persistent!`, `conj!` | omitted: a unique `conj` loop is in place (§2.5) | P3 | |
| 63 | `defn` | `defun`, overloaded by arity (L1); the `fn` literal is not | syntax §3.1 | |
| 64 | `apply`, `trampoline`, `letfn`, `binding`, `eval`, `meta`, `type`, `satisfies?` | omitted (§4.17) | static types, real tail calls and loops, no dynamic vars, no reflection | S27 S28 |
| 65 | `hash` of `1` and `1.0` differ and `(hash 0.0)` differs from `(hash -0.0)` | `Hash f64` hashes `-0.0` as `0.0` and one NaN | `=` and `hash` agree | S34 |
| 66 | `pmap` is lazy and bounded by cores | eager, one task per element until a pool exists | | |
| 67 | `(vec x)` returns `x` for a vector | same, shared by count | | |
| 68 | three argument-order conventions and their exceptions | kept, written down as §3 N1 to N3, and a swapped call is a compile error (`cannot unify (Vec i64) with (fn (a) b)`) | statically checked | S01 same |
| 69 | `take`, `drop`, `rest`, `butlast` return `nil` or `()` inconsistently | every slicing function returns a possibly empty recipe, never `nil`; a negative count traps | one empty per type | S24 |
| 70 | a character is not a one-character string | `char` is a Unicode scalar and a distinct type; `(char->str c)`; `(str/chars s)` is the view | same in both | S33 same |
| 71 | destructuring defaults and short or long inputs | `[a b]` matches exactly two; `[a b & r]` binds a new `Vec`; the default is `unwrap-or`; a refutable binding traps | patterns state the shape (§2.6) | S38 |
| 72 | the result type of sequence functions is a zoo | one sentence: an adaptor returns a recipe, an operation that needs an owned result (`sort reverse shuffle take-last split-at`) returns a `Vec`, `vec` is the explicit materialiser | | S65 |
| 73 | `(str :k)`, `(name :k)` and symbols | `(show :k)` is `:k`; `name` returns the text; there are no symbols at run time | keywords are flat scalars | S68 |
| 74 | `(update m k inc)` fails on a missing key (NPE); `(update m k f)` hands `f` `nil` | `f` sees the value; a missing key traps `update: no key`; `(update-or m k f d)` supplies a default and `(update-opt m k f)` hands `f` the `(Option v)` (`fnil` for ported code) | the common case, the key present, pays no `Option`, which is a heap object per call for a scalar (A5); Clojure's text `(update m k inc)` compiles | |
| 75 | `juxt` returns a vector; `partial` and `constantly` take any arity; `swap!` takes extra arguments only as Clojure's | `juxt` gives a `Pair` or `Triple` (two or three functions; a vector would need one result type); `partial` leaves one parameter free and `partial2` two (a macro cannot see `f`'s arity); `constantly` is unary; `swap!` is a macro that wraps the extra arguments in a closure | a `fn` has fixed arity and one result type | |
| 76 | `max-key`, `min-key` and `distinct?` take varargs | take a collection: `(max-key f c)` is an `(Option e)`; ties go to the last, as Clojure's | no variadic callees (`apply` is omitted, §4.17) | |
| 77 | `subs` counts UTF-16 units; `(str nil)` is `""` | `subs` takes byte offsets and traps when an offset splits a character; a bare `nil` has no type, so `(str nil)` is a compile error (`ambiguous constraint Show a`, [R] A10 str-nil); `(str o)` of an `Option` shows `nil` or `(some x)` | no hidden unit; typing | |
| 78 | `[k v]` is an expression and the entry of a map, and `(into {} [[1 2]])` works | a vector of two is a `(Vec a)` of one element type, not a tuple, so `(into {} [[1 2]])` is a type error; the entry is a `Pair`, and `zip` and `juxt` give `Pair`s, which a `Map` `conj`s (`(into {} (zip ks vs))` is Clojure's `(into {} (map vector ks vs))`, [R] A10 pair) | one tuple type; heterogeneous `[a b]` stays a type error | |
| 79 | `(disj m k)` on a map and `(dissoc s k)` on a set are errors | both type-check: `dissoc` and `disj` are one `Dissoc` | not worth a protocol of its own; stated | |
| 80 | `replace` takes a vector as a map; `subvec` returns a vector; `subseq` takes a function | `replace` takes a `Map`; `subvec` returns a `Slice` (a view); `subseq` takes a `Cmp` enum (`Lt Le Gt Ge`) | typing; a function value is not a test the B-tree can use | |
| 81 | a recipe is an `if` arm like any value | `(if flag (filter p v) v)` is a type error (`cannot unify (Vec i64) with (Filtered (Vec i64) i64)`): each adaptor is its own type; `(seq-of c)` erases it at a stated cost, `(vec c)` materialises ([R] A10 join) | recipes are structs, not a tagged `seq` | |
| 82 | `defrecord` gives `=`, `hash` and `str` for free | `defstruct` derives nothing unless `(derive Eq P)` is written; §9 Q22 recommends deriving `Eq Ord Hash Show Debug` when every field has them | today `(= (P 1 "x") (P 1 "x"))` is `no implementation of Eq for P` ([R] A10 dd) | |

## 6. Where the library lives

### 6.1 What exists today [R] A9

`fib.prelude` is `lib/prelude.fib`, 491 lines, compiled into the binary, read, expanded and checked
together with the program on every run. Other modules load from **the main file's directory only**
(`a.b` is `a/b.fib` beside the main file): there is no library root, no environment path, no cached
interface and no re-export. **Two `:use`d modules that export one name do not make an error: the first
`:use` wins, silently.** `y.m1` and `y.m2` both export `peek`; `(:use y.m1 y.m2)` then `(peek 3)` is 3 and
`(:use y.m2 y.m1)` is 4, under both tools ([R] A10 use). `spec/syntax.md` §5 says the name is an error when
referenced unqualified, so the spec and the code disagree; this page does not change either: §7 E12 and §9
Q27 ask the owner to make the checker do what §5 says. A local definition shadows a `:use`d one, the prelude
included (a program that defines its own `Box`, `Entry` or `Pair` works, [R] A6).

### 6.2 Layout (proposed)

```
lib/prelude.fib      shrinks to the glue over the primitives: println, eprintln, the private raw writes
lib/fib/core.fib     Option helpers, Pair Triple Step Result, Unit and the numeric functions, compare, the combinators,
                     Eq Ord Hash Show Debug for the collections                                      implicit :use
lib/fib/seq.fib      Reducible, Cursor, sources, recipes, consumers, sort, Xf                        implicit :use
lib/fib/coll.fib     the key-addressed protocols, Vec (trie), List, Map and Set (HAMT), Slice,
                     get-in family, merge, group-by, frequencies                                     implicit :use
lib/fib/print.fib    println print pr prn str format printf (the macros), Show/Debug helpers         implicit :use
lib/fib/sorted.fib   SortedMap SortedSet (B-tree), Queue                                             require
lib/fib/string.fib   clojure.string                                                                  require as str
lib/fib/char.fib, regex.fib, math.fib, set.fib, walk.fib, data.fib, random.fib, io.fib, sys.fib, async.fib, lazy.fib, test.fib
```

Rules. The four implicit modules are `:use`d by every module as `fib.prelude` is now; their export sets
are disjoint, and a test that loads all of them reports any name two of them export: because the compiler
checks nothing (§6.1), that test is the only protection against about 400 unqualified names colliding, and it
is a CI gate of tranche 0. Everything else
is `:require`d with an alias, so `str/join` never meets the task-wait builtin `join`. A module is
found under the main file's directory, then under each `-I DIR` (also `FIB_LIB`), then under the
roots embedded in the binary, as `include_str!` embeds the prelude today (§7 E7); a facade module
re-exports with `(:export-from m ..)`. The compiler's own modules (`compiler/`) keep working
unchanged, because a local definition shadows a library name; the one exception is `compiler/util/result.fib`:
a library `Result` and that file's `Result` are two types of one name, so a module that `:use`s it cannot
exchange values with one that does not, and the file is deleted in the commit that adds `Result` to the library
(three public names of `compiler/` collide with the library's unqualified names today: `Result`, `entry`, and the
private `digit-value`; with `Entry` and `entry` gone only `Result` remains).

### 6.3 Macros

A macro costs nothing under `fibref` and about 80 to 85 ms under `fibc`, once per distinct macro per
program: one JIT module each ([R] A9: an empty `main` 0.10 s; 1, 5, 20 distinct macros 0.20, 0.50,
1.70 s; `fibref` 0.20 s for 20). So a macro that every program uses is a Rust prelude macro, like `when`
and `->`: **`str`, `println`, `print`, `prn`, `pr`, `swap!`, the variadic folds of `+ - * < > <= >= = max
min comp merge conj assoc dissoc bit-and bit-or bit-xor`, the literals `{..}` (`hash-map`) and `#{..}`
(`hash-set`), `if-not`, `when-not`, `partial`, and `list`** (E1), each specified by a case that quotes its
expansion; the list grew by ten names because each is on every program's path and a fibber macro would
cost 80 to 95 ms each. The counting loop that the prelude's `for-each` and `range` macros make over a literal
`(range a b)` and a literal one-parameter `fn` stays, retargeted: `run!` and `doseq` over such a range expand
to it, because the library spelling `(run! f (range 0 1000))` otherwise costs a retain and two indirect calls
per iteration where the fused loop has none ([R] A10 forloop); deleting the `for-each` function (§8.3 C)
without rewriting its macro would leave the declined path calling an unbound name. The
macros that only some programs use are written in fibber and live with their module: `for doseq
get-in assoc-in update-in some-> some->> cond-> cond->> condp case as-> partial2 with format printf time
delay lazy-seq try-let`. The Rust list is the part of this design that stage 2 (M6) must re-implement; it is
kept short for that reason, and §6.6 lists the rest of what stage 2 mirrors. Once the macro runner compiles all
of a program's macros in one module (§7 E2) the fixed cost is paid once, and the Rust list can shrink to the
forms every program needs.

**A macro written in fibber cannot name its own module's functions, and the user's names capture its
template.** Only `fib.prelude/x` skips the user's scope; for any other module an expansion resolves where it
is used. A macro `(defmacro mu (v) `(first ,v))` in `lib3` (which `:use`s a `lib1` with its own `first`) gives
**1000**, the user's own `first`, when the user module defines one, and `no implementation of Seq for (Vec
i64)` when it does not: the macro's `lib1/first`, which returns 5, is never reached (A10 hyg, m4 and m5).
`(l1/first ..)` and `(lib1/first ..)` in a template are `unbound name l1/first` and `unbound name lib1/first`
(m2, m3), and `((var l1/first) ..)` is `var: no definition named l1/first` (m6, m7). The prelude's own macros
are hygienic only because Rust writes `fib.prelude/` heads. So a user who writes `(defun filter ..)` or
`(defun get ..)`, which is very common, silently changes what `for`, `get-in`, `case` and `update-in` mean, and
the Rust macros of E1 that expand to a method of `fib.coll` (`conj assoc dissoc merge`) have the same problem.
§7 E11 is the rule: a template symbol resolves in the macro's defining module (Clojure's syntax-quote
qualification), or at least the `fib.prelude/` treatment extends to every implicit `fib.*` module.

### 6.4 Compile time

Two costs, and the first version of this page modelled only the first. (1) **The front end** is linear in
library lines, about 13 to 23 µs per line (491 to 24,550 lines, design record, not re-run), so unused library
text is cheap: a main that `:require`s a 2400-line module and uses none of it costs 0.20 s against 0.10 s for an
empty `main` ([R] A10 timing). (2) **Instantiation and code generation dominate.** Every library function a
program uses is monomorphised at its types and the JIT compiles each instance: a main that uses 80 distinct
functions of that module has 17,256 lIR lines and takes **0.60 s** under `fibc run` (0.40 s of it over the 0.20 s
of the unused load), and `lair run` of the same lIR takes 0.40 s at `-O 0` and **1.00 s at `-O 2`** (A10
timing): about 2 ms per instantiated function at `-O 0`, and a factor 2.5 for `-O 2`. The prototype's 33
checks instantiate most of 460 lines (18,031 lIR lines, 0.40 to 0.50 s under `fibc`, 0.10 s under `fibref`, A9;
a main that only `:use`s the five modules is 0.10 s: the critic's tm1, not re-run). So **C6 is not tiny** (§7): defaulting `fibc run` to
`-O 2` would multiply the cost of every run by 2.5, and it becomes `fibc build` at `-O 2` and `fibc run` at
`-O 0` with a flag. The plan, in order: (1) the macro runner (E2): measured; (2) a cache of each library
module's checked interface, keyed by a hash of its source (types §3.9 defines the interface; the prelude is the
first customer); (3) demand-driven body checking: a library function with a full signature needs its body
checked only when it is used, and the library's own CI runs the full check (`check_library` exists); (4) a
**cache of monomorphised library instances** at the common element types (`each-while` at `(Vec i64)` and `str`
repeats in every program), the one item that touches the dominant cost, which (2) and (3) do not. (2), (3) and
(4) are hypotheses; (2) and (3) are not needed below about 20,000 library lines, and (4) is needed as soon as a
typical program instantiates a few hundred library functions.

### 6.5 Names the prelude already uses

| Name | Today | Resolution |
|---|---|---|
| `some` | `Option`'s constructor and pattern | stays; Clojure's `some` is `any?`/`find-map` |
| `empty`, `cons` | the `List` variants | variants become `Empty`, `Cons`; `empty` is `Emptyable`, `cons` a function |
| `next` | the `Iter` method | removed with `Iter` |
| `join` | the task-wait builtin | stays; the string join is `str/join` |
| `range` | a macro for two arguments and a function for one | one function overloaded by arity (L1), returning `Range` |
| `map count first rest nth get assoc conj` | exist at narrower types (`map` on `Vec` only; `first`, `rest` on `List`) | widened by `Reducible` and the protocols of §2.3 |
| `derive`, `defstruct` | a prelude macro, a core form | Clojure's meanings are omitted; no clash remains |
| `println` | the one-`str` function | a macro over `Show` in head position; a function generic over `Show` serves value position, and `fib.prelude/println` stays reachable ([R] A6, A10 twin) |
| `Box box unbox` | `(Box a)` and two functions; 11, 4 and 13 case files | stay in the prelude; no Clojure meaning, no clash |
| `push! append` | `push!` writes through `&` (10 case files; §2.5's and N5's example), `append` is its alias (7 case files, 1 `compiler/` use) | `push!` stays; `append` is removed (P6) and its 7 cases are ported |
| `length` | `(length s)` is `str-len` (3 case files) | removed: `str-len`; the 3 cases are ported |
| `block-on yield panic` | task and abort helpers (`block-on` in 11 case files) | stay in `fib.async` and the prelude; no clash |
| `str-join` | `(Vec str) -> str`; 3 case files, 54 `compiler/` uses | stays as the builtin under `str/join`; the compiler's 54 uses are ported only when their tranche lands |
| `str-chars` | `(Vec char)`; 1 case file, 4 `compiler/` uses | stays; `(str/chars s)` is the view and `str-chars` is `(vec (str/chars s))` |
| `str-bytes` | a fresh `(Array i8)` per call; 2 case files, 9 `compiler/` uses | stays; `str-byte-at` and `str-find` avoid its copy (§7 L18) |
| `vec-empty map-empty set-empty` | the zero-argument constructors; 21, 1 and 0 `compiler/` uses | stay; `empty` takes a value; `[]`, `{}` and `#{}` are the spelling of new code |
| `set-contains? map-put! map-del! disj` | 1 case file each | `set-contains?` is removed (`contains?` is `Keyed`); `map-put!` and `map-del!` stay with `push!` (N5); `disj` is the `Dissoc` method |
| `eprintln` | the one-`str` function; 3 `compiler/` uses | stays beside `println`, with the same generic twin |
| `Countable Indexable Seq Traversable Iter Associative` | the prelude protocols | removed (§2.3); `Countable` has 1 case file, `Traversable`/`Iter`/`Seq`/`Indexable` none by name |
| `first` on a `List`, `(list ..)`, `next` | `first` returns `e` and `rest` a `List` today; `(first ` is in 2 case files (01 and 61), `(list ` in 4, `(next ` in 2 | `first` returns `(Option e)`, so case 61's `(defun head (xs) (first xs))` changes type; **case 01 is one of the 20 owner-decided cases** and its port needs the owner's sign-off (§9 Q28) |

### 6.6 What stage 2 mirrors

M6 step 2 (the expander) is next, and every Rust-side item of this page is something stage 2 must reproduce
byte for byte. The first version listed only E1; this is the whole list, so the owner can sequence it.

| Item | What stage 2 must do | Tranche | Notes |
|---|---|---|---|
| E1 macros (§6.3) | the 30 Rust macros, each specified by a case that quotes its expansion | T0 | `list` expands to qualified names; `for-each`/`range`/`dotimes` counting loops stay |
| E3, E4 | bracket binding forms; flat `cond` (six call sites in three files of `compiler/`: `jit-demo.fib`, `syntax/lexer.fib` x3, `lair/call.fib` x2, besides the Rust macro `logic.rs` and two Rust test files) | T2 | additive; replacement |
| E8 | `#(..)` and `#{..}` read as forms | T2 | four pinned reader tests turn from errors into reads (`compiler/tests/reader/harness-067-hash-paren.fib`, `harness-068-hash-brace.fib`, `reader-218-dispatch-brace.fib`, `reader-220-dispatch-paren.fib`), and `spec/bootstrap.md` §2 defines no dump line for a form the reader synthesises (the position of the generated `fn`): it must be added in the same commit |
| L1, L7, L8, L3b, L6, L9, L14, L15 | arity clauses, irrefutable patterns in parameters, refutable `let`, tuple patterns, or-patterns, `{:keys ..}`, keyword in call position, `def` initialisers | T2, T3 | each changes the expander or the checker that M6 steps 2 to 4 reproduce |
| L16, L17 | determined variables in an impl context; `derive Debug` and the derive-by-default decision | T2, T3 | the checker; the Rust `derive` module |
| L13 | a diagnostic for an unconsumed recipe (§7) | T2 | a new diagnostic class and a type attribute: the compiler has no warnings today |
| E11 | macro templates resolve in the defining module | T2 | the expander |
| C1 to C7 | closure types, last-use move, exclusive `&`, array primitives, unboxed `Option`, `-O` defaults, move-out of an owned shell | T3 and after | lowering and the ownership checker |
| `format`, `printf` | directives `%s %d %f %x`, with width, zero fill and case: `%4d`, `%04X` | T4 | the compiler's reader dump needs `{:04X}` and `U+0041` (`compiler/util/text.fib`'s `hex-upper`); until `format` has them `hex-upper` stays, and ROADMAP rule 6 puts formatting for diagnostics first, so the directives with width and fill move to **T1** if the owner agrees (§9 Q29) |

ROADMAP M7 rule 4 also asks for a small-vector fast path and `str` building without quadratic copies; the
page has neither, and §9 Q26 recommends the second.

## 7. Language and compiler changes

Ranked by what they unblock for the library, cheapest first within a tier. Sizes are estimates; every
**Evidence** is a program in Appendix A that was run. "Needed by" names the first tranche (§8) that
cannot ship without it; every row also has the stand-in the library uses until it lands.

### 7.1 Stage-1 divergences (method rule 6: the interpreter and the compiler must agree)

| # | Change | Evidence | Size | Needed by |
|---|---|---|---|---|
| B1 | `fibc` lowers a protocol method that has its own type variable; `fibref` accepts it | `(defprotocol (Fold s e) (fold (self f: (fn (r e) r) :borrow init: r) -> r))`: `fibc` says `unsupported: a type variable reached the lowering: Gen(1)`, `fibref` returns 45 (A6). Cause: `method_target` passes only the impl's variables | small | C1 (a method that takes an `Fn` argument), accumulator-typed methods |
| B2 | quasiquote expands to a prelude-qualified `concat`; `` `() `` works under `fibc` | with `(defun concat (a b) ..)` in scope, every quasiquote macro is `concat takes 2 argument(s), got 3` (A6); `` (defmacro u () `()) `` is `macro u failed: concat of nothing` under `fibc` and 1 under `fibref` (A6) | tiny | T1: the library defines `concat` |
| B3 | protocol method symbols are mangled with the defining module | a user `(defprotocol (Collection s e) (conj ..))` with an `impl` for `(Vec a)` and any use of the prelude's `conj` on a `Vec` gives `fibc`: `compile failed: duplicate definition of @m.Collection.conj.$Vec.t0_.str`, while `fibref` accepts the program and the user's `conj` wins (result 0): a divergence in both directions (A6, [R] A10 b3) | small | **T0**: tranche 1's `Collection`, `Seq` and `Indexable` collide with the prelude's until it lands |
| B4 | polymorphic recursion: `fibc` never finishes monomorphising, `fibref` runs | `(defun len (xs: c) :where ((Reducible c e)) -> i64 (if (empty? xs) 0 (+ 1 (len (drop 1 xs)))))` and `(len [1 2 3])`: `fibc` is `memory allocation of 577136 bytes failed` under `ulimit -v 4000000`, `fibref` is `result: 3` ([R] A10 poly). Both tools must reject a function that calls itself at a strictly larger type, with a message that names it (`len recurses at (Dropped c e): polymorphic recursion is not supported; use loop or a List`) | small | T2 (before `rest` ships; without a bound the monomorphiser takes the machine down) |

### 7.2 Language and expander (small to medium each; L16 and L19 are the owner's decisions)

| # | Change | Evidence | Needed by |
|---|---|---|---|
| L1 | **Arity overloading** in `defun`: `(defun name (:arity (a: T) -> R body) (:arity (a: T b: T) -> R body))`, resolved by argument count in the expander; each clause has its **own** `-> R`, `:where` and `:private`, because `get` returns `(Option v)` in one arity and `v` in the other; the explicit `:arity` marker keeps the grammar apart from L7's patterns (with L7, `(defun f ((Pair a b) k: i64) body)` has a list first in its parameters and a clause also has a list first, and the two differ only for a body that is a direct application); the one-clause form `(defun name (a: T) -> R body)` stays; two clauses of one count are an error; a protocol method and clauses of other arities may share a name | two `defun`s of one name: `f is already defined`; a protocol method and a `defun` of one name: `gg is already defined` (A6); `(get m 5 0)` is `get takes 2 argument(s), got 3` (§7 D1 makes the message say `use get-or`) | T2 (`get/3 nth/3 reduce/2 range/n sort/2 sort-by/3 update/4 map/3 subs/2 index-of/2 partition/n repeat/2`); T1 uses the stand-ins of §2.3, deleted when L1 lands. Moving L1 into T1 for two names was considered and rejected (§10) |
| L2 | rest parameters `(defun f (a ... rest))` binding a `(Vec t)`; settle that `&` alone is reserved first | `(defun f (a: i64 & xs) ..)` compiles as a three-parameter function and `(f 1 2 3)` returns 1 (A6): a spec/code disagreement with syntax §1.1 | T3 (user variadics); the library's variadics are macros |
| L3 | `Pair`, `Triple` in the prelude (done in the prototype) | `(. (Pair 1 2) fst)` is `unbound name Pair`; a user-defined generic struct works (A6) | T1 |
| L3b | tuple-like structs (`Pair Triple`) accept vector patterns `[k v]` | `(match (Pair 1 2) ([a b] ..))` is `cannot unify (Pair i64 i64) with (Vec a)` (A6) | T2 (`(fn [[k v]] ..)`, `for [[k v] m]`) |
| L4 | **static protocol methods** (no `self`, dispatched on the type the context expects, as Rust's `Default::default()`) | `(defprotocol Dflt (dflt () -> Self))` is `a method is (name (self qual* x: T qual*) -> type)` (A6); the `Unit` witness works meanwhile (A7) | T3 (`assoc-in` creating levels, generic `sum` of an empty source, `(+)`) |
| L5 | none: `let` does not generalise (types §2.4); the library builds an `Xf` per use or uses the factory form of §2.1 | `Xf` serves two accumulator types (A8) | |
| L6 | or-patterns `(or p q)`, alternatives binding the same names at the same types | `(match [1 2] ((or [a] [a b]) a) ..)` is `or is not a variant or struct` (A6) | T3 (`case`, `condp`) |
| L7 | irrefutable patterns in `fn`, `defun`, `loop` parameters, as sugar for a `let` in the body | `a fn parameter is sym or sym: type` (A6) | T2 |
| L8 | a refutable `let` pattern traps `let: pattern does not match`; `let-else` | `a let pattern must be irrefutable` (A6) | T2 |
| L9 | named-field struct patterns `(Name :field p ..)` and `{:keys [a b]}` on a struct | `(match 1 ({:keys [a]} a) (_ 0))` is `braces are not allowed in patterns` (A6) | T3 |
| L10 | wrapping integer builtins `unchecked-add -subtract -multiply -negate -inc -dec` at the operand's width; `lIR`'s `add sub mul` already carry no overflow flags | `(+ 9223372036854775807 1)` is `trap: integer overflow in + at i64` (A6) | T3 (the multiplicative hash finaliser, generators). **Not T1**: the rotate-and-xor `hash-combine` of tranche 1 needs only `shl shr bit-or bit-xor`, which do not trap ([R] A10 hash) |
| L11 | exact math as builtins (`sqrt floor ceil rint copysign`); `extern` accepted by `fibref` | `(unsafe (fptosi i64 (sqrt 49.0)))` with `(extern sqrt :private (f64) -> f64)`: `fibc` prints 7, `fibref` says `unsupported: extern sqrt is not available in the reference interpreter` (A6) | T4 |
| L12 | `Result` in the prelude, and a scope-exit hook (a `Drop`-like protocol) for non-memory cleanup; **not `try!`**: there is no early return, so the threading form is the block macro `try-let` (§2.10), which needs no language change | `Result` is `compiler/util/result.fib` only; `(defmacro try! (r) `(match ,r ((Ok v) v) ((Err e) (return (Err e)))))` is `unbound name return`, and `try-let` runs, 106 under both tools ([R] A10 try) | T1 (`Result`), T2 (`try-let`), T5 (`with-open`, `line-seq`) |
| L13 | a diagnostic for a value of an adaptor type that is dropped unconsumed, as a statement: **medium, not small**: a new diagnostic class and a type attribute ("must be consumed"). Recommendation: an **error** for a statement of such a type, which needs only the attribute | none: a requirement of P2 (a recipe not run is silent); the compiler has no non-fatal diagnostic of any kind (`grep -rniE warning crates/fibref/src crates/fibc/src` finds one test-harness name) | T2 |
| L14 | a keyword that unifies with `(fn (S) T)` elaborates to `(fn (x) (. x k))` when `S` is a struct with that field and to `get` when `S` is a `(Map keyword v)`; `(:k x)` in head position is the same rule. The narrow rule needs no choice of call-position semantics; the alternative is a reader form `.name` for `(fn (x) (. x name))` | `(:a {:a 1})` is `cannot unify keyword with (fn (a) b)` (A6); `(map :name ps)`, `(sort-by :age ps)`, `(group-by :dept ps)`, `(filter :active ps)` are the commonest Clojure lines, and the group-by-then-count idiom is 46 tokens against Clojure's 29 without it | T2 (moved up from T5; §9 Q11) |
| L15 | `def` initialisers: a total library constructor of constants (`set vec hash-set hash-map zipmap`) is a constant expression, evaluated once before `main` in module order; an `atom` or `cell` is not (P7) | `(def ok: (Set i64) (set [1 3]))` is `def ok: initialiser is not a constant expression`; so are `(def v: i64 (f 2))` and `(def counter: (Atom i64) (atom 0))`, while `(def v: (Vec i64) [1 2 3])` and `(def m: (Map i64 i64) {1 2})` run (A10 def); `#{1 2}` becomes `(hash-set ..)` with E8 and would be rejected by the same rule | T2 (`(def stopwords #{"a" "the"})`, `(def table (zipmap ..))`; §9 Q23) |
| L16 | an impl's context (and a method's own variable) may name a type variable that a constraint determines from the head through a protocol's determined parameter (`(Cursable c k)` determines `k`): the liberal coverage condition, as Jones's functional dependencies give | `(impl (Cursable (Wrap k)) (Src c) :where ((Cursable c k)) ..)` is `type variable k is not a parameter of the impl head` ([R] A10 cur-impl; types §3.3 states the Paterson condition); the same rule rejects an impl for the cursor of an adaptor (`(Mapped c e b)` over `(Cursable c k)`) and a push `Each` protocol whose visitor is a protocol parameter (`push1.fib`), and `a method is (name (self qual* x: T qual*) -> type)` rejects a bound on a method's own variable (`push2.fib`, A10 push) | T2 (adaptor cursors, `(zip xs (map f xs))`); C1's push chain; **the owner's decision** (a type-system change) |
| L17 | `derive Debug` (and the `Show`/`Debug` text of every type of §2.7); whether `defstruct` and `defenum` derive `Eq Ord Hash Show Debug` by default when every field has them (§9 Q22) | `(derive Debug P)` is `cannot derive Debug: only Eq, Ord, Hash and Show` under both tools; `(= (P 1 "x") (P 1 "x"))` is `no implementation of Eq for P` (A10 dd); derived `Show` prints `(P 1 x)` | T1 (`Debug`), T2 (the default) |
| L18 | builtins `(str-byte-at s i)` (one byte, no allocation, traps out of range) and `(str-find s pat from)` (the first byte offset at or after `from`) in both tools | `(str-bytes s)` allocates a fresh array per call: 100 objects for 100 calls (A10 str), so `str/index-of` from an offset is O(n) copying per call and a tokenizer O(n²) | T1 (every string function of §4.7) |
| L19 | an integer literal whose value is exactly representable adopts a float type when it unifies with one; a variable never does | `(* 2 1.5)` is `cannot unify f64 with i64` (A6) | **the owner's decision** (§9 Q24) |
| E1 | the thirty Rust prelude macros of §6.3 (`str println print prn pr swap!`, the variadic folds, `{..}`, `#{..}`, `if-not`, `when-not`, `partial`, `list`), and the retargeted counting loop of `for-each`, `range` and `run!`/`doseq` over a literal range; a macro and the prelude function of one name coexist today (`swap!` over the builtin: 30 under both tools, A10 swap) | `+ takes 2 argument(s), got 3`, `unbound name str`, `println 5` is `cannot unify i64 with str` (A6); the fused loop of `(for-each (range 0 1000) (fn (i) ..))` has no call in `main`, the library spelling has a retain and two indirect calls per iteration (A10 forloop) | T0 |
| E2 | the macro runner compiles all of a program's macros in one module (or runs small ones in the evaluator); calls to functions of required **and used** modules work at expansion time (the macro-time module is the transitive closure of both), and the two tools say the same thing when they do not. **Unproven**: syntax §3.16 says `(var m/f)` works, and no program of the review made it work | 1, 5, 20 macros: 0.20, 0.50, 1.70 s under `fibc` (A9). A macro that calls `(u/bump 3)`, or `((var u/bump) 3)`, of a required module is `macro twice failed: f4.fib:2:46: unbound name u/bump`/`var: no definition named u/bump` under both tools, and with `(:use mu)` and a bare `(bump 3)` `fibc` says `unbound name bump` while `fibref` says `macro twice calls bump, which is not available at expansion time; move bump to a required module`, although the module IS required ([R] A10 mh) | T2 |
| E3 | bracket binding forms accepted beside the parenthesised: `let loop fn doseq for if-let when-let dotimes` | `(let ([a 1 b 2]) ..)` is `malformed let: a binding is (pattern expression) or (name: type expression)` (A6) | T2 |
| E4 | Clojure's flat `cond` replaces `(cond (t e) ..)`; six call sites in three files of `compiler/` are migrated (`jit-demo.fib`, `syntax/lexer.fib` x3, `lair/call.fib` x2) | `(cond (= 1 2) 5 true 6)` is `malformed cond: a clause is (test body+)` (A6); a clause and a flat test cannot be told apart, so it is a replacement | T2 |
| E5 | the expander rejects duplicate literal keys in `{..}` and `#{..}` | `(count {1 2 1 3})` is 1 (A6) | T3 |
| E6 | freeing a linked object is iterative: dropping a long `List` must not recurse (small, runtime) | a 300,000-cell `List` overflows the stack under `fibc run` (`thread 'main' has overflowed its stack`), 50,000 cells are fine (A6); an executable exits 139 (design record) | T2 (`List` is the recursion type) |
| E7 | a library root list (`-I DIR`, `FIB_LIB`, embedded roots) and `(:export-from ..)` | modules load from the main file's directory only (A9) | **T0** |
| E8 | reader macros `#(..)` and `#{..}` reading as `(fn (%1 ..) ..)` and `(hash-set ..)`; the M6 reader and its dump are extended in the same commit | `unknown reader syntax #(: only #_ is defined`, `#{` likewise (A6) | T2 |
| E9 | an on-disk cache of each module's checked interface; demand-driven body checking | §6.4: hypotheses | after 20,000 library lines |
| E10 | hazards found: `Hash f64` and `-0.0`/NaN; **`derive Hash` and the prelude's `Hash (List a)` trap on overflow** (`h*31 + hash x`: replaced by `hash-combine`, §2.7); the `List` variants `empty`, `cons` renamed `Empty`, `Cons`, and the `list` macro expands to qualified names | `(get (assoc (map-empty) 0.0 1) -0.0)` is `nil` while `(= 0.0 -0.0)` (A6); `(hash (P2 "hello" "world"))` of a `derive Hash` struct of two strings and `(hash (list "a" "b" "c"))` are `trap: integer overflow in * at i64` under both tools (A10 hash); `(defun empty ..)` then `(list 1 2)` is `cannot unify (fn :send ((Vec i64)) bool) with (List i64)` (A6, A10 list) | **T0** (`Hash f64`, `derive Hash`, `Hash (List a)`), T2 (`List`) |
| E11 | a free symbol in a macro template resolves in the macro's **defining module** (Clojure's syntax-quote qualification), not at the use site; binders stay unrenamed (`gensym` covers them, syntax §3.16 declines hygiene), or at least the `fib.prelude/` treatment extends to every implicit `fib.*` module | a library macro `` `(first ,v) `` gives 1000, the user's `first`, from a module that defines one, and `no implementation of Seq` from one that does not (A10 hyg); the library's fibber macros (`for get-in case update-in`) and the Rust macros that expand to a `fib.coll` method (`conj assoc dissoc merge`) change meaning when the user writes `(defun filter ..)` or `(defun get ..)` | T2 (the first fibber macro that calls another function) |
| E12 | two `:use`d modules that export one name: an error when the name is referenced unqualified, as `spec/syntax.md` §5 says; the code takes the first `:use` silently | `(:use y.m1 y.m2)` then `(peek 3)` is 3, `(:use y.m2 y.m1)` is 4, under both tools (A10 use). A spec rule and the code disagree; **reported, not changed** (§9 Q27) | T0 |
| E13 | `if-let` expands to `(match e ((some p) a) (_ b))` so that a refutable pattern falls to the else (§2.4) | `(if-let ([a b] o) ..)` is `non-exhaustive match: missing (some [])` (A10 iflet); the same `match` written by hand returns 9 under both tools | T1 |
| D1 | diagnostics: an `Option` where a `bool` is wanted says `use some?, if-let or unwrap-or`; a `bool` predicate where a comparator is wanted says `wrap it in (comparator ..)`; a transducer where a function is wanted says `compose with xf`; a call with one argument too many to `get`, `nth`, `reduce`, `sort`, `update` says `use get-or`, `nth-or`, `reduce1`, `sort-with`, `update-or` (until L1) | `cannot unify (Option (Vec i64)) with bool`, `cannot unify (fn :send (a a) bool) with (fn (a a) i64)`, `cannot unify (Xf i64 i64) with (fn (a) b)` (A10 comp), `get takes 2 argument(s), got 3` | T1 |
| H1 | the case harness reads an object count: a header `allocs: <= N` (a maximum), checked against the `A` lines of `fibc run --trace` (and `fibc itrace`, which gives the same count) | a case header is `spec expect result audit error trap`: no count (§2.5); `fibc run --trace` and `fibc itrace` print identical `A` counts, 2099 for the chain of A1 | **T0** (the count cases of §8.1 item 5 cannot be written without it) |

### 7.3 Compiler performance (medium each; the gates of P2 and P3)

| # | Change | Evidence | Size | Gates |
|---|---|---|---|---|
| C6 | `fibc build` optimises at level 2 by default; `fibc run` stays at level 0 with a flag for 2 | `JitOptions::default()` and `Options::default()` are level 0 (read in `crates/lair/src/jit/mod.rs`, `aot.rs`, `fibc/main.rs`); `-O 2` leaves five indirect call sites on the chain (A1); **the change is not tiny**: a main of 17,256 lIR lines takes 0.40 s at `-O 0` and 1.00 s at `-O 2` under `lair run` (A10 timing), so a default of 2 for `run` multiplies the cost of every run by 2.5 | small for `build`, medium with the instance cache of §6.4 | C1's payoff, at build time |
| C2 | **a last use moves**: when a variable's last use on every path is the argument of a call or a `cell`/constructor, `consume` moves instead of retain-then-release; loop variables move into the call before `recur` rebinds | `bump` called in a loop: 1001 arrays for 1000 calls; `explain`: `arg 1 acc: retain` (A4) | medium | P3: in-place `conj`/`assoc`/`update` with no API change |
| C3 | **exclusive `&`**: when `&x` is the only mention of `x` and `x` is not captured, copy-in shares the object without acquiring | `(push! &v i)` 1000 times: 2094 objects, `&` copy-in acquires (A4) | small-medium | `push!`, `map-put!`, builders |
| C4 | array primitives for the library: `(array-push! &a x)` with capacity, `(array-update! &a i f)` that moves the slot out and stores the result, an uninitialised `(Array a)` | `array` needs an element, so no generic empty array exists and the prelude has `VecEmpty` (prelude comment) | small-medium | the in-place trie and tail |
| C1 | **closure types**: a `fn` literal passed to a parameter of a type variable bounded by the built-in `Fn` is a nominal type whose captures are fields, a monomorphisation key; `(fn (A) R)` stays the dynamic type; **an erasure rule at joins** (two closure types, or two adaptors holding them, meeting in an `if` or `match` unify at the erased `(fn (A) R)` of the parameter; §2.2) | functor emulation of a **pull** chain: 0 indirect calls, 0 retains per element, and `-O 2` leaves none on the chain (A2), at the price of 1500 calls of `f` for 1000 elements (A10 d1c); the **push** form cannot be declared today (A10 push) and needs B1 and L16; the impl head on `(fn ..)` is rejected as it stands (A6); with a type per closure `(if flag (map2 (Inc ..) v) (map2 (Dec ..) v))` is `cannot unify (Mapped (Vec i64) Dec) with (Mapped (Vec i64) Inc)` (A10 c1emul) | medium to large (with the erasure rule and L16) | P2 at Rust speed; `sequence`/`eduction` |
| C5 | unboxed `(Option scalar)` and small enums and structs by value (lIR already has by-value aggregates) | 100 calls returning `(Option i64)` and `(Step i64)`: 200 allocations (A5); `get` on a `(Map i64 i64)` allocates per lookup; `(first v)` in a loop of 1000 is +1000 objects, `zip` +1003, `keep` +1002 (A10 alloc) | medium-large | `get`, `first`, `reduce-while`, `Step`, `zip`, `keep`; scheduled with tranche 3, whose count cases for these stay `open` until it lands |
| C7 | a `match` or field read on an owned shell that is dead afterwards **moves the payload out**: no retain of the field, no release of it with the shell | `bump` that matches an owned `(E1 n arr)` and writes `arr` through a cell: four chained calls on unique temporaries cost 10 objects (A10 own); a `self :owned` method on a struct with an array field, 1000 times in a loop: 2002 objects, `explain`: `(cell ..) arg 1 (. self a): retain`, `release [self] (exit)`; a struct held in a cell updates in place: 1000 `set-field! &c n ..` allocate nothing | medium | P3 for the existing `Vec` and `Map` (C2 to C4 fix only the caller's side, §2.5); the alternative is the struct `Vec` of §9 Q18 |

An alternative to C1 that was proposed, specialising a callee on a literal closure passed to a
non-escaping `:borrow` parameter, makes the visitors of `each-while` direct but not a closure stored
in an adaptor struct, so it does not reach the chain's user functions; it is not adopted (§9 Q17).

**What is not needed.** A transient API; a Perceus-style reuse of matched shells (large; considered only
if C2 to C4 leave the `Vec` header as the measured bottleneck); a rank-2 type; callable collections; a
second sequence protocol.

## 8. Implementation plan

### 8.1 How every tranche is judged

method.md applies: nothing is done until a test that can fail says so. A tranche is done when:

1. **Every function of its rows has an `accept` case** in `cases/stdlib/`, run in the interpreter
   (memory audit clean) and compiled, with equal results (method rule 6; `fibc cases cases/stdlib`).
   A case's header lists the names it covers (`;; covers: map filter take`). A test, `stdlib_table`,
   parses §4 of this page and fails if a row with `T` at or below the current tranche has no case that
   covers it, or if a case covers a name that is not in the table: the table is executable.
2. **Differential tests against a naive reference written in fibber** (`cases/stdlib/ref/`): every
   library function that has an obviously-correct eager version (a `Vec` loop of five lines) is run
   against it over a few hundred generated inputs (a seeded xorshift in fibber; never thousands: the
   machine has been taken down by sweeps before). The data structures get model tests: `Vec`'s trie
   against a plain array across counts 0 to 2000 (crossing the 32 and 1024 boundaries), `Map` and `Set`
   against an association `Vec` over random operation sequences with a key type whose hash is `x mod 7`
   (collision buckets), sorted collections against a sorted `Vec`, with an invariant checker run after
   every operation.
3. **Generated programs against a model** (method rule 5): `fibgen` is extended with `->>` pipelines of
   the adaptors over small `Vec i64`s and pure generated lambdas; the generator evaluates the pipeline
   itself and embeds the expected result as a check. A failure is minimised and becomes a case.
4. **Laws**: `=` implies equal `hash`; `compare` is antisymmetric, total (NaN included) and agrees with `=` and
   `<` everywhere except at NaN, where `(= nan nan)` is false and `(compare nan nan)` is 0; `sort` is stable
   and a permutation; `(into (empty c) c)` equals `c`; each on generated nested values, floats with NaNs
   among them. A hash law over strings and over structs of strings runs to lengths where the old combiner
   trapped (§2.7).
5. **Count cases** make the zero-cost claims executable (`FIB_TRACE=1` counts, read by the harness header
   `allocs: <= N`, §7 H1, which does not exist yet): a three-stage pipeline over n and over 10n elements
   allocates the same number of objects; a bulk `vec` of n allocates at most n/32 + c; a `conj` loop of n
   allocates at most n + c once C2 to C4 and C7 land, and at most c once `Vec` is a struct (§9 Q18); an
   `assoc` loop of n has a bound of its own, which this page does not guess (1000 `assoc`s allocate 7935
   objects today, A4, re-run by the compilability critic: every path node is a shell with the same field
   problem as `Vec`, so it follows C7). The first is required from tranche 1 and passes today (A1); the
   second is required once the `Vec` module's bulk builder lands (**[H]**, §2.5); the others are listed as
   `open` in the harness until their changes land, and a run prints them as failing, never as pending
   (method.md: pending is not pass). The gate of tranche 3 is therefore `conj` at most n + c, not c: the
   first version required a bound that its own §2.5 said the design could not meet.
6. **A mutation review** of each tranche's source, as the reader had (spec/bootstrap.md §3): mutants of
   the fibber source run against the tranche's tests, and each survivor becomes a case. The review ran nine
   mutants of the prototype against `t1.fib`: six were caught, **three survived** (`sort` made unstable,
   `update` ignoring the old value, `cycle` of an empty source, which loops forever). With three checks
   added (stability, `update` on a present key, `cycle` of an empty source) and three more mutants
   (`nth` ignoring its index, `sort-by-with` reversed, `compare` of two NaNs), all twelve are caught, two by
   not terminating (an inverted `Filtered` and the `Cycle`), which a harness with a timeout reports as a
   failure ([R] A10 mut).
7. **The compiler is the integration test**: stage 2 (M6) replaces its hand-written helpers with library
   calls as each tranche lands, and `cargo test -p fibc --test bootstrap` (byte-identical reader dumps)
   stays green.

### 8.2 Tranches

| T | Content | Needs (§7) | Test emphasis | Rows |
|---|---|---|---|---|
| 0 | the library root and re-export; the thirty Rust macros of §6.3; `Hash f64`; `derive Hash` and `Hash (List a)` over `hash-combine`; the quasiquote fix; the protocol-mangling fix; the duplicate-`:use` check (if the owner agrees, §9 Q27); the harness's `allocs` header; the disjointness test of the four implicit modules as a CI gate | E7, E1, B2, B3, E10, E12, H1 | cases for each macro's expansion text; a `hash` of 14 strings and of a two-string struct that does not trap; both tools agree on B3's program | |
| 1 | **what stage 2 needs.** `Pair Triple Step Result Unit`; `unwrap unwrap-or map-opt and-then`; `min max abs inc dec mod compare desc`; `hash-combine hash-ordered-coll hash-unordered-coll`; `Eq Ord Hash Show Debug` for `Vec Map Set Pair Triple` (`derive Debug`); `Reducible`, its sources (`Vec List Array Option Range Map Set Chars`) and `map filter remove take drop take-while concat mapcat`; `reduce reduce1 reduce-while first last nth any? every? none? find-first find-map count empty? run!`; `into vec set` (bulk builders); `sort sort-by sort-with sort-by-with reverse`; `Lookup Assoc Dissoc Keyed Collection Emptyable Stack` with `get get-or assoc dissoc update update-or update-opt fnil contains? includes? keys vals key val merge group-by frequencies select-keys`; strings: `str`, and qualified `str/join str/split str/split-lines str/trim str/triml str/trimr str/index-of str/includes? str/starts-with? str/ends-with? str/blank? str/chars`, with `subs str-len str-byte-at str-find`, `parse-long`, `digit? whitespace?`; `println print prn`, and their generic twins; `write-file`; the builtins' names as §4.1 | T0, L3 (`Pair`, `Triple` in the prelude), L12 (`Result`), L17 (`Debug`), L18 (`str-byte-at`, `str-find`), E13 (`if-let`), D1 | model tests for `Vec Map Set` and `sort`; laws; a hash law over strings; the integration test | 155 |
| 2 | **the Clojure surface.** the rest of the sequence functions (`partition* interleave zip zip-with seq-of seq= map-indexed keep reductions distinct interpose drop-while rest second mapv filterv repeat iterate cycle`), `Cursor`; `for doseq cond if-not when-not as->`; `partial partial2 complement constantly`; `with try-let`; the keyword in call position; the arity forms of §4 on L1, deleting the stand-ins; patterns in parameters, bracket forms | L1, L3b, L7, L8, L14, L15, E3, E4, E8, E11, L13, E2, B4, E6 (`List`), E10 (`List`), L16 (adaptor cursors, if the owner accepts it) | the 60 programs of the design record (20 each from three designs, all ran) ported as cases; expansion texts; a recipe sent to a task; polymorphic recursion rejected by both tools | 56 |
| 3 | **collections.** `get-in assoc-in update-in update-keys update-vals`; `condp case juxt dedupe`; `fib.set`; `fib.sorted` (B-tree, `Queue`); `Slice`/`subvec`; `rseq`; `Xf`, `Stepper`, `transduce`, `into/3`, `xf`; the multiplicative hash finaliser; `unchecked-*`; the in-place paths of `Vec` and `Map` (C2 to C4 and C7, or the struct `Vec`, §9 Q18) | L2 (user variadics), L4, L6, L9, L10, E5, C2, C3, C4, C7, C5 (the `first`/`get`/`zip` count cases stay `open` until it lands) | B-tree model with `valid?`; the `conj` count case at n + c moves from `open` to required | 84 |
| 4 | **strings and numbers.** `fib.string` in full, `fib.char`, `fib.regex` (non-backtracking), `fib.math`, `fib.random`, `fib.walk`, `fib.sys`, `fib.io`, `format printf` | L11, L12 | strings against a byte-loop reference over generated UTF-8 (every scalar width, boundaries); regex against a bounded backtracking reference; math bit for bit between the interpreter and the compiled program on 3000 generated patterns, and against libm in a Rust test (exact functions equal, the others within a stated number of ulps); `Rng` against known first values | 76 |
| 5 | **the long tail.** `fib.lazy` (`LSeq`), `Delay Promise` helpers, `pprint`, `fib.data` (`Val`, `diff`), `sequence eduction` (after C1), `iteration`, `subseq`, the relational `set/project index join` | C1, L12 | as above | 20 |

The Rows column counts the survey names assigned to the tranche in §4 (155 + 56 + 84 + 76 + 20 =
391 names, plus 43 names Clojure lacks that the library adds; the other 282 survey names are omitted or
replaced by another construct). Tranche 1 includes the core forms and builtins that already exist. The Needs
column was checked against §7's "Needed by" column row by row in the review; the first version omitted B3,
L3, L12, L10, L2, E6, E10 and C5.
After tranche 3 the library is *viable* in ROADMAP rule 5's sense, and the benchmark suite (fibber
and Rust kernels, a ratio per kernel against the aim of within 1.5x) is designed then, not here.

### 8.3 Migration of the prelude

A. Add the library modules beside `fib.prelude`; names that collide (`map count range first rest`) are
shadowed by the `:use`, which the module system does silently ([R] A6), and the library's protocols that reuse a
prelude name (`Collection`, `Seq`, `Indexable`) need §7 B3 first, or step C in the same commit as step A.
B. Port the cases that use the names being replaced. Counted by grep over the 191 `.fib` files of
`cases/ownership`: `range` in 7, `for-each` in 5, `pmap` in 4, `filter-iter` in 2, `iter` in 2, `(map` in 1,
`collect` in 1, `Countable` in 1, and, found by the review, `first` in 2 (cases 01 and 61: `first` on a `List`
changes type from `e` to `(Option e)`), `(list ` in 4, `(next ` in 2, `append` in 7, `length` in 3,
`set-contains?`, `map-put!`, `map-del!` and `disj` in 1 each, and `(. e key)`/`(. e val)` on a map entry in case
181 (the `Entry` struct is removed for `Pair`, §2.3); §6.5 lists every prelude name. C. Delete `Traversable`,
`Iter`, `Seq`, `Indexable`, `Countable` (with `Countable str`, which counts bytes), `Associative`, `for-each`,
`iter`, `collect`, `filter-iter`, `VecIter`, `append`, `length`, `Entry`, and the Vec-only `map`; make the four
implicit modules the implicit `:use`. D. Move `Vec` and `Map` out of `lib/prelude.fib` with their
layout unchanged (`fibc`'s `rt/vec.lir` and `fibref`'s `eval/vecs.rs` read it). No case counts a string
literal and `compiler/` calls `str-len` (19 uses), so making `count` of a `str` a type error costs nothing
that was found; the compiler's own errors list any other use.

## 9. Open questions for the owner

The owner prefers a recommendation to an open question; each row has one, with the evidence it rests
on. "Decide" means the owner's sign-off turns it **Decided** in the page it changes.

| # | Question | Recommendation | Why |
|---|---|---|---|
| Q1 | Do `first last peek get find max-key` return `Option`, with a trap for the sure case? | **Yes**, and one `(unwrap o)`; no `first!` family. | the `nil` rule (§2.4); `!` already means mutation; `nth` is the trapping `first` |
| Q2 | Does `List` stay a first-class collection beside `Vec`, and what are its variants called? | **Stay** (it is the persistent recursion type and the `list`/`cons` idiom); rename the variants `Empty`, `Cons`. | `empty` and `cons` clash with Clojure's functions and the `list` macro is unhygienic and now expands to qualified names ([R] A6, A10 list) |
| Q3 | What is the unit of a string? | **`str` is not a collection**; bytes (`str-len`, every offset), characters through `chars`. | S18; the hazard `(subs s 0 (count s))` disappears because `count` of a `str` does not compile |
| Q4 | `compare` and the comparator convention | **`compare` returns -1/0/1 as `i64`; a comparator is `(fn (e e) i64)`**; a predicate is not a comparator. | Clojure's own, without its predicate quirk (S44) |
| Q5 | Duplicate keys in a map literal | **Reject duplicate literal keys** at expansion; a computed duplicate: last wins. | Clojure's reader rejects them; the `assoc` desugaring hides data loss |
| Q6 | `update-keys` and `map-invert` collisions | **Trap** `update-keys: duplicate key`. | no silent loss; the lossy path is `into` |
| Q7 | Is `f64` `Hash`? | **Yes**, hashing `-0.0` as `0.0` and one NaN; a NaN key is never found, as in Clojure. | `(= 0.0 -0.0)` with different hashes is a bug ([R] A6); making `f64` not `Hash` would break `frequencies` over floats |
| Q8 | `round` | **Half up** (toward positive infinity), as Java's `Math.round` and Clojure's. | rule 1; `rint` is half to even |
| Q9 | Reader macros | **Add `#(..)` and `#{..}`** (read as ordinary forms); **no regex literal**: `(re "..")` is a macro that checks the pattern at compile time. | `#()` is the biggest ergonomic gap; the M6 reader must change in the same commit (E8) |
| Q10 | `Show` and `Debug`; is `println` a variadic macro? | **Yes to both** (§2.7, §2.10). | S16, S17; every existing `(println "text")` keeps working |
| Q11 | Is `(:k x)` sugar for a field or a `get`, and when? | **Yes, in tranche 2, as the narrow checker rule L14** (a keyword that unifies with `(fn (S) T)` elaborates to a field access or a `get` by `S`); the first version said tranche 5. If the checker rule is too much, a reader form `.name` for `(fn (x) (. x name))`. | a macro cannot choose; the commonest Clojure lines are `(map :name ps)`, `(sort-by :age ps)`, `(group-by :dept ps)`; field access on an unannotated lambda parameter already infers when the collection fixes the element type ([R] A6, programs t4 to t9) |
| Q12 | `-by`, `-with` or `-key`? | **`-by` key function, `-key` Clojure's two, `-with` comparator or combiner** (§3 N4). | S26 |
| Q13 | Recipes that re-run, or memoising sequences? | **Recipes**; `LSeq` for recursive definitions in tranche 5. | memoisation costs a cell and a closure per element and fibber has no GC to make that cheap |
| Q14 | Push or pull as the primitive? | **Push (`Reducible`) with a pull `Cursor` for lockstep walks, conditional on L16 (Q30).** If the owner declines L16, the push chain cannot reach 0 indirect calls and the choice reopens. | §2.1: pull is cheaper today (3 indirect calls against 6) and, with static closures, reaches 0, but runs `f` twice per surviving element (1500 calls for 1000, A10 d1c); push is one loop per source, early exit is a value, no per-element `Option`, and its static form cannot be declared today (A10 push) |
| Q15 | Checked arithmetic and the hash | **Keep the trap by default; add wrapping builtins (L10, tranche 3); `fibc build` at `-O 2`, `fibc run` stays at `-O 0` (C6); ship the rotate-and-xor `hash-combine` in tranche 0 and 1 so no combiner traps; replace the integer hash by a 64-bit finaliser in one commit with types §2.12 and the cases' expected orders.** | every `+` is `sadd-overflow` plus a branch (it blocks vectorisation) and Rust's release build wraps; today's `h*31 + x` combiners trap on two strings (A10 hash); the multiplicative finaliser needs a wrapping multiply; iteration order changes once, in both implementations together, and printed maps do not change at all (§2.7) |
| Q16 | Clojure's binding vectors and flat `cond` | **Accept `[a 1 b 2]` beside `((a 1) (b 2))` (additive); replace `cond` by Clojure's flat form.** | E3 breaks nothing; E4 breaks six call sites in three files of `compiler/`, mechanically |
| Q17 | Closure types (C1) or fusion macros? | **C1; no fusion macros.** | a fused `->>` covers only a visible literal pipeline, costs 80 to 95 ms per distinct macro, and is Rust-side code that stage 2 must re-implement; C1 serves every call site |
| Q18 | Does `Vec` become a struct with a spare-capacity tail, held in a cell and updated through `set-field!`? | **Yes, decided before tranche 3, not after its counts**: the struct `Vec` plus C2 and C3. C7 (moving a payload out of an owned shell) is the alternative that keeps the enum, and is a medium ownership-checker change that also serves `Map`. | the evidence moved: C2 to C4 alone do not make the existing `Vec` update in place (2002 objects for 1000 puts on an owned struct with an array field, A10 own), a struct held in a cell updates in place today (1000 `set-field!` allocate nothing), and a required count case cannot wait for a measurement that §2.5 already predicts; the layout is shared by `fibc` (`rt/vec.lir`, `lower/pattern.rs`, `macros/abi.rs`) and `fibref` (`eval/vecs.rs`), so both change together |
| Q19 | Which macros are Rust? | **The thirty of §6.3 (ten more than the first version: `swap!`, `bit-and bit-or bit-xor`, `{..}`, `#{..}`, `if-not`, `when-not`, `partial`, `list`), then shrink the list once E2 lands.** | stage 2 pays for each Rust macro; every program pays 80 to 95 ms for each fibber macro it uses, and the ten are on every program's path |
| Q20 | Heterogeneous data (`{:a 1 :b "x"}`) | **A `Val` enum in `fib.data` with `get-in`** for JSON-shaped data; structs for records. | the type system has one value type per map |
| Q21 | Transducers | **`Xf` as a factory of steppers with a flush (§2.1); `xf` composes left to right; `sequence` and `eduction` after C1.** | one value serves two accumulator types today ([R] A8); a stateful transducer needs the flush (`partition-all` loses its last group without it, A10 xf); the recipe forms hit the `:borrow` rule |
| Q22 | Do `defstruct` and `defenum` derive `Eq Ord Hash Show Debug` by default? | **Yes, when every field has them; `(derive ..)` stays for the rest, and a qualifier opts out.** | a Clojure record is `=`, `hash` and printable by default; today `(= (P 1 "x") (P 1 "x"))` is `no implementation of Eq for P`, and a struct as a map key is `no implementation of Hash for P` (A10 dd); `derive Debug` does not exist yet (L17) |
| Q23 | `def` initialisers, and a top-level `atom` | **Evaluate total library constructors of constants once before `main` in module order (L15); no top-level `atom` or `cell`.** | `(def stopwords #{"a" "the"})` and `(def table (zipmap ..))` are common and fail today (A10 def); a global mutable cell is the global state P7 rules out, and an `Atom` passed as an argument is the replacement |
| Q24 | Does an integer literal adopt a float type? | **Yes, as a literal-only rule (L19)**: a literal whose value is exactly representable adopts `f64` or `f32` when it unifies with one; variables never do. | `(* 2 x)`, `(/ x 2)`, `(+ x 1)` on doubles are everywhere in numeric Clojure, and today are `cannot unify f64 with i64` ([R] A6); it changes types §2.12 in both tools, so it is the owner's call, and Rust's answer (no) is the other defensible one |
| Q25 | `try-let` or an early-return form? | **`try-let` (a block macro, no language change).** A `return` core form would need a ruling on every live local released at the exit (types §6.3), `loop` and tail calls, and the ownership checker. | `try!` as a macro is `unbound name return` (A10 try); `try-let` runs |
| Q26 | A string builder (ROADMAP M7 rule 4: `str` building without quadratic copies) | **`StrBuf` in `fib.string` over a growable `(Array i8)`, after C4; until then `str/join` and `format` build in one pass.** | the page had neither; `str-concat` in a loop copies; the compiler's own `hex-upper` and reader dump need building text |
| Q27 | `:use` collisions: the first `:use` wins, `spec/syntax.md` §5 says an error | **Make the checker raise the error §5 documents, with a case; until then the disjointness test of §6.2 is the protection.** | spec and code disagree (A10 use); this page changes neither, per method.md |
| Q28 | Porting case 01, which is one of the 20 owner-decided cases | **Sign off the port of `(first xs)` on a `List` to `(Option e)` before it is touched.** | case 01 and case 61 use `first` on a `List`, which returns `e` today (§6.5) |
| Q29 | `format` directives with width, fill and case (`%04X`) in tranche 1 | **Yes: `%s %d %f %x %X` with width and zero fill in tranche 1, the rest of `format` and `printf` in 4.** | ROADMAP rule 6 puts formatting for diagnostics first, and the reader dump's `{:04X}` and `U+0041` are `compiler/util/text.fib`'s `hex-upper` today |
| Q30 | L16: impl contexts and method variables may name a variable that a constraint determines | **Yes.** | it is the one rule that lets an adaptor have a cursor (`(zip xs (map f xs))` works in both orders) and a push chain have static visitors (Q14); today `type variable k is not a parameter of the impl head` (A10 cur-impl); it is the liberal coverage condition, not an ambiguity |
| Q31 | One tuple type: `Pair` replaces the prelude's `Entry` | **Yes.** | `(into {} (zip ks vs))` and `(into {} (map (juxt f g) xs))` are the two commonest ways to build a map and are `cannot unify (Pair ..) with (Entry a b)` with two types; a protocol has one `Collection` instance per head, so a `Map` cannot take both (`overlapping instances: Collection for (Map k v)`, A10 pair); one case uses the fields (`(. e key)`, case 181) |
| Q32 | `update` hands the function the value, not an `Option` | **Yes: `update` traps on a missing key as Clojure throws; `update-or` and `update-opt` (with `fnil`) are the other two.** | `(update m :n inc)` is the commonest `update`, and the first version rejected it (`cannot unify (fn :send (i64) i64) with (fn ((Option a)) a)`, A10 update) |

## 10. Review record

Three critics read this page against the compiler: ergonomics (21 findings: 30 Clojure idioms written beside
the page's API), compilability (19: every code block and every claim about the compiler re-derived) and
consistency (26: the tables against each other, the survey, the repository and the other specs). The verdicts
are **accepted** (the page changed), **accepted in part** (the page changed and the rest is rejected, with the
reason), **moved to §9** (the change needs the owner, with a recommendation) and **rejected** (with a reason). A
finding's "what changed" names the section; the programs that decided it are in A10. Where a finding repeats
another critic's it points to it. Nothing in the repository was edited: this page is the only file written, and
two spec/code disagreements it found are reported (§9 Q27, §7 B4), not resolved.

### 10.1 Ergonomics

| Finding | Verdict | What changed |
|---|---|---|
| E01 `with` named and never defined | accepted | `with` is a row of §4.2 (T2), a macro that runs today (A10 with); §2.6 and §5 row 58 point to it |
| E02 keyword-as-function deferred to T5 | moved to §9 | `(:k m)` is T2, §7 L14, §9 Q11 recommends the narrow checker rule (or a `.name` reader form) |
| E03 `(update m k inc)` does not compile | accepted | `update` hands `f` the value and traps on a missing key; `update-or` and `update-opt` are the other two (§2.4, §4.5, §5 row 74, §9 Q32; A10 update) |
| E04 a recipe has no `Show`/`Eq`, a struct neither | accepted in part | one `Show` per adaptor over `show-seq`, `seq=` for equality across `Reducible`s (A10 showseq); derive-by-default is §9 Q22 (L17, §5 row 82). Rejected: `=` between a recipe and a `Vec`: `=` is on one type (§2.7) |
| E05 the second operand of `map`/`zip` must be a `Vec`, `Range` or `Array` | accepted in part | §2.1 rule 5 states the operand order and the error; the cursor of an adaptor cannot be written (A10 cur-impl), so the fix is §7 L16 and §9 Q30. Rejected: a `pairs` name, because `(zip-with f (rest xs) xs)` already works and a name that needs L16 anyway adds nothing |
| E06 recursion on `(rest xs)` makes `fibc` run out of memory | accepted | §7 B4 (both tools reject polymorphic recursion), §2.1 (A10 poly) |
| E07 `def` accepts only literals | moved to §9 | `def` row, §7 L15, §9 Q23, P7 |
| E08 `swap!` with arguments; `run!` with a non-unit function | accepted | `swap!` is a Rust macro over the builtin (E1, A10 swap); `run!` takes any result (A10 run) |
| E09 descending sort has no tranche-1 spelling | accepted | `sort-by-with` and `desc` (new, T1), the diagnostic of §7 D1; ran (A10 sortby) |
| E10 a macro cannot call a function of any module but the prelude | accepted | §7 E2 rewritten with the evidence and marked unproven, §7 E11, §6.3 (A10 mh, hyg) |
| E11 two tuple types | accepted | `Entry` and `entry` are removed, `Pair` is the one tuple type (§2.3, §4.3, §4.5, §5 rows 26 and 78, §9 Q31; A10 pair) |
| E12 the printed form of an entry, a pair, an `Option` | accepted in part | `Pair` and `Triple` print `[a b]`, with a table of texts in §2.7. Rejected: an `Option` printing as the bare `x`: `(Option (Option a))` would be unreadable, so it prints `nil` or `(some x)` |
| E13 no `apply`, no `(max c)`, no transpose | accepted in part | the `apply` row of §4.17 lists the spellings of the usual uses. Rejected: `max-of`/`min-of` (`(unwrap (reduce max xs))` is the one way, P6) and `transpose` (needs a typed matrix) |
| E14 an integer literal does not adopt a float type | moved to §9 | §7 L19, §9 Q24 recommends a literal-only rule; the owner's call |
| E15 `println` and `str` cannot be passed as values | accepted | function twins generic over `Show` (§2.10, §4.14; A10 twin) |
| E16 `comp` of transducers is a type error | accepted in part | the diagnostic names `xf` (§7 D1), §5 row 44 says `comp` may dispatch on `Fn` and `Xf` after C1. Rejected for now: one polymorphic `comp`, because an impl head on a function type is rejected today (A6) |
| E17 `(if (seq xs) ..)` is a type error | accepted in part | the `seq` row says so and gives `(not (empty? xs))`, and §7 D1 improves the message. Rejected: a `non-empty?` name (P6: one way) |
| E18 `count` of a size-preserving recipe skips the function | accepted | §2.1 rule 4 and the `count` row (A10 count) |
| E19 `sort-by` calls the key per comparison | accepted | the key is computed once per element: decorate, sort, undecorate (§4.4; A10 sortby: 5 calls for 5 elements) |
| E20 map print order differs from Clojure's | accepted | `Show` and `Debug` of a `Map` and a `Set` print in the order of the keys' `Show` text, independent of the hash (§2.7, §5 row 28; A10 showmap) |
| E21 `(get m k 0)` and `(nth v i d)` are arity errors in T1 | accepted in part | `get-or` and `nth-or` rows say so and §7 D1 makes the error say `use get-or`. Rejected: moving L1 into T1 for two names (L1 is a grammar change that must be designed with L7, finding consistency C22) |

### 10.2 Compilability

| Finding | Verdict | What changed |
|---|---|---|
| C01 push plus C1 has no evidence | accepted | §2.1 and §2.2 now say the 0-and-0 chain is pull and runs `f` 1500 times, and that push needs §7 L16 (the three rejected declarations are quoted); §9 Q14 is conditional on Q30; A2 text corrected (A10 push, d1c) |
| C02 C1 makes valid programs ill-typed | accepted | C1 includes an erasure rule at joins (§7 C1, §2.2; A10 c1emul) |
| C03 C2 to C4 do not make the existing `Vec` update in place | accepted | §2.5 rewritten, §7 C7, §9 Q18 now recommends the struct `Vec` before tranche 3 (A10 own) |
| C04 the required count cases contradict the analysis | accepted | the T3 gate is `conj` at most n + c; `assoc` has its own bound after C7; the harness header `allocs` is §7 H1 in T0 (§2.5, §8.1 item 5) |
| C05 "nothing is allocated per element" is false | accepted in part | P2, §2.1 rules 2 and 4 qualified with the measured counts; `to-vec` makes `vec` of a `Vec` the identity; `Chars` is a decoder, 2 objects for 1000 characters (A10 alloc, nth, chars). Rejected: `zip` visiting two arguments (that is what `zip-with` is: +4 objects against +1003) and a Map walk by `each-kv-while` as the primitive (`reduce-kv` already is; the leaf storing the `Pair` is [H] and C5) |
| C06 library macros cannot name their own module's functions | accepted | §6.3 paragraph, §7 E11 (A10 hyg) |
| C07 `try!` cannot be a macro | accepted | `try-let` replaces it (§2.10, §4.3, L12, §9 Q25; A10 try) |
| C08 the compile-time model is per line, the cost per instantiation | accepted | §6.4 rewritten with the two costs, a new plan item (the instance cache), C6 narrowed to `build` (A10 timing) |
| C09 recipes are not conditional-friendly | accepted | §2.1 "A recipe is a type", `seq-of` (new, T2), §5 row 81 (A10 join) |
| C10 recipes cannot cross a task boundary | accepted | the colour parameter on adaptor structs (§2.1; A10 col) |
| C11 `Xf` has no completion step | accepted | `Stepper` with a flush (§2.1, §9 Q21; A10 xf) |
| C12 complexity and law statements that do not hold | accepted | `empty?` stops at the first element produced, `Range` size is closed-form, `compare` is total, `max`/`min` return NaN (§2.1, §2.7; A10 count, rangesize, nan) |
| C13 `if-let` refutable patterns and the `:as` spelling | accepted | §2.4 row and §7 E13 (A10 iflet) |
| C14 `with` used and never defined | accepted | same as ergonomics E01 |
| C15 expansion-time helper calls fail today | accepted | §7 E2 marked unproven, the two messages quoted (A10 mh) |
| C16 string functions pay a full copy per call | accepted | §7 L18, the rows `str-byte-at` and `str-find`, §2.9 (A10 str) |
| C17 a `:borrow` collection parameter cannot feed an adaptor | accepted | §2.1 (3) and §2.2 conventions (A10 bw, sortby) |
| C18 three of nine mutants survive | accepted | §8.1 item 6; three checks added, three mutants added, all twelve caught (A10 mut) |
| C19 `println` spacing and the `list` hazard text | accepted | A6 row corrected (the probe joined without a separator), the library's macro joins with a space (A10 pm); `cons` is in E10's text |

### 10.3 Consistency

| Finding | Verdict | What changed |
|---|---|---|
| C01 (blocker) every combiner of T1 traps on overflow | accepted | `hash-combine` is a rotate-and-xor mixer from builtins that exist, so L10 stays in T3; `derive Hash` and `Hash (List a)` move to it in T0; rows `hash-combine`, `hash-ordered-coll`, `hash-unordered-coll` are T1; §2.7, §7 E10, §9 Q15 (A10 hash) |
| C02 B3 is not in tranche 0 | accepted | B3 is T0 and in T0's Needs; §8.3 step A says so (A10 b3) |
| C03 `try!` has no implementation | accepted | same as compilability C07 |
| C04 two `:use`d modules: the first wins, not an error | accepted | §6.1 corrected, §6.2 makes the disjointness test a T0 gate and deletes `compiler/util/result.fib` with `Result`; the spec/code disagreement is §7 E12 and §9 Q27, reported and not resolved (A10 use) |
| C05 the prelude names that are not Clojure names have no disposition | accepted | §6.5 has a row per public prelude name with counts; §8.3 B recounts; case 01 needs the owner's sign-off (§9 Q28) |
| C06 the deviation table lacks the library's own deviations | accepted | §5 rows 74 to 82 |
| C07 the Needs columns omit language items | accepted | §8.2 Needs rebuilt from §7 "Needed by" (B3, L3, L12, L2, L10, E6, E10, C5 added) |
| C08 the fused counting loop is removed and the replacement is slower | accepted | §6.3 and §7 E1: `run!` and `doseq` over a literal range expand to the counting loop (A10 forloop) |
| C09 `last` O(1) unless `Indexable` cannot be written; `nth` rejects recipes | accepted | `nth`, `last`, `to-vec` are `Reducible` methods with walking defaults; `Indexable` is removed (§2.1, §2.3; A10 nth) |
| C10 `compare` and `Eq` disagree on NaN | accepted | `compare` on `f64` is a total order, the laws exclude only `=` at NaN (§2.7, §5 row 16; A10 nan) |
| C11 two tuple types | accepted | same as ergonomics E11 |
| C12 `empty?` always O(1), adaptors allocate nothing | accepted | same as compilability C05 and C12 |
| C13 `derive Debug` does not exist | accepted | §7 L17 and the text of every shown type in §2.7 (A10 dd) |
| C14 stale or contradictory cross-references and names | accepted | each of the seven items fixed: `write-line` removed, `chars` is `str/chars`, `flat-map` is `mapcat`, the protocols of §4.17 renamed, names without a row given one (`seq=`, `xf`, `partial2`) or dropped (`distinct-by`, `count-by`, `juxt-all`, `array-from`, `array-map`, `append-file`, `with-output-string`), `spit` is T1, E4 says six call sites |
| C15 the §2.3 instance table is incomplete | accepted in part | table regenerated, protocols named in §4 defined, `Ord` of `Map`/`Set` not offered. Rejected: splitting `Dissoc` so that `(disj m k)` fails; the leniency is §5 row 79 |
| C16 argument-order and naming rules have unlisted exceptions | accepted | N3, N4, N5, N6, N8 amended, N11 added, `includes?` and `and-then` reordered, `str/lines` removed, `unless` an alias to remove, stand-ins deleted when L1 lands (§2.3) |
| C17 `str`, `print`, `prn`, `pr` have no function twin | accepted | same as ergonomics E15 |
| C18 variant and type names shadow, and `list` is unhygienic | accepted in part | `list` expands to qualified names (works, A10 list). Rejected: dropping the rename: the library defines functions `empty` and `cons`, so the variants cannot keep the names |
| C19 §5 row 1 says `(first nil)` is a type error | accepted | row 1 and the §2.4 row corrected (A10 nth) |
| C20 the language changes are Rust front-end changes that stage 2 must mirror | accepted | §6.6 lists them with tranche and notes; the Rust macro list grew by ten; §9 Q26 and Q29 |
| C21 L13 is not small | accepted | L13 re-sized (a new diagnostic class and a type attribute), recommendation: an error for a statement of an adaptor type |
| C22 L1 and L7 have colliding surface syntax | accepted | L1's clause marker is `:arity`, each clause with its own `-> R` and `:where` (§7 L1, §4.3 `defn`) |
| C23 `parse-double` through `strtod` makes the tools disagree | accepted | the grammar is in §2.9 and is checked in fibber before `strtod` (A10 pd) |
| C24 §6.4's model contradicts its measurement | accepted | same as compilability C08 |
| C25 `swap!`, `conj`, `comp` are `keep` but differ | accepted | verdicts changed to `adapt`, with `def`, which also differs (§4) |
| C26 counts: 192 case files, "Required by" | accepted | 191 files of `cases/ownership`; §2.3 "Required by" corrected |

Tally. Of the 66 findings, 53 are accepted, 10 are accepted in part (each rejects a proposal inside it: 12
proposals in all, named in the rows) and 3 are moved to §9 with a recommendation; none is rejected whole. By
severity, the critics' one blocker (consistency C01) and 29 majors (ergonomics E01 to E10, compilability C01 to
C10, consistency C02 to C10) are all resolved or moved to §9: ergonomics E02 and E07 are moved, E05 is resolved
as far as the library can go and its language change is §9 Q30. The findings that need the owner are in §9 with a
recommendation: Q11 (keyword call, E02), Q22 (derive by default, E04), Q23 (`def` initialisers, E07), Q24 (float literals, E14), Q27 (the
`:use` rule), Q28 (case 01), Q30 (L16, E05 and compilability C01) and the others added in the review (Q25, Q26, Q29, Q31, Q32).


## Appendix A. Evidence

Programs run in the session that wrote this page, with the compiler binaries of that session
(`fibc`, `fibref`, `lair`), each one at a time. They are in the design record (`stdlib/` of the
session's scratch directory, **not in the repository**), so the programs that carry a claim are
quoted here. Counts of heap objects are the `A` lines of `FIB_TRACE=1`; indirect calls, retains and
releases are counted per function in the output of `fibc emit`.

**A1. The chain today (push).** `(vec (range 0 1000))` costs 2095 objects (`conj` allocates 2.09 per
element). Adding

```lisp
(->> v (map (fn (x) (* x x))) (filter (fn (x) (even? x))) (reduce (fn (a x) (+ a x)) 0))
```

makes it 2099, result 166167000: **+4 objects** (two recipe structs, two stored closures), none per
element. The four functions on the per-element path have, in `fibc emit`: `each-while` of `Vec` 1 retain
and 1 indirect call; the `Mapped` visitor 2 retains, 1 release, 2 indirect calls; the `Filtered` visitor
2 retains, 2 releases, 2 indirect calls; the `reduce` visitor 1 retain, 1 release, 1 indirect call:
**6 indirect calls and 6 retains per element**. `lair build -O 2 --emit llvm` leaves five indirect
call sites on the chain (and the object drop hook).

**A2. Closures as types, emulated.** The same chain with the three functions written as functor structs
implementing `(Fn1 f a b)`, `(Fn2 f a b r)` and a cursor chain (`d1.fib`): result 166167000, 2101
objects, and in `fibc emit` every function on the path (`reduce-c`, each `advance`, `current`, `call1`,
`call2`) has **0 indirect calls, 0 retains, 0 releases**; at `-O 2` the one indirect call in the module
is the object drop hook. That chain is a pull chain, and with a counting `Sq` it calls `f` 1500 times for 1000
elements (A10 d1c). Written as a macro, a literal `((fn (x) ..) arg)` is still an indirect call
through a stack closure (3 per iteration, design record `m0`); `(let ((x arg)) ..)` is not.

**A3. The pull chain.** The same chain with `advance!`/`current` cursors and a view per adaptor
(design "fidelity", `c3A.fib`): result 998970, 2104 objects (about 2094 build the input). Per element:
3 indirect calls (the `reduce` step, the `filter` advance, the `map` advance), each preceded by a retain.

**A4. In place, or not.**

| Program | Result | Objects |
|---|---|---|
| `bump` writes its `(Array i64)` parameter through a local cell; 1000 calls in a `loop` (`u1`) | 500 | 1001 |
| the same fed temporaries, `(bump (bump (bump (bump (array 4 0) 0) 1) 2) 3)` (`u2`) | 2 | 5 |
| `(set! c (conj @c i))` 1000 times (`i1`) | 1000 | 2094 |
| `(push! &v i)` 1000 times (`i3`) | 1000 | 2094 |
| `(array-set! &c i i)` on an array in a `let` cell, 1000 times (`i5`) | 999 | 1 |

`fibref explain u1.fib`: `defun bump ... params: a owned (rule 1: stored) escapes=yes ... calls: @3:12
(cell ..) call arg 1 a: retain`; in `main`: `@11:26 (bump ..) call arg 1 acc: retain` and `@11:11 recur
arg 2: moved before the jump: release [acc] (old loop value)`.

**A5. Option of a scalar.** Over 100 iterations, a function returning `(Option i64)` and one returning
`(defenum (Step a) (Done v: a) (Cont v: a))` at `i64`: result 9900, **200 objects**, `nil` included.

**A6. What the compiler says** (each is a one-form program; `fibc run` unless noted):

| Program | Output |
|---|---|
| `(defun f (a: i64) -> i64 a)` and `(defun f (a: i64 b: i64) -> i64 (+ a b))` | `f is already defined` |
| a protocol method `gg` and `(defun gg (m: (Vec a) k: i64 d: a) -> a ..)` | `gg is already defined` |
| `(defun f (a: i64 & xs) -> i64 (+ a 0))`, `(f 1 2 3)` | `1` |
| `(defprotocol (Fold s e) (fold (self f: (fn (r e) r) :borrow init: r) -> r))`, an `impl` for `(Vec a)`, `(fold (range 10) (fn (a x) (+ a x)) 0)` | `fibc`: `unsupported: a type variable reached the lowering: Gen(1)`; `fibref`: `result: 45` |
| `(defmacro u () `())` and `(do (u) 1)` | `fibc`: `macro u failed: concat of nothing`; `fibref`: `result: 1` |
| `(defun concat (a: (Vec i64) b: (Vec i64)) -> (Vec i64) a)`, `(defmacro twice (x) `(do ,x ,x))`, `(twice 4)` | `concat takes 2 argument(s), got 3` |
| `(impl (Fn1 a b) (fn (a) b) ..)` | `an instance head is a type constructor applied to distinct variables (or, at a colour parameter, a colour)` |
| `(match (Pair 1 2) ([a b] (+ a b)))` | `cannot unify (Pair i64 i64) with (Vec a)` |
| `(match [1 2] ((or [a] [a b]) a) (_ 0))` | `or is not a variant or struct` |
| `(let ([a 1 b 2]) (+ a b))` | `malformed let: a binding is (pattern expression) or (name: type expression)` |
| `(let (([a b] [1 2])) (+ a b))` | `a let pattern must be irrefutable` |
| `((fn ((P a b)) a) 1)` | `a fn parameter is sym or sym: type` |
| `(cond (= 1 2) 5 true 6)` | `malformed cond: a clause is (test body+)` |
| `(defprotocol Dflt (dflt () -> Self))` and `(dflt)` | `a method is (name (self qual* x: T qual*) -> type)` |
| `(extern sqrt :private (f64) -> f64)` and `(unsafe (fptosi i64 (sqrt 49.0)))` | `fibc`: `7`; `fibref`: `unsupported: extern sqrt is not available in the reference interpreter` |
| `(count {1 2 1 3})` | `1` |
| `(+ 9223372036854775807 1)` | `trap: integer overflow in + at i64` |
| `(match 1 ({:keys [a]} a) (_ 0))` | `braces are not allowed in patterns` |
| `(. (Pair 1 2) fst)` | `unbound name Pair` |
| `(defprotocol (Collection s e) (conj (self x: e) -> Self))`, `(impl (Collection a) (Vec a) ..)`, then `(conj ["x"] "y")` | `compile failed: duplicate definition of @m.Collection.conj.$Vec.t0_.str` |
| `(+ 1 2 3)`; `(str 1 2)`; `(reduce ..)`; `(sum [1 2])`; `(inc 1)` | `+ takes 2 argument(s), got 3`; `unbound name str`; `unbound name reduce`; `unbound name sum`; `unbound name inc` |
| `(println 5)` | `cannot unify i64 with str` |
| `#(+ % 1)`; `#{1 2}`; `(:a {:a 1})` | `unknown reader syntax #(: only #_ is defined`; `unknown reader syntax #{: only #_ is defined`; `cannot unify keyword with (fn (a) b)` |
| `(= [1 2] [1 2])`; `(show [1 2])`; `(assoc [1 2] 0 9)`; `(first nil)` | `no implementation of Eq for (Vec i64)`; `... Show for (Vec i64)`; `... Associative for (Vec i64)`; `... Seq for (Option a)` |
| `(= 1 1.0)`; `(or (some 1) false)` | `cannot unify f64 with i64`; `cannot unify (Option i64) with bool` |
| `(nth [1 2] 5)` | `trap: nth: index out of range` |
| `(get (assoc (map-empty) 0.0 1) -0.0)` with `(= 0.0 -0.0)`, `(hash 0.0)` and `(hash -0.0)`, a NaN key | the sum of the four tests is `1`: only `(= 0.0 -0.0)` holds |
| `(defun empty (v: (Vec i64)) -> bool ..)`, `(count (list 1 2))` | `cannot unify (fn :send ((Vec i64)) bool) with (List i64)` |
| `(defmacro println (... xs) ..)` expanding to `(fib.prelude/println (str-join [(show x) ..]))`, then `(println 5 "a" true)` and `(apply1 println "v")` where `apply1` takes a `(fn (str) unit)` | prints `5atrue` and `v` (the probe's macro joins the shown arguments without a separator; with a space between them it prints `5 a true`, A10 pm): a macro and a function of one name coexist, and `fib.prelude/println` is reachable without a `:require` (`(:require [fib.prelude ..])` is `module fib.prelude is not at fib/prelude.fib`) |
| a `build` that conses 300000 onto `empty` in a `loop` and returns the head; then 50000 | `thread 'main' has overflowed its stack` under `fibc run`; 49999 |
| `(defstruct (Box a) ..)`, `(defstruct (Entry k v) ..)` redefined by the program | works: a local definition shadows a prelude type (result 7, audit clean) |
| `(map (fn (p) (. p age)) ps)`, `filter`, `group-by`, `sort-by` over `(Vec P)`, unannotated `p` | all type-check and run (4, 3, 3, 2, 2, 2) |

**A7. The prototype** (`stdlib/synth/proto/`, 5 modules, 427 lines, plus `xf.fib`; the revised prototype and its 48 checks are A10 t1r): `Reducible` and the
sources, the adaptors `map filter remove keep take drop take-while drop-while mapcat concat
reductions`, `Cursor` and `zip zip-with`, the consumers of §2.1, the protocols of §2.3 with `Map`, `Vec`
and `Set` instances, `into vec set keys vals update fnil group-by frequencies merge-with`, a stable `sort`,
`Eq Ord Hash` for `Vec`, `Show` and `Debug`.

* `t1.fib`, 33 checks: result 0 under `fibc` and under `fibref` (`audit: clean=true leak-cycles=0
  leaks=0 errors=0`). Changing the expectation of `sort` and of `show-vec` prints `FAIL sort` and `FAIL
  show-vec`: the checks can fail. The checks include: a pipeline, an infinite source with `take`,
  `first` of empty and non-empty, `count` of a size-preserving and of a filtering recipe, `Option` as a
  source, `reduce-while` over an infinite source, `any? find-first find-map mapcat concat zip zip-with
  reductions`, `update` with `fnil`, `get`/`assoc`/`peek`/`pop` on a `Vec`, `contains?`, `frequencies
  group-by sort sort-by`, sorting vectors of vectors, a `Vec` as a map key, `show`, `debug`, `show` of a
  map, `(count (chars "aé"))` is 2 and `(str-len "aé")` is 3, `compare`.
* `t2.fib`: a binary search tree joins with one `impl` (`each-while`, six lines) and then `vec`, `take`,
  `filter`, `count`, `sum`, `reduce1` work on it: 6 checks, result 0 under both tools, audit clean.
* `(count "abc")` is `no implementation of Reducible for str`.
* Counts: `(vec (range2 0 1000))` 2095 objects; with the chain of A1, 2099; `(count (sort v))` of 1000
  elements minus the building of `v`: 4190 = 2094 (the `vec` inside `sort-with`) + 2094 (the result) + 2:
  the merge itself allocates its two buffers and writes in place.
* `num1.fib` of design "zero" (the `Unit` witness: `inc dec zero? pos? abs` at `i64`, `i32` and `f64`): result
  1046.
* The three design prototypes were re-run: design "zero" (`progs.fib`, 20 programs) result 0
  under both tools, with an edited expectation printing `FAIL p3`; design "fidelity" (20 programs):
  every output equals its recorded output; design "realistic" (`verify.sh`, 43 mains): every result
  equals its recorded result.

**A8. Transducers.** `Xf` (§2.1), `xmap xfilter xtake xdrop xmapcat xf-comp transduce into-xf`: 6 checks,
result 0 under both tools, audit clean, including one `Xf` value used with a `Vec` and with a `Set` as
the target; the revision adds a flush to the stepper (A10 xf). As a `Reducible` recipe (`(Xformed c a b)`) it is rejected: `implementation of
Reducible/each-while for (Xformed c a b) makes parameter k escape; the protocol declares it :borrow`;
and `transduce` with a `:borrow` reducing function: `parameter f of transduce is declared :borrow but
escapes`.

**A9. Compile time.** An empty `main`: 0.10 s, 47 MB (`fibc run`). The prototype with `t1.fib`: 0.40 to
0.50 s, 73 MB; `fibref run`: 0.10 s, 10 MB. Macros (`mac1`, `mac5`, `mac20`: one, five and twenty
distinct one-line macros, each used once): 0.20, 0.50, 1.70 s under `fibc`; 0.20 s for twenty under
`fibref`. `JitOptions::default()` has `opt_level: u8` defaulting to 0 and `fibc`'s `run` and `build`
use the defaults. Modules load from the main file's directory (`crates/fibref/src/modules.rs`, read).

**A10. Evidence of the review.** Programs run while this page was revised, one process at a time, `timeout 300` (and `ulimit -v 4000000` for
`poly`), with the same binaries as A1 to A9. They are in the reviser's scratch directory, `stdlib/revise/`,
**not in the repository**: `sl/` is the revised prototype (6 modules plus `with.fib`), `t1r.fib` its 48 checks,
`t10.fib` the transducer checks, `ev/` the one-form probes, `mut/` the mutation run; the critics' programs
(`stdlib/{ergonomics,compilability,consistency}/`) were copied and re-run, not trusted. Unless an entry says
otherwise a program gives the same output under `fibc` and `fibref`, and `fibref` ends `audit:  clean=true
leak-cycles=0 leaks=0 errors=0`. Counts are the `A` lines of `fibc run --trace`.

* **t1r, t10.** The revised prototype (`Reducible` with `nth last to-vec`, `Pair` as the one tuple type, `update`
  in three forms, `compare` total, `hash-combine`, adaptor `Show`, `Chars` decoder, `Range` size, `seq-of`,
  `sort-by-with`, `seq=`, the `Stepper` transducers): `t1r.fib`, 48 checks, result 0 under both tools; `t10.fib`,
  the checks of A8 and three on `xpartition-all`, result 0. `t10` with one expectation edited prints `FAIL
  partition-all-flush`. The chain of A1 is still 2099 objects against 2095 for its input (`a_chain`, `a_base`),
  also through `fibc itrace` (`A=2099`).
* **hash.** `(defstruct P2 (a: str b: str))`, `(derive Hash P2)`, `(hash (P2 "hello" "world"))`: `trap: integer
  overflow in * at i64` under both tools. `(hash (list "a" "b" "c"))`: the same, at `<prelude>:3:36`. A
  `hash-combine h x` = `(bit-xor (rotl h 5) (bit-xor x (rotl x 31)))` with `rotl` from `shl`, `shr`, `bit-or`,
  over a `(Vec str)`: `["hello" "world"]` is `-400993756763253696`, fourteen one-letter strings
  `1971437196018332428`, `["ab" "c"]` against `["a" "bc"]` and `["a" "b"]` against `["b" "a"]` are `false` and
  `false`.
* **nan.** `compare` over `(!= a a)`: `[(compare nan nan) (compare nan 1.0) (compare 1.0 nan) (compare 1.0
  2.0) (compare 0.0 -0.0)]` is `[0 1 -1 -1 0]`; `(sort [3.0 nan 1.0 2.0 nan 0.5])` and the same elements in
  another order are both `[0.5 1.0 2.0 3.0 NaN NaN]`; `max` and `min` over `(!= a a)`: `[true true true true
  false]` for `(nan? (max nan 1.0))`, `(max 1.0 nan)`, `(min nan 1.0)`, `(min 1.0 nan)`, `(max 1.0 2.0)`.
* **nth.** `(nth (filter odd? v) 3)`, `(nth (map inc v) 5)`, `(nth v 7)`, `(nth (iterate inc 0) 9)` over
  `v = 0..99`: `[7 6 7 9]`; `(last (filter odd? v))`, `(last v)`, `(last (take 0 v))`: `[(some 99) (some 99) nil]`;
  `(= (vec v) v)` true. `(count (vec (vec (vec v))))` of 1000 elements: 2095 objects, the same as `(count v)`.
* **alloc.** Over a 1000-element `Vec` `v` and a 1000-entry `Map` `m` (base 11033 objects): `(reduce f 0 (zip v
  v))` 12036 (+1003), `(zip-with g v v)` 11037 (+4), `(keep (fn (x) (some x)) v)` 12035 (+1002), `(map id v)`
  11035 (+2), `(reduce f 0 m)` 13033 (+2000: the prelude's `Entry` and the `Pair` it is wrapped in), a loop of
  1000 `(unwrap (first v))` 12033 (+1000) against 1000 `(nth v 0)` 11033 (+0).
* **count.** A `map` with a counting function over `[1 2]`: `[(count r) calls]` is `[2 0]`, `[(sum r) calls]`
  `[3 2]`; `(empty? (filter p (range2 0 100000)))` with a predicate that never passes calls it 100000 times.
* **rangesize.** `Range` with a closed-form `size`: `[10 4 4 0 0 3]` for `(range2 0 10)`, `(range3 0 10 3)`,
  `(range3 10 0 -3)`, `(range2 5 5)`, `(range2 5 0)`, `(range3 0 9 3)`, and `count` equals the length of `to-vec`
  for eight ranges.
* **cur.** `(zip-with (fn (b a) (- b a)) (rest xs) xs)` is `[3 5 7 9]` and `(zip (map inc xs) xs)` is `[[2 1] [5
  4] [10 9] [17 16] [26 25]]`; `(zip xs (map inc xs))` is `no implementation of Cursable for (Mapped (Vec
  i64) i64 i64)`.
* **cur-impl.** `(impl (Cursable (Wrap k)) (Src c) :where ((Cursable c k)) (cursor (self) (Wrap (cursor (. self
  src)))))` is `impl-det2.fib:5:55: type variable k is not a parameter of the impl head`; the same with the
  variable in the head (`(impl (Cursor e) (Wrap k e) :where ((Cursor k e)) ..)`) is accepted (result 0). The
  cursors of `Mapped`, `Dropped` and `Taken` over `(Cursable c k)` (`ev/adaptor-cursors/`) give five such
  errors.
* **push.** The push chain with a visitor protocol parameter (`push1.fib`): `type variable k is not a parameter
  of the impl head` (three times) and `the context constraint (Fn2 t0 t1 t2 t1) is not smaller than the
  instance head`; with a bound on the method's variable (`push2.fib`): `a method is (name (self qual* x: T qual*)
  -> type)`; without the bound (`push3.fib`): `no implementation of Fn1 for k; add (Fn1 k) to the :where of the
  impl`.
* **d1c.** The pull chain with functor closures (`d1c.fib`, a counting `Sq` through `map` then `filter even?`
  over 1000 elements): result `1500`, the number of calls of `f`.
* **c1emul.** `(if flag (map2 (Inc true) v) (map2 (Dec true) v))` with `(defstruct (Mapped c f) ..)` and a
  functor per function: `cannot unify (Mapped (Vec i64) Dec) with (Mapped (Vec i64) Inc)`; the same with
  `(Mapped c e b)` and plain closures type-checks today (`c1b.fib` of the compilability critic, result 12).
* **join.** `(if flag (filter odd? v) v)` counted: `cannot unify (Vec i64) with (Filtered (Vec i64) i64)`;
  `(defun pipeline (flag v) -> (dyn (Reducible i64)) (if flag (seq-of (filter odd? v)) (seq-of v)))` gives
  `[1 3 5]`, `[1 2 3 4 5]` and `2` for `(count (take 2 (pipeline true v)))`.
* **col.** `(defstruct (Mapped c e b k :colour) (src: c f: (fn k (e) b)))` and `(join (spawn (fn () (sum r))))`
  with `r` a `Mapped` built outside: 12 under both tools. Without the colour parameter, the compilability critic's
  `sp1.fib`: `value of type (fn :local (i64) i64) cannot be shared between threads: closure capture r, field f of
  Mapped`.
* **bw.** `(defun g (v: (Vec i64) :borrow) -> i64 (reduce f 0 (map h v)))`: `parameter v of g is declared :borrow
  but escapes`; without the qualifier it runs (12).
* **sortby.** `(sort-by key [3 1 2 5 4])` with a counting key: 5 calls; `(sort-by-with (fn (x) (rem x 3)) desc
  [1 2 3 4 5 6])` is `[2 5 1 4 3 6]` (stable); `(sort-by (fn (p) (. p snd)) [(Pair "a" 2) (Pair "b" 1) (Pair "c" 2)])` is
  `[[b 1] [a 2] [c 2]]`; `(sort-by-with f cmp c)` with `f` declared `:borrow` is `parameter f of sort-by-with is declared
  :borrow but escapes`.
* **sort.** `(sort v)` of 1000 elements: 4193 objects against 2097 for building `v`, so 2096: the result
  (2094) and 2 buffers; the `vec` inside is the identity (4190 before).
* **update.** The page's first `update`: `(update (assoc (map-empty) "a" 1) "a" inc)` is `cannot unify (fn :send
  (i64) i64) with (fn ((Option a)) a)`. The revision: `(update m "a" inc)` `(some 2)`, `(update-or (map-empty) "w"
  inc 0)` `(some 1)`, `(update-opt (map-empty) "w" (fnil inc 0))` `(some 1)`, `(update (map-empty) "missing" inc)`
  `trap: update: no key` (under `fibref` `trapped:`, audit clean at the abort).
* **pair.** `(into (map-empty) (zip [1 2 3] [4 5 6]))`, `(into (map-empty) (map (juxt f g) [1 2]))`, and an
  `into` of `(Pair x (* x x))` values: all `Map`s with the expected entries (three checks of `t1r`). A second
  `(impl (Collection (Pair k v)) (Map k v) ..)` beside `(Collection (Entry k v))`: `overlapping instances: Collection
  for (Map k v) is already implemented at sl/coll.fib:49:1`.
* **showmap.** Twelve integer keys inserted ascending and descending: both print `{0 0, 1 1, 10 100, 11 121, 2 4,
  3 9, ...}` (equal text); `(show (set [3 1 2]))` is `#{1 2 3}`; `(show (into (map-empty) (zip ["b" "a"] [2 1])))` is
  `{a 1, b 2}`.
* **showseq.** `(show (map inc [1 2 3]))` `[2 3 4]`, `(show (take 3 (iterate inc 5)))` `[5 6 7]`, `(show (filter odd?
  (range2 0 10)))` `[1 3 5 7 9]`, `(seq= (map inc [1 2]) [2 3])` true; the first version: `no implementation of Show
  for (Mapped (Vec i64) i64 i64)` (ergonomics critic, g3).
* **chars.** A 1000-character string of `a é € 😀 z`: `(str-len s)` 0 objects, `(count (chars s))` 2 (the decoder),
  `(count (str-chars s))` 2095; `(= (to-vec (chars "aé€😀z")) (str-chars "aé€😀z"))` true, `(count (chars ..))` 5.
* **str.** `(array-len (str-bytes s))` in a loop of 100: result 1100 (the allocation count of 100 arrays is the
  compilability critic's `sb1`, not re-run).
* **pd.** A `parse-double` that checks the grammar of §2.9 in fibber and then calls `strtod`: `"1e5"` is `100000.0`,
  `"inf"`, `"0x10"`, `" 12"` and `"12abc"` are `nil`, `"-.5"` is `-0.5`, `"3."` is `3.0`, `"."` and `"1e"` are `nil`,
  `"+2.5E-3"` is `0.0025`, `""` is `nil`, the same under both tools. Without the check, the BRIEF's `strtod` wrapper
  on `1e5 inf 0x10 " 12" nan 12abc`: `fibc` prints `100000.0 inf 16.0 12.0 NaN 12.0`, `fibref` prints `100000.0 inf`
  and then `unsupported: hex float` (consistency critic, `sd.fib`, re-run).
* **str-nil.** `(show nil)`: `ambiguous constraint Show a in main; add an annotation`.
* **xf.** `(into-xf (vec-empty) (xpartition-all 2) [1 2 3 4 5])` is `[[1 2] [3 4] [5]]`; `(xf-comp (xmap inc)
  (xpartition-all 2))` gives `[[2 3] [4 5] [6]]`; `(xf-comp (xpartition-all 2) (xmap count))` gives `[2 2 1]`; with
  `[[1 2] [3 4]]` expected, `FAIL partition-all-flush`.
* **comp.** `(into-xf (vec-empty) (comp (xmap inc) (xfilter odd?)) [1 2 3])`: `cannot unify (Xf i64 i64) with (fn
  (a) b)`.
* **run.** `(run! (fn (x) (swap! a (fn (n) (+ n x)))) [1 2 3])` returns `@a` 6.
* **swap.** `(defmacro swap! (a f ... args) `(fib.prelude/swap! ,a (fn (x__) (,f x__ ,@args))))`, then `(swap! a +
  5)` and `(swap! a * 2)` on `(atom 10)`: 30.
* **twin.** `(defun str (x: a) :where ((Show a)) -> str (show x))` and the same for `println`: `(run! println [1 2 3])`
  prints 1, 2, 3, `(join "," (map str [10 20]))` is `10,20`.
* **pm.** A variadic `println` macro that joins with `(str-concat " " ..)`: `(println 5 "a" true)` prints `5 a true`,
  `(println)` an empty line, and the function in value position (`(apply1 println "v")`) prints `v`.
* **with.** `(defmacro with (r ... ps) ..)` over a cell and `set-field!`, in a module that does not define `concat`:
  `[(. (nth (. db2 users) 0) score) (. (nth (. db users) 0) score) (. db2 n) (. db n)]` is `[4 3 3 2]`;
  `(. (with (User "x" 1 2) (name "y") (age 9)) name)` is `y`. In a module that `:use`s a module defining `concat`
  the macro is `concat takes 2 argument(s), got 3` (B2).
* **try.** `(defmacro try! (r) `(match ,r ((Ok v) v) ((Err e) (return (Err e)))))` is `unbound name return`;
  `(defmacro try-let (bs body) (match bs ((List [(List [name e]) & more]) `(match ,e ((Ok ,name) (try-let ,(List
  more) ,body)) ((Err er__) (Err er__)))) (_ body)))` with `(try-let ((a (half n)) (b (half a))) (Ok (+ a b)))`:
  `(f 8)` gives `(Ok 6)` and `(f 6)` the `Err`, the sum of the two results is 106.
* **iflet.** `(match o ((some [a b]) (+ a b)) (_ -1))` over `(some [5 6])`, `(some [1])`, `nil`: the sum is 9;
  `(if-let ([a b] o) (+ a b) -1)`: `non-exhaustive match: missing (some [])`.
* **dd.** `(derive Debug P)`: `cannot derive Debug: only Eq, Ord, Hash and Show`; `(= (P 1 "x") (P 1 "x"))` with no
  derive: `no implementation of Eq for P`; `(derive Show P)` shows `(P 1 x)`, `(show (some 3))` `(some 3)`, a `nil` `nil`.
* **list.** With a user `(defun empty ..)` and `(defun cons ..)`, `(match (fib.prelude/cons 1 (fib.prelude/cons 2
  fib.prelude/empty)) ((fib.prelude/cons h _) (+ h (cons 10 20))) (_ -1))` is 31; `(count (list 1 2))` after a user
  `(defun empty ..)` is `cannot unify (fn :send ((Vec i64)) bool) with (List i64)`.
* **poly.** `(defun len (xs: c) :where ((Reducible c e)) -> i64 (if (empty? xs) 0 (+ 1 (len (drop 1 xs)))))`, `(len
  [1 2 3])`: `fibc` `memory allocation of 577136 bytes failed` (under `ulimit -v 4000000`), `fibref` `result: 3`.
* **def.** `(def ok: (Set i64) (set [1 3]))`: `def ok: initialiser is not a constant expression`; `(def counter:
  (Atom i64) (atom 0))`: the same; `(def v: (Vec i64) [1 2 3])` and `(def m: (Map i64 i64) {1 2})`: 4 for their counts.
* **hyg.** `lib1` defines `first` (returns 5); `lib3 (:use lib1)` defines `(defmacro mu (v) `(first ,v))`. A user module
  that defines its own `first` (1000) and calls `(l3/mu [1])`: 1000. One that does not: `no implementation of Seq for
  (Vec i64)`. `(l1/first ,v)` in a template (`lib2`, which requires `lib1`): `unbound name l1/first`; `lib1/first`:
  `unbound name lib1/first`; `((var l1/first) ,v)` and `((var lib1/first) ,v)`: `var: no definition named l1/first`
  and `... lib1/first`.
* **use.** `y.m1` and `y.m2` both define `peek`: `(:use y.m1 y.m2)` then `(peek 3)` is 3 (m1's identity), `(:use y.m2
  y.m1)` is 4.
* **mh.** `mu` defines `bump`; a macro in a module that `(:require [mu :as u])` and expands to `(Int ((var u/bump) 3)
  :i64)`: `macro twice failed: f4.fib:2:46: var: no definition named u/bump`; with `(:use mu)` and a bare `(bump 3)`:
  `fibc` `... unbound name bump`, `fibref` `macro twice calls bump, which is not available at expansion time; move
  bump to a required module`.
* **b3.** `(defprotocol (Collection s e) (conj (self x: e) -> Self))`, `(impl (Collection a) (Vec a) (conj (self x)
  (vec-empty)))`, `(count (conj ["x"] "y"))`: `fibc` `compile failed: b3.fib:1716:1: error: duplicate definition of
  @m.Collection.conj.$Vec.t0_.str`; `fibref` result 0.
* **own.** `own1.fib` (`(put (self :owned x: e) -> Self)` on `(Buf (a: (Array i64)))`, 1000 times in a `loop`): 2002
  objects; `fibref explain` lists `(put ..) call arg 1 b: retain` and `release [b] (exit)`. `ip1.fib` (four chained
  `bump`s of an owned `(E1 n arr)` on unique temporaries): 10 objects; `ip2.fib` (one `bump`): 4. `sf1.fib`
  (1000 `(set-field! &c n ..)` on a struct held in a cell, and 1000 fresh arrays): 1002 objects, the 2 beyond the
  arrays being the struct and its first array.
* **timing.** An empty `main`: 0.10 s, 47 MB. A main that requires a 2400-line module and uses none of it: 0.20 s,
  63 MB. A main that uses 80 distinct functions of it (`mainu.fib`): 17,256 lIR lines, 0.60 s, 75 MB under `fibc run`;
  `lair run` of that lIR (`lair run -O 0`, `-O 2`): 0.40 s and 1.00 s.
* **forloop.** `(for-each (range 0 1000) (fn (i) (set! acc (+ @acc i))))`: `main` is a plain loop in `fibc emit` (the
  one `indirect-call` in it is the drop hook at exit); `(run! (fn (i) ..) (range2 0 1000))`: `each-while` of `Range` and
  `run!` each hold an `indirect-call` and a `fib.retain` per iteration (two and two in all).
* **mut.** Twelve mutants of the prototype against `t1r.fib` under `fibref`: ten caught by a `FAIL` line (taken one late,
  dropped one extra, `Option` source never visiting, `Iterate` skipping the seed, `Mapped` size off by one, `sort` unstable,
  `update` ignoring `f`, `Vec` `nth` ignoring its index, `sort-by-with` reversed, `compare` of two NaNs); two by not
  terminating (inverted `Filtered`, `Cycle` of an empty source: `TIMEOUT`).
