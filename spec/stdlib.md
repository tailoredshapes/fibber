# The standard library (M7)

Status: **Proposed**. The owner asked on 2026-10-01 for a library that "steals Clojure's, or as
close to it", is as ergonomic as Clojure and has the run-time performance of Rust, said that no
benchmarking or optimisation comes before the library is viable (ROADMAP M7), and then gave the
tie-breaker that governs this page: **unless it breaks memory safety, Clojure has the ergonomics
we are replicating; if it does, Rust has them** (ROADMAP M7 rule 0, §1 P0). This page is the design: the
rule and the principles that follow from it (§1), the abstractions (§2), the naming rules (§3), every
name of Clojure's core, set, string, walk, data and math namespaces with its verdict and its
fibber signature (§4), the few places that still differ from Clojure and the failing program or the
decided fact behind each (§5), where the code lives (§6), the language and compiler changes it needs
(§7), the order of work and how each step is judged (§8), what the rule does not settle, each with a
recommendation (§9), and the review records (§10). Nothing here is **Decided** until the owner signs
it, except where a line says **Decided** and names the date; method.md applies throughout: a function is
done when a test that can fail says so.

How this page was made. One agent inventoried Clojure (673 names, 71 sharp edges); three wrote
independent designs (zero-cost first, Clojure fidelity first, type-system realism first); a fifth judged
them (the scoring is in the design record, `synth-notes.md`) and wrote the first version. Three critics
(ergonomics, compilability, consistency) then checked it and raised 66 findings; a further agent revised
the page, and §10.1 to §10.3 record every finding with its verdict. When the owner gave the tie-breaker,
two auditors classified the page against it independently, one presuming the page's reasons held and one
presuming every deviation wrong (§10.4 gives their counts), each running programs to test every reason the
page gave; a last agent decided where the two differed, re-ran the deciding programs, and rewrote this
page, and §10.4 records every changed item with its reason. A claim about what the
compiler does cites an item of Appendix A, **[R]** where it matters: a program that was run with `fibc`
and `fibref` in the session that wrote this page or in a review (A10, A11), its output quoted. A claim that is
a design record's and was not re-run, or a prediction, says so (**[H]** for a hypothesis; a checker or
runtime change that cannot be prototyped without editing the compiler is a **[sketch]** and names the failing
program that shows the gap). Nothing was measured
for run-time speed; the only such numbers are counts of heap objects, indirect calls and count operations, which
are properties of the design that can be read off `FIB_TRACE=1` and `fibc emit` (ROADMAP M7 rule 5 allows no
timing yet); the one timing on the page is the cost of compiling, which §6.4 needs.

## 1. Principles

### 1.1 The rule, and the test a deviation must pass

**P0. The rule** (owner, 2026-10-01; **Decided**). Unless it breaks memory safety, Clojure has the
ergonomics fibber replicates; where Clojure's way would break memory safety, Rust's way is the
ergonomics. A function, form or behaviour that differs from Clojure's needs one of two reasons, and no
other:

1. **A memory-safety reason**, which this page states as *a failing program*: Clojure's behaviour written
   in fibber that either is rejected by the ownership or type checker for a safety reason, or compiles and
   then misbehaves in one of five ways: it reads freed memory or follows a null pointer, frees twice,
   writes through an alias that another holder (or task) observes, races (two tasks write one location),
   or builds an invalid `str` that the decoder's bounds then no longer protect. If the program cannot be
   written, the deviation is not allowed. The programs that exist are in §5 (`aset`'s value semantics,
   a global `Cell`, `locking`, the UTF-16 unit, an uninitialised array slot).
2. **A static-typing fact the language already decided** (types §1, §2.11; ROADMAP M7 rule 0 says "or the
   static-typing the language already decided"): `nil` is `Option`'s empty variant and nothing else; a
   function type has a fixed arity; there is no run-time type information; a `Vec` has one element type and a
   `Map` one key type and one value type; a result type cannot depend on a value; a protocol instance has one
   head today. Each is listed in §5 with the program that shows it, and each that is a simplification rather
   than a necessity has a typed twist in this page that keeps Clojure's text (§2.4, §2.11, §4.2).

"Cleaner", "faster", "one way", "less surprising", "no GC", "it is a wart" and "the reader may be
confused" are **not** reasons. In particular a **leak is not a memory-safety failure**: ownership.md §6 says a
leaked cycle is not a violation, and the audit reports it without error (`audit: clean=false leak-cycles=2
leaks=0 errors=0` for a cell that holds a closure that captures itself, [R] A11 e8b). A cost reason ("a lazy
cell per element is slow", "character offsets are O(n)") is not a reason either; where cost and the rule
collide the page follows the rule, says what the cost is, and puts the collision in §9 for the owner, who
set both aims.

Where a language or compiler change is what the rule requires (truthiness of `Option`, callable collections,
arity-reading `partial`), §7 lists it with its size and the program that shows the gap, and the owner signs it
there; §9 holds only what the rule does not settle.

### 1.2 The principles

| # | Principle | What it means for a function |
|---|---|---|
| P0 | The rule | §1.1: Clojure unless memory safety; Rust where Clojure's way would break it; a deviation carries its failing program or its decided typing fact. |
| P1 | Clojure's names, shapes, argument order and behaviour first | A name keeps Clojure's meaning and Clojure's spelling; a name fibber already has under another spelling (`array-get`, `shl`, `read-file`, `defun`) gets Clojure's as an alias (§3 N12). Sequence functions take the sequence last, collection functions take the collection first (§3). Behaviour includes the sharp edges of Clojure that are not safety matters (`compare` of vectors, `(take-nth 0 c)`, `(range 0 1 0.1)`): they are replicated, and §9 Q33 asks whether the owner exempts them. |
| P2 | Zero-cost by construction | Everything generic is monomorphised and every protocol call is static (compiler.md §7). A sequence function over a pure function returns a small struct that remembers its source and its function (a *recipe*), never a lazy cell and never a copy; an adaptor allocates nothing per element except what it hands on (until C5 a struct or an `(Option scalar)` is a heap object: `zip`'s `Pair`, `keep`'s `Option`, a `Map` walk's `Pair`, [R] A10 alloc), and only the materialisers (`vec`, `into`, `set`, `sort`, `group-by`) allocate per element by their signature. Sequences whose elements come from effects are memoised (`LSeq`, §2.1); where memoising and zero cost collide, §9 Q34. |
| P3 | Persistent in the API, in place when unique | `conj`, `assoc`, `update` keep Clojure's value semantics. Where the ownership checker proves the collection unique they update it in place, so a loop of `assoc`s is Clojure's transient without a transient API. `transient`, `persistent!`, `conj!` exist as identity wrappers so that Clojure's text resolves (§4.4). Whether the in-place claim holds is a test (§2.5), not a belief. |
| P4 | Unboxed elements, a real hash | A `(Vec i64)` stores `i64`s. The hash that types §2.12 fixes (an integer hashes to itself, FNV-1a for text) is replaced before the HAMT's speed is judged (§9 Q15, **Decided**). Iteration order of a `Map` and a `Set` is the hash's, as in Clojure, except that a small `Map` keeps insertion order as Clojure's array map does (§2.7). |
| P5 | Clojure's values where the type system has a tag for them | `nil` is `Option`'s empty variant, and a condition accepts `bool` or `(Option T)` (the falsy values are `false` and `nil`, as Clojure's, [sketch] §7 L20). A function that is undefined on some input of its type returns `Option` where Clojure returns `nil` (`first`, `get`, `peek`, `find`, `parse-long`, `index-of`); one for which Clojure throws traps, with a message and a position (`nth`, `pop`, `assoc` past the end), until exceptions are decided (§9 Q35). `or`, `and`, `when`, `some->`, `update` and `reduce` keep Clojure's text over those types (§2.4). |
| P6 | Every spelling of Clojure's, and where Clojure has several ways, all of them | A protocol exists where the monomorphiser can dispatch on it (§2.3); a run-time predicate (`vector?`, `seq?`, `instance?`) has no run-time type to test and is not offered (§5 T3); everything else Clojure has is offered, as a function, a macro or an alias. The library's own additions (`Pair`, `unwrap`, `map-opt`, `and-then`, `try-let`, `seq-of`, `zip`, `find-first`) are marked `(new)` and never take the place of a Clojure name. |
| P7 | State: Clojure's global state is allowed where it is safe | A top-level `atom`, `defonce`, a dynamic var with `binding`, a global random generator, `defmulti` and metadata are offered (§2.11): none breaks memory safety. A global `Cell` or `Weak` is not: it would be reachable from every task and written by two (§5 M1). |
| P8 | Ships on today's compiler | Each tranche of §8 is written in the form that compiles today. The compiler changes of §7 make the same source faster or the same names terser; they do not change what a function means, except the syntax and checker items that the rule requires and the owner signs separately: E3, E4, E8, E14, L14, L19, L20 to L26. |
| P9 | Every function has an executable test | Cases run both ways (method rule 6), generated programs against a model (rule 5), differential tests against a naive reference written in fibber (§8). The compiler (M6) is the first customer: what stage 2 needs lands first. |

Two non-goals. The library does not try to be a Clojure interpreter: no `eval` (a result type that cannot be
known statically, and the compiler inside every binary), no reflection (no run-time type information), no
Java instance interop (`.method`, `new`); the static names Clojure code writes (`Math/sqrt`, `Long/MAX_VALUE`,
`Integer/parseInt`, `Character/isDigit`) are offered as modules (§4.16). Arbitrary-precision numbers and
ratios are library types (`BigInt`, `Ratio`, `BigDecimal`; tranche 5, §2.8), not a non-goal. The library does
not hide cost: `count` says in §4 whether it is O(1) or a walk, and re-traversing a recipe re-runs it (§2.1).

## 2. The core abstractions

Code blocks are marked. `ran:` means the block (or the module it is cut from) was compiled and run
with `fibc` and with `fibref` and the audit was clean (Appendix A, A7, A11); `proposed:` means the
compiler does not accept it today, and the error it gives is quoted where it matters.

### 2.1 Iteration: one push protocol, a pull protocol for lockstep walks, a closed seq type for recursion

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
   `(Pair k v)`), `Set`, `Range`, `Iterate`, `Repeat`, `Cycle`, `Chars`, **`str` (a source of `char`s, as
   Clojure's string is a seqable of characters, §2.9)**, and every adaptor below.
2. **Adaptors** (`map filter remove keep take drop take-while drop-while mapcat concat
   map-indexed reductions partition interleave zip ...`) are functions that build, in O(1), a struct
   holding the source and the parameters, and implement `Reducible` over it. Over a pure function they are
   *recipes*: traversing one twice runs it twice, calling `f` again. Nothing runs until a consumer walks it,
   and the adaptor itself allocates nothing per element; what it hands to the next stage can be an
   object until C5 (`zip` yields a `Pair` and `keep` an `Option`: +1003 and +1002 objects over 1000
   elements against +4 for `zip-with` and +2 for `map`; `(first v)` in a loop +1000 against `(nth v 0)`
   +0; a `Map` walk +2000 in the prototype, the prelude's `Entry` and the `Pair` it is re-wrapped in,
   [R] A10 alloc). **Clojure's lazy seqs are cached, so the observable difference is an effectful function:**
   a counting `map` traversed twice makes 6 calls where Clojure's makes 3 ([R] A11 t20, result 624), and the rule
   does not accept cost as a reason. So a sequence whose elements come from effects is memoised: `repeatedly`,
   `line-seq`, `iteration`, `lazy-seq`, `lazy-cat`, `file-seq` return an `LSeq`, a head and a `Lazy` cell forced
   once, which runs the source's effect once per element across two traversals (5 calls, sum 10 twice, audit
   clean, A11 t21); `(cache c)` memoises any finite recipe (A11 e8c: `12 12 3`); `doall` is `vec` and `dorun` is
   `run!`. A self-referential `LSeq` (the classic `fibs`, `ones`) is a reference cycle through a cell:
   `audit: clean=false leak-cycles=4 leaks=0 errors=0` (A11 t22), a leak the ownership model allows (§1.1) and
   not a safety failure; an immortal `def` hides it. An `LSeq` holds a `Cell`, so it cannot cross a task
   (`cell cannot be shared between threads: closure capture s, field box of Cached has type (Cell (Option (Vec
   i64)))`, A11 e8d); the same memo over an `Atom` crosses (A11 e8e, result 6), and a `Mutex` (§7 C9) gives
   Clojure's run-once-under-contention exactly. Whether the default for **every** adaptor is memoised, with
   the checker inserting `cache` where the use count of a recipe variable exceeds one so that a single-use
   pipeline stays zero-cost, is §9 Q34.
3. **Consumers** (`reduce first last some every? count empty? into vec set sort ...`) are
   loops over `each-while`. `first`, `some`, `every?`, `take`, `take-while`, `empty?`
   stop the source through the `bool`, so `(take 5 (iterate inc 0))`, `(first (filter p (iterate inc 0)))` and
   `(some p (iterate inc 0))` terminate. Clojure's `reduced` is `(reduced x)`, a `(Done x)`: plain `reduce`
   over a **literal** `fn` reads it (a macro rewrites the tails of the body, so `f`'s type stays `(fn (a e) a)`;
   `(reduce (fn (acc x) (if (> acc 10) (reduced acc) (+ acc x))) 0 [5 6 7 8 9])` is 11, over `(iterate inc 1)`
   with a limit 105, an infinite source ends, A11 e20 and t81), and `reduce-while` is the form for a function
   that is not a literal. It allocates a heap enum per step until unboxed enums land (§7 C5), so the plain
   consumers use the `bool` channel.
4. **`count` is `size`**: O(1) where the source holds its elements (`Vec Map Set Array Range Option Slice`),
   a walk for every adaptor, so **`(count (map f c))` calls `f`, as Clojure's does** (the prototype's `Mapped`
   no longer overrides `size`: `[(count r) calls]` is `[2 2]` and `[(sum r) calls]` `[3 4]` over `[1 2]`, A11
   count; the first version of this page made it O(1) and not call `f`, which was a cost reason). `Range` has a
   closed-form size ([R] A10 rangesize). `empty?` stops at the first element the recipe produces: O(1) on a
   source that holds its elements, and `(empty? (filter p c))` calls `p` until an element passes (100,000 calls
   for a predicate that never does, A10 count). **`nth`, `last` and `to-vec` are methods with a walking
   default**, so `(nth (filter p c) 3)` works in O(n) as Clojure's does and `(nth v 3)` is O(1). A generic
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

   `Vec` and `Range` are `Cursable` in the prototype; `Array`, `List`, `Chars` and `str` follow the same
   pattern. **Every adaptor is `Cursable` by buffering**: the cursor of a `Mapped`, `Filtered` or `Taken` is
   the cursor of a `Vec` built once from `(to-vec self)` (one buffer per `zip`, none per element):

```lisp
;; ran: fib.seq.cursor (A11 e16)
(impl (Cursable (VecCur b)) (Mapped c e b) :where ((Reducible c e)) (cursor (self) (cursor (to-vec self))))
(impl (Cursable (VecCur e)) (Filtered c e) :where ((Reducible c e)) (cursor (self) (cursor (to-vec self))))
(impl (Cursable (VecCur e)) (Taken c e) :where ((Reducible c e)) (cursor (self) (cursor (to-vec self))))
```

   so `(zip xs (map inc xs))` is `[[1 2] [2 3] [3 4] [4 5] [5 6]]`, `(zip xs (filter odd? xs))` is `[[1 1] [2 3] [3
   5]]` and `(zip-with (fn (b a) (- b a)) (rest xs) xs)` runs, in either order, under both tools (A11 e16). The first
   version of this page said `no implementation of Cursable for (Mapped (Vec i64) i64 i64)` and made
   argument order a rule; that error is real without the three impls ([R] A10 cur) and is not a reason. The cursor of
   `(Mapped c e b)` that needs no buffer would be a struct over the cursor of `c`, and the impl that says so is
   rejected by the rule that an impl's context names only variables of its head, `type variable k is not a
   parameter of the impl head` (A10 cur-impl, A11 t23; types §3.3, the Paterson condition). §7 L16
   (**Decided**, owner, 2026-10-01) lifts the rule for a variable that a constraint determines; then `Mapped`,
   `Dropped`, `Taken` and `Zipped` have bufferless cursors (one `advance!` each) and `Filtered` and `Mapcat`
   keep the buffer (a pull over them must buffer).
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
   :borrow`, A8). They wait for closure types (§7 C1), where the step is a static call. The function-name
   form is `xf`: `(xf (map f) (filter p))` composes left to right, and `comp` of two transducers is the one place
   where Clojure's `comp` cannot be function composition today (§5 T9): after C1 `comp` is arity-reading and
   dispatches on `Fn` and `Xf` (§7 L22), which gives Clojure's text `(comp (map f) (filter p))`; an impl head on a
   function type is rejected as it stands (A6).
8. **Recursion on `rest` uses a closed seq type.** Clojure's `(defn len [xs] (if (seq xs) (inc (len (rest
   xs))) 0))` is the basic idiom, and for a generic `c` with `rest` returning `(Dropped c e)` it is polymorphic
   recursion, which `fibc` cannot finish ([R] A10 poly; §7 B4 makes both tools reject it). The cure is
   Clojure's own structure: `seq`, `rest` and `next` belong to `Seqable c s`, where `s` is one **closed** seq type per
   collection and `rest` of an `s` is an `s`: a `Vec` has the `Slice` view, a `List` itself, a `Range` itself, a
   `str` a view that steps by one character, and any other `Reducible` the memoised `LSeq`. The idiom then
   type-checks, is generic over every collection that has a seq type and runs under both tools:

```lisp
;; ran: fib.seqable (A11 p/seq1)
(defprotocol (Seqable c s)
  (seq (self) -> (Option s))        ; nil when empty, else the closed seq type
  (rest (self) -> s))               ; empty when empty, never nil
(impl (Seqable (Slice a)) (Vec a) ..)  (impl (Seqable (Slice a)) (Slice a) ..)  (impl (Seqable (List a)) (List a) ..)
(defun next (c: c) :where ((Seqable c s) (Seqable s s)) -> (Option s) (seq (rest c)))
(defun len (xs: c) :where ((Seqable c s) (Seqable s s)) -> i64
  (if-let (s (seq xs)) (+ 1 (len (rest s))) 0))     ; (len [5 6 7 8]) 4, (len (list 1 2 3)) 3, (len []) 0
```

   `len` is instantiated at `(Vec i64)` and then at `(Slice i64)`, which calls itself: a fixed point, not a
   chain. `first` stays a `Reducible` method (`(Option e)`, stops after one element). A `rest` over a recipe
   (`(rest (map f c))`) allocates one `LSeq` cell per step as Clojure's does, so the idiom is the
   convenient form and `reduce` or `loop` the zero-cost one; the `Seqable` instances of the adaptors are one
   line each (**[H]**: the `LSeq` of A11 t21 exists, the instances over adaptors were not written).

**Why push, with a pull cursor beside it.** Three designs were measured. Pull with a mutable cursor
and `current`/`advance!` (design "fidelity") has 3 indirect calls and 3 retain pairs per element
today; push (designs "zero" and "realistic") has 6 and 6 ([R] A1, A3), and neither allocates per
element. With static closures (§2.2) the **pull** chain reaches 0 and 0 (A2), but it runs the user's
function twice for every element that survives a `filter` (a counting `map` then `filter even?` over
1000 elements calls `f` 1500 times: `filter` tests `current` and the reducer reads it again, [R] A10
d1c), which is the weakness charged to pull below. The **push** chain reaches 0 and 0 only if a protocol
method may carry its own visitor type with a bound, and no program of the review could declare that
(§2.2, A10 push): it is a prediction that needs §7 L16 (**Decided**), not a measurement (§9.1). Push
wins on what is not a speed question: a source is one `loop` (a tree, a HAMT, `mapcat`, `cycle` are one-liners, where a cursor
needs an explicit stack); early exit is a return value; each user function runs exactly once per
element (a pull cursor needs a slot cell per adaptor to avoid re-running `f` behind a `filter`: design "fidelity" has one, and design "zero" counted 15 calls of `f` for 10 elements without it, both from the design record and not re-run here); and no
`(Option e)` is made per element, which is a heap object when `e` is a scalar ([R] A5). Pull wins
where two sources advance together, so that is the one place it is offered. A pull `seq` of
`(Option cursor)` would be free for object-typed cursors (a null pointer); the recursion idiom it serves (`(rest s)` as a
loop variable) is served by the closed seq type of rule 8, a `Slice`, a `List` or an `LSeq`, and not by a cursor over a
recipe. The unbounded case remains a hazard: a function that recurses on `(drop 1 c)` for any `Reducible c` calls
itself at `(Dropped c e)`, then at `(Dropped (Dropped c e) e)`, without end (polymorphic recursion): `fibc` exhausts
memory (`memory allocation of 577136 bytes failed` under `ulimit -v 4000000`) and `fibref` returns 3 ([R] A10 poly), a
stage-1 divergence that §7 B4 closes by making both tools reject it with a message that names the recursion.

**Materialising** is explicit and the only per-element cost: `vec`, `into`, `set`, `zipmap`,
`sort`, `reverse`, `group-by`, `frequencies`. The bulk ones build in one function
over a growable buffer and assemble the trie once, about n/32 allocations instead of 2n (§2.5); `vec` of a
`Vec` is the identity.
Each adaptor has an eager spelling only where Clojure has one (`mapv`, `filterv`, aliases of
`(vec (map ..))`; `doall` is `vec`).

**Infinite and lazy.** `iterate repeat cycle` are ordinary sources that only an
early-exiting consumer ends. Memoised laziness, for a recursive definition such as the Fibonacci
numbers, is the library type `LSeq` (`fib.lazy`) with `lazy-seq` and `lazy-cat` as macros over it: a head and a
`(Lazy a)` forced once. It moves from tranche 5 to tranche 2, because the classic recursive producers, the
`Seqable` instances of the adaptors and the effect-sourced sequences of rule 2 need it. Fibber has no GC, so a cell per
element is a cost the pure adaptors avoid; the rule puts the cost where Clojure puts it, on the sequences
that run effects.

**A recipe is a type.** Each adaptor is a distinct struct, which has three consequences. (1) *Joins.*
`(if flag (filter p v) v)` is `cannot unify (Vec i64) with (Filtered (Vec i64) i64)`, and two different
adaptors in the arms of an `if` or `match` fail the same way; `cond->` over a sequence is the same case
([R] A10 join). Clojure's text is fine there, and no safety reason forbids it, so the checker rule of §7 L24
unifies two arms that are both `Reducible` of one element type at `(dyn (Reducible e))`, the erasure rule that C1
already needs for closures; `(seq-of c)` is the explicit form, a `(dyn (Reducible e))` that erases the type at
the cost of one heap object and an indirect call per visit; it type-checks and runs (`pipeline` in A10 join),
and the cost is in the name. (2) *Threads.* A recipe holds a closure, and a closure built outside a task cannot be sent:
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

**Arity-reading combinators.** A function type is `(fn (A B) R)` with the arity in the type, which is the
static-typing fact behind every Clojure combinator that returns "a function of any arity" (§5 T2). It does not
make them impossible: the arity can be read from the argument's type, so `partial`, `comp`, `complement`,
`juxt`, `every-pred`, `some-fn`, `memoize`, `fnil` and `apply` are checker built-ins that elaborate against the
`(fn (A..) R)` of their function arguments (§7 L22 **[sketch]**): `(partial f a)` for `f: (fn (A B C) R)` has
type `(fn (B C) R)`, `(comp f g)` has the arity of `g`, `(apply f xs)` with `f: (fn (A B) R)` is `(f (nth v 0) (nth
v 1))` after a check that `(count v)` is 2 (a trap where Clojure throws `ArityException`), and a table of the
variadic folds (`+ * str max min conj merge concat comp`) names the fold that `(apply + xs)` stands for. What
the library has today is the fixed-arity form, which fails on exactly the programs that Clojure's text
needs ([R] A11, programs `arity` to `arity4`, with `sl.core`'s `partial` and `comp`): `(partial add3 1)` for a
three-parameter `add3` is `cannot unify (fn :send (i64 i64 i64) i64) with (fn (a b) c)`; `(comp inc add2)` is
`cannot unify (fn :send (i64 i64) i64) with (fn (a) i64)`; `(constantly 7)` where a `(fn (i64 i64) i64)` is
expected is `cannot unify (fn (a) i64) with (fn (i64 i64) i64)`. `constantly` is the one built-in whose arity
comes from the expected type, not from an argument. A **multi-arity `fn` literal** and a variadic function
value (`%&`) have no single function type and stay out (§5 T2); the multi-arity `defn` is overloading by
argument count (§7 L1), which is a property of a name, not of a value.

### 2.3 The protocol hierarchy

| Protocol | Methods | Required by | Instances |
|---|---|---|---|
| `Reducible s e` | `each-while`, `size`, `nth` (traps out of range), `last`, `to-vec` | every sequence function and consumer | `Vec List Array Option Map Set Range Iterate Repeat Cycle Chars str Slice Queue SortedMap SortedSet Reversed Repeatedly FRange Matches Lines LSeq Cached`, every adaptor, user types |
| `Seqable c s` | `seq -> (Option s)`, `rest -> s` | `seq rest next` and the recursion idiom (§2.1 rule 8) | `Vec` and `Slice` (seq type `Slice`), `List`, `Range`, `str` (a view that steps by a character), `LSeq`; every adaptor (seq type `LSeq`) |
| `Cursable s c`, `Cursor c e` | `cursor`; `advance!`, `current` | `zip interleave map/3 zipmap` (second operand) | `Vec Range Array Slice str` and their cursors; every adaptor by buffering, and without a buffer for `Mapped Dropped Taken Zipped` after L16 |
| `Lookup s k v` | `get -> (Option v)` | `get update find select-keys`, and a `Lookup` value in call position (§2.11) | `Map`; `Vec` (key `i64`); `str` (key `i64`, value `char`); `SortedMap`; `Slice`; `(Option s)` after L16; user types |
| `Assoc s k v` | `assoc` | `assoc update assoc-in` | `Map`; `Vec` (index at most the count: `i = count` appends, as Clojure's); `SortedMap`; a struct by a literal keyword (L21) |
| `Dissoc s k` | `dissoc` | `dissoc disj` | `Map Set SortedMap SortedSet` |
| `Keyed s k` | `contains?` | `contains?` | `Map Set SortedMap SortedSet`, **and `Vec` and `Slice` (the index, as Clojure's: `(contains? [10 20] 1)` is true and `(contains? [10 20] 20)` false)**; `includes?` is the element test |
| `Collection s e` | `conj` | `into conj` (`vec`, `set` and `merge` are `Reducible` plus `Hash`/`Eq`) | `Vec` (end), `List` (front), `Set`, `Map` (of `Pair`; of a two-element `Vec` after L23), `Queue` (back), `SortedMap` (of `Pair`), `SortedSet` |
| `Emptyable` | `empty` | `empty select-keys assoc-in` | `Vec Map Set List Queue SortedMap SortedSet` |
| `Stack s e` | `peek -> (Option e)`, `pop` | `peek pop` | `Vec` (end), `List` (front), `Queue` (front) |
| `Reversible s e` | `each-while-rev` | `rseq` | `Vec Array Range Slice SortedMap SortedSet` |
| `KeyReducible m k v` | `each-kv-while` | `reduce-kv` | `Map`, `Vec` (index, element) |
| `Truthy r`, `Payload r p` | `truthy? -> bool`; `payload -> p` | the predicate parameter `(fn (e) r)` of `filter remove some every? not-any? keep take-while drop-while`; `if-let`; `some` | `bool` (payload `bool`), `(Option a)` (payload `a`); no other type (a condition that cannot be false) |
| `Cmp r` | `resolve` | the comparator parameter `(fn (e e) r)` of `sort sort-by sorted-map-by sorted-set-by comparator` | `i64` (negative, zero, positive), `bool` (Clojure's predicate form: true is -1, else the swapped call decides) |
| `Pattern p` | `find-in`, `split-by`, `replace-in` | `str/split str/replace str/replace-first str/index-of str/last-index-of re-find` | `str`, `char`, `Regex` |
| `Eq Ord Hash Show` | built in; `derive` | `= < hash println` | scalars, `str`, `Option`, `List`; **added**: `Vec Map Set Pair Triple` (§2.7); `Ord` of a `Map` or `Set` is not offered (Clojure's `compare` throws on a map) |
| `ToStr` | `to-str -> str` | `str` | `str` itself, scalars, `char`, `(Option a)` (empty text for `nil`), collections (the `Debug` text, as Clojure's `(str ["a"])`), derived structs |
| `Debug` | `debug -> str` | `pr prn pr-str` | as `Show`; strings quoted (§2.7) |
| `Num`, `Bits` | built in (types §2.12) | `+ - * / bit-and ...` | integer and float types; the library's `BigInt`, `Ratio`, `BigDecimal` (§2.8) |
| `Unit t` | `zero`, `one` (take a value of the type as witness) | `inc dec abs zero? sum` | `i8..i64`, `f32`, `f64` |
| `ToByte ToShort ToInt ToLong ToFloat ToDouble ToChar` | `byte short int long float double char` | the conversions | numeric types |
| `Fn f args r` (after C1) | `call` | every function argument; the callable collections | every `(fn ..)`; `Map Set Vec` and keyword instances after C1 and L23 (§2.11) |

The prelude's `Countable`, `Indexable`, `Traversable`, `Iter` and `Seq` protocols and its `for-each`, `map`,
`iter`, `collect`, `filter-iter` are replaced by `Reducible`, `Seqable` and the functions of §4 (of the 191 case files of
`cases/ownership`, one uses `Countable` and two use `filter-iter`; §6.5 and §8.3 give every prelude name and the
migration). `Associative` splits into `Lookup`, `Assoc` and `Dissoc` so that a vector can be looked up, and a set
can be dissociated, without pretending to be the other; the prelude's `Entry` struct gives way to `Pair`, the one
tuple type (§5 T4); `dissoc` and `disj` are one `Dissoc`, so `(disj m k)` on a
`Map` type-checks (more lenient than Clojure, which rejects it: no program Clojure accepts is rejected). Protocols that §4 names only in a signature are defined with their module:
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

`Keyed` on a `Vec` is one impl, and `assoc` at the count appends, both as Clojure's, both under both tools ([R] A11
e6):

```lisp
;; ran: fib.coll (A11 e6)
(impl (Keyed i64) (Vec a) (contains? (self i) (and (>= i 0) (< i (count self)))))
(defun assoc-c (v: (Vec a) i: i64 x: a) -> (Vec a) (if (= i (count v)) (conj v x) (assoc v i x)))
;; (contains? [10 20] 1) true, (contains? [10 20] 20) false, (assoc-c [10 20] 2 30) [10 20 30], (assoc-c [10 20] 0 30) [30 20],
;; (assoc-c [10 20] 5 30) trap: assoc: index out of range
```

The two protocols that were not in the first version carry two of Clojure's habits that the page had refused.
A **comparator is any function whose result is a `Cmp`**: `i64` (the int form) or `bool` (Clojure's predicate
form), dispatched on the result type, static after monomorphisation, and `<`, `>` and `compare` are function
values already ([R] A11 t17 and e4):

```lisp
;; ran: fib.cmp (A11 t17)
(defprotocol Cmp (resolve (self k: (fn () Self) :borrow) -> i64))
(impl Cmp i64  (resolve (self k) self))
(impl Cmp bool (resolve (self k) (if self -1 (if (k) 1 0))))     ; Clojure's two-call rule for a predicate
(defun sort-cmp (cmp: (fn (e e) r) :borrow c: c) :where ((Reducible c e) (Cmp r)) -> (Vec e)
  (sort-with (fn (a b) (resolve (cmp a b) (fn () (cmp b a)))) c))
;; (sort-cmp < xs) [0 1 2 3 4 5], (sort-cmp > xs) [5 4 3 2 1 0], (sort-cmp compare xs), (sort-cmp (fn (a b) (compare b a)) xs),
;; a stable (sort-by-cmp key > ps); a named predicate over [3 1 2 3 1] keeps both equal elements: [1 1 2 3 3]
```

and a **predicate result** is a `Truthy`: the same pattern, with `bool` and `(Option a)` as the two instances (a
blanket `(impl Truthy a ..)` is `an instance head must not be a type variable`, A11 t06, so no other type is
truthy), which is what makes `(filter (fn (k) (get m k)) ks)` and `(some #(get m %) ks)` run
([R] A11 t05, result 4; `some` over it, A11 `some`):

```lisp
;; ran: fib.truthy (A11 t04, some)
(defprotocol Truthy (truthy? (self) -> bool))
(impl Truthy bool (truthy? (self) self))
(impl Truthy (Option a) (truthy? (self) (match self (nil false) ((some _) true))))
(defprotocol (Payload s p) (payload (self) -> p))
(impl (Payload bool) bool (payload (self) self))
(impl (Payload a) (Option a) (payload (self) (unwrap self)))
(defun some-p (p: (fn (e) r) c: c) :where ((Reducible c e) (Truthy r) (Payload r x)) -> (Option x)
  (find-map (fn (e) (let ((v (p e))) (if (truthy? v) (some (payload v)) nil))) c))
;; (some-p even? [1 3 4 6]) (some true); (some-p (fn (x) (get m x)) [1 2 3]) (some 20); (some-p even? [1 3]) nil
```

Protocol methods carry Clojure's names. A name whose Clojure arities differ by a default value
(`get`, `nth`, `reduce`, `sort`, `update`, `sort-by`, `range`, `map`) has its shortest arity as the method or function and
the others as clauses of the same name once arity overloading exists (§7 L1); until then they are `get-or`, `nth-or`,
`reduce1`, `sort-with`, `sort-by-with`, `range-by` and `zip-with` (§4), and `update` is the macro of §2.4 (its extra
arguments are Clojure's, so it takes no stand-in). **The stand-ins are deleted in the commit that lands L1**, and the cases that
used them are ported in the same commit; the typed forms `update-or`, `update-opt` and `reduce1` (which keep the
`Option` where Clojure's `(f)` or `nil` have no type) stay as the typed twins. **[R]** A protocol method may not
share a name with a `defun` clause today (`f is already defined`), so L1 must say that a method and clauses of
other arities may coexist.

**Defaults.** A default method written once is specialised per type. `Reducible.size`, `nth`, `last` and
`to-vec` are the examples, overridden where the source knows better: by `Vec` ([R] A10 nth, rangesize) and, in
the same way, by `Array`, `Range`, `Option` and `Slice`; an adaptor does **not** override `size`, so `count`
walks it and calls its function (§2.1 rule 4); `Ord`'s `<=`, `>`, `>=` and `Eq`'s `!=` are the prelude's.

**Instances are global facts** (syntax §5): the library's `impl Eq (Vec a)` is visible to every
program, which is what lets a `Vec` be a map key. A protocol is dispatched on its first parameter's head today,
so two instances of one head that differ only in a determined parameter overlap: `overlapping instances: OrHit
for (Option a) is already implemented` and `overlapping instances: Collection for (Map k v) is already
implemented` ([R] A11 t70, t71). That one restriction is what keeps `(into {} [[1 2]])`, `or` of an `Option` with
a default, `=` across sequential types, `merge` with `nil` and `str/replace` by replacement type out of plain library
code; §7 L23 (**multi-parameter dispatch**: instances keyed by the heads of every dispatch parameter) lifts it, and §7
L20, L24 give the cheapest checker rules where L23 is not taken (§9 Q39).

### 2.4 `nil`, `Option`, truthiness and `empty`

`nil` is `Option`'s empty variant (types §1.5, decided: it is a static-typing fact, §5 T1). `Option` is itself
a `Reducible` of size 0 or 1, so absence composes with sequence functions. Clojure's punning is kept where a
type can carry it, and replaced by a typed twin where it cannot:

| Clojure idiom | Here | Rule |
|---|---|---|
| `(get m k)`, `(first c)`, `(peek s)`, `(find m k)`, `(last c)` | `(Option v)` | a miss is `nil`, never a trap; absent and `nil`-valued cannot be confused (§5 T1); `(unwrap o)` and `(nth c 0)` are the sure cases |
| `(get m k d)`, `(nth c i d)` | clause of `get`, `nth` (§7 L1); `get-or`, `nth-or` until it lands | the clause has its own result type `v` |
| `(if (seq xs) ..)`, `(when (get m k) ..)`, `(while (peek s) ..)`, `(filter :active ps)`, `(remove nil? xs)` | **a condition of type `bool` or `(Option T)` is accepted**: truthy when `true` or `(some _)`; `(Option bool)` truthy when `(some true)` (Clojure's `false` is falsy) | memory-safe, typed and free at run time: the elaboration is a direct call to an identity (`fibc emit` of A11 e1z shows `(call @m.Truthy.truthy?.bool t1)` to a body `(ret p0)`); [sketch] checker rule §7 L20; the library runs today as the `Truthy` protocol (§2.3, A11 t04: `if`, `or` with a default, `or` of two options, `and` and `when` over `bool` and `Option`, 127 under both tools). Today `(if (get m 1) 1 2)` is `cannot unify (Option i64) with bool` (A11 t01), `(or (get m 2) 7)` the same (t02), `(if 5 1 2)` `cannot unify i64 with bool` (t03). Any other type as a condition stays a compile error: it cannot be false (`(if* 5 1 2)` is `no implementation of Truthy for i64`, A11 n15) |
| `(or (get m k) d)`, `(or (get m k) (get m j))`, `(and (get m k) (> x 0))` | Clojure's text: `or` and `and` are checker forms typed by their **last** operand: `(or (Option T) T)` is `T`, `(or (Option T) (Option T))` and `(or bool bool)` keep their type; `and` has the type of its last operand; a first operand of another type is the compile error above | one `or` for two result types needs the operand types, not just the first (an instance per head overlaps: `OrHit for (Option a)`, A11 t70, e2): a checker rule, or L23. The macros `or-d` and `or-e` of A11 t04 run today as two names; `(unwrap-or o d)` is the spelling that needs nothing |
| `(some pred c)` | `(Option payload)`: `(some even? c)` is `(some true)` or `nil`, `(some #(get m %) ks)` the first non-nil value | over the predicate's `Truthy` result ([R] A11 `some`: `(some true)`, `(some 20)`, `nil`); `some` is also `Option`'s constructor, so the two arities are one name by L1 extended to a constructor (A11 n13: a `defun some` of two parameters makes `(some 5)` `some takes 2 argument(s), got 1`) or a Rust macro that picks by argument count; `find-first` and `find-map` are extras |
| `(if-let [x e] a b)`, `(when-let ..)`, `(if-some ..)`, `(when-some ..)`, `(when-first [x c] ..)` | `if-let` over a `Truthy` (`x` binds the payload; the pattern may be any pattern, refutable ones included: the else is taken on a mismatch); `if-some` is the presence test; `when-first` is `(when-let (x (first c)) ..)` | `(if-let ([a b] o) ..)` expands to `(match e ((some [a b]) a) (_ b))` so a shorter vector falls to the else (the macro of today expands to `(nil b)` and rejects it: `non-exhaustive match: missing (some [])`, [R] A10 iflet); `[a b :as all]` is `([a b] :as all)` |
| `(when c body)`, `(if c a)`, `(cond t1 e1)`, `(when-let ..)` without an else | **unit when the body is `unit`, else `(Option T)` with `some` around the body**; `(keep #(when (even? %) (* % 2)) xs)` is Clojure's text | `(when c 5)` is `cannot unify unit with i64` today (A11 t82, k7); a macro that wraps in `some` runs (12, A11 t83); the general rule needs the arm's type, so it is a checker rule (§7 L20 [sketch]). `cond` with no matching clause and no `:else` is `nil`/`unit` the same way; `case` and `condp` with no match trap, as Clojure throws |
| `(map f nil)`, `(count nil)`, `(first nil)` | empty recipe, 0, `nil` | `Option` is `Reducible` ([R] A11 n9: `(first nil)` is `nil`, `(count nil)` 0) |
| `(conj nil x)`, `(assoc nil k v)`, `(merge nil m)` | a **literal** `nil` as the first argument is a form the macro sees: `(conj nil x)` is `(conj (list) x)`, `(assoc nil k v)` is `{k v}`, `(merge nil m)` is `m`; an `Option`-typed value is `(conj (or o []) x)` | the result type of `conj` on a `nil` of unknown collection has no single answer (`List` for nil, `Vec` for a vector, the static-typing fact §5 T1): `no implementation of Collection for (Option a)` (A11 n7) names it |
| `(get nil k)`, `(get (get m :a) :b)`, `(-> m :a :b)` | `Lookup` for `(Option s)`: a lookup through `nil` is `nil` (**after §7 L16, Decided**) | `(impl (Lookup k v) (Option s) :where ((Lookup s k v)) ..)` is `type variable k is not a parameter of the impl head` today (A11 t23) and `an instance head is a type constructor applied to distinct variables` for a nested head (e5); `get-in` and `some->` are the spellings that run today |
| `(update m k f)`, `(update m k f x ..)` | `f` receives the value, and extra arguments as in Clojure; a missing key traps `update: no key` where Clojure's `(update m :n inc)` throws an NPE; a literal `(fnil g d)` in the `f` position is routed to `update-or` | `(update m "a" inc)`, `(update m "w" (fnil + 100) 7)` and `(update m "a" + 10)` are 6, 107 and 15 in one program ([R] A11 t24: a macro that sees the literal `fnil`); `update-or` (a default) and `update-opt` (hands `f` the `(Option v)`) are the typed forms for an `f` that is not a literal; the 4-arity of `update` is Clojure's extra argument, so the default form keeps its own name, `update-or` |
| `(some-> o f g)`, `(some->> o f g)` | each step takes the unwrapped value; a step that returns an `(Option b)` is kept and a plain `b` is wrapped, so Clojure's text works | a library cannot tell the two apart (no blanket impl, A11 t06): a checker elaboration (§7 L22 [sketch]); until then `(some-> o (and-then f) (map-opt g))` |
| `(empty c)` | `Emptyable`: the empty collection of `c`'s type | a method with a `self`, so it type-checks today; `(empty 5)` has no type (Clojure returns `nil`) and is not offered for a non-collection |

An `Option` of an object type is a null pointer and allocates nothing; an `Option` of a scalar, and
every enum with a payload, is a heap object today ([R] A5: 200 allocations for 100 calls). `get` on
a `(Map i64 i64)` therefore allocates once per lookup until §7 C5 lands; it is a cost of the
representation, not of the API, so the API does not bend to it. The printed form of an `Option` is Clojure's
printed form of its payload or of `nil` (§2.7).

### 2.5 Mutation and uniqueness

The API is persistent. Two things make it fast, and neither needs a transient API (`transient`, `persistent!`, `conj!` and the rest exist as identity wrappers over the same updates, so Clojure's text resolves: [R] A11 t60, `(persistent! (conj! (conj! (transient []) 1) 2))` is `[1 2]`).

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

What exists [R] A6: `match` takes literals, `_`, names, `(Variant p ..)`, `nil`, `(some p)`, vector patterns
with `& rest` and `:as`, struct patterns, guards; `let` takes irrefutable patterns (a struct pattern, `[& r]`);
`fn`, `defun` and `loop` take symbols only; `(let [a 1] ..)` is `malformed let`.

| Clojure | Here today | Proposed (§7) |
|---|---|---|
| `(let [[a b] v] ..)` | `match` with `[a b]`; a refutable `let` pattern is rejected | **a vector pattern in a binding position takes a prefix, as Clojure's**: it binds the first two, ignores the rest and traps `let: pattern does not match` when there are fewer (Clojure binds `nil`, which has no type here, §5 T1) (L8) |
| `(fn [[k v]] ..)`, `(defn f [{:keys [a]}] ..)` | `a fn parameter is sym or sym: type` | irrefutable patterns in `fn`, `defn`, `defun`, `loop`, `for`, `doseq` parameters, as sugar for a `let` (L7) |
| `{:keys [a b]}`, `{:keys [a] :or {a 1}}`, `{:strs [a]}`, `{:syms [a]}` | `(. s a)` reads a field and `(with s (a v))` replaces one (§4.2); no punning today | the pattern on a struct binds the fields; on a `(Map keyword v)` (`:strs` on a `(Map str v)`, `:syms` on a map keyed by `Form` symbols) it binds `(get m :a)`: an `(Option v)`, or `(unwrap-or (get m :a) 1)` for a key that has an `:or` default, which is Clojure's rule (L9) |
| `[k v]` on a map entry | `(Pair k v)`, the one tuple type | tuple-like structs (`Pair`, `Triple`) accept vector patterns (L3b) |
| `(case x 1 :a (2 3) :b)` | `match` with literals | or-patterns `(or p q)` (L6); no match **traps** `case: no matching clause`, as Clojure throws, and is a compile error only when the scrutinee is an enum that is not covered |
| `(let [a 1 b 2] ..)`, `(loop [i 0] ..)` | `(let ((a 1) (b 2)) ..)` | bracket forms accepted beside the parenthesised ones (E3) |
| `(cond t1 e1 t2 e2)` | `(cond (t1 e1) ..)` | Clojure's flat pairs (E4) |

A pattern states its shape in `match`: `[a b]` matches exactly two elements and `[a b & r]` binds a new `Vec`
(empty, never `nil`), because `match` dispatches on shape; in a binding position the prefix rule above is
Clojure's. A refutable pattern in a binding position is an explicit trap, as `nth` is.

### 2.7 Equality, order, hash, print

* `=` is structural on **one** static type, with one family exception. Clojure's `=` ignores the concrete type
  within a family: `(= [1 2] (list 1 2))` and `(= [2 3] (map inc [1 2]))` are true. The families are the sequential
  one (`Vec List Slice Range Array`, recipes, `LSeq`), the map one (`Map`, `SortedMap`) and the set one (`Set`,
  `SortedSet`); where the operand types of `=` differ and belong to one family, the checker elaborates it to
  `seq=` (or its map and set twins), **[sketch]** §7 L24; today `(= [1] (list 1))` is `cannot unify` and the
  library form is `(seq= a b)` ([R] A10 showseq: `(seq= (map inc [1 2]) [2 3])` is true). Across families
  Clojure answers `false`; here a `Vec` and a `Set` do not unify, which is the type error of the decided
  fact that `=` has one type (§5 T7). Floats follow IEEE (`(= nan nan)` false, `(= 0.0 -0.0)` true). **Numbers:**
  with the owner's L19 (**Decided**) a literal adopts the other operand's numeric type, so `(= 1 1.0)` is
  `(= 1.0 1.0)`: **true**, which is Clojure's `==` and not its `=` (false). That is a consequence of L19, recorded
  here and in §5 C1; `==` is an alias of `=`. For two variables of different numeric types `(= n 1.0)` is
  `cannot unify f64 with i64` today (A11 t62, n3) and §9 Q36 asks whether the operators promote.
* `Eq Hash Show Debug ToStr` exist for `Vec Map Set Pair Triple Option List`, `Ord` for `Vec Pair Triple Option List`
  (not for `Map` and `Set`: Clojure's `compare` throws on a map), and `derive` for user types, which today derives
  `Eq Ord Hash Show` only (`(derive Debug P)` is `cannot derive Debug: only Eq, Ord, Hash and Show`, [R] A10
  dd; §7 L17). Today `(= [1 2] [1 2])` is `no implementation of Eq for (Vec i64)` ([R] A6); the prototype's
  instances make it work, and a `Vec` a map key (A7). **`Ord` on a `Vec` is Clojure's: the shorter vector is
  less, then element by element** (`(sort [[1 2 3] [9 9] [1 2]])` is `[[1 2] [9 9] [1 2 3]]`, A11 t1r); `Ord (List a)`
  is lexicographic (Clojure's lists are not comparable, so Rust's order).
* `compare` returns `-1`, `0` or `1` as an `i64` and is **built on `<`, as Clojure's is**: `(if (< a b) -1 (if (< b a) 1 0))`.
  A comparator, wherever one is taken (`sort`, `sort-by`, `sorted-map-by`, `sorted-set-by`), is any function whose
  result is a `Cmp` (§2.3): an `i64` as Clojure's int form, or a `bool` as Clojure's predicate form (`(sort < xs)`,
  `(sort-by :age > ps)`; A11 t17, e4), so a predicate is a comparator, and `(sorted-set-by (fn [a b] (< (count a)
  (count b))) "ab" "cd")` has one element, as in Clojure. `(comparator less?)` converts. **A NaN compares equal to
  everything**: `[(compare nan nan) (compare nan 1.0) (compare 1.0 nan) (compare 1.0 2.0) (compare 0.0 -0.0)]` is
  `[0 0 0 -1 0]` ([R] A11 nan), so `(sort [3.0 nan 1.0 2.0 nan 0.5])` is `[1.0 2.0 3.0 NaN NaN 0.5]` and the same
  elements in another order sort to `[NaN 0.5 1.0 NaN 2.0 3.0]`: the order of a sort over an inconsistent
  comparator is unspecified and never unsafe (a merge sort over arrays reads only indices it has checked). The
  first version of this page made `compare` a total order (NaN greatest and equal to itself) so that `sort` is
  deterministic: that is a cost reason, not a memory-safety one, and Clojure does not do it. `max` and `min` return
  NaN when either argument is NaN, as Clojure's do (A10 nan). §9 Q33 asks the owner whether the 8 replicated
  sharp edges (this one among them) are exempted as bugs.
* `Hash f64` hashes `-0.0` as `0.0` and one NaN, so `(get (assoc m 0.0 1) -0.0)` finds the key
  ([R] it is `nil` today, A6). A NaN key is never found, as in Clojure. This is the contract of `Hash` (`=` implies
  equal hashes), kept because a lookup that misses `-0.0` after storing `0.0` is the failure the HAMT must not
  have; whether Clojure's `(hash 0.0)` and `(hash -0.0)` differ was not verified (no Clojure on the machine), and
  if they do the row is a replicated bug that Q33 offers to exempt. **The combiners of today trap.**
  `derive Hash` and the prelude's `Hash (List a)` fold with `h*31 + hash x`, and a struct of two strings or a
  list of three is `trap: integer overflow in * at i64` under both tools ([R] A10 hash), so a `Map` keyed by
  a two-string struct or a `(Vec str)`, the bread and butter of a symbol table, fails in tranche 1 as soon as
  the keys are long enough. `hash-combine` is therefore a **rotate-and-xor mixer built from `shl`, `shr`,
  `bit-or` and `bit-xor`, which never trap and exist today**: it hashes a 30-element `Vec` and a `Vec` of
  strings without a trap and tells `["ab" "c"]` from `["a" "bc"]` and `["a" "b"]` from `["b" "a"]` (A10
  hash); `derive Hash` and every `Hash` instance of the library use it, and `hash-unordered-coll` folds the
  elements' hashes with `bit-xor`, which is commutative and does not trap, so `Hash (Map k v)` and `Hash (Set k)`
  are order independent. Both ship in tranche 0 and 1 (§8.2); the multiplicative finaliser that needs a wrapping
  multiply (§7 L10) replaces the rotate-xor in tranche 3, in one commit with the new integer hash (§9 Q15, **Decided**).
* **Three printers, with Clojure's meanings.** `str` is `ToStr`, `print` and `println` are `Show` (display),
  `pr`, `prn`, `pr-str` and `dbg` are `Debug` (readable). The three differ on a string inside a collection and on
  `nil`, as Clojure's do:

  | value | `(str v)` | `(println v)` | `(pr v)` |
  |---|---|---|---|
  | `"a"` | `a` | `a` | `"a"` |
  | `["a" 1]` | `["a" 1]` | `[a 1]` | `["a" 1]` |
  | `\a` | `a` | `a` | `\a` |
  | `:k` | `:k` | `:k` | `:k` |
  | `nil` (an empty `Option`) | the empty text | `nil` | `nil` |
  | `(some "a")` | `a` | `a` | `"a"` |
  | a recipe, `List`, `Range`, `LSeq` over `1 2` | `(1 2)` | `(1 2)` | `(1 2)` |
  | `Pair`, `Triple`, `Vec` | `[1 2]` | `[1 2]` | `[1 2]` |
  | `Set` | `#{1 2}` | `#{1 2}` | `#{1 2}` |
  | `Map` | `{1 2, 3 4}` | `{1 2, 3 4}` | `{1 2, 3 4}` |
  | a struct made by `derive Show` / `derive Debug` | `(Name f1 f2)` | `(Name f1 f2)` | `(Name f1 f2)` |

  `(str ["a"])` is `["a"]` and `(println ["a"])` is `[a]` in Clojure, which is the `ToStr` of a collection being its `Debug`
  text and the `Show` text being the display form; the first version of this page made `str` and `println`
  agree, which is a cost-free rule that Clojure does not follow. `(str nil)` with a literal `nil` is the empty
  text (a macro sees it; the bare `nil` has no type, `ambiguous constraint Show a`, A11 t64); `(str o)` of an
  `Option` is the empty text or its payload's `str` ([R] A11 e11: `x=1`, `y=`, `v=["a" "b"]`, `[a b]`). A present value prints as itself, as in
  Clojure, which has no wrapper: the prelude's `Show (Option a)` prints `(some 3)` today ([R] A11 t87: `(some 3)`,
  `(some a)`), a prelude change of one line each for `Show`, `Debug` and `ToStr`; an `(Option (Option a))` is as
  ambiguous as Clojure's nested nil. A seq (a recipe, `List`, `Range`) prints in parentheses and a vector in
  brackets, which the first version collapsed to brackets: `(show (map inc [1 2 3]))` is `(2 3 4)`, `(show (take 3
  (iterate inc 5)))` `(5 6 7)`, `(show (filter odd? (range2 0 10)))` `(1 3 5 7 9)` (A11 showseq); a recipe's `Eq` is `(seq= a b)`, not `=`.
* **Map and set iteration order is unspecified**, as in Clojure: the HAMT's order, a function of the hash and the
  insertion history, the same in the interpreter and the compiled program (the hash is part of the spec). **A
  `Map` of at most 8 entries keeps insertion order, as Clojure's array map does**: it is a `Vec` of entries
  until the ninth key and a HAMT after (library code over `Pair`, [R] A11 e13: keys `b a c` print `{b 2, a 1, c 3}`,
  `assoc` of an existing key keeps its position `{b 20, a 1, c 3}`, nine integer keys then print in hash order
  `{0 0, 1 1, 2 4, ...}`), so a small literal prints as written and small maps get faster; the compilers know `Map` only through `Form`'s
  `Map` variant, so the change is the library's. `Show` and `Debug` print the iteration order, not a sorted
  order. Tests compare with `=` or sort; the change of the integer hash (§9 Q15) changes the order of maps of more
  than 8 entries once, in both implementations together. A numerically ordered print is a `SortedMap`
  (`fib.sorted`).

### 2.8 Numbers

Arithmetic is the builtin binary `Num` method on one type; the variadic spellings are macros that
fold (`(+ a b c)` is `(+ (+ a b) c)`, `(< a b c)` is a short-circuit chain). **`(+)` is `0` and `(*)` is `1`, as Clojure's**:
the macro expands to the literal, which adopts the numeric type its context requires (L19), and is an `i64` when
nothing constrains it, so `(reduce + [])` style code has a zero. Integers divide toward zero, `rem` has the dividend's
sign, `mod` the divisor's, overflow traps at every width (Clojure's `+` throws on a `long` overflow too;
types §2.12; **Decided**, §9 Q15), float division is IEEE. `(/ 7 2)` on two integers is `3`, Rust's, because
Clojure's result is `7/2` or `3` according to the values and a result type cannot depend on a value (§5 T5);
`quot` is an alias of integer `/`, and ratios are a library type (below). Literals have one type, so `inc`, `dec`,
`abs`, `zero?`, `pos?`, `neg?`, `even?`, `odd?`, `mod` take a `Unit` witness (`(one x)`), which makes
them generic over every numeric type ([R] A7: design "zero"'s `num1`). Conversions are checked:
`(long x)`, `(int x)`, `(byte x)`, `(double x)`, `(char n)` dispatch on the argument type and trap when
the value does not fit, as Clojure throws; `(trunc i8 x)` keeps the low bits, never traps, and is `unchecked-byte`
(`unchecked-short unchecked-int unchecked-long unchecked-float unchecked-double unchecked-char` and the `-int`
families are aliases of `trunc`, `fptosi`, `sext`, `fptrunc`, `fpext` and the wrapping family at the right width, §4.3);
`(fptosi i64 x)` saturates. `unchecked-*` wrap at the operand's width and need a new builtin (§7 L10); `clojure.math` is `fib.math`,
with the IEEE-exact functions (`sqrt floor ceil rint copysign`) as builtins and the transcendental ones written
in fibber over `f64->bits`, so the interpreter and the compiled program agree to the bit (§7 L11); the Java
spellings `Math/sqrt`, `Math/pow`, `Math/abs`, `Math/floor`, `Long/MAX_VALUE`, `Integer/parseInt` are modules of
that name (A11 e19: a module `Long` gives `Long/MAX_VALUE` and `(Long/bitCount 255)` verbatim, `9223372036854775807` and
8, under both tools). `round` is half up, as Java's `Math.round` that Clojure's wraps.

**Literals (L19, Decided: owner, 2026-10-01).** An integer literal whose value is exactly representable adopts a
float type when it unifies with one; a variable never does through this rule. `(* 2 1.5)` is `cannot unify f64 with
i64` today ([R] A11 n2); with L19 it is `3.0`. By the same mechanism an integer literal adopts any integer type in which
it is exactly representable: `(+ x 1)` with `x: i32` is `cannot unify i64 with i32` today (A11 n3c), a friction that is not
about floats (§7 L26, a proposed extension: the rule is the one the owner gave, applied to the other widths); an
unsuffixed float literal adopts `f32` in an `f32` context, rounded to nearest, as Rust's `0.1`. No default is searched: an
unconstrained literal stays `i64` and `f64`. Literal expressions adopt together: `(+ 1 2)` where an `f64` is expected
is `(+ 1.0 2.0)`.

**Variables.** Clojure promotes at every mixed operation: `long` to `double` in `+ - * / < <= > >= == min max`, and
at `(Math/sqrt n)`. The promotion lattice `i8 < i16 < i32 < i64 < f64`, `f32 < f64` is statically typable and has
no memory-safety content (the lossy `i64` to `f64` above 2^53 is Clojure's own), so by the rule it is
Clojure's behaviour; the decided fact it overturns is types §1.1 D3 (no implicit conversion), a simplification
and not a necessity. Today, for variables, `(+ n 2.5)`, `(< n 2.5)`, `(f 2)` for `(f x: f64)` and `(/ (reduce-sum xs)
(count xs))` into an `f64` are each `cannot unify f64 with i64` ([R] A11 n1, n10, n11, t85), and
`(/ (sitofp f64 n) (sitofp f64 m))` is the form that runs (25, t84). The proposed rule is operator-level: when
the two operands of a builtin `Num`/`Ord`/`Eq` operator are distinct numeric types that are both known when the
constraint is solved, the narrower converts (`sitofp` from an integer to a float, `sext` between integers, `fpext`
from `f32` to `f64`), at the operands of the numeric builtins and at a parameter declared `f64`/`f32` or a wider
integer, never narrowing and never at a `let`, a return or a field; an operand whose type is still a variable when
a function generalises is unified as now, so `(defun add (a b) (+ a b))` stays `∀a. (Num a) ⇒ a a → a` and `(add n
2.5)` still fails: a *function of numbers* that accepts mixed numbers would need a promotion result on every
arithmetic function (a multi-parameter numeric protocol, §7 L23), which this page does not propose. It is a checker rule and a
deferred constraint in both tools (**[sketch]**, §7 L26) and lifts D3, so it is the owner's: §9 Q36 recommends yes.

**Ratios, `BigInt`, `BigDecimal`.** These are library types, not a non-goal: neither breaks memory safety or static
typing as an explicit type. A `Ratio` struct with `(impl Num Ratio ..)` and `Eq` makes `(/ 1/1 2/1)`, `(+ h (/ 1/1 3/1))`
(5/6) and a generic `(defun twice (x: a) :where ((Num a)) -> a (+ x x))` work through the builtin names ([R] A11 t50,
result 507, audit clean); the reader literals are `1N`, `1M` and `1/2` (E14), `numerator`, `denominator`, `rationalize`,
`bigint`, `bigdec`, `biginteger` and `with-precision` are library functions (tranche 5), and `+'`, `-'`, `*'`,
`inc'`, `dec'` return a `BigInt` always: what stays a deviation is Clojure's *auto-promotion* (a `Long` when it fits, a
`BigInt` when it does not), whose result type depends on the values (§5 T5).

**Randomness** is Clojure's: `rand`, `rand-int`, `rand-nth`, `shuffle`, `random-sample`, `random-uuid` use a
global generator, an `(Atom i64)` holding a xoshiro state seeded from the clock (a top-level `atom`, §2.11, needs L15),
and the explicit generator is the module `rng/`: `(rng/seed 42)` gives an `Rng` value and `(rng/rand r)`,
`(rng/rand-int r n)`, `(rng/shuffle r c)` take it, so a test is deterministic without a dynamic var. An `Rng` is a
value with its state in cells, one per task.

### 2.9 Strings

A `str` is immutable UTF-8 and, as Clojure's string is, **a seqable of characters**: a `Reducible char`, a `Lookup` from
an `i64` to a `char`, a `Seqable` (its seq type steps by one character) and a `Cursable`. `count`, `first`, `rest`,
`map`, `filter`, `reverse`, `seq`, `get`, `nth`, `subs`, `str/index-of` and `str/last-index-of` all count **characters**, so an
`index-of` result feeds `subs`, and `(apply str (filter digit? s))`, `(reverse s)` and `(frequencies s)` are Clojure's
text. The byte layer keeps honest names for tokenizers and the compiler: `str-len` (bytes, O(1)), `str-byte-at`,
`str-find`, `str-slice` (byte offsets, which trap when they split a character) and `str-bytes`; the compiler's 19
uses of `str-len` and its byte-offset reader are unaffected.

**Evidence that the unit is a choice and not a safety matter** ([R] A11 e3a, e3b, e3e, t10, t12, t13). A
`(impl (Reducible char) str ..)` in user code (the decoder of §2.1) gives, under both tools with a clean audit,
`(count "aé€😀z")` is 5, `(first "héllo")` `(some h)`, `(vec (map char->i32 "aé"))` `[97 233]`; with
character-offset `subs` and `index-of`, `(subs s 0 (count s))` is `aé€😀z`, `(subs s 1 3)` `é€`, `(index-of s "😀")`
`(some 3)`, `(subs-from s 3)` `😀z`, `(index-of s "q")` `nil`; `(reduce .. 0 s)`, an existential test over the characters (`any?` in the prototype, `some` in §2.4),
`(frequencies s)` and `(sort s)` (`[1 2 3 a b é]`) run. The first version's reason for bytes was that
`(subs s 0 (count s))` slices wrongly when `count` and `subs` use different units; with both in characters the
hazard cannot occur. Slicing by bytes at a non-boundary traps, `str-slice [0, 1) splits a character` (t12), and a
`str` built from invalid bytes traps, `str-from-bytes: invalid UTF-8` (t13): validity is enforced where a `str` is
built, so a character offset can never produce an invalid `str`. **The unit is the Unicode scalar, not Clojure's
UTF-16 unit**, and that is a memory-safety deviation (§5 M4): `(count "😀")` is 2 in Clojure because U+1F600 is a
surrogate pair, and a `subs` through the pair would have to make a `str` that is not valid UTF-8, whose lead
byte then claims continuation bytes that are not there; the decoder's bounds rest on the validity invariant. The two
agree on the Basic Multilingual Plane.

**The cost is O(n)** for `count`, `nth`, `subs`, `index-of` on non-ASCII text, since a character offset in UTF-8
is a walk. It is not a reason (§1.1), so the page follows Clojure and says what it costs: an **ASCII flag in the
`str` header**, computed during the UTF-8 validation that already scans every constructed `str`, makes all four
O(1) for ASCII text and a byte walk otherwise; it is a representation change in both tools (§7 C10, **[sketch]**),
and §9 Q37 recommends it. `(into "" xs)` is not Clojure (a string is not a collection that `conj` builds; it
throws), so there is no `Collection` instance for `str`; `(apply str xs)`, `(str/join xs)` and the `StrBuf` of §9 Q26
build text (`conj` onto a `str` by `str-concat` in a loop allocates a string per element, 4006 objects for 1000
characters, A11 e3f).

**Function by function.** `str/split` takes a pattern as Clojure's does: a `Regex`, `#","`, and, as a typed superset, a `str`
separator through the `Pattern` protocol (§2.3); it **drops trailing empty strings** as Java's does and `(str/split s re
-1)` keeps them. `str/replace` and `str/replace-first` take a `str`, `char` or `Regex` match through `Pattern`
and a `str` replacement (`$1` is interpreted for a regex match and not for a string, as Clojure's); a function
replacement is `str/replace-with` until L23 lets one name dispatch on the replacement's type too (A11 t70: two
instances of one head overlap); `str/re-quote-replacement` escapes `$` and `\`. `upper-case` and `lower-case` are
Unicode full case mappings, `upper` and `lower` on a `char` the simple one-to-one mapping; `Character/isDigit`,
`isLetter`, `isWhitespace` are the Java definitions (Unicode `Nd`, letters, `White_Space` without the no-break spaces).
`(str a b ..)` is a macro over `ToStr`; `format` and `printf` are macros that check each directive against its
argument's type at compile time when the format string is a literal and take a runtime check otherwise (the
arguments are `Arg` values built by `ToArg`, and a mismatch traps as Clojure throws `IllegalFormatException`).
`index-of` returns `(Option i64)`, never -1, as Clojure's `clojure.string/index-of` returns `nil`. **`Chars` is a decoder
over one byte array**: `(count (chars s))` of a 1000-character string allocates 2 objects, where the prelude's `str-chars` (a
`conj` loop of `(Vec char)`) allocates 2095 ([R] A10 chars), and the two agree on `"aé€😀z"`. Every string function of §4.7 over
bytes is built on `str-bytes`, which allocates a fresh array per call (100 objects for 100 calls, A10 str), so a tokenizer
that calls `str/index-of` from an offset copies the string each time; §7 L18 adds `(str-byte-at s i)` and `(str-find s pat from)`,
which allocate nothing. `parse-double` has the grammar `[+-]? (digits ['.' digits*] | '.' digits) ([eE] [+-]? digits)?` and
the names `NaN`, `Infinity` and `-Infinity` (**[H]**: the run of A10 pd covers the decimal part only); the grammar is checked in
fibber before `strtod` is called, because `strtod` accepts hex, a leading space, `inf` and trailing junk, and
`fibref` rejects hex (`unsupported: hex float`) where `fibc` prints 16.0, so the tools disagree without the
check ([R] A10 pd: the check followed by `strtod` gives the same eleven answers under both tools).

### 2.10 Errors and effects

A recoverable failure is a value where Clojure has no counterpart for a typed result, and an exception where it has: `trap`
is the only abort today (types §2.11, **Decided** 2026-09-28), and the rule says to follow Clojure unless memory safety
forbids. The two halves:

**Values.** `(Result a e)`, with `Ok` and `Err`, is in the prelude (it is `compiler/util/result.fib` today, which is deleted in the
commit that adds `Result`, §6.2), and `(try-let ((x e) (y f)) body)` binds each `Ok` payload in turn and yields the first `Err` as
the value of the whole expression. This is Rust's `?` ergonomics where Clojure has no typed counterpart. **It is not
`try!`**: the language has no early return, so a macro that leaves the enclosing function cannot be written
(`(return (Err e))` is `unbound name return`, [R] A10 try), and `try-let` is a block macro over nested `match` that runs
today, 106 under both tools (A10 try). Parsers return `Option` (`parse-long`) or `Result` (`regex`, `read-string`).

**Exceptions.** `throw`, `try`, `catch`, `finally`, `ex-info`, `ex-data`, `ex-message`, `ex-cause` and `Throwable->map` are Clojure's,
and **no memory-safety failure prevents them**: the failure the abort model avoids is a caught exception that skips the
releases of the frames it unwinds, which *leaks* (the audit would report it) and does not corrupt, because a surviving
frame holds its own counts and a stack object is one the compiler proved non-escaping (§1.1: a leak is not a safety
failure). A correct implementation releases the live owned locals of every unwound scope, and the ownership checker
already computes those sets for every scope exit (types §6.3). So the cost is plumbing, not a prohibition: an early exit
that is also a scope exit for each frame between `throw` and `catch`, as either unwinding tables (lIR has none:
`(trap)` is `llvm.trap`, "without unwinding") or an effect "may throw", inferred transitively, that makes each such
function return a hidden `Result` and reuses the release code of the normal return. Both are large and neither can be
prototyped without compiler changes (**[sketch]**, §7 L28). Today a `trap` in a spawned task ends the whole process:
`trap: boom` then `Aborted` under `fibc`, `trapped:` under `fibref` ([R] A11 t40, e9). The plan, in order: (1)
`ex-info`, `ex-data`, `ex-message`, `ex-cause` are plain data now, a struct `ExInfo` of a message, a `(Map keyword Val)` and an
`(Option ExInfo)` cause, usable as the `e` of a `Result`; (2) a task's trap is isolated, `join` returning the trap as a
value (Rust's `JoinHandle::join`), which needs no unwinding in the parent; (3) `throw` and `try`/`catch`/`finally` by one
of the two shapes above. Step (3) amends a decision the owner made before the rule, so it is §9 Q35; until it lands
`(throw e)` is a `trap` of `(ex-message e)`, and a program that must recover returns a `Result`. `finally`'s
non-memory cleanup (closing a handle) is the scope-exit hook of §7 L12, after which `with-open` is a macro over it.

**Effects.** Randomness is Clojure's global generator and the explicit `rng/` module (§2.8); time and the environment
are functions of `fib.sys`. `println` is a macro over `Show` (several arguments are joined with a space, [R] A10 pm); the
prelude's one-`str` function of that name is replaced in value position by a **function twin generic over `Show`**,
and so are `str`, `print`, `prn` and `pr`: `(run! println [1 2 3])` and `(map str xs)` work, which a macro alone cannot
do (`unbound name str`), because a macro and a function of one name coexist (head position expands, [R] A6, A10 twin).

### 2.11 Callables, state and scope

**Callable collections and keywords.** In Clojure a `Map`, a `Set`, a `Vector` and a keyword are functions: `(m k)`,
`(#{1 2} x)`, `(v 0)`, `(:k m)`, `(map m ks)`, `(filter #{1 2} xs)`, `(some #{x} c)`. Types §1.4 decided that only functions are
callable, and it is a simplification, not a necessity: the types are known, so a `Lookup` value in call position or
where a `(fn (K) R)` is expected is eta-expanded to `(fn (k) (get x k))`. Today each is a type error ([R] A11 t51, n5:
`cannot unify (Map i64 i64) with (fn (a) b)`; n6: `(filter s [1 2 3])` with a `Set` is `cannot unify (Set i64) with
(fn (a) bool)`; k1: `(:a m)` is `cannot unify keyword with (fn (a) b)`; k2: `(v 1)` is `cannot unify (Vec i64) with (fn (a)
b)`). The rule (§7 L21 **[sketch]**, generalising L14) is: a `Map k v` is `(fn (k) (Option v))` and with a default
`(m k d)`; a `Set k` is `(fn (k) (Option k))`, which is a predicate through the `Truthy` of §2.3 so that `(filter #{1 2}
xs)` works; a `Vec e` is `(fn (i64) e)` and traps out of range as Clojure's `(v 5)` throws; a keyword is `(fn (S) T)`
elaborating to the field read when `S` is a struct with that field and to `get` when `S` is a `(Map keyword v)`,
and `(:k x d)` supplies the default. After C1, `Fn` is a protocol and each is one `impl` and needs no special rule. A
record by keyword is the same mechanism: `(assoc r :k v)` is `(with r (k v))`, `(update r :k f)` and `(:k r)` the field
update and read, type-checked against the field; the macros that do it for a literal keyword run today ([R] A11 e14: 30,
31, 32), and the checker rule is what tells a struct from a `Map` at the same call. `(assoc p :x 5)` on a struct is `no
implementation of Associative for P` otherwise (A11 t53).

**Top-level state.** `def` evaluates **any** expression once before `main`, in module order (an init function per
module, §7 L15): `(def counter (atom 0))`, `(def stopwords #{"a" "the"})`, `(def table (zipmap ks vs))`, and `defonce` is
`def`. Today the initialiser must be a constant: `(def counter: (Atom i64) (atom 0))` is `def counter: initialiser is not
a constant expression` ([R] A11 t56, n8b), a restriction of the checker's grammar and not a safety rule. **The one
exclusion is memory safety (§5 M1):** a top-level `Cell` or `Weak` is reachable from every task and written by two. The
checker already stops the closure form of the race, `cell cannot be shared between threads: closure capture c has type
(Cell i64)` ([R] A11 t41, e8d), and `(def c: (Cell i64) (cell 0))` is rejected today as `def c: initialiser is not a constant
expression` (n8); with L15 the rule becomes a type test, *the type of a `def` may not contain a `Cell` or a `Weak`*, and an
`Atom`, whose payload is `Send`, is allowed. `volatile!` is `cell`, `vswap!` is `(set! c (f @c))` and `vreset!` is `set!`, as Clojure's volatiles are thread-confined.

**Dynamic vars.** `(def :dynamic *x*: T v)` (Clojure's `^:dynamic`) declares a var of one type; `(binding [*x* e] body)`,
`with-bindings`, `bound-fn` and `with-redefs` (below) rebind it for the dynamic extent of the body. A binding is an owned,
counted value released at the scope's exit; there are no exceptions, so no unwinding issue; the bindings are conveyed to a
`spawn`ed task as a captured `Send` value, as Clojure's `future` conveys them. It needs a per-task binding slot in the task
header (§7 C11). `*out*`, `*in*`, `*err*`, `*print-length*` and `*command-line-args*` are such vars; `println` writes to
`*out*` (a `(dyn Writer)`), and `with-out-str` binds it to a string-building writer (the `StrBuf` of §9 Q26), `with-in-str`
likewise, and `(args)` is the builtin under the Clojure name `*command-line-args*`.

**Metadata.** `meta`, `with-meta`, `vary-meta`, `alter-meta!` and `reset-meta!` are a `(Option (Map keyword Val))` field on the four
collection headers and on structs; `Eq` and `Hash` ignore it; `^:private` is the `:private` qualifier, `^:dynamic` the
`:dynamic` qualifier and `^Type` the annotation `x: T` (checked). A representation change with `Val` (§9 Q20 settled:
`fib.data`), tranche 5.

**Multimethods and hierarchies.** `(defmulti area (fn (s) (:type s)))` declares a `(Multi d a r)`, a global `(Atom (Map d (fn :send (a) r)))`
and the dispatch function; `defmethod` is a top-level form that registers a method in the module init; `derive`, `isa?` and
`parents` build and query an explicit hierarchy value held in an `Atom`. `d`, `a` and `r` are fixed per multimethod: static
typing is kept, and the dispatch on a *value* is what a protocol (dispatch on a type) is not. Tranche 5; needs L15.

**Concurrency.** `locking` is `(locking m (fn (v) ..))` on a **`Mutex a` that owns its value**, Rust's form, because Clojure's lock
on an arbitrary object would let two tasks hold one `Cell` (§5 M6, needs a runtime mutex, §7 C9). `add-watch`,
`remove-watch`, `set-validator!` and `get-validator` are a library: a `Ref` of an `Atom`, a watch list `(Atom (Vec (fn :send (a a)
unit)))` and a validator `(fn :send (a) bool)`, called after `swap!`, which type-checks and runs, 42 under both tools with a clean
audit (A11 e18; t42 gives 50): the first version's reason for omitting them, "callbacks cannot hold borrowed values
safely", is false because a `:send` closure holds owned values only. `future` is a macro over `spawn` and `@f` is `join`;
`future-cancel` sets a flag that the task polls with `(cancelled?)`, which is Java's interrupt (cooperative at blocking
points) in a form that cannot skip a release (§5 M5); `pmap` is eager with one task per element today, chunked over
`ncpu + 2` tasks with the order kept in tranche 3 ([R] A11 e15), and lazy over an `LSeq` of futures in tranche 5.
`with-redefs` is an opt-in: a function declared `:redefinable` calls through a global `(Atom (fn :send ..))` slot and
`with-redefs` swaps it, so only a function that asks pays an indirect call (a direct call is what the monomorphiser wants
everywhere else). `with-open` is a macro over the scope-exit hook of §7 L12. `letfn` binds a group of mutually
recursive local functions: a macro over cells works and leaks a reference cycle per entry, `audit: clean=false
leak-cycles=4` (A11 t55, a leak and not a safety failure); the cure is one shared closure environment for the group (§7 C8), so
`letfn` ships with C8.

## 3. Argument order and naming

| # | Rule | Examples |
|---|---|---|
| N1 | A function over a **sequence** takes the sequence **last**; a function over a **collection value** takes it **first**. So `->>` threads sequences and `->` threads collections. | `(map f c)`, `(take n c)`, `(reduce f init c)`; `(assoc m k v)`, `(get m k)`, `(conj c x)`, `(update m k f)`, `(subvec v a b)` |
| N2 | The function argument of a sequence function comes first, so a literal `fn` reads before the data. | `(filter p c)`, `(group-by f c)`, `(sort-by key c)` |
| N3 | The exceptions Clojure has are kept and listed: `into` is collection-first for its target; `str/join` and `str/split` take the separator and the string in Clojure's two orders; `reduce-kv` (`f init m`) and `set/select` (`p s`) take the collection last, as Clojure's do; `str/includes? s sub` takes the string first. The library's own `includes?` (new) takes the element first (`(includes? x c)`) so that `->>` threads the source; Clojure has no such name, its idiom is `(some #{x} c)`. | `(into to c)`, `(str/join sep c)`, `(str/split s re)`, `(reduce-kv f init m)` |
| N4 | **Clojure's own suffixes, with their inconsistency kept**: `-by` takes a key function in `sort-by group-by partition-by` and a comparator in `sorted-map-by sorted-set-by`; `-key` is `max-key`/`min-key`; `-with` takes a combiner in `merge-with`. The first version renamed `sorted-map-by` to `sorted-map-with` "for one suffix, one meaning", which is not a reason (§1.1). The stand-ins for arities that need L1 (`sort-with`, `sort-by-with`, `get-or`, `nth-or`, `reduce1`, `range-by`, `zip-with`) are deleted when L1 lands. | `(sort-by count c)`, `(sort-by count > c)`, `(sorted-map-by > 1 2)`, `(merge-with + a b)` |
| N5 | A `?` suffix is a `bool` result; a `!` suffix means the function writes through an `&` parameter, a cell or an `Atom` (`push! swap! reset! set!`), is a transient wrapper (`conj!`), or runs only for its effect on each element (`run!`, Clojure's). A persistent function never ends in `!`. | `empty?`, `contains?`, `push!` |
| N6 | `->` in a name is a conversion `from->to`, as the builtins have it (`char->i32`); the library's conversions that Clojure names `long`, `int`, `double` keep those names. The builtin casts that are one instruction name the target first (`(trunc i8 x)`, `(zext i64 b)`, `(fptosi i64 x)`): two conventions, because the first is a name and the second a type argument. | `(long x)`, `(char->str c)` |
| N7 | A function that returns a recipe is named for what it does; an eager twin exists where Clojure has it (`mapv filterv`) and `doall` is `vec`. Materialisers are `vec set into zipmap sort reverse`. | `(vec (map f c))` |
| N8 | Types are CamelCase; an adaptor's struct is named for the adaptor in the past participle where its verb has one (`Mapped Filtered Taken Dropped Kept`) and by the noun otherwise (`Cat Mapcat Iterate Repeat Cycle Reductions Distinct ZipWith TreeSeq`). §4 writes `(Mapped c e b)`, the data parameters, which is what compiles today; §2.2 writes `(Mapped c f)`, the form after C1, and a colour parameter follows either (§2.1). Modules are `fib.x`, required with an alias (`str/`, `set/`, `math/`, `rng/`), and the Java class names `Math`, `Long`, `Integer`, `Double`, `Character`, `System`, `Thread` are implicit aliases of modules (§4.16). | |
| N9 | A name that the prelude's `join` (the task wait) or core forms use is never redefined unqualified; the library's `join` is always `str/join`, as Clojure's `clojure.string/join` is. | |
| N10 | In the signature column of §4: parameters in order, `->` result, ` \| ` introduces the protocol constraints (`Reducible c e`), lower-case letters are type variables, `;` separates the signatures of the arities of one name. `c` is a source, `s` a keyed collection, `e` an element. | |
| N11 | An `Option` function takes the function first and the `Option` last, as a sequence function does, so `->>` threads it: `(map-opt f o)`, `(and-then f o)`. They are the library's additions where Clojure has `nil` punning and `some->`. | `(->> o (map-opt inc) (and-then half))` |
| N12 | **Clojure's name is the name.** Where fibber already has a builtin under another spelling (`array-get`, `array-set!`, `array-len`, `array-copy`, `shl`, `sar`, `shr`, `popcount`, `!=`, `defun`, `read-file`, `write-file`, `spawn`, `i64-max`), Clojure's spelling (`aget`, `aset`, `alength`, `aclone`, `bit-shift-left`, `bit-shift-right`, `unsigned-bit-shift-right`, `Long/bitCount`, `not=`, `defn`, `slurp`, `spit`, `future`, `Long/MAX_VALUE`) is an alias of it, and the existing spelling stays for the cases and the compiler that use it. A name the library adds because Clojure's text has no typed counterpart is marked `(new)` and never takes the place of a Clojure name. | `(aget a i)`, `(bit-shift-left x 3)`, `(not= a b)` |

## 4. The function table

Every name of the survey has one row: 673 names of `clojure.core` 1.12, `clojure.set`,
`clojure.string`, `clojure.walk`, `clojure.data` and `clojure.math`, grouped by the module that holds
it, plus 42 names that Clojure lacks and the library adds (marked `(new)`). `#"..."` shares the row of `#"regex"`. Columns:

* **Verdict**: `keep` (same name, same meaning), `adapt` (same name and text, a typed twist, stated), `alias` (Clojure's name
  for something fibber already has under another spelling or as a one-line function: the same meaning), `new`, and, in §4.17,
  `omit` (not offered: the JVM, no run-time type information, a result type that depends on a value, or a name that means something
  else here). The first version also had `rename` and `replace`; the rule removed them, because a name Clojure has is the name
  (§3 N12). Over the survey's names: keep 144, adapt 367, alias 72, omit 89 (the first version: keep 138, adapt 206, rename 47, replace 82,
  omit 200; §10.4.6 says what happened to each). The counts are those of a script that parses these tables (`build4.py` of the rule
  review), which the executable `stdlib_table` of §8.1 replaces.
* **Fibber**: the spelling. `macro`, `core form`, `reader`, `pattern` say what kind of thing it is.
* **Signature**: in the notation of §3 N10; each arity of an overloaded name is one signature, separated
  by `;`. Where an arity needs L1 (§7), the stand-in the first tranches use is named in the note.
* **T**: the tranche of §8 that delivers it (names that exist today are `1`).
* **Note**: what differs from Clojure, and where it matters, the cost.

Offered over the survey's names by tranche: T1 154, T2 82, T3 170, T4 93, T5 84.

### 4.1 Builtins (compiler primitives, syntax §4.3)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `aget` | alias | `(aget a i)` | `(Array a) i64 -> a` | 1 | alias of the builtin `array-get`; traps out of range |
| `aset` | adapt | `(aset &a i x)` | `&(Array a) i64 a -> unit` | 1 | alias of `array-set!`; **value semantics**: a write through `&` is in place only when `a` is unique, else it copies, so a second holder keeps the old array (9, not Clojure's 99, [R] A11 t54): an aliased in-place write would be a data race between tasks and would corrupt the shared nodes of the persistent collections (§5 M2) |
| `alength` | alias | `(alength a)` | `(Array a) -> i64` | 1 | alias of `array-len` |
| `aclone` | alias | `(aclone a)` | `(Array a) -> (Array a)` | 1 | alias of `(array-copy a 0 (array-len a))` |
| `make-array` | adapt | `(make-array T n)` | `i64 -> (Array T) \| T scalar or (Option a)` | 1 | a macro on the type argument: zero for a scalar, `nil` for an `Option`; an object type has no default and no null slot (§5 M3), so the builtin `(array n x)` takes the element and `(array 3)` is `array takes 2 argument(s), got 1` ([R] A11 t90); an uninitialised array for the library is §7 C4 |
| `+` | adapt | `(+ a b ..) (+)` | `t t -> t \| Num t; -> t \| Num t` | 1 | the builtin is binary on one type, overflow traps (Clojure's `+` throws on a `long` overflow too); a macro folds more arguments; `(+)` is `0`, the literal adopting its context's numeric type (L19); a variable of another numeric type: §9 Q36 |
| `-` | adapt | `(- a b ..) (- a)` | `t t -> t \| Num t; t -> t \| Num t` | 1 | binary builtin, `neg` for one argument; macro folds; `(-)` is an arity error, as Clojure's |
| `*` | adapt | `(* a b ..) (*)` | `t t -> t \| Num t; -> t \| Num t` | 1 | as `+`; `(*)` is `1`; `product` for a collection |
| `/` | adapt | `(/ a b)` | `t t -> t \| Num t` | 1 | integers truncate toward zero and trap on zero: Clojure's `(/ 7 2)` is the ratio `7/2`, a result type that depends on the values (§5 T5), so `quot` is the alias and `Ratio` a library type (§2.8) |
| `quot` | alias | `(quot a b)` | `t t -> t \| Num t` | 1 | alias of `/` on integers: truncates toward zero, traps on zero |
| `rem` | keep | `(rem a b)` | `t t -> t \| Num t` | 1 | builtin; the sign of the dividend |
| `<` | adapt | `(< a b ..)` | `t t -> bool \| Ord t` | 1 | builtin `Ord` method on any ordered type; a macro chains more arguments ; mixed numeric operands: literals adopt (L19), variables §9 Q36 |
| `>` | adapt | `(> a b ..)` | `t t -> bool \| Ord t` | 1 | as `<` |
| `<=` | adapt | `(<= a b ..)` | `t t -> bool \| Ord t` | 1 | IEEE at floats |
| `>=` | adapt | `(>= a b ..)` | `t t -> bool \| Ord t` | 1 | IEEE at floats |
| `bit-and` | keep | `(bit-and a b ..)` | `t t -> t \| Bits t` | 1 | builtin, binary; a macro folds more |
| `bit-or` | keep | `(bit-or a b ..)` | `t t -> t \| Bits t` | 1 | as `bit-and` |
| `bit-xor` | keep | `(bit-xor a b ..)` | `t t -> t \| Bits t` | 1 | as `bit-and` |
| `bit-not` | keep | `(bit-not a)` | `t -> t \| Bits t` | 1 | builtin |
| `bit-shift-left` | alias | `(bit-shift-left a n)` | `t t -> t \| Bits t` | 1 | alias of the builtin `shl`; the distance is taken modulo the width, as Java's |
| `bit-shift-right` | alias | `(bit-shift-right a n)` | `t t -> t \| Bits t` | 1 | alias of the builtin `sar` (arithmetic) |
| `unsigned-bit-shift-right` | alias | `(unsigned-bit-shift-right a n)` | `t t -> t \| Bits t` | 1 | alias of the builtin `shr` (logical) |
| `Long/bitCount` | alias | `(Long/bitCount a)` | `t -> t \| Bits t` | 1 | alias of the builtin `popcount`, through the module `Long` (§4.16) |
| `=` | adapt | `(= a b ..)` | `t t -> bool \| Eq t` | 1 | structural on one type; within a family (sequential, map, set) across types by the checker (`seq=`, §2.7, L24); `(= 1 1.0)` is true for a literal by L19 (§5 C1); a macro chains more arguments |
| `==` | alias | `(== a b ..)` | `t t -> bool \| Eq t` | 1 | alias of `=` on numbers: across types for a literal through L19, for variables §9 Q36 |
| `not=` | alias | `(not= a b ..)` | `t t -> bool \| Eq t` | 1 | alias of the builtin `!=` |
| `not` | keep | `(not x)` | `r -> bool \| Truthy r` | 1 | builtin on `bool`; a condition of type `(Option T)` is accepted by L20: `(not (get m k))` |
| `atom` | keep | `(atom v)` | `a -> (Atom a) \| Send a` | 1 | builtin; an atom of a type that is not `Send` is a compile error; a top-level `(def a (atom 0))` is allowed (§2.11) |
| `swap!` | adapt | `(swap! a f arg ..)` | `(Atom a) (fn (a ..) a) .. -> a` | 1 | a prelude macro over the builtin: `(swap! a + 5)` is `(swap! a (fn (x) (+ x 5)))`, and the macro and the builtin coexist ([R] A10 swap: 30 under both tools); returns the new value; `f` may run more than once |
| `reset!` | keep | `(reset! a v)` | `(Atom a) a -> unit` | 1 | builtin |
| `deref` | keep | `@x` | `(Atom a) -> a` | 1 | builtin |
| `set!` | adapt | `(set! c v)` | `(Cell a) a -> unit` | 1 | builtin on a `Cell`; also `(set! (. obj field) v)`, and on a dynamic var inside `binding` (§2.11) |
| `volatile!` | alias | `(volatile! x)` | `a -> (Cell a)` | 2 | alias of `cell`; thread-confined, as Clojure's volatile |
| `vswap!` | adapt | `(vswap! c f arg ..)` | `(Cell a) (fn (a) a) .. -> a` | 2 | a macro for `(set! c (f @c arg ..))`; returns the new value |
| `vreset!` | alias | `(vreset! c v)` | `(Cell a) a -> a` | 2 | `set!` that returns `v` |
| `long-array` | alias | `(long-array n) (long-array c)` | `i64 -> (Array t); c -> (Array t) \| Reducible c t` | 3 | `(array n 0)`, or the elements of a source |
| `double-array` | alias | `(double-array n) (double-array c)` | `i64 -> (Array t); c -> (Array t) \| Reducible c t` | 3 | `(array n 0.0)`, or the elements of a source |
| `float-array` | alias | `(float-array n) (float-array c)` | `i64 -> (Array t); c -> (Array t) \| Reducible c t` | 3 | `(array n 0.0f32)`, or the elements of a source |
| `short-array` | alias | `(short-array n) (short-array c)` | `i64 -> (Array t); c -> (Array t) \| Reducible c t` | 3 | `(array n 0i16)`, or the elements of a source |
| `int-array` | alias | `(int-array n) (int-array c)` | `i64 -> (Array t); c -> (Array t) \| Reducible c t` | 3 | `(array n 0i32)`, or the elements of a source |
| `byte-array` | alias | `(byte-array n) (byte-array c)` | `i64 -> (Array t); c -> (Array t) \| Reducible c t` | 3 | `(array n 0i8)`, or the elements of a source; the same `(Array i8)` that `str-bytes` returns, signed |
| `boolean-array` | alias | `(boolean-array n) (boolean-array c)` | `i64 -> (Array t); c -> (Array t) \| Reducible c t` | 3 | `(array n false)`, or the elements of a source |
| `char-array` | alias | `(char-array n) (char-array c)` | `i64 -> (Array char); c -> (Array char)` | 3 | an `(Array char)` of Unicode scalars |
| `object-array` | adapt | `(object-array n)` | `i64 -> (Array (Option a))` | 3 | filled with `nil`, as Clojure's |
| `to-array` | adapt | `(to-array c)` | `c -> (Array e) \| Reducible c e` | 3 | a typed `(Array e)` from any source; `into-array` the same with a type argument (an empty source keeps its type); `to-array-2d` an `(Array (Array a))` |
| `into-array` | alias | `(into-array c)` | `c -> (Array e) \| Reducible c e` | 3 | as `to-array` |
| `to-array-2d` | adapt | `(to-array-2d c)` | `c -> (Array (Array e))` | 3 | a source of sources |
| `amap` | adapt | `(amap a i ret expr)` | macro | 3 | a loop with `array-set!` over a copy: value semantics already |
| `areduce` | adapt | `(areduce a i ret init expr)` | macro | 3 | a loop over `array-get`; `(reduce f init a)` is the function form, an `(Array a)` being `Reducible` |

### 4.2 Core forms, macros and reader syntax

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `doseq` | keep | `(doseq [x xs :when p] body)` | macro | 2 | runs for effect, same modifiers |
| `if` | keep | `(if c a b) (if c a)` | core form | 1 | `c` is `bool` or `(Option T)` (§2.4, L20); the one-armed form is `unit` for a unit arm, else `(Option T)` (syntax §3.4) |
| `if-not` | adapt | `(if-not c a b)` | macro | 2 | `(if (not c) a b)`; one-armed as `if` |
| `if-let` | adapt | `(if-let [x e] a b)` | macro `(Option a)` | 1 | spelled `(if-let (x e) a b)` today, the bracket form needs §7 E3; over a `Truthy` (`bool`, `(Option a)`; `x` binds the payload) with any pattern, refutable ones included: the macro expands to `(match e ((some x) a) (_ b))` (§2.4) |
| `when` | adapt | `(when c body ..)` | macro | 1 | the value is `unit` when the body is, else `(Option T)` with `some` around the body (L20); `(when c 5)` is `cannot unify unit with i64` today ([R] A11 t82) |
| `when-not` | adapt | `(when-not c body ..) (unless c body ..)` | macro | 2 | `unless` is the prelude's name, kept as an alias: nothing requires removing it; one-armed as `when` |
| `when-let` | adapt | `(when-let [x e] body ..)` | macro | 1 | spelled `(when-let (x e) ..)` today (bracket form: §7 E3); over a `Truthy`; the value is `unit` or `(Option T)` as `when` |
| `cond` | adapt | `(cond t1 e1 t2 e2 :else d)` | macro | 2 | Clojure's flat pairs replace the prelude's `(cond (t e) ..)` (§7 E4); `:else` or `true` ends it; **falling off the end is `nil`** (`unit` or `(Option T)` as `when`, L20), as Clojure's |
| `condp` | adapt | `(condp pred x t1 r1 t2 r2 default)` | macro | 3 | over `cond`; no clause and no default traps `condp: no matching clause`, as Clojure throws; the `:>>` clause passes the test result to a function |
| `case` | adapt | `(case x lit r (lit1 lit2) r2 default)` | macro | 3 | expands to `match` with literal patterns and or-patterns (§7 L6); no match and no default traps `case: no matching clause`, as Clojure throws; a compile error only for an enum with an uncovered variant |
| `and` | adapt | `(and a b ..)` | core form `a b -> typeof b \| Truthy a` | 1 | a checker form (L20): every operand but the last is a condition (`bool` or `(Option T)`); the value has the last operand's type; short-circuit |
| `or` | adapt | `(or a b ..)` | core form `(Option t) t -> t; (Option t) (Option t) -> (Option t); bool bool -> bool` | 1 | a checker form (L20): `(or (get m k) 0)` is Clojure's text; `(unwrap-or o d)` is the spelling that runs today; the two result types for one first operand are why a library `or` overlaps (A11 t70, e2) |
| `do` | keep | `(do e ..)` | core form | 1 | `(do)` is `()` |
| `let` | adapt | `(let [a 1 b 2] body)` | core form | 2 | the bracket form is sugar over the parenthesised pairs (§7 E3); patterns allowed, refutable ones trap (§7 L8) |
| `loop` | keep | `(loop [i 0 acc x] ..)` | core form | 2 | as `let`: bracket sugar |
| `recur` | keep | `(recur ..)` | core form | 1 | tail position inside `loop` only |
| `fn` | adapt | `(fn [x y] body)` | core form | 2 | bracket parameters are sugar; patterns allowed in parameters (§7 L7); no arity overloading in a `fn` literal: a closure value has one type with one arity (§5 T2) |
| `def` | adapt | `(def name: T v)` | core form | 1 | immortal; evaluates **any** expression once before `main`, in module order (§2.11, L15); the type may not contain a `Cell` or a `Weak` (§5 M1); `:dynamic` declares a dynamic var. Today the initialiser must be a constant: `(def ok: (Set i64) (set [1 3]))`, `(def v: i64 (f 2))` and `(def counter: (Atom i64) (atom 0))` are `def .. : initialiser is not a constant expression`, while `(def v: (Vec i64) [1 2 3])` and `{1 2}` run ([R] A10 def, A11 t56) |
| `defmacro` | keep | `(defmacro name (params) body)` | core form | 1 | rest parameter `...` |
| `dotimes` | keep | `(dotimes [i n] body)` | macro | 1 | prelude macro, spelled `(dotimes (i n) ..)` today; the bracket form needs §7 E3 |
| `for` | adapt | `(for [x xs :when p :let [y e] y ys] body)` | macro | 2 | a recipe (`Mapped`/`Mapcat`/`Filtered` chain), not a `Vec`; `:when`, `:while`, `:let` |
| `while` | keep | `(while c body ..)` | macro | 1 | `c` is a condition: `bool` or `(Option T)` (L20) |
| `doto` | keep | `(doto x (f a) g)` | macro | 1 | prelude macro |
| `with` (new) | new | `(with e (field v) ..)` | macro `s -> s` | 2 | a copy of the struct `e` with the named fields replaced, `e` untouched: Clojure's `assoc` on a record, and Rust's `S { f: v, ..e }`; it expands to a cell and `set-field!`, which runs today, in a module that does not itself define `concat` (B2): `(with db (users ..) (n 3))` gives 4 3 3 2 under both tools ([R] A10 with); `update-in` over records is `with` nested; a unique `e` can reuse its object (§2.5, C7) |
| `comment` | keep | `(comment ..)` | macro | 3 | expands to `()` |
| `time` | adapt | `(time e)` | macro | 4 | prints the elapsed time to stderr, returns `e`'s value; needs `now-ns` |
| `assert` | keep | `(assert c) (assert c msg)` | macro | 1 | prelude macro; always on, traps |
| `var` | keep | `(var name)` | core form | 1 | the definition a name resolves to, not a Var object |
| `quote` | keep | `'x` | core form | 1 | the value is a `Form` |
| `ns` | keep | `(ns a.b (:require [c.d :as d]) (:use e))` | core form | 1 | `:require` and `:use`, and `:refer`, `:only`, `:exclude`, `:rename` (E15); no `:import` |
| `defprotocol` | keep | `(defprotocol (P self det*) (m (self x: T) -> R))` | core form | 1 | static dispatch |
| `->` | keep | `(-> x f (g a))` | macro | 1 | prelude macro |
| `->>` | keep | `(->> x f (g a))` | macro | 1 | prelude macro |
| `as->` | keep | `(as-> x v (f a v) ..)` | macro | 2 |  |
| `cond->` | adapt | `(cond-> x t1 f1 t2 f2)` | macro | 3 | tests are conditions (`bool` or `(Option T)`), flat pairs; a test is never given the value, as Clojure's |
| `cond->>` | adapt | `(cond->> x t1 f1 t2 f2)` | macro | 3 | as `cond->` |
| `some->` | adapt | `(some-> o f g)` | macro `(Option a)` | 3 | each step takes the unwrapped value; a step that returns an `(Option b)` is kept and a plain `b` is wrapped (§2.4, L22 [sketch]); stops at the first `nil`; until L22, `(some-> o (and-then f) (map-opt g))` |
| `some->>` | adapt | `(some->> o f g)` | macro `(Option a)` | 3 | as `some->`, threading last |
| `partial` | adapt | `(partial f a ..)` | arity-reading form `(fn (A.. B..) R) A.. -> (fn (B..) R)` | 2 | leaves the rest of `f`'s parameters free, read from `f`'s type (L22 [sketch]); the Rust macro of today leaves exactly one ([R] A11 arity: `(partial add3 1)` is `cannot unify (fn :send (i64 i64 i64) i64) with (fn (a b) c)`) |
| `partial2` (new) | new | `(partial2 f a)` | macro `(fn (a b c) d) a -> (fn (b c) d)` | 2 | a stand-in for the second free parameter, deleted when L22 lands; not a Clojure name |
| `comp` | adapt | `(comp f g ..) (comp)` | arity-reading form `(fn (B) C) (fn (A..) B) -> (fn (A..) C)` | 1 | right to left; the result has the arity of the last function (L22; today `(comp inc add2)` is `cannot unify (fn :send (i64 i64) i64) with (fn (a) i64)`, A11 arity2); `(comp)` is `identity`; a macro nests for more; transducers compose with `xf` until C1, after which `(comp (map f) (filter p))` dispatches on `Fn` and `Xf` (§5 T9) |
| `juxt` | adapt | `(juxt f g)` | `(fn (A..) B) (fn (A..) C) .. -> (fn (A..) (Pair B C))` | 3 | arity-reading (L22); two or three functions give `Pair`/`Triple`; more give a `Vec` when the results unify, `(map (fn (f) (f x)) fs)` otherwise |
| `complement` | adapt | `(complement p)` | `(fn (A..) bool) -> (fn (A..) bool)` | 2 | arity-reading (L22); over a `Truthy` result |
| `constantly` | adapt | `(constantly x)` | `b -> (fn (A..) b)` | 2 | any arity, which comes from the expected type (L22); unary today (`(constantly 7)` where a `(fn (i64 i64) i64)` is expected is `cannot unify (fn (a) i64) with (fn (i64 i64) i64)`, A11 arity3) |
| `fnil` | adapt | `(fnil f d)` | `(fn (A..) b) a -> (fn ((Option a) ..) b)` | 1 | any arity (L22); the `Option` is what `update-opt` hands over; `(update-or m k f d)` is the short form |
| `memoize` | adapt | `(memoize f)` | `(fn (a) b) -> (fn (a) b) \| Hash a, Eq a` | 4 | arity-reading (L22): the cache key is the tuple of the arguments; keeps an `(Atom (Map k v))`; `f` must be sendable |
| `every-pred` | adapt | `(every-pred p q)` | `(fn (A..) r) .. -> (fn (A..) bool) \| Truthy r` | 3 | arity-reading (L22) |
| `some-fn` | adapt | `(some-fn f g)` | `(fn (A..) r) .. -> (fn (A..) (Option x)) \| Truthy r, Payload r x` | 3 | arity-reading (L22); the first truthy value |
| `completing` | adapt | `(completing f fin)` | `(fn (a e) a) (fn (a) r) -> (Completing a e r)` | 5 | with transducers |
| `cat` | adapt | `(cat)` | `-> (CatXf)` | 5 | the transducer form of `mapcat identity`; with transducers |
| `halt-when` | adapt | `(halt-when p)` | `(fn (e) bool) -> (HaltXf e)` | 5 | with transducers |
| `:keys` | adapt | `{:keys [x y]}` | pattern | 3 | struct field punning in `let`/`fn`/`match` (§7 L9); on a `(Map keyword v)` it binds `(get m :a)`, an `(Option v)`, or `(unwrap-or (get m :a) d)` for a key with an `:or` default |
| `:strs` | adapt | `{:strs [a b]}` | pattern | 3 | as `:keys` over a `(Map str v)` |
| `:syms` | adapt | `{:syms [a b]}` | pattern | 3 | as `:keys` over a map keyed by `Form` symbols (macros) |
| `:or` | adapt | `{:keys [a] :or {a 1}}` | pattern | 3 | supplies `(unwrap-or (get m :a) 1)` for the key |
| `:as` | keep | `(pat :as v)` | pattern | 1 | syntax §3.6 |
| `&` | keep | `[a b & r]` | pattern | 1 | `r` is a new `Vec` |
| `_` | keep | `_` | pattern | 1 | the wildcard |
| `'x` | keep | `'x` | reader | 1 | syntax §1.2 |
| ``x` | adapt | ``x` | reader | 1 | quasiquote with the `x#` auto-gensym (E14) and, with E11, qualification of free symbols in the macro's defining module |
| `~x` | keep | `~x` | reader | 1 | Clojure's unquote; the comma becomes whitespace (E14: `[1,2]` is `[1 2]`); `,x` is the spelling of today and migrates with the macros of `lib/` and `compiler/` |
| `~@x` | keep | `~@x` | reader | 1 | as `~x`, splicing a `(Vec Form)` |
| `@x` | keep | `@x` | reader | 1 | cells, atoms and weak references |
| `#'x` | adapt | `#'x` | core form | 3 | reads as `(var x)` (E14) |
| `#_` | keep | `#_` | reader | 1 | syntax §1.1 |
| `#(...)` | adapt | `#(f % %2)` | reader | 2 | reads as `(fn (%1 %2) (f %1 %2))`; `%` is `%1`; no `%&`; nesting is an error (§7 E8) |
| `#{...}` | adapt | `#{a b}` | reader | 2 | reads as `(hash-set a b)` (§7 E8) |
| `(:k m)` | adapt | `(:k x)` | checker | 2 | a keyword that unifies with `(fn (S) T)` elaborates to `(fn (x) (. x k))` when `S` is a struct with that field and to `get` when `S` is a `(Map keyword v)`; `(:k m d)` supplies a default; `(map :name ps)`, `(sort-by :age ps)`, `(group-by :dept ps)` are the commonest Clojure lines (§7 L14, L21). A `Map`, `Set` and `Vec` are callable the same way (§2.11) |
| `^Type` | adapt | `^T x` | reader | 3 | reads as `x: T` (E14); `^:private` is the `:private` qualifier, `^:dynamic` the `:dynamic` qualifier |
| `^meta` | adapt | `^{:k v} x` | reader | 5 | attaches metadata (§2.11); `^:k` is `^{:k true}` |
| `::kw` | adapt | `::kw` | reader | 3 | reads as the flat keyword `:module/kw` of the current module (E14) |
| `1N 1M 1/2` | adapt | `1N 1M 1/2` | reader | 5 | literals of `BigInt`, `BigDecimal`, `Ratio` (E14, §2.8); width suffixes `1i32 2.5f32` stay |
| `##Inf ##-Inf ##NaN` | adapt | `##Inf ##-Inf ##NaN` | reader | 3 | read as `f64-inf`, `(neg f64-inf)`, `f64-nan` (E14); types §2.12 has no literal, so the reader desugars |
| `#?(:clj ..)` | adapt | `#?(:fib x :default y)` | reader | 5 | reader conditionals with the feature `:fib` and `:default` (E14) |
| `#inst "..."` | adapt | `#tag form` | reader | 5 | `#tag form` reads as `(tag form)`; a date type is a time library (tail) |
| `defn-` | alias | `(defn- name [x: T ..] -> R body)` | macro | 1 | `defn` with the `:private` qualifier (a Rust macro with `defn`, §6.3) |
| `declare` | adapt | `(declare name ..)` | macro | 2 | a no-op: every name is bound before any body is checked (syntax §3.1) |
| `defonce` | alias | `(defonce name v)` | macro | 2 | `def`: a `def` is evaluated once before `main` (L15) |
| `if-some` | adapt | `(if-some [x e] a b)` | macro `(Option a)` | 2 | the presence test: `(match e ((some x) a) (_ b))`; differs from `if-let` only for an `(Option bool)` holding `false` |
| `when-some` | adapt | `(when-some [x e] body ..)` | macro | 2 | as `if-some`; value `unit` or `(Option T)` as `when` |
| `when-first` | adapt | `(when-first [x c] body ..)` | macro | 2 | `(when-let (x (first c)) body ..)` |
| `letfn` | adapt | `(letfn [(f [x] ..) (g [y] ..)] body)` | macro | 3 | a group of mutually recursive local functions; ships with the shared closure environment of C8 (a macro over cells works and leaks a cycle per entry, `audit: clean=false leak-cycles=4`, [R] A11 t55) |
| `binding` | adapt | `(binding [*x* e] body)` | macro | 5 | rebinds a dynamic var for the dynamic extent of the body (§2.11, C11) |
| `with-bindings` | adapt | `(with-bindings {v e} body)` | macro | 5 | as `binding` over a map of vars |
| `bound-fn` | adapt | `(bound-fn [x] ..)` | macro | 5 | a closure that carries the current bindings into a task |
| `with-redefs` | adapt | `(with-redefs [f g] body)` | macro | 5 | swaps the slot of a function declared `:redefinable` (§2.11); only such a function pays an indirect call |
| `locking` | adapt | `(locking m (fn (v) ..))` | `(Mutex a) (fn (a) r) -> r` | 5 | a `Mutex` that owns its value (§5 M6, C9); Clojure's lock on an arbitrary object would let two tasks share a `Cell` |
| `with-open` | adapt | `(with-open [r e] body)` | macro | 5 | a macro over the scope-exit hook (§7 L12) |
| `with-out-str` | adapt | `(with-out-str body ..)` | macro `-> str` | 5 | binds `*out*` to a string-building writer (§2.11) |
| `with-in-str` | adapt | `(with-in-str s body ..)` | macro | 5 | binds `*in*` (§2.11) |
| `with-local-vars` | adapt | `(with-local-vars [x 1] ..)` | macro | 3 | a `cell` per name; `var-get` is `@x`, `var-set` is `set!` |
| `defmulti` | adapt | `(defmulti name dispatch-fn)` | macro | 5 | a `(Multi d a r)`: a global `Atom` of methods and the dispatch function; `d`, `a`, `r` fixed per multimethod (§2.11) |
| `defmethod` | adapt | `(defmethod name dv [x] ..)` | macro | 5 | registers a method in the module init |
| `extend-type` | adapt | `(extend-type T P (m [this] ..))` | macro | 3 | expands to `impl` (a global fact, syntax §3.10) |
| `extend-protocol` | adapt | `(extend-protocol P T1 (m ..) T2 (m ..))` | macro | 3 | several `impl`s |
| `reify` | adapt | `(reify P (m [this] ..))` | macro | 3 | an anonymous struct, an `impl` and a `(dyn P)` value |
| `defrecord` | adapt | `(defrecord Name [f: T ..])` | macro | 2 | `defstruct` that derives `Eq Ord Hash Show Debug ToStr` (Q22 settled); a struct is not a map: no extra keys, no `dissoc` of a field (§5 T4); `(assoc r :k v)` and `(:k r)` by L21 |
| `deftype` | adapt | `(deftype Name [f: T ..])` | macro | 3 | `defstruct` with `(Cell T)` fields for mutable state (syntax §3.7) |
| `lazy-cat` | adapt | `(lazy-cat c ..)` | macro `-> (LSeq e)` | 2 | each collection expression delayed: a macro over `lazy-seq` and `concat` |
| `require` | adapt | `(require '[x :as y])` | top-level form | 4 | hoisted into the module's `ns` clause at expansion time; `ns` accepts `:refer` and `:only` |
| `use` | adapt | `(use ..)` | top-level form | 4 | hoisted into the `:use` clause |
| `refer` | adapt | `(refer ..)` | top-level form | 4 | hoisted into `:refer` |
| `refer-clojure` | adapt | `(refer-clojure ..)` | top-level form | 4 | `(:refer-clojure :exclude [map])`: the prelude is always used, a local definition shadows a `:use`d name, so the clause only silences the shadowing |
| `alias` | adapt | `(alias ..)` | top-level form | 4 | hoisted into `:as` |
| `macroexpand` | adapt | `(macroexpand form)` | `Form -> Form` | 5 | a function at macro time, a library over `Form` once the expander is a library (the compiler has `fibc expand`) |
| `macroexpand-1` | adapt | `(macroexpand-1 form)` | `Form -> Form` | 5 | as `macroexpand` |
| `apply` | adapt | `(apply f xs)` | arity-reading form `(fn (A..) R) c -> R` | 3 | with `f: (fn (A B) R)` it is `(f (nth v 0) (nth v 1))` after a check that the count is 2 (a trap where Clojure throws `ArityException`); the variadic folds `(apply + xs)`, `(apply max xs)`, `(apply str xs)`, `(apply concat xss)`, `(apply merge ms)` are a table of named folds; a literal vector spreads ([R] A11 e12: 6, 3, 6, `312`); the first version omitted `apply` for "dynamic arity", but the arity is in the function's type (L22) |
| `derive` | adapt | `(derive child parent)` | macro | 5 | over keywords it builds a hierarchy held in an `Atom` for `defmulti`; `(derive Eq P)` over a protocol and a type is the existing type form, told apart by the first argument's kind; `isa?` and `parents` read the hierarchy |
| `isa?` | adapt | `(isa? child parent)` | `k k -> bool` | 5 | over the hierarchy of `derive`; not a run-time type test (§5 T3) |
| `->Name` | adapt | `(->Point x y)` | function | 2 | the positional constructor `(Point x y)` as a function value, so `(map ->Point xs ys)` works; `fibc` says `unsupported: a constructor as a value` while `fibref` returns 2 ([R] A11 t86), a stage-1 divergence (§7 B5) |
| `map->Name` | adapt | `(map->Point m)` | `(Map keyword v) -> Name` | 3 | from a `(Map keyword v)` when every field has type `v`; a missing key traps; heterogeneous fields go through `Val` (§5 T4) |
| `pcalls` | adapt | `(pcalls f g ..)` | macro `(fn () r) .. -> (Vec r)` | 3 | thunks of one result type, run as tasks (`plet`, syntax §3.12) |
| `pvalues` | adapt | `(pvalues e1 e2 ..)` | macro `r ... -> (Vec r)` | 3 | as `pcalls` over expressions |

### 4.3 `fib.core` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `str` | adapt | `(str a ..)` | macro `a ... -> str \| ToStr a` | 1 | concatenates the `to-str` of the arguments: a string as itself, a collection as its `pr` text, `nil` as the empty text (§2.7); `(str)` is `""`; a Rust prelude macro (§6.3) with a function twin |
| `subs` | adapt | `(subs s a b) (subs s a)` | `str i64 i64 -> str; str i64 -> str` | 1 | **CHARACTER offsets** (Unicode scalars), pairing with `count` and `str/index-of` as Clojure's do; never splits a character; O(n) until the ASCII flag (C10); the byte layer is `str-slice` (§2.9) |
| `name` | adapt | `(name k)` | `keyword -> str` | 4 | keywords are flat; `(name :a/b)` is `b` |
| `keyword` | adapt | `(keyword s) (keyword ns s)` | `str -> keyword; str str -> keyword` | 4 | interns at run time |
| `gensym` | keep | `(gensym) (gensym prefix)` | `-> str; str -> str` | 1 | builtin; macro time only (syntax §3.16); the zero-argument form is not there today: `gensym takes 1 argument(s), got 0` ([R] A11 gensym), so it is a clause by L1 |
| `char` | adapt | `(char n)` | `t -> char \| ToChar t` | 2 | checked: traps on a non-scalar; `i32` and `i64` |
| `parse-long` | adapt | `(parse-long s)` | `str -> (Option i64)` | 1 | `nil` on any malformed input, never a trap |
| `parse-double` | adapt | `(parse-double s)` | `str -> (Option f64)` | 3 | correctly rounded through `strtod`, after the grammar of §2.9 is checked in fibber: hex, a leading space, `inf` and trailing junk give `nil` under both tools ([R] A10 pd) |
| `parse-boolean` | adapt | `(parse-boolean s)` | `str -> (Option bool)` | 3 |  |
| `Integer/parseInt` | adapt | `(Integer/parseInt s) (Integer/parseInt s radix)` | `str -> (Option i32); str i64 -> (Option i32)` | 3 | module `Integer`; an `Option` where Java throws `NumberFormatException` (§2.10); `parse-i32` is the unqualified alias |
| `Long/parseLong` | adapt | `(Long/parseLong s) (Long/parseLong s radix)` | `str -> (Option i64); str i64 -> (Option i64)` | 3 | module `Long`; an `Option`; `parse-i64` is the unqualified alias |
| `Double/parseDouble` | adapt | `(Double/parseDouble s)` | `str -> (Option f64)` | 3 | module `Double`; an `Option` |
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
| `rand` | adapt | `(rand) (rand n)` | `-> f64; f64 -> f64` | 4 | the global generator, a top-level atom (§2.8, L15); `(rng/rand r)` takes an explicit `Rng` |
| `rand-int` | adapt | `(rand-int n)` | `i64 -> i64` | 4 | as `rand`; `(rng/rand-int r n)` |
| `zero?` | keep | `(zero? x)` | `t -> bool \| Eq t, Unit t` | 1 |  |
| `pos?` | keep | `(pos? x)` | `t -> bool \| Ord t, Unit t` | 1 |  |
| `neg?` | keep | `(neg? x)` | `t -> bool \| Ord t, Unit t` | 1 |  |
| `even?` | keep | `(even? n)` | `t -> bool \| Bits t, Unit t` | 1 | any integer width |
| `odd?` | keep | `(odd? n)` | `t -> bool \| Bits t, Unit t` | 1 |  |
| `NaN?` | keep | `(NaN? x)` | `f64 -> bool` | 3 |  |
| `infinite?` | keep | `(infinite? x)` | `f64 -> bool` | 3 |  |
| `Long/MAX_VALUE` | alias | `Long/MAX_VALUE` | `i64` | 1 | module `Long` ([R] A11 e19); `i64-max` is the same `def`; also `i64-min`, `i32-max`, `i32-min`, ... |
| `Long/MIN_VALUE` | alias | `Long/MIN_VALUE` | `i64` | 1 | module `Long`; `i64-min` |
| `Double/MAX_VALUE` | alias | `Double/MAX_VALUE` | `f64` | 3 | module `Double`; `f64-max`; also `f64-epsilon`, `f64-min-positive`, `f64-inf`, `f64-nan` |
| `identical?` | adapt | `(identical? a b)` | `a a -> bool` | 3 | pointer equality on objects, value equality on scalars (scalars have no identity); observes sharing, never equality; `same?` is not offered |
| `compare` | adapt | `(compare a b)` | `t t -> i64 \| Ord t` | 1 | -1, 0 or 1, **built on `<`** as Clojure's: a NaN compares equal to everything; vectors compare by length first (§2.7, §9 Q33); one static type |
| `hash` | keep | `(hash x)` | `t -> i64 \| Hash t` | 1 | builtin method; instances for collections are library; `f64` hashes `-0.0` as `0.0` (§2.7) |
| `hash-combine` | adapt | `(hash-combine h x)` | `i64 i64 -> i64` | 1 | rotate and xor from `shl shr bit-or bit-xor`: never traps, exists today ([R] A10 hash); a multiplicative mixer replaces it with L10 (T3) |
| `hash-ordered-coll` | keep | `(hash-ordered-coll c)` | `c -> i64 \| Reducible c e, Hash e` | 1 | for `Vec` and `List`, over `hash-combine` |
| `hash-unordered-coll` | keep | `(hash-unordered-coll c)` | `c -> i64 \| Reducible c e, Hash e` | 1 | commutative (an xor of the elements' hashes); for `Map` and `Set` |
| `mix-collection-hash` | keep | `(mix-collection-hash h n)` | `i64 i64 -> i64` | 3 | the finaliser |
| `nil?` | keep | `(nil? o)` | `(Option a) -> bool` | 1 |  |
| `some?` | keep | `(some? o)` | `(Option a) -> bool` | 1 |  |
| `identity` | keep | `(identity x)` | `a -> a` | 1 |  |
| `distinct?` | adapt | `(distinct? c)` | `c -> bool \| Reducible c e, Hash e, Eq e` | 3 | varargs in Clojure: a macro nests `(distinct? a b c)`; the collection form is the base |
| `defn` | adapt | `(defn name doc? [x: T ..] -> R body)` | macro | 1 | a Rust macro over `defun` (§6.3): bracket parameters, docstring; multi-arity in Clojure's shape `(defn name ([x: T] -> R body) ([x: T y: T] -> R body))`, each clause with its own result type, by §7 L1 (the parameter vector tells a clause from a single-arity body, so no `:arity` marker is needed, [R] A11 e10: `defn`, `let*`, `loop*` with `[a 1 b 2]` as macros over `defun`/`let`/`loop` run, 13 under both tools); `defun` stays |
| `36rZZ 2r1010` | keep | `2r1010 36rZZ` | reader | 3 | radix literals (E14); `0x1F` and `0b1010` stay |
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
| `namespace` | adapt | `(namespace k)` | `keyword -> (Option str)` | 4 | the part before the `/` of a flat keyword, `nil` when there is none |
| `symbol` | adapt | `(symbol s) (symbol ns s)` | `str -> Form` | 4 | a `Sym` of `fib.syntax`: symbols exist as `Form`s, not as a run-time type of their own |
| `find-keyword` | adapt | `(find-keyword s)` | `str -> (Option keyword)` | 4 | a lookup in the intern table; `keyword` interns |
| `true?` | adapt | `(true? x)` | `bool -> bool; (Option bool) -> bool` | 3 | `(= x true)` / `(= o (some true))` |
| `false?` | adapt | `(false? x)` | `bool -> bool` | 3 | `(not x)`; `nil` is not `false`, as Clojure's |
| `boolean` | adapt | `(boolean x)` | `r -> bool \| Truthy r` | 3 | the truthiness of a `bool` or `(Option T)` (§2.4) |
| `pos-int?` | adapt | `(pos-int? n)` | `t -> bool \| Int t` | 3 | `(pos? n)` on an integer type; `neg-int?` and `nat-int?` likewise; a float is a compile error, as every type predicate |
| `neg-int?` | adapt | `(neg-int? n)` | `t -> bool \| Int t` | 3 | `(neg? n)` on an integer type |
| `nat-int?` | adapt | `(nat-int? n)` | `t -> bool \| Int t` | 3 | `(not (neg? n))` on an integer type |
| `unchecked-add-int` | alias | `(unchecked-add-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-subtract-int` | alias | `(unchecked-subtract-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-multiply-int` | alias | `(unchecked-multiply-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-negate-int` | alias | `(unchecked-negate-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-inc-int` | alias | `(unchecked-inc-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-dec-int` | alias | `(unchecked-dec-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-divide-int` | alias | `(unchecked-divide-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-remainder-int` | alias | `(unchecked-remainder-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-byte` | alias | `(unchecked-byte x)` | `t -> u` | 3 | `(trunc i8 x)`; keeps the low bits and never traps |
| `unchecked-short` | alias | `(unchecked-short x)` | `t -> u` | 3 | `(trunc i16 x)`; keeps the low bits and never traps |
| `unchecked-int` | alias | `(unchecked-int x)` | `t -> u` | 3 | `(trunc i32 x)`; keeps the low bits and never traps |
| `unchecked-long` | alias | `(unchecked-long x)` | `t -> u` | 3 | `(fptosi i64 x) or (sext i64 x)`; keeps the low bits and never traps |
| `unchecked-float` | alias | `(unchecked-float x)` | `t -> u` | 3 | `(fptrunc f32 x)`; keeps the low bits and never traps |
| `unchecked-double` | alias | `(unchecked-double x)` | `t -> u` | 3 | `(fpext f64 x)`; keeps the low bits and never traps |
| `unchecked-char` | alias | `(unchecked-char x)` | `t -> u` | 3 | `(i32->char n)`; keeps the low bits and never traps; traps on a non-scalar (a `char` is a scalar, not 16 bits) |
| `+'` | adapt | `(+' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `-'` | adapt | `(-' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `*'` | adapt | `(*' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `inc'` | adapt | `(inc' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `dec'` | adapt | `(dec' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `bigint` | adapt | `(bigint ..)` | `t -> BigInt` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `bigdec` | adapt | `(bigdec ..)` | `t -> BigDecimal` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `biginteger` | adapt | `(biginteger ..)` | `t -> BigInt` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `rationalize` | adapt | `(rationalize ..)` | `f64 -> Ratio` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `numerator` | adapt | `(numerator ..)` | `Ratio -> BigInt` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `denominator` | adapt | `(denominator ..)` | `Ratio -> BigInt` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `with-precision` | adapt | `(with-precision ..)` | macro | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `parse-uuid` | adapt | `(parse-uuid s)` | `str -> (Option Uuid)` | 5 | a `Uuid` struct; `random-uuid` takes the global generator |
| `random-uuid` | adapt | `(random-uuid)` | `-> Uuid` | 5 | from the global generator |
| `char-escape-string` | adapt | `char-escape-string` | `(Map char str)` | 5 | a constant `def` |
| `char-name-string` | adapt | `char-name-string` | `(Map char str)` | 5 | a constant `def` |
| `ex-info` | adapt | `(ex-info msg data) (ex-info msg data cause)` | `str (Map keyword Val) -> ExInfo` | 3 | plain data, usable as the error of a `Result` (§2.10) |
| `ex-data` | adapt | `(ex-data e)` | `ExInfo -> (Map keyword Val)` | 3 | the fields of the error |
| `ex-message` | adapt | `(ex-message e)` | `ExInfo -> str` | 3 | the message |
| `ex-cause` | adapt | `(ex-cause e)` | `ExInfo -> (Option ExInfo)` | 3 | the cause |
| `Throwable->map` | adapt | `(Throwable->map e)` | `ExInfo -> (Map keyword Val)` | 5 | no stack trace (§2.10) |
| `throw` | adapt | `(throw e)` | core form | 5 | `trap` of `(ex-message e)` until unwinding lands; `try`, `catch`, `finally` need §7 L28 (§9 Q35) |
| `catch` | adapt | `(catch T e ..)` | clause of `try` | 5 | see `try` |
| `finally` | adapt | `(finally ..)` | clause of `try` | 5 | memory is released by the scope exits the ownership checker already computes; non-memory cleanup is the scope-exit hook (§7 L12) |
| `try` | adapt | `(try body (catch T e ..) (finally ..))` | core form | 5 | needs unwinding (§2.10, L28, §9 Q35); a `Result` and `try-let` are the forms that run today |
| `print-method` | adapt | `(defmethod print-method T ..)` | `impl Show`, `impl Debug` | 2 | an `impl Show` and `impl Debug` for the type is the static form of the extension point |
| `realized?` | adapt | `(realized? x)` | `(LSeq a) -> bool` | 2 | an `LSeq`, `Delay`, `Promise` or task has a forced flag; an adaptor has no cell and is a compile error |
| `meta` | adapt | `(meta ..)` | `s -> (Option (Map keyword Val))` | 5 | the metadata of a collection or struct (§2.11) |
| `with-meta` | adapt | `(with-meta ..)` | `s (Map keyword Val) -> s` | 5 | a copy with the metadata; `Eq` and `Hash` ignore it |
| `vary-meta` | adapt | `(vary-meta ..)` | `s (fn (m) m) -> s` | 5 | `(with-meta x (f (meta x)))` |
| `alter-meta!` | adapt | `(alter-meta! ..)` | `(Ref a) (fn (m) m) -> m` | 5 | on a `Ref`/`Atom` |
| `reset-meta!` | adapt | `(reset-meta! ..)` | `(Ref a) m -> m` | 5 | on a `Ref`/`Atom` |
| `inst-ms` | adapt | `(inst-ms t)` | `Inst -> i64` | 5 | with a date type, a time library (tail) |

### 4.4 `fib.seq` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `seq` | adapt | `(seq c)` | `c -> (Option s) \| Seqable c s` | 2 | `nil` when empty, else the closed seq type `s` of the collection (a `Slice` for a `Vec`, §2.1 rule 8): `(if (seq xs) ..)` works because an `Option` is a condition (L20), and `(if-let (s (seq xs)) ..)` runs today; the page's first version said `(if (seq xs) ..)` is a type error |
| `vec` | keep | `(vec c)` | `c -> (Vec e) \| Reducible c e` | 1 | the `to-vec` method: a `Vec` argument is returned as is (no allocation, [R] A10 nth), any other source builds into one buffer |
| `vector` | adapt | `(vector a b ..)` | macro `a ... -> (Vec a)` | 2 | the literal `[a b ..]`; as a function value `(map vector xs ys)` is `[x y]`, a `Pair` when the types differ (§7 L25), a `Vec` when they unify |
| `vector-of` | adapt | `(vector-of :i32 1 2)` | macro | 3 | a `(Vec i32)` with the suffixed literals; `(Vec i64)` is already unboxed, so the keyword only selects the width |
| `list` | keep | `(list a ..)` | macro `a ... -> (List a)` | 1 | prelude macro, kept; expands to `Cons`/`Empty` |
| `list*` | adapt | `(list* a .. l)` | macro `a ... (List a) -> (List a)` | 3 | nested `cons`; the last argument is a `List` |
| `cons` | adapt | `(cons x c)` | `a c -> (Consed c a) \| Reducible c a` | 2 | any collection, a recipe that holds the head, as Clojure's `cons` returns a seq ([R] A11 t80: `(cons 0 [1 2 3])` has the elements 0 1 2 3); `(list ..)` and the `Cons` variant of `List` keep their names (§5 row 22 of the first version: `Empty`, `Cons`) |
| `hash-map` | adapt | `{k v ..} or (hash-map k v ..)` | macro `k v ... -> (Map k v) \| Hash k, Eq k` | 2 | an odd count is a compile error; duplicate literal keys are an error (§7 E5); a `Map` of at most 8 entries keeps insertion order, as Clojure's array map does (§2.7) |
| `hash-set` | adapt | `#{a ..} or (hash-set a ..)` | macro `a ... -> (Set a) \| Hash a, Eq a` | 2 | the reader reads `#{..}` as `(hash-set ..)` (§7 E8) |
| `range` | adapt | `(range n) (range a b) (range a b s)` | `t -> (Range t); t t -> (Range t); t t t -> (Range t) \| Num t, Ord t` | 1 | overloaded by arity (§7 L1); integers exact; **floats accumulate as Clojure's do**: `(range 0.0 1.0 0.1)` adds the step to the previous element, 11 elements, the last `0.9999999999999999` ([R] A11 range; `frange` is the `a + i*s` form, 10 elements); a step of 0 repeats the start forever (an infinite recipe); `(range)` is `(iterate inc 0)` |
| `repeat` | adapt | `(repeat x) (repeat n x)` | `a -> (Repeat a); i64 a -> (Taken (Repeat a) a)` | 2 | overloaded by arity |
| `repeatedly` | adapt | `(repeatedly f) (repeatedly n f)` | `(fn () a) -> (LSeq a); i64 (fn () a) -> (LSeq a)` | 3 | memoised: `f` runs once per element across traversals, as Clojure's (§2.1 rule 2) |
| `iterate` | keep | `(iterate f x)` | `(fn (a) a) a -> (Iterate a)` | 2 | infinite source; a consumer that stops ends it |
| `cycle` | keep | `(cycle c)` | `c -> (Cycle c e) \| Reducible c e` | 2 | an empty source ends at once |
| `lazy-seq` | adapt | `(lazy-seq body)` | macro `(LSeq a) -> (Lazy a)` | 2 | the memoising sequence `LSeq`: a head and a `Lazy` cell forced once (§2.1, [R] A11 t21: 5 calls, sum 10 twice); the classic recursive producers need it, so it moves from tranche 5 to 2 |
| `concat` | adapt | `(concat a b ..)` | `c1 c2 -> (Cat c1 c2 e) \| Reducible c1 e, Reducible c2 e` | 1 | two-arity function; macro nests for more |
| `interleave` | adapt | `(interleave a b ..)` | `c1 c2 -> (Interleaved c1 c2 e) \| Cursable c1 k1, Cursor k1 e, Cursable c2 k2, Cursor k2 e` | 2 | truncates to the shorter; every operand is `Cursable` (adaptors by buffering, §2.1 rule 5, in any order); `zip-strict` traps on a mismatch |
| `interpose` | keep | `(interpose sep c)` | `e c -> (Interposed c e) \| Reducible c e` | 2 |  |
| `tree-seq` | adapt | `(tree-seq branch? children root)` | `(fn (n) bool) (fn (n) d) n -> (TreeSeq n d) \| Reducible d n` | 3 | typed over one node type; depth first, no explicit stack in the caller |
| `re-seq` | adapt | `(re-seq re s)` | `Regex str -> (Matches)` | 4 | `Reducible Match`; `fib.regex` |
| `line-seq` | adapt | `(line-seq r)` | `Reader -> (LSeq str)` | 5 | memoised `LSeq`; needs a scope-exit hook to close the handle (§7 L12); `fib.io`; `lines-of` is not offered |
| `iteration` | adapt | `(iteration step init)` | `(fn (k) (Option (Pair v k))) k -> (LSeq v)` | 5 | memoised `LSeq`; `fib.seq` |
| `first` | adapt | `(first c)` | `c -> (Option e) \| Reducible c e` | 1 | `nil` on empty; use `(unwrap (first c))` or `(nth c 0)` for the sure case |
| `ffirst` | adapt | `(ffirst c)` | `c -> (Option e) \| Reducible c d, Reducible d e` | 2 | `(and-then first (first c))`; the first version said it "needs nested sequential element types": two `Reducible` constraints do it ([R] A11 t60) |
| `nfirst` | adapt | `(nfirst c)` | `c -> (Option s) \| Reducible c d, Seqable d s` | 3 | `(next (first c))` |
| `second` | adapt | `(second c)` | `c -> (Option e) \| Reducible c e` | 2 |  |
| `fnext` | alias | `(fnext c)` | `c -> (Option e)` | 2 | alias of `second` |
| `last` | adapt | `(last c)` | `c -> (Option e) \| Reducible c e` | 1 | the `last` method: a walk by default, O(1) on `Vec` ([R] A10 nth) and, by the same override in the instance, on `Array Range Slice` |
| `butlast` | adapt | `(butlast c)` | `c -> (DroppedLast c e) \| Reducible c e` | 3 | empty when short, where Clojure has `nil` (§5 T1); `(seq (butlast c))` is the `Option` |
| `rest` | adapt | `(rest c)` | `c -> s \| Seqable c s` | 2 | `Seqable.rest`: the closed seq type, so recursion on `rest` is not polymorphic recursion (§2.1 rule 8); `(drop 1 c)` is the recipe form, and recursion on it over a generic `c` is rejected by both tools (§7 B4) |
| `next` | adapt | `(next c)` | `c -> (Option s) \| Seqable c s, Seqable s s` | 2 | `(seq (rest c))` ([R] A11 seq1: `(len [5 6 7 8])` is 4 with it) |
| `nnext` | adapt | `(nnext c)` | `c -> (Option s)` | 3 | `(next (next c))` |
| `nthnext` | adapt | `(nthnext n c)` | `i64 c -> (Option s)` | 3 | `(next ..)` n times; `nil` when exhausted |
| `nthrest` | alias | `(nthrest c n)` | `c i64 -> (Dropped c e)` | 3 | alias of `drop` with Clojure's argument order |
| `nth` | adapt | `(nth c i) (nth c i d)` | `c i64 -> e \| Reducible c e; c i64 e -> e` | 1 | the `nth` method: O(1) on `Vec` ([R] A10 nth) and, by the same override, on `Array Range Slice`, a walk on any other source, so `(nth (filter p c) 3)` works (A10 nth); traps out of range as Clojure throws; the 3-arity is `nth-or` until L1 |
| `take` | keep | `(take n c)` | `i64 c -> (Taken c e) \| Reducible c e` | 1 | stops the source at n |
| `take-while` | keep | `(take-while p c)` | `(fn (e) r) c -> (TakenWhile c e) \| Reducible c e, Truthy r` | 1 |  |
| `take-nth` | adapt | `(take-nth n c)` | `i64 c -> (TakenNth c e) \| Reducible c e` | 3 | `(take-nth 0 c)` repeats the first element forever, as Clojure's (replicated, §9 Q33) |
| `take-last` | adapt | `(take-last n c)` | `i64 c -> (Vec e) \| Reducible c e` | 3 | materialises |
| `drop` | keep | `(drop n c)` | `i64 c -> (Dropped c e) \| Reducible c e` | 1 |  |
| `drop-while` | keep | `(drop-while p c)` | `(fn (e) r) c -> (DroppedWhile c e) \| Reducible c e, Truthy r` | 2 |  |
| `drop-last` | adapt | `(drop-last c) (drop-last n c)` | `c -> (DroppedLast c e); i64 c -> (DroppedLast c e) \| Reducible c e` | 3 | overloaded by arity; holds n elements back |
| `split-at` | adapt | `(split-at n c)` | `i64 c -> (Pair (Vec e) (Vec e)) \| Reducible c e` | 3 | one pass |
| `splitv-at` | alias | `(splitv-at n c)` | `i64 c -> (Pair (Vec e) (Vec e))` | 3 | alias of `split-at` (which returns `Vec`s) |
| `split-with` | adapt | `(split-with p c)` | `(fn (e) r) c -> (Pair (Vec e) (Vec e)) \| Reducible c e, Truthy r` | 3 | one pass; `p` runs once per element up to and including the first failure |
| `subvec` | adapt | `(subvec v a b) (subvec v a)` | `(Vec e) i64 i64 -> (Slice e)` | 3 | a `Slice`: an O(1) view holding a count on `v`, `Reducible`, `Lookup`, `Assoc`, `Collection`, `Stack` and `Keyed`, so `(conj (subvec v 0 2) x)` works as Clojure's; `(vec (subvec ..))` copies; indexes checked at the call |
| `peek` | adapt | `(peek s)` | `s -> (Option e) \| Stack s e` | 2 | the end `conj` adds at: a `Vec`'s last, a `List`'s first |
| `pop` | adapt | `(pop s)` | `s -> s \| Stack s e` | 2 | traps when empty; in place when unique |
| `rseq` | adapt | `(rseq c)` | `c -> (Reversed c e) \| Reversible c e` | 3 | `Vec`, `Range`, sorted collections |
| `count` | adapt | `(count c)` | `c -> i64 \| Reducible c e` | 1 | `size`: O(1) for `Vec Map Set Array Range Option Slice` (and `str` with the ASCII flag, C10), a walk for every adaptor, so **`(count (map f c))` calls `f`** as Clojure's does ([R] A11 count); characters on a `str` (§2.9) |
| `bounded-count` | keep | `(bounded-count n c)` | `i64 c -> i64 \| Reducible c e` | 3 | stops after n elements |
| `empty?` | adapt | `(empty? c)` | `c -> bool \| Reducible c e` | 1 | stops at the first element the recipe produces (§2.1 rule 4) |
| `not-empty` | adapt | `(not-empty c)` | `c -> (Option c) \| Reducible c e` | 2 | same as `seq` |
| `map` | adapt | `(map f c) (map f c1 c2) (map f c1 c2 c3)` | `(fn (e) b) c -> (Mapped c e b) \| Reducible c e; (fn (a b) r) c1 c2 -> (ZipWith c1 c2 a b k r) \| Reducible c1 a, Cursable c2 k, Cursor k b` | 1 | overloaded by arity (L1; `zip-with` until then); returns a re-runnable recipe, not a `Vec`; the second operand is any source (adaptors by buffering, §2.1 rule 5); `(map f)` is the transducer |
| `mapv` | adapt | `(mapv f c)` | `(fn (e) b) c -> (Vec b) \| Reducible c e` | 2 | alias of `(vec (map f c))` |
| `mapcat` | adapt | `(mapcat f c)` | `(fn (e) d) c -> (Mapcat c e d b) \| Reducible c e, Reducible d b` | 1 | flattening is free in the push model |
| `map-indexed` | adapt | `(map-indexed f c)` | `(fn (i64 e) b) c -> (MapIndexed c e b) \| Reducible c e` | 2 |  |
| `keep` | adapt | `(keep f c)` | `(fn (e) (Option b)) c -> (Kept c e b) \| Reducible c e` | 2 | `f` returns an `Option`; `nil`s are dropped, `some`s unwrapped |
| `keep-indexed` | adapt | `(keep-indexed f c)` | `(fn (i64 e) (Option b)) c -> (KeptIndexed c e b) \| Reducible c e` | 3 |  |
| `reverse` | adapt | `(reverse c)` | `c -> (Vec e) \| Reducible c e` | 1 | eager; `rseq` is the O(1) view |
| `sort` | adapt | `(sort c) (sort cmp c)` | `c -> (Vec e) \| Reducible c e, Ord e; (fn (e e) r) c -> (Vec e) \| Reducible c e, Cmp r` | 1 | stable merge sort; **the comparator is any function whose result is a `Cmp`**: an `i64` or Clojure's predicate form, so `(sort < xs)` and `(sort > xs)` work ([R] A11 t17, e4); `(sort-with cmp c)` is the 2-arity until L1; sorting a `Vec` copies it once (the `vec` inside is the identity), 2096 objects for 1000 elements against 4190 before ([R] A10 sort) |
| `sort-by` | adapt | `(sort-by key c) (sort-by key cmp c)` | `(fn (e) k) c -> (Vec e) \| Reducible c e, Ord k; (fn (e) k) (fn (k k) r) c -> (Vec e) \| Reducible c e, Cmp r` | 1 | the key is computed once per element (decorate, sort, undecorate: 5 calls for 5 elements, [R] A10 sortby); `(sort-by val > m)` is Clojure's text; `sort-by-with` is the 3-arity until L1 |
| `sort-by-with` (new) | new | `(sort-by-with key cmp c)` | `(fn (e) k) (fn (k k) r) c -> (Vec e) \| Reducible c e, Cmp r` | 1 | the 3-arity of `sort-by` until L1, then deleted; stable; `key` is stored, so it is not `:borrow` |
| `shuffle` | adapt | `(shuffle c)` | `c -> (Vec e) \| Reducible c e` | 4 | the global generator (§2.8); `(rng/shuffle r c)` takes an `Rng` |
| `distinct` | keep | `(distinct c)` | `c -> (Distinct c e) \| Reducible c e, Hash e, Eq e` | 2 | first occurrences, in order |
| `dedupe` | keep | `(dedupe c)` | `c -> (Deduped c e) \| Reducible c e, Eq e` | 3 |  |
| `replace` | adapt | `(replace smap c)` | `s c -> (Replaced c e) \| Lookup s e e, Reducible c e` | 3 | any `Lookup` as the substitution map, a `(Vec e)` included (index to value), as Clojure's |
| `partition` | adapt | `(partition n c) (partition n step c) (partition n step pad c)` | `i64 c -> (Partitioned c e); i64 i64 c -> (Partitioned c e); i64 i64 d c -> (Partitioned c e) \| Reducible c e, Reducible d e` | 2 | yields `(Vec e)`; drops an incomplete last group, as Clojure's does (`partition-all` keeps it) |
| `partitionv` | alias | `(partitionv n c)` | as `partition` | 3 | alias of `partition` (yields `Vec`s) |
| `partition-all` | adapt | `(partition-all n c) (partition-all n step c)` | `i64 c -> (Partitioned c e); i64 i64 c -> (Partitioned c e) \| Reducible c e` | 2 | yields `(Vec e)` |
| `partitionv-all` | alias | `(partitionv-all n c)` | as `partition-all` | 3 | alias of `partition-all` |
| `partition-by` | keep | `(partition-by f c)` | `(fn (e) k) c -> (PartitionedBy c e k) \| Reducible c e, Eq k` | 2 | yields `(Vec e)` |
| `group-by` | adapt | `(group-by f c)` | `(fn (e) k) c -> (Map k (Vec e)) \| Reducible c e, Hash k, Eq k` | 1 | one pass; each group in encounter order |
| `frequencies` | keep | `(frequencies c)` | `c -> (Map e i64) \| Reducible c e, Hash e, Eq e` | 1 |  |
| `zipmap` | keep | `(zipmap ks vs)` | `c1 c2 -> (Map k v) \| Reducible c1 k, Cursable c2 k2, Cursor k2 v, Hash k, Eq k` | 2 | truncates to the shorter |
| `reduce` | adapt | `(reduce f init c) (reduce f c)` | `(fn (a e) a) a c -> a \| Reducible c e; (fn (e e) e) c -> e \| Reducible c e` | 1 | the 2-arity returns `e` and traps `reduce: empty collection` on an empty `c`, except for the literal heads `+ * str conj merge concat` whose identity a macro table knows (Clojure calls `(f)`); `reduce1` is the `Option` form; `(reduced x)` inside a literal `fn` stops the reduction (§2.1 rule 3) |
| `reduce-kv` | keep | `(reduce-kv f init m)` | `(fn (a k v) a) a m -> a \| KeyReducible m k v` | 2 | `Map` (entries) and `Vec` (index, element); collection last (§3 N3); the walk passes `k` and `v` as two arguments, so it builds no `Pair` |
| `reductions` | adapt | `(reductions f init c) (reductions f c)` | `(fn (a e) a) a c -> (Reductions c e a) \| Reducible c e` | 2 | emits `init` first; the 2-arity seeds with the first element |
| `transduce` | adapt | `(transduce xf f init c)` | `Xf (fn (a b) a) a c -> a` | 3 | transducers are values of `(Xf a b)`, a factory of steppers with a flush (§2.1); `comp` composes them after C1, `xf` until then |
| `xf` (new) | new | `(xf t1 t2 ..)` | macro `(Xf a b) (Xf b c) -> (Xf a c)` | 3 | composes transducers left to right, `(xf-comp x y)` nested; `comp` composes them after C1 (§5 T9) |
| `run!` | keep | `(run! f c)` | `(fn (e) r) c -> unit \| Reducible c e` | 1 | replaces the prelude's `for-each`; `f` may return any `r`, which is dropped (monomorphised, no cost): `(run! (fn (x) (swap! a + x)) xs)` is 6 under both tools ([R] A10 run); over a literal `(range a b)` and a literal one-parameter `fn` it expands to the counting loop that the prelude's `for-each` macro makes today (§6.3) |
| `dorun` | alias | `(dorun c)` | `c -> unit \| Reducible c e` | 2 | consumes for effect: `run!` with no function |
| `doall` | alias | `(doall c)` | `c -> (Vec e) \| Reducible c e` | 2 | alias of `vec`: materialises, and memoises an effectful recipe |
| `some` | adapt | `(some pred c)` | `(fn (e) r) c -> (Option x) \| Reducible c e, Truthy r, Payload r x` | 2 | the first truthy value of `(pred x)` (§2.4, [R] A11 `some`: `(some true)`, `(some 20)`, `nil`); `some` is also `Option`'s constructor, so one name by L1 extended to a constructor or a Rust macro that picks by argument count (A11 n13); `find-map` and `find-first` are the library's extras, which stage 2 uses in tranche 1 |
| `every?` | keep | `(every? p c)` | `(fn (e) r) c -> bool \| Reducible c e, Truthy r` | 1 |  |
| `not-any?` | keep | `(not-any? p c)` | `(fn (e) r) c -> bool \| Reducible c e, Truthy r` | 1 | `none?` is not offered |
| `not-every?` | keep | `(not-every? p c)` | `(fn (e) r) c -> bool \| Reducible c e, Truthy r` | 2 |  |
| `filter` | adapt | `(filter p c)` | `(fn (e) r) c -> (Filtered c e) \| Reducible c e, Truthy r` | 1 | `p` returns a `Truthy` (`bool` or `Option`): `(filter :parent nodes)`, `(remove nil? xs)` ([R] A11 t05) |
| `filterv` | adapt | `(filterv p c)` | `(fn (e) r) c -> (Vec e) \| Reducible c e, Truthy r` | 2 | alias of `(vec (filter p c))` |
| `remove` | keep | `(remove p c)` | `(fn (e) r) c -> (Filtered c e) \| Reducible c e, Truthy r` | 1 |  |
| `max-key` | adapt | `(max-key k c) (max-key k x y ..)` | `(fn (e) k) c -> (Option e) \| Reducible c e, Ord k` | 2 | ties: the last wins, as Clojure's; the collection form is the base and a macro nests the varargs text |
| `min-key` | adapt | `(min-key k c) (min-key k x y ..)` | `(fn (e) k) c -> (Option e) \| Reducible c e, Ord k` | 2 | ties: the last wins, as Clojure's; as `max-key` |
| `rand-nth` | adapt | `(rand-nth c)` | `c -> e \| Reducible c e` | 4 | the global generator, traps on an empty source as Clojure throws; `(rng/rand-nth r c)` |
| `random-sample` | adapt | `(random-sample p c)` | `f64 c -> (Sampled c e) \| Reducible c e` | 4 | the global generator; `(rng/random-sample r p c)` |
| `sequence` | adapt | `(sequence xf c)` | `Xf c -> (XfApplied xf c)` | 5 |  |
| `eduction` | adapt | `(eduction xf c)` | `Xf c -> (XfApplied xf c)` | 5 |  |
| `any?` | keep | `(any? x)` | `a -> bool` | 1 | Clojure's `any?`: a one-argument predicate that is always true (the default spec predicate); the quantifier is `some` (the first version made `(any? p c)` the quantifier, which silently changes a ported program) |
| `reduced` | adapt | `(reduced x)` | `a -> (Step a)` | 1 | `(Done x)`; `reduce-while` reads it, and so does plain `reduce` over a literal `fn` (§2.1 rule 3) |
| `find-first` (new) | new | `(find-first p c)` | `(fn (e) bool) c -> (Option e) \| Reducible c e` | 1 | the value form of `(first (filter p c))`; not a Clojure name; stage 2 uses it before `some` exists |
| `zip` (new) | new | `(zip a b)` | `c1 c2 -> (Zipped c1 c2) \| Reducible c1 x, Cursable c2 k, Cursor k y` | 2 | yields `(Pair x y)` (one object per element until C5); truncates to the shorter; the second operand is any source, adaptors by buffering (§2.1 rule 5) |
| `zip-with` (new) | new | `(zip-with f a b)` | `(fn (x y) r) c1 c2 -> (ZipWith ..)` | 2 | the 3-arity of `map` until L1, then deleted; no `Pair` per element: +4 objects over 1000 elements ([R] A10 alloc) |
| `seq-of` (new) | new | `(seq-of c)` | `c -> (dyn (Reducible e)) \| Reducible c e` | 2 | the type-erased source for a join and for a field of unknown source type: `(if flag (seq-of (filter p v)) (seq-of v))` runs ([R] A10 join); L24 inserts it at a join automatically; one heap object and an indirect call per visit |
| `seq=` (new) | new | `(seq= a b)` | `c1 c2 -> bool \| Reducible c1 e, Reducible c2 e, Eq e` | 2 | equality across `Reducible`s: `(seq= (map inc [1 2]) [2 3])` is true ([R] A10 showseq); the checker elaborates `=` to it when the operand types are of one family (§2.7, L24) |
| `zip-strict` (new) | new | `(zip-strict a b)` | `c1 c2 -> (Zipped c1 c2)` | 3 | traps on a length mismatch |
| `sum` (new) | new | `(sum c)` | `c -> e \| Reducible c e, Num e, Unit e` | 1 | `(reduce + 0 c)`; needs a static `zero` for an empty generic `e` (§7 L4); `i64` until then |
| `product` (new) | new | `(product c)` | `c -> e \| Reducible c e, Num e, Unit e` | 2 |  |
| `reduce-while` (new) | new | `(reduce-while f init c)` | `(fn (a e) (Step a)) a c -> a \| Reducible c e` | 1 | Clojure's `reduced`; boxed `Step` allocates per step until §7 C5 |
| `frange` (new) | new | `(frange a b s)` | `f64 f64 f64 -> (FRange)` | 3 | the non-accumulating `a + i*s` for `i < ceil((b-a)/s)`; an extra: `range` over floats is Clojure's accumulating form; step 0 traps |
| `nth-or` (new) | new | `(nth-or c i d)` | `c i64 e -> e \| Reducible c e` | 1 | the 3-arity of `nth` until L1, then deleted |
| `reduce1` (new) | new | `(reduce1 f c)` | `(fn (e e) e) c -> (Option e) \| Reducible c e` | 1 | the 2-arity of `reduce` until L1, then deleted; `(unwrap (reduce1 max xs))` is Clojure's `(apply max xs)` |
| `sort-with` (new) | new | `(sort-with cmp c)` | `(fn (e e) r) c -> (Vec e) \| Reducible c e, Cmp r` | 1 | the comparator form of `sort` (until L1, then deleted); stable; a `Cmp` result, so a predicate works |
| `range-by` (new) | new | `(range-by a b s)` | `i64 i64 i64 -> Range` | 1 | the 3-arity of `range` until L1, then deleted; `(range a b)` is today's macro |
| `flatten` | adapt | `(flatten c)` | `c -> (Flattened c d e) \| Reducible c d, Reducible d e` | 3 | one level of nesting for a source of sources; arbitrary depth over a recursive enum (`Val`, a `Tree`) by a `Flatten` protocol; Clojure's heterogeneous nesting is `[1 [2 [3]]]`, which is `cannot unify i64 with (Vec ..)` ([R] A11 t91 for the analogous heterogeneous vector, §5 T4) |
| `transient` | alias | `(transient ..)` | `(Vec a) -> (Vec a)` | 3 | the identity over the unique in-place update (P3); also for `Map` and `Set` |
| `persistent!` | alias | `(persistent! ..)` | `(Vec a) -> (Vec a)` | 3 | the identity |
| `conj!` | alias | `(conj! ..)` | `(Vec a) a -> (Vec a)` | 3 | `conj`: `(persistent! (conj! (transient v) x))` is `(conj v x)` ([R] A11 t60: `[1 2]`) |
| `assoc!` | alias | `(assoc! ..)` | `s k v -> s \| Assoc s k v` | 3 | `assoc` |
| `dissoc!` | alias | `(dissoc! ..)` | `s k -> s \| Dissoc s k` | 3 | `dissoc` |
| `disj!` | alias | `(disj! ..)` | `s k -> s \| Dissoc s k` | 3 | `disj` |
| `pop!` | alias | `(pop! ..)` | `s -> s \| Stack s e` | 3 | `pop` |
| `reduced?` | adapt | `(reduced? s)` | `(Step a) -> bool` | 3 | a function over `Step` (a `match` is the pattern form); `unreduced` and `ensure-reduced` likewise |
| `unreduced` | adapt | `(unreduced s)` | `(Step a) -> a` | 3 | the payload |
| `ensure-reduced` | adapt | `(ensure-reduced s)` | `(Step a) -> (Step a)` | 3 | `Done` of the payload unless it is one |
| `replicate` | alias | `(replicate n x)` | `i64 a -> (Taken (Repeat a) a)` | 3 | alias of `(repeat n x)`; deprecated in Clojure and still there |
| `file-seq` | adapt | `(file-seq path)` | `str -> (LSeq str)` | 5 | a memoised recipe of paths; needs a directory-listing primitive (`fib.io`) |

### 4.5 `fib.coll` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `find` | adapt | `(find m k)` | `s k -> (Option (Pair k v)) \| Lookup s k v` | 2 | returns the entry, a `Pair` |
| `select-keys` | keep | `(select-keys m ks)` | `s c -> s \| Lookup s k v, Assoc s k v, Emptyable s, Reducible c k` | 1 | absent keys are skipped |
| `conj` | adapt | `(conj c x ..)` | `c e -> c \| Collection c e` | 1 | one rule per type: `List` front, `Vec` end, `Set` anywhere, `Map` takes a `Pair` (a two-element `Vec` after L23); a literal `nil` first argument is `(list)` (§2.4); macro for more than one `x` |
| `assoc` | adapt | `(assoc m k v ..)` | `s k v -> s \| Assoc s k v` | 1 | `Map` and `Vec` (index at most the count: `i = count` appends, as Clojure's, and beyond it traps, [R] A11 e6); a struct by a literal keyword is `with` (L21); a literal `nil` is `{}` (§2.4); macro for more pairs |
| `dissoc` | keep | `(dissoc m k ..)` | `s k -> s \| Dissoc s k` | 1 | `Map` and `Set`; macro for more keys |
| `get` | adapt | `(get c k) (get c k d)` | `s k -> (Option v) \| Lookup s k v; s k v -> v \| Lookup s k v` | 1 | absent and `nil`-valued differ by construction; overloaded by arity |
| `get-in` | adapt | `(get-in m [k1 k2 ..]) (get-in m [k1 ..] d)` | macro `m k1 .. -> (Option v); m k1 .. v -> v` | 3 | the path is a literal vector, one `get` per level, so each level has its own key type; a path computed at run time has no static type over nested maps of different value types (§5 T4) and is a function over `Val` ([R] A11 e17: `(get-in-v v [:inner :x])` is `(some 1)`, `[:inner :nope]` `nil`) |
| `assoc-in` | adapt | `(assoc-in m [k1 k2 ..] v)` | macro `m k1 .. v -> m \| Lookup, Assoc, Emptyable per level` | 3 | a missing level is `(empty ..)` of the level's type; needs static `empty` (§7 L4) |
| `update` | adapt | `(update m k f)` | `s k (fn (v) v) -> s \| Lookup s k v, Assoc s k v` | 1 | a macro: `f` sees the value, a missing key traps `update: no key` (Clojure's `(update m :n inc)` throws an NPE); `(update m k f x ..)` passes extra arguments as Clojure's does; a literal `(fnil g d)` in the `f` position is routed to `update-or` ([R] A11 t24: 6, 107, 15); an `f` that is not a literal uses `update-or` or `update-opt` |
| `update-or` (new) | new | `(update-or m k f d)` | `s k (fn (v) v) v -> s \| Lookup s k v, Assoc s k v` | 1 | `f` sees `d` when the key is missing: Clojure's `(update m w (fnil inc 0))` for an `f` that is not a literal `fnil`; stays when L1 lands (the 4-arity of `update` is Clojure's extra-argument form) |
| `update-opt` (new) | new | `(update-opt m k f)` | `s k (fn ((Option v)) v) -> s \| Lookup s k v, Assoc s k v` | 1 | `f` sees `(Option v)` as Clojure's sees `nil`: `(update-opt m w (fnil inc 0))` is the literal port |
| `update-in` | adapt | `(update-in m [k1 ..] f)` | macro `m k1 .. (fn (v) v) -> m` | 3 | typed per level like `get-in`; `update` at each level, so a missing key traps; `assoc-in` creates levels; the empty path is a compile error |
| `update-keys` | adapt | `(update-keys m f)` | `(Map k v) (fn (k) k2) -> (Map k2 v) \| Hash k2, Eq k2` | 3 | two keys mapping to one: **the last wins**, as Clojure's (replicated, §9 Q33) |
| `update-vals` | keep | `(update-vals m f)` | `(Map k v) (fn (v) w) -> (Map k w) \| Hash k, Eq k` | 3 | the value type may change |
| `contains?` | keep | `(contains? c k)` | `s k -> bool \| Keyed s k` | 1 | `Map` and `Set` (key, member) and `Vec`, `Slice` (the **index**, as Clojure's, [R] A11 e6); `includes?` is the element test |
| `array-map` | alias | `(array-map k v ..)` | `k v ... -> (Map k v)` | 2 | alias of `hash-map`: a `Map` of at most 8 entries keeps insertion order (§2.7) |
| `into` | adapt | `(into to c) (into to xf c)` | `t c -> t \| Collection t e, Reducible c e` | 1 | the 3-arity takes a transducer (tranche 3); a bulk path builds in one buffer; `(into {} [[1 2]])` needs L23 (a two-element `Vec` entry, §5 T6); `(into {} (zip ks vs))` works |
| `empty` | adapt | `(empty c)` | `s -> s \| Emptyable s` | 2 | `Vec`, `Map`, `Set`, `List`; takes a value, so no static method is needed |
| `merge` | adapt | `(merge m1 m2 ..)` | `(Map k v) (Map k v) -> (Map k v) \| Hash k, Eq k` | 1 | a literal `nil` operand is skipped (§2.4); an `Option`-typed operand after L16; `(merge)` has no type; macro nests for more |
| `merge-with` | adapt | `(merge-with f m1 m2 ..)` | `(fn (v v) v) (Map k v) (Map k v) -> (Map k v) \| Hash k, Eq k` | 2 | `f` combines a conflict (old, new) |
| `keys` | adapt | `(keys m)` | `m -> (Mapped m (Pair k v) k) \| Reducible m (Pair k v)` | 1 | an empty recipe, not `nil` |
| `vals` | adapt | `(vals m)` | `m -> (Mapped m (Pair k v) v) \| Reducible m (Pair k v)` | 1 |  |
| `key` | keep | `(key p)` | `(Pair k v) -> k` | 1 | the map element is a `Pair`; `key` and `val` read it (a function, not a field: `(. p fst)`) |
| `val` | keep | `(val p)` | `(Pair k v) -> v` | 1 |  |
| `disj` | keep | `(disj s x ..)` | `s k -> s \| Dissoc s k` | 1 |  |
| `set` | keep | `(set c)` | `c -> (Set e) \| Reducible c e, Hash e, Eq e` | 1 |  |
| `includes?` (new) | new | `(includes? x c)` | `e c -> bool \| Reducible c e, Eq e` | 1 | an extra for `->>`; Clojure's idiom is `(some #{x} c)`, which runs once sets are callable (L21) |
| `Slice` (new) | new | `(Slice e)` | struct `(v: (Vec e) lo: i64 hi: i64)` | 3 | the O(1) view `subvec` returns; `Reducible` (with an O(1) `nth`), `Lookup`, `Assoc`, `Collection`, `Stack`, `Keyed` |
| `get-or` (new) | new | `(get-or c k d)` | `s k v -> v \| Lookup s k v` | 1 | the 3-arity of `get` until L1 (§7), then deleted; `(get m k 0)` is `get takes 2 argument(s), got 3` until then (the error says `use get-or`, §7 D1) |

### 4.6 `fib.sorted`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `subseq` | adapt | `(subseq sc test key)` | `s (fn (i64 i64) bool) k -> (Range) \| Sorted s k v` | 5 | `test` is `<`, `<=`, `>` or `>=` as a function value, as Clojure's; the collection applies `(test (compare ek key) 0)`, so no `Cmp` enum is needed |
| `rsubseq` | adapt | `(rsubseq sc test key)` | `s (fn (i64 i64) bool) k -> (Reversed ..) \| Sorted s k v` | 5 |  |
| `sorted-map` | adapt | `(sorted-map k v ..)` | macro `k v ... -> (SortedMap k v) \| Ord k` | 3 | a B-tree; one key type, so no run-time failure |
| `sorted-map-by` | keep | `(sorted-map-by cmp k v ..)` | `(fn (k k) r) k v ... -> (SortedMap k v) \| Cmp r` | 3 | `cmp` is any `Cmp` function: `<`, `compare`, a predicate; equal under the comparator means the same key |
| `sorted-set` | adapt | `(sorted-set a ..)` | macro `a ... -> (SortedSet a) \| Ord a` | 3 | a B-tree |
| `sorted-set-by` | keep | `(sorted-set-by cmp a ..)` | `(fn (a a) r) a ... -> (SortedSet a) \| Cmp r` | 3 | "equal under the comparator" means the same element: `(sorted-set-by (fn [a b] (< (count a) (count b))) "ab" "cd")` has one element, as Clojure's |
| `comparator` | adapt | `(comparator less?)` | `(fn (a a) bool) -> (fn (a a) i64)` | 3 | converts a predicate to an `i64` comparator, as Clojure's; a predicate is also a comparator without it (§2.7) |
| `clojure.lang.PersistentQueue/EMPTY` | adapt | `(queue)` | `-> (Queue a)` | 3 | `conj` at the back, `peek` and `pop` at the front; `Collection`, `Stack`, `Reducible`; `PersistentQueue/EMPTY` is the `def` of that module for ported code |

### 4.7 `fib.string` (alias `str`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.string/blank?` | keep | `(str/blank? s)` | `str -> bool` | 1 | empty or whitespace only |
| `clojure.string/capitalize` | keep | `(str/capitalize s)` | `str -> str` | 4 |  |
| `clojure.string/ends-with?` | keep | `(str/ends-with? s suffix)` | `str str -> bool` | 1 |  |
| `clojure.string/escape` | adapt | `(str/escape s f)` | `str (fn (char) (Option str)) -> str` | 4 | `f` is a function, or any `Lookup` such as a `(Map char str)` once maps are callable (L21) |
| `clojure.string/includes?` | keep | `(str/includes? s sub)` | `str str -> bool` | 1 |  |
| `clojure.string/index-of` | adapt | `(str/index-of s sub) (str/index-of s sub from)` | `str str -> (Option i64); str str i64 -> (Option i64)` | 1 | **character offset** (§2.9); `(Option i64)`, never -1; the value is a `str` or a `char` (`Pattern`) |
| `clojure.string/join` | adapt | `(str/join c) (str/join sep c)` | `c -> str \| Reducible c e, Show e; str c -> str \| Reducible c e, Show e` | 1 | always qualified: an unqualified `join` is the task-wait builtin (§3 N9) |
| `clojure.string/last-index-of` | adapt | `(str/last-index-of s sub)` | `str str -> (Option i64)` | 3 | character offset |
| `clojure.string/lower-case` | keep | `(str/lower-case s)` | `str -> str` | 4 | Unicode full case mapping, locale independent |
| `clojure.string/replace` | adapt | `(str/replace s from to)` | `str p str -> str \| Pattern p` | 3 | the match is a `str`, a `char` or a `Regex` (`Pattern`); a `str` replacement, with `$1` interpreted for a regex match and not for a string, as Clojure's; a function replacement is `str/replace-with` until L23 |
| `clojure.string/replace-first` | adapt | `(str/replace-first s from to)` | `str p str -> str \| Pattern p` | 3 | as `str/replace` |
| `clojure.string/reverse` | adapt | `(str/reverse s)` | `str -> str` | 4 | by Unicode scalar value, not by grapheme |
| `clojure.string/split` | adapt | `(str/split s re) (str/split s re limit)` | `str p -> (Vec str) \| Pattern p; str p i64 -> (Vec str) \| Pattern p` | 1 | `re` is a `Regex` (`#","`) or, as a typed superset, a `str` separator; **drops trailing empty strings** as Clojure's (Java's `split`); a `limit` of -1 keeps them |
| `clojure.string/split-lines` | keep | `(str/split-lines s)` | `str -> (Vec str)` | 1 | splits at LF and CRLF |
| `clojure.string/starts-with?` | keep | `(str/starts-with? s prefix)` | `str str -> bool` | 1 | the prelude builtin `starts-with?`, re-exported: one function with two names for the 2 cases that use it, not two implementations |
| `clojure.string/trim` | keep | `(str/trim s)` | `str -> str` | 1 | Unicode whitespace |
| `clojure.string/trim-newline` | keep | `(str/trim-newline s)` | `str -> str` | 3 |  |
| `clojure.string/triml` | keep | `(str/triml s)` | `str -> str` | 1 |  |
| `clojure.string/trimr` | keep | `(str/trimr s)` | `str -> str` | 1 |  |
| `clojure.string/upper-case` | keep | `(str/upper-case s)` | `str -> str` | 4 | Unicode full case mapping |
| `chars` (new) | new | `(str/chars s)` | `str -> Chars` | 1 | the explicit `Reducible char` view, a decoder over one byte array (2 objects for 1000 characters, [R] A10 chars); a `str` is itself a `Reducible char` (§2.9) |
| `str-len` (new) | new | `(str-len s)` | `str -> i64` | 1 | builtin: bytes, O(1) |
| `str-byte-at` (new) | new | `(str-byte-at s i)` | `str i64 -> i8` | 1 | builtin (§7 L18): one byte, no allocation, traps out of range; `str-bytes` allocates an array per call |
| `str-find` (new) | new | `(str-find s pat from)` | `str str i64 -> (Option i64)` | 1 | builtin (§7 L18): the byte offset of the first match at or after `from`, no allocation; `str/index-of` is built on it |
| `replace-with` (new) | new | `(str/replace-with s re f)` | `str Regex (fn (Match) str) -> str` | 4 | the function replacement, `(fn (Match) str)`, until L23 lets `str/replace` dispatch on the replacement's type |
| `clojure.string/re-quote-replacement` | adapt | `(str/re-quote-replacement s)` | `str -> str` | 4 | escapes `$` and `\` for a regex replacement template |

### 4.8 `fib.char`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `Character/isDigit` | adapt | `(Character/isDigit c)` | `char -> bool` | 4 | module `Character`; Unicode `Nd`, as Java's; the ASCII-only test the compiler uses is `digit?` (new, tranche 1) |
| `Character/isLetter` | adapt | `(Character/isLetter c)` | `char -> bool` | 4 | module `Character`; Unicode letters (needs a table); alias `letter?` |
| `Character/isLetterOrDigit` | adapt | `(Character/isLetterOrDigit c)` | `char -> bool` | 4 | module `Character`; alias `alphanumeric?` |
| `Character/isWhitespace` | adapt | `(Character/isWhitespace c)` | `char -> bool` | 4 | module `Character`; Java's definition (no no-break spaces); `whitespace?` (new, tranche 1) is the Unicode `White_Space` property the compiler uses |
| `Character/isUpperCase` | adapt | `(Character/isUpperCase c)` | `char -> bool` | 4 | module `Character`; alias `upper?` |
| `Character/isLowerCase` | adapt | `(Character/isLowerCase c)` | `char -> bool` | 4 | module `Character`; alias `lower?` |
| `Character/toUpperCase` | adapt | `(Character/toUpperCase c)` | `char -> char` | 4 | module `Character`; the simple one-to-one mapping; alias `upper` |
| `Character/toLowerCase` | adapt | `(Character/toLowerCase c)` | `char -> char` | 4 | module `Character`; alias `lower` |
| `Character/digit` | adapt | `(Character/digit c radix)` | `char i64 -> (Option i64)` | 3 | `(Option i64)` where Java returns -1; alias `digit-value` |
| `Character/forDigit` | adapt | `(Character/forDigit n radix)` | `i64 i64 -> (Option char)` | 3 | `(Option char)` where Java returns NUL; alias `digit-char` |
| `digit?` | new | `(digit? c)` | `char -> bool` | 1 | ASCII digits; what the compiler's tokenizer needs |
| `whitespace?` | new | `(whitespace? c)` | `char -> bool` | 1 | the Unicode `White_Space` property |

### 4.9 `fib.regex`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `re-pattern` | adapt | `(regex s)` | `str -> (Result Regex str)` | 4 | a non-backtracking engine for the patterns that fit it, so those cannot hang a call, and a backtracking matcher with a step budget for backreferences and lookaround (tranche 5): Java's regex has both and no safety reason forbids them |
| `#"regex"` | adapt | `(re "..")` | macro `str -> Regex` | 4 | reads as `(re "regex")`, checked at read/expand time (E14); `(re "..")` is the macro form |
| `re-find` | adapt | `(re-find re s)` | `Regex str -> (Option Match)` | 4 | `Match` has `whole`, `groups` (`(Vec (Option str))`) and byte offsets, whatever the pattern: Clojure's result is a string without groups and a vector with them, a result type that depends on the pattern (§5 T5) |
| `re-matches` | adapt | `(re-matches re s)` | `Regex str -> (Option Match)` | 4 | the whole string must match |
| `re-matcher` | adapt | `(re-matcher re s)` | `Regex str -> Matcher` | 5 | a struct with `Cell` state, single-threaded; `(re-find m)` advances it |
| `re-groups` | adapt | `(re-groups m)` | `Matcher -> (Option Match)` | 5 | the groups of the matcher's last match |

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
| `clojure.math/random` | adapt | `(rand rng)` | `Rng -> f64` | 4 | the global generator (§2.8) |
| `clojure.math/add-exact` | alias | `(math/add-exact ..)` | `t t -> t \| Num t` | 3 | alias of `+`, which already traps on overflow |
| `clojure.math/subtract-exact` | alias | `(math/subtract-exact ..)` | `t t -> t \| Num t` | 3 | alias of `-`, which already traps on overflow |
| `clojure.math/multiply-exact` | alias | `(math/multiply-exact ..)` | `t t -> t \| Num t` | 3 | alias of `*`, which already traps on overflow |
| `clojure.math/negate-exact` | alias | `(math/negate-exact ..)` | `t -> t \| Num t` | 3 | alias of `neg`, which already traps on overflow |
| `clojure.math/increment-exact` | alias | `(math/increment-exact ..)` | `t -> t \| Num t` | 3 | alias of `inc`, which already traps on overflow |
| `clojure.math/decrement-exact` | alias | `(math/decrement-exact ..)` | `t -> t \| Num t` | 3 | alias of `dec`, which already traps on overflow |
| `Math/sqrt` | alias | `(Math/sqrt ..)` | as `math/sqrt` | 3 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/pow` | alias | `(Math/pow ..)` | as `math/pow` | 4 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/abs` | alias | `(Math/abs ..)` | as `abs` | 1 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/floor` | alias | `(Math/floor ..)` | as `math/floor` | 3 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/ceil` | alias | `(Math/ceil ..)` | as `math/ceil` | 3 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/round` | alias | `(Math/round ..)` | as `math/round` | 3 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/random` | alias | `(Math/random ..)` | as `math/random` | 4 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/log` | alias | `(Math/log ..)` | as `math/log` | 4 | the module `Math` (§4.16, [R] A11 e19) |

### 4.11 `fib.set` (alias `set`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.set/union` | adapt | `(set/union a b)` | `(Set e) (Set e) -> (Set e) \| Hash e, Eq e` | 3 | the larger set is extended; macro nests for more |
| `clojure.set/intersection` | adapt | `(set/intersection a b)` | `(Set e) (Set e) -> (Set e) \| Hash e, Eq e` | 3 |  |
| `clojure.set/difference` | adapt | `(set/difference a b)` | `(Set e) (Set e) -> (Set e) \| Hash e, Eq e` | 3 |  |
| `clojure.set/select` | adapt | `(set/select p s)` | `(fn (e) bool) (Set e) -> (Set e) \| Hash e, Eq e` | 3 | `(set (filter p s))` is the general form |
| `clojure.set/project` | adapt | `(set/project xrel ks)` | `(Set (Map k v)) c -> (Set (Map k v)) \| Reducible c k` | 5 | needs `Hash (Map k v)` |
| `clojure.set/rename-keys` | adapt | `(set/rename-keys m kmap)` | `(Map k v) (Map k k) -> (Map k v) \| Hash k, Eq k` | 5 | a collision: **the last wins**, as Clojure's (replicated, §9 Q33) |
| `clojure.set/rename` | adapt | `(set/rename xrel kmap)` | `(Set (Map k v)) (Map k k) -> (Set (Map k v))` | 5 |  |
| `clojure.set/index` | adapt | `(set/index xrel ks)` | `(Set (Map k v)) c -> (Map (Map k v) (Set (Map k v)))` | 5 |  |
| `clojure.set/map-invert` | adapt | `(set/map-invert m)` | `(Map k v) -> (Map v k) \| Hash v, Eq v` | 3 | duplicate values: the last wins, as Clojure's (replicated, §9 Q33) |
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
| `clojure.walk/postwalk-demo` | adapt | `(walk/postwalk-demo form)` | `n -> n \| Walkable n` | 4 | prints each form visited, with `dbg` |
| `clojure.walk/prewalk-demo` | adapt | `(walk/prewalk-demo form)` | `n -> n \| Walkable n` | 4 | as `postwalk-demo` |
| `clojure.walk/macroexpand-all` | adapt | `(walk/macroexpand-all form)` | `Form -> Form` | 5 | as `macroexpand` (§4.2) |

### 4.13 `fib.data` (alias `data`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.data/diff` | adapt | `(data/diff a b)` | `t t -> (Triple (Option t) (Option t) (Option t)) \| Diffable t` | 5 | Clojure's `[only-a only-b both]` as a `Triple` of `Option`s (`nil` for nothing), destructured by `[a b both]` once L3b lands; for `Map`, `Set`, `Vec` |

### 4.14 `fib.print` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `println` | adapt | `(println a ..)` | macro `a ... -> unit \| Show a` | 1 | one `Show` argument or several, separated by a space ([R] A10 pm); in value position it is a function generic over `Show`, as are `str print prn pr` (`(run! println xs)`, `(map str xs)`, [R] A10 twin); writes to `*out*` |
| `print` | adapt | `(print a ..)` | macro `a ... -> unit \| Show a` | 1 | no newline |
| `pr` | adapt | `(pr a ..)` | macro `a ... -> unit \| Debug a` | 2 | strings quoted and escaped at every depth (`Debug`) |
| `prn` | adapt | `(prn a ..)` | macro `a ... -> unit \| Debug a` | 1 | `pr` and a newline |
| `print-str` | adapt | `(print-str a ..)` | macro `a ... -> str \| Show a` | 2 | `print` to a string: the arguments joined with a space (`(print-str 1 2)` is `1 2`, `(str 1 2)` is `12`: the first version mapped it to `str`) |
| `pr-str` | adapt | `(pr-str a ..)` | macro `a ... -> str \| Debug a` | 2 | `pr` to a string; `debug-str` is not offered |
| `flush` | keep | `(flush)` | `-> unit` | 4 | the prelude writes unbuffered, so a no-op until buffering exists |
| `printf` | adapt | `(printf "fmt" a ..)` | macro `-> unit` | 4 | the directives are checked against the argument types at compile time |
| `format` | adapt | `(format "fmt" a ..)` | macro `-> str` | 4 | `%s` over `Show`, `%d` an integer, `%f` a float, `%x`; the format string must be a literal; locale independent |
| `pprint` | adapt | `(pprint x)` | `a -> unit \| Pretty a` | 5 | long tail |
| `read-line` | adapt | `(read-line)` | `-> (Option str)` | 4 | `nil` at end of input; `fib.io` |
| `slurp` | adapt | `(slurp path)` | `str -> (Option str)` | 1 | alias of the builtin `read-file`; an `Option` where Clojure throws on a missing file |
| `spit` | adapt | `(spit path s)` | `str str -> bool` | 1 | alias of the builtin `write-file`, which exists today; `:append true` appends |
| `println-str` | adapt | `(println-str a ..)` | macro `a ... -> str` | 2 | `print-str` and a newline |
| `prn-str` | adapt | `(prn-str a ..)` | macro `a ... -> str \| Debug a` | 2 | `pr-str` and a newline |
| `newline` | alias | `(newline)` | `-> unit` | 1 | writes a newline to `*out*` |
| `print-table` | adapt | `(print-table rows)` | `(Vec (Map keyword v)) -> unit \| Show v` | 5 | over rows of one value type |
| `tap>` | adapt | `(tap> x)` | `(dyn Show) -> bool` | 5 | a global tap list in an `Atom`; `add-tap`, `remove-tap` |
| `add-tap` | adapt | `(add-tap f)` | `(fn :send ((dyn Show)) unit) -> unit` | 5 | registers a tap |
| `remove-tap` | adapt | `(remove-tap f)` | as `add-tap` | 5 | removes a tap |
| `read-string` | adapt | `(read-string s)` | `str -> (Result Form str)` | 4 | the reader of `fib.syntax` (spec/bootstrap.md); `(read-as T s)` through a `Read` protocol for typed data |
| `read` | adapt | `(read r)` | `Reader -> (Result Form str)` | 5 | over a reader value; `read+string` also returns the text read |
| `read+string` | adapt | `(read+string r)` | `Reader -> (Result (Pair Form str) str)` | 5 | as `read` |
| `*out*` | adapt | `*out*` | `(dyn Writer)` | 5 | a dynamic var (§2.11, C11); `println` writes to `*out*` |
| `*in*` | adapt | `*in*` | `(dyn Reader)` | 5 | a dynamic var (§2.11, C11); `println` writes to `*out*` |
| `*print-length*` | adapt | `*print-length*` | `(Option i64)` | 5 | a dynamic var (§2.11, C11); `println` writes to `*out*` |

### 4.15 `fib.async`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `swap-vals!` | adapt | `(swap-vals! a f)` | `(Atom a) (fn (a) a) -> (Pair a a)` | 4 | old and new |
| `reset-vals!` | adapt | `(reset-vals! a v)` | `(Atom a) a -> (Pair a a)` | 4 |  |
| `compare-and-set!` | adapt | `(compare-and-set! a old new)` | `(Atom a) a a -> bool` | 4 | **by identity** for objects (a pointer compare-and-swap, as Clojure's) and by value for scalars; `Eq` is not consulted |
| `delay` | adapt | `(delay e)` | macro `a -> (Delay a)` | 4 | a closure and a `Cell`; forced once; `@d` is `force` |
| `force` | adapt | `(force d)` | `(Delay a) -> a` | 4 |  |
| `promise` | adapt | `(promise)` | `-> (Promise a) \| Send a` | 5 | an `Atom` and a wait; needs a blocking wait primitive |
| `deliver` | adapt | `(deliver p v)` | `(Promise a) a -> bool` | 5 | true when this call delivered |
| `future` | adapt | `(future body ..)` | macro `-> (Task a)` | 1 | a macro over `spawn`; `@f` is `(join f)`; the closure is `:send`, so a `Cell` capture is rejected ([R] A11 t41: `cell cannot be shared between threads: closure capture c has type (Cell i64)`) |
| `future-call` | adapt | `(future-call f)` | `(fn :send () a) -> (Task a)` | 1 | `(spawn f)` |
| `future-done?` | adapt | `(future-done? t)` | `(Task a) -> bool` | 5 | alias `done?` |
| `pmap` | keep | `(pmap f c)` | `(fn :send (a) b) c -> (Vec b) \| Reducible c a` | 1 | eager, one task per element today; chunked over `ncpu + 2` tasks with the order kept in tranche 3 ([R] A11 e15: `[1 4 9 16 25 36 49 64 81 100]`); lazy over an `LSeq` of futures in tranche 5 |
| `seque` | adapt | `(seque n c)` | `i64 c -> (LSeq e) \| Reducible c e` | 5 | a bounded producer/consumer over an `Atom` queue and a task |
| `add-watch` | adapt | `(add-watch r k f)` | `(Ref a) k (fn :send (k a a) unit) -> unit` | 5 | a `Ref` of an `Atom` and a watch list `(Atom (Vec (fn :send ..)))`; runs under both tools ([R] A11 e18: 42, t42: 50) |
| `remove-watch` | adapt | `(remove-watch r k)` | `(Ref a) k -> unit` | 5 | see `add-watch` |
| `set-validator!` | adapt | `(set-validator! r f)` | `(Ref a) (fn :send (a) bool) -> unit` | 5 | a `:send` closure called before the value is stored; a false result traps `Invalid reference state`, as Clojure throws |
| `get-validator` | adapt | `(get-validator r)` | `(Ref a) -> (Option (fn :send (a) bool))` | 5 | see `set-validator!` |
| `future-cancel` | adapt | `(future-cancel t)` | `(Task a) -> bool` | 5 | sets a flag the task polls with `(cancelled?)`, Java's cooperative interrupt; a task is never killed mid-flight, which would skip the releases of everything it owns (§5 M5) |
| `future-cancelled?` | adapt | `(future-cancelled? t)` | `(Task a) -> bool` | 5 | reads the flag |

### 4.16 `fib.sys`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `Thread/sleep` | adapt | `(Thread/sleep ms)` | `i64 -> unit` | 4 | module `Thread`; `(sleep ms)` is the alias |
| `System/currentTimeMillis` | adapt | `(System/currentTimeMillis)` | `-> i64` | 4 | module `System`; `(now-ms)` is the alias |
| `System/nanoTime` | adapt | `(System/nanoTime)` | `-> i64` | 4 | monotonic; `(nano-time)` is the alias |
| `System/getenv` | adapt | `(System/getenv name)` | `str -> (Option str)` | 4 | an `Option` where Java returns null; `(getenv name)` is the alias |
| `System/exit` | adapt | `(System/exit n)` | `i64 -> a` | 4 | or return `n` from `main`; `(exit n)` is the alias |
| `*command-line-args*` | adapt | `*command-line-args*` | `-> (Vec str)` | 1 | a `def` of the builtin `(args)` (L15); the empty `Vec` where Clojure has `nil` |
| `clojure-version` | adapt | `(clojure-version)` | `-> str` | 4 | the library's version string; `(version)` is the alias |

### 4.17 Not offered

The names that Clojure has and this library does not offer, with the reason. None is a memory-safety reason: they are the JVM, no run-time
type information (§5 T3), a result type that depends on a value (§5 T5) or is a union (§5 T8), legacy structs, and the name `defstruct`, which
is fibber's record form. The first version's 282 rows were 192 reversed into the tables above (§10.4.6), 1 duplicate (`#"..."`), and these 89.

| Clojure name | Verdict | Instead | Reason |
|---|---|---|---|
| `iterator-seq` | omit | - | Java objects: no JVM here; fibber's `Reducible` protocol is the native thing |
| `enumeration-seq` | omit | - | Java objects |
| `xml-seq` | omit | - | walks `clojure.xml`'s dynamic map trees and the JVM XML parser (heterogeneous values and the JVM) |
| `resultset-seq` | omit | - | Java objects |
| `map-entry?` | omit | - | a run-time type predicate: the entry is a `Pair`, a distinct struct, and its answer is a static fact (§5 T3) |
| `sorted?` | omit | - | a type, `SortedMap`/`SortedSet` (§5 T3) |
| `struct-map` | omit | - | legacy structs hold values of any type under keyword keys (§5 T4); superseded by records in Clojure itself |
| `struct` | omit | - | legacy; see `struct-map` |
| `create-struct` | omit | - | legacy; see `struct-map` |
| `accessor` | omit | - | legacy; see `struct-map` |
| `defstruct` | omit | - | the name is fibber's record form (syntax §3.7); Clojure's legacy basis is superseded by records |
| `char?` | omit | - | static types: `char` is a type (§5 T3) |
| `num` | omit | - | no boxed `Number` in a typed language (§5 T3) |
| `boolean?` | omit | - | static types (§5 T3) |
| `seq?` | omit | - | static types: `Reducible`/`Seqable` are protocols; the question is a constraint (§5 T3) |
| `coll?` | omit | - | static types: a `Collection` constraint instead |
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
| `decimal?` | omit | - | static types |
| `ratio?` | omit | - | static types |
| `rational?` | omit | - | static types |
| `fn?` | omit | - | static types |
| `ifn?` | omit | - | static types: a callable is a `Fn` constraint (§2.11) |
| `associative?` | omit | - | static types: `Lookup` and `Assoc` constraints |
| `sequential?` | omit | - | static types |
| `counted?` | omit | - | static types: `Reducible.size` is O(1) where the source knows (§2.1) |
| `reversible?` | omit | - | static types: a `Reversible` constraint |
| `indexed?` | omit | - | static types: `Reducible.nth` is O(1) where the source knows (§2.1) |
| `seqable?` | omit | - | static types: a `Reducible` constraint |
| `ident?` | omit | - | static types |
| `simple-ident?` | omit | - | static types |
| `qualified-ident?` | omit | - | static types |
| `simple-keyword?` | omit | - | static types |
| `qualified-keyword?` | omit | - | static types |
| `simple-symbol?` | omit | - | static types |
| `qualified-symbol?` | omit | - | static types |
| `uuid?` | omit | - | static types |
| `inst?` | omit | - | static types |
| `bytes?` | omit | - | static types: an `(Array i8)` is a type |
| `class?` | omit | - | no reflection (§5 T3) |
| `instance?` | omit | - | no run-time type information: a `match` on an enum is the run-time test (§5 T3) |
| `satisfies?` | omit | - | instances are checked at compile time (types §4) |
| `extends?` | omit | - | as `satisfies?` |
| `record?` | omit | - | static types |
| `var?` | omit | - | a `var` is a definition, not an object (§4.2) |
| `special-symbol?` | omit | - | no reflection |
| `reader-conditional?` | omit | - | reader data; static types |
| `tagged-literal?` | omit | - | reader data; static types |
| `chunked-seq?` | omit | - | a JVM implementation detail |
| `type` | omit | - | no reflection (§5 T3) |
| `class` | omit | - | no reflection (§5 T3) |
| `cast` | omit | - | static types |
| `eval` | omit | - | a result type that cannot be known statically, and the compiler inside every binary (§1.2 non-goals); macros run at expansion time on the JIT (syntax §3.16) |
| `import` | omit | - | Java interop |
| `in-ns` | omit | - | REPL namespaces: modules are linked statically by `ns` |
| `proxy` | omit | - | Java interop |
| `definterface` | omit | - | Java interop |
| `gen-class` | omit | - | Java interop |
| `..` | omit | - | Java interop |
| `trampoline` | omit | - | its result is a value or a function, a union type (§5 T8); mutual recursion in one module is a tail call (syntax §2, types §6.10) |
| `memfn` | omit | - | Java interop |
| `volatile?` | omit | - | static types |
| `delay?` | omit | - | static types |
| `future?` | omit | - | static types |
| `ex-triage` | omit | - | Java exception classification |
| `clojure.data/equality-partition` | omit | - | protocol internals; `Diffable` replaces them |
| `clojure.data/diff-similar` | omit | - | protocol internals |
| `load-string` | omit | - | no run-time compiler; as `eval` |
| `load-file` | omit | - | no run-time compiler; as `eval` |
| `destructure` | omit | - | patterns are checked by the compiler, so there is no expansion to call; macro writers use `match` |
| `extend` | omit | - | a map of method functions of different types is heterogeneous (§5 T4); `extend-type` and `extend-protocol` are offered |
| `uri?` | omit | - | no URI type; a static type predicate |
| `test` | omit | - | tests are programs with executable verdicts (spec/method.md) |
| `requiring-resolve` | omit | - | no run-time name resolution |
| `print-simple` | omit | - | printer internals |
| `print-dup` | omit | - | printer internals |
| `reader-conditional` | omit | - | reader data of `#?`; no use at run time |
| `tagged-literal` | omit | - | reader data of `#tag` |
| `munge` | omit | - | compiler internals |

## 5. Deviations from Clojure

The first version of this page had 82 deviation rows. Under the rule (§1.1) a deviation needs a memory-safety failing
program or a static-typing fact the language already decided. Of the 82, 32 had neither and are reversed, 21 are adapted to
a typed twin that keeps Clojure's text (some as the language change of §7 that makes them Clojure's), 11 stand on a decided
typing fact (group T below), and 18 were already Clojure's behaviour (§10.4.2 gives every row). The memory-safety group M is new:
the first version gave one memory-safety reason, for `add-watch`, and it was false (§4.15). What remains is below, in five
groups: **M** memory safety, **T** decided static typing, **D** decisions the owner made before the rule, **C** consequences of
the owner's decisions under the rule, and **S** differences that exist only until an item of §7 lands (a stage limit, not a
deviation of the design).

### 5.1 Memory safety (M): the failing program

| # | Clojure | Here | The failing program, or why none can be run |
|---|---|---|---|
| M1 | any `def` value may be a mutable cell (`(def c (volatile! 0))`), shared by every thread | `def` may hold an `Atom`, never a `Cell` or `Weak` (§2.11) | a `Cell` in a top-level `def` is reachable from every task without being captured, so two tasks write one `Cell` and the capture check does not see it. The check that exists rejects the closure form, `cell cannot be shared between threads: closure capture c has type (Cell i64)` ([R] A11 t41), and `(def c: (Cell i64) (cell 0))` is `def c: initialiser is not a constant expression` (n8); with L15 the rule becomes "the type of a `def` contains no `Cell`/`Weak`". An `Atom` is `Send` and safe: `(def a: (Atom i64) (atom 0))` is rejected today only by the same grammar (n8b) |
| M2 | `(aset a i x)` mutates the array that every holder sees | `(aset &a i x)` has value semantics: a write is in place only when `a` is unique, else it copies | `(let ((a (cell (array 3 0))) (b @a)) (array-set! &a 0 9) (+ (array-get @a 0) (* 10 (array-get b 0))))` is 9 here and 99 in Clojure ([R] A11 t54): the persistent collections share `(Array a)` nodes, so an aliased write would change a vector's value under its holder, and across tasks it is a data race. Clojure's text `(aset a 0 9)` becomes `(aset &a 0 9)` |
| M3 | `(make-array T n)` and `(object-array n)` fill with `nil` or a zero | scalars default to zero and `(Option a)` to `nil`; an object type needs the element: `(array n x)` | an uninitialised slot of an object type is a null pointer read as an object. `(array 3)` is `array takes 2 argument(s), got 1` ([R] A11 t90); only an unsafe uninitialised array (§7 C4) could exhibit it, and it is for the library |
| M4 | a string counts UTF-16 units: `(count "😀")` is 2 and `(subs s 0 1)` may cut a surrogate pair | a character is a Unicode scalar: `(count "😀")` is 1; the two agree on the Basic Multilingual Plane | a `subs` through a surrogate pair would have to build a `str` that is not valid UTF-8, whose lead byte then claims continuation bytes that are not there, and the decoder's bounds rest on validity (§2.9). The invariant is enforced at construction: `str-slice [0, 1) splits a character` (A11 t12), `str-from-bytes: invalid UTF-8` (t13) |
| M5 | `(future-cancel f)` interrupts the thread | sets a flag the task polls with `(cancelled?)` (Java's interrupt is cooperative too) | an asynchronous kill stops a thread between two instructions of a retain, a release or a heap update and leaves a count or a structure inconsistent. Not constructed as a program (there is no cancel primitive); the weaker form of the argument is the ownership model's no-leak promise (ownership.md §2) |
| M6 | `(locking x body)` locks any object's monitor | `(locking m (fn (v) ..))` on a `Mutex a` that owns its value (§2.11) | the monitor of an arbitrary object would let two tasks hold one `Cell`: the program of M1 and t41. Rust's `Mutex<T>` is the form where Clojure's would break safety, which is the rule's second clause |

### 5.2 Static typing already decided (T): and the typed twin that keeps Clojure's text

| # | Clojure | Here | The decided fact, the program that shows it, the twin |
|---|---|---|---|
| T1 | `nil` is a value of every type | `nil` is `Option`'s empty variant (types §1.5): `first last peek get find max-key` and `parse-long`, `index-of` return `(Option v)`; `keys` of an empty map is an empty recipe | `(conj nil x)` is `no implementation of Collection for (Option a)` ([R] A11 n7): the result is a `List` for `nil` and a `Vec` for a vector. Twins: a condition accepts `(Option T)` (§2.4), a literal `nil` first argument of `conj assoc merge` expands (§2.4), `(get (get m :a) :b)` after L16, `some->`. `(first xs)` of a `List` returning `Option` is **Decided** (Q28) |
| T2 | a `fn` literal may have several arities and a rest parameter; `partial`, `comp`, `juxt` return functions of any arity | a closure value has one type with one arity (types §1.4); `(fn ([x] ..) ([x y] ..))` and `%&` are not offered; the combinators are arity-reading checker forms (§2.2, L22) | `(partial add3 1)` for a three-parameter `add3` is `cannot unify (fn :send (i64 i64 i64) i64) with (fn (a b) c)` ([R] A11 arity). Twin: the checker reads the arity from the argument's type; a multi-arity `defn` is overloading of a name (L1) |
| T3 | `vector?`, `seq?`, `instance?`, `type`, `class`, `satisfies?`, `eval` ask a run-time tag | no run-time type information: the answer is a constant of the static type; `eval` has no static result type (§1.2) | a typed `vector?` exists only where an impl does: `no implementation of IsVec for i64` ([R] A11 n16). Twins: a protocol constraint in the signature, a `match` on an enum, a predicate over `Val`, `isa?` over a hierarchy value |
| T4 | a vector or map holds values of any types: `[1 "a"]`, `{:a 1 :b "x"}`; a record is a map with extra keys; `get-in` takes a path computed at run time | one element type per `Vec`, one key and one value type per `Map`; a struct is not a map | `[1 "a"]` is `cannot unify i64 with str` ([R] A11 t91, k8), `{:a 1 :b "x"}` the same (t92). Twins: the entry is a `Pair`/`Triple` (§2.3, L25 for a literal), a map literal of mixed values is `(Map keyword Val)` (L25), `Val` and a runtime `get-in-v` ([R] A11 e17), `defrecord` with `(assoc r :k v)` by L21 |
| T5 | a result type that depends on a value: `(/ 7 2)` is a ratio, `+'` a `Long` or `BigInt`, `re-find` a string or a vector by the group count, `flatten` any nesting | one result type per function: integer `/` truncates (`quot` is its alias), `+'` returns a `BigInt`, `re-find` returns a `Match`, `flatten` is one level or over a recursive enum | `(/ 7 2)` is `3` ([R] A11 k9). Twins: `Ratio` and `BigInt` library types ([R] A11 t50), `Match` with `whole` and `groups`, `Val` |
| T6 | a protocol or multimethod dispatches on every argument | a protocol is dispatched on one head per parameter today (types §4) | two instances of one head overlap: `overlapping instances: OrHit for (Option a) is already implemented` (A11 t70), `... Collection for (Map k v) ...` (t71). It is why `(into {} [[1 2]])`, `or` of an `Option` with a default in predicate position, and `str/replace` by replacement type are not plain library code; L23 lifts it (§7) |
| T7 | `=` across types of one family is true, across families false | `=` has one type (types §2.12); within a family the checker elaborates to `seq=` (§2.7, L24) | `(= [1] #{1})` is a `cannot unify`, which is what Clojure's `false` would hide |
| T8 | an `if` or `or` may return different types in its arms: `(or (even? x) (get m k))`, `(if c 1 "a")`; `trampoline`'s result is a value or a function | no union types: arms unify | `(if c 1 "a")` is `cannot unify i64 with str`. Twins: recipes and closures erase at a join (L24, C1), `seq-of`; `trampoline` is not offered (§4.17) |
| T9 | `(comp (map f) (filter p))` composes transducers, which are functions generic in the accumulator | a transducer is an `Xf` value, a factory of steppers; `xf` composes them, and after C1 `comp` dispatches on `Fn` and `Xf` (§2.1 rule 7) | `let` does not generalise (types §2.4), and an impl head on a function type is rejected: `(comp (xmap inc) (xfilter odd?))` is `cannot unify (Xf i64 i64) with (fn (a) b)` ([R] A10 comp) |

### 5.3 Decisions the owner made before the rule

| # | Clojure | Here | Status |
|---|---|---|---|
| D1 | `throw`, `try`, `catch`, `finally`, `ex-info` | `trap` is the only abort, and a trap in a task ends the process (types §2.11, **Decided** 2026-09-28) | no memory-safety failure prevents exceptions (§2.10); the rule would reverse the decision, and it is the owner's to amend: §9 Q35 |
| D2 | mixed numeric operands promote | no implicit conversion between numeric variables (types §1.1 D3); literals adopt (L19) | a statically typable lattice with no safety content (§2.8); §9 Q36 recommends lifting it |
| D3 | `(abs Long/MIN_VALUE)` is `Long/MIN_VALUE` | overflow traps at every width, `abs` included (types §2.12, **Decided**, §9 Q15) | Clojure's `+` throws on overflow too; this is the one overflow it ignores |

### 5.4 Consequences of the owner's decisions under the rule

| # | Clojure | Here | Why |
|---|---|---|---|
| C1 | `(= 1 1.0)` is `false`, and `(== 1 1.0)` is `true` | `(= 1 1.0)` is `true`: the literal `1` adopts `f64` (L19, **Decided**), so `=` between two numbers is `==` | the owner's L19; `==` is an alias of `=` (§4.1) |
| C2 | the printed order of a map of more than 8 entries is the JVM hash's | the order of this library's hash; changes once with the new integer hash (§9 Q15, **Decided**) | the hash is part of the spec; tests compare with `=` or sort (§2.7) |

### 5.5 Until a §7 item lands (a stage limit, not a deviation of the design)

| # | Clojure | Today | Lands with |
|---|---|---|---|
| S1 | `(if (get m k) ..)`, `(or (get m k) 0)`, `(when c 5)` as a value | `cannot unify (Option i64) with bool` ([R] A11 t01, t02), `cannot unify unit with i64` (t82); the `Truthy` library and macros run (t04, 127) | L20 |
| S2 | `(m k)`, `(#{1 2} x)`, `(v 0)`, `(:k m)`, `(filter #{1 2} xs)` | `cannot unify (Map i64 i64) with (fn (a) b)` (A11 t51, n5), `(Set i64)` with `(fn (a) bool)` (n6), `keyword` (k1), `(Vec i64)` (k2) | L21, or C1's `Fn` |
| S3 | `partial`, `comp`, `constantly`, `juxt`, `apply` of any arity | the one-free `partial`, unary `comp` and `constantly` (A11 arity..arity4) | L22 |
| S4 | `(into {} [[1 2]])`, `(some #(.. (Option bool)) ..)`, `str/replace` with a function | overlapping instances (A11 t70, t71) | L23 |
| S5 | `(if flag (filter p v) v)`, `(= [1] (list 1))` | `cannot unify (Vec i64) with (Filtered (Vec i64) i64)` ([R] A10 join), `cannot unify Range with (Vec i64)` (A11 n4); `seq-of` and `seq=` run | L24 |
| S6 | `(+ x 1)` for `x: i32`, `(+ n 2.5)` for a variable | `cannot unify i64 with i32` (A11 n3c), `cannot unify f64 with i64` (n1) | L26, §9 Q36 |
| S7 | `(get m k 0)`, `(nth c i d)`, `(reduce f c)`, `(sort cmp c)`, `(map f c1 c2)`, `(defn f ([x] ..) ([x y] ..))` | `get takes 2 argument(s), got 3`; the stand-ins `get-or`, `nth-or`, `reduce1`, `sort-with`, `zip-with` run | L1 |
| S8 | `(def counter (atom 0))`, `(def stopwords #{"a" "the"})`, a global `rand` | `def counter: initialiser is not a constant expression` (A11 t56) | L15 |
| S9 | `(throw ..)` caught by `try` | `trap` (A11 t40, e9) | L28, §9 Q35 |
| S10 | a lazy seq crosses a thread | an `LSeq` or `cache` holds a `Cell`: `cell cannot be shared between threads: closure capture s, field box of Cached has type (Cell (Option (Vec i64)))` (A11 e8d); the `Atom` memo crosses (e8e) | C9 (`Mutex`) |
| S11 | `(count "aé€😀z")`, `(subs s 1 3)`, `(map f "abc")` are O(1) or O(n) characters | today `count` of a `str` is `no implementation of Reducible for str` (A7); the user-code impl runs (A11 e3a, e3b) | tranche 4, C10 for O(1) |
| S12 | `~x`, `#"re"`, `2r1010`, `^:private`, `::kw` | `unknown reader syntax`; `,x` is the unquote | E14 |
| S13 | `(println (some 3))` prints `3`, and `(str ["a"])` is `["a"]` | `(some 3)` (A11 t87); `str` and `println` both `Show` | the prelude change of §2.7 |

### 5.6 Sharp edges of Clojure that this page replicates

These are not safety matters, so the rule says to replicate them: `compare` built on `<` (a NaN compares equal to everything,
[R] A11 nan), `compare` of vectors by length first, `(take-nth 0 c)` repeating the first element, `(range 0 1 0.1)`
accumulating (11 elements, [R] A11 range) and step 0 repeating the start, `update-keys`, `set/map-invert` and
`set/rename-keys` keeping the last of a collision, `partition` dropping an incomplete last group, and, if Clojure's
`(hash 0.0)` differs from `(hash -0.0)` (not verified), that too. §9 Q33 asks whether the owner exempts any of them as
bugs; until then they are Clojure's. The one place where a replicated behaviour gives up determinism is a `sort` over floats that
contain a NaN (§2.7).

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
lib/fib/char.fib, regex.fib, math.fib, set.fib, walk.fib, data.fib, random.fib (global generator and `rng/`), io.fib, sys.fib,
                     async.fib, lazy.fib (LSeq), bigint.fib, ratio.fib, multi.fib (defmulti), meta.fib, dyn.fib (binding), test.fib
lib/Math.fib, Long.fib, Integer.fib, Double.fib, Character.fib, System.fib, Thread.fib
                     the Java static names Clojure code writes, implicit aliases (E15)
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
min merge conj assoc dissoc bit-and bit-or bit-xor`, the literals `{..}` (`hash-map`) and `#{..}`
(`hash-set`), `if-not`, `when-not`, `defn`, `defn-`, `some` (by argument count), `update`, and `list`** (E1),
each specified by a case that quotes its expansion. The list is thirty-two, two swapped against the first version:
**`comp` and `partial` leave** (they are arity-reading checker forms, L22, which a macro cannot be because it cannot
see `f`'s arity) and **`defn`, `defn-`, `some` and `update` join** (each is on every program's path, and a fibber macro
would cost 80 to 95 ms each); `and`, `or`, `when` and the one-armed `if`/`cond` expand to checker forms (L20). The
counting loop that the prelude's `for-each` and `range` macros make over a literal
`(range a b)` and a literal one-parameter `fn` stays, retargeted: `run!` and `doseq` over such a range expand
to it, because the library spelling `(run! f (range 0 1000))` otherwise costs a retain and two indirect calls
per iteration where the fused loop has none ([R] A10 forloop); deleting the `for-each` function (§8.3 C)
without rewriting its macro would leave the declined path calling an unbound name. The
macros that only some programs use are written in fibber and live with their module: `for doseq
get-in assoc-in update-in cond-> cond->> condp case as-> with format printf time delay lazy-seq lazy-cat
try-let if-some when-some when-first defrecord vswap! with-local-vars pcalls pvalues extend-type reify`.
The Rust list is the part of this design that stage 2 (M6) must re-implement; it is
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
| `some` | `Option`'s constructor and pattern | stays; Clojure's `(some pred c)` is the two-argument clause of the same name (§7 L1), and `find-map`, `find-first` are extras |
| `any?` | not defined | Clojure's unary `any?`; the quantifier is `some` |
| `empty`, `cons` | the `List` variants | variants become `Empty`, `Cons`; `empty` is `Emptyable`, `cons` a function (Clojure's names are the functions) |
| `next` | the `Iter` method | removed with `Iter`; Clojure's `next` is `Seqable` (§2.1 rule 8) |
| `join` | the task-wait builtin | stays; the string join is `str/join` |
| `range` | a macro for two arguments and a function for one | one function overloaded by arity (L1), returning `Range` |
| `map count first rest nth get assoc conj` | exist at narrower types (`map` on `Vec` only; `first`, `rest` on `List`) | widened by `Reducible`, `Seqable` and the protocols of §2.3 |
| `derive`, `defstruct` | a prelude macro, a core form | `derive` is also Clojure's hierarchy form, told apart by the first argument's kind (§4.2); `defstruct` stays fibber's record form (§4.17) |
| `println` | the one-`str` function | a macro over `Show` in head position; a function generic over `Show` serves value position, and `fib.prelude/println` stays reachable ([R] A6, A10 twin) |
| `Box box unbox` | `(Box a)` and two functions; 11, 4 and 13 case files | stay in the prelude; no Clojure meaning, no clash |
| `push! append` | `push!` writes through `&` (10 case files; §2.5's and N5's example), `append` is its alias (7 case files, 1 `compiler/` use) | both stay: `push!` is the form, `append` the alias it is; nothing requires removing a name that the rule does not make wrong |
| `length` | `(length s)` is `str-len` (3 case files) | stays as an alias of `str-len`; `count` of a `str` is characters (§2.9), so a reader who wants bytes writes `str-len` |
| `block-on yield panic` | task and abort helpers (`block-on` in 11 case files) | stay in `fib.async` and the prelude; no clash |
| `str-join` | `(Vec str) -> str`; 3 case files, 54 `compiler/` uses | stays as the builtin under `str/join`; the compiler's 54 uses are ported only when their tranche lands |
| `str-chars` | `(Vec char)`; 1 case file, 4 `compiler/` uses | stays; `(vec (str/chars s))` or `(vec s)` is the same, since a `str` is a `Reducible char` |
| `str-bytes` | a fresh `(Array i8)` per call; 2 case files, 9 `compiler/` uses | stays; `str-byte-at` and `str-find` avoid its copy (§7 L18) |
| `vec-empty map-empty set-empty` | the zero-argument constructors; 21, 1 and 0 `compiler/` uses | stay; `empty` takes a value; `[]`, `{}` and `#{}` are the spelling of new code |
| `set-contains? map-put! map-del! disj` | 1 case file each | `set-contains?` stays as an alias of `contains?`; `map-put!` and `map-del!` stay with `push!` (N5); `disj` is the `Dissoc` method |
| `eprintln` | the one-`str` function; 3 `compiler/` uses | stays beside `println`, with the same generic twin |
| `Countable Indexable Seq Traversable Iter Associative` | the prelude protocols | removed (§2.3); `Countable` has 1 case file, `Traversable`/`Iter`/`Seq`/`Indexable` none by name; `Countable str`, which counts bytes, is replaced by the `Reducible char` instance, so `count` of a `str` is characters |
| `first` on a `List`, `(list ..)`, `next` | `first` returns `e` and `rest` a `List` today; `(first ` is in 2 case files (01 and 61), `(list ` in 4, `(next ` in 2 | `first` returns `(Option e)`, so case 61's `(defun head (xs) (first xs))` changes type; **case 01 is one of the 20 owner-decided cases, and the owner decided (Q28, 2026-10-01, by the rule) that `first` returns `Option`**, so its port is the owner's own decision |
| `Show (Option a)`, `Show` of a `Map` | `(some 3)`, and key-order | printed as Clojure prints them (§2.7); no case pins printed text (the headers pin results and traps; 21 case files call `show` or `println`), so the change costs no case |

### 6.6 What stage 2 mirrors

M6 step 2 (the expander) is next, and every Rust-side item of this page is something stage 2 must reproduce
byte for byte. The first version listed only E1; this is the whole list, so the owner can sequence it.

| Item | What stage 2 must do | Tranche | Notes |
|---|---|---|---|
| E1 macros (§6.3) | the 32 Rust macros, each specified by a case that quotes its expansion | T0 | `list` expands to qualified names; `for-each`/`range`/`dotimes` counting loops stay; `defn` converts bracket parameters |
| E3, E4 | bracket binding forms; flat `cond` (six call sites in three files of `compiler/`: `jit-demo.fib`, `syntax/lexer.fib` x3, `lair/call.fib` x2, besides the Rust macro `logic.rs` and two Rust test files) | T2 | additive; replacement |
| E8, E14 | `#(..)` and `#{..}` read as forms; `~x`, `~@x` with the comma as whitespace, `x#`, `#'x`, `#"re"`, radix, `1N 1M 1/2`, `##Inf`, `#?`, `#tag`, `^`, `::kw` | T2, T3 | four pinned reader tests turn from errors into reads (`compiler/tests/reader/harness-067-hash-paren.fib`, `harness-068-hash-brace.fib`, `reader-218-dispatch-brace.fib`, `reader-220-dispatch-paren.fib`), the comma change touches every `,x` of `lib/`, `compiler/` and the cases, and `spec/bootstrap.md` §2 defines no dump line for a form the reader synthesises (the position of the generated `fn`): it must be added in the same commit (§9 Q38) |
| L1, L7, L8, L3b, L6, L9, L14, L15, L21 | arity clauses (a constructor clause for `some`), irrefutable patterns in parameters, refutable and prefix `let`, tuple patterns, or-patterns, `{:keys ..}` with `:or`, keyword and collection in call position, `def` initialisers and the module init | T2, T3 | each changes the expander or the checker that M6 steps 2 to 4 reproduce |
| L20, L22, L24, L25, L26 | truthiness (`if`/`and`/`or`/`when`/`cond`), arity-reading forms, joins and families, heterogeneous literals, literal widths and promotion | T2, T3, T4 | the checker; each is a rule over the types the checker has already solved, so stage 4 (the checker) carries them |
| L16, L17, L23 | determined variables in an impl context; `derive Debug`/`ToStr` and the derive-by-default decision; multi-parameter dispatch (§9 Q39) | T2, T3 | the checker; the Rust `derive` module |
| L13 | a warning for an unconsumed recipe (§7) | T2 | a new diagnostic class and a type attribute: the compiler has no warnings today |
| E11 | macro templates resolve in the defining module | T2 | the expander |
| C1 to C11 | closure types, last-use move, exclusive `&`, array primitives, unboxed `Option`, `-O` defaults, move-out of an owned shell, one shared closure environment, the mutex, the ASCII flag, the binding slot | T3 and after | lowering, the runtime and the ownership checker |
| `format`, `printf` | directives `%s %d %f %x`, with width, zero fill and case: `%4d`, `%04X` | T4 | the compiler's reader dump needs `{:04X}` and `U+0041` (`compiler/util/text.fib`'s `hex-upper`); until `format` has them `hex-upper` stays, and ROADMAP rule 6 puts formatting for diagnostics first, so the directives with width and fill move to **T1** if the owner agrees (§9 Q29) |

ROADMAP M7 rule 4 also asks for a small-vector fast path and `str` building without quadratic copies; the
page has neither, and §9 Q26 recommends the second.

## 7. Language and compiler changes

Ranked by what they unblock for the library, cheapest first within a tier. Sizes are estimates; every
**Evidence** is a program in Appendix A that was run (**[sketch]** where the change cannot be prototyped
without editing the compiler: the failing program is quoted and the change is described). "Needed by" names the first
tranche (§8) that cannot ship without it; every row also has the stand-in the library uses until it lands.
The rule (§1.1) is what adds L20 to L29, E14, E15, B5 and C8 to C11: each is a place where Clojure's text is
feasible and no memory-safety failure forbids it. They change types or syntax the owner decided, so the owner signs each
(P8); L16, L19, Q15 and Q28 are already **Decided** (owner, 2026-10-01, by the rule).

### 7.1 Stage-1 divergences (method rule 6: the interpreter and the compiler must agree)

| # | Change | Evidence | Size | Needed by |
|---|---|---|---|---|
| B1 | `fibc` lowers a protocol method that has its own type variable; `fibref` accepts it | `(defprotocol (Fold s e) (fold (self f: (fn (r e) r) :borrow init: r) -> r))`: `fibc` says `unsupported: a type variable reached the lowering: Gen(1)`, `fibref` returns 45 (A6). Cause: `method_target` passes only the impl's variables | small | C1 (a method that takes an `Fn` argument), accumulator-typed methods |
| B2 | quasiquote expands to a prelude-qualified `concat`; `` `() `` works under `fibc` | with `(defun concat (a b) ..)` in scope, every quasiquote macro is `concat takes 2 argument(s), got 3` (A6); `` (defmacro u () `()) `` is `macro u failed: concat of nothing` under `fibc` and 1 under `fibref` (A6) | tiny | T1: the library defines `concat` |
| B3 | protocol method symbols are mangled with the defining module | a user `(defprotocol (Collection s e) (conj ..))` with an `impl` for `(Vec a)` and any use of the prelude's `conj` on a `Vec` gives `fibc`: `compile failed: duplicate definition of @m.Collection.conj.$Vec.t0_.str`, while `fibref` accepts the program and the user's `conj` wins (result 0): a divergence in both directions (A6, [R] A10 b3) | small | **T0**: tranche 1's `Collection`, `Seq` and `Indexable` collide with the prelude's until it lands |
| B4 | polymorphic recursion: `fibc` never finishes monomorphising, `fibref` runs | `(defun len (xs: c) :where ((Reducible c e)) -> i64 (if (empty? xs) 0 (+ 1 (len (drop 1 xs)))))` and `(len [1 2 3])`: `fibc` is `memory allocation of 577136 bytes failed` under `ulimit -v 4000000`, `fibref` is `result: 3` ([R] A10 poly). Both tools must reject a function that calls itself at a strictly larger type, with a message that names it (`len recurses at (Dropped c e): polymorphic recursion is not supported; use loop or a List`) | small | T2 (before `rest` ships; without a bound the monomorphiser takes the machine down) |
| B5 | `fibc` lowers a struct constructor passed as a function value; `fibref` already runs it | `(map Pair xs ys)`-style use of a constructor as a value: `fibc` says `unsupported: a constructor as a value` and `fibref` returns 2 ([R] A11 t86); `->Name` and `(map ->Point xs ys)` need it | small | T2 (`->Name`, `vector` as a function value) |

### 7.2 Language and expander (small to medium each; L16 and L19 are Decided, L20 to L29 are the changes the rule requires)

| # | Change | Evidence | Needed by |
|---|---|---|---|
| L1 | **Arity overloading, in Clojure's shape**: `(defn name ([a: T] -> R body) ([a: T b: T] -> R body))`, resolved by argument count in the expander; each clause has its **own** `-> R`, `:where` and `:private`, because `get` returns `(Option v)` in one arity and `v` in the other. Clojure's parameter vector `[x y]` tells a clause `([x] ..)` from a single-arity `[x] body`, so **no `:arity` marker is needed** for `defn` (the first version's marker was for `defun`'s parenthesised parameters, whose clause form collides with L7's patterns; `defun` keeps one clause); two clauses of one count are an error; a protocol method and clauses of other arities may share a name; **a clause may be a constructor's: `some` is `Option`'s constructor of one argument and Clojure's `some` of two**; `(gensym)` is a zero-argument clause | two `defun`s of one name: `f is already defined`; a protocol method and a `defun` of one name: `gg is already defined` (A6); `(get m 5 0)` is `get takes 2 argument(s), got 3` (§7 D1 makes the message say `use get-or`); a `defun some` of two parameters makes `(some 5)` `some takes 2 argument(s), got 1` ([R] A11 n13); `(gensym)` is `gensym takes 1 argument(s), got 0` (A11 gensym); `defn` with bracket parameters is a macro over `defun` that runs (A11 e10: 13 under both tools) | T2 (`get/3 nth/3 reduce/2 range/n sort/2 sort-by/3 map/3 subs/2 index-of/2 partition/n repeat/2 some/2 gensym/0`); T1 uses the stand-ins of §2.3, deleted when L1 lands. Moving L1 into T1 for two names was considered and rejected (§10) |
| L2 | rest parameters `(defun f (a ... rest))` binding a `(Vec t)`; settle that `&` alone is reserved first | `(defun f (a: i64 & xs) ..)` compiles as a three-parameter function and `(f 1 2 3)` returns 1 (A6): a spec/code disagreement with syntax §1.1 | T3 (user variadics); the library's variadics are macros |
| L3 | `Pair`, `Triple` in the prelude (done in the prototype) | `(. (Pair 1 2) fst)` is `unbound name Pair`; a user-defined generic struct works (A6) | T1 |
| L3b | tuple-like structs (`Pair Triple`) accept vector patterns `[k v]` | `(match (Pair 1 2) ([a b] ..))` is `cannot unify (Pair i64 i64) with (Vec a)` (A6) | T2 (`(fn [[k v]] ..)`, `for [[k v] m]`) |
| L4 | **static protocol methods** (no `self`, dispatched on the type the context expects, as Rust's `Default::default()`) | `(defprotocol Dflt (dflt () -> Self))` is `a method is (name (self qual* x: T qual*) -> type)` (A6); the `Unit` witness works meanwhile (A7) | T3 (`assoc-in` creating levels, generic `sum` of an empty source, `(+)`) |
| L5 | none: `let` does not generalise (types §2.4); the library builds an `Xf` per use or uses the factory form of §2.1 | `Xf` serves two accumulator types (A8) |  |
| L6 | or-patterns `(or p q)`, alternatives binding the same names at the same types | `(match [1 2] ((or [a] [a b]) a) ..)` is `or is not a variant or struct` (A6) | T3 (`case`, `condp`) |
| L7 | irrefutable patterns in `fn`, `defun`, `loop` parameters, as sugar for a `let` in the body | `a fn parameter is sym or sym: type` (A6) | T2 |
| L8 | a refutable `let` pattern traps `let: pattern does not match`; **a vector pattern in a binding position takes a prefix** (binds the first n, ignores the rest, traps if there are fewer), as Clojure's; `let-else`; `match` keeps exact shapes | `a let pattern must be irrefutable` (A6) | T2 |
| L9 | named-field struct patterns `(Name :field p ..)` and `{:keys [a b] :or {a 1}}`, `:strs`, `:syms`: on a struct they bind fields, on a `(Map keyword v)` they bind `(get m :a)` (an `Option`, or `(unwrap-or ..)` for a key with `:or`) | `(match 1 ({:keys [a]} a) (_ 0))` is `braces are not allowed in patterns` (A6) | T3 |
| L10 | wrapping integer builtins `unchecked-add -subtract -multiply -negate -inc -dec` at the operand's width; `lIR`'s `add sub mul` already carry no overflow flags | `(+ 9223372036854775807 1)` is `trap: integer overflow in + at i64` (A6) | T3 (the multiplicative hash finaliser, generators). **Not T1**: the rotate-and-xor `hash-combine` of tranche 1 needs only `shl shr bit-or bit-xor`, which do not trap ([R] A10 hash) |
| L11 | exact math as builtins (`sqrt floor ceil rint copysign`); `extern` accepted by `fibref` | `(unsafe (fptosi i64 (sqrt 49.0)))` with `(extern sqrt :private (f64) -> f64)`: `fibc` prints 7, `fibref` says `unsupported: extern sqrt is not available in the reference interpreter` (A6) | T4 |
| L12 | `Result` in the prelude, and a scope-exit hook (a `Drop`-like protocol) for non-memory cleanup; **not `try!`**: there is no early return, so the threading form is the block macro `try-let` (§2.10), which needs no language change | `Result` is `compiler/util/result.fib` only; `(defmacro try! (r) `(match ,r ((Ok v) v) ((Err e) (return (Err e)))))` is `unbound name return`, and `try-let` runs, 106 under both tools ([R] A10 try) | T1 (`Result`), T2 (`try-let`), T5 (`with-open`, `line-seq`) |
| L13 | a **warning** for a value of an adaptor type that is dropped unconsumed, as a statement: **medium, not small**: a new diagnostic class and a type attribute ("must be consumed"). Clojure accepts `(map println xs)` as a statement (and does nothing), so under the rule it is a lint and not an error; the first version recommended an error | none: a requirement of P2 (a recipe not run is silent); the compiler has no non-fatal diagnostic of any kind (`grep -rniE warning crates/fibref/src crates/fibc/src` finds one test-harness name) | T2 |
| L14 | a keyword that unifies with `(fn (S) T)` elaborates to `(fn (x) (. x k))` when `S` is a struct with that field and to `get` when `S` is a `(Map keyword v)`; `(:k x)` in head position is the same rule; `(:k x d)` supplies a default. L21 generalises it to `Map`, `Set` and `Vec`. The narrow rule needs no choice of call-position semantics; the alternative is a reader form `.name` for `(fn (x) (. x name))` | `(:a {:a 1})` is `cannot unify keyword with (fn (a) b)` (A6, [R] A11 k1); `(map :name ps)`, `(sort-by :age ps)`, `(group-by :dept ps)`, `(filter :active ps)` are the commonest Clojure lines, and the group-by-then-count idiom is 46 tokens against Clojure's 29 without it | T2 |
| L15 | `def` initialisers: **any expression**, evaluated once before `main` in module order by an init function per module (Clojure evaluates them at load); a top-level `atom` is allowed; **the type of a `def` may not contain a `Cell` or a `Weak`** (not `Send`: a global `Cell` is reachable from every task, §5 M1); top-level forms that register (`defmethod`, `add-tap`) run in the init | `(def ok: (Set i64) (set [1 3]))` is `def ok: initialiser is not a constant expression`; so are `(def v: i64 (f 2))` and `(def counter: (Atom i64) (atom 0))` ([R] A10 def, A11 t56, n8b), while `(def v: (Vec i64) [1 2 3])` and `(def m: (Map i64 i64) {1 2})` run; `(def c: (Cell i64) (cell 0))` is the same error (n8); `#{1 2}` becomes `(hash-set ..)` with E8 | T2 (`(def stopwords #{"a" "the"})`, `(def table (zipmap ..))`), T4 (`rand`, `atom`s) |
| L16 | **Decided** (owner, 2026-10-01, by the rule). An impl's context (and a method's own variable) may name a type variable that a constraint determines from the head through a protocol's determined parameter (`(Cursable c k)` determines `k`): the liberal coverage condition, as Jones's functional dependencies give. It gives `Lookup` for `(Option s)` (a lookup through `nil`), bufferless cursors for `Mapped Dropped Taken Zipped`, and push visitors with a bound | `(impl (Cursable (Wrap k)) (Src c) :where ((Cursable c k)) ..)` is `type variable k is not a parameter of the impl head` ([R] A10 cur-impl, A11 t23, t61b); the same rule rejects an impl for the cursor of an adaptor and a push `Each` protocol whose visitor is a protocol parameter (`push1.fib`), and `a method is (name (self qual* x: T qual*) -> type)` rejects a bound on a method's own variable (`push2.fib`, A10 push); a nested head `(Option (Map k v))` is `an instance head is a type constructor applied to distinct variables` (A11 e5) | T2 (`(get (get m :a) :b)`; adaptor cursors without the buffer); C1's push chain |
| L17 | `derive Debug` and `ToStr` (and the text of every type of §2.7); **`defstruct` and `defenum` derive `Eq Ord Hash Show Debug ToStr` by default when every field has them** (settled by the rule: a Clojure record is `=`, `hash` and printable); `(derive ..)` stays for the rest and a qualifier opts out | `(derive Debug P)` is `cannot derive Debug: only Eq, Ord, Hash and Show` under both tools; `(= (P 1 "x") (P 1 "x"))` is `no implementation of Eq for P` (A10 dd); derived `Show` prints `(P 1 x)` | T1 (`Debug`), T2 (the default) |
| L18 | builtins `(str-byte-at s i)` (one byte, no allocation, traps out of range) and `(str-find s pat from)` (the first byte offset at or after `from`) in both tools | `(str-bytes s)` allocates a fresh array per call: 100 objects for 100 calls (A10 str), so `str/index-of` from an offset is O(n) copying per call and a tokenizer O(n²) | T1 (every string function of §4.7) |
| L19 | **Decided** (owner, 2026-10-01, by the rule): an integer literal whose value is exactly representable adopts a float type when it unifies with one; a variable never does through this rule. L26 extends the same mechanism to the integer widths | `(* 2 1.5)` is `cannot unify f64 with i64` (A6, [R] A11 n2); `(+ x 1)` with `x: i32` is `cannot unify i64 with i32` (n3c) | T2 (`(* 2 x)`, `(/ x 2)`, `(+ x 1)` on doubles) |
| L20 | **Truthiness, typed.** A test of `if`, `when`, `while`, `cond`, `and`, `or`, `not`, `when-not`, `if-not`, `cond->` and `some->` accepts `bool` or `(Option T)` (truthy when `true` or `(some _)`; `(Option bool)` truthy when `(some true)`, Clojure's `false`); `and` has the type of its last operand; `or` is typed by its last operand (`(or (Option T) T)` is `T`, `(or (Option T) (Option T))` is `(Option T)`, `(or bool bool)` is `bool`); a one-armed `if`/`when`/`cond`/`when-let` is `unit` for a unit body and `(Option T)` with `some` around the body otherwise; any other type as a condition is a compile error. Both tools; `and`, `or` and `when` become checker forms. **[sketch]** | `(if (get m 1) 1 2)` and `(or (get m 2) 7)` are `cannot unify (Option i64) with bool` ([R] A11 t01, t02), `(if 5 1 2)` `cannot unify i64 with bool` (t03), `(when c 5)` `cannot unify unit with i64` (t82); the library form runs: `Truthy` and macros give `if`, `or`, `and`, `when` over `bool` and `Option`, 127 under both tools (t04), a predicate with a `Truthy` result 4 (t05), a macro that wraps in `some` 12 (t83); `(if* (some false) 1 0)` is 1, so `(Option bool)` needs its own rule (e2b); one `or` for two result types overlaps, `overlapping instances: Or2 for (Option a)` (e2, t70); no blanket impl, `an instance head must not be a type variable` (t06); the elaboration is a direct call to an identity (`fibc emit`, A11 e1z); run-time cost nil. small-medium | T2 (`if-let`, `when-let`, `some`, `filter` over `Option`) |
| L21 | **Callable collections and keywords.** A `Map`, `Set`, `Vec` and keyword in call position, and where a `(fn (A) R)` is expected, elaborate to the `get` eta-expansion of §2.11; `(assoc r :k v)` and `(update r :k f)` on a struct with a literal keyword elaborate to `with`; after C1 `Fn` is a protocol and each is one `impl`. **[sketch]** | `cannot unify (Map i64 i64) with (fn (a) b)` ([R] A11 t51, n5), `(Set i64)` with `(fn (a) bool)` (n6), `keyword` (k1), `(Vec i64)` (k2); `(assoc p :x 5)` is `no implementation of Associative for P` (t53); the literal-keyword macros run, 30 31 32 (e14). medium | T2 (keyword), T3 (the collections) |
| L22 | **Arity-reading forms**: `partial`, `comp`, `complement`, `juxt`, `every-pred`, `some-fn`, `memoize`, `fnil`, `constantly` (arity from the expected type) and `apply` (a literal vector spreads; a runtime collection checks its count; a table names the variadic folds) are core forms the checker elaborates from the `(fn (A..) R)` of their function arguments; `some->` and `some->>` (a step that returns an `Option` is kept, a plain value wrapped) are the same mechanism. **[sketch]** | `(partial add3 1)` is `cannot unify (fn :send (i64 i64 i64) i64) with (fn (a b) c)`, `(comp inc add2)` `cannot unify (fn :send (i64 i64) i64) with (fn (a) i64)`, `(constantly 7)` where `(fn (i64 i64) i64)` is expected `cannot unify (fn (a) i64) with (fn (i64 i64) i64)` ([R] A11 arity..arity4); `apply` as a macro runs for a literal vector and the named folds: 6, 3, 6, `312` (e12); a library cannot tell an `Option`-returning step from a plain one (t06). medium | T3 |
| L23 | **Multi-parameter dispatch** (**owner's decision**: a type-system change): a protocol may declare several dispatch parameters, an instance is keyed by the heads of all of them, an overlap check runs on the product and ambiguity is reported (types §3.3, §4), with L16. As library code it gives `or`/`and` by operand types, `=` across families, `(into {} [[1 2]])` (a two-element `Vec` entry beside the `Pair` entry), `merge` with `nil`, `str/replace` by replacement type, `(Option bool)` in predicate position and the numeric protocols of L26. Every use has a cheaper checker rule (L20, L24) if the owner does not take it (§9 Q39) | `overlapping instances: OrHit for (Option a) is already implemented` ([R] A11 t70), `overlapping instances: Collection for (Map k v) is already implemented` (t71), `an instance head is a type constructor applied to distinct variables` (t61), `type variable k is not a parameter of the impl head` (t23, t61b). large | none required; T3 if taken |
| L24 | **Joins and families.** (a) At an `if` or `match` join whose arms are distinct types that are both `Reducible` of one element type (recipes, `Vec`, `List`), unify at `(dyn (Reducible e))`: C1's erasure rule extended to recipes; (b) `(= a b)` with operand types of one family (sequential, map, set) elaborates to `seq=` or its map and set twins. **[sketch]** | `(if flag (filter p v) v)` is `cannot unify (Vec i64) with (Filtered (Vec i64) i64)` ([R] A10 join); `(seq-of ..)` at both arms runs; `(= [1] (range ..))` is `cannot unify Range with (Vec i64)` (A11 n4), `seq=` runs (A10 showseq). medium | T3 |
| L25 | **Heterogeneous literals.** A map literal whose values do not unify elaborates to `(Map keyword Val)` with `to-val` on each value; a two- or three-element vector literal whose elements do not unify is a `Pair`/`Triple` (a homogeneous one stays a `Vec`), so `(into {} [[:a 1]])` and `(map vector ks vs)` follow Clojure; `Val` is `fib.data`. **[sketch]** | `[1 "a"]` and `{:a 1 :b "x"}` are `cannot unify i64 with str` ([R] A11 k8, t91, t92); `Val` with `to-val` runs, `{:name "ann", :inner {:tags ["a" "b"], :x 1}}` and a runtime `get-in-v` (e17). small-medium | T3 (literals), T5 (`Val`) |
| L26 | **Numbers.** (a) literal adoption across integer widths and `f32` (L19's mechanism); (b) operator-level promotion for variables along `i8 < i16 < i32 < i64 < f64`, `f32 < f64` at the numeric builtins and at parameters declared `f64`/`f32` or a wider integer, never narrowing and never at `let`, return or field; (b) lifts types §1.1 D3 and is §9 Q36. A deferred constraint in the checker and in `fibref`; a generic `(defun add (a b) (+ a b))` stays `∀a. (Num a) ⇒ a a → a`. **[sketch]** | `(+ n 2.5)` is `cannot unify f64 with i64` ([R] A11 n1), `(< n 2.5)` (n10), `(f 2)` for `(f x: f64)` (n11), `(/ (reduce-sum xs) (count xs))` into an `f64` (t85), `(* x 2)` with `x: f64` (t63), `(+ x 1)` with `x: i32` (n3c); `(/ (sitofp f64 n) (sitofp f64 m))` is 25 (t84). medium | T3 (a), T4 (b, if signed) |
| L27 | **Metadata**: a `(Option (Map keyword Val))` field on the four collection headers and on structs, ignored by `Eq` and `Hash`; `^{..}` and `^:k` read as `with-meta` forms. **[sketch]** | none: no program of the review needs it; `meta` and friends are Clojure names (§4.3) | T5 |
| L28 | **Exceptions**: `throw`, `try`, `catch`, `finally` by unwinding: either a landing-pad `invoke` in lIR, or a transitively inferred "may throw" effect that makes each such function return a hidden `Result` and reuses the release code of the normal return; the first step is a task's trap returned by `join`. **The owner's decision** (amends types §2.11, §9 Q35). **[sketch]** | `trap` in a spawned task: `trap: boom` then `Aborted` under `fibc`, `trapped:` under `fibref` ([R] A11 t40, e9); lir.md `(trap)` is `llvm.trap` "without unwinding"; the ownership checker already computes the live owned locals of every scope exit (types §6.3). large | T5 (after viability) |
| L29 | **Dynamic vars**: the `:dynamic` qualifier on `def` and a `binding` core form that pushes and pops at the scope exit and is conveyed into `spawn` as a captured `Send` value (runtime C11); `*out*`, `*in*`, `*print-length*`, `with-out-str`, `with-redefs` are libraries over it. **[sketch]** | `(def :dynamic *x*: i64 1)` does not exist; no program can show it | T5 |
| E1 | the thirty-two Rust prelude macros of §6.3 (`str println print prn pr swap!`, the variadic folds, `{..}`, `#{..}`, `if-not`, `when-not`, `defn`, `defn-`, `some`, `update`, `list`), and the retargeted counting loop of `for-each`, `range` and `run!`/`doseq` over a literal range; a macro and the prelude function of one name coexist today (`swap!` over the builtin: 30 under both tools, A10 swap) | `+ takes 2 argument(s), got 3`, `unbound name str`, `println 5` is `cannot unify i64 with str` (A6); the fused loop of `(for-each (range 0 1000) (fn (i) ..))` has no call in `main`, the library spelling has a retain and two indirect calls per iteration (A10 forloop) | T0 |
| E2 | the macro runner compiles all of a program's macros in one module (or runs small ones in the evaluator); calls to functions of required **and used** modules work at expansion time (the macro-time module is the transitive closure of both), and the two tools say the same thing when they do not. **Unproven**: syntax §3.16 says `(var m/f)` works, and no program of the review made it work | 1, 5, 20 macros: 0.20, 0.50, 1.70 s under `fibc` (A9). A macro that calls `(u/bump 3)`, or `((var u/bump) 3)`, of a required module is `macro twice failed: f4.fib:2:46: unbound name u/bump`/`var: no definition named u/bump` under both tools, and with `(:use mu)` and a bare `(bump 3)` `fibc` says `unbound name bump` while `fibref` says `macro twice calls bump, which is not available at expansion time; move bump to a required module`, although the module IS required ([R] A10 mh) | T2 |
| E3 | bracket binding forms accepted beside the parenthesised: `let loop fn doseq for if-let when-let dotimes` | `(let ([a 1 b 2]) ..)` is `malformed let: a binding is (pattern expression) or (name: type expression)` (A6) | T2 |
| E4 | Clojure's flat `cond` replaces `(cond (t e) ..)`; six call sites in three files of `compiler/` are migrated (`jit-demo.fib`, `syntax/lexer.fib` x3, `lair/call.fib` x2) | `(cond (= 1 2) 5 true 6)` is `malformed cond: a clause is (test body+)` (A6); a clause and a flat test cannot be told apart, so it is a replacement | T2 |
| E5 | the expander rejects duplicate literal keys in `{..}` and `#{..}` (Clojure's reader does; a computed duplicate: the last wins) | `(count {1 2 1 3})` is 1 (A6) | T3 |
| E6 | freeing a linked object is iterative: dropping a long `List` must not recurse (small, runtime) | a 300,000-cell `List` overflows the stack under `fibc run` (`thread 'main' has overflowed its stack`), 50,000 cells are fine (A6); an executable exits 139 (design record) | T2 (`List` is the recursion type) |
| E7 | a library root list (`-I DIR`, `FIB_LIB`, embedded roots) and `(:export-from ..)` | modules load from the main file's directory only (A9) | **T0** |
| E8 | reader macros `#(..)` and `#{..}` reading as `(fn (%1 ..) ..)` and `(hash-set ..)`; the M6 reader and its dump are extended in the same commit (E14 adds the rest of Clojure's reader) | `unknown reader syntax #(: only #_ is defined`, `#{` likewise (A6) | T2 |
| E9 | an on-disk cache of each module's checked interface; demand-driven body checking | §6.4: hypotheses | after 20,000 library lines |
| E10 | hazards found: `Hash f64` and `-0.0`/NaN; **`derive Hash` and the prelude's `Hash (List a)` trap on overflow** (`h*31 + hash x`: replaced by `hash-combine`, §2.7); the `List` variants `empty`, `cons` renamed `Empty`, `Cons`, and the `list` macro expands to qualified names | `(get (assoc (map-empty) 0.0 1) -0.0)` is `nil` while `(= 0.0 -0.0)` (A6); `(hash (P2 "hello" "world"))` of a `derive Hash` struct of two strings and `(hash (list "a" "b" "c"))` are `trap: integer overflow in * at i64` under both tools (A10 hash); `(defun empty ..)` then `(list 1 2)` is `cannot unify (fn :send ((Vec i64)) bool) with (List i64)` (A6, A10 list) | **T0** (`Hash f64`, `derive Hash`, `Hash (List a)`), T2 (`List`) |
| E11 | a free symbol in a macro template resolves in the macro's **defining module** (Clojure's syntax-quote qualification), not at the use site; binders stay unrenamed (`gensym` covers them, syntax §3.16 declines hygiene), or at least the `fib.prelude/` treatment extends to every implicit `fib.*` module | a library macro `` `(first ,v) `` gives 1000, the user's `first`, from a module that defines one, and `no implementation of Seq` from one that does not (A10 hyg); the library's fibber macros (`for get-in case update-in`) and the Rust macros that expand to a `fib.coll` method (`conj assoc dissoc merge`) change meaning when the user writes `(defun filter ..)` or `(defun get ..)` | T2 (the first fibber macro that calls another function) |
| E12 | two `:use`d modules that export one name: an error when the name is referenced unqualified, as `spec/syntax.md` §5 says; the code takes the first `:use` silently | `(:use y.m1 y.m2)` then `(peek 3)` is 3, `(:use y.m2 y.m1)` is 4, under both tools (A10 use). A spec rule and the code disagree; **reported, not changed** (§9 Q27) | T0 |
| E13 | `if-let` expands to `(match e ((some p) a) (_ b))` so that a refutable pattern falls to the else (§2.4) | `(if-let ([a b] o) ..)` is `non-exhaustive match: missing (some [])` (A10 iflet); the same `match` written by hand returns 9 under both tools | T1 |
| E14 | **Clojure's reader**: `~x` and `~@x` with the comma as whitespace (`[1,2]` is `[1 2]`); `x#` auto-gensym in a quasiquote; `#'x`; `#"re"`; radix literals `2r1010 36rZZ`; `1N 1M 1/2`; `##Inf ##-Inf ##NaN` (desugared to `f64-inf` and `f64-nan`); `#?(:fib x :default y)`; `#tag form`; `^T x`, `^{..}`, `^:k`; `::kw`. The M6 reader and its dump change in the same commit, and **every `,x` of `lib/`, `compiler/` and the cases migrates to `~x`** (§9 Q38) | `unknown reader syntax #(: only #_ is defined` (A6); syntax §1.1: a comma touching a form is the unquote, so `[1,2]` unquotes the 2; the 218 reader tests of `compiler/tests/reader/` pin today's reads. small each, large churn | T2 (with E8), T3 |
| E15 | **Modules and names**: the Java class names `Math Long Integer Double Character System Thread` are implicit aliases of modules of that name; `ns` accepts `:refer`, `:only`, `:exclude` and `:rename` and a top-level `(require ..)`, `(use ..)`, `(alias ..)`, `(refer-clojure ..)` is hoisted into the `ns`; `declare` is a no-op | a module named `Long` gives `Long/MAX_VALUE` and `(Long/bitCount 255)` verbatim, `9223372036854775807` and 8 ([R] A11 e19); `ns` takes `:require` and `:use` only (syntax §5). small | T4 |
| D1 | diagnostics: a type other than `bool` or `(Option T)` where a condition is wanted says `a value of type i64 is always true; write the test`; a transducer where a function is wanted says `compose with xf`; a call with one argument too many to `get`, `nth`, `reduce`, `sort`, `range` says `use get-or`, `nth-or`, `reduce1`, `sort-with`, `range-by` (until L1) | `cannot unify i64 with bool` (A11 t03), `cannot unify (Xf i64 i64) with (fn (a) b)` (A10 comp), `get takes 2 argument(s), got 3` | T1 |
| H1 | the case harness reads an object count: a header `allocs: <= N` (a maximum), checked against the `A` lines of `fibc run --trace` (and `fibc itrace`, which gives the same count) | a case header is `spec expect result audit error trap`: no count (§2.5); `fibc run --trace` and `fibc itrace` print identical `A` counts, 2099 for the chain of A1 | **T0** (the count cases of §8.1 item 5 cannot be written without it) |

### 7.3 Compiler performance and runtime (medium each; the gates of P2 and P3, and of the rule's items)

| # | Change | Evidence | Size | Gates |
|---|---|---|---|---|
| C6 | `fibc build` optimises at level 2 by default; `fibc run` stays at level 0 with a flag for 2 | `JitOptions::default()` and `Options::default()` are level 0 (read in `crates/lair/src/jit/mod.rs`, `aot.rs`, `fibc/main.rs`); `-O 2` leaves five indirect call sites on the chain (A1); **the change is not tiny**: a main of 17,256 lIR lines takes 0.40 s at `-O 0` and 1.00 s at `-O 2` under `lair run` (A10 timing), so a default of 2 for `run` multiplies the cost of every run by 2.5 | small for `build`, medium with the instance cache of §6.4 | C1's payoff, at build time |
| C2 | **a last use moves**: when a variable's last use on every path is the argument of a call or a `cell`/constructor, `consume` moves instead of retain-then-release; loop variables move into the call before `recur` rebinds | `bump` called in a loop: 1001 arrays for 1000 calls; `explain`: `arg 1 acc: retain` (A4) | medium | P3: in-place `conj`/`assoc`/`update` with no API change |
| C3 | **exclusive `&`**: when `&x` is the only mention of `x` and `x` is not captured, copy-in shares the object without acquiring | `(push! &v i)` 1000 times: 2094 objects, `&` copy-in acquires (A4) | small-medium | `push!`, `map-put!`, builders |
| C4 | array primitives for the library: `(array-push! &a x)` with capacity, `(array-update! &a i f)` that moves the slot out and stores the result, an uninitialised `(Array a)` | `array` needs an element, so no generic empty array exists and the prelude has `VecEmpty` (prelude comment) | small-medium | the in-place trie and tail |
| C1 | **closure types**: a `fn` literal passed to a parameter of a type variable bounded by the built-in `Fn` is a nominal type whose captures are fields, a monomorphisation key; `(fn (A) R)` stays the dynamic type; **an erasure rule at joins** (two closure types, or two adaptors holding them, meeting in an `if` or `match` unify at the erased `(fn (A) R)` of the parameter; §2.2) | functor emulation of a **pull** chain: 0 indirect calls, 0 retains per element, and `-O 2` leaves none on the chain (A2), at the price of 1500 calls of `f` for 1000 elements (A10 d1c); the **push** form cannot be declared today (A10 push) and needs B1 and L16; the impl head on `(fn ..)` is rejected as it stands (A6); with a type per closure `(if flag (map2 (Inc ..) v) (map2 (Dec ..) v))` is `cannot unify (Mapped (Vec i64) Dec) with (Mapped (Vec i64) Inc)` (A10 c1emul) | medium to large (with the erasure rule and L16) | P2 at Rust speed; `sequence`/`eduction` |
| C5 | unboxed `(Option scalar)` and small enums and structs by value (lIR already has by-value aggregates) | 100 calls returning `(Option i64)` and `(Step i64)`: 200 allocations (A5); `get` on a `(Map i64 i64)` allocates per lookup; `(first v)` in a loop of 1000 is +1000 objects, `zip` +1003, `keep` +1002 (A10 alloc) | medium-large | `get`, `first`, `reduce-while`, `Step`, `zip`, `keep`; scheduled with tranche 3, whose count cases for these stay `open` until it lands |
| C7 | a `match` or field read on an owned shell that is dead afterwards **moves the payload out**: no retain of the field, no release of it with the shell | `bump` that matches an owned `(E1 n arr)` and writes `arr` through a cell: four chained calls on unique temporaries cost 10 objects (A10 own); a `self :owned` method on a struct with an array field, 1000 times in a loop: 2002 objects, `explain`: `(cell ..) arg 1 (. self a): retain`, `release [self] (exit)`; a struct held in a cell updates in place: 1000 `set-field! &c n ..` allocate nothing | medium | P3 for the existing `Vec` and `Map` (C2 to C4 fix only the caller's side, §2.5); the alternative is the struct `Vec` of §9 Q18 |
| C8 | **one closure environment for a group of mutually recursive local functions** (`letfn`); no cycle is needed, as a local recursive closure already calls itself through its code pointer (ownership.md §6) | two local functions that reach each other through two cells run and `audit: clean=false leak-cycles=4 leaks=0 errors=0` under `fibref` ([R] A11 t55): a leak, not a safety failure | medium | `letfn` (T3) |
| C9 | **a runtime mutex** `(Mutex a)` that owns its value: `locking`, and a run-once `LSeq`/`Delay` that crosses tasks | two tasks writing one `Cell` is rejected, `cell cannot be shared between threads` ([R] A11 t41); the `Cell` memo cannot cross (e8d), the `Atom` memo can but may run a thunk twice under a race (e8e) | medium | `locking`, thread-safe `LSeq` (T5) |
| C10 | **an ASCII flag in the `str` header**, computed in the UTF-8 validation pass that already scans every constructed `str`: `count`, `nth`, `subs` and `index-of` by character offset are O(1) for ASCII text and a byte walk otherwise; a layout change in both tools **[sketch]** | validation happens at construction: `str-from-bytes: invalid UTF-8`, `str-slice [0, 1) splits a character` ([R] A11 t13, t12); a user `Reducible char` over `str` gives the semantics today (e3a, e3b) at O(n) | medium | the cost of Clojure's string unit (§9 Q37) |
| C11 | **a per-task binding slot** in the task header, copied into a spawned task's header from a `Send` capture | none: depends on L29 | medium | `binding`, `*out*` (T5) |

An alternative to C1 that was proposed, specialising a callee on a literal closure passed to a
non-escaping `:borrow` parameter, makes the visitors of `each-while` direct but not a closure stored
in an adaptor struct, so it does not reach the chain's user functions; it is not adopted (§9 Q17).

**What is not needed.** A transient API (the Clojure names are identity wrappers, §4.4); a Perceus-style reuse of matched shells (large; considered only
if C2 to C4 leave the `Vec` header as the measured bottleneck); a rank-2 type; a second sequence protocol (the `Seqable` of §2.1 is the closed-rest form of the one that exists).

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
4. **Laws**: `=` implies equal `hash`; `compare` is antisymmetric and transitive on every value except a NaN, which
   compares 0 against everything, as Clojure's does (§2.7); `sort` is stable and a permutation, and ordered for floats
   without a NaN; `(into (empty c) c)` equals `c`; each on generated nested values, floats with NaNs among them for the
   permutation law. A hash law over strings and over structs of strings runs to lengths where the old combiner
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
8. **The rule is executable.** Each M row of §5 has a case in `cases/stdlib/rule/` whose header expects the rejection or
   the value quoted there (`;; error:` or `;; result:`), so a reason that stops being true fails a test; each S row has
   its failing program as a case in the `open` list, which passes when the item of §7 lands and the program is accepted
   (a case that is `open` is printed as failing, never as pending). The two audits of §10.4 ran programs for
   every reason (A11); those programs are the first cases.

### 8.2 Tranches

| T | Content | Needs (§7) | Test emphasis | Rows |
|---|---|---|---|---|
| 0 | the library root and re-export; the thirty-two Rust macros of §6.3; `Hash f64`; `derive Hash` and `Hash (List a)` over `hash-combine`; the quasiquote fix; the protocol-mangling fix; the duplicate-`:use` check (if the owner agrees, §9 Q27); the harness's `allocs` header; the disjointness test of the four implicit modules as a CI gate | E7, E1, B2, B3, E10, E12, H1 | cases for each macro's expansion text; a `hash` of 14 strings and of a two-string struct that does not trap; both tools agree on B3's program | |
| 1 | **what stage 2 needs.** `Pair Triple Step Result Unit`; `unwrap unwrap-or map-opt and-then`; `min max abs inc dec mod compare` (Clojure's, built on `<`); `hash-combine hash-ordered-coll hash-unordered-coll`; `Eq Ord Hash Show Debug ToStr` for `Vec Map Set Pair Triple` (`derive Debug`; `Ord` of a `Vec` by length first); `Reducible`, its sources (`Vec List Array Option Range Map Set Chars`) and `map filter remove take drop take-while concat mapcat`; `reduce reduce1 reduce-while first last nth find-first find-map every? not-any? any? count empty? run! quot`; `into vec set` (bulk builders); `sort sort-by sort-with sort-by-with reverse` (a comparator is a `Cmp`); `Lookup Assoc Dissoc Keyed Collection Emptyable Stack` with `get get-or assoc dissoc update update-or update-opt fnil contains? includes? keys vals key val merge group-by frequencies select-keys`; strings: `str` (`ToStr`), and qualified `str/join str/split str/split-lines str/trim str/triml str/trimr str/index-of str/includes? str/starts-with? str/ends-with? str/blank? str/chars`, with `subs str-len str-byte-at str-find`, `parse-long`, `digit? whitespace?`; `println print prn`, and their generic twins; `newline`; `write-file`; the builtins' names and Clojure aliases as §4.1; `defn defn-` (one clause) | T0, L3 (`Pair`, `Triple` in the prelude), L12 (`Result`), L17 (`Debug`), L18 (`str-byte-at`, `str-find`), E13 (`if-let` over `Option`), D1 | model tests for `Vec Map Set` and `sort`; laws; a hash law over strings; the integration test | 154 |
| 2 | **the Clojure surface.** the rest of the sequence functions (`partition* interleave zip zip-with seq-of seq= map-indexed keep reductions distinct interpose drop-while second mapv filterv repeat iterate cycle some`), `Cursor` and the buffering `Cursable` adaptors; `Seqable seq rest next ffirst`; `LSeq lazy-seq lazy-cat doall dorun repeatedly` (effect-sourced sequences memoised); the small insertion-ordered `Map`; Clojure's printed forms; `for doseq cond if-not when-not as->`, `if-some when-some when-first declare defonce`; **truthiness (L20), the keyword in call position (L14, L21), `and`/`or` typed**; `partial` (one free) `partial2 complement constantly`; `with try-let defrecord ->Name vector volatile!`; the arity forms of §4 on L1, deleting the stand-ins; patterns in parameters, bracket forms; the reader additions of E8 and E14 | L1, L3b, L7, L8, L14, L15, L17, L19, L20, L21 (keyword), E3, E4, E8, E14, E11, L13, E2, B4, B5, E6 (`List`), E10 (`List`), L16 (**Decided**: adaptor cursors without the buffer, `Lookup` for `Option`) | the 60 programs of the design record (20 each from three designs, all ran) ported as cases; expansion texts; a recipe sent to a task; polymorphic recursion rejected by both tools; the failing programs of §5.5 S1, S2, S7, S8 flip from `open` | 82 |
| 3 | **collections.** `get-in assoc-in update-in update-keys update-vals`; `condp case juxt dedupe flatten`; `fib.set`; `fib.sorted` (B-tree, `Queue`); `Slice`/`subvec`; `rseq`; `Xf`, `Stepper`, `transduce`, `into/3`, `xf`; the multiplicative hash finaliser; `unchecked-*` and the aliases of §4.1 and §4.3 (`aget`, `long-array`, `transient`); `ex-info` as data; **arity-reading forms (L22: `partial comp juxt apply some->`), joins and families (L24), heterogeneous literals (L25), literal widths (L26 a), `letfn` (C8)**; chunked `pmap`; the in-place paths of `Vec` and `Map` (C2 to C4 and C7, or the struct `Vec`, §9 Q18) | L2 (user variadics), L4, L6, L9, L10, E5, L22, L24, L25, L26, C2, C3, C4, C7, C8, C5 (the `first`/`get`/`zip` count cases stay `open` until it lands) | B-tree model with `valid?`; the `conj` count case at n + c moves from `open` to required | 170 |
| 4 | **strings and numbers.** `str` as a `Reducible char` and `fib.string` in full (character offsets, `Pattern`), `fib.char`, `fib.regex` (non-backtracking core), `fib.math`, `fib.random` (the global generator, L15 init), `fib.walk`, `fib.sys`, the Java-name modules (E15), `fib.io`, `format printf`, `read-string`; **variable promotion (L26 b) if the owner signs §9 Q36** | L11, L12, L15 (top-level atoms), E15, L26 (b), C10 for O(1) character offsets | strings against a byte-loop reference over generated UTF-8 (every scalar width, boundaries) and against a character-offset reference; regex against a bounded backtracking reference; math bit for bit between the interpreter and the compiled program on 3000 generated patterns, and against libm in a Rust test (exact functions equal, the others within a stated number of ulps); `Rng` against known first values | 93 |
| 5 | **the long tail.** dynamic vars and `binding` (L29, C11) with `*out*` and `with-out-str`; metadata (L27); `defmulti` and hierarchies; `locking` and a thread-safe `LSeq` (C9); `BigInt Ratio BigDecimal` and their literals; `fib.data` (`Val`, `diff`), `pprint`, `sequence eduction` (after C1), `iteration`, `subseq`, the relational `set/project index join`, `seque`, `add-watch`, `with-redefs`, `file-seq`, `re-matcher`, lazy `pmap`; the backtracking regex fallback; **exceptions (L28) after the owner's §9 Q35** | C1, L12, L27, L28, L29, C9, C11 | as above | 84 |

The Rows column counts the survey names assigned to the tranche in §4 (154 + 82 + 170 + 93 + 84 =
583 names, plus 42 names Clojure lacks that the library adds; the other 89 survey names are not offered (§4.17), and
`#"..."` shares the row of `#"regex"`). Tranche 1 includes the core forms and builtins that already exist. The Needs
column was checked against §7's "Needed by" column row by row in the review; the first version omitted B3,
L3, L12, L10, L2, E6, E10 and C5.
After tranche 3 the library is *viable* in ROADMAP rule 5's sense, and the benchmark suite (fibber
and Rust kernels, a ratio per kernel against the aim of within 1.5x) is designed then, not here. The order
is still led by what the compiler needs: tranches 0 and 1 are the same as before the rule, and what the rule
added (truthiness, callables, arity-reading forms, the string unit) lands in tranches 2 to 4 behind the
Clojure surface it makes terse.

### 8.3 Migration of the prelude

A. Add the library modules beside `fib.prelude`; names that collide (`map count range first rest`) are
shadowed by the `:use`, which the module system does silently ([R] A6), and the library's protocols that reuse a
prelude name (`Collection`, `Seq`, `Indexable`) need §7 B3 first, or step C in the same commit as step A.
B. Port the cases that use the names being replaced. Counted by grep over the 191 `.fib` files of
`cases/ownership`: `range` in 7, `for-each` in 5, `pmap` in 4, `filter-iter` in 2, `iter` in 2, `(map` in 1,
`collect` in 1, `Countable` in 1, and, found by the review, `first` in 2 (cases 01 and 61: `first` on a `List`
changes type from `e` to `(Option e)`), `(list ` in 4, `(next ` in 2, `map-put!`, `map-del!` and `disj` in 1 each (`append`, `length` and `set-contains?` stay as aliases, §6.5), and `(. e key)`/`(. e val)` on a map entry in case
181 (the `Entry` struct is removed for `Pair`, §2.3); §6.5 lists every prelude name. C. Delete `Traversable`,
`Iter`, `Seq`, `Indexable`, `Countable` (with `Countable str`, which counts bytes), `Associative`, `for-each`,
`iter`, `collect`, `filter-iter`, `VecIter`, `Entry`, and the Vec-only `map`; make the four
implicit modules the implicit `:use`. D. Move `Vec` and `Map` out of `lib/prelude.fib` with their
layout unchanged (`fibc`'s `rt/vec.lir` and `fibref`'s `eval/vecs.rs` read it). No case counts a string
literal and `compiler/` calls `str-len` (19 uses), so making `count` of a `str` count characters costs nothing that
was found; the compiler's own errors list any other use. The printed forms change (an `Option` prints its payload, a seq prints in
parentheses, a small `Map` in insertion order), and no case pins printed text: the headers pin results and traps (§6.5).

## 9. Open questions for the owner

The owner prefers a recommendation to an open question; each row has one, with the evidence it rests on. "Decide" means
the owner's sign-off turns it **Decided** in the page it changes. The rule (§1.1) settled most of the first version's 32
questions; §10.4 lists each with the verdict. What is left is what the rule does not settle: a collision between the
owner's two aims (Clojure's semantics against Rust's speed), a decision made before the rule that the rule would
reverse, a sequencing question, and implementation choices.

### 9.1 Decided

| # | Decision | Owner, date | Where it lands |
|---|---|---|---|
| Q15 | Checked arithmetic: overflow traps by default; wrapping builtins (L10, tranche 3); `fibc build` at `-O 2` and `fibc run` at `-O 0` (C6); the rotate-and-xor `hash-combine` in tranches 0 and 1 so no combiner traps; the integer hash replaced by a 64-bit finaliser in one commit with types §2.12 and the cases' expected orders | 2026-10-01, by the rule | §2.7, §2.8, §7 L10, C6 |
| Q24 (L19) | An integer literal whose value is exactly representable adopts a float type when it unifies with one; a variable never does through this rule | 2026-10-01, by the rule | §2.8, §5 C1, §7 L19 |
| Q28 | `(first xs)` on a `List` returns `(Option e)`; case 01 (one of the 20 owner-decided cases) and case 61 are ported to it | 2026-10-01, by the rule | §6.5, §8.3 |
| Q30 (L16) | An impl's context and a method's own variable may name a type variable that a constraint determines from the head | 2026-10-01, by the rule | §2.1 rule 5, §7 L16 |

Q14 (push as the primitive, with a pull `Cursor`) was recommended "conditional on L16"; L16 is decided, so the push design stands without the condition:
pull is cheaper today (3 indirect calls against 6) and runs `f` twice per surviving element (1500 calls for 1000, A10 d1c); push is one loop
per source, early exit is a value, and its bufferless static form needs L16 ([R] A10 push).

### 9.2 What the rule does not settle

| # | Question | Recommendation | Why |
|---|---|---|---|
| Q33 | Does the owner exempt any of the sharp edges that the rule replicates, because they are bugs and not ergonomics? The eight: `compare` built on `<` (a NaN compares 0 against everything, so a float `sort` with a NaN is input-order dependent), `compare` of vectors by length first, `(take-nth 0 c)` repeating the first element, `(range 0 1 0.1)` accumulating, a step of 0 repeating the start, `update-keys`/`map-invert`/`rename-keys` keeping the last of a collision, `partition` dropping an incomplete group, and `(hash 0.0)` against `(hash -0.0)` if Clojure's differ (not verified) | **Replicate all of them (the rule), except that `Hash`/`Eq` agree on `-0.0` whatever Clojure does** (the HAMT must find a stored key), and let the owner strike any other row; each is a one-line change either way | none is a memory-safety matter (a merge sort over arrays reads only checked indices, [R] A11 nan); the two auditors flagged them "bug-compat" and left the call to the owner; the cost of replicating `compare` is the nondeterministic float sort of §2.7 |
| Q34 | Memoised or recipe: is the default for **every** adaptor a memoised `LSeq`, as Clojure's lazy seqs are cached? | **Recipes for pure adaptors, memoised `LSeq` for sources that run effects (§2.1 rule 2), and the checker inserts `cache` where the use count of a recipe variable exceeds one** (the L13 type attribute plus the ownership checker's use count), so a single-use pipeline stays at the loop a Rust iterator chain makes and a reused one behaves as Clojure's | the rule says Clojure (the only observable difference is an effectful `f`: 6 calls where Clojure makes 3, [R] A11 t20) and the other aim says zero cost; a memoising default costs a `Lazy` cell per element ([R] A11 t21); the use-count insertion is [sketch]; the two audits agreed that this is the one collision of the two aims |
| Q35 | Exceptions: amend types §2.11 (abort-only, **Decided** 2026-09-28) so that `throw`/`try`/`catch`/`finally` exist? | **Yes, after the library is viable**: `ex-info` as data now, a task's trap isolated by `join` next, unwinding last (§2.10, L28), and the types amendment written before the work | no memory-safety failure prevents them (a caught exception that skips releases leaks, §1.1); the abort model was chosen for simplicity before the rule; unwinding is a large change in lIR, the ownership checker and `fibref`, and cannot be prototyped without editing them; ported Clojure code that catches cannot run until it exists |
| Q36 | Do the numeric builtins promote for variables (`(+ n 2.5)` with `n: i64`), lifting types §1.1 D3? | **Yes, operator-level along `i8 < i16 < i32 < i64 < f64`, `f32 < f64`, never narrowing, never at `let`, return or field (§2.8, L26)**; a generic `(add n 2.5)` still fails | Clojure promotes at every mixed operation, the lattice is statically typable and has no safety content, and the owner already accepted the literal case; today `(+ n 2.5)`, `(< n 2.5)`, `(f 2)` for `(f x: f64)` and `(/ (reduce-sum xs) (count xs))` into an `f64` are `cannot unify f64 with i64` ([R] A11 n1, n10, n11, t85); Rust's answer (no) is the other defensible one; the auditors split (one recommended lifting D3, one kept it and left the call to the owner) |
| Q37 | Character offsets in a UTF-8 `str` are O(n) (`count`, `nth`, `subs`, `index-of`). Take the ASCII flag in the `str` header (C10)? | **Yes**: computed in the validation pass that already scans every constructed `str`, it makes the four O(1) for ASCII text and a byte walk otherwise; the byte layer (`str-len`, `str-byte-at`, `str-find`, `str-slice`) stays for tokenizers | the rule says the unit is Clojure's characters and cost is not a reason (§2.9); the aim of Rust-class speed says an O(n) `count` is not acceptable in a loop; the flag is a representation change in both tools and not a language change |
| Q38 | Sequencing the reader changes: `#(..)`, `#{..}` (E8) and the rest of Clojure's reader (E14, including the comma as whitespace and `~x`), against M6 step 1 (the reader port) and the 218 pinned reader tests | **Land E8 and E14 in one commit after M6 step 1 closes, extending the reader dump of spec/bootstrap.md §2 once, and migrate every `,x` of `lib/`, `compiler/` and the cases in the same commit** | the rule requires Clojure's text; each reader change costs two readers (Rust and fibber) and the churn of the macros; doing it once avoids doing it twice |
| Q39 | Multi-parameter dispatch (L23), a type-system change, or the checker rules (L20, L24) alone? | **Take L23**: it removes the rest of the overlaps (A11 t70, t71) as library code, with `(into {} [[1 2]])`, `merge` with `nil`, `str/replace` by replacement type and `(Option bool)` predicates; **keep L20 and L24 as checker rules anyway** because they are cheaper and give better messages | the two auditors named it "the largest lever" (one) and "expensive, with a checker rule for each use" (the other); the rule needs the uses, not the mechanism; it costs an instance lookup keyed by several heads and an overlap check on the product |
| Q17 | Closure types (C1) or fusion macros? | **C1; no fusion macros.** | a fused `->>` covers only a visible literal pipeline, costs 80 to 95 ms per distinct macro, and is Rust-side code that stage 2 must re-implement; C1 serves every call site |
| Q18 | Does `Vec` become a struct with a spare-capacity tail, held in a cell and updated through `set-field!`? | **Yes, decided before tranche 3, not after its counts**: the struct `Vec` plus C2 and C3. C7 (moving a payload out of an owned shell) is the alternative that keeps the enum, and is a medium ownership-checker change that also serves `Map`. | the evidence moved: C2 to C4 alone do not make the existing `Vec` update in place (2002 objects for 1000 puts on an owned struct with an array field, A10 own), a struct held in a cell updates in place today (1000 `set-field!` allocate nothing), and a required count case cannot wait for a measurement that §2.5 already predicts; the layout is shared by `fibc` (`rt/vec.lir`, `lower/pattern.rs`, `macros/abi.rs`) and `fibref` (`eval/vecs.rs`), so both change together. Subvectors and `rest` want an offset field in the same struct (`Slice`, §2.1 rule 8) |
| Q19 | Which macros are Rust? | **The thirty-two of §6.3 (`comp` and `partial` leave for the checker; `defn`, `defn-`, `some`, `update` join), then shrink the list once E2 lands.** | stage 2 pays for each Rust macro; every program pays 80 to 95 ms for each fibber macro it uses, and these are on every program's path |
| Q26 | A string builder (ROADMAP M7 rule 4: `str` building without quadratic copies) | **`StrBuf` in `fib.string` over a growable `(Array i8)`, after C4; until then `str/join` and `format` build in one pass.** | `str-concat` in a loop copies (4006 objects for 1000 characters, [R] A11 e3f); the compiler's own `hex-upper` and reader dump need building text; `with-out-str` is built on it |
| Q27 | `:use` collisions: the first `:use` wins, `spec/syntax.md` §5 says an error | **Make the checker raise the error §5 documents, with a case; until then the disjointness test of §6.2 is the protection.** | spec and code disagree (A10 use); this page changes neither, per method.md |
| Q29 | `format` directives with width, fill and case (`%04X`) in tranche 1 | **Yes: `%s %d %f %x %X` with width and zero fill in tranche 1, the rest of `format` and `printf` in 4.** | ROADMAP rule 6 puts formatting for diagnostics first, and the reader dump's `{:04X}` and `U+0041` are `compiler/util/text.fib`'s `hex-upper` today |

## 10. Review record

Three critics read this page against the compiler: ergonomics (21 findings: 30 Clojure idioms written beside
the page's API), compilability (19: every code block and every claim about the compiler re-derived) and
consistency (26: the tables against each other, the survey, the repository and the other specs). The verdicts
are **accepted** (the page changed), **accepted in part** (the page changed and the rest is rejected, with the
reason), **moved to §9** (the change needs the owner, with a recommendation) and **rejected** (with a reason). A
finding's "what changed" names the section; the programs that decided it are in A10. Where a finding repeats
another critic's it points to it. Nothing in the repository was edited: this page is the only file written, and
two spec/code disagreements it found are reported (§9 Q27, §7 B4), not resolved. **§10.1 to §10.3 are the records of those reviews and cite the first version's §5 row numbers and §9 question numbers**; §10.4 is the later review against the owner's tie-breaker, which changed most of what they left standing, and §10.4.2 and §10.4.3 map the first version's numbers to this page.

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

### 10.4 Review against the rule

When the owner gave the tie-breaker (ROADMAP M7 rule 0, §1 P0), two auditors classified the page against it independently, each
running programs to test the reasons the page gave (§A11); a third agent decided where they differed and rewrote the page. **Auditor a**
presumed the page's reasons held until the rule contradicted them: 713 rows (every `adapt`, `rename`, `replace` and `omit` name of §4, 82 §5 rows,
11 principles and non-goals, 42 design choices, 11 naming rules, 32 questions), **KEEP 172, REVERSE 271, ADAPT 270** (158 agree with the page,
112 change it), so the page changed in 383 of 713 rows; flags: bug-compat 16, type-system 61, expensive 42; 52 programs. **Auditor b** presumed
every deviation wrong until a failing program showed its memory-safety reason: 871 entries, **KEEP 138, REVERSE 216, ADAPT 219, SAME 298**; of the 573
that deviate or are not offered, 138 stand (24%): **1** on a memory-safety reason (a global `Cell` or `Weak`), **2** on the ownership model's no-leak
promise (`future-cancel`), 21 on the JVM or Clojure internals and about 114 on static typing already decided. Both found that **almost nothing the page
gave as a reason was a memory-safety reason**: the one the page did give, for `add-watch`, is false ([R] A11 t42, e18: 50 and 42). The 82 deviation rows of the
first version were classified one by one; the tables below record every item that changed, with what it was, what it is and why. An **outcome** is the first word of
"Now": *reversed* (Clojure's behaviour, no deviation left), *adapted* (Clojure's text with a typed twist, or a deviation narrowed), *kept* (the deviation stands, with
its reason in §5), *same* (the page already did what Clojure does), *decided* (the owner decided it).

#### 10.4.1 Principles and non-goals

| Item | Was | Now | Reason |
|---|---|---|---|
| P1 | Clojure's names first; a deviation where §5 says so, with the reason | adapted. P0 states the rule; P1 is "names, shapes, argument order and behaviour"; every §5 row carries a failing program or a decided typing fact | the clause "unless §5 says otherwise" was used for cleanliness, speed and one-way reasons that the rule does not accept (§1.1) |
| P2 | zero-cost: recipes, never a lazy cell, nothing per element | adapted. recipes for pure adaptors; sequences whose elements come from effects are memoised (`LSeq`); `cache`, `doall`; §9 Q34 asks about the default | a counting `map` traversed twice makes 6 calls where Clojure makes 3 (A11 t20); the memoised form runs, 5 calls (t21); cost is not a reason |
| P3 | persistent, in place when unique; no transient API | adapted. the same, and `transient` `persistent!` `conj!` `assoc!` `dissoc!` `disj!` `pop!` exist as identity wrappers | Clojure's text must resolve; `(persistent! (conj! (conj! (transient []) 1) 2))` is `[1 2]` (A11 t60) |
| P4 | unboxed elements, a real hash | same. plus: a `Map` of at most 8 entries keeps insertion order (§2.7) | an implementation property, invisible in Clojure programs |
| P5 | types, not truthiness; `or`/`and`/`when` take `bool` only | reversed. a condition accepts `bool` or `(Option T)`; `or`/`and` typed by operands; one-armed `when` is `unit` or `(Option T)` | memory-safe, typed and free at run time (A11 t04, e1z); the rule says Clojure; other types as conditions stay errors (no blanket impl, t06) |
| P6 | one way: no aliases, no run-time predicates | reversed (predicates kept). every Clojure name is offered, as a function, macro or alias; run-time predicates stay out (§5 T3) | "one way" is not a reason; a predicate needs run-time type information the monomorphised program does not carry |
| P7 | no dynamic vars, no global random generator, no metadata, no top-level atom | reversed except a global `Cell`/`Weak`. top-level atoms, `binding`, global `rand`, `defmulti`, metadata are offered (§2.11) | none breaks memory safety; a global `Cell` is reachable from every task (§5 M1, A11 t41) |
| P8 | ships on today's compiler; items the owner signs separately | same. the list of signed items grows to E3 E4 E8 E14 L14 L19 L20 to L26 | the rule requires them (§7) |
| P9 | every function has an executable test | same. §8.1 item 8 makes the rule's reasons executable | method.md |
| NG1 | no `eval`, no reflection, no arbitrary precision, no ratios, no Java interop | adapted. `eval` and reflection kept (§5 T3); `BigInt`, `Ratio`, `BigDecimal` are library types (§2.8); the Java static names are modules (§4.16) | an explicit big or rational type breaks neither safety nor typing (A11 t50); only auto-promotion has a value-dependent result type (§5 T5) |
| NG2 | does not hide cost; re-traversing a recipe re-runs it | same. `count` says whether it is O(1) or a walk; the second half now applies to pure recipes only | documentation of cost is not a deviation |

#### 10.4.2 The 82 deviations of the first version's §5 (row numbers are the first version's)

| Row | Was | Now | Reason |
|---|---|---|---|
| 1 | nil is Option's variant; `(conj nil x)`, `(get nil k)` type errors | adapted. kept as T1; `(count nil)`, `(first nil)` work; a literal `nil` first argument of `conj assoc merge` expands; `(get (get m :a) :b)` after L16; a condition accepts `Option` | typing (types §1.5); the twin keeps Clojure's text (A11 n7, n9, t23) |
| 2 | `first last peek get find max-key` return `Option` | kept. T1; `first` of a `List` is **Decided** (Q28) | typing; `Option` is a condition now, so `(if (first xs) ..)` is Clojure's text |
| 3 | `(some pred c)` renamed `any?`/`find-map` | reversed. `(some pred c)` returns the first truthy value; `any?` is Clojure's constantly-true; `find-map`, `find-first` stay as extras | the clash with the constructor is the arity (L1), not a reason (A11 n13, some) |
| 4 | `if and or when` take `bool`; `(unwrap-or o d)` is `(or o d)` | reversed. L20 | memory-safe, typed, free (A11 t04, e1z) |
| 5 | `when`, `when-let` return `unit` | reversed. `unit` or `(Option T)`; `cond` falls off the end as `nil` | `(keep #(when (even? %) ..) xs)` is Clojure's commonest line (A11 t82, t83) |
| 6 | no `next`; `rest` is `(drop 1 c)` | reversed. `next` and a closed-rest `rest` over `Seqable` | the recursion idiom type-checks without polymorphic recursion (A11 seq1) |
| 7 | `contains?` is `Map` and `Set` only | reversed. `Vec` and `Slice` tests an index | "the inconsistency ROADMAP rule 1 names" is not a reason; one impl (A11 e6) |
| 8 | `nth` traps; `(get v i)` is `Option`; vectors, maps, sets, keywords not callable | adapted. `nth` and `get` as before; callables by L21 | only-functions-are-callable is a simplification, not a typing necessity (A11 t51, k1, k2) |
| 9 | `count` is `size`; `str` is not a collection; three units | reversed. `str` is a `Reducible char`; `count` walks adaptors and calls `f`; the unit is the scalar (§5 M4) | both units are memory-safe (A11 e3a, e3b, t12, t13); the one failure is UTF-16 surrogates |
| 10 | recipes re-run; `LSeq` is the memoising type, tranche 5 | adapted. effect-sourced sequences memoised; `LSeq` in tranche 2; Q34 | a leak is not unsafe (A11 t22); the 6-against-3 calls (t20) is a cost reason |
| 11 | `(reduce f c)` returns `Option`; `reduced` is `Done` for `reduce-while` | reversed. `reduce` returns `e`, traps on empty except the identity table; `reduced` inside a literal `fn`; `reduce1` the `Option` form | a macro rewrites the body, so `f`'s type is unchanged (A11 e20, t81) |
| 12 | `map`/`zipmap`/`interleave` second operand `Cursable`, an adaptor not | adapted. every adaptor `Cursable` by buffering; L16 (**Decided**) removes the buffer for four | it is not a safety rule (A11 e16) |
| 13 | `partition` drops the incomplete group | same | Clojure's meaning |
| 14 | `(= 1 1.0)` a compile error; no `==` | adapted. `==` alias; `(= 1 1.0)` is true by L19 (§5 C1); variables Q36 | the owner's L19 |
| 15 | `(= [1] '(1))` a type error; `seq=` | reversed. the checker elaborates `=` within a family (L24) | equality inside a family is static; Clojure's answer (A11 n4) |
| 16 | `compare` total on `f64`; `Ord` lexicographic | reversed. `compare` on `<`; vectors by length first; Q33 | not safety matters; a merge sort over arrays is safe under any comparator (A11 nan) |
| 17 | a comparator is `(fn (e e) i64)`, never a predicate | reversed. `Cmp` on the result type | A11 t17, e4: `(sort < xs)` runs |
| 18 | `max-key` last of equals | same | Clojure's |
| 19 | `sorted-map-with` for `sorted-map-by` | reversed. Clojure's names | "one suffix, one meaning" is not a reason |
| 20 | `conj` per type; `(conj nil x)` type error | adapted. as row 1; literal `nil` expands | typing (A11 n7) |
| 21 | `(assoc v i x)` requires `i < count` | reversed. `i = count` appends | one line (A11 e6) |
| 22 | `List` variants renamed `Empty`, `Cons` | same. the variants are internal; `empty` and `cons` are Clojure's functions | not a deviation from Clojure behaviour |
| 23 | `get-in` family macros over a literal path | kept. T4; a runtime path over `Val` is a function | a runtime path over nested maps is an infinite type, `cannot unify v with (Map str v)` (A11 n12); `Val` runs (e17) |
| 24 | maps and vectors hold one type | kept. T4; L25 and `Val` are the twins | `cannot unify i64 with str` (A11 k8, t91, t92) |
| 25 | `(:k m)`, `(m :k)` not adopted | reversed. L14 and L21 | a coercion at call heads; static typing is not violated |
| 26 | map entry is a `Pair` | kept. T4; `[k v]` patterns by L3b; L25 literal | one element type per `Vec` |
| 27 | `keys`/`vals` of empty return an empty recipe | kept. T1; `(seq (keys m))` is the `Option` | a union of seq and `nil` is a dynamic type |
| 28 | `Show` of a `Map` in key-text order | reversed. insertion order up to 8 entries, iteration order after | "no implementation detail to depend on" is not a reason (A11 e13) |
| 29 | `Show`/`Debug`; `str` and `println` agree; nil as `(some x)` | reversed. `ToStr`, `Show`, `Debug` with Clojure's texts | A11 e11, t87; none is a safety matter |
| 30 | `merge` on `Map`s of one type, never `nil` | adapted. a literal `nil` operand is skipped; `Option` operands after L16 | typing; A11 t23, t61b |
| 31 | `into` needs `Collection` and `Reducible` | kept. a bad element type is a compile error | static typing is a gain |
| 32 | `update-keys` collisions trap | reversed. the last wins | no safety reason; Q33 |
| 33 | `range` integer and exact; step 0 traps | reversed. Clojure's accumulating `range`; step 0 repeats; `frange` the extra | determinism is not a reason (A11 range) |
| 34 | one numeric type; overflow traps; `/` truncates; no ratios; `(+)` an error | adapted. `(+)` is 0; ratios and `BigInt` are libraries; overflow traps (Q15 **Decided**); `/` truncates (T5); mixed numerics L19, Q36 | A11 t50, n1, n3c |
| 35 | `round` half up; checked conversions | same | Clojure's |
| 36 | `same?`; `compare-and-set!` by `Eq` | reversed. `identical?`; pointer compare-and-swap for objects | the primitive an atom has |
| 37 | `rand`, `shuffle`, `rand-nth` take an `Rng` | reversed. global generator; `rng/` for the explicit form | not a safety matter; an `Atom` is safe |
| 38 | `#()` without `%&`, nesting an error | kept. `%&` needs a variadic function type (T2); nesting is Clojure's error too | static typing |
| 39 | `case` no match a compile error | reversed. traps `case: no matching clause` | Clojure throws; stricter is not a reason |
| 40 | `cond`, `condp`, `case` all trap or fail | reversed. `cond` is `nil`; `condp` and `case` trap | Clojure's per-form rules |
| 41 | `str` and `println` agree | reversed. as row 29 | A11 e11 |
| 42 | `some->` steps return `Option`; `cond->` bool tests | adapted. Clojure's text via L22 and L20 | typed twist; a library cannot tell the two step kinds (t06) |
| 43 | `swap!` may rerun `f` | same | Clojure's |
| 44 | `xf` composes transducers; `comp` is function composition | kept. T9; after C1 `comp` dispatches | an impl head on a function type is rejected (A10 comp) |
| 45 | `Xf` is a factory of steppers | kept. T9 | `let` does not generalise |
| 46 | `mapv`, `filterv` only | same | Clojure's |
| 47 | `-by`/`-key`/`-with` zoo | reversed. Clojure's names as Clojure has them (§3 N4) | one suffix, one meaning is not a reason |
| 48 | `dorun`, `doall` vanish; an unconsumed recipe an error | reversed. aliases; L13 a warning | Clojure accepts `(map println xs)` as a statement |
| 49 | duplicate literal keys rejected | same | Clojure's reader |
| 50 | `subvec` shares and holds a count; a `Slice` | adapted. `Slice` is `Collection`, `Assoc`, `Stack`, `Keyed` | so `(conj (subvec v 0 2) x)` works |
| 51 | `peek`/`pop` per type; `pop` of empty traps | same | Clojure throws |
| 52 | `split-with` one pass | same | unobservable for a pure `p` |
| 53 | `str/split` keeps trailing empties and takes a literal; `replace` three functions | reversed. drops trailing empties; takes a `Regex` or `str` (`Pattern`); `replace` by match type; `replace-with` for a function until L23 | round-trip with `join` is not a reason |
| 54 | `index-of` a byte offset | reversed. character offset, `Option` | A11 e3b |
| 55 | `upper-case` vs `upper` | same | different functions of different types |
| 56 | regex non-backtracking | adapted. plus a backtracking matcher with a step budget (T5) | no safety reason; a stack overflow is a crash |
| 57 | `format` a macro with a literal string | adapted. a non-literal checks at run time | Clojure's run-time `format` |
| 58 | `defstruct` with `with` | adapted. `defrecord`; `(assoc r :k v)` and `(:k r)` by L21 (A11 e14) | a record is a map in Clojure; a struct has no extra keys (T4) |
| 59 | no run-time predicates | kept. T3 | A11 n16 |
| 60 | loop variables typed | same | n/a |
| 61 | `sorted-set-by` mixed key type cannot occur | same | static typing |
| 62 | `transient` omitted | reversed. identity wrappers | A11 t60 |
| 63 | `defn` renamed `defun`, overloaded by `:arity` | reversed. `defn` in Clojure's shape, no `:arity` marker | A11 e10: `defn` runs as a macro |
| 64 | `apply`, `trampoline`, `letfn`, `binding`, `eval`, `meta`, `type`, `satisfies?` omitted | adapted. `apply` L22, `letfn` C8, `binding` L29, `meta` L27 offered; `trampoline`, `eval`, `type`, `satisfies?` kept omitted (T3, T8) | each has its row in §4 |
| 65 | `Hash f64` hashes `-0.0` as `0.0` | kept. the contract (`=` implies equal hashes); Q33 | whether Clojure differs was not verified |
| 66 | `pmap` eager, one task per element | adapted. chunked in T3, lazy in T5 | results identical; not a deviation to defend (A11 e15) |
| 67 | `(vec x)` returns `x` | same | Clojure's |
| 68 | three argument-order conventions | same | Clojure's |
| 69 | slicing functions never `nil`; a negative count traps | reversed. a negative count acts as 0, as `(take -1 c)` is `()` | no reason |
| 70 | `char` distinct from a one-char string | same | Clojure's |
| 71 | `[a b]` matches exactly two; refutable binding traps | adapted. in a binding position a prefix, as Clojure's; `match` keeps exact shapes | a missing element has no `nil` of its type |
| 72 | the result type of sequence functions | same. recipes, `Vec` for owned results, `Seqable` for `seq`/`rest` | a statement of the design |
| 73 | `(show :k)` is `:k` | same | Clojure's |
| 74 | `update` hands `f` the value; `update-or`, `update-opt` | adapted. a macro: extra arguments as Clojure's, a literal `fnil` routed to `update-or` | A11 t24: 6, 107, 15 |
| 75 | `juxt` Pair/Triple; `partial` one free; `constantly` unary | adapted. arity-reading forms (L22) | the arity is in the function's type (A11 arity..arity4) |
| 76 | `max-key`, `min-key`, `distinct?` take a collection | adapted. a macro nests the varargs text | static typing: no variadic callee |
| 77 | `subs` byte offsets; `(str nil)` a compile error | reversed. character offsets; `(str nil)` is `""` for a literal | A11 e3b, e11 |
| 78 | `[k v]` is a `Vec`; `(into {} [[1 2]])` a type error | adapted. L25 makes a heterogeneous literal a `Pair`; the homogeneous case needs L23 | one element type per `Vec` |
| 79 | `(disj m k)` and `(dissoc s k)` type-check | same. a lenient superset | no program Clojure accepts is rejected |
| 80 | `replace` takes a `Map`; `subvec` a `Slice`; `subseq` a `Cmp` enum | reversed. `replace` over any `Lookup` (a `Vec` too); `subseq` takes `<` `<=` `>` `>=` as values; `Slice` adapted | all feasible |
| 81 | `(if flag (filter p v) v)` a type error; `seq-of` | adapted. L24 erases at the join | static typing (T8) with a checker rule |
| 82 | `defstruct` derives nothing unless asked | reversed. derive `Eq Ord Hash Show Debug ToStr` by default | a record is `=`, `hash` and printable (Q22 settled) |

#### 10.4.3 The 32 questions of the first version's §9

| Q | Was | Now | Reason |
|---|---|---|---|
| Q1 | first/last/peek/get/find return `Option` | kept. T1; Q28 **Decided** | typing |
| Q2 | `List` stays; variants `Empty`, `Cons` | same | internal names |
| Q3 | `str` is not a collection; bytes | reversed. §2.9 | cost is not a reason; A11 e3a, e3b |
| Q4 | `compare` -1/0/1; a predicate is not a comparator | reversed. `Cmp` | A11 t17 |
| Q5 | reject duplicate literal keys | same | Clojure's reader |
| Q6 | `update-keys` and `map-invert` collisions trap | reversed. the last wins | Q33 |
| Q7 | `f64` is `Hash`; `-0.0` as `0.0` | kept. the contract; Q33 | unverified against Clojure |
| Q8 | `round` half up | same | Clojure's |
| Q9 | `#(..)` `#{..}`, no regex literal | adapted. `#"re"` joins them (E14) | a reader rule like `#{}` |
| Q10 | `Show`/`Debug`; variadic `println` | adapted. three printers | A11 e11 |
| Q11 | `(:k x)` by the narrow checker rule L14 | adapted. L14 plus L21 | a coercion |
| Q12 | `-by` key, `-key` max/min, `-with` comparator or combiner | reversed. Clojure's own | §3 N4 |
| Q13 | recipes that re-run, `LSeq` in tranche 5 | adapted. split into Q34 | the collision of the two aims |
| Q14 | push with a pull cursor, conditional on L16 | decided. L16 is Decided, so unconditional | §9.1 |
| Q15 | checked arithmetic and the hash | decided. owner, 2026-10-01 | §9.1 |
| Q16 | bracket forms additive; flat `cond` | same | Clojure's |
| Q17 | closure types, no fusion macros | same. stays in §9.2 | implementation |
| Q18 | `Vec` a struct with a spare tail | same. stays in §9.2 (offset field added) | implementation |
| Q19 | which macros are Rust | adapted. thirty-two | §6.3 |
| Q20 | heterogeneous data: a `Val` enum | kept. T4; L25 adds the literal | static typing |
| Q21 | transducers as `Xf` with a flush | kept. T9 | `let` does not generalise |
| Q22 | derive `Eq Ord Hash Show Debug` by default | same. settled; `ToStr` added | a Clojure record is `=` and printable |
| Q23 | `def` initialisers; no top-level atom | reversed. any expression, atoms allowed, `Cell`/`Weak` excluded | only the `Cell` is a safety matter (§5 M1) |
| Q24 | an integer literal adopts a float type | decided. L19, owner, 2026-10-01 | §9.1 |
| Q25 | `try-let`, no early return | kept. an addition (Rust's `?`); exceptions are Q35 | no early return is a language decision (types §6.3) |
| Q26 | `StrBuf` | same. stays in §9.2 | an addition |
| Q27 | `:use` collisions | same. stays in §9.2 | spec and code disagree |
| Q28 | sign off the port of case 01 | decided. owner, 2026-10-01 | §9.1 |
| Q29 | `format` directives in tranche 1 | same. stays in §9.2 | sequencing |
| Q30 | L16 | decided. owner, 2026-10-01 | §9.1 |
| Q31 | `Pair` replaces `Entry` | kept. T4 | one tuple type; overlap (A11 t71) |
| Q32 | `update` hands `f` the value | adapted. a macro with `fnil` routing and extra arguments | A11 t24 |

#### 10.4.4 Design choices of §2

| Item | Was | Now | Reason |
|---|---|---|---|
| §2.1 r1 | `str` is not a source | reversed | A11 e3a, e3b |
| §2.1 r2 | adaptors are re-running recipes | adapted. memoised for effect sources | A11 t20, t21 |
| §2.1 r3 | consumers stop through a `bool`; `reduced` read by `reduce-while` | adapted. `reduced` also inside a literal `fn` | A11 e20 |
| §2.1 r4 | `count` is `size`; `(count (map f c))` does not call `f` | reversed. an adaptor does not override `size` | A11 count |
| §2.1 r4 | `nth`, `last`, `to-vec` are methods with walking defaults | same | Clojure's `nth` on a seq walks |
| §2.1 r5 | second operand of `zip`/`map` is `Cursable`; an adaptor is not until L16 | adapted. buffering impls now | A11 e16 |
| §2.1 r7 | transducers are `Xf` factories | kept. T9 | types §2.4 |
| §2.1 | push as the primitive, pull for lockstep | same | an implementation choice; L16 Decided |
| §2.1 | a recipe is a type: no joins; `seq-of` | adapted. L24 erases at a join | static typing with a checker rule |
| §2.1 | a colour parameter per stored closure | same | a closure that captures a `Cell` cannot cross a task (A11 t41) |
| §2.2 | closure types and conventions | same. plus arity-reading combinators | the arity is in the type |
| §2.3 | `Associative` split into `Lookup`, `Assoc`, `Dissoc`, `Keyed` (not on `Vec`) | reversed in part. `Keyed` on `Vec` | A11 e6 |
| §2.3 | stand-in names deleted when L1 lands | adapted. `update-or`, `reduce1` stay as the typed twins | they are not stand-ins |
| §2.3 | `Unit` witness; conversions checked | same | static typing; Clojure throws too |
| §2.4 | `or` takes `bool` only | reversed | L20 |
| §2.4 | `if-let` over `Option` | adapted. over any `Truthy` | L20 |
| §2.4 | `when-let` is a statement | reversed | one-armed rule |
| §2.4 | `some->` steps return `Option` | adapted | L22 |
| §2.4 | `Show` of an `Option` is `nil` or `(some x)` | reversed | A11 t87 |
| §2.5 | no transient API | adapted. identity wrappers | A11 t60 |
| §2.6 | `[a b]` exact; `{:keys}` on a struct; no `:or` | adapted. prefix in binding positions; `:keys :strs :syms :or` | Clojure's rule |
| §2.7 | `=` one type; `(= 1 1.0)` error; no `==` | adapted | L24, L19 |
| §2.7 | `compare` total on `f64` | reversed | A11 nan |
| §2.7 | `Ord` on sequences lexicographic | reversed. vectors by length first | Q33 |
| §2.7 | `Hash f64` hashes `-0.0` as `0.0`; `hash-combine` rotate-xor | kept | the `Hash` contract; Q15 **Decided** |
| §2.7 | `Show` display, `Debug` readable; str = println | reversed. three printers | A11 e11 |
| §2.7 | `Show` of a `Map` in key-text order | reversed | A11 e13 |
| §2.8 | `(+)` is a compile error | reversed | L19 |
| §2.8 | no integer-to-float literal adoption | decided. L19 | §9.1 |
| §2.8 | no promotion for variables | kept pending Q36 | D3; §9 Q36 |
| §2.9 | `str` not a collection; byte offsets; `split` keeps trailing empties; `replace` literal | reversed | A11 e3a..e3e |
| §2.9 | `Chars` decoder; `parse-double` grammar | same | implementation |
| §2.10 | `Result` and `try-let`; no `ex-info`, `try`, `catch`, `finally` | adapted. `Result` stays as Rust's addition; exceptions planned (Q35) | no safety reason (A11 t40) |
| §2.10 | `Rng` value, no global generator | reversed. global plus `rng/` | an `Atom` is safe |
| §2.10 | function twins of `println`, `str` | same | matches Clojure |

#### 10.4.5 Naming rules of §3

| Rule | Was | Now | Reason |
|---|---|---|---|
| N1 | sequence last, collection first | same | Clojure's |
| N2 | function first | same | Clojure's |
| N3 | listed exceptions; `includes?` element first | same | `includes?` is marked an extra |
| N4 | `-by` key, `-key`, `-with` comparator or combiner; `sorted-map-with` | reversed. Clojure's own suffixes | "one suffix, one meaning" is not a reason |
| N5 | `!` suffix | same. plus `conj!` | Clojure's |
| N6 | conversions `from->to`; casts name the target first | same | both exist |
| N7 | recipes named for what they do; eager twins only `mapv filterv` | same. plus `doall` | Clojure's |
| N8 | types CamelCase; adaptors past participle | same | internal |
| N9 | `join` never redefined | same | Clojure's `clojure.string/join` is qualified |
| N10 | signature notation | same | documentation |
| N11 | `Option` functions take the function first | same | additions |

#### 10.4.6 The names of §4, by what happened to them

| Names | Was | Now | Reason |
|---|---|---|---|
| names the first version renamed (47) | `rename`: fibber's spelling instead of Clojure's | reversed. Clojure's name is the name (§3 N12); the existing builtin or function stays as an alias: `aget`, `alength`, `aclone`, `bit-shift-left`, `bit-shift-right`, `unsigned-bit-shift-right`, `Long/bitCount`, `not=`, `Long/MAX_VALUE`, `Long/MIN_VALUE`, `Double/MAX_VALUE`, `aset`, `set!`, `#'x`, `Integer/parseInt`, `Long/parseLong`, `Double/parseDouble`, `identical?`, `defn`, `some`, `Character/isDigit`, `Character/isLetter`, `Character/isLetterOrDigit`, `Character/isWhitespace`, `Character/isUpperCase`, `Character/isLowerCase`, `Character/toUpperCase`, `Character/toLowerCase`, `Character/digit`, `Character/forDigit`, `print-str`, `slurp`, `spit`, `future`, `future-call`, `Thread/sleep`, `System/currentTimeMillis`, `System/nanoTime`, `System/getenv`, `System/exit`, `*command-line-args*`, `clojure-version`, `~x`, `~@x`, `not-any?`, `sorted-map-by`, `sorted-set-by` | the rename had neither a safety nor a typing reason |
| names the first version did not offer, now in §4.1: array constructors and volatiles (19) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `==`, `to-array-2d`, `object-array`, `to-array`, `amap`, `areduce`, `vswap!`, `int-array`, `into-array`, `quot`, `volatile!`, `vreset!`, `long-array`, `double-array`, `float-array`, `short-array`, `byte-array`, `char-array`, `boolean-array` | array constructors that Clojure has are one-line macros, and value semantics of writes is the one safety matter (§5 M2); volatiles are `cell` |
| names the first version did not offer, now in §4.2: macros, reader forms and global-state forms (48) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `defonce`, `lazy-cat`, `isa?`, `if-some`, `when-some`, `when-first`, `declare`, `binding`, `with-redefs`, `locking`, `macroexpand`, `macroexpand-1`, `refer`, `apply`, `bound-fn`, `:strs`, `:syms`, `:or`, `with-in-str`, `::kw`, `1N 1M 1/2`, `##Inf ##-Inf ##NaN`, `#?(:clj ..)`, `#inst "..."`, `derive`, `refer-clojure`, `alias`, `with-bindings`, `defrecord`, `->Name`, `map->Name`, `deftype`, `letfn`, `require`, `use`, `extend-type`, `extend-protocol`, `reify`, `defmulti`, `defmethod`, `with-open`, `with-out-str`, `pcalls`, `pvalues`, `^Type`, `^meta`, `with-local-vars`, `defn-` | macros over core forms (A11 t30), reader forms (E14), `binding`, `locking`, `defmulti`, `with-redefs`, `letfn`: none breaks memory safety except a shared `Cell` (§5 M1, M6) |
| names the first version did not offer, now in §4.3: numbers, exceptions as data, metadata, small predicates (57) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `unchecked-divide-int`, `unchecked-remainder-int`, `unchecked-add-int`, `unchecked-subtract-int`, `unchecked-multiply-int`, `unchecked-negate-int`, `unchecked-inc-int`, `unchecked-dec-int`, `namespace`, `symbol`, `parse-uuid`, `random-uuid`, `+'`, `-'`, `*'`, `inc'`, `dec'`, `bigint`, `bigdec`, `biginteger`, `rationalize`, `numerator`, `denominator`, `pos-int?`, `neg-int?`, `nat-int?`, `true?`, `false?`, `boolean`, `realized?`, `meta`, `with-meta`, `vary-meta`, `alter-meta!`, `reset-meta!`, `Throwable->map`, `char-escape-string`, `char-name-string`, `with-precision`, `find-keyword`, `inst-ms`, `ex-info`, `ex-data`, `ex-message`, `ex-cause`, `throw`, `try`, `catch`, `finally`, `print-method`, `unchecked-byte`, `unchecked-short`, `unchecked-char`, `unchecked-int`, `unchecked-long`, `unchecked-float`, `unchecked-double` | `BigInt` and `Ratio` as library types (A11 t50), the exact and `unchecked-*` families as aliases, `ex-info` as data, metadata as a field, `true?`/`boolean` over `Truthy` |
| names the first version did not offer, now in §4.4: trivial sequence functions and transients (27) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `fnext`, `nthrest`, `transient`, `persistent!`, `conj!`, `assoc!`, `dissoc!`, `disj!`, `pop!`, `replicate`, `vector-of`, `file-seq`, `ffirst`, `nfirst`, `nnext`, `nthnext`, `flatten`, `vector`, `next`, `reduced?`, `unreduced`, `ensure-reduced`, `splitv-at`, `partitionv`, `partitionv-all`, `dorun`, `doall` | one-line functions over `Reducible` (A11 t60), `next` over `Seqable`, `flatten` one level, `dorun`/`doall`, `transient` as identity wrappers |
| names the first version did not offer, now in §4.5: collections (1) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `array-map` | `array-map` as the small `Map` |
| names the first version did not offer, now in §4.7: strings (1) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `clojure.string/re-quote-replacement` | `re-quote-replacement`, needed once `str/replace` takes a template |
| names the first version did not offer, now in §4.9: regex (2) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `re-matcher`, `re-groups` | `re-matcher`, `re-groups` as a struct with cell state |
| names the first version did not offer, now in §4.10: math (14) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `clojure.math/add-exact`, `clojure.math/subtract-exact`, `clojure.math/multiply-exact`, `clojure.math/negate-exact`, `clojure.math/increment-exact`, `clojure.math/decrement-exact`, `Math/sqrt`, `Math/pow`, `Math/abs`, `Math/floor`, `Math/ceil`, `Math/round`, `Math/random`, `Math/log` | the `Math/` statics and `*-exact` as aliases of operations that already trap |
| names the first version did not offer, now in §4.12: walk (3) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `clojure.walk/postwalk-demo`, `clojure.walk/prewalk-demo`, `clojure.walk/macroexpand-all` | the demos and `macroexpand-all` |
| names the first version did not offer, now in §4.14: printing and reading (13) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `newline`, `println-str`, `prn-str`, `print-table`, `tap>`, `read`, `*print-length*`, `read+string`, `remove-tap`, `add-tap`, `read-string`, `*out*`, `*in*` | `println-str`, `prn-str`, `newline`, `read-string`, taps and the dynamic-var printers |
| names the first version did not offer, now in §4.15: concurrency (7) | `omit` or `replace` in §4.17 | reversed. offered, as a function, macro or alias: `seque`, `add-watch`, `remove-watch`, `set-validator!`, `get-validator`, `future-cancel`, `future-cancelled?` | `add-watch` and validators as a library (A11 e18), `future-cancel` cooperative, `pcalls`, `seque` |
| rows whose text changed, other typed twists (52) | `adapt`/`keep` with the first version's note | adapted. `make-array`, `-`, `*`, `/`, `atom`, `if-not`, `when-not`, `condp`, `case`, `fn`, `def`, `ns`, `cond->`, `:keys`, ``x`, `str`, `name`, `keyword`, `rand-int`, `compare`, `hash`, `distinct?`, `36rZZ 2r1010`, `cons`, `hash-map`, `butlast`, `subvec`, `replace`, `reduce`, `transduce`, `any?`, `reduced`, `conj`, `get-in`, `update`, `contains?`, `into`, `merge`, `rsubseq`, `comparator`, `clojure.lang.PersistentQueue/EMPTY`, `clojure.string/join`, `re-pattern`, `#"regex"`, `re-find`, `clojure.data/diff`, `println`, `pr`, `pr-str`, `compare-and-set!`, `delay`, `future-done?` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, truthiness: a predicate result is a `Truthy` (15) | `adapt`/`keep` with the first version's note | adapted. `not`, `if-let`, `when-let`, `and`, `complement`, `every-pred`, `some-fn`, `take-while`, `drop-while`, `split-with`, `every?`, `not-every?`, `filter`, `filterv`, `remove` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, arity-reading combinators (7) | `adapt`/`keep` with the first version's note | adapted. `some->`, `partial`, `comp`, `juxt`, `constantly`, `fnil`, `memoize` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, the character unit and `Pattern` (7) | `adapt`/`keep` with the first version's note | adapted. `subs`, `count`, `clojure.string/index-of`, `clojure.string/last-index-of`, `clojure.string/replace`, `clojure.string/replace-first`, `clojure.string/split` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, truthiness: a condition accepts `Option` (6) | `adapt`/`keep` with the first version's note | adapted. `if`, `when`, `cond`, `or`, `while`, `seq` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, replicated sharp edges (6) | `adapt`/`keep` with the first version's note | adapted. `take-nth`, `max-key`, `min-key`, `update-keys`, `clojure.set/rename-keys`, `clojure.set/map-invert` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, the global generator (5) | `adapt`/`keep` with the first version's note | adapted. `rand`, `shuffle`, `rand-nth`, `random-sample`, `clojure.math/random` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, memoised sequences (5) | `adapt`/`keep` with the first version's note | adapted. `repeatedly`, `lazy-seq`, `line-seq`, `iteration`, `pmap` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, numeric operands: literal adoption and promotion (3) | `adapt`/`keep` with the first version's note | adapted. `+`, `<`, `=` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, callables (3) | `adapt`/`keep` with the first version's note | adapted. `(:k m)`, `assoc`, `clojure.string/escape` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, arity overloading (3) | `adapt`/`keep` with the first version's note | adapted. `gensym`, `range`, `map` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, comparators: any function whose result is a `Cmp` (3) | `adapt`/`keep` with the first version's note | adapted. `sort`, `sort-by`, `subseq` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, every adaptor is `Cursable` (1) | `adapt`/`keep` with the first version's note | adapted. `interleave` | the rule: Clojure's text, with the typed twin the row now states |
| rows whose text changed, closed seq types (1) | `adapt`/`keep` with the first version's note | adapted. `rest` | the rule: Clojure's text, with the typed twin the row now states |
| rows that did not change (227) | `keep` or `adapt` | same. no change | the first version already did what Clojure does, or had a typed twin that the rule accepts |
| names that stay omitted (89) | `omit` or `replace` | kept. not offered, each with its reason in §4.17: Java interop and the JVM, no run-time type information (§5 T3), value-dependent or union result types (§5 T5, T8), legacy structs, the name `defstruct`, `eval`, internals | memory safety is the reason for none of them: they are the JVM or decided static typing |


#### 10.4.7 Counts

The tables above count **843 items** by outcome. Of the first version's principles, deviation rows, questions, design choices and naming rules (171 items):
reversed 54, adapted 42, kept 20, same 49, decided 6. Of the 673 survey names of §4 (672 rows; `#"..."` shares one): reversed 239 (47 renames undone, 192
names offered that the first version did not), adapted 117, kept 89 (not offered), same 227 (the row did not change). **In all: reversed 293, adapted 159,
kept 109, same 276, decided 6.** "Reversed" means no deviation remains; "kept" means a deviation stands and §5 gives its reason. Auditor a kept five rows on a
memory-safety reason (`aset`, `locking`, `make-array` of an object type, the UTF-16 unit, `future-cancel`); auditor b kept one (a global `Cell` or `Weak`) and two on the
ownership no-leak promise. §5.1 takes the union and says, for each row, which program was run, which checker rejection is quoted and which has none (M5).

**Where the two auditors differed, and how it was decided.** Re-running the deciding program is how every row below was settled.

| Item | Auditor a | Auditor b | Decision and evidence |
|---|---|---|---|
| lockstep over adaptors (`(zip xs (map inc xs))`) | adapted as the page: needs L16 | adapted: every adaptor `Cursable` by buffering, no language change | **b**: the three impls run, `[[1 2] .. [5 6]]` and the `filter` case ([R] A11 e16); L16 (Decided) then removes the buffer for four types |
| `(count (map f c))` | adapted as the page: invisible for pure `f` | reversed: realise the seq | **b**: an adaptor does not override `size`, `[2 2]` and `[3 4]` (A11 count); the O(1) was a cost reason |
| default for adaptors: memoised or re-running | hybrid: effect sources memoised, `LSeq` early | recipes plus `cache`, and use-count insertion of `cache` | **both, as the recommendation of §9 Q34**: hybrid now, use-count insertion as the faithful form at zero cost |
| promotion for variables | lift D3 for the lattice | keep D3, the owner's call | **a**, as §9 Q36 recommends: statically typable, no safety content; not signed because D3 was decided |
| `some`, `any?` | reverse: arity separates the constructor, a `Truthy` predicate result gives the value | adapt: arity (L1) and a `Truthy` `opt-of` | **they agree**: `Truthy` and `Payload` give `(some pred c)` and run (A11 some); L1 separates the constructor; `any?` is Clojure's unary |
| recursion on `rest` | a closed `Seq` protocol (t18) | `Seqable` and `Rest` (e7) | **both, merged**: `Seqable` with `seq` and `rest`; a generic `len` over `Vec` and `List` runs (A11 seq1) |
| multi-dispatch protocols | the largest lever | a checker rule per use | **both**: take L23 and keep L20, L24 (§9 Q39) |
| `trampoline` | omit: the result is a union | adapt over a `Bounce` enum | **a**: the text would differ from Clojure's, so it is not Clojure's `trampoline` (§5 T8) |
| `namespace`, `symbol` | reverse | keep: no run-time symbols | **a for both**: `namespace` splits a flat keyword at `/`, `symbol` goes through `Form`'s `Sym` |
| reader conditionals `#?` | reverse | keep: one platform | **a**: one platform is not a reason; E14 |
| `extend` | keep: a map of method functions of different types | reverse as a macro | **a**: heterogeneous map (§5 T4); `extend-type` and `extend-protocol` are offered |
| `with-redefs` | adapt as an opt-in marker | keep: static dispatch | **a**: only a `:redefinable` function pays an indirect call |
| legacy `struct-map`, `struct`, `defstruct` | keep | reverse | **a**: values of any type under keyword keys (§5 T4); the name `defstruct` is fibber's |
| `compare` of NaN | reverse (bug-compat) | reverse, owner may keep | **reverse**, with §9 Q33 for the owner; `sort` over a NaN is input-order dependent and memory-safe (A11 nan) |
| `Hash f64` and `-0.0` | reverse (bug-compat) | keep: the `Hash` contract | **b**: a lookup that misses a stored `-0.0` is the failure the HAMT must not have; unverified against Clojure |
| `future-cancel` | keep | keep, a weak reason | **adapted**: a cooperative flag, Java's own interrupt (§5 M5) |
| `into ""` | not Clojure (a `String` throws) | add it with a `StrBuf` | **a**: not offered; `(apply str ..)` and `str/join` build text |
| `(Option bool)` as a condition | not distinguished | `(some true)` is truthy, presence is wrong | **b**: `(some true)` in the checker rule; presence in predicate position until L23 (A11 e2b) |
| `clojure.data/diff` | a `Diff` struct (as the page) | a `Triple` | **b**: Clojure's vector of three, a `Triple` of `Option`s |
| `subseq` | probe `(test 1 0)` | a macro on the literal `<` | **a**: no special form needed |
| callables | a coercion at call heads (D3-like) | generalise L14 | **one rule, L21**; after C1 an `impl` per type |
| `pmap` | a pool | chunked tasks | **b** in tranche 3, lazy over an `LSeq` of futures in tranche 5 |
| `Show` of an `Option`, `str` of a collection | a `Str` protocol | a `ToStr` protocol | **both**: three printers `ToStr`, `Show`, `Debug` (§2.7) |
| `Character/isDigit` | Unicode `Nd`, as Java | alias module | **both**: `Character/isDigit` is Unicode, `digit?` the ASCII test the compiler uses |
| `letfn` | adapt: a macro over cells leaks a cycle | reverse: lower to one environment | **both**: a macro leaks a cycle (A11 t55), C8 removes it; `letfn` ships with C8 |

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

* **t1r, t10.** *(The prototype was changed by the rule review, A11 t1r.)* The revised prototype (`Reducible` with `nth last to-vec`, `Pair` as the one tuple type, `update`
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
* **nan.** *(Superseded by A11 nan: the rule makes `compare` Clojure's, built on `<`.)* `compare` over `(!= a a)`: `[(compare nan nan) (compare nan 1.0) (compare 1.0 nan) (compare 1.0
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
* **count.** *(Superseded by A11 count: an adaptor no longer overrides `size`, so `count` calls `f`.)* A `map` with a counting function over `[1 2]`: `[(count r) calls]` is `[2 0]`, `[(sum r) calls]`
  `[3 2]`; `(empty? (filter p (range2 0 100000)))` with a predicate that never passes calls it 100000 times.
* **rangesize.** `Range` with a closed-form `size`: `[10 4 4 0 0 3]` for `(range2 0 10)`, `(range3 0 10 3)`,
  `(range3 10 0 -3)`, `(range2 5 5)`, `(range2 5 0)`, `(range3 0 9 3)`, and `count` equals the length of `to-vec`
  for eight ranges.
* **cur.** *(Superseded by A11 e16: every adaptor is `Cursable` by buffering.)* `(zip-with (fn (b a) (- b a)) (rest xs) xs)` is `[3 5 7 9]` and `(zip (map inc xs) xs)` is `[[2 1] [5
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
* **sortby.** *(`desc`, the prototype's descending comparator, is `>` on this page: §2.3.)* `(sort-by key [3 1 2 5 4])` with a counting key: 5 calls; `(sort-by-with (fn (x) (rem x 3)) desc
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
* **showmap.** *(Superseded by A11 showmap: a `Map` prints in iteration order.)* Twelve integer keys inserted ascending and descending: both print `{0 0, 1 1, 10 100, 11 121, 2 4,
  3 9, ...}` (equal text); `(show (set [3 1 2]))` is `#{1 2 3}`; `(show (into (map-empty) (zip ["b" "a"] [2 1])))` is
  `{a 1, b 2}`.
* **showseq.** *(Superseded by A11 showseq: a seq prints in parentheses.)* `(show (map inc [1 2 3]))` `[2 3 4]`, `(show (take 3 (iterate inc 5)))` `[5 6 7]`, `(show (filter odd?
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

**A11. Evidence of the review against the rule.** Two auditors ran programs to test every reason this page gave (their
directories are `stdlib/rule-a/` and `stdlib/rule-b/` of the session's scratch directory, 68 and 49 files); the revising
agent copied them to `stdlib/rrevise/{a,b}/`, **re-ran every program that a claim of this page cites**, one process at a time with
`timeout 300`, `fibc run` and then `fibref run`, and wrote or changed programs in `stdlib/rrevise/p/` on a copy of the A10
prototype (`sl/`, changed in `core.fib`, `show.fib` and `seq.fib`): `range`, `arity` to `arity4`, `some`, `seq1`, `gensym`, and the A10 programs
`count`, `nan`, `showseq`, `showmap` and `t1r`. They are **not in the repository**.
Unless an entry says otherwise a program gives the same output under both tools, and `fibref` ends `audit:  clean=true
leak-cycles=0 leaks=0 errors=0`. A tag such as `t04` or `e8c` is a file of `a/` or `b/`; the new ones are named.

**Truthiness and conditions**

| Tag | Program | Output |
|---|---|---|
| t01, t02, t03 | `(if (get m 1) 1 2)`; `(or (get m 2) 7)`; `(if 5 1 2)` | `cannot unify (Option i64) with bool` (twice); `cannot unify i64 with bool` |
| t04 | `Truthy` and `Payload` protocols with macros `if2 or-d or-e and2 when2` over `bool` and `Option` | 127 under both tools |
| t05 | `filter-t` with a predicate returning an `Option` and one returning a `bool` | 4 |
| t06 | `(impl Truthy a ..)` | `an instance head must not be a type variable` |
| e1, e1z | the same as macros `if* when* and* or*` (`tr/truthy.fib`); `fibc emit` of `(f 5)` | `99`, `1`, `true`, `true`, `when ran`, `0`, `1`; the emitted `f.f` has `(call @m.Truthy.truthy?.bool t1)` and `m.Truthy.truthy?.bool` is `(ret p0)`: a direct call to an identity |
| e2, e2b | two `Or2` instances for `(Option a)`; `(if* (some false) 1 0)` | `overlapping instances: Or2 for (Option a) is already implemented at e2.fib:2:1`; `1` (presence, where Clojure's `false` is falsy) |
| t82, t83, k7 | `(when c 5)` as a value (and `(when (= 1 1) 5)`); a macro that wraps the body in `some` | `cannot unify unit with i64` (twice); 12 |
| some (p/some.fib) | `some-p` over `Truthy` and `Payload`, and `any?` as `(fn (x) true)` | `(some true)`, `(some 20)`, `nil`, `(some 20)`, `(some true)` |
| n7, n9 | `(conj nil 1)` on an `Option`; `(first nil)`, `(count nil)` | `no implementation of Collection for (Option a)`; `nil`, 0 |
| n13 | a user `(defun some (p c) ..)` then `(some 5)` | `some takes 2 argument(s), got 1` |
| n12, n15 | a `get-in` over a runtime path of nested `Map`s; `(if* 5 1 2)` over `Truthy` | `cannot unify v with (Map str v)`; `no implementation of Truthy for i64` |
| t24 | a macro `update2` that routes a literal `(fnil g d)` to `update-or` | 128 (`(update2 m "a" inc)` 6, `(update2 m "w" (fnil + 100) 7)` 107, `(update2 m "a" + 10)` 15) |
| t81, e20 | `reduced` inside a literal `fn` of a plain `reduce`, by macro | 17; `11`, `6`, `105` |
| t30, e10 | flat `cond`, `case`, `condp`, `if-some`, `when-some`, `when-first`, `defn`, `defn-`, `declare` as macros; `defn` and `let*` with `[a 1 b 2]` | 140; 13 |
| t60 | `ffirst`, `fnext`, `next`, `nfirst`, `nthnext`, `nthrest`, `dorun`, `doall`, `transient`/`conj!`/`persistent!`, `quot`, `true?`, `false?`, `volatile!` | result 0 (ten checks pass) |
| t66 | `contains?` on a `Vec` by index, `assoc` at the count, `count`/`first`/`map` of `nil` | result 0 (six checks) |
| e6 | `(impl (Keyed i64) (Vec a) ..)` and an `assoc` that appends at the count | `true`, `false`, `[10 20 30]`, `[30 20]`, then `trap: assoc: index out of range` |
| e12 | `apply` as a macro | 6, 3, 6, `312` |

**Sequences and laziness**

| Tag | Program | Output |
|---|---|---|
| t20 | a counting `map` traversed twice | 624: 6 calls, sum 24 (Clojure makes 3 calls) |
| t21 | a memoised `LSeq` (a head and a `Lazy` cell forced once) traversed twice | 520: 5 calls, sum 10 twice |
| t22 | a self-referential `LSeq` | 5; `fibref`: `audit: clean=false leak-cycles=4 leaks=0 errors=0` (the permitted leak; `fibc` prints 5) |
| e8a, e8b, e8c | a counting `map` reduced twice; a cell holding a closure that captures itself; `(cache (map f ..))` | `12 12 6`; 0 with `clean=false leak-cycles=2`; `12 12 3` |
| e8d, e8e | that memo captured by `spawn`; the same memo over an `Atom` | `cell cannot be shared between threads: closure capture s, field box of Cached has type (Cell (Option (Vec i64)))`; 6 |
| e16 | every adaptor `Cursable` by buffering (`rb/curbuf.fib`): `(zip xs (map inc xs))`, `(zip (map inc xs) xs)`, `(zip-with .. (rest xs) xs)`, `(zip xs (filter odd? xs))` | `[[1 2] [2 3] [3 4] [4 5] [5 6]]`, `[[2 1] [3 2] [4 3] [5 4] [6 5]]`, `[1 1 1 1]`, `[[1 1] [2 3] [3 5]]` |
| count (p/count.fib) | the A10 `count` program with `Mapped` no longer overriding `size` | `[2 2]`, `[3 4]`, `[1 100000]` (A10 had `[2 0]`, `[3 2]`) |
| seq1 (p/seq1.fib) | `Seqable` with `seq` and `rest` over `Vec`, `Slice`, `List`; the generic `len` of §2.1 rule 8; `next` | `4`, `3`, `0`, result 0 |
| t80 | a `Consed` recipe (`(cons 0 [1 2 3])`) and an insertion-ordered small map `OMap` that becomes a `Map` at the ninth key | `[100 93 86 79]` (keys of four insertions in order), `[0 1 2 3]`, `[65 100 72 44 79 51 86 58 93]` (nine insertions, hash order), result 0 |
| t1r (p/t1r.fib) | the 48 checks of A10 on the changed prototype: `compare` on `<`, `Vec` order by length first, `Mapped` without `size`, recipes shown in parentheses, two new checks (`sort-vec-length-first`, `count-calls-f`) | result 0 under both tools (50 checks) |
| showseq, showmap (p/) | `(show (map inc [1 2 3]))`, `(show (take 3 (iterate inc 5)))`, `(show (filter odd? (range2 0 10)))`, `seq=`; a map of twelve integer keys and `(show (set [3 1 2]))` | `(2 3 4)`, `(5 6 7)`, `(1 3 5 7 9)`, `true`; `{0 0, 1 1, 2 4, ..., 11 121}` (iteration order, the integer hash being the identity), `true`, `#{1 2 3}`, `{b 2, a 1}` |
| e13 | an insertion-ordered small map (`rb/amap.fib`): `AMap k v` up to 8 entries, then a `Map` | `{b 2, a 1, c 3}`, `{b 20, a 1, c 3}`, `(some 1)`, `{0 0, 1 1, 2 4, 3 9, 4 16, 5 25, 6 36, 7 49, 8 64}`, `[0 1 2 .. 11]` |
| nan (p/nan.fib) | `compare` on `<` over NaNs; `max`, `min`; `sort` of floats with a NaN in two orders | `[0 0 0 -1 0]`; `[false false false false]`; `[1.0 2.0 3.0 NaN NaN 0.5]` and `[NaN 0.5 1.0 NaN 2.0 3.0]` |
| range (p/range.fib) | a float range by repeated addition and by `a + i*s` | 11 and 10 elements; the eleventh is `0.9999999999999999`, the tenth of the other `0.9` |
| t17, e4 | `Cmp`/`CmpRes` on the comparator's result type; `(sort-cmp < xs)`, `>`, `compare` lambdas, `(sort-by-cmp key > ps)`; a named predicate | 0 failures; `[1 2 3]`, `[3 2 1]`, `[1 2 3]`, `[1 1 2 3 3]`, `[1 2 3]`, `[1 2 3]` |

**Strings, numbers, global state, callables**

| Tag | Program | Output |
|---|---|---|
| e3a, e3b, e3e | `(impl (Reducible char) str ..)` in user code with character-offset `subs` and `index-of`; `(into "" ..)`, `frequencies`, `sort` of a string | `5`, `(some h)`, `[97 233]`, `[a b c]`; `5`, `aé€😀z`, `é€`, `(some 3)`, `😀z`, `nil`, `[a é € 😀 z]`; `123`, `3é2b1a`, `a1b`, 6, `true`, `{1 1, 2 1, 3 1, a 1, b 1, é 1}`, `[1 2 3 a b é]` |
| e3f | `into ""` over 1000 characters by `str-concat` | 1000; 4006 objects (the last `F` line of `--trace`, auditor b, not re-run with `--trace`) |
| t12, t13 | `(str-slice "é" 0 1)`; `(str-from-bytes (array 2 -1i8))` | `trap: str-slice [0, 1) splits a character`; `trap: str-from-bytes: invalid UTF-8` |
| e11, t87, t88, t64 | a `ToStr` protocol (`rb/tostr.fib`); `(show (some 3))`, `(show (some "a"))`; `(show [1 2])`; `(show nil)` | `x=1`, `y=`, `v=["a" "b"]`, `[a b]`; `(some 3)`, `(some a)`; `no implementation of Show for (Vec i64)`; `ambiguous constraint Show a in main; add an annotation` |
| t62, t63, t85, t84 | `(= 1 1.0)`; `(* x 2)` with `x: f64`; `(/ (reduce-sum xs) (count xs))` into an `f64`; `(/ (sitofp f64 n) (sitofp f64 m))` | `cannot unify f64 with i64`; `cannot unify i64 with f64`; `cannot unify i64 with f64`; 25 |
| n1, n2, n3c, n10, n11 | `(+ n 2.5)`; `(* 2 1.5)`; `(+ x 1)` with `x: i32`; `(< n 2.5)`; `(f 2)` for `(f x: f64)` | `cannot unify f64 with i64` (four), `cannot unify i64 with i32`, `cannot unify i64 with f64` |
| t50, k9 | a `Ratio` struct with `(impl Num Ratio ..)`, `Eq` and a generic `(twice x)`; `(/ 7 2)` | 507; `3` |
| e19 | a module named `Long` with `MAX_VALUE` and `bitCount` | `9223372036854775807`, 8 |
| t56, n8, n8b | `(def counter: (Atom i64) (atom 0))`; `(def c: (Cell i64) (cell 0))`; `(def a: (Atom i64) (atom 0))` | `def .. : initialiser is not a constant expression` |
| t41, t54, t90 | two tasks writing one `Cell`; `array-set!` with a second holder; `(array 3)` | `cell cannot be shared between threads: closure capture c has type (Cell i64)`; 9 (Clojure: 99); `array takes 2 argument(s), got 1` |
| t40, e9 | `trap` in a spawned task | `trap: boom` then `Aborted` under `fibc`; `trapped:` under `fibref`, `audit:  clean at the abort` |
| t55 | two local functions that reach each other through two cells | 1; `fibref`: `clean=false leak-cycles=4` |
| t42, e18 | `add-watch` and a validator as a library: an `(Atom (Vec (fn :send (a a) unit)))` and a `(fn :send (a) bool)` | 50; 42 |
| e15 | `pmap` chunked over `k` tasks with the order kept | `[1 4 9 16 25 36 49 64 81 100]` |
| t51, n5, n6, k1, k2 | a `Map`, a `Set`, a keyword and a `Vec` where a function is expected | `cannot unify (Map i64 i64) with (fn (a) b)` (twice), `cannot unify (Set i64) with (fn (a) bool)`, `cannot unify keyword with (fn (a) b)`, `cannot unify (Vec i64) with (fn (a) b)` |
| t53, e14 | `(assoc p :x 5)` on a struct; macros `assoc-kw`, `update-kw`, `kw-get` for a literal keyword | `no implementation of Associative for P`; 30, 31, 32 |
| arity..arity4 (p/) | `(partial add3 1)`, `(comp inc add2)`, `(constantly 7)` where `(fn (i64 i64) i64)` is expected, `(partial add3 1)` called with one argument, on `sl.core`'s `partial`, `comp`, `constantly` | `cannot unify (fn :send (i64 i64 i64) i64) with (fn (a b) c)`; `cannot unify (fn :send (i64 i64) i64) with (fn (a) i64)`; `cannot unify (fn (a) i64) with (fn (i64 i64) i64)`; the first message again |
| k8, n16, n4 | `[1 "a"]`; `{:a 1 :b "x"}`; a typed `vector?`; `(= [1] (range ..))` | `cannot unify i64 with str` (k8, t91, t92); `no implementation of IsVec for i64`; `cannot unify Range with (Vec i64)` |
| t86 | a struct constructor passed as a function value | `fibc`: `unsupported: a constructor as a value`; `fibref`: `result: 2` |
| gensym (p/) | a macro that calls `(gensym)` | `gensym takes 1 argument(s), got 0` under both tools |
| t23, t61, t61b, t70, t71, e5 | `(impl (Lookup k v) (Option s) :where ((Lookup s k v)) ..)`; the same for an `AsMap` protocol; two `OrHit` instances for `(Option a)`; two `Collection` instances for `(Map k v)`; a nested head | `type variable k is not a parameter of the impl head`; `a method is (name (self qual* x: T qual*) -> type)` and `type variable m is not a parameter of the impl head`; `overlapping instances: OrHit for (Option a) is already implemented at t70.fib:2:1`; `overlapping instances: Collection for (Map k v) is already implemented at t71.fib:2:1`; `an instance head is a type constructor applied to distinct variables (or, at a colour parameter, a colour)` |
| e17 | a `Val` enum with `ToVal`, `hmap2` and a runtime `get-in-v` | `{:name "ann", :inner {:tags ["a" "b"], :x 1}}`, `(some 1)`, `(some ["a" "b"])`, `nil` |
| t10 | a `str` as a `Reducible char`: `count`, `first`, `map`, `filter`, `take`, `get`, `nth`, `subs` by character, `reverse` on `"aé€😀z"` | result 0 (nine checks) |
