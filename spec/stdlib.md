# The standard library (M7)

Status: **Proposed; frozen for implementation after the third revision (2026-10-01)**: a change after this point goes through §9 and
the owner, not through another revision of the page. The owner asked on 2026-10-01 for a library that "steals Clojure's, or as
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

**State of the tree (2026-10-02).** The page was written against `36ed472` and frozen there. Tranche 0 and the Rust half of tranche 1 have
landed since (§7.5 lists every item with its commit and its cases, §8.2 the tranches); a sentence below that says what the compiler or the
library does "today", or that something exists "until" an item lands, describes `36ed472` unless §7.5 says the item has landed, and §7.5 is
then the current statement. The sentences that landed work made false have been corrected in place; the rest are history, and a spec rule
and a code behaviour that disagree are listed in §7.5.2 and not resolved here (method.md).

How this page was made. One agent inventoried Clojure (673 names, 71 sharp edges); three wrote
independent designs (zero-cost first, Clojure fidelity first, type-system realism first); a fifth judged
them (the scoring is in the design record, `synth-notes.md`) and wrote the first version. Three critics
(ergonomics, compilability, consistency) then checked it and raised 66 findings; a further agent revised
the page, and §10.1 to §10.3 record every finding with its verdict. When the owner gave the tie-breaker,
two auditors classified the page against it independently, one presuming the page's reasons held and one
presuming every deviation wrong (§10.4 gives their counts), each running programs to test every reason the
page gave; a last agent decided where the two differed, re-ran the deciding programs, and rewrote this
page (§10.4.1 to §10.4.7); an adversarial checker then ran the rewrite against the rule and raised 28 findings (15
deviations without a reason, 6 safety reasons that a safe program refutes, 6 inconsistencies, 1 unproven block), and a
second revision resolved every one, by changing the page or by rejecting the finding with a reason (§10.4.8); a third check raised 17 (8 major, 9 minor), and while
the third revision resolved them the owner decided four questions the page had carried (lazy sequences are Clojure's memoised seqs with a fused loop as an optimisation,
integer `/` is a `Ratio`, strings order by code point, floats print as Clojure's), so the third revision also reworked the sections those decisions touch (§10.4.9, §9.1).
A claim about what the
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
   then misbehaves in one of four ways: it reads freed memory or follows a null pointer, frees twice,
   races (two tasks write one location, which for a slot that holds an object is a double release), or builds an
   invalid `str` that the decoder's bounds then no longer protect. A write through an alias that another holder
   *in the same task* observes is **not** a failure: it is Clojure's `aset` and `volatile!`, and it runs with a clean
   audit ([R] A12 p1, p10). If the program cannot be written, the deviation is not allowed. The programs that
   exist are in §5.1 (a global `Cell`; an array shared by two tasks). Five reasons that the first rewrite gave
   (`aset`'s value semantics, `locking`, the UTF-16 unit, an uninitialised object slot, `volatile!`'s `Cell`) were
   refuted by programs that run and are no longer deviations (§10.4.8).
2. **A static-typing fact the language already decided** (types §1, §2.11; ROADMAP M7 rule 0 says "or the
   static-typing the language already decided"): `nil` is `Option`'s empty variant and nothing else; a
   function type has a fixed arity; there is no run-time type information; a `Vec` has one element type and a
   `Map` one key type and one value type; a result type cannot depend on a value; a `char` is a Unicode scalar
   (types §1.1). A limit of today's compiler is not a decided fact: it is a stage limit (§5.5) that a §7 item lifts. Each
   is listed in §5 with the program that shows it, and each that is a simplification rather
   than a necessity has a typed twist in this page that keeps Clojure's text (§2.4, §2.11, §4.2). Where an item of §7 itself
   lifts a decided typing fact (C1's erasure rule, L21, L24 and L25 coerce where types §1.7 says nothing is coerced; C1 gives a
   closure a type with fields where types §1.4 says a closure type carries no capture list), the T-row of §5 stands until the
   owner signs that item, and §7.4 names the decided text each one overturns.

"Cleaner", "faster", "one way", "less surprising", "no GC", "it is a wart" and "the reader may be
confused" are **not** reasons. In particular a **leaked cycle through cells is not a memory-safety failure**: ownership.md §1
says no program without `unsafe` can leak anything *except* a cycle through cells, §6 says such a cycle is not a violation, and
the audit reports it apart from the errors (`audit: clean=false leak-cycles=2 leaks=0 errors=0` for a cell that holds a closure
that captures itself, [R] A11 e8b); any other leak the audit counts under `leaks=` as an error (`crates/fibref/src/cases/evaluator.rs`),
so "a leak" in general is not something the ownership model allows. A cost reason ("a lazy
cell per element is slow", "character offsets are O(n)") is not a reason either; where cost and the rule
collide the page follows the rule, says what the cost is, and puts the collision in §9 for the owner, who
set both aims. No deviation of the page stands on the cost side: the one that did (a recipe consumed once where Clojure's lazy
seqs are cached) the owner decided for Clojure's behaviour on 2026-10-01 (§2.1 rule 2, §9.1 Q34), keeping the zero-cost loop as an
optimisation that changes no observable behaviour.

Where a language or compiler change is what the rule requires (truthiness of `Option`, callable collections,
arity-reading `partial`), §7 lists it with its size and the program that shows the gap, and the owner signs it
there; §9 holds only what the rule does not settle.

### 1.2 The principles

| # | Principle | What it means for a function |
|---|---|---|
| P0 | The rule | §1.1: Clojure unless memory safety; Rust where Clojure's way would break it; a deviation carries its failing program or its decided typing fact. |
| P1 | Clojure's names, shapes, argument order and behaviour first | A name keeps Clojure's meaning and Clojure's spelling; a name fibber already has under another spelling (`array-get`, `shl`, `read-file`, `defun`) gets Clojure's as an alias (§3 N12). Sequence functions take the sequence last, collection functions take the collection first (§3). Behaviour includes the sharp edges of Clojure that are not safety matters (`compare` of vectors, `(take-nth 0 c)`, `(range 0 1 0.1)`): they are replicated, and the owner may strike any of them (§9.1 Q33). |
| P2 | Zero-cost by construction, Clojure's semantics by decision | Everything generic is monomorphised and every protocol call is static (compiler.md §7). A sequence function returns Clojure's **memoised lazy seq** (`LSeq`: a value traversed twice runs its function once; **Decided**, owner, 2026-10-01, §9.1 Q34), and a chain of sequence functions that is consumed in place, by construction exactly once, is **fused** by the expander into one push loop over small *recipe* structs that remember their source and their function: no seq is built and nothing is allocated per element except what a stage hands on (until C5 a struct or an `(Option scalar)` is a heap object: `zip`'s `Pair`, `keep`'s `Option`, a `Map` walk's `Pair`, [R] A10 alloc). The rule that decides which form a call takes is syntactic and sound (§2.1 rule 2, §7 E16; [R] A13 lz1, lz2, lz3: +5 objects for a fused chain over 1000 elements against +17526 for the same chain bound to a name). Only the materialisers (`vec`, `into`, `set`, `sort`, `group-by`) allocate per element by their signature. |
| P3 | Persistent in the API, in place when unique | `conj`, `assoc`, `update` keep Clojure's value semantics. Where the ownership checker proves the collection unique they update it in place, so a loop of `assoc`s is Clojure's transient without a transient API. `transient`, `persistent!`, `conj!` exist as identity wrappers so that Clojure's text resolves (§4.4). Whether the in-place claim holds is a test (§2.5), not a belief. |
| P4 | Unboxed elements, a real hash | A `(Vec i64)` stores `i64`s. The hash that types §2.12 fixes (an integer hashes to itself, FNV-1a for text) is replaced before the HAMT's speed is judged (§9 Q15, **Decided**). Iteration order of a `Map` and a `Set` is the hash's, as in Clojure, except that a small `Map` keeps insertion order as Clojure's array map does (§2.7). |
| P5 | Clojure's values where the type system has a tag for them | `nil` is `Option`'s empty variant, and a condition accepts `bool` or `(Option T)` (the falsy values are `false` and `nil`, as Clojure's, [sketch] §7 L20). A function that is undefined on some input of its type returns `Option` where Clojure returns `nil` (`first`, `get`, `peek`, `find`, `parse-long`, `index-of`); one for which Clojure throws traps, with a message and a position (`nth`, `pop`, `assoc` past the end, `Integer/parseInt`, `slurp`, `re-pattern`, `read-string`, `max-key` of nothing), until exceptions land (§2.10; Q35 is settled by the rule); each has a typed twin `try-<name>` that returns a `Result` for a program that must recover (§3 N13). `or`, `and`, `when`, `some->`, `update` and `reduce` keep Clojure's text over those types (§2.4). |
| P6 | Every spelling of Clojure's, and where Clojure has several ways, all of them | A protocol exists where the monomorphiser can dispatch on it (§2.3); a predicate that Clojure asks of a run-time tag (`vector?`, `seq?`, `satisfies?`) is a checker form that folds to the static answer, a variant test over `Val` and `Form` (§4.18, §5 T3), and `instance?`, `type`, `class` are not offered; everything else Clojure has is offered, as a function, a macro or an alias. The library's own additions (`Pair`, `unwrap`, `map-opt`, `and-then`, `try-let`, `seq-of`, `zip`, `find-first`) are marked `(new)` and never take the place of a Clojure name. |
| P7 | State: Clojure's global state is allowed where it is safe | A top-level `atom`, `defonce`, a dynamic var with `binding`, a global random generator, `defmulti` and metadata are offered (§2.11): none breaks memory safety. A global `Cell` or `Weak` is not: it would be reachable from every task and written by two (§5 M1); `volatile!` is an `Atom` (§2.11), and an array is Clojure's shared mutable object inside one task, a handle that cannot cross a task (§5 M2). |
| P8 | Ships on today's compiler | Each tranche of §8 is written in the form that compiles today. The compiler changes of §7 make the same source faster or the same names terser; they do not change what a function means, except the syntax, checker and runtime items that the rule requires and the owner signs separately: E3, E4, E8, E14, E15, E16, L2, L14, L20 to L29, L30, L31, C8 to C12 (L16 and L19 are **Decided**, and the owner's decisions of 2026-10-01 settle E16, L30 and C12: §9.1). |
| P9 | Every function has an executable test | Cases run both ways (method rule 6), generated programs against a model (rule 5), differential tests against a naive reference written in fibber (§8). The compiler (M6) is the first customer: what stage 2 needs lands first. |

Two non-goals. The library does not try to be a Clojure interpreter: no `eval` (a result type that cannot be
known statically, and the compiler inside every binary), no reflection (no run-time type information), no
Java instance interop (`.method`, `new`); the static names Clojure code writes (`Math/sqrt`, `Long/MAX_VALUE`,
`Integer/parseInt`, `Character/isDigit`) are offered as modules (§4.16). Arbitrary-precision numbers and
ratios are library types (`BigInt`, `Ratio`, `BigDecimal`; tranche 5, §2.8), not a non-goal. The library does
not hide cost: `count` says in §4 whether it is O(1) or a walk; a sequence that is bound to a name is a memoised seq that costs a cell per element, and a chain consumed in place is fused and costs none (§2.1 rule 2).

## 2. The core abstractions

Code blocks are marked. `ran:` means the block (or the module it is cut from) was compiled and run
with `fibc` and with `fibref` and the audit was clean (Appendix A, A7, A11, A12); a block that says it is an excerpt abbreviates a body with `..` and is not compilable
as printed; `proposed:` means the compiler does not accept it today, and the error it gives is quoted where it matters.

### 2.1 Iteration: Clojure's lazy seqs, one push protocol that fuses a chain, a pull protocol for lockstep walks, a closed seq type for recursion

A collection is anything that can feed its elements to a function. The protocol has one required
method, `each-while`: internal iteration with early exit by a `bool`. This is `reduce` (Clojure's
`IReduce`) made the primitive, and it is what Rust calls `try_fold`. The bodies of `nth`, `last` and
`to-vec` are abbreviated here; the module that ran has them ([R] A10 nth).

```lisp
;; ran: fib.seq (an excerpt: each `..` stands for the body in the module, so the block is not compilable as printed)
(defprotocol (Reducible s e)
  (each-while (self k: (fn (e) bool) :borrow) -> bool)   ; false from k stops the walk; the result says whether it ran to the end
  (size (self) -> i64                                    ; default: a walk; O(1) where the source knows
    (let ((n (cell 0)))
      (do (each-while self (fn (x) (do (set! n (+ @n 1)) true)))
          @n)))
  (nth (self i: i64) -> e ..)                            ; default: a walk that traps past the end; O(1) on Vec Array Range SubVec
  (last (self) -> (Option e) ..)                         ; default: a walk
  (to-vec (self) -> (Vec e) ..))                         ; default: a push loop; a Vec returns itself
(defenum (Step a) (More v: a) (Done v: a))                ; Clojure's `reduced`: the step of `reduce-while`
(defstruct (Pair a b) (fst: a snd: b))                    ; the tuple the library needs (§7 L3)
```

Rules:

1. **Sources** implement `Reducible` with one `impl`: `Vec`, `SubVec`, `VSeq`, `List`, `Array`, `Option` (zero or one
   element, so `(map inc nil)` is empty and `nil` flows through a pipeline), `Map` (yields
   `(Pair k v)`), `Set`, `Range`, `Iterate`, `Repeat`, `Cycle`, `Chars`, **`str` (a source of `char`s, as
   Clojure's string is a seqable of characters, §2.9)**, `MArray`, `LSeq` (the lazy seq of rule 2, realised by its first walk, [R] A13 lz1),
   and the recipe structs of rule 2, which are never values of a program.
2. **Adaptors** (`map filter remove keep take drop take-while drop-while mapcat concat
   map-indexed reductions partition interleave zip ...`) return **Clojure's lazy seq**: an `(LSeq e)`, a chain of nodes each
   realised once by a thunk held in a cell, so a value traversed twice runs its function once per element, nothing runs until a
   node is demanded, and an infinite source is fine (**Decided**, owner, 2026-10-01, §9.1 Q34; [R] A13 lz1: a bound `map` with a
   counting `f` summed twice gives `[12 12 3]`; a `map` over an infinite `iterate` has made 0 calls when bound and 3 after
   `(vec (take 3 s))`; `first`, `rest` and `seq` walk it, `[(some 10) (some 12) nil]`; it prints `(10 12)`). The lazy forms are
   written as Clojure writes them, over the node of a seq (`lmap`, `lfilter`, `ltake` in the prototype, three lines each);
   `lazy-seq`, `lazy-cat`, `doall`, `dorun`, `realized?` and chunking are Clojure's (§5.6).

   **The fused path.** Each adaptor also has a *recipe* form: a struct holding the source and the parameters that implements
   `Reducible` over it (`Mapped`, `Filtered`, `Taken`, ..., §3 N8), built in O(1) and allocating nothing per element. A recipe is
   never a value of the program: the expander rewrites a chain of adaptor calls that is consumed exactly once *by construction*
   into recipes, and the terminal consumer walks them as one push loop (§7 E16). The rule, applied after macro expansion to every
   call whose head resolves to the library's definition (a user's own `map` is not rewritten):

   * **A** is the set of sequence functions that have a recipe form: the adaptors above, `cons`, `zip`, the
     three-argument `map` (Clojure's two-collection form, which replaced `zip-with`; the expander's `STAGES` takes a call of exactly two
     arguments, so today it stays the lazy `map$3`, not the recipe `ZipWith`), `interleave`, `cycle`, and what `for` expands to. **T** is the set of *terminal consumers*: the functions that walk their
     collection argument at most once and return something that is not a seq: `reduce reduce1 reduce-while reduce-kv transduce
     into vec set run! dorun doseq count empty? some every? not-any? not-every? last nth frequencies group-by sort sort-by
     sort-by-cached zipmap mapv filterv str/join shuffle rand-nth to-array into-array find-first find-map includes? seq= bounded-count
     sum product`, the folds of `apply` (`max-key`, `min-key`, `distinct?`, `str`, `concat`, ...) and the printers.
   * **R**: in a call `(t args.. c)` with `t ∈ T`, the collection argument becomes `F[c]`; in every other position a call
     `(a args.. c)` with `a ∈ A` is the lazy form, an `LSeq`, over arguments that are not rewritten.
   * **F**: `F[(a args.. c)]` with `a ∈ A` is the recipe `(a* args.. F[c])` (every collection argument of a lockstep adaptor is
     rewritten); `F[x]` for any other expression is `x`, evaluated as written: a `Vec`, a `Range`, a bound `LSeq`, which the loop
     reads or realises.

   It is sound because a recipe made by `F` occurs exactly once, as the argument of the consumer or adaptor that holds it: there
   is no name through which it could be traversed again, and a consumer in `T` traverses its argument at most once, so the chain's
   functions run once per element consumed, as the memoised seq would have run them. `first`, `second`, `rest`, `next`, `seq`,
   `doall`, `take-last` and every function that returns a seq are **not** in `T` (the owner's words: a result that is bound, returned,
   stored or traversed by `first`/`rest`/`seq` is a real memoising seq). The one latitude the fused loop takes is Clojure's own: a
   lazy seq over a chunked source realises up to 32 elements ahead of its consumer (A12 chunk, §5.6), so the number of calls an
   early-exiting consumer makes to a pure `f`, and the interleaving of `f`'s effects with the consumer's, are unspecified in
   Clojure, and the two paths here differ only within that latitude. What the rewrite costs: one pass over the expanded forms,
   nothing at run time; a fused chain allocates two objects per stage (the struct and its stored closure) and none per element
   ([R] A13 lz2: +5 objects over the input for the chain of A1), where the same chain bound to a name is realised as a seq ([R] A13
   lz3: +17526 objects for 1000 elements, about 12 per node realised: the seq's struct, cell, thunk, node and closure). So
   `(let ((m (map f c))) (reduce + 0 m))` pays for the seq, as Clojure's does, and `(reduce + 0 (map f c))` and
   `(->> c (map f) (filter p) (reduce + 0))` pay nothing: `->>` is a macro, and the rewrite sees the nested calls. The prototype of
   the rewrite is two macros, `fuse` and `fuse-arg`, that do to one form what the expander pass does to every form ([R] A13 lz1:
   `(fuse (reduce (fn (a x) (+ a x)) 0 (lmap f (lfilter odd? [1 2 3 4]))))` gives `[8 2]`, two calls of `f`; `(fuse (vec (ltake 2
   (lmap f (lseq-iterate inc 0)))))` gives `[0 2]` with 2 calls, an infinite source, no chunk and no seq; a bound seq as the source
   of two fused chains is realised once, `[0 0 3]`).

   **As implemented** (tranche 1, R9, commit `605a26e`; `crates/fibref/src/expand/fuse.rs` and `fuse/{tables,scan,stage}.rs`, mirrored in
   `compiler/expand/fuse{,tab,scan,run}.fib`; cases 820 to 836, which pass under both tools in the runs of §7.5; the chain of A1 allocates at most 4 objects over 50 and over 500 elements, `allocs: <= 4` in cases
   825 and 826). The pass runs after the module's macros have expanded and the module's public names are known, over each top-level form that calls a
   terminal, and where the rule above is silent it decides:

   * **The sets are the page's restricted to what exists.** **A** is `map filter remove take drop take-while mapcat concat` (the others join when their
     functions and recipes land: `cons zip interleave cycle`, the three-argument `map` and `for`, tranche 2); **T** is `reduce reduce1 reduce-while reduce-nonempty count
     empty? last nth into vec set run! every? not-any? find-first find-map group-by frequencies sort sort-by sum` (`sort` with one argument and `sort-by` with two are rows; the clause-picked heads `sort$2` and `sort-by$3` have no row, so
     a chain under them is read as the seq it is bound to; `str/join` and
     `seq=` are not names of the library yet). `reduce-nonempty`, which the `reduce` macro of §6.3 emits for `(reduce f c)`, is in T and is no row of §4.
   * **Recipe constructors take the source first**, `(Mapped c f)`, `(Taken c n)`, `(Cat a b)`: the `(a* args.. F[c])` of rule R is notation. `nth` is a
     terminal whose collection is its **first** argument, `(nth c i)`; every other terminal takes it last.
   * **A stage is rewritten only if its function or count argument is a symbol, a literal or a `fn` form.** A recipe holds its source before its
     parameter, so an argument that is a call (`(take (dec n) c)`, `(map (partial f 1) c)`, `(map (comp f g) c)`) would run after the source's effects, and
     the rewrite would reorder them (case 831 pins the order); such a stage keeps its lazy form, and so does the chain below it. Soundness is kept; the
     zero-allocation path for those stages is given up until a rewrite binds the argument first.
   * **`remove` is `Filtered` over the negated truthiness**: `(remove p c)` is `(Filtered c (fn (x) (fib.prelude/not (fib.core/truthy? (p x)))))`, with `p` a
     symbol (bound once to a gensym named `#fuse.N`) or a `fn` literal (its last body form wrapped). The plain `(not (p x))` does not type-check for a
     predicate that returns an `Option` (case 832); the rewrite needs `fib.core` in scope, like every expansion that names `fib.core/`.
   * **A stage with the wrong number of arguments** (`(map f a b)`, which is §2.1 rule 5's `map/3`) **is not a stage**, and **a recipe written by hand is
     never fused** (case 834).
   * **Which heads are the library's.** A bare head counts only if (1) the top-level form does not bind it (`let`, `loop`, `fn` and `defun` parameters and
     pattern variables, with quoted data skipped), (2) the module does not define it at top level, (3) no non-library module that this one uses, re-exports
     included, exports it, and (4) the module sees the implicit module that holds it: `fib.seq`, or `fib.coll` for `vec`, `set` and `into`. A head written
     `fib.seq/map` or `fib.coll/vec` is the library's whatever the module defines (case 833). Modules whose `ns` starts with `fib.` and modules that do not
     see `fib.seq` are not rewritten. A `:require` alias (`s/map`) is not recognised. With the implicit list filled (`IMPLICIT_LIB`, §6.2, the flip) rule (3) runs end to end: a program that `:use`s a module that exports `first` or `map`
     calls its own (case 872), and a program's own `map` is not an error (case 871).
   * **The let rule (stage 2; performance batch 3, P2; cases 4200 to 4203).** A seq bound by a `let` and read once is the chain it is. Before the fusing walk, a binding `(n e)` whose
     pattern is a plain symbol is folded into its reader when the symbol occurs once in all that follows it, that one occurrence is the collection of a terminal consumer
     or of a chain of stages (the tables above) that is the init of the next binding or the one body form of the `let`, and every other argument of the terminal and of the
     stages is a symbol, a literal or a `fn`. Then `e` is evaluated once, at the same place in the program's order (the arguments it moves past have no effect), and the
     chain is in a collection position, so it is fused: `(let ((r (range n)) (m (map f r)) (a (reduce + 0 m))) ..)` is `(let ((a (reduce + 0 (map f (range n))))) ..)`. A seq
     that is read twice, or from a closure, or by anything that is not the library's consumer, is not touched and stays the memoised seq of case 822 and 827. The Rust
     `expand/fuse.rs` has no such rule (frozen); the case headers say `stage: 2` (compiler/mirror-pending/P2-fuse-let.md). Measured: `scripts/bench/lazy-bound.fib` over 5e6
     elements, 1.39 s and 1.5 GB before, 0.05 s and 1.5 MB after.
   * **The dump** of a program that sees `fib.seq` shows the recipes and the gensyms `#fuse.N`; its format is unchanged (bootstrap §2, §5.3).

   **Not adopted.** The second revision's affine recipes (a value consumed once, a compile error on the second use) and the first
   rewrite's use-count `cache` insertion are deleted: the owner decided Q34, and the check the affine design needed (a closure that
   consumes a captured recipe may be called once) needs a closure kind in the type, which types §1.4 excludes (**Decided**); its
   residual claim was also wrong by the length of the sequence ([R] A13 ev1, ev2: `[10 8]` and `[1 8]` where Clojure's cached seq
   makes 4 calls). An `LSeq` holds a `Cell`, so it cannot cross a task (`cell cannot be shared between threads: closure capture s,
   field box of Cached has type (Cell (Option (Vec i64)))`, A11 e8d) and cannot be a `def` (§5 M1): a global lazy value,
   `(def fibs (lazy-cat [0 1] (map + fibs (rest fibs))))` or `(def cfg (delay (load)))`, waits for the run-once cell of §7 C9, which
   this revision recommends for tranche 3 (§9.2 Q41); until then it is a `defn` that builds the seq. A self-referential `LSeq`
   (`fibs`, `ones`) is a reference cycle through a cell, `audit: clean=false leak-cycles=4 leaks=0 errors=0` (A11 t22), the one leak
   the ownership model allows (§1.1).

3. **Consumers** (`reduce first last some every? count empty? into vec set sort ...`) are
   loops over `each-while`. `first`, `some`, `every?`, `take`, `take-while`, `empty?`
   stop the source through the `bool`, so `(take 5 (iterate inc 0))`, `(first (filter p (iterate inc 0)))` and
   `(some p (iterate inc 0))` terminate. Clojure's `reduced` is `(reduced x)`, a `(Done x)`: plain `reduce`
   over a **literal** `fn` reads it (a macro rewrites the tails of the body, so `f`'s type stays `(fn (a e) a)`;
   `(reduce (fn (acc x) (if (> acc 10) (reduced acc) (+ acc x))) 0 [5 6 7 8 9])` is 11, over `(iterate inc 1)`
   with a limit 105, an infinite source ends, A11 e20 and t81), and `reduce-while` is the form for a function
   that is not a literal. It allocates a heap enum per step until unboxed enums land (§7 C5), so the plain
   consumers use the `bool` channel.
4. **`count` is `size`**: O(1) where the source holds its elements (`Vec Map Set Array Range Option SubVec`),
   a walk for every adaptor, so **`(count (map f c))` calls `f`, as Clojure's does** (the prototype's `Mapped`
   no longer overrides `size`: `[(count r) calls]` is `[2 2]` and `[(sum r) calls]` `[3 4]` over `[1 2]`, A11
   count; the first version of this page made it O(1) and not call `f`, which was a cost reason). `Range` has a
   closed-form size ([R] A10 rangesize). `empty?` stops at the first element the recipe produces: O(1) on a
   source that holds its elements, and `(empty? (filter p c))` calls `p` until an element passes (100,000 calls
   for a predicate that never does, A10 count). A bound `LSeq` is realised by its first walk and read by every later one:
   `(count s)` and then `(reduce + 0 s)` call `f` once per element in all ([R] A13 lz1, `[0 0 3]`). **`nth`, `last` and `to-vec` are methods with a walking
   default**, so `(nth (filter p c) 3)` works in O(n) as Clojure's does and `(nth v 3)` is O(1). A generic
   `vec` cannot special-case a type, which is why `to-vec` is a method: `(vec v)` of a `Vec` is the
   identity, and `(vec (vec (vec v)))` of a 1000-element `Vec` allocates nothing beyond `v` (A10 nth).
5. **Lockstep walks** (`zip`, `interleave`, three-argument `map`) need the second
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
   5]]` and `(map (fn (b a) (- b a)) (rest xs) xs)` runs, in either order, under both tools (A11 e16). The first
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

   **The transducer arity** of every Clojure function that has one has the same shape, a factory of steppers: `map filter remove take
   take-while drop drop-while take-nth keep keep-indexed map-indexed mapcat partition-all partition-by dedupe distinct interpose replace
   random-sample`, and `cat`, `halt-when`, `completing` (§4.2, §4.4). Until L1 makes `(map f)` the one-argument clause of `map`, each is
   `x<name>`: `xmap xfilter xremove xtake xtake-while xdrop xdrop-while xtake-nth xkeep xkeep-indexed xmap-indexed xmapcat xpartition-all
   xpartition-by xdedupe xdistinct xinterpose xreplace xrandom-sample`. Fourteen ran (`xmap xfilter xtake xmapcat xpartition-all` in A8, and nine
   more in A12 xf2: a stateful stepper keeps its state in a per-traversal cell, an early-stopping one returns `false`, one that holds elements
   back says so in its flush); `xdrop xkeep xkeep-indexed xreplace xrandom-sample cat halt-when completing` are the same shape and were not
   written here:

```lisp
;; ran: fib.xf (A12 xf2, three of its nine; the rest are the same shape)
(defun noflush () -> (fn ((fn (b) bool)) bool) (fn (k) true))
(defun xtake-while (p: (fn (a) bool)) -> (Xf a a)
  (Xf (fn () (Stepper (fn (x k) (if (p x) (k x) false)) (noflush)))))
(defun xdedupe () -> (Xf a a) :where ((Eq a))
  (Xf (fn () (let ((prev (cell nil)))
               (Stepper (fn (x k) (match @prev
                                    (nil (do (set! prev (some x)) (k x)))
                                    ((some p) (if (= p x) true (do (set! prev (some x)) (k x))))))
                        (noflush))))))
(defun xpartition-by (f: (fn (a) k)) :where ((Eq k)) -> (Xf a (Vec a))      ; holds the open group, emitted by the flush
  (Xf (fn () (let ((buf (cell (vec-empty))) (last (cell nil)))
               (Stepper (fn (x k)
                          (let ((v (f x)))
                            (match @last
                              (nil (do (set! last (some v)) (set! buf (conj @buf x)) true))
                              ((some l) (if (= l v)
                                            (do (set! buf (conj @buf x)) true)
                                            (let ((g @buf)) (do (set! buf (conj (vec-empty) x)) (set! last (some v)) (k g))))))))
                        (fn (k) (if (> (count @buf) 0) (k @buf) true)))))))
;; (into-xf [] (xpartition-by odd?) [1 3 2 4 5]) is [[1 3] [2 4] [5]]; (xdedupe) over [1 1 2 2 2 1] gives [1 2 1]; ten checks, result 0
```

8. **Recursion on `rest` uses a closed seq type.** Clojure's `(defn len [xs] (if (seq xs) (inc (len (rest
   xs))) 0))` is the basic idiom, and for a generic `c` with `rest` returning `(Dropped c e)` it is polymorphic
   recursion, which `fibc` cannot finish ([R] A10 poly; §7 B4 makes both tools reject it). The cure is
   Clojure's own structure: `seq`, `rest` and `next` belong to `Seqable c s`, where `s` is one **closed** seq type per
   collection and `rest` of an `s` is an `s`: a `Vec` has the `VSeq`, a `List` itself, a `Range` itself, a
   `str` a view that steps by one character, `Iterate`, `Repeat`, `Cycle` and `LSeq` themselves, and every other collection
   (`Map`, `Set`, `Array`, `MArray`, `Option`, `Queue`, `SortedMap`, `SortedSet`, `Reversed`, `Matches`) the `LSeq` of its elements,
   **one `impl` per head**: a blanket instance over `Reducible` is `an instance head must not be a type variable`, and without the
   instance `(seq {1 2 3 4})` is `no implementation of Seqable for (Map i64 i64)` ([R] A13 blanket, blanket2); the instances for
   `Map`, `Set`, `Array` and `Option` run, and the generic `len` below is `[2 3 2 1 2 5]` over them, a `Vec`'s `rest` and a lazy `map`
   ([R] A13 seqable). `VSeq` is a *seq*: a
   `List` of consed front elements over a `Vec` and an offset, so `conj` conses at the front and it prints in
   parentheses, as Clojure's `(conj (rest [1 2 3]) 0)` is `(0 2 3)`. It is **not** `subvec`'s result: `SubVec` is a vector
   view whose `conj` appends and which prints in brackets, as Clojure's `(conj (subvec [1 2 3] 1) 0)` is `[2 3 0]`. The first
   version of this page had one type, `Slice`, for both, with one `conj`, and `(conj (rest v) 0)` printed `[2 3 0]` under both
   tools ([R] A12 q_slice). The idiom type-checks, is generic over every collection that has a seq type and runs under both
   tools; `sort` returns a `VSeq` too, as Clojure's returns a seq (`(conj (sort [3 1 2]) 0)` is `(0 1 2 3)`):

```lisp
;; ran: fib.seqable (A12 vseq; the main of that program is left out; Coll2 and conj2 stand for Collection and conj, §7 B3)
(defstruct (VSeq a) (front: (List a) v: (Vec a) lo: i64))
(impl (Reducible a) (VSeq a)
  (each-while (self k)
    (if (each-while (. self front) k)
        (let ((v (. self v)) (n (count (. self v))))
          (loop ((i (. self lo))) (if (< i n) (if (k (nth v i)) (recur (+ i 1)) false) true)))
        false))
  (size (self) (+ (size (. self front)) (- (count (. self v)) (. self lo))))
  (to-vec (self) (if (= (. self lo) 0) (match (. self front) ((fib.prelude/empty) (. self v)) (_ (default-to-vec self))) (default-to-vec self))))   ; (vec (sort xs)) is the sorted Vec itself
(defun default-to-vec :private (c: (VSeq a)) -> (Vec a)
  (let ((acc (cell (vec-empty)))) (do (each-while c (fn (x) (do (set! acc (conj @acc x)) true))) @acc)))
(defprotocol (Seqable c s) (seq (self) -> (Option s)) (rest (self) -> s))     ; nil when empty / empty when empty, never nil
(defun vseq-of (v: (Vec a)) -> (VSeq a) (VSeq fib.prelude/empty v 0))
(impl (Seqable (VSeq a)) (Vec a)
  (seq (self) (if (> (count self) 0) (some (vseq-of self)) nil))
  (rest (self) (VSeq fib.prelude/empty self (if (> (count self) 0) 1 0))))
(impl (Seqable (VSeq a)) (VSeq a)
  (seq (self) (if (> (size self) 0) (some self) nil))
  (rest (self)
    (match (. self front)
      ((fib.prelude/cons _ t) (VSeq t (. self v) (. self lo)))
      ((fib.prelude/empty) (VSeq fib.prelude/empty (. self v) (if (< (. self lo) (count (. self v))) (+ (. self lo) 1) (. self lo)))))))
(defprotocol (Coll2 s e) (conj2 (self x: e) -> Self))
(impl (Coll2 a) (VSeq a) (conj2 (self x) (VSeq (fib.prelude/cons x (. self front)) (. self v) (. self lo))))   ; conj conses at the front
(defstruct (SubVec a) (v: (Vec a) lo: i64 hi: i64))                                                            ; subvec's result
(impl (Coll2 a) (SubVec a) (conj2 (self x) (let ((v (to-vec self))) (SubVec (conj v x) 0 (+ (size self) 1)))))  ; conj appends
(defun sort-seq (c: c) :where ((Reducible c e) (Ord e)) -> (VSeq e) (vseq-of (sort c)))
(defun next (c: c) :where ((Seqable c s) (Seqable s s)) -> (Option s) (seq (rest c)))
(defun len (xs: c) :where ((Seqable c s) (Seqable s s)) -> i64 (if-let (s (seq xs)) (+ 1 (len (rest s))) 0))
;; (len [5 6 7 8]) 4, (len (rest v)) 2; (conj2 (rest [1 2 3]) 0) shows (0 2 3), (conj2 (subvec [1 2 3] 1 3) 0) shows [2 3 0],
;; (conj2 (sort-seq [3 1 2]) 0) shows (0 1 2 3), (sort-seq [3 1 2]) shows (1 2 3): [3 2 0 3] for len, under both tools;
;; (to-vec (sort-seq [3 1 2])) = [1 2 3], (to-vec (rest [1 2 3])) = [2 3]
```

   `len` is instantiated at `(Vec i64)` and then at `(VSeq i64)`, which calls itself: a fixed point, not a
   chain. `first` is a function over `each-while` (`(Option e)`, stops after one element), not a method of
   `Seqable`; the methods of `Reducible` are `each-while size nth last to-vec`. `(rest (map f c))` is `rest` of an `LSeq`:
   one node per step, as Clojure's, so the idiom is the convenient form and a fused `reduce` or a `loop` the zero-cost one.

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
loop variable) is served by the closed seq type of rule 8, a `VSeq`, a `List` or an `LSeq`, and not by a cursor over a
recipe. The unbounded case remains a hazard: a function that recurses on `(drop 1 c)` for any `Reducible c` calls
itself at `(Dropped c e)`, then at `(Dropped (Dropped c e) e)`, without end (polymorphic recursion): `fibc` exhausts
memory (`memory allocation of 577136 bytes failed` under `ulimit -v 4000000`) and `fibref` returns 3 ([R] A10 poly), a
stage-1 divergence that §7 B4 closes by making both tools reject it with a message that names the recursion.

**Materialising** is explicit and the only per-element cost: `vec`, `into`, `set`, `zipmap`,
`sort`, `reverse`, `group-by`, `frequencies`. The bulk ones build in one function
over a growable buffer and assemble the trie once, about n/32 allocations instead of 2n (§2.5); `vec` of a
`Vec` is the identity.
Each adaptor has an eager spelling only where Clojure has one (`mapv`, `filterv`, aliases of
`(vec (map ..))`; `doall` realises an `LSeq` and returns it, `dorun` walks it for effect).

**Infinite and lazy.** `iterate repeat cycle` are ordinary sources that only an early-exiting consumer ends, each its
own seq type (rule 8). Memoised laziness is the library type `LSeq` (`fib.seq`, tranche 1; [R] A13 lz1): the result of
every sequence function that is not fused (rule 2), of `lazy-seq` and `lazy-cat` (macros over it), of `repeatedly`,
`line-seq`, `iteration` and `file-seq`, and the seq of the keyed collections (rule 8). A node is realised once; a seq
that forces itself while it is being realised traps `lazy-seq: a seq forced itself` (Clojure's loops or throws); a seq
that merely refers to itself (`ones`) is fine. The cell per node is the cost Clojure pays, paid where Clojure pays it;
the fused loop pays it nowhere.

**The seq is one type.** Every sequence function returns an `(LSeq e)`, which has three consequences. (1) *Joins.*
`(if flag (map f v) (filter p v))` unifies, both arms being `(LSeq i64)`. `(if flag (filter p v) v)` does not, an
`(LSeq i64)` against a `(Vec i64)` (for the first version's recipes it was `cannot unify (Vec i64) with (Filtered (Vec
i64) i64)`, [R] A10 join); `cond->` over a sequence is the same case. Clojure's text is fine there, and no safety reason
forbids it, so the checker rule of §7 L24 unifies two arms that are both `Reducible` of one element type at
`(dyn (Reducible e))`, the erasure rule that C1 already needs for closures; `(seq-of c)` is the explicit form, a
`(dyn (Reducible e))` that erases the type at the cost of one heap object and an indirect call per visit; it type-checks
and runs (`pipeline` in A10 join), and the cost is in the name. (2) *Threads.* An `LSeq` holds a `Cell` and cannot cross
a task until §7 C9 lands (§5 S10, A11 e8d); a recipe never escapes the expression that fuses it, so the fused loop needs
no colour parameter (the first version's colour parameter on adaptor structs, A10 col, served recipes that were values;
a `:send` closure that builds its chain inside the task is the form today). (3) *Borrows.* A recipe stores its source,
so a library function whose parameter is `:borrow` cannot feed a fused chain from it: `parameter v of g is declared
:borrow but escapes` (A10 bw). Collection parameters of library `defun`s are left to inference (owned).

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
`:borrow` either: `parameter f of sort-by-with is declared :borrow but escapes` (A10 sortby, when the function was `sort-by-with`; `sort-by`'s key is not `:borrow`).
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
| `Reducible s e` | `each-while`, `size`, `nth` (traps out of range), `last`, `to-vec`, and, since P0, `to-lseq` (plan PC-13: a default through `to-vec`, overridden by `LSeq` itself, so that two lazy adaptors do not realise their source in full) | every sequence function and consumer | `Vec List Array MArray Option Map Set Range Iterate Repeat Cycle Chars str SubVec VSeq Queue SortedMap SortedSet Reversed FRange Matches Lines LSeq`, the recipe structs of the fused path (never values of a program, §2.1 rule 2), user types |
| `Seqable c s` | `seq -> (Option s)`, `rest -> s` | `seq rest next` and the recursion idiom (§2.1 rule 8) | `Vec`, `SubVec` and `VSeq` (seq type `VSeq`), `List`, `Range`, `Iterate`, `Repeat`, `Cycle`, `LSeq` (each itself), `str` and `Chars` (a view that steps by a character); `Map Set Array MArray Option Queue SortedMap SortedSet Reversed Matches` (seq type `LSeq`), **one impl per head** ([R] A13 seqable; a blanket impl is rejected, A13 blanket) |
| `Cursable s c`, `Cursor c e` | `cursor`; `advance!`, `current` | `zip interleave map/3 zipmap` (the second operand of the fused path) | `Vec Range Array MArray SubVec VSeq LSeq str` and their cursors; every recipe by buffering, and without a buffer for `Mapped Dropped Taken Zipped` after L16 |
| `Lookup s k v` | `get -> (Option v)` | `get update find select-keys`, and a `Lookup` value in call position (§2.11) | `Map`; `Vec`, `SubVec`, `Array` and `MArray` (key `i64`; [R] A13 marray: `(get (int-array [1 2]) 0)` is `(some 1)`); `str` (key `i64`, value `char`); `SortedMap`; `(Option s)` after L16; user types |
| `Assoc s k v` | `assoc` | `assoc update assoc-in` | `Map`; `Vec` and `SubVec` (index at most the count: `i = count` appends, as Clojure's); `SortedMap`; a struct by a literal keyword (L21) |
| `Dissoc s k` | `dissoc` | `dissoc disj` | `Map Set SortedMap SortedSet` |
| `Keyed s k` | `contains?` | `contains?` | `Map Set SortedMap SortedSet`, **and `Vec`, `SubVec`, `Array`, `MArray` and `str` (the index, as Clojure's: `(contains? [10 20] 1)` is true and `(contains? [10 20] 20)` false; `(contains? "abc" 1)` true; A13 marray)**; `includes?` is the element test |
| `Collection s r e` | `conj -> r` (the result type `r` is determined by the instance: the collection itself, or an `(LSeq e)` for a seq) | `into conj` (`into` requires `r = s`; `vec` is `Reducible`, `set` is `Reducible` plus `Hash`/`Eq`; `merge` is over `Map`s only, §4.5) | `Vec` (end), `SubVec` (end), `List` (front), `VSeq` (front), `Set`, `Map` (of `Pair`; of a two-element `Vec` after L23), `Queue` (back), `SortedMap` (of `Pair`), `SortedSet`; **`LSeq`, `Range`, `Iterate`, `Repeat`, `Cycle` (a cons at the front, an `LSeq`, as Clojure's `(conj (range 3) 9)` is `(9 0 1 2)`; [R] A13 conjseq: `(0 2 3)`, `(9 0 1 2)`, `[1 2 3]`, and `into` over the determined instance `[0 1 2]`)** |
| `Emptyable` | `empty` | `empty select-keys assoc-in` | `Vec Map Set List VSeq Queue SortedMap SortedSet` (`VSeq` is `fib.coll.seqs`'s, tranche 1) |
| `Stack s e` | `peek -> (Option e)`, `pop` | `peek pop` | `Vec` (end), `SubVec` (end), `List` (front), `Queue` (front) |
| `Reversible s e` | `each-while-rev` | `rseq` | `Vec Array Range SubVec SortedMap SortedSet` |
| `KeyReducible m k v` | `each-kv-while` | `reduce-kv` | `Map`, `Vec` (index, element) |
| `Truthy r`, `Payload r p` | `truthy? -> bool`; `payload -> p` | the predicate parameter `(fn (e) r)` of `filter remove some every? not-any? keep take-while drop-while`; `if-let`; `some` | `bool` (payload `bool`), `(Option a)` (payload `a`); no other type (a condition that cannot be false) |
| `Cmp r` | `resolve` | the comparator parameter `(fn (e e) r)` of `sort sort-by sorted-map-by sorted-set-by comparator` | `i64` (negative, zero, positive), `bool` (Clojure's predicate form: true is -1, else the swapped call decides) |
| `Pattern p` | `find-in`, `split-by`, `replace-in` | `str/split str/replace str/replace-first str/index-of str/last-index-of` (`re-find` takes a `Regex` only, as Clojure's) | `str` and `char` (tranche 1), `Regex` (tranche 4) |
| `Eq Ord Hash Show` | built in; `derive` | `= < hash println` | scalars, `str` (`Ord str` is code-point order, the builtin's: §2.7, §5 D5), `Option`, `List`; **added**: `Vec Map Set Pair Triple Ratio` (§2.7, §2.8); `Ord` of a `Map` or `Set` is not offered (Clojure's `compare` throws on a map) |
| `ToStr` | `to-str -> str` | `str` | `str` itself, scalars, `char`, `(Option a)` (empty text for `nil`), collections (the `Debug` text, as Clojure's `(str ["a"])`), derived structs |
| `Debug` | `debug -> str` | `pr prn pr-str` | as `Show`; strings quoted (§2.7) |
| `Num`, `Bits` | built in (types §2.12; after L30 `Num` is `+ - * neg quot rem`, and `/` is `Div`'s) | `+ - * quot rem bit-and ...` | integer and float types; the library's `BigInt`, `Ratio`, `BigDecimal` (§2.8); `Bits` has integer instances only, so `Bits t` is the page's "integer type" constraint |
| `Div a r` | `/ -> r` (the result type is determined by the instance) | `/` (§2.8, L30) | `i8..i64 -> (Ratio t)`, `f32 -> f32`, `f64 -> f64`, `(Ratio t) -> (Ratio t)`, `BigInt -> (Ratio BigInt)` (tranche 5); [R] A13 div |
| `Unit t` | `zero`, `one` (take a value of the type as witness) | `inc dec abs zero? sum` | `i8..i64`, `f32`, `f64` |
| `ToByte ToShort ToInt ToLong ToFloat ToDouble ToChar` | `byte short int long float double char` | the conversions | numeric types; `ToInt` and `ToLong` also take a `char`, so `(- (int c) (int \0))` is Clojure's text ([R] A12 int2) |
| `Fn f args r` (after C1) | `call` | every function argument; the callable collections | every `(fn ..)`; `Map Set Vec` and keyword instances after C1 and L23 (§2.11) |

The prelude's `Countable`, `Indexable`, `Traversable`, `Iter` and `Seq` protocols and its `for-each`, `map`,
`iter`, `collect`, `filter-iter` are replaced by `Reducible`, `Seqable` and the functions of §4 (of the 191 case files of
`cases/ownership`, one uses `Countable` and two use `filter-iter`; §6.5 and §8.3 give every prelude name and the
migration). `Associative` splits into `Lookup`, `Assoc` and `Dissoc` so that a vector can be looked up, and a set
can be dissociated, without pretending to be the other; the prelude's `Entry` struct gave way to `Pair`, the one
tuple type (§5 T4; done with L4, commit `5ea989f`: a `Map` walk hands out `(Pair k v)` with fields `fst` and `snd`); `dissoc` and `disj` are one `Dissoc`, so `(disj m k)` on a
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
e6); `Keyed` and `Lookup` on a `str` and on an `Array` are one impl each, since `(contains? "abc" 1)` and `(get (int-array [1 2]) 0)` are
Clojure's ([R] A12 keyed, the second block below):

```lisp
;; ran: fib.coll (A11 e6)
(impl (Keyed i64) (Vec a) (contains? (self i) (and (>= i 0) (< i (count self)))))
(defun assoc-c (v: (Vec a) i: i64 x: a) -> (Vec a) (if (= i (count v)) (conj v x) (assoc v i x)))
;; (contains? [10 20] 1) true, (contains? [10 20] 20) false, (assoc-c [10 20] 2 30) [10 20 30], (assoc-c [10 20] 0 30) [30 20],
;; (assoc-c [10 20] 5 30) trap: assoc: index out of range
```

```lisp
;; ran: fib.coll (A12 keyed)
(impl (Keyed i64) str (contains? (self i) (and (>= i 0) (< i (count (str-chars self))))))
(impl (Lookup i64 char) str (get (self i) (if (and (>= i 0) (< i (count (str-chars self)))) (some (nth (str-chars self) i)) nil)))
(impl (Keyed i64) (Array a) (contains? (self i) (and (>= i 0) (< i (array-len self)))))
(impl (Lookup i64 a) (Array a) (get (self i) (if (and (>= i 0) (< i (array-len self))) (some (array-get self i)) nil)))
;; (contains? "abc" 1) true, (contains? "abc" 3) false, (get "abc" 2) (some c), (contains? (array 2 7) 2) false, (get (array 2 7) 5) nil
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
the others as clauses of the same name once arity overloading exists (§7 L1); landed (Y11) as `get$3`, `nth$3`, `subs$2`, `str/index-of$3`, `str/split$3`, `reductions$2`, `repeat$2`, `repeatedly$2`, `range$3`, `partition$2 $3 $4` and `partition-all$2 $3`; Y11b: `sort$2`, `sort-by$3` and `map$3` (Clojure's `(sort cmp c)`, `(sort-by key cmp c)` and `(map f c1 c2)`; the stand-ins `sort-with`, `sort-by-with` and `zip-with` are deleted); the Rust fusion tables (expand/fuse/tables.rs) key a call on the base name and the call's own argument count, so `sort`/1, `sort-by`/2 and `map`/2 keep their rows and the longer clauses match none (a chain under `sort$2` is built as the seq it is bound to; cases 338, 806, 809). Still stand-ins: `reduce1` and `range-by` (the target of the Rust macro `range`). `update` is the macro of §2.4 (its extra
arguments are Clojure's, so it takes no stand-in). **The stand-ins that landed were deleted by Y11**, and the cases that
used them are ported in the same commit; the typed forms `update-or`, `update-opt` and `reduce1` (which keep the
`Option` where Clojure's `(f)` or `nil` have no type) stay as the typed twins. **[R]** A protocol method may not
share a name with a `defun` clause today (`f is already defined`), so L1 must say that a method and clauses of
other arities may coexist.

**Defaults.** A default method written once is specialised per type. `Reducible.size`, `nth`, `last` and
`to-vec` are the examples, overridden where the source knows better: by `Vec` ([R] A10 nth, rangesize) and, in
the same way, by `Array`, `Range`, `Option` and `SubVec`; an adaptor does **not** override `size`, so `count`
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
| `(get m k d)`, `(nth c i d)` | clause of `get`, `nth` (§7 L1, landed) | the clause has its own result type `v` |
| `(if (seq xs) ..)`, `(when (get m k) ..)`, `(while (peek s) ..)`, `(filter :active ps)`, `(remove nil? xs)` | **a condition of type `bool` or `(Option T)` is accepted**: truthy when `true` or `(some _)`; `(Option bool)` truthy when `(some true)` (Clojure's `false` is falsy) | memory-safe, typed and free at run time: the elaboration is a direct call to an identity (`fibc emit` of A11 e1z shows `(call @m.Truthy.truthy?.bool t1)` to a body `(ret p0)`); [sketch] checker rule §7 L20; the library runs today as the `Truthy` protocol (§2.3, A11 t04: `if`, `or` with a default, `or` of two options, `and` and `when` over `bool` and `Option`, 127 under both tools). Today `(if (get m 1) 1 2)` is `cannot unify (Option i64) with bool` (A11 t01), `(or (get m 2) 7)` the same (t02), `(if 5 1 2)` `cannot unify i64 with bool` (t03). Any other type as a condition stays a compile error: it cannot be false (`(if* 5 1 2)` is `no implementation of Truthy for i64`, A11 n15) |
| `(or (get m k) d)`, `(or (get m k) (get m j))`, `(and (get m k) (> x 0))` | Clojure's text: `or` and `and` are checker forms typed by their **last** operand: `(or (Option T) T)` is `T`, `(or (Option T) (Option T))` and `(or bool bool)` keep their type; `and` has the type of its last operand; a first operand of another type is the compile error above | one `or` for two result types needs the operand types, not just the first (an instance per head overlaps: `OrHit for (Option a)`, A11 t70, e2): a checker rule, or L23. The macros `or-d` and `or-e` of A11 t04 run today as two names; `(unwrap-or o d)` is the spelling that needs nothing |
| `(some pred c)` | `(Option payload)`: `(some even? c)` is `(some true)` or `nil`, `(some #(get m %) ks)` the first non-nil value | over the predicate's `Truthy` result ([R] A11 `some`: `(some true)`, `(some 20)`, `nil`); `some` is also `Option`'s constructor, so the two arities are one name by L1 extended to a constructor (A11 n13: a `defun some` of two parameters makes `(some 5)` `some takes 2 argument(s), got 1`) or a Rust macro that picks by argument count; `find-first` and `find-map` are extras |
| `(if-let [x e] a b)`, `(when-let ..)`, `(if-some ..)`, `(when-some ..)`, `(when-first [x c] ..)` | `if-let` over a `Truthy` (`x` binds the payload; the pattern may be any pattern, refutable ones included: the else is taken on a mismatch); `if-some` is the presence test; `when-first` is `(when-let (x (first c)) ..)` | `(if-let ([a b] o) ..)` expands to `(match e ((some [a b]) a) (_ b))` so a shorter vector falls to the else (the macro of `36ed472` expanded to `(nil b)` and rejected it: `non-exhaustive match: missing (some [])`, [R] A10 iflet; since E13 it expands as stated, syntax §4.4, cases 800 to 802); `[a b :as all]` is `([a b] :as all)` |
| `(when c body)`, `(if c a)`, `(cond t1 e1)`, `(when-let ..)` without an else | **unit when the body is `unit`, else `(Option T)` with `some` around the body**; `(keep #(when (even? %) (* % 2)) xs)` is Clojure's text | `(when c 5)` is `cannot unify unit with i64` today (A11 t82, k7); a macro that wraps in `some` runs (12, A11 t83); the general rule needs the arm's type, so it is a checker rule (§7 L20 [sketch]). `cond` with no matching clause and no `:else` is `nil`/`unit` the same way; `case` and `condp` with no match trap, as Clojure throws |
| `(map f nil)`, `(count nil)`, `(first nil)` | an empty seq, 0, `nil` | `Option` is `Reducible` ([R] A11 n9: `(first nil)` is `nil`, `(count nil)` 0) |
| `(conj nil x)`, `(assoc nil k v)`, `(merge nil m)` | a **literal** `nil` as the first argument is a form the macro sees: `(conj nil x)` is `(conj (list) x)`, `(assoc nil k v)` is `{k v}`, `(merge nil m)` is `m`; an `Option`-typed value is `(conj (or o []) x)` (of the three only `merge`'s literal `nil` is skipped, as the `merge` macro of R6a does; the `conj` and `assoc` rules are not implemented, syntax §4.4) | the result type of `conj` on a `nil` of unknown collection has no single answer (`List` for nil, `Vec` for a vector, the static-typing fact §5 T1): `no implementation of Collection for (Option a)` (A11 n7) names it |
| `(get nil k)`, `(get (get m :a) :b)`, `(-> m :a :b)` | `Lookup` for `(Option s)`: a lookup through `nil` is `nil` (**after §7 L16, Decided**) | `(impl (Lookup k v) (Option s) :where ((Lookup s k v)) ..)` is `type variable k is not a parameter of the impl head` today (A11 t23) and `an instance head is a type constructor applied to distinct variables` for a nested head (e5); `get-in` and `some->` are the spellings that run today |
| `(update m k f)`, `(update m k f x ..)` | `f` receives the value, and extra arguments as in Clojure; a missing key traps `update: no key` where Clojure's `(update m :n inc)` throws an NPE; a literal `(fnil g d)` in the `f` position is routed to `update-or` | `(update m "a" inc)`, `(update m "w" (fnil + 100) 7)` and `(update m "a" + 10)` are 6, 107 and 15 in one program ([R] A11 t24: a macro that sees the literal `fnil`); `update-or` (a default) and `update-opt` (hands `f` the `(Option v)`) are the typed forms for an `f` that is not a literal; the 4-arity of `update` is Clojure's extra argument, so the default form keeps its own name, `update-or`; the `update` macro of R6b does exactly this (syntax §4.4, cases 782 and 783) |
| `(some-> o f g)`, `(some->> o f g)` | each step takes the unwrapped value; a step that returns an `(Option b)` is kept and a plain `b` is wrapped, so Clojure's text works | a library cannot tell the two apart (no blanket impl, A11 t06): a checker elaboration (§7 L22 [sketch]); until then `(some-> o (and-then f) (map-opt g))` |
| `(empty c)` | `Emptyable`: the empty collection of `c`'s type | a method with a `self`, so it type-checks today; `(empty 5)` has no type (Clojure returns `nil`) and is not offered for a non-collection |

An `Option` of an object type is a null pointer and allocates nothing; an `Option` of a scalar, and
every enum with a payload, is a heap object in the interpreter and the stage-1 compiler ([R] A5: 200 allocations for 100 calls);
in stage 2 an `Option` of a scalar is a pair held by value (types §8.1; §7 C5, first half; `cases/ownership` 267, 270), so `get` on
a `(Map i64 i64)` allocates nothing for its result there and once per hit in the interpreter. The cost was of the
representation, not of the API, so the API did not bend to it. The printed form of an `Option` is Clojure's
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

**`(update! c f)` is the in-place form for a value in a `Cell`** (row `update!`): `(set! c (conj @c x))` copies because `@c` acquires, and `(update! c (fn (v) (conj v x)))` moves the value out of the cell for the call of `f`. Cells only; an `Atom` copies (it is shared) and keeps `swap!`.

**A loop of `conj` or `assoc` is in place iff the accumulator is unique.** The table and the analysis below are the
state measured at `de1c007`, before the in-place update landed (performance batch 2, design
docs/design/in-place-update.md), when it never was ([R] A4). What landed since is in "When is an update in place"
below; the counts of the table have not been re-measured in this section:

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

**When is an update in place (for users).** What follows is checked against the implementation named in each line;
spec/types.md §2.13.1 has the primitives.

1. **In place means: the value has one holder at the moment of the update, and the update takes that holder's
   reference.** The test is made at run time, by `fib.unique?` (`rt/core.lir`): none of the flags `SHARED`,
   `IMMORTAL`, `STACK`, `HAS-WEAK`, and count exactly 1. If it fails, the primitive copies, stores the copy into the
   place and releases the old value; the old value is not written. So a value that anyone else holds is never mutated,
   whatever the program does, and a wrong guess by the compiler about uniqueness can only cost a copy.
2. **The cell idiom is how library code gets there.** `lib/prelude.fib` updates a vector tail or a trie node by putting
   the array in a local `cell`, moving a child out with `array-take!`, and writing it back with `array-set!` (`vnode-assoc`,
   `vnode-pop`, the descent of `mnode-assoc` and `mnode-dissoc`), appending with `array-push!` (`vec-conj`, `vnode-push`)
   and removing with `array-pop!` (`vec-pop`, `vnode-pop`). `vec-assoc`, `vec-conj` and `vec-pop` use them for the
   tail and the path to the leaf, and `assoc` of an **existing** key of a `Map` or `Set` uses them for the descent.
   Inserting a **new** key still copies the array of the node that gains it (`array-insert`, `array-remove` in the
   prelude are not in-place primitives).
3. **A cell holding the only reference lets `update!` work.** `(update! c (fn (v) (conj v x)))` moves the value out of
   the cell for the call of the function (`cell-update!`, `compiler/emit/lower/cells.fib`), so `conj` sees count 1 and
   may write in place. `(set! c (conj @c x))` does not: `@c` acquires, so `conj` sees count 2 and copies
   (`lib/fib/core/cells.fib`). If something else also holds the value (another variable that is used again, a field of a
   struct, an element of another vector, a snapshot read with `@c` before), the count is 2 or more and `update!` copies;
   cases 4050 and 4051 are written to fail if it did not.
4. **A variable's last use hands over its count.** The ownership pass of the compiler in fibber (`compiler/own/lastuse.fib`)
   moves a binding's reference into the call at its last use, and takes a field out of a dead owned shell at its last use
   (`compiler/mirror-pending/last-use.md`). This removes the extra count that made the loop of §2.5 above copy. A variable
   that is used again after the call is retained first, has count 2, and the call copies, correctly. Two more rules make
   the library's own functions fire (`compiler/own/unit.fib` `decide`, `compiler/own/walk/state.fib` `w-want-own`): a
   `defun` parameter that is a borrowed shell is **inferred owned** when a field of it would be taken out at its last use
   (so `vec-conj`, `vec-assoc`, `vec-pop`, `map-assoc` take their vector or map owned, and a caller that uses its value again
   retains it first), and the payload of a direct `(some x)` sub-pattern is a field slot of its shell and is taken out too.
   The methods `conj`, `assoc`, `dissoc` and `pop` declare `self :owned` (`lib/fib/coll/protocols.fib`) for the same reason.
4a. **A constructor that is the last use of its shell is built in the shell** (shell reuse; `compiler/own/lastuse.fib`
   `record-reuse`, `compiler/emit/lower/objects.fib` `build-object`). When a function destructures a value with a
   constructor pattern and builds a value of the same variant, and nothing reads the shell afterwards, the emitted code
   tests `fib.unique?` of the shell: unique, the fields of the shell are overwritten (the old values released, a field that
   was taken out is null) and the shell is the result; else the object is allocated as usual and the shell released. This
   removes the `VecOf`, `VBranch` and `VLeaf` shells of an update: 1000 `assoc` on a unique 100000-element vector allocate
   nothing (case 4010), 1000 `conj` allocate only the tail arrays and leaves, one in 32 (case 4011), and 1000 `assoc` of
   present keys on a `Map` allocate nothing (case 4015). It never applies to a stack object or to a shell of another type.
5. **Atoms are never unique.** A value stored in an `Atom` is marked `SHARED` before the store (types §6.3, `fib.share`,
   `rt/atom.lir`), and `fib.unique?` refuses a `SHARED` object, so `swap!` works on a copy. `(update! an-atom f)` is a type
   error (case 4052). The same holds for anything a task captured: `spawn` shares its captures, so neither side updates
   such a value in place. The copy that an update makes is a fresh object, and the updater may update it in place afterwards.
6. **Literals and `def` values are never unique** (`IMMORTAL`), nor is anything that a `weak` ever pointed to (`HAS-WEAK`,
   never cleared). `(assoc [1 2 3] 0 9)` on a literal copies.
7. **`&` parameters and `push!`.** An `&` argument is copied in with a retain (types §6.6), and the prelude defines
   `push!` as `(set! v (vec-conj @v x))` (`lib/prelude.fib`), where `@v` acquires as well; so a `push!` call is not an
   in-place update of the caller's vector. Code that wants the in-place path keeps the accumulator in a `loop` variable
   or in a cell updated with `update!`.
8. **How to tell.** `fibc run --trace` prints one `A` line per heap object allocated; a count case (`allocs: <= N`, spec/method.md
   rule 3) bounds it. The cases that are written for this are `cases/stdlib/4000` to `4016` (persistence, and counts: none is
   `open` now) and `4050` to `4054` (`update!`), and `cases/ownership` 248 to 266 (the rules above, each with the case that
   fails when the rule is wrong); `scripts/mutant-unique.sh` checks that the persistence cases fail when `fib.unique?` lies.

**What is tested.** The zero-cost claims are tests with counts (§8): a three-stage pipeline over n and
over 10n elements allocates the same number of objects (passes today: +4 for 1000, A1), a bulk `vec` of
n allocates at most n/32 + c, and a `conj` loop of n allocates about n/32 + c on a unique accumulator (case 4011:
190 for 1000, 2094 before the in-place update; a struct `Vec` is not needed). The harness had no object
count at `36ed472`: a case header was `spec expect result audit error trap`; §7 H1 (landed, §7.5) added `allocs` (a maximum), `covers` and `open`, and the
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
  one (`Vec SubVec List VSeq Range Array`, recipes, `LSeq`), the map one (`Map`, `SortedMap`) and the set one (`Set`,
  `SortedSet`); where the operand types of `=` differ and belong to one family, the checker elaborates it to
  `seq=` (or its map and set twins), **[sketch]** §7 L24; today `(= [1] (list 1))` is `cannot unify` and the
  library form is `(seq= a b)` ([R] A10 showseq: `(seq= (map inc [1 2]) [2 3])` is true). Across families
  Clojure answers `false`; here a `Vec` and a `Set` do not unify, which is the type error of the decided
  fact that `=` has one type (§5 T7). Floats follow IEEE (`(= nan nan)` false, `(= 0.0 -0.0)` true). **Numbers:**
  with the owner's L19 (**Decided**) a literal adopts the other operand's numeric type, so `(= 1 1.0)` is
  `(= 1.0 1.0)`: **true**, which is Clojure's `==` and not its `=` (false). That is a consequence of L19, recorded
  here and in §5 C1; `==` is an alias of `=`. For two variables of different numeric types `(= n 1.0)` is
  `cannot unify f64 with i64` today (A11 t62, n3) and the operators promote at a mixed call by the rule (§2.8, L26 b).
* `Eq Hash Show Debug ToStr` exist for `Vec Map Set Pair Triple Option List`, `Ord` for `Vec Pair Triple Option List`
  (not for `Map` and `Set`: Clojure's `compare` throws on a map), and `derive` for user types, which derives
  `Eq Ord Hash Show Debug ToStr` since R8 (commit `605a26e`; at `36ed472` `(derive Debug P)` was `cannot derive Debug: only Eq, Ord, Hash and Show`, [R] A10
  dd; now any other protocol is `cannot derive X: only Eq, Ord, Hash, Show, Debug and ToStr`). The text of a derived `Debug` is the record text of the
  table below, `#m.P{:x 1, :y "x"}`, with three rules the page left open (syntax §3.16, cases 810 to 817): `m` is the module the `derive` form is
  expanded in, because the expander's type table does not record the module a type was defined in, so a `derive` placed in another module than the
  type's prints the deriving module; a variant field written without a name is called by its position, `#m.Wrap{:0 3, :1 "q"}`; and a derived `ToStr` is
  the same text built from the fields' `Debug`, so a type that derives only `ToStr` has `to-str` and no `debug` (a recursive one fails with `no
  implementation of Debug for ..` at its recursive field). An enum none of whose variants has a field gets nothing for `Eq Ord Hash Show` (the compilers
  have those) and does get `Debug` and `ToStr`. The derived instances name `fib.core/Debug`, so the deriving module needs `fib.core` in scope (a
  `:use fib.core` until the implicit list is filled, §6.2; otherwise `unknown protocol fib.core/Debug`). `defstruct` and `defenum` deriving all six by
  default is the second half of L17 and is not done (§7.5). At `36ed472` `(= [1 2] [1 2])` was `no implementation of Eq for (Vec i64)` ([R] A6); the library's `Eq (Vec a)` (`fib.seq.hash`, wave 1,
  commit `4485838`) makes it work, and a `Vec` a map key (A7). **`Ord` on a `Vec` is Clojure's: the shorter vector is
  less, then element by element** (`(sort [[1 2 3] [9 9] [1 2]])` is `[[1 2] [9 9] [1 2 3]]`, A11 t1r); `Ord (List a)`
  is lexicographic (Clojure's lists are not comparable, so Rust's order).
* **`Ord str` is code-point order** (**Decided**, owner-invited, 2026-10-01, the owner may overrule; §5 D5, §9.1): the order of the
  UTF-8 bytes, which is the builtin instance (`memcmp` in `rt/str.lir`, `fibref`'s own compare) and the lexicographic
  order of the string's own `char`s, a `char` being a Unicode scalar (types §1.1, **Decided**): with `a` the one-character string
  U+FFFF and `b` the string U+1F600, `(< a b)` and `(< (i32->char 65535i32) (i32->char 128512i32))` are both true and `(< b a)` false
  ([R] A13 ordstr, `true true false true` under both tools), where Clojure, comparing UTF-16 units, puts the surrogate pair first
  (`(compare "😀" "￿")` is negative **[K]**). The two orders differ only when a character above U+FFFF meets one in U+E000 to
  U+FFFF (the 1068 of 5184 pairs on which A12 u16cmp found them apart); on every other pair, the Basic Multilingual Plane included,
  they agree. The owner invited the argument for code-point order (a string order that disagrees with the order of its own
  characters would be inconsistent, and it needs no runtime change) and decided it; the second revision's UTF-16 fix-up (`cmp-u16`,
  A12 u16cmp) and the runtime change it needed are withdrawn. `<`, `compare`, `sort`, `sorted-map` and `max` of strings use the
  builtin order; `Eq` and `Hash` are unchanged.
* **Floats print as Clojure's** (**Decided**, owner, 2026-10-01; §9.1): `str`, `println` and `pr` give Java's `Double.toString`
  text, positional for `1e-3 <= |x| < 1e7` with at least one digit after the point, otherwise `d.dddE<exp>` with no plus sign, the
  shortest digits that read back to the value, and `NaN`, `Infinity`, `-Infinity`, `-0.0`: `1.0E21`, `1.0E-7`, `1.0E7`,
  `1.23456789E7`, `0.001`, `1.0E-4`, `100.0`, `9999999.0`, `-0.0`, `Infinity`, `-Infinity`, `NaN`, `-1.23456789E9`, `1.5`, `0.1`;
   **the three non-finite values are the exception** (found by differential testing against Clojure 1.12, 2026-10-02): only `(str x)` of a bare
   float gives `Infinity`, `-Infinity`, `NaN`; `pr`, `print`, `println` and the text of a float inside a collection (`(str [x])` too) write the reader
   forms `##Inf`, `##-Inf`, `##NaN`. Library `Debug` does this (case 2973); the builtin `Show`, used by `print` and `println`, still writes
   `Infinity` (open case 2974, X3)
  ([R] A13 fltfmt: a library function over today's `show` prints exactly these fifteen texts under both tools, so the rule is
  specified by a program). It amends the owner's decision of 2026-09-30 (types §2.12: positional, never an exponent, `inf`,
  `-inf`) and is a runtime change in both tools, §7 C12 (tranche 0: the text of a float is observable by everything);
  `parse-double` already reads `Infinity` (§2.9). Until C12 lands `(show 1e21)` is `1000000000000000000000.0` and
  `(show (/ 1.0 0.0))` is `inf` ([R] A13 fl; §5 S18). Floats are `f64` by default, as Clojure's doubles, and float literals read
  as they do today.
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
  NaN when either argument is NaN, as Clojure's do (A10 nan). The rule replicates the sharp edges (§5.6);
  the owner may strike any of them (§9.1 Q33).
* `Hash f64` hashes `-0.0` as `0.0` and one NaN, so `(get (assoc m 0.0 1) -0.0)` finds the key
  ([R] it is `nil` today, A6). A NaN key is never found, as in Clojure. This is the contract of `Hash` (`=` implies
  equal hashes), kept because a lookup that misses `-0.0` after storing `0.0` is the failure the HAMT must not
  have; whether Clojure's `(hash 0.0)` and `(hash -0.0)` differ was not verified (no Clojure on the machine), and
  if they do, the rule says to replicate Clojure's (§9.1 Q33), a one-line change (types §2.12 hashes a float by its bits, which already differs for `-0.0`). **The combiners of today trap.**
  `derive Hash` and the prelude's `Hash (List a)` fold with `h*31 + hash x`, and a struct of two strings or a
  list of three is `trap: integer overflow in * at i64` under both tools ([R] A10 hash), so a `Map` keyed by
  a two-string struct or a `(Vec str)`, the bread and butter of a symbol table, fails in tranche 1 as soon as
  the keys are long enough. `hash-combine` is therefore a **rotate-and-xor mixer built from `shl`, `shr`,
  `bit-or` and `bit-xor`, which never trap and exist today**: it hashes a 30-element `Vec` and a `Vec` of
  strings without a trap and tells `["ab" "c"]` from `["a" "bc"]` and `["a" "b"]` from `["b" "a"]` (A10
  hash); `derive Hash` and every `Hash` instance of the library use it, and `hash-unordered-coll` folds the
  elements' hashes with `bit-xor`, which is commutative and does not trap, so `Hash (Map k v)` and `Hash (Set k)`
  are order independent. Both ship in tranche 0 and 1 (§8.2); the multiplicative finaliser that needs a wrapping
  multiply (§7 L10) replaces the rotate-xor in tranche 3, in one commit with the new integer hash (§9 Q15, **Decided**). **Update:** the working tree's
  `lib/prelude.fib` (committed since the second revision read it: HEAD and the tree agree) defines a `hash-combine` of its own, not this rotate-and-xor: a rotate by 7, an xor with a constant and
  eight invertible xorshift steps, pinned by cases 198 to 200 (read, not run); it also never traps, the library adopts it, and the rotate-xor
  of A10 hash is the prototype's.
* **Three printers, with Clojure's meanings.** `str` is `ToStr`, `print` and `println` are `Show` (display),
  `pr`, `prn` and `pr-str` are `Debug` (readable); `dbg` is the prelude's debugging macro, which prints with `Show`
  (`(fib.prelude/show t)`, syntax §4.4: it changes to `Debug` only if the owner wants Clojure's `pr`-style text for it). The three differ on a string inside a collection and on
  `nil`, as Clojure's do:

  | value | `(str v)` | `(println v)` | `(pr v)` |
  |---|---|---|---|
  | `"a"` | `a` | `a` | `"a"` |
  | `["a" 1]` | `["a" 1]` | `[a 1]` | `["a" 1]` |
  | `\a` | `a` | `a` | `\a` |
  | `:k` | `:k` | `:k` | `:k` |
  | `nil` (an empty `Option`) | the empty text | `nil` | `nil` |
  | `(some "a")` | `a` | `a` | `"a"` |
  | a recipe, `List`, `Range`, `LSeq`, `VSeq` over `1 2` | `(1 2)` | `(1 2)` | `(1 2)` |
  | `Pair`, `Triple`, `Vec`, `SubVec` | `[1 2]` | `[1 2]` | `[1 2]` |
  | `Set` | `#{1 2}` | `#{1 2}` | `#{1 2}` |
  | `Map` | `{1 2, 3 4}` | `{1 2, 3 4}` | `{1 2, 3 4}` |
  | a record (`defrecord`, `defstruct`, `derive Show`/`Debug`) of module `m` | `#m.P{:x 1, :y "x"}` | `#m.P{:x 1, :y x}` | `#m.P{:x 1, :y "x"}` |
  | an enum variant with fields, `(Done 3)` | `#m.Done{:v 3}` | `#m.Done{:v 3}` | `#m.Done{:v 3}` |

  `Debug` of a `str` quotes it and escapes the seven characters of Clojure's `char-escape-string`: `\"` `\\` `\n` `\t` `\r` `\f` `\b` (`debug-escape` in
  `fib.core.protocols`, case 613; a char's text is case 602); every other character, a non-ASCII one included, is written as it is. A record prints as Clojure's record does, the field names being static (`#m.P{:x 1, :y "x"}`; Clojure's text has the namespace where this has the
  module; `(str r)` is believed to be the `pr` text **[K, not verified: no Clojure on the machine]**, and `println` leaves a string field unquoted
  as it does at every depth); a field-less enum shows as its variant's name (types §2.12, **Decided**); an enum variant with fields is a record of
  the variant. `(str ["a"])` is `["a"]` and `(println ["a"])` is `[a]` in Clojure, which is the `ToStr` of a collection being its `Debug`
  text and the `Show` text being the display form; the first version of this page made `str` and `println`
  agree, which is a cost-free rule that Clojure does not follow. `(str nil)` with a literal `nil` is the empty
  text (a macro sees it; the bare `nil` has no type, `ambiguous constraint Show a`, A11 t64), and a literal `nil` argument of `println`, `print`, `prn` or
  `pr` prints the word `nil` (the macros of R5 turn it into the string `"nil"`, for the same reason: syntax §4.4, case 751; a string literal argument of the
  three Show and Debug printers goes through `show` and `debug` like any other, so `(println s)` of a `str` copies it once, one more heap object than
  the one-`str` function of `36ed472` allocated, case 614); `(str o)` of an
  `Option` is the empty text or its payload's `str` ([R] A11 e11: `x=1`, `y=`, `v=["a" "b"]`, `[a b]`). A present value prints as itself, as in
  Clojure, which has no wrapper: the prelude's `Show (Option a)` printed `(some 3)` at `36ed472` ([R] A11 t87: `(some 3)`,
  `(some a)`) and, since R2 (commit `5ea989f`), is written by hand in `lib/prelude.fib` (the derived `Show` of `Option` and `List` is gone from
  `PRELUDE_SOURCE`) to print the payload, `nil` as `nil`, and a `List` as `(1 2)` or `()`; an `(Option (Option a))` is as
  ambiguous as Clojure's nested nil. A seq (a recipe, `List`, `Range`) prints in parentheses and a vector in
  brackets, which the first version collapsed to brackets: `(show (map inc [1 2 3]))` is `(2 3 4)`, `(show (take 3
  (iterate inc 5)))` `(5 6 7)`, `(show (filter odd? (range2 0 10)))` `(1 3 5 7 9)` (A11 showseq); a recipe's `Eq` is `(seq= a b)`, not `=`.
* **Map and set iteration order is unspecified**, as in Clojure: the HAMT's order, a function of the hash and the
  insertion history, the same in the interpreter and the compiled program (the hash is part of the spec). **A
  `Map` of at most 8 entries keeps insertion order, as Clojure's array map does**: it is a `Vec` of entries
  until the ninth key and a HAMT after (library code over `Pair`, [R] A11 e13: keys `b a c` print `{b 2, a 1, c 3}`,
  `assoc` of an existing key keeps its position `{b 20, a 1, c 3}`, nine integer keys then print in hash order
  `{0 0, 1 1, 2 4, ...}`), so a small literal prints as written and small maps get faster; the compilers know `Map` only through `Form`'s
  `Map` variant, so the change is the library's. **`hash-map` and `array-map` are two spellings of two shapes of one type, as
  Clojure's:** the literal `{..}`, `array-map`, `(into {} ..)`, `zipmap` and `group-by` start in the array shape, which keeps insertion order and leaves
  it when an `assoc` takes the map over 8 entries (Clojure's `PersistentArrayMap`), and `hash-map` builds the HAMT shape from the start, so
  `(hash-map :b 2 :a 1)` prints in hash order where `{:b 2 :a 1}` and `(array-map :b 2 :a 1)` print as written; a `Set` is always hashed (Clojure's
  `PersistentHashSet`). The shape is a tag inside `Map` (**[H]**: the array shape ran, A11 e13; the tag was not written). `Show` and `Debug` print the iteration order, not a sorted
  order. Tests compare with `=` or sort; the change of the integer hash (§9 Q15) changes the order of maps of more
  than 8 entries once, in both implementations together. A numerically ordered print is a `SortedMap`
  (`fib.sorted`).

### 2.8 Numbers

Arithmetic is the builtin binary `Num` method on one type; the variadic spellings are macros that
fold (`(+ a b c)` is `(+ (+ a b) c)`, `(< a b c)` is a short-circuit chain). **`(+)` is `0` and `(*)` is `1`, as Clojure's**:
the macro expands to the literal, which adopts the numeric type its context requires (L19), and is an `i64` when
nothing constrains it, so `(reduce + [])` style code has a zero. Overflow traps at every width (Clojure's `+` throws on a `long`
overflow too; types §2.12; **Decided**, §9 Q15); float arithmetic is IEEE.

**Division (Q40, Decided, owner, 2026-10-01).** `/` on two integers is exact, as Clojure's: `(/ 7 2)` is the ratio `7/2` and
`(/ 6 3)` is `2`; `quot` truncates toward zero, `rem` has the dividend's sign and `mod` the divisor's, on integers and on floats
alike (`(quot 7.5 2.0)` is `3.0` and `(rem 7.5 2.0)` `1.5`, as Clojure's; [R] A13 div). The typed design, since `Num`'s methods have
the type `Self Self -> Self` (types §2.12) and a function generic over `Num t` is checked once: **`/` leaves `Num`** and is the one
method of `(Div a r)`, a protocol whose result type the instance determines (§2.3): `i8..i64` divide to `(Ratio t)`, `f32` and `f64`
to themselves, `(Ratio t)` to `(Ratio t)` and `BigInt` to `(Ratio BigInt)` (tranche 5). Generic code therefore has one answer
whatever the caller's type: `(defun half (x: t) :where ((Div t r) (Num t)) -> r (/ x (+ x x)))` is `1/2` at `i64`, `0.5` at `f64`
and `1/2` at `(Ratio i64)`, and `(/ (sum xs) (count xs))` over integers is `5/2` ([R] A13 div: `[7/2 2 -3/2 1/2 0.5 1/2]`, `5/2`,
`7/2` at `i32`; `div` stands for `/` in the prototype, `/` being the builtin's name today). `Num` keeps `+ - * neg quot rem`
(`quot` is the builtin integer `/` of `36ed472`, LLVM's `sdiv`, with the same traps, whose texts keep naming `/`; **on floats it is the quotient rounded to the
operand's width and then rounded toward zero, as Clojure's `quot` on doubles is, and it never traps**: a zero divisor gives an infinity or a NaN where Clojure's
throws, and a negative quotient that rounds to zero is `-0.0`. The compiled lowering is libm's `trunc` (`truncf` at `f32`) of the `fdiv`, not the
`(fdiv (fsub a (frem a b)) b)` this page first named, which gives `2.9999999999999996` for `(quot 9.6 2.8)` where the answer is `3.0`, and differed
from `(a / b).trunc()` in 9296 of 100000 random pairs; `fdiv` is a method of the builtin protocol `Float`, whose instances are `f32` and `f64`, and not a
primitive. The names and the split are **Decided** (Q40); the float details are **Proposed**, types §2.12, and cases 206 to 209 pin them), so a function
generic over `Num t` that divides writes `quot` or constrains `Div`, and a loop that wants the machine's division writes `quot`.
This is §7 L30: the rename of the builtin in both tools, an `fdiv` primitive for the float instances, the `Div` protocol and
`(Ratio t)`, all in tranche 1; the compiler's one integer division (`compiler/syntax/number.fib`), the three case files that divide
integers (102, 103, 190), the generator and two unit tests write `quot` (§8.3). The result type of Clojure's `/` depends on the
values (a `Long` when the denominator is 1), which §5 T5 forbids: `(/ 6 3)` is a `(Ratio i64)` that prints `2`, and the predicates
look at the value, so `(ratio? (/ 6 3))` is `false` and `(integer? (/ 6 3))` `true`, as Clojure's are (A13 div: `[true false]`).
Until L30 lands the builtin `/` truncates (`(/ 7 2)` is `3`, [R] A11 k9; §5 S17), and a generic `(/ x (+ x x))` at `i64` is `0`
([R] A13 quot), which is the inconsistency the `Div` protocol removes. L30 has landed (§7.5): `quot` and `Float.fdiv`
(commit `3d63d00`), the library's `Div` and `(Ratio t)` (`fib.core.num`, commit `4485838`, with the instances of §2.3 and `numerator`, `denominator`, and
the Ratio's `Show`, `ToStr` and `Debug` text `7/2`, `2` for a denominator of 1), and, with the flip, the third step: `/` left `Num` and the builtin (the checker's table, the
interpreter and `fibc`'s lowering) and the `Num` instance of `Ratio` lost its `/`, so `/` is `Div`'s method in every module (case 873: a function whose bound is `Num t` and whose body
divides two `t`s takes `Div`'s `/`, the checker adds the `Div` bound, and a call at `i64` is rejected because the quotient is a ratio; case 877: the same function at floats). The library writes
`quot` and never `/` on integers (plan PC-14).

**`(Ratio t)`** (tranche 1) is a struct of a numerator and a denominator of one integer width, normalised (lowest terms, a
positive denominator), with `Num`, `Div`, `Eq`, `Ord`, `Hash` and `Show`; `numerator`, `denominator`, `ratio?`, `rationalize`
(the exact decimal expansion of a float, `(rationalize 0.1)` is `1/10`, tranche 3), `double` and `long` (truncating) convert; the
reader literal `7/2` is E14 (a); a mixed operation adopts (an integer literal becomes the ratio with denominator 1, L19's rule
extended to `Ratio`) or promotes (`i64 < (Ratio i64) < f64` in the lattice of L26 b, so `(+ r 1.5)` is a float, as Clojure's is);
`compare` and `=` over two ratios are by value (`(< 1/3 1/2)`, `(= (/ 2 4) (/ 1 2))` and equal hashes: `[true true true]`, A13 div).
Its components are 64-bit (the operand width) and **trap on overflow** (`(+ 1/3037000500 1/3037000501)` is `trap: integer overflow
in * at i64`, [R] A12 ratio), only when a component of the *normalised* ratio, or an intermediate product, does not fit: a ratio is reduced by the gcd before its
sign is fixed, so `(/ -128i8 -2i8)` is `64/1` at `i8` (cases 093 to 098) while `(/ -128i8 -1i8)` is `128/1` and traps `integer overflow in / at i8`
(the exact division of the numerator by the common divisor), where Clojure's `BigInteger` ratio is exact: `(Ratio BigInt)` is the exact form, tranche 5 with
`BigInt`, and Clojure's auto-promotion to it is the one value-dependent result type that stays a deviation (§5 T5).

Literals have one type, so `inc`, `dec`,
`abs`, `zero?`, `pos?`, `neg?`, `even?`, `odd?`, `mod` take a `Unit` witness (`(one x)`), which makes
them generic over every numeric type ([R] A7: design "zero"'s `num1`). Conversions are checked:
`(long x)`, `(int x)`, `(byte x)`, `(double x)`, `(char n)` dispatch on the argument type and trap when
the value does not fit, as Clojure throws; `int` and `long` take a `char` too, so `(- (int c) (int \0))` is Clojure's text (`unbound name int` today,
[R] A12 int1; one `ToInt` and one `ToLong` instance for `char` run, A12 int2); `(trunc i8 x)` keeps the low bits, never traps, and is `unchecked-byte`
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
deferred constraint in both tools (**[sketch]**, §7 L26) and lifts D3; the rule settles it (§9.1, Q36) and the owner signs the §7 item.

**`BigInt`, `BigDecimal`.** Library types, not a non-goal: an explicit big type breaks neither memory safety nor static typing. A
`Ratio` struct with `(impl Num Ratio ..)` and a generic `(defun twice (x: a) :where ((Num a)) -> a (+ x x))` work through the
builtin names ([R] A11 t50, result 507); the reader literals are `1N` and `1M` (E14 c), `bigint`, `bigdec`, `biginteger` and
`with-precision` are library functions (tranche 5), and `+'`, `-'`, `*'`, `inc'`, `dec'` return a `BigInt` always: what stays a
deviation is Clojure's *auto-promotion* (a `Long` when it fits, a `BigInt` when it does not), whose result type depends on the
values (§5 T5).

**Randomness** is Clojure's: `rand`, `rand-int`, `rand-nth`, `shuffle`, `random-sample`, `random-uuid` use a
global generator, an `Atom` holding a xoshiro256** state (a struct of four `i64`s) seeded from the clock (a top-level `atom`, §2.11, needs L15),
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
UTF-16 unit, and the reason is a decided typing fact, not memory safety** (§5 T10). Clojure's counts are replicable and safe: a user-code UTF-16
`count` and `subs` run with a clean audit, `(u16-len "a😀z")` is 4, `(u16-subs s 0 3)` is `a😀`, and a bound inside a pair traps `subs: index splits
a surrogate pair` ([R] A12 p2, p2b); the first rewrite's claim that a `subs` through a pair would build an invalid `str` described a function that nobody has to write.
What decides the unit is `char`: it is a Unicode scalar in an `i32` (types §1.1, **Decided**), so the seq of a string has one element per scalar, and
Clojure's idiom `(dotimes [i (count s)] (nth s i))` needs `count` and `nth` to agree. With `count` in UTF-16 units and `nth` by element it fails on a
supplementary character: `(count "a😀z")` is 4 and the loop over `(nth cs i)` traps `nth: index out of range` ([R] A12 unit: `[4 3]`, `128731`, then the
trap); with both by scalar it runs. So `count`, `nth`, `get`, `subs` and `index-of` count scalars. The two agree on the Basic Multilingual Plane and differ
only for supplementary characters: `(count "😀")` is 1 here and 2 in Clojure. The order of strings is code-point order, the order of their characters (§2.7, §5 D5:
the owner-invited decision of 2026-10-01).

**The cost is O(n)** for `count`, `nth`, `subs`, `index-of` on non-ASCII text, since a character offset in UTF-8
is a walk. It is not a reason (§1.1), so the page follows Clojure and says what it costs: an **ASCII flag in the
`str` header**, computed during the UTF-8 validation that already scans every constructed `str`, makes all four
O(1) for ASCII text and a byte walk otherwise; it is a representation change in both tools (§7 C10, **[sketch]**),
and the rule settles it (§9.1, Q37). `(into "" xs)` is not Clojure (a string is not a collection that `conj` builds; it
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
that calls `str/index-of` from an offset copies the string each time; §7 L18 (landed, commit `3d63d00`) adds `(str-byte-at s i)` and `(str-find s pat from)`,
which copy nothing: `str-byte-at` allocates nothing and `str-find` allocates its result, one `(Option i64)` object per call, hit or miss, in the interpreter, because an `Option` of a
scalar is a heap enum there (types §8.1; `cases/ownership` 213 and 218, `cases/stdlib` 580), so a tokenizer's `str/index-of` from an offset costs one small object per call and no copy; stage 2 holds the `Option` as a pair and allocates nothing (C5, `cases/ownership` 270). At tranche 1 the character-offset functions are an O(n) walk per call (O(1) for ASCII text after C10), which is why a tokenizer, the
compiler included, uses the byte layer and not `str/index-of`. `parse-double` has the grammar `[+-]? (digits ['.' digits*] | '.' digits) ([eE] [+-]? digits)?` and
the names `NaN`, `Infinity` and `-Infinity` (**[H]**: the run of A10 pd covers the decimal part only); the grammar is checked in
fibber before `strtod` is called, because `strtod` accepts hex, a leading space, `inf` and trailing junk, and
`fibref` rejects hex (`unsupported: hex float`) where `fibc` prints 16.0, so the tools disagree without the
check ([R] A10 pd: the check followed by `strtod` gives the same eleven answers under both tools).

### 2.10 Errors and effects

A recoverable failure is a value where Clojure has no counterpart for a typed result, and an exception where it has: `trap`
is the only abort today (types §2.11, **Decided** 2026-09-28), and the rule says to follow Clojure unless memory safety
forbids. The two halves:

**Values.** `(Result a e)`, with `Ok` and `Err`, is in the prelude (`compiler/util/result.fib` was deleted when
the compiler's modules were ported, M2, §8.3), and `(try-let ((x e) (y f)) body)` binds each `Ok` payload in turn and yields the first `Err` as
the value of the whole expression. This is Rust's `?` ergonomics where Clojure has no typed counterpart. **It is not
`try!`**: the language has no early return, so a macro that leaves the enclosing function cannot be written
(`(return (Err e))` is `unbound name return`, [R] A10 try), and `try-let` is a block macro over nested `match` that runs
today, 106 under both tools (A10 try). `parse-long` and `parse-double` return `Option`, as Clojure's return `nil`; the Java-named parsers
(`Integer/parseInt`, `Long/parseLong`, `Double/parseDouble`), `slurp`, `re-pattern`, `read-string` and `max-key` of nothing trap, as Clojure's throw (§1.2 P5), and
each of them has a typed twin `try-<name>` that returns a `(Result a str)` for a program that must recover (§3 N13).

**Exceptions.** `throw`, `try`, `catch`, `finally`, `ex-info`, `ex-data`, `ex-message`, `ex-cause` and `Throwable->map` are Clojure's,
and **no memory-safety failure prevents them**: the failure the abort model avoids is a caught exception that skips the
releases of the frames it unwinds, which would *leak* those objects (the audit counts such a leak as an error, `leaks=`, §1.1,
so an implementation that skipped them would not pass) but would not corrupt anything: no freed memory is read, nothing is
freed twice, a surviving frame holds its own counts and a stack object is one the compiler proved non-escaping. So the
hazard is a leak to be avoided, not one of the four failures of P0 item 1. A correct implementation releases the live owned locals of every unwound scope, and the ownership checker
already computes those sets for every scope exit (types §6.3). So the cost is plumbing, not a prohibition: an early exit
that is also a scope exit for each frame between `throw` and `catch`, as either unwinding tables (lIR has none:
`(trap)` is `llvm.trap`, "without unwinding") or an effect "may throw", inferred transitively, that makes each such
function return a hidden `Result` and reuses the release code of the normal return. Both are large and neither can be
prototyped without compiler changes (**[sketch]**, §7 L28). Today a `trap` in a spawned task ends the whole process:
`trap: boom` then `Aborted` under `fibc`, `trapped:` under `fibref` ([R] A11 t40, e9). The plan, in order: (1)
`ex-info`, `ex-data`, `ex-message`, `ex-cause` are plain data now, a struct `ExInfo` of a message, a `(Map keyword Val)` and an
`(Option ExInfo)` cause, usable as the `e` of a `Result`; (2) a task's trap is isolated, `join` returning the trap as a
value (Rust's `JoinHandle::join`), which needs no unwinding in the parent; (3) `throw` and `try`/`catch`/`finally` by one
of the two shapes above. Step (3) amends a decision the owner made before the rule (types §2.11), which the rule reverses (§5 D1; §9.1 Q35: yes, after the library is
viable), so the owner signs it as a types amendment; until it lands
`(throw e)` is a `trap` of `(ex-message e)`, and a program that must recover returns a `Result`. `finally`'s
non-memory cleanup (closing a handle) is the scope-exit hook of §7 L12, after which `with-open` is a macro over it.

**Effects.** Randomness is Clojure's global generator and the explicit `rng/` module (§2.8); time and the environment
are functions of `fib.sys`. `println` is a macro over `Show` (several arguments are joined with a space, [R] A10 pm); the
prelude's one-`str` function of that name is replaced in value position by a **function twin** (`println` and `print` generic over `Show`, `str` over `ToStr`, `pr` and `prn` over `Debug`, as the
macros are, §2.7), so `(run! println [1 2 3])` works and `(map str [["a"]])` is `["a"]`, as Clojure's, which a macro alone cannot
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
`Atom`, whose payload is `Send`, is allowed. `volatile!` is an `Atom`: Clojure's volatile is visible across threads and its `vswap!` is not atomic, which an `Atom` with
`vswap!` as `(reset! v (f @v))` is, and which is memory-safe (two tasks adding 300 each give a result in 1 to 600 with a clean audit, [R] A12 p12); `vreset!` is
`reset!` returning its value. A `Cell` is the thread-confined form and stays out of `def`s (§5 M1), **and so does every type that
holds one**: an `LSeq` (§2.1 rule 2), a `Delay`, an `MArray`, a `Matcher`. A global lazy value or a global `delay`, the commonest
Clojure globals after an atom, wait for the run-once cell of §7 C9 (recommended for tranche 3, §9.2 Q41); until then
`(defn fibs [] ..)` builds the seq per call, and a global memo is an `Atom` whose thunk may run twice under a race (A11 e8e).

**Arrays.** Clojure's arrays are mutable objects that every holder sees. The builtin `(Array a)` is a value instead (a write copies unless the array is
unique, `array-set!` through `&`), which is the form for fibber code and the one the nodes of the persistent collections use. Clojure's text needs the shared
object, so `long-array`, `make-array`, `to-array`, `aclone` and the rest return an `(MArray t)`, a struct around a `(Cell (Array t))`, and `aget`, `aset`, `alength`,
`amap` and `areduce` read and write it in place: **every holder sees the writes**, inside one task, as Clojure's `aset` does (`b` and a closure that share `a` give
`[9 4 0]`, and an array of objects `[y x]`, [R] A12 aset1, aset2; the write through a cell that two closures share is Clojure's behaviour, A12 p1, p10). What stays
a deviation is the race: Clojure lets two threads write one array and the cell-capture check does not, `cell cannot be shared between threads: closure capture a,
field c of MArray has type (Cell (Array i64))` ([R] A12 aset3, §5 M2). `vec`, `to-array` and `aclone` copy, so a persistent collection never shares a node
with an array that a user can write. The cost is a struct, a cell and the array behind every access, which a loop that cares avoids with the value form (`&` with
C3, C4).

```lisp
;; ran: fib.array (A12 aset2)
(defstruct (MArray t) (c: (Cell (Array t))))
(defun make-marray (n: i64 x: t) -> (MArray t) (MArray (cell (array n x))))
(defun aget (a: (MArray t) i: i64) -> t (let ((c (. a c))) (array-get @c i)))
(defun aset (a: (MArray t) i: i64 x: t) -> unit (let ((c (. a c))) (array-set! &c i x)))
(defun alength (a: (MArray t)) -> i64 (let ((c (. a c))) (array-len @c)))
;; (let ((a (make-marray 3 0))) (let ((b a) (w (fn (i: i64 x: i64) (aset a i x)))) (w 0 9) (aset b 1 4) ..)): a shows [9 4 0] and b is the same array
```

**Dynamic vars.** `(def :dynamic *x*: T v)` (Clojure's `^:dynamic`) declares a var of one type; `(binding [*x* e] body)`,
`with-bindings`, `bound-fn` and `with-redefs` (below) rebind it for the dynamic extent of the body. A binding is an owned,
counted value released at the scope's exit; there are no exceptions, so no unwinding issue; the bindings are conveyed to a
`spawn`ed task as a captured `Send` value, as Clojure's `future` conveys them. It needs a per-task binding slot in the task
header (§7 C11). `*out*`, `*in*`, `*err*`, `*print-length*` and `*command-line-args*` are such vars; `println` writes to
`*out*` (a `(dyn Writer)`), and `with-out-str` binds it to a string-building writer (the `StrBuf` of §9 Q26), `with-in-str`
likewise, and `(args)` is the builtin under the Clojure name `*command-line-args*`.

**Metadata.** `meta`, `with-meta`, `vary-meta`, `alter-meta!` and `reset-meta!` are a `(Option (Map keyword Val))` field on the four
collection headers and on structs; `Eq` and `Hash` ignore it; `^:private` is the `:private` qualifier, `^:dynamic` the
`:dynamic` qualifier and `^Type` the annotation `x: T` (checked). A representation change with `Val` (§9 Q20, kept on §5 T4:
`fib.data`), tranche 5.

**Multimethods and hierarchies.** `(defmulti area (fn (s) (:type s)))` declares a `(Multi d a r)`, a global `(Atom (Map d (fn :send (a) r)))`
and the dispatch function; `defmethod` is a top-level form that registers a method in the module init; `derive`, `isa?` and
`parents` build and query an explicit hierarchy value held in an `Atom`. `d`, `a` and `r` are fixed per multimethod: static
typing is kept, and the dispatch on a *value* is what a protocol (dispatch on a type) is not. Tranche 5; needs L15.

**Concurrency.** `locking` is Clojure's `(locking x body ..)`: the body is not a closure and the value is the body's. No memory-safety failure forbids it:
a lock built from two `Atom`s (a ticket lock), and 300 `(locking lk (reset! n (+ @n 1)))` in each of two tasks, give 600 under both tools with a clean audit ([R] A12 p3),
and since no `Cell` can be shared by two tasks (§5 M1) there is no shared non-atomic state for the lock to guard that the checker lets through; its uses are effects
(`(locking *out* (println ..))`) and sequences of `Atom` updates. Clojure's monitor of an arbitrary object is a monitor table keyed by object identity, an entry made on
the first lock and dropped when the last holder leaves, reentrant for the holding task, which needs the runtime mutex of §7 C9 (**[sketch]**; `x` must be an object, a scalar
has no identity, types §1.1). `(Mutex a)`, a lock that owns its value, is the library's addition for Rust-style code. `add-watch`,
`remove-watch`, `set-validator!` and `get-validator` are a library: a `Ref` of an `Atom`, a watch list `(Atom (Vec (fn :send (a a)
unit)))` and a validator `(fn :send (a) bool)`, called after `swap!`, which type-checks and runs, 42 under both tools with a clean
audit (A11 e18; t42 gives 50): the first version's reason for omitting them, "callbacks cannot hold borrowed values
safely", is false because a `:send` closure holds owned values only. `future` is a macro over `spawn` and `@f` is `join`;
`future-cancel` is Clojure's: it sets the task's interrupt flag, which blocking calls (`Thread/sleep`, `deref` of a future or a promise) observe and answer by ending
the task with an `InterruptedException`, which here is a trap of that task and, once a task's trap is isolated (§2.10 step 2), a value of `join`; a computation that
polls uses `(cancelled?)`. No memory-safety failure forbids it (Java's interrupt is cooperative too, and the first rewrite's asynchronous kill was never
what it did, nor a program that was written); until step 2 lands a cancelled task that blocks keeps running (§5 S16); `pmap` is eager with one task per element today, chunked over
`ncpu + 2` tasks with the order kept in tranche 3 ([R] A11 e15), and lazy over an `LSeq` of futures in tranche 5.
`with-redefs` works on **any** function, as Clojure's does, and costs nothing for a function that no `with-redefs` names: the compiler sees the whole program, so a
function that some `(with-redefs [f g] ..)` mentions anywhere is called through a global `(Atom (fn :send ..))` slot and every other function stays a direct call
(the first rewrite's `:redefinable` opt-in was a cost reason, §10.4.8 F4). The slot is global and swapped for the dynamic extent of the body, racy across tasks exactly as
Clojure's is, and a global `Atom` is memory-safe; `g` must have `f`'s type, and for a generic `f` the slot is the instance at `g`'s types (no run-time types, §5 T3).
**[sketch, §7 L29]**: no program of the review exercises it. `with-open` is a macro over the scope-exit hook of §7 L12. `letfn` binds a group of mutually
recursive local functions: a macro over cells works and leaks a reference cycle per entry, `audit: clean=false
leak-cycles=4` (A11 t55, a leak and not a safety failure); the cure is one shared closure environment for the group (§7 C8), so
`letfn` ships with C8.

## 3. Argument order and naming

| # | Rule | Examples |
|---|---|---|
| N1 | A function over a **sequence** takes the sequence **last**; a function over a **collection value** takes it **first**. So `->>` threads sequences and `->` threads collections. | `(map f c)`, `(take n c)`, `(reduce f init c)`; `(assoc m k v)`, `(get m k)`, `(conj c x)`, `(update m k f)`, `(subvec v a b)` |
| N2 | The function argument of a sequence function comes first, so a literal `fn` reads before the data. | `(filter p c)`, `(group-by f c)`, `(sort-by key c)` |
| N3 | The exceptions Clojure has are kept and listed: `into` is collection-first for its target; `str/join` and `str/split` take the separator and the string in Clojure's two orders; `reduce-kv` (`f init m`) and `set/select` (`p s`) take the collection last, as Clojure's do; `str/includes? s sub` takes the string first. The library's own `includes?` (new) takes the element first (`(includes? x c)`) so that `->>` threads the source; Clojure has no such name, its idiom is `(some #{x} c)`. | `(into to c)`, `(str/join sep c)`, `(str/split s re)`, `(reduce-kv f init m)` |
| N4 | **Clojure's own suffixes, with their inconsistency kept**: `-by` takes a key function in `sort-by group-by partition-by` and a comparator in `sorted-map-by sorted-set-by`; `-key` is `max-key`/`min-key`; `-with` takes a combiner in `merge-with`. The first version renamed `sorted-map-by` to `sorted-map-with` "for one suffix, one meaning", which is not a reason (§1.1). The stand-ins for arities that need L1 (`get-or`, `nth-or`, `reduce1`, `range-by`, `subs-from`, `str/split-limit`, `str/index-of-from`, the `x<name>` transducers) are deleted when L1 lands (`sort-with`, `sort-by-with` and `zip-with` went with Y11b: `sort/2`, `sort-by/3`, `map/3`). | `(sort-by count c)`, `(sort-by count > c)`, `(sorted-map-by > 1 2)`, `(merge-with + a b)` |
| N5 | A `?` suffix is a `bool` result; a `!` suffix means the function writes through an `&` parameter, a cell or an `Atom` (`push! swap! reset! set!`), is a transient wrapper (`conj!`), or runs only for its effect on each element (`run!`, Clojure's). A persistent function never ends in `!`. | `empty?`, `contains?`, `push!` |
| N6 | `->` in a name is a conversion `from->to`, as the builtins have it (`char->i32`); the library's conversions that Clojure names `long`, `int`, `double` keep those names. The builtin casts that are one instruction name the target first (`(trunc i8 x)`, `(zext i64 b)`, `(fptosi i64 x)`): two conventions, because the first is a name and the second a type argument. | `(long x)`, `(char->str c)` |
| N7 | A sequence function is named for what it does and returns a lazy seq (§2.1 rule 2); an eager twin exists where Clojure has it (`mapv filterv`) and `doall` realises the seq. Materialisers are `vec set into zipmap sort reverse`. | `(vec (map f c))` |
| N8 | Types are CamelCase; an adaptor's struct is named for the adaptor in the past participle where its verb has one (`Mapped Filtered Taken Dropped Kept`) and by the noun otherwise (`Cat Mapcat Iterate Repeat Cycle Reductions Distinct ZipWith TreeSeq`). The recipe structs are internal to the fused path (§2.1 rule 2) and §4 names the seq type, `(LSeq e)`; §2.2 writes `(Mapped c f)`, the form after C1. Modules are `fib.x`, required with an alias (`str/`, `set/`, `math/`, `rng/`), and the Java class names `Math`, `Long`, `Integer`, `Double`, `Character`, `System`, `Thread` are implicit aliases of modules (§4.16). | |
| N9 | A name that the prelude's `join` (the task wait) or core forms use is never redefined unqualified; the library's `join` is always `str/join`, as Clojure's `clojure.string/join` is. | |
| N10 | In the signature column of §4: parameters in order, `->` result, ` \| ` introduces the protocol constraints (`Reducible c e`), lower-case letters are type variables, `;` separates the signatures of the arities of one name. `c` is a source, `s` a keyed collection, `e` an element. | |
| N11 | An `Option` function takes the function first and the `Option` last, as a sequence function does, so `->>` threads it: `(map-opt f o)`, `(and-then f o)`. They are the library's additions where Clojure has `nil` punning and `some->`. | `(->> o (map-opt inc) (and-then half))` |
| N12 | **Clojure's name is the name.** Where fibber already has a builtin under another spelling (`shl`, `sar`, `shr`, `popcount`, `!=`, `defun`, `read-file`, `write-file`, `spawn`), Clojure's spelling (`bit-shift-left`, `bit-shift-right`, `unsigned-bit-shift-right`, `Long/bitCount`, `not=`, `defn`, `slurp`, `spit`, `future`) is an alias of it, and the existing spelling stays for the cases and the compiler that use it. Where Clojure's name means a different thing (`aget`, `aset`, `alength`, `aclone` read and write the shared `MArray`, §2.11, §4.1) it is a function of its own and the builtin (`array-get`, `array-set!`, `array-len`, `array-copy`) stays the value form. A name the library adds because Clojure's text has no typed counterpart is marked `(new)` and never takes the place of a Clojure name. | `(aget a i)`, `(bit-shift-left x 3)`, `(not= a b)` |
| N13 | **A Clojure function that throws has a typed twin `try-<name>`** that returns a `(Result a str)` (or an `(Option a)` where the only failure is "absent"): `try-slurp`, `try-re-pattern`, `try-read-string`, `try-parse-int`. The function itself traps, as Clojure's throws (§1.2 P5); the twin is the library's addition and never takes the place of a Clojure name. The builtins that return `Option` today (`read-file`) keep their names. | `(try-slurp path)`, `(try-parse-int s)` |
| N14 | **No library function is named like a constructor or a core form** (`some`, `nil`, `Ok`, `Err`, `Pair`, `quote`, `list`): patterns and the expander read those names, so a library `defun some` makes every pattern `(some x)` `some is not a variant or struct` (tranche 2 plan C2-5). A package that adds a library name first checks it against `compiler/`: a `defun` of a local name wins ([R] A6), so a clash is silent and changes what the compiler's own code means. **Proposed** (plan PC2-14, §8.2.1) | `(some pred c)` is a Rust macro (§6.3), never a function |

## 4. The function table

Every name of the survey has one row: 673 names of `clojure.core` 1.12, `clojure.set`,
`clojure.string`, `clojure.walk`, `clojure.data` and `clojure.math`, grouped by the module that holds
it, plus 76 names that Clojure lacks and the library adds (marked `(new)`; 28 of them the stand-ins and gaps the third revision added, §10.4.9). `#"..."` shares the row of `#"regex"`. Columns:

* **Verdict**: `keep` (same name, same meaning), `adapt` (same name and text, a typed twist, stated), `alias` (Clojure's name
  for something fibber already has under another spelling or as a one-line function: the same meaning), `new`, and, in §4.17,
  `omit` (not offered: the JVM, no run-time type information, a result type that depends on a value, or a name that means something
  else here). The first version also had `rename` and `replace`; the rule removed them, because a name Clojure has is the name
  (§3 N12). Over the survey's names: keep 147, adapt 426, alias 54, omit 45 (the second revision: keep 145, adapt 427, alias 55, omit 45; the first rewrite: keep 144, adapt 367, alias 72, omit 89; the first version: keep 138, adapt 206, rename 47, replace 82,
  omit 200; §10.4.6 says what happened to each, §10.4.8 to the second revision's moves: 44 predicates from omit to adapt, 17 rows from alias to adapt (the arrays, `doall`, `array-map`, the `v` forms), 1 from adapt to keep, 30 tranche moves). The counts are those of a
  script that parses these tables (`tbl.py` of the third revision, 748 rows), which the executable `stdlib_table` of §8.1 replaces.
* **Fibber**: the spelling. `macro`, `core form`, `reader`, `pattern` say what kind of thing it is.
* **Signature**: in the notation of §3 N10; each arity of an overloaded name is one signature, separated
  by `;`. Where an arity needs L1 (§7), the stand-in the first tranches use is named in the note.
* **T**: the tranche of §8 that delivers the row **in the form its signature and note give on the compiler of that tranche** (names that exist today are `1`). A note that names a
  later §7 item (`until L1`, `after L22`, `L26 b`) describes the form the row takes then, and the row is complete without it; a Java-named row (`Long/MAX_VALUE`, `Math/abs`) is a module the
  program `:require`s from its tranche and Clojure's bare text with E15 (tranche 4). A script checks that no row names a §7 item that a later tranche's Needs column holds as a requirement of the row
  (§10.4.8 C1).
* **Note**: what differs from Clojure, and where it matters, the cost.

Offered over the survey's names by tranche: T1 153, T2 81, T3 199, T4 113, T5 81 (`seq`, `rest` and `next` moved from T2 to T1: the compiler's recursion idiom, `VSeq` and `sort` need them, tranche 1 plan PC-5).

### 4.1 Builtins (compiler primitives, syntax §4.3)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `aget` | adapt | `(aget a i)` | `(MArray a) i64 -> a` | 3 | reads the shared array (§2.11); traps out of range; the builtin `array-get` is the value form |
| `aset` | adapt | `(aset a i x)` | `(MArray a) i64 a -> unit` | 3 | writes the array in place and **every holder sees the write**, as Clojure's, inside one task ([R] A12 aset1, aset2); two tasks cannot hold one array, the cell-capture check rejects it (§5 M2); the value form is the builtin `array-set!` through `&` |
| `alength` | adapt | `(alength a)` | `(MArray a) -> i64` | 3 | the length of the shared array; the builtin `array-len` is the value form |
| `aclone` | adapt | `(aclone a)` | `(MArray a) -> (MArray a)` | 3 | a copy in a new cell: later writes to either are not seen by the other, as Clojure's |
| `make-array` | adapt | `(make-array T n)` | `i64 -> (MArray T) \| T scalar or (Option a)` | 3 | a macro on the type argument: zero for a scalar; for an object type `T` the array is an `(MArray (Option T))` filled with `nil`, as Clojure's `(make-array String 3)` is filled with `null` (§5 T1); no uninitialised object slot is ever visible (the first rewrite's reason M3 had no failing program, §10.4.8 R5); the builtin `(array n x)` is the value form |
| `+` | adapt | `(+ a b ..) (+)` | `t t -> t \| Num t; -> t \| Num t` | 1 | the builtin is binary on one type, overflow traps (Clojure's `+` throws on a `long` overflow too); a macro folds more arguments; `(+)` is `0`, an `i64` until L19 (tranche 2) lets the literal adopt its context's numeric type (**landed**, R6a: `(+ a b c)` is the left fold, so the first sum that overflows traps even if a later argument would cancel it); a variable of another numeric type promotes at the operator (§2.8, L26 b, tranche 4) |
| `-` | adapt | `(- a b ..) (- a)` | `t t -> t \| Num t; t -> t \| Num t` | 1 | binary builtin, `neg` for one argument; macro folds; `(-)` is an arity error, as Clojure's |
| `*` | adapt | `(* a b ..) (*)` | `t t -> t \| Num t; -> t \| Num t` | 1 | as `+`; `(*)` is `1`; `product` for a collection |
| `/` | adapt | `(/ a b)` | `t t -> r \| Div t r` | 1 | **exact on two integers: a `(Ratio t)`** (Q40, **Decided**, owner, 2026-10-01): `(/ 7 2)` is `7/2` and `(/ 6 3)` prints `2`; IEEE on floats; a ratio over ratios; the method of `Div`, whose instance fixes the result type, so generic code has one answer ([R] A13 div, §2.8); `quot` truncates; a literal operand adopts the other's type (L19); `Div` and `(Ratio t)` are the library's (`fib.core.num`, landed) and `/` is `Div`'s method in every module: the builtin `/` left `Num` with the flip (L30's last step, §7.5), so `(/ 7 2)` is the ratio `7/2` and integer division is `quot` |
| `quot` | adapt | `(quot a b)` | `t t -> t \| Num t` | 1 | the builtin truncating division (the integer `/` of `36ed472`, LLVM's `sdiv`): toward zero, traps on zero (`integer / by zero`, the text keeps naming `/`); **landed** (R3, commit `3d63d00`). On floats the quotient rounded to the width and then toward zero, libm's `trunc` of the `fdiv`, `(quot 7.5 2.0)` is `3.0` as Clojure's ([R] A13 div); a zero divisor gives an infinity or a NaN and does not trap (**Proposed**, §2.8, types §2.12; `cases/ownership` 206 to 209). Cases 102, 103 and 190 and the generator write `quot`; the compiler's one integer `/` (`compiler/syntax/number.fib`) is ported at the flip (§8.3) |
| `rem` | keep | `(rem a b)` | `t t -> t \| Num t` | 1 | builtin; the sign of the dividend; on floats `fmod` (types §2.12): `(rem 7.5 2.0)` is `1.5` ([R] A13 div) |
| `<` | adapt | `(< a b ..)` | `t t -> bool \| Ord t` | 1 | builtin `Ord` method on any ordered type; a macro chains more arguments ; mixed numeric operands: literals adopt (L19), variables promote at the operator (L26 b) |
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
| `Long/bitCount` | alias | `(Long/bitCount a)` | `i64 -> i64` | 1 | `popcount` through the module `Long` (`lib/Long.fib`, §4.16). **`i64 -> i64`, not generic**: Java's `Long.bitCount` widens an `int` to a `long` first, so a generic `t -> t` would answer 32 for `(Long/bitCount -1i32)` where Java answers 64; with the one `i64` instance a narrower integer is refused by the checker (nothing widens, types §1.1) instead of being counted at the wrong width |
| `=` | adapt | `(= a b ..)` | `t t -> bool \| Eq t` | 1 | structural on one type; within a family (sequential, map, set) across types by the checker (`seq=`, §2.7, L24); `(= 1 1.0)` is true for a literal by L19 (§5 C1); a macro chains more arguments |
| `==` | alias | `(== a b ..)` | `t t -> bool \| Eq t` | 1 | alias of `=` on numbers: across types for a literal through L19, for variables by promotion (L26 b) |
| `not=` | alias | `(not= a b ..)` | `t t -> bool \| Eq t` | 1 | alias of the builtin `!=` |
| `not` | keep | `(not x)` | `r -> bool \| Truthy r` | 1 | builtin on `bool`; a condition of type `(Option T)` is accepted by L20: `(not (get m k))` |
| `atom` | keep | `(atom v)` | `a -> (Atom a) \| Send a` | 1 | builtin; an atom of a type that is not `Send` is a compile error; a top-level `(def a (atom 0))` is allowed (§2.11) |
| `swap!` | adapt | `(swap! a f arg ..)` | `(Atom a) (fn (a ..) a) .. -> a` | 1 | a prelude macro over the builtin (**landed**, R6a): `(swap! a + 5)` is `(fib.prelude/swap! a (fn (v) (+ v 5)))` with `v` a gensym, the extra arguments evaluated inside the closure each time the builtin calls it, and the macro and the builtin coexist (the two-argument call is declined: [R] A10 swap, 30 under both tools; syntax §4.4); returns the new value; `f` may run more than once (it must be pure: on a scalar atom the loop is a `cmpxchg` loop and `f` is called again whenever another task changed the atom between the read and the swap; types §8.6) |
| `reset!` | keep | `(reset! a v)` | `(Atom a) a -> unit` | 1 | builtin; an atom of a scalar (`bool i8 i16 i32 i64 f32 f64 char`) is lock-free: one atomic store (types §8.6) |
| `compare-and-set!` | add | `(compare-and-set! a old new)` | `(Atom a) a a -> bool` | P-count-a | builtin: stores `new` and answers `true` when the atom holds `old` (a scalar's bits, an object's address, Clojure's `identical?`), else `false` and no change; one `cmpxchg` on a scalar atom, under the lock on an object atom. Not in the Rust seed: cases carry `;; stage: 2` |
| `adder`, `add!`, `inc!`, `sum` | add | `(adder)` `(add! a n)` `(inc! a)` `(sum a)` | `-> Adder`, `Adder i64 -> unit`, `Adder -> unit`, `Adder -> i64` | P-count-a | module `fib.adder` (`:use`d, not implicit): Java's `LongAdder`, a counter of 32 scalar-atom stripes chosen by the calling thread (`sys-thread-stripe`); `sum` adds the stripes and is exact once the adding tasks are joined (a `sum` while tasks add is a total of some of their adds). For hot counters; an atom stays the tool when decisions are taken on the value |
| `deref` | keep | `@x` | `(Atom a) -> a` | 1 | builtin; also a `Cell` (its value), a `Weak` (an `Option`) and, **Proposed** (owner's rule 2026-10-01), a `Task`, whose `@t` is `(join t)` (types §2.9; cases ownership/232 to 236) |
| `set!` | adapt | `(set! c v)` | `(Cell a) a -> unit` | 1 | builtin on a `Cell`; also `(set! (. obj field) v)`, and on a dynamic var inside `binding` (§2.11) |
| `volatile!` | alias | `(volatile! x)` | `a -> (Atom a) \| Send a` | 2 | an `Atom` (§2.11): visible across tasks and `vswap!` not atomic, as Clojure's volatile ([R] A12 p12); `Cell` is the thread-confined form |
| `vswap!` | adapt | `(vswap! v f arg ..)` | `(Atom a) (fn (a) a) .. -> a` | 2 | a macro for `(reset! v (f @v arg ..))`: not atomic, as Clojure's; returns the new value |
| `update!` | add | `(update! c f)` | `(Cell a) (fn :send (a) a) -> unit` | L6 | takes the value out of the cell, applies `f`, stores the result: a value that only the cell held reaches `f` with count 1 and `f`'s `conj`/`assoc` write in place (§2.5); a value another holder also has has count 2 and is copied, so no other holder changes. **Cells only**: an `Atom` is shared and never unique, `swap!` is its form and `(update! an-atom f)` is a type error; `f` is `:send`, so it cannot capture a cell and read the empty one; unit, not the new value (returning it would acquire it); `f` runs once; extra arguments are written in the closure. Library function over the builtin `cell-update!` |
| `vreset!` | alias | `(vreset! v x)` | `(Atom a) a -> a` | 2 | `reset!` that returns `x` |
| `long-array` | adapt | `(long-array n) (long-array c)` | `i64 -> (MArray i64); c -> (MArray i64) \| Reducible c i64` | 3 | a shared `MArray` of zeros, or a copy of the elements of a source (§2.11) |
| `double-array` | adapt | `(double-array n) (double-array c)` | `i64 -> (MArray f64); c -> (MArray f64) \| Reducible c f64` | 3 | a shared `MArray` of `0.0`, or a copy of the elements of a source |
| `float-array` | adapt | `(float-array n) (float-array c)` | `i64 -> (MArray f32); c -> (MArray f32) \| Reducible c f32` | 3 | a shared `MArray` of `0.0f32`, or a copy of the elements of a source |
| `short-array` | adapt | `(short-array n) (short-array c)` | `i64 -> (MArray i16); c -> (MArray i16) \| Reducible c i16` | 3 | a shared `MArray` of zeros, or a copy of the elements of a source |
| `int-array` | adapt | `(int-array n) (int-array c)` | `i64 -> (MArray i32); c -> (MArray i32) \| Reducible c i32` | 3 | a shared `MArray` of zeros, or a copy of the elements of a source |
| `byte-array` | adapt | `(byte-array n) (byte-array c)` | `i64 -> (MArray i8); c -> (MArray i8) \| Reducible c i8` | 3 | a shared `MArray` of zeros, or a copy of the elements of a source; `str-bytes` returns the value `(Array i8)`, and `(byte-array (str-bytes s))` copies it |
| `boolean-array` | adapt | `(boolean-array n) (boolean-array c)` | `i64 -> (MArray bool); c -> (MArray bool) \| Reducible c bool` | 3 | a shared `MArray` of `false`, or a copy of the elements of a source |
| `char-array` | adapt | `(char-array n) (char-array c)` | `i64 -> (MArray char); c -> (MArray char)` | 3 | an `(MArray char)` of Unicode scalars (§5 T10) |
| `object-array` | adapt | `(object-array n)` | `i64 -> (MArray (Option a))` | 3 | filled with `nil`, as Clojure's |
| `to-array` | adapt | `(to-array c)` | `c -> (MArray e) \| Reducible c e` | 3 | a typed `(MArray e)` from any source; `into-array` the same with a type argument (an empty source keeps its type); `to-array-2d` an `(MArray (MArray a))` |
| `into-array` | adapt | `(into-array c)` | `c -> (MArray e) \| Reducible c e` | 3 | as `to-array` |
| `to-array-2d` | adapt | `(to-array-2d c)` | `c -> (MArray (MArray e))` | 3 | a source of sources |
| `amap` | adapt | `(amap a i ret expr)` | macro | 3 | a loop with `aset` over an `aclone` of `a`, as Clojure's |
| `areduce` | adapt | `(areduce a i ret init expr)` | macro | 3 | a loop over `aget`; `(reduce f init a)` is the function form, an `(MArray a)` being `Reducible` |
| `MArray` (new) | new | `(MArray t)` | struct `(c: (Cell (Array t)))` | 3 | the shared mutable array of §2.11 that `long-array`, `make-array`, `to-array` and `aclone` return: `Reducible`, `Seqable`, `Cursable`, and `Lookup` and `Keyed` by index ([R] A12 aset2, A13 marray: `(get (int-array [1 2]) 0)` is `(some 1)`); it holds a `Cell`, so it cannot be a `def` (§5 M1) or cross a task (§5 M2) |

### 4.2 Core forms, macros and reader syntax

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `doseq` | keep | `(doseq [x xs :when p] body)` | macro | 2 | runs for effect, same modifiers |
| `if` | keep | `(if c a b) (if c a)` | core form | 1 | `c` is `bool` or `(Option T)` (§2.4, L20); the one-armed form is `unit` for a unit arm, else `(Option T)` (syntax §3.4) |
| `if-not` | adapt | `(if-not c a b)` | macro | 2 | `(if (not c) a b)`; one-armed as `if` |
| `if-let` | adapt | `(if-let [x e] a b)` | macro `(Option a)` | 1 | spelled `(if-let (x e) a b)` today, the bracket form needs §7 E3; over a `Truthy` (`bool`, `(Option a)`; `x` binds the payload) with any pattern, refutable ones included: the macro expands to `(match e ((some x) a) (_ b))` (§2.4; **landed**, E13, R7: `(if-let (p e) a)` has `()` as its else, `if-let` over a `Truthy` other than `Option` is L20, case 723 open) |
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
| `fn` | adapt | `(fn [x y] body)` | core form | 2 | bracket parameters are sugar; patterns allowed in parameters (§7 L7); a literal with several arities or a rest parameter takes the arity of its **expected type** at a call site where that type is known (L22: `(reduce (fn ([] 0) ([a] a) ([a b] (+ a b))) xs)` keeps the two-argument clause); a multi-arity function that is stored or returned has no single type (§5 T2) |
| `def` | adapt | `(def name: T v)` | core form | 1 | immortal; evaluates **any** expression once before `main`, in module order (§2.11, L15); the type may not contain a `Cell` or a `Weak` (§5 M1); `:dynamic` declares a dynamic var. Today the initialiser must be a constant: `(def ok: (Set i64) (set [1 3]))`, `(def v: i64 (f 2))` and `(def counter: (Atom i64) (atom 0))` are `def .. : initialiser is not a constant expression`, while `(def v: (Vec i64) [1 2 3])` and `{1 2}` run ([R] A10 def, A11 t56). An `MArray` holds a `Cell`, so a global mutable array is the same rejection; a global read-only table is a `Vec`. |
| `defmacro` | keep | `(defmacro name (params) body)` | core form | 1 | rest parameter `& xs`, as Clojure's (`...` is accepted until the macros of `lib/` and `compiler/` migrate, §7 L2) |
| `dotimes` | keep | `(dotimes [i n] body)` | macro | 1 | prelude macro, spelled `(dotimes (i n) ..)` today; the bracket form needs §7 E3 |
| `for` | adapt | `(for [x xs :when p :let [y e] y ys] body)` | macro | 2 | a lazy seq (an `LSeq`), fused when consumed in place (§2.1 rule 2), not a `Vec`; `:when`, `:while`, `:let` |
| `while` | keep | `(while c body ..)` | macro | 1 | `c` is a condition: `bool` or `(Option T)` (L20) |
| `doto` | keep | `(doto x (f a) g)` | macro | 1 | prelude macro |
| `with` (new) | new | `(with e (field v) ..)` | macro `s -> s` | 2 | a copy of the struct `e` with the named fields replaced, `e` untouched: Clojure's `assoc` on a record, and Rust's `S { f: v, ..e }`; it expands to a cell and `set-field!`, which runs today, in a module that does not itself define `concat` (B2): `(with db (users ..) (n 3))` gives 4 3 3 2 under both tools ([R] A10 with); `update-in` over records is `with` nested; a unique `e` can reuse its object (§2.5, C7) |
| `comment` | keep | `(comment ..)` | macro | 3 | expands to `()` |
| `time` | adapt | `(time e)` | macro | 4 | prints the elapsed time to stderr, returns `e`'s value; needs `now-ns` |
| `assert` | keep | `(assert c) (assert c msg)` | macro | 1 | prelude macro; always on, traps |
| `var` | keep | `(var name)` | core form | 1 | the definition a name resolves to, not a Var object |
| `quote` | keep | `'x` | core form | 1 | the value is a `Form` |
| `ns` | keep | `(ns a.b (:require [c.d :as d]) (:use e))` | core form | 1 | `:require` and `:use` today, and `:refer`, `:only`, `:exclude`, `:rename` with E15 (tranche 4); no `:import` |
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
| `` `x `` | adapt | `` `x `` | reader | 1 | quasiquote with the `x#` auto-gensym (E14) and, with E11, qualification of free symbols in the macro's defining module |
| `~x` | keep | `~x` | reader | 2 | Clojure's unquote; the comma becomes whitespace (E14: `[1,2]` is `[1 2]`); `,x` is the spelling of today and migrates with the macros of `lib/` and `compiler/` |
| `~@x` | keep | `~@x` | reader | 2 | as `~x`, splicing a `(Vec Form)` |
| `@x` | keep | `@x` | reader | 1 | cells, atoms, weak references and tasks |
| `#'x` | adapt | `#'x` | core form | 3 | reads as `(var x)` (E14) |
| `#_` | keep | `#_` | reader | 1 | syntax §1.1 |
| `#(...)` | adapt | `#(f % %2)` | reader | 2 | reads as `(fn (%1 %2) (f %1 %2))`; `%` is `%1`; `%&` binds the rest as a `Vec` where the expected function type fixes the arity (L22); nesting is an error (§7 E8) |
| `#{...}` | adapt | `#{a b}` | reader | 2 | reads as `(hash-set a b)` (§7 E8) |
| `:k` | adapt | `(:k x)` | checker | 2 | a keyword that unifies with `(fn (S) T)` elaborates to `(fn (x) (. x k))` when `S` is a struct with that field and to `get` when `S` is a `(Map keyword v)`; `(:k m d)` supplies a default; `(map :name ps)`, `(sort-by :age ps)`, `(group-by :dept ps)` are the commonest Clojure lines (§7 L14, L21). A `Map`, `Set` and `Vec` are callable the same way (§2.11); the row is named `:k` and not `(:k m)` because a name in a case's `covers:` line has no space |
| `^Type` | adapt | `^T x` | reader | 3 | reads as `x: T` (E14); `^:private` is the `:private` qualifier, `^:dynamic` the `:dynamic` qualifier |
| `^meta` | adapt | `^{:k v} x` | reader | 5 | attaches metadata (§2.11); `^:k` is `^{:k true}` |
| `::kw` | adapt | `::kw` | reader | 3 | reads as the flat keyword `:module/kw` of the current module (E14) |
| `1N 1M 1/2` | adapt | `1N 1M 1/2` | reader | 5 | `1N` and `1M` are literals of `BigInt` and `BigDecimal` (E14 c, tranche 5); `7/2` is a `(Ratio i64)` literal with E14 (a), tranche 2 (§2.8); width suffixes `1i32 2.5f32` stay |
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
| `with-redefs` | adapt | `(with-redefs [f g] body)` | macro | 5 | works on **any** function, as Clojure's: the compiler sees the whole program, so a function that some `with-redefs` names is called through a global `Atom` slot and every other function directly (§2.11, L29); `g` has `f`'s type |
| `locking` | keep | `(locking x body ..)` | macro `x body ... -> r` | 5 | Clojure's: the monitor of an object, a table keyed by identity (§2.11, C9); `x` must be an object; a lock from two `Atom`s runs today ([R] A12 p3: 600); `(Mutex a)` (new) is the owning form |
| `with-open` | adapt | `(with-open [r e] body)` | macro | 5 | a macro over the scope-exit hook (§7 L12) |
| `with-out-str` | adapt | `(with-out-str body ..)` | macro `-> str` | 5 | binds `*out*` to a string-building writer (§2.11) |
| `with-in-str` | adapt | `(with-in-str s body ..)` | macro | 5 | binds `*in*` (§2.11) |
| `with-local-vars` | adapt | `(with-local-vars [x 1] ..)` | macro | 3 | a `cell` per name; `var-get` is `@x`, `var-set` is `set!` |
| `defmulti` | adapt | `(defmulti name dispatch-fn)` | macro | 5 | a `(Multi d a r)`: a global `Atom` of methods and the dispatch function; `d`, `a`, `r` fixed per multimethod (§2.11) |
| `defmethod` | adapt | `(defmethod name dv [x] ..)` | macro | 5 | registers a method in the module init |
| `extend-type` | adapt | `(extend-type T P (m [this] ..))` | macro | 3 | expands to `impl` (a global fact, syntax §3.10) |
| `extend-protocol` | adapt | `(extend-protocol P T1 (m ..) T2 (m ..))` | macro | 3 | several `impl`s |
| `reify` | adapt | `(reify P (m [this] ..))` | macro | 3 | an anonymous struct, an `impl` and a `(dyn P)` value |
| `defrecord` | adapt | `(defrecord Name [f: T ..])` | macro | 2 | `defstruct` that derives `Eq Ord Hash Show Debug ToStr` (§7 L17; recommended, §9.2 Q22; the macro writes the derives, the default derive of `defstruct` is not in tranche 2, §8.2.1 item 5); a struct is not a map: no extra keys, no `dissoc` of a field (§5 T4); `(assoc r :k v)` and `(:k r)` by L21 |
| `deftype` | adapt | `(deftype Name [f: T ..])` | macro | 3 | `defstruct` with `(Cell T)` fields for mutable state (syntax §3.7) |
| `lazy-cat` | adapt | `(lazy-cat c ..)` | macro `-> (LSeq e)` | 2 | each collection expression delayed: a macro over `lazy-seq` and `concat` |
| `require` | adapt | `(require '[x :as y])` | top-level form | 4 | hoisted into the module's `ns` clause at expansion time; `ns` accepts `:refer` and `:only` |
| `use` | adapt | `(use ..)` | top-level form | 4 | hoisted into the `:use` clause |
| `refer` | adapt | `(refer ..)` | top-level form | 4 | hoisted into `:refer` |
| `refer-clojure` | adapt | `(refer-clojure ..)` | top-level form | 4 | `(:refer-clojure :exclude [map])`: the prelude is always used, a local definition shadows a `:use`d name, so the clause only silences the shadowing |
| `alias` | adapt | `(alias ..)` | top-level form | 4 | hoisted into `:as` |
| `macroexpand` | adapt | `(macroexpand form)` | `Form -> Form` | 5 | a function at macro time, a library over `Form` once the expander is a library (the compiler has `fibc expand`) |
| `macroexpand-1` | adapt | `(macroexpand-1 form)` | `Form -> Form` | 5 | as `macroexpand` |
| `apply` | adapt | `(apply f xs)` | arity-reading form `(fn (A..) R) c -> R` | 3 | with `f: (fn (A B) R)` it is `(f (nth v 0) (nth v 1))` after a check that the count is 2 (a trap where Clojure throws `ArityException`); the variadic folds `(apply + xs)`, `(apply max xs)`, `(apply str xs)`, `(apply concat xss)`, `(apply merge ms)`, `(apply max-key k xs)`, `(apply min-key k xs)`, `(apply distinct? xs)` are a table of named folds (the collection forms of `max-key`, `min-key` and `distinct?` live here, so their one-argument calls keep Clojure's meaning, §4.3, §4.4); a literal vector spreads ([R] A11 e12: 6, 3, 6, `312`); the first version omitted `apply` for "dynamic arity", but the arity is in the function's type (L22) |
| `derive` | adapt | `(derive child parent)` | macro | 5 | over keywords it builds a hierarchy held in an `Atom` for `defmulti`; `(derive Eq P)` over a protocol and a type is the existing type form, told apart by the first argument's kind; `isa?` and `parents` read the hierarchy |
| `isa?` | adapt | `(isa? child parent)` | `k k -> bool` | 5 | over the hierarchy of `derive`; not a run-time type test (§5 T3) |
| `->Name` | adapt | `(->Point x y)` | function | 2 | the positional constructor `(Point x y)` as a function value, so `(map ->Point xs ys)` works; `fibc` says `unsupported: a constructor as a value` while `fibref` returns 2 ([R] A11 t86), a stage-1 divergence (§7 B5) |
| `map->Name` | adapt | `(map->Point m)` | `(Map keyword v) -> Name` | 3 | from a `(Map keyword v)` when every field has type `v`; a missing key traps; heterogeneous fields go through `Val` (§5 T4) |
| `pcalls` | adapt | `(pcalls f g ..)` | macro `(fn () r) .. -> (Vec r)` | 3 | thunks of one result type, run as tasks (`plet`, syntax §3.12) |
| `pvalues` | adapt | `(pvalues e1 e2 ..)` | macro `r ... -> (Vec r)` | 3 | as `pcalls` over expressions |

### 4.3 `fib.core` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `str` | adapt | `(str a ..)` | macro `a ... -> str \| ToStr a` | 1 | concatenates the `to-str` of the arguments: a string as itself, a collection as its `pr` text, `nil` as the empty text (§2.7); `(str)` is `""`; a Rust prelude macro (§6.3) with a function twin (**landed**, R5, `605a26e`): a right fold of `fib.prelude/str-concat` over the pieces, a string literal as it is, a literal `nil` as `""`, any other piece `(fib.core/to-str x)`, so it needs `fib.core` in scope (syntax §4.4) |
| `subs` | adapt | `(subs s a b) (subs s a)` | `str i64 i64 -> str; str i64 -> str` | 1 | **CHARACTER offsets** (Unicode scalars, §5 T10), pairing with `count` and `str/index-of` as Clojure's do; never splits a character; O(n) until the ASCII flag (C10); the byte layer is `str-slice` (§2.9); the 2-arity is `subs-from` until L1 |
| `name` | adapt | `(name k)` | `keyword -> str` | 4 | keywords are flat; `(name :a/b)` is `b` |
| `keyword` | adapt | `(keyword s) (keyword ns s)` | `str -> keyword; str str -> keyword` | 4 | interns at run time |
| `gensym` | keep | `(gensym) (gensym prefix)` | `-> Form; str -> Form` | 1 | builtin whose result is a `Sym` form, `(fn (str) Form)` in the checker (case 713); macro time only (syntax §3.16); the zero-argument form is not there today: `gensym takes 1 argument(s), got 0` ([R] A11 gensym), so it is a clause by L1 (tranche 2; `(gensym "G__")` until then) |
| `char` | adapt | `(char n)` | `t -> char \| ToChar t` | 2 | checked: traps on a non-scalar; `i32` and `i64` |
| `parse-long` | adapt | `(parse-long s)` | `str -> (Option i64)` | 1 | `nil` on any malformed input, never a trap |
| `parse-double` | adapt | `(parse-double s)` | `str -> (Option f64)` | 3 | correctly rounded through `strtod`, after the grammar of §2.9 is checked in fibber: hex, a leading space, `inf` and trailing junk give `nil` under both tools ([R] A10 pd) |
| `parse-boolean` | adapt | `(parse-boolean s)` | `str -> (Option bool)` | 3 |  |
| `Integer/parseInt` | adapt | `(Integer/parseInt s) (Integer/parseInt s radix)` | `str -> i32; str i64 -> i32` | 3 | module `Integer`; traps `For input string: ".."` on a malformed string, as Java throws `NumberFormatException` (§1.2 P5); `try-parse-int` (new) is the `Option` twin |
| `try-parse-int` (new) | new | `(try-parse-int s) (try-parse-int s radix)` | `str -> (Option i32); str i64 -> (Option i32)` | 3 | the typed twin of `Integer/parseInt` (§3 N13); `parse-i32` is the unqualified alias |
| `Long/parseLong` | adapt | `(Long/parseLong s) (Long/parseLong s radix)` | `str -> i64; str i64 -> i64` | 3 | module `Long`; traps on a malformed string, as Java throws; `parse-long` is Clojure's own nil-returning parser, an `(Option i64)` |
| `Double/parseDouble` | adapt | `(Double/parseDouble s)` | `str -> f64` | 3 | module `Double`; traps on a malformed string, as Java throws; `parse-double` is the `(Option f64)` parser |
| `inc` | keep | `(inc x)` | `t -> t \| Num t, Unit t` | 1 | any numeric type; traps on overflow |
| `dec` | keep | `(dec x)` | `t -> t \| Num t, Unit t` | 1 |  |
| `mod` | keep | `(mod a b)` | `t t -> t \| Num t, Ord t, Unit t` | 1 | the sign of the divisor, built on `rem`; an integer zero divisor traps, a float one never does: it gives `NaN` where Clojure throws, as `rem` does (types §4, Decided 2026-09-28: float arithmetic never traps) (A13 div) |
| `max` | adapt | `(max a b ..)` | `t t -> t \| Ord t` | 1 | any ordered type; a macro folds more; a NaN argument gives NaN, as Clojure's ([R] A10 nan) |
| `min` | adapt | `(min a b ..)` | `t t -> t \| Ord t` | 1 | as `max` |
| `abs` | adapt | `(abs x)` | `t -> t \| Num t, Ord t, Unit t` | 1 | traps on the minimum of an integer type |
| `unchecked-add` | adapt | `(unchecked-add a b)` | `t t -> t \| Bits t` | 3 | wrapping, at the operand's width (§7 L10) |
| `unchecked-subtract` | adapt | `(unchecked-subtract a b)` | `t t -> t \| Bits t` | 3 | wrapping |
| `unchecked-multiply` | adapt | `(unchecked-multiply a b)` | `t t -> t \| Bits t` | 3 | wrapping; hash mixers and generators need it |
| `unchecked-negate` | adapt | `(unchecked-negate a)` | `t -> t \| Bits t` | 3 | wrapping |
| `unchecked-inc` | adapt | `(unchecked-inc a)` | `t -> t \| Bits t` | 3 | wrapping |
| `unchecked-dec` | adapt | `(unchecked-dec a)` | `t -> t \| Bits t` | 3 | wrapping |
| `byte` | adapt | `(byte x)` | `t -> i8 \| ToByte t` | 2 | checked: traps when the value does not fit, and for a float that is out of range before it is cut (`127.9`) or a `NaN`; `(trunc i8 x)` wraps |
| `short` | adapt | `(short x)` | `t -> i16 \| ToShort t` | 2 | checked; a float is range-checked before it is cut, a `NaN` traps (Clojure's `RT.shortCast`) |
| `int` | adapt | `(int x)` | `t -> i32 \| ToInt t` | 2 | checked; a float is range-checked before it is cut toward zero (`2147483647.9` traps), a `NaN` is `0`, an infinity traps, as Clojure's `RT.intCast`; a `char` gives its code point ([R] A12 int2). |
| `long` | adapt | `(long x)` | `t -> i64 \| ToLong t` | 2 | checked; a `NaN` is `0` and an infinity traps, as Clojure's `RT.longCast`; `(sext i64 x)` widens without a check; a `char` gives its code point. |
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
| `Long/MAX_VALUE` | alias | `Long/MAX_VALUE` | `i64` | 1 | a `def` of the module `Long` (`lib/Long.fib`, [R] A11 e19). **There is no `i64-max`, `i64-min`, `i32-max` or `i32-min` in the library or the prelude**: the only `i64-max` is a private `def` of `compiler/expand/args.fib`; the family is not provided until a package needs it |
| `Long/MIN_VALUE` | alias | `Long/MIN_VALUE` | `i64` | 1 | a `def` of the module `Long` |
| `Double/MAX_VALUE` | alias | `Double/MAX_VALUE` | `f64` | 3 | module `Double`; `f64-max`; also `f64-epsilon`, `f64-min-positive`, `f64-inf`, `f64-nan` (none of them exists yet) |
| `identical?` | adapt | `(identical? a b)` | `a a -> bool` | 3 | pointer equality on objects, value equality on scalars (scalars have no identity); observes sharing, never equality; `same?` is not offered |
| `compare` | adapt | `(compare a b)` | `t t -> i64 \| Ord t` | 1 | -1, 0 or 1, **built on `<`** as Clojure's: a NaN compares equal to everything; vectors compare by length first and strings by code point (§2.7, §5 D5); one static type |
| `hash` | keep | `(hash x)` | `t -> i64 \| Hash t` | 1 | builtin method; instances for collections are library; `f64` hashes `-0.0` as `0.0` (§2.7) |
| `hash-combine` | adapt | `(hash-combine h x)` | `i64 i64 -> i64` | 1 | rotate and xor from `shl shr bit-or bit-xor`: never traps, exists today ([R] A10 hash); a multiplicative mixer replaces it with L10 (T3); the working tree's prelude has its own xorshift mixer, which the library adopts (§2.7) |
| `hash-ordered-coll` | keep | `(hash-ordered-coll c)` | `c -> i64 \| Reducible c e, Hash e` | 1 | for `Vec` and `List`, over `hash-combine` |
| `hash-unordered-coll` | keep | `(hash-unordered-coll c)` | `c -> i64 \| Reducible c e, Hash e` | 1 | commutative (an xor of the elements' hashes); for `Map` and `Set` |
| `mix-collection-hash` | keep | `(mix-collection-hash h n)` | `i64 i64 -> i64` | 3 | the finaliser |
| `nil?` | keep | `(nil? o)` | `(Option a) -> bool` | 1 |  |
| `some?` | keep | `(some? o)` | `(Option a) -> bool` | 1 |  |
| `identity` | keep | `(identity x)` | `a -> a` | 1 |  |
| `distinct?` | adapt | `(distinct? x y ..)` | macro `a a .. -> bool \| Hash a, Eq a` | 3 | Clojure's varargs: a macro nests, and `(distinct? x)` is `true` as Clojure's; the collection form is `(apply distinct? c)` (§4.2 `apply`), so a one-argument call keeps Clojure's meaning |
| `defn` | adapt | `(defn name doc? [x: T ..] -> R body)` | macro | 1 | a Rust macro over `defun` (§6.3; **landed** for one clause, R6b, `605a26e`: bracket parameters, a docstring that is dropped, `defn-` is `:private`; a `&` in the vector is the error `rest parameters need L2` and a clause list `several arities need L1`, syntax §4.4, cases 780 and 781); multi-arity in Clojure's shape `(defn name ([x: T] -> R body) ([x: T y: T] -> R body))`, each clause with its own result type, by §7 L1 (the parameter vector tells a clause from a single-arity body, so no `:arity` marker is needed, [R] A11 e10: `defn`, `let*`, `loop*` with `[a 1 b 2]` as macros over `defun`/`let`/`loop` run, 13 under both tools); `defun` stays |
| `36rZZ 2r1010` | keep | `2r1010 36rZZ` | reader | 3 | radix literals (E14); `0x1F` and `0b1010` stay |
| `Result` (new) | new | `(defenum (Result a e) (Ok v: a) (Err e: e))` | type | 1 | the prelude has the `Result` of `compiler/util/result.fib`, which M2 deleted (two types of one name cannot exchange values, §6.2); `(try-let ..)` threads `Err` |
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
| `Ratio` (new) | new | `(Ratio t)` | struct `(n: t d: t)` | 1 | the exact quotient of two integers of width `t`, normalised: `Num Div Eq Ord Hash Show`, `(/ 7 2)` is one (§2.8, Q40 **Decided**; [R] A13 div, A12 ratio); traps on overflow at its width; `(Ratio BigInt)` is tranche 5 |
| `namespace` | adapt | `(namespace k)` | `keyword -> (Option str)` | 4 | the part before the `/` of a flat keyword, `nil` when there is none |
| `symbol` | adapt | `(symbol s) (symbol ns s)` | `str -> Form` | 4 | a `Sym` of `fib.syntax`: symbols exist as `Form`s, not as a run-time type of their own |
| `find-keyword` | adapt | `(find-keyword s)` | `str -> (Option keyword)` | 4 | a lookup in the intern table; `keyword` interns |
| `true?` | adapt | `(true? x)` | `bool -> bool; (Option bool) -> bool` | 3 | `(= x true)` / `(= o (some true))` |
| `false?` | adapt | `(false? x)` | `bool -> bool` | 3 | `(not x)`; `nil` is not `false`, as Clojure's |
| `boolean` | adapt | `(boolean x)` | `r -> bool \| Truthy r` | 3 | the truthiness of a `bool` or `(Option T)` (§2.4) |
| `pos-int?` | adapt | `(pos-int? n)` | `t -> bool \| Bits t, Ord t, Unit t` | 3 | `(pos? n)` on an integer type; `neg-int?` and `nat-int?` likewise; a float is a compile error, as every type predicate |
| `neg-int?` | adapt | `(neg-int? n)` | `t -> bool \| Bits t, Ord t, Unit t` | 3 | `(neg? n)` on an integer type |
| `nat-int?` | adapt | `(nat-int? n)` | `t -> bool \| Bits t, Ord t, Unit t` | 3 | `(not (neg? n))` on an integer type |
| `unchecked-add-int` | alias | `(unchecked-add-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-subtract-int` | alias | `(unchecked-subtract-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-multiply-int` | alias | `(unchecked-multiply-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-negate-int` | alias | `(unchecked-negate-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-inc-int` | alias | `(unchecked-inc-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-dec-int` | alias | `(unchecked-dec-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-divide-int` | alias | `(unchecked-divide-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-remainder-int` | alias | `(unchecked-remainder-int a ..)` | `i32 .. -> i32` | 3 | the wrapping family at `i32` (§7 L10) |
| `unchecked-byte` | alias | `(unchecked-byte x)` | `t -> u` | 3 | `(trunc i8 x)`: of an integer keeps the low bits and never traps (of a float, `fptosi` first, as Java's cast) |
| `unchecked-short` | alias | `(unchecked-short x)` | `t -> u` | 3 | `(trunc i16 x)`: of an integer keeps the low bits and never traps (of a float, `fptosi` first, as Java's cast) |
| `unchecked-int` | alias | `(unchecked-int x)` | `t -> u` | 3 | `(trunc i32 x)`: of an integer keeps the low bits and never traps (of a float, `fptosi` first, as Java's cast) |
| `unchecked-long` | alias | `(unchecked-long x)` | `t -> u` | 3 | for a float `(fptosi i64 x)`, which saturates (NaN is 0), as Java's `(long) d`; for an integer `(sext i64 x)`; never traps |
| `unchecked-float` | alias | `(unchecked-float x)` | `t -> u` | 3 | `(fptrunc f32 x)`: rounds to nearest, never traps |
| `unchecked-double` | alias | `(unchecked-double x)` | `t -> u` | 3 | `(fpext f64 x)`: exact, never traps |
| `unchecked-char` | alias | `(unchecked-char x)` | `t -> u` | 3 | `(i32->char n)`: traps on a non-scalar (a `char` is a scalar, not 16 bits, §5 T10) |
| `+'` | adapt | `(+' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `-'` | adapt | `(-' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `*'` | adapt | `(*' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `inc'` | adapt | `(inc' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `dec'` | adapt | `(dec' a ..)` | `t .. -> BigInt \| Num t` | 5 | returns a `BigInt` always: Clojure's auto-promotion gives a `Long` when it fits, a result type that depends on the values (§5 T5) |
| `bigint` | adapt | `(bigint ..)` | `t -> BigInt` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `bigdec` | adapt | `(bigdec ..)` | `t -> BigDecimal` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `biginteger` | adapt | `(biginteger ..)` | `t -> BigInt` | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `rationalize` | adapt | `(rationalize x)` | `f64 -> (Ratio i64)` | 3 | the exact decimal expansion of the float as a ratio, `(rationalize 0.1)` is `1/10` (§2.8); traps when it does not fit 64 bits; over `BigInt` in tranche 5 |
| `numerator` | keep | `(numerator r)` | `(Ratio t) -> t` | 1 | the normalised numerator ([R] A13 div: `[3 2]` for `(/ 6 4)`) |
| `denominator` | keep | `(denominator r)` | `(Ratio t) -> t` | 1 | the normalised denominator, always positive |
| `with-precision` | adapt | `(with-precision ..)` | macro | 5 | a library type with `Num Eq Ord Hash Show` (§2.8, [R] A11 t50) |
| `parse-uuid` | adapt | `(parse-uuid s)` | `str -> (Option Uuid)` | 5 | a `Uuid` struct; `random-uuid` takes the global generator |
| `random-uuid` | adapt | `(random-uuid)` | `-> Uuid` | 5 | from the global generator |
| `char-escape-string` | adapt | `char-escape-string` | `(Map char str)` | 5 | a constant `def` |
| `char-name-string` | adapt | `char-name-string` | `(Map char str)` | 5 | a constant `def` |
| `ex-info` | adapt | `(ex-info msg data) (ex-info msg data cause)` | `str d -> (ExInfo d)`, three arguments as `ex-info-cause` | 3 | plain data, usable as the error of a `Result` (§2.10). **Delivered** in `fib.ex` (cases 924, 926): `ExInfo` is generic in its data `d` because the page's `Val` is fib.data's and does not exist yet; the three-argument arity is the function `ex-info-cause` until L1. **Not delivered:** `Val` as the data type, the arity |
| `ex-data` | adapt | `(ex-data e)` | `(ExInfo d) -> d` | 3 | the fields of the error. Delivered in `fib.ex` (case 924), over the generic `d` as above |
| `ex-message` | adapt | `(ex-message e)` | `(ExInfo d) -> str` | 3 | the message. Delivered in `fib.ex` (case 924) |
| `ex-cause` | adapt | `(ex-cause e)` | `(ExInfo d) -> (Option (ExInfo d))` | 3 | the cause. Delivered in `fib.ex` (case 924) |
| `Throwable->map` | adapt | `(Throwable->map e)` | `ExInfo -> (Map keyword Val)` | 5 | no stack trace (§2.10) |
| `throw` | adapt | `(throw e)` | core form | 5 | `trap` of `(ex-message e)` until unwinding lands; `try`, `catch`, `finally` need §7 L28 (§9.1 Q35). **Not delivered:** `throw` is not a form yet (write `(trap (ex-message e))`); stage 1 of L28 (a task's trap isolated, `try-join`, below) is delivered and is not `try` |
| `catch` | adapt | `(catch T e ..)` | clause of `try` | 5 | see `try` |
| `finally` | adapt | `(finally ..)` | clause of `try` | 5 | memory is released by the scope exits the ownership checker already computes; non-memory cleanup is the scope-exit hook (§7 L12) |
| `try` | adapt | `(try body (catch T e ..) (finally ..))` | core form | 5 | needs unwinding (§2.10, L28, §9.1 Q35); a `Result` and `try-let` are the forms that run today |
| `print-method` | adapt | `(impl Debug T ..)` | `impl Show`, `impl Debug` | 2 | an `impl Show` and `impl Debug` for the type is the static form of the extension point: `(impl Debug T (debug (self) ..))` is what `pr` and `prn` print, `(impl Show T (show (self) ..))` what `print` and `println` print; the spelling is the protocol, `Debug`, which a case that defines one contains (`stdlib_table` spells an `impl` form by its protocol, §8.1 item 1) |
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
| `seq` | adapt | `(seq c)` | `c -> (Option s) \| Seqable c s` | 1 | `nil` when empty, else the closed seq type `s` of the collection (a `VSeq` for a `Vec`, §2.1 rule 8): `(if (seq xs) ..)` works because an `Option` is a condition (L20), and `(if-let (s (seq xs)) ..)` runs today; the page's first version said `(if (seq xs) ..)` is a type error. On an `LSeq` it is the seq itself, or `nil` when empty ([R] A13 lz1). |
| `vec` | keep | `(vec c)` | `c -> (Vec e) \| Reducible c e` | 1 | the `to-vec` method: a `Vec` argument is returned as is (no allocation, [R] A10 nth), any other source builds into one buffer; defined in `fib.coll` beside the trie (§2.5, §6.2) |
| `vector` | adapt | `(vector a b ..)` | macro `a ... -> (Vec a)` | 2 | the literal `[a b ..]`; as a function value `(map vector xs ys)` is `[x y]`, a `Pair` when the types differ (§7 L25), a `Vec` when they unify. The heterogeneous function value waits for L25 (tranche 3). |
| `vector-of` | adapt | `(vector-of :i32 1 2)` | macro | 3 | a `(Vec i32)` with the suffixed literals; `(Vec i64)` is already unboxed, so the keyword only selects the width |
| `list` | keep | `(list a ..)` | macro `a ... -> (List a)` | 1 | prelude macro, kept; expands to `Cons`/`Empty` |
| `list*` | adapt | `(list* a .. l)` | macro `a ... (List a) -> (List a)` | 3 | nested `cons`; the last argument is a `List` |
| `cons` | adapt | `(cons x c)` | `a c -> (LSeq a) \| Seqable c s` | 2 | any seqable: a lazy cons onto `(seq c)`, as Clojure's `cons` returns a seq ([R] A13 conjseq: `lcons`) ([R] A11 t80: `(cons 0 [1 2 3])` has the elements 0 1 2 3); `(list ..)` and the `Cons` variant of `List` keep their names (§5 row 22 of the first version: `Empty`, `Cons`) |
| `hash-map` | adapt | `(hash-map k v ..)` | macro `k v ... -> (Map k v) \| Hash k, Eq k` | 2 | builds the HAMT shape, in hash order from the start; the literal `{k v ..}` is the array shape, in insertion order up to 8 entries (§2.7); an odd count is a compile error; duplicate literal keys are an error (§7 E5, tranche 3) |
| `hash-set` | adapt | `#{a ..} or (hash-set a ..)` | macro `a ... -> (Set a) \| Hash a, Eq a` | 2 | the reader reads `#{..}` as `(hash-set ..)` (§7 E8) |
| `range` | adapt | `(range n) (range a b) (range a b s)` | `t -> (Range t); t t -> (Range t); t t t -> (Range t) \| Num t, Ord t` | 1 | overloaded by arity (§7 L1); integers exact; **floats accumulate as Clojure's do**: `(range 0.0 1.0 0.1)` adds the step to the previous element, 11 elements, the last `0.9999999999999999` ([R] A11 range; `frange` is the `a + i*s` form, 10 elements); a step of 0 repeats the start forever (an infinite recipe); `(range)` is `(iterate inc 0)`; until L1 Clojure's three-argument call is the library's `range-by`: the `range` macro declines a call of three arguments and the checker's arity error says `use range-by` (D1, landed, case 808) |
| `repeat` | adapt | `(repeat x) (repeat n x)` | `a -> (Repeat a); i64 a -> (LSeq a)` | 2 | overloaded by arity |
| `repeatedly` | adapt | `(repeatedly f) (repeatedly n f)` | `(fn () a) -> (LSeq a); i64 (fn () a) -> (LSeq a)` | 2 | memoised: `f` runs once per element across traversals, as Clojure's (§2.1 rule 2) |
| `iterate` | keep | `(iterate f x)` | `(fn (a) a) a -> (Iterate a)` | 2 | infinite source; a consumer that stops ends it |
| `cycle` | keep | `(cycle c)` | `c -> (Cycle c e) \| Reducible c e` | 2 | an empty source ends at once |
| `lazy-seq` | adapt | `(lazy-seq body)` | macro `body -> (LSeq a)` | 1 | delays the body, a seq expression, into an `LSeq` node realised once (§2.1 rule 2; [R] A13 lz1, A11 t21); the type of every sequence function's result, so it is tranche 1 |
| `concat` | adapt | `(concat a b ..)` | `c1 c2 -> (LSeq e) \| Reducible c1 e, Reducible c2 e` | 1 | the two-argument function; a macro nests for more, `(concat a)` is `(seq a)` and `(concat)` the empty recipe of its context's element type, as Clojure's `()` |
| `interleave` | adapt | `(interleave a b ..)` | `c1 c2 -> (LSeq e) \| Reducible c1 e, Reducible c2 e` | 2 | truncates to the shorter; lazy over the seqs of both; in the fused path every operand is walked by a cursor (recipes by buffering, §2.1 rule 5, in any order); `zip-strict` traps on a mismatch |
| `interpose` | keep | `(interpose sep c)` | `e c -> (LSeq e) \| Reducible c e` | 2 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `tree-seq` | adapt | `(tree-seq branch? children root)` | `(fn (n) bool) (fn (n) d) n -> (LSeq n) \| Reducible d n` | 3 | typed over one node type; depth first, no explicit stack in the caller |
| `re-seq` | adapt | `(re-seq re s)` | `Regex str -> (Vec Match)` | 4 | eager; `fib.regex` (§4.9) |
| `line-seq` | adapt | `(line-seq r)` | `Reader -> (LSeq str)` | 5 | memoised `LSeq`; needs a scope-exit hook to close the handle (§7 L12); `fib.io`; `lines-of` is not offered |
| `iteration` | adapt | `(iteration step init)` | `(fn (k) (Option (Pair v k))) k -> (LSeq v)` | 5 | memoised `LSeq`; `fib.seq` |
| `first` | adapt | `(first c)` | `c -> (Option e) \| Reducible c e` | 1 | `nil` on empty; use `(unwrap (first c))` or `(nth c 0)` for the sure case. On an `LSeq` it realises the first node only ([R] A13 lz1). |
| `ffirst` | adapt | `(ffirst c)` | `c -> (Option e) \| Reducible c d, Reducible d e` | 2 | `(and-then first (first c))`; the first version said it "needs nested sequential element types": two `Reducible` constraints do it ([R] A11 t60) |
| `nfirst` | adapt | `(nfirst c)` | `c -> (Option s) \| Reducible c d, Seqable d s` | 3 | `(next (first c))` |
| `second` | adapt | `(second c)` | `c -> (Option e) \| Reducible c e` | 2 |  |
| `fnext` | alias | `(fnext c)` | `c -> (Option e)` | 2 | alias of `second` |
| `last` | adapt | `(last c)` | `c -> (Option e) \| Reducible c e` | 1 | the `last` method: a walk by default, O(1) on `Vec` ([R] A10 nth) and, by the same override in the instance, on `Array Range SubVec` |
| `butlast` | adapt | `(butlast c)` | `c -> (LSeq e) \| Reducible c e` | 3 | empty when short, where Clojure has `nil` (§5 T1); `(seq (butlast c))` is the `Option` |
| `rest` | adapt | `(rest c)` | `c -> s \| Seqable c s` | 1 | `Seqable.rest`: the closed seq type, so recursion on `rest` is not polymorphic recursion (§2.1 rule 8); `(drop 1 c)` is the recipe form, and recursion on it over a generic `c` is rejected by both tools (§7 B4) |
| `next` | adapt | `(next c)` | `c -> (Option s) \| Seqable c s, Seqable s s` | 1 | `(seq (rest c))` ([R] A11 seq1: `(len [5 6 7 8])` is 4 with it) |
| `nnext` | adapt | `(nnext c)` | `c -> (Option s)` | 3 | `(next (next c))` |
| `nthnext` | adapt | `(nthnext n c)` | `i64 c -> (Option s)` | 3 | `(next ..)` n times; `nil` when exhausted |
| `nthrest` | alias | `(nthrest c n)` | `c i64 -> (LSeq e)` | 3 | alias of `drop` with Clojure's argument order |
| `nth` | adapt | `(nth c i) (nth c i d)` | `c i64 -> e \| Reducible c e; c i64 e -> e` | 1 | the `nth` method: O(1) on `Vec` ([R] A10 nth) and, by the same override, on `Array Range SubVec`, a walk on any other source, so `(nth (filter p c) 3)` works (A10 nth); traps out of range as Clojure throws; the 3-arity is `nth-or` until L1 |
| `take` | keep | `(take n c)` | `i64 c -> (LSeq e) \| Reducible c e` | 1 | stops the source at n. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `take-while` | keep | `(take-while p c)` | `(fn (e) r) c -> (LSeq e) \| Reducible c e, Truthy r` | 1 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `take-nth` | adapt | `(take-nth n c)` | `i64 c -> (LSeq e) \| Reducible c e` | 3 | `(take-nth 0 c)` repeats the first element forever, as Clojure's (replicated, §9.1 Q33). The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `take-last` | adapt | `(take-last n c)` | `i64 c -> (VSeq e) \| Reducible c e` | 3 | materialises into a `Vec` and returns the seq over it, as Clojure's returns a seq |
| `drop` | keep | `(drop n c)` | `i64 c -> (LSeq e) \| Reducible c e` | 1 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `drop-while` | keep | `(drop-while p c)` | `(fn (e) r) c -> (LSeq e) \| Reducible c e, Truthy r` | 2 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `drop-last` | adapt | `(drop-last c) (drop-last n c)` | `c -> (LSeq e); i64 c -> (LSeq e) \| Reducible c e` | 3 | overloaded by arity; holds n elements back |
| `split-at` | adapt | `(split-at n c)` | `i64 c -> (Pair (VSeq e) (VSeq e)) \| Reducible c e` | 3 | one pass that fills two `Vec`s, the first n and the rest, each returned as its `VSeq` (a `VSeq` has no upper bound, so one `Vec` cannot carry both views): Clojure's `[(take n c) (drop n c)]`, a vector of two seqs, printing `[(1 2) (3 4)]` |
| `splitv-at` | adapt | `(splitv-at n c)` | `i64 c -> (Pair (Vec e) (Vec e)) \| Reducible c e` | 3 | Clojure's vector form of `split-at`: both halves are `Vec`s |
| `split-with` | adapt | `(split-with p c)` | `(fn (e) r) c -> (Pair (VSeq e) (VSeq e)) \| Reducible c e, Truthy r` | 3 | one pass into two `Vec`s, as `split-at`; `p` runs once per element up to and including the first failure |
| `subvec` | adapt | `(subvec v a b) (subvec v a)` | `(Vec e) i64 i64 -> (SubVec e)` | 3 | **a vector**: an O(1) view holding a count on `v`, with `Reducible`, `Lookup`, `Assoc`, `Collection` (`conj` appends), `Stack` and `Keyed`, printing in brackets, as Clojure's `SubVector`: `(conj (subvec [1 2 3] 1 3) 0)` is `[2 3 0]`, where `rest`'s seq conses at the front (§2.1 rule 8, [R] A12 vseq). When `Vec` is the struct with an offset field (§9 Q18) `subvec` returns a `Vec` itself; `(vec (subvec ..))` copies; indexes checked at the call |
| `peek` | adapt | `(peek s)` | `s -> (Option e) \| Stack s e` | 2 | the end `conj` adds at: a `Vec`'s last, a `List`'s first |
| `pop` | adapt | `(pop s)` | `s -> s \| Stack s e` | 2 | traps when empty; in place when unique |
| `rseq` | adapt | `(rseq c)` | `c -> (Reversed c e) \| Reversible c e` | 3 | `Vec`, `Range`, sorted collections |
| `count` | adapt | `(count c)` | `c -> i64 \| Reducible c e` | 1 | `size`: O(1) for `Vec Map Set Array Range Option SubVec` (and `str` with the ASCII flag, C10), a walk for every adaptor, so **`(count (map f c))` calls `f`** as Clojure's does ([R] A11 count); characters on a `str` (§2.9) |
| `bounded-count` | keep | `(bounded-count n c)` | `i64 c -> i64 \| Reducible c e` | 3 | stops after n elements |
| `empty?` | adapt | `(empty? c)` | `c -> bool \| Reducible c e` | 1 | stops at the first element the source produces (§2.1 rule 4); on an `LSeq` it realises one node |
| `not-empty` | adapt | `(not-empty c)` | `c -> (Option c) \| Reducible c e` | 2 | the collection itself or `nil` (a condition accepts an `Option`), where `seq` returns the seq type `s` |
| `map` | adapt | `(map f c) (map f c1 c2) (map f c1 c2 c3)` | `(fn (e) b) c -> (LSeq b) \| Reducible c e; (fn (a b) r) c1 c2 -> (LSeq r) \| Reducible c1 a, Reducible c2 b; (fn (a b d) r) c1 c2 c3 -> (LSeq r) \| Reducible c1 a, Reducible c2 b, Reducible c3 d` | 1 | overloaded by arity (`map$2`, `map$3`; Y11b landed the two-collection clause, `zip-with` is gone; the three-collection `(map f c1 c2 c3)` is not offered, case 809: `map takes 2 or 3 argument(s), got 4`); returns a lazy memoised seq, fused when consumed in place (§2.1 rule 2), not a `Vec`; in the fused path the second and third operands are walked by a cursor (any source; recipes by buffering, §2.1 rule 5); `(map f)` is the transducer (`xmap` until L1, §2.1 rule 7) |
| `mapv` | adapt | `(mapv f c)` | `(fn (e) b) c -> (Vec b) \| Reducible c e` | 2 | alias of `(vec (map f c))` |
| `mapcat` | adapt | `(mapcat f c)` | `(fn (e) d) c -> (LSeq b) \| Reducible c e, Reducible d b` | 1 | flattening is free in the push model. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `map-indexed` | adapt | `(map-indexed f c)` | `(fn (i64 e) b) c -> (LSeq b) \| Reducible c e` | 2 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `keep` | adapt | `(keep f c)` | `(fn (e) (Option b)) c -> (LSeq b) \| Reducible c e` | 2 | `f` returns an `Option`; `nil`s are dropped, `some`s unwrapped. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `keep-indexed` | adapt | `(keep-indexed f c)` | `(fn (i64 e) (Option b)) c -> (LSeq b) \| Reducible c e` | 3 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `reverse` | adapt | `(reverse c)` | `c -> (List e) \| Reducible c e` | 1 | eager: a `List`, as Clojure's returns one (`(conj (reverse [1 2 3]) 0)` is `(0 3 2 1)`); `rseq` is the O(1) view |
| `sort` | adapt | `(sort c) (sort cmp c)` | `c -> (VSeq e) \| Reducible c e, Ord e; (fn (e e) r) c -> (VSeq e) \| Reducible c e, Cmp r` | 1 | stable merge sort; **returns a `VSeq`, a seq, as Clojure's `sort` does**: `(conj (sort [3 1 2]) 0)` is `(0 1 2 3)` and it prints in parentheses ([R] A12 vseq), and `(vec (sort xs))` is the sorted `Vec` itself; **the comparator is any function whose result is a `Cmp`**: an `i64` or Clojure's predicate form, so `(sort < xs)` and `(sort > xs)` work ([R] A11 t17, e4); `(sort cmp c)` is the clause `sort$2` (Y11b, case 806); sorting a `Vec` copies it once (the `vec` inside is the identity), 2096 objects for 1000 elements against 4190 before ([R] A10 sort) |
| `sort-by` | adapt | `(sort-by key c) (sort-by key cmp c)` | `(fn (e) k) c -> (VSeq e) \| Reducible c e, Ord k; (fn (e) k) (fn (k k) r) c -> (VSeq e) \| Reducible c e, Cmp r` | 1 | **the key function runs twice per comparison, as Clojure's does** (`(sort (fn [x y] (compare (key x) (key y))) c)`: 18 calls for 5 elements, [R] A12 sortby); `sort-by-cached` (new) decorates, sorts and undecorates, one call per element (Rust's `sort_by_cached_key`); `(sort-by val > m)` is Clojure's text; `(sort-by key cmp c)` is the clause `sort-by$3` (Y11b, case 322) |
| `sort-by-cached` (new) | new | `(sort-by-cached key c) (sort-by-cached key cmp c)` | `(fn (e) k) c -> (VSeq e) \| Reducible c e, Ord k` | 3 | the decorate-sort-undecorate form: the key is computed once per element ([R] A12 sortby: 5 calls against `sort-by`'s 18); an expensive key function is the reason to use it |
| `shuffle` | adapt | `(shuffle c)` | `c -> (Vec e) \| Reducible c e` | 4 | the global generator (§2.8); `(rng/shuffle r c)` takes an `Rng` |
| `distinct` | keep | `(distinct c)` | `c -> (LSeq e) \| Reducible c e, Hash e, Eq e` | 2 | first occurrences, in order. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `dedupe` | keep | `(dedupe c)` | `c -> (LSeq e) \| Reducible c e, Eq e` | 3 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `replace` | adapt | `(replace smap c)` | `s c -> (LSeq e) \| Lookup s e e, Reducible c e` | 3 | any `Lookup` as the substitution map, a `(Vec e)` included (index to value), as Clojure's. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `partition` | adapt | `(partition n c) (partition n step c) (partition n step pad c)` | `i64 c -> (LSeq (VSeq e)); i64 i64 c -> (LSeq (VSeq e)); i64 i64 d c -> (LSeq (VSeq e)) \| Reducible c e, Reducible d e` | 2 | yields `(VSeq e)`, each group a seq that prints in parentheses, as Clojure's; drops an incomplete last group, as Clojure's does (`partition-all` keeps it). The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `partitionv` | adapt | `(partitionv n c)` | `i64 c -> (LSeq (Vec e))` | 3 | Clojure's vector form of `partition`: yields `(Vec e)`; `partitionv-all` and the 3- and 4-argument arities likewise |
| `partition-all` | adapt | `(partition-all n c) (partition-all n step c)` | `i64 c -> (LSeq (VSeq e)); i64 i64 c -> (LSeq (VSeq e)) \| Reducible c e` | 2 | yields `(VSeq e)`. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `partitionv-all` | adapt | `(partitionv-all n c)` | `i64 c -> (LSeq (Vec e)) \| Reducible c e` | 3 | the vector form of `partition-all`: yields `(Vec e)` |
| `partition-by` | keep | `(partition-by f c)` | `(fn (e) k) c -> (LSeq (VSeq e)) \| Reducible c e, Eq k` | 2 | yields `(VSeq e)`. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `group-by` | adapt | `(group-by f c)` | `(fn (e) k) c -> (Map k (Vec e)) \| Reducible c e, Hash k, Eq k` | 1 | one pass; each group in encounter order |
| `frequencies` | keep | `(frequencies c)` | `c -> (Map e i64) \| Reducible c e, Hash e, Eq e` | 1 |  |
| `zipmap` | keep | `(zipmap ks vs)` | `c1 c2 -> (Map k v) \| Reducible c1 k, Cursable c2 k2, Cursor k2 v, Hash k, Eq k` | 2 | truncates to the shorter |
| `reduce` | adapt | `(reduce f init c) (reduce f c)` | `(fn (a e) a) a c -> a \| Reducible c e; (fn (e e) e) c -> e \| Reducible c e` | 1 | the 2-arity returns `e` and traps `reduce: empty collection` on an empty `c`, except for the literal heads `+ * str conj merge concat` whose identity a macro table knows (Clojure calls `(f)`); `reduce1` is the `Option` form; `(reduced x)` inside a literal `fn` stops the reduction (§2.1 rule 3). **Landed** as the 33rd Rust macro (R6b, PC-6; syntax §4.4): the two-argument call is `reduce-nonempty` (no row of this table; it is in the fusion set T, §2.1), and of the six literal heads `+ * conj merge` start from their identity, **`str` and `concat` work as Clojure's do (R14, the owner's rule of 2026-10-01)**: a literal `str` is folded with `(fn (a x) (fib.prelude/str a x))` from `""` (the value `str` is the one-argument library function), `(reduce str init c)` the same over `init`, and a literal `concat` starts from the empty lazy seq `(fib.seq/lazy-node (fn () fib.seq/LNil))` (cases 794, 795); a head that the module defines itself is that function and has no identity (case 796); `(reduce + [1.5 2.5])` is `cannot unify f64 with i64` until L19 (cases 786, 790) |
| `reduce-kv` | keep | `(reduce-kv f init m)` | `(fn (a k v) a) a m -> a \| KeyReducible m k v` | 2 | `Map` (entries) and `Vec` (index, element); collection last (§3 N3); the walk passes `k` and `v` as two arguments, so it builds no `Pair`; `reduced` inside `f` is not read (`reduce-while` is the early exit) |
| `reductions` | adapt | `(reductions f init c) (reductions f c)` | `(fn (a e) a) a c -> (LSeq a) \| Reducible c e` | 2 | emits `init` first; the 2-arity seeds with the first element |
| `transduce` | adapt | `(transduce xf f init c)` | `Xf (fn (a b) a) a c -> a` | 3 | transducers are values of `(Xf a b)`, a factory of steppers with a flush (§2.1); `comp` composes them after C1, `xf` until then |
| `xf` (new) | new | `(xf t1 t2 ..)` | macro `(Xf a b) (Xf b c) -> (Xf a c)` | 3 | composes transducers left to right, `(xf-comp x y)` nested; `comp` composes them after C1 (§5 T9) |
| `xmap` (new) | new | `(xmap f)` | `(fn (a) b) -> (Xf a b)` | 3 | the transducer arity of `map` until L1 makes `(map ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A8 |
| `xfilter` (new) | new | `(xfilter p)` | `(fn (a) r) -> (Xf a a) \| Truthy r` | 3 | the transducer arity of `filter` until L1 makes `(filter ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A8 |
| `xremove` (new) | new | `(xremove p)` | `(fn (a) r) -> (Xf a a) \| Truthy r` | 3 | the transducer arity of `remove` until L1 makes `(remove ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xtake` (new) | new | `(xtake n)` | `i64 -> (Xf a a)` | 3 | the transducer arity of `take` until L1 makes `(take ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A8 |
| `xtake-while` (new) | new | `(xtake-while p)` | `(fn (a) r) -> (Xf a a) \| Truthy r` | 3 | the transducer arity of `take-while` until L1 makes `(take-while ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xdrop` (new) | new | `(xdrop n)` | `i64 -> (Xf a a)` | 3 | the transducer arity of `drop` until L1 makes `(drop ..)` without a collection the transducer (§2.1 rule 7); deleted then; the same shape, not written |
| `xdrop-while` (new) | new | `(xdrop-while p)` | `(fn (a) r) -> (Xf a a) \| Truthy r` | 3 | the transducer arity of `drop-while` until L1 makes `(drop-while ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xtake-nth` (new) | new | `(xtake-nth n)` | `i64 -> (Xf a a)` | 3 | the transducer arity of `take-nth` until L1 makes `(take-nth ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xkeep` (new) | new | `(xkeep f)` | `(fn (a) (Option b)) -> (Xf a b)` | 3 | the transducer arity of `keep` until L1 makes `(keep ..)` without a collection the transducer (§2.1 rule 7); deleted then; the same shape, not written |
| `xkeep-indexed` (new) | new | `(xkeep-indexed f)` | `(fn (i64 a) (Option b)) -> (Xf a b)` | 3 | the transducer arity of `keep-indexed` until L1 makes `(keep-indexed ..)` without a collection the transducer (§2.1 rule 7); deleted then; the same shape, not written |
| `xmap-indexed` (new) | new | `(xmap-indexed f)` | `(fn (i64 a) b) -> (Xf a b)` | 3 | the transducer arity of `map-indexed` until L1 makes `(map-indexed ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xmapcat` (new) | new | `(xmapcat f)` | `(fn (a) c) -> (Xf a b) \| Reducible c b` | 3 | the transducer arity of `mapcat` until L1 makes `(mapcat ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A8 |
| `xpartition-all` (new) | new | `(xpartition-all n)` | `i64 -> (Xf a (Vec a))` | 3 | the transducer arity of `partition-all` until L1 makes `(partition-all ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A8, A10 xf (the flush) |
| `xpartition-by` (new) | new | `(xpartition-by f)` | `(fn (a) k) -> (Xf a (Vec a)) \| Eq k` | 3 | the transducer arity of `partition-by` until L1 makes `(partition-by ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xdedupe` (new) | new | `(xdedupe)` | `-> (Xf a a) \| Eq a` | 3 | the transducer arity of `dedupe` until L1 makes `(dedupe ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xdistinct` (new) | new | `(xdistinct)` | `-> (Xf a a) \| Hash a, Eq a` | 3 | the transducer arity of `distinct` until L1 makes `(distinct ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xinterpose` (new) | new | `(xinterpose sep)` | `a -> (Xf a a)` | 3 | the transducer arity of `interpose` until L1 makes `(interpose ..)` without a collection the transducer (§2.1 rule 7); deleted then; ran in A12 xf2 |
| `xreplace` (new) | new | `(xreplace smap)` | `s -> (Xf a a) \| Lookup s a a` | 3 | the transducer arity of `replace` until L1 makes `(replace ..)` without a collection the transducer (§2.1 rule 7); deleted then; the same shape, not written |
| `xrandom-sample` (new) | new | `(xrandom-sample p)` | `f64 -> (Xf a a)` | 3 | the transducer arity of `random-sample` until L1 makes `(random-sample ..)` without a collection the transducer (§2.1 rule 7); deleted then; the same shape, not written |
| `run!` | keep | `(run! f c)` | `(fn (e) r) c -> unit \| Reducible c e` | 1 | replaces the prelude's `for-each`; `f` may return any `r`, which is dropped (monomorphised, no cost): `(run! (fn (x) (swap! a + x)) xs)` is 6 under both tools ([R] A10 run); over a literal `(range a b)` and a literal one-parameter `fn` it expands to the counting loop that the prelude's `for-each` macro makes today (§6.3) |
| `dorun` | alias | `(dorun c)` | `c -> unit \| Reducible c e` | 2 | walks for effect, realising a seq: `run!` with no function |
| `doall` | adapt | `(doall c)` | `(LSeq e) -> (LSeq e)` | 2 | realises every node of the seq and returns it, as Clojure's `(doall s)` does (§2.1 rule 2); over a collection it is the identity |
| `some` | adapt | `(some pred c)` | `(fn (e) r) c -> (Option x) \| Reducible c e, Truthy r, Payload r x` | 2 | the first truthy value of `(pred x)` (§2.4, [R] A11 `some`: `(some true)`, `(some 20)`, `nil`); `some` is also `Option`'s constructor, so it is the Rust macro of E1 that picks by argument count: `(some pred c)` expands to `(fib.seq/some-p pred c)` and `(some x)` declines and stays the constructor (plan PC2-3, §8.2.1 item 3; a library `defun some` is impossible, N14, and L1 has no constructor clause); `find-map` and `find-first` are the library's extras, which stage 2 uses in tranche 1 |
| `every?` | keep | `(every? p c)` | `(fn (e) r) c -> bool \| Reducible c e, Truthy r` | 1 | stops at the first failure |
| `not-any?` | keep | `(not-any? p c)` | `(fn (e) r) c -> bool \| Reducible c e, Truthy r` | 1 | `none?` is not offered |
| `not-every?` | keep | `(not-every? p c)` | `(fn (e) r) c -> bool \| Reducible c e, Truthy r` | 2 |  |
| `filter` | adapt | `(filter p c)` | `(fn (e) r) c -> (LSeq e) \| Reducible c e, Truthy r` | 1 | `p` returns a `Truthy` (`bool` or `Option`): `(filter :parent nodes)`, `(remove nil? xs)` ([R] A11 t05). The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `filterv` | adapt | `(filterv p c)` | `(fn (e) r) c -> (Vec e) \| Reducible c e, Truthy r` | 2 | alias of `(vec (filter p c))` |
| `remove` | keep | `(remove p c)` | `(fn (e) r) c -> (LSeq e) \| Reducible c e, Truthy r` | 1 | The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `max-key` | adapt | `(max-key k x y ..)` | macro `(fn (e) k) e e .. -> e \| Ord k` | 2 | Clojure's text: a macro folds the arguments, and `(max-key k x)` is `x` as Clojure's; ties: the last wins, as Clojure's; the collection form is `(apply max-key k c)` (§4.2 `apply`), which traps `max-key: empty` on an empty source as `(apply max-key k [])` throws (§1.2 P5) |
| `min-key` | adapt | `(min-key k x y ..)` | macro `(fn (e) k) e e .. -> e \| Ord k` | 2 | as `max-key`: `(min-key k x)` is `x`, ties go to the last, `(apply min-key k c)` is the collection form and traps `min-key: empty` on an empty source |
| `rand-nth` | adapt | `(rand-nth c)` | `c -> e \| Reducible c e` | 4 | the global generator, traps on an empty source as Clojure throws; `(rng/rand-nth r c)` |
| `random-sample` | adapt | `(random-sample p c)` | `f64 c -> (LSeq e) \| Reducible c e` | 4 | the global generator; `(rng/random-sample r p c)`. The transducer arity is `x<name>` until L1 (§2.1 rule 7). |
| `sequence` | adapt | `(sequence xf c)` | `(Xf a b) c -> (LSeq b) \| Reducible c a` | 5 | a lazy seq of the transduced elements |
| `eduction` | adapt | `(eduction xf c)` | `(Xf a b) c -> (LSeq b) \| Reducible c a` | 5 | as `sequence`; Clojure's eduction re-runs per traversal, which a memoised seq does not: a typed twist that only an effectful `xf` shows |
| `any?` | keep | `(any? x)` | `a -> bool` | 1 | Clojure's `any?`: a one-argument predicate that is always true (the default spec predicate); the quantifier is `some` (the first version made `(any? p c)` the quantifier, which silently changes a ported program) |
| `reduced` | adapt | `(reduced x)` | `a -> (Step a)` | 1 | `(Done x)`; `reduce-while` reads it, and so does plain `reduce` over a literal `fn` (§2.1 rule 3) |
| `find-first` (new) | new | `(find-first p c)` | `(fn (e) bool) c -> (Option e) \| Reducible c e` | 1 | the value form of `(first (filter p c))`; not a Clojure name; stage 2 uses it before `some` exists |
| `find-map` (new) | new | `(find-map f c)` | `(fn (e) (Option b)) c -> (Option b) \| Reducible c e` | 1 | the first `some` that `f` returns, the value form of `(some f c)` for an `Option`-returning `f`; not a Clojure name; stage 2 uses it before `some` exists ([R] A11 some, A10 t1r) |
| `zip` (new) | new | `(zip a b)` | `c1 c2 -> (LSeq (Pair x y)) \| Reducible c1 x, Reducible c2 y` | 2 | yields `(Pair x y)` (one object per element until C5); truncates to the shorter; in the fused path the second operand is walked by a cursor (any source; recipes by buffering, §2.1 rule 5) |
| `seq-of` (new) | new | `(seq-of c)` | `c -> (dyn (Reducible e)) \| Reducible c e` | 2 | the type-erased source for a join and for a field of unknown source type: `(if flag (seq-of (filter p v)) (seq-of v))` runs ([R] A10 join); L24 inserts it at a join automatically; one heap object and an indirect call per visit |
| `seq=` (new) | new | `(seq= a b)` | `c1 c2 -> bool \| Reducible c1 e, Reducible c2 e, Eq e` | 2 | equality across `Reducible`s: `(seq= (map inc [1 2]) [2 3])` is true ([R] A10 showseq); the checker elaborates `=` to it when the operand types are of one family (§2.7, L24) |
| `zip-strict` (new) | new | `(zip-strict a b)` | `c1 c2 -> (LSeq (Pair x y)) \| Reducible c1 x, Reducible c2 y` | 3 | traps on a length mismatch |
| `sum` (new) | new | `(sum c)` | `c -> e \| Reducible c e, Num e, Unit e` | 1 | `(reduce + 0 c)`; needs a static `zero` for an empty generic `e` (§7 L4); `i64` until then |
| `product` (new) | new | `(product c)` | `c -> e \| Reducible c e, Num e, Unit e` | 2 |  |
| `reduce-while` (new) | new | `(reduce-while f init c)` | `(fn (a e) (Step a)) a c -> a \| Reducible c e` | 1 | Clojure's `reduced`; boxed `Step` allocates per step until §7 C5 |
| `frange` (new) | new | `(frange a b s)` | `f64 f64 f64 -> (FRange)` | 3 | the non-accumulating `a + i*s` for `i < ceil((b-a)/s)`; an extra: `range` over floats is Clojure's accumulating form; step 0 traps |
| `reduce1` (new) | new | `(reduce1 f c)` | `(fn (e e) e) c -> (Option e) \| Reducible c e` | 1 | the 2-arity of `reduce` until L1, then deleted; `(unwrap (reduce1 max xs))` is Clojure's `(apply max xs)` |
| `range-by` (new) | new | `(range-by a b s)` | `i64 i64 i64 -> Range` | 1 | the 3-arity of `range` until L1, then deleted; `(range a b)` is today's macro |
| `flatten` | adapt | `(flatten c)` | `c -> (LSeq e) \| Reducible c d, Reducible d e` | 3 | one level of nesting for a source of sources; arbitrary depth over a recursive enum (`Val`, a `Tree`) by a `Flatten` protocol; Clojure's heterogeneous nesting is `[1 [2 [3]]]`, which is `cannot unify i64 with (Vec ..)` ([R] A11 t91 for the analogous heterogeneous vector, §5 T4) |
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
| `replicate` | alias | `(replicate n x)` | `i64 a -> (LSeq a)` | 3 | alias of `(repeat n x)`; deprecated in Clojure and still there |
| `file-seq` | adapt | `(file-seq path)` | `str -> (LSeq str)` | 5 | a memoised recipe of paths; needs a directory-listing primitive (`fib.io`) |

### 4.5 `fib.coll` (implicit)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `find` | adapt | `(find m k)` | `s k -> (Option (Pair k v)) \| Lookup s k v` | 2 | returns the entry, a `Pair` |
| `select-keys` | keep | `(select-keys m ks)` | `s c -> (Map k v) \| Lookup s k v, Hash k, Eq k, Reducible c k` | 1 | absent keys are skipped; the result is always a `Map`, whatever `s` is, as Clojure's (it builds from `{}`; found by differential testing, 2026-10-02) |
| `conj` | adapt | `(conj c x ..)` | `s e -> r \| Collection s r e` | 1 | one rule per type: `List` front, `Vec` end, `Set` anywhere, `Map` takes a `Pair` (a two-element `Vec` after L23); a literal `nil` first argument is `(list)` (§2.4); macro for more than one `x` (**landed**, R6a: `(conj c)` is `c`; the literal-`nil` first argument rule is not implemented); `VSeq` front, `SubVec` end; a seq (`LSeq`, `Range`, `Iterate`, `Repeat`, `Cycle`) conses at the front and the result is an `LSeq`, Clojure's `(conj (range 3) 9)` being `(9 0 1 2)`: `Collection s r e` has the result type determined by the instance (§2.3; [R] A13 conjseq) |
| `assoc` | adapt | `(assoc m k v ..)` | `s k v -> s \| Assoc s k v` | 1 | `Map` and `Vec` (index at most the count: `i = count` appends, as Clojure's, and beyond it traps, [R] A11 e6); a struct by a literal keyword is `with` (L21); a literal `nil` is `{}` (§2.4; not implemented); macro for more pairs (**landed**, R6a; a key without a value is `malformed assoc: a key without a value`) |
| `dissoc` | keep | `(dissoc m k ..)` | `s k -> s \| Dissoc s k` | 1 | `Map` and `Set`; macro for more keys |
| `get` | adapt | `(get c k) (get c k d)` | `s k -> (Option v) \| Lookup s k v; s k v -> v \| Lookup s k v` | 1 | absent and `nil`-valued differ by construction; overloaded by arity |
| `get-in` | adapt | `(get-in m [k1 k2 ..]) (get-in m [k1 ..] d)` | macro `m k1 .. -> (Option v); m k1 .. v -> v` | 3 | the path is a literal vector, one `get` per level, so each level has its own key type; a path computed at run time has no static type over nested maps of different value types (§5 T4) and is a function over `Val` ([R] A11 e17: `(get-in-v v [:inner :x])` is `(some 1)`, `[:inner :nope]` `nil`) |
| `assoc-in` | adapt | `(assoc-in m [k1 k2 ..] v)` | macro `m k1 .. v -> m \| Lookup, Assoc, Emptyable per level` | 3 | a missing level is `(empty ..)` of the level's type; needs static `empty` (§7 L4) |
| `update` | adapt | `(update m k f)` | `s k (fn (v) v) -> s \| Lookup s k v, Assoc s k v` | 1 | a macro: `f` sees the value, a missing key traps `update: no key` (Clojure's `(update m :n inc)` throws an NPE); `(update m k f x ..)` passes extra arguments as Clojure's does; a literal `(fnil g d)` in the `f` position is routed to `update-or` ([R] A11 t24: 6, 107, 15); an `f` that is not a literal uses `update-or` or `update-opt`; **landed** (R6b, syntax §4.4) |
| `update-or` (new) | new | `(update-or m k f d)` | `s k (fn (v) v) v -> s \| Lookup s k v, Assoc s k v` | 1 | `f` sees `d` when the key is missing: Clojure's `(update m w (fnil inc 0))` for an `f` that is not a literal `fnil`; stays when L1 lands (the 4-arity of `update` is Clojure's extra-argument form) |
| `update-opt` (new) | new | `(update-opt m k f)` | `s k (fn ((Option v)) v) -> s \| Lookup s k v, Assoc s k v` | 1 | `f` sees `(Option v)` as Clojure's sees `nil`: `(update-opt m w (fnil inc 0))` is the literal port |
| `update-in` | adapt | `(update-in m [k1 ..] f)` | macro `m k1 .. (fn (v) v) -> m` | 3 | typed per level like `get-in`; `update` at each level, so a missing key traps; `assoc-in` creates levels; the empty path is a compile error |
| `update-keys` | adapt | `(update-keys m f)` | `(Map k v) (fn (k) k2) -> (Map k2 v) \| Hash k2, Eq k2` | 3 | two keys mapping to one: **the last wins**, as Clojure's (replicated, §9.1 Q33) |
| `update-vals` | keep | `(update-vals m f)` | `(Map k v) (fn (v) w) -> (Map k w) \| Hash k, Eq k` | 3 | the value type may change |
| `contains?` | keep | `(contains? c k)` | `s k -> bool \| Keyed s k` | 1 | `Map` and `Set` (key, member) and `Vec`, `SubVec`, `Array`, `str` (the **index**, as Clojure's, [R] A11 e6, A12 keyed); `includes?` is the element test |
| `array-map` | adapt | `(array-map k v ..)` | `k v ... -> (Map k v)` | 2 | the array shape: insertion order, and it stays in it until an `assoc` takes it over 8 entries (§2.7); `hash-map` builds the HAMT shape |
| `into` | adapt | `(into to c) (into to xf c)` | `t c -> t \| Collection t t e, Reducible c e; t (Xf e b) c -> t \| Collection t t b, Reducible c e` | 1 | the 3-arity takes a transducer (tranche 3); a bulk path builds in one buffer; `(into {} [[1 2]])` needs L23 (a two-element `Vec` entry, §5.5 S4); `(into {} (zip ks vs))` works |
| `empty` | adapt | `(empty c)` | `s -> s \| Emptyable s` | 1 | `Vec`, `Map`, `Set`, `List`; takes a value, so no static method is needed |
| `merge` | adapt | `(merge m1 m2 ..)` | `(Map k v) (Map k v) -> (Map k v) \| Hash k, Eq k` | 1 | a literal `nil` operand is skipped (§2.4); an `Option`-typed operand after L16; `(merge)` has no type and is the macro's error `malformed merge: needs at least one argument` (**landed**, R6a); macro nests for more |
| `merge-with` | adapt | `(merge-with f m1 m2)` | `(fn (v v) v) (Map k v) (Map k v) -> (Map k v) \| Hash k, Eq k` | 2 | `f` combines a conflict (old, new); two maps: more are a `reduce` (case 2580), a third map is `merge-with takes 3 argument(s), got 4` (case 2579) |
| `keys` | adapt | `(keys m)` | `m -> (LSeq k) \| Reducible m (Pair k v)` | 1 | an empty recipe, not `nil` |
| `vals` | adapt | `(vals m)` | `m -> (LSeq v) \| Reducible m (Pair k v)` | 1 |  |
| `key` | keep | `(key p)` | `(Pair k v) -> k` | 1 | the map element is a `Pair`; `key` and `val` read it (a function, not a field: `(. p fst)`) |
| `val` | keep | `(val p)` | `(Pair k v) -> v` | 1 |  |
| `disj` | keep | `(disj s x ..)` | `s k -> s \| Dissoc s k` | 1 |  |
| `set` | keep | `(set c)` | `c -> (Set e) \| Reducible c e, Hash e, Eq e` | 1 |  |
| `includes?` (new) | new | `(includes? x c)` | `e c -> bool \| Reducible c e, Eq e` | 1 | an extra for `->>`; Clojure's idiom is `(some #{x} c)`, which runs once sets are callable (L21) |
| `SubVec` (new) | new | `(SubVec e)` | struct `(v: (Vec e) lo: i64 hi: i64)` | 3 | the O(1) vector view `subvec` returns: `Reducible` (an O(1) `nth`), `Lookup`, `Assoc`, `Collection` (end), `Stack`, `Keyed`; it becomes `Vec` itself when `Vec` is the struct with an offset field (§9 Q18) |
| `VSeq` (new) | new | `(VSeq e)` | struct `(front: (List e) v: (Vec e) lo: i64)` | 1 | the seq of a `Vec` (`rest`, `seq`, `next`) and the result of `sort`, `sort-by` and `take-last`, and the groups of `partition`: `Reducible`, `Seqable`, `Collection` (`conj` conses at the front), printing in parentheses ([R] A12 vseq) |
| `LSeq` (new) | new | `(LSeq e)` | struct `(box: (Cell (LState e)))` | 1 | Clojure's lazy seq: a node realised once by a thunk held in one cell (§2.1 rule 2; [R] A13 lz1); the result of every sequence function that is not fused, of `lazy-seq`, `repeatedly`, `line-seq`, `iteration`, `file-seq`, and the seq of the keyed collections; `Reducible`, `Seqable`, `Cursable`, `Collection` (a cons at the front), `Show` in parentheses; it holds a `Cell`, so it is not a `def` (§5 M1) and does not cross a task (§5 S10) |

### 4.6 `fib.sorted`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `subseq` | adapt | `(subseq sc test key)` | `s (fn (i64 i64) bool) k -> (SubSeq s) \| Sorted s k v` | 5 | `test` is `<`, `<=`, `>` or `>=` as a function value, as Clojure's; the collection applies `(test (compare ek key) 0)`, so no `Cmp` enum is needed |
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
| `clojure.string/blank?` | keep | `(str/blank? s)` | `str -> bool` | 1 | empty or whitespace only, in Java's sense (as `trim`) |
| `clojure.string/capitalize` | keep | `(str/capitalize s)` | `str -> str` | 4 |  |
| `clojure.string/ends-with?` | keep | `(str/ends-with? s suffix)` | `str str -> bool` | 1 |  |
| `clojure.string/escape` | adapt | `(str/escape s f)` | `str (fn (char) (Option str)) -> str` | 4 | `f` is a function, or any `Lookup` such as a `(Map char str)` once maps are callable (L21) |
| `clojure.string/includes?` | keep | `(str/includes? s sub)` | `str str -> bool` | 1 |  |
| `clojure.string/index-of` | adapt | `(str/index-of s sub) (str/index-of s sub from)` | `str str -> (Option i64); str str i64 -> (Option i64)` | 1 | **character offset** (§2.9); `(Option i64)`, never -1; the value is a `str` or a `char` (`Pattern`); the `from` arity is `str/index-of-from` until L1 |
| `clojure.string/join` | adapt | `(str/join c) (str/join sep c)` | `c -> str \| Reducible c e, ToStr e; str c -> str \| Reducible c e, ToStr e` | 1 | always qualified: an unqualified `join` is the task-wait builtin (§3 N9); `(str/join c)` is `(str/join "" c)` until L1 |
| `clojure.string/last-index-of` | adapt | `(str/last-index-of s sub) (str/last-index-of s sub from)` | `str str -> (Option i64); str str i64 -> (Option i64)` | 3 | character offset; `from` searches backwards from that offset, as Clojure's |
| `clojure.string/lower-case` | keep | `(str/lower-case s)` | `str -> str` | 4 | Unicode full case mapping, locale independent |
| `clojure.string/replace` | adapt | `(str/replace s from to)` | `str p str -> str \| Pattern p` | 3 | the match is a `str`, a `char` or a `Regex` (`Pattern`); a `str` replacement, with `$1` interpreted for a regex match and not for a string, as Clojure's; a function replacement is `str/replace-with` until L23 |
| `clojure.string/replace-first` | adapt | `(str/replace-first s from to)` | `str p str -> str \| Pattern p` | 3 | as `str/replace` |
| `clojure.string/reverse` | adapt | `(str/reverse s)` | `str -> str` | 4 | by Unicode scalar value, not by grapheme |
| `clojure.string/split` | adapt | `(str/split s re) (str/split s re limit)` | `str p -> (Vec str) \| Pattern p; str p i64 -> (Vec str) \| Pattern p` | 1 | `re` is a `Regex` (`#","`) or, as a typed superset, a `str` separator; **drops trailing empty strings** as Clojure's (Java's `split`); a `limit` of -1 keeps them; the `limit` arity is `str/split-limit` until L1 |
| `clojure.string/split-lines` | keep | `(str/split-lines s)` | `str -> (Vec str)` | 1 | splits at LF and CRLF |
| `clojure.string/starts-with?` | keep | `(str/starts-with? s prefix)` | `str str -> bool` | 1 | the prelude builtin `starts-with?`, re-exported: one function with two names for the 2 cases that use it, not two implementations |
| `clojure.string/trim` | keep | `(str/trim s)` | `str -> str` | 1 | **Java's `Character/isWhitespace`**, as Clojure's: U+0009 to U+000D, U+001C to U+001F, U+0020, U+1680, U+2000 to U+2006, U+2008 to U+200A, U+2028, U+2029, U+205F, U+3000; the no-break spaces U+00A0, U+2007 and U+202F are **not** trimmed (`char/whitespace?` is the Unicode `White_Space` property and does count them); `triml`, `trimr` and `blank?` use the same set; a string with nothing to trim is returned as it is (`cases/stdlib` 555, 556, 560, 571, 581) |
| `clojure.string/trim-newline` | keep | `(str/trim-newline s)` | `str -> str` | 3 |  |
| `clojure.string/triml` | keep | `(str/triml s)` | `str -> str` | 1 | Java's whitespace, as `trim` |
| `clojure.string/trimr` | keep | `(str/trimr s)` | `str -> str` | 1 | Java's whitespace, as `trim` |
| `clojure.string/upper-case` | keep | `(str/upper-case s)` | `str -> str` | 4 | Unicode full case mapping |
| `chars` (new) | new | `(str/chars s)` | `str -> Chars` | 1 | the explicit `Reducible char` view, a decoder over one byte array (2 objects for 1000 characters, [R] A10 chars); a `str` is itself a `Reducible char` (§2.9) |
| `str-len` (new) | new | `(str-len s)` | `str -> i64` | 1 | builtin: bytes, O(1) |
| `str-byte-at` (new) | new | `(str-byte-at s i)` | `str i64 -> i8` | 1 | builtin (§7 L18, landed): the byte at `i`, negative above 127, no allocation, traps `str-byte-at: index {i} out of range 0..{n}`; `str-bytes` allocates an array per call |
| `str-find` (new) | new | `(str-find s pat from)` | `str str i64 -> (Option i64)` | 1 | builtin (§7 L18, landed): the byte offset of the first match at or after `from`, an empty `pat` found at `from`; copies nothing and allocates only its result, **one `(Option i64)` object per call in the interpreter, none in stage 2 (C5, `cases/ownership` 270)** (hit or miss; the string is never copied); traps `str-find: from {from} out of range 0..{n}` and `str-find: from {from} splits a character`; `str/index-of` is built on it |
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

`(:require [fib.regex :as re])` or `(:use fib.regex)`: a facade over `lib/fib/regex/` (`charset ast parse prog pike dfa api`). Delivered by
the shootout's RE package, ahead of tranche 4's schedule, because regex-redux needs it; `re-matcher` and the
backtracking fallback (tranche 5) are not delivered. The literal `#"..."` is delivered by stage 2 (batch 6, RXM). The design is below the table.

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `re-pattern` | adapt | `(re-pattern s)` | `str -> Regex` | 4 | traps with `PatternSyntaxException: <message> near index N in <pattern>` on a bad pattern, as Clojure throws; `try-re-pattern` (new) returns a `(Result Regex str)`. The engine is non-backtracking, so no pattern can hang a call; what it refuses is listed below |
| `try-re-pattern` (new) | new | `(try-re-pattern s)` | `str -> (Result Regex str)` | 4 | the typed twin of `re-pattern` (§3 N13); `regex` as an unqualified alias is not delivered |
| `#"regex"` | adapt | `#"regex"` | reader literal, `Regex` | 4 | delivered by stage 2 only (the Rust reader is frozen). The reader reads it as `(fib.prelude/re "regex")` with Clojure's escape rule: a backslash and the character after it stay as they are, so `#"\d+"` is the pattern `\d+` and `#"a\"b"` the pattern `a\"b`. After a top-level form is expanded (`compiler/expand/regex.fib`) each such form is replaced by the name of a `:private` `def` placed before the form, `(def re$N :private (fib.regex.api/re-pattern "regex"))`, and the module requires `fib.regex.api` (a module that has a literal needs no `:require` of its own). The pattern is checked at that point with the library's parser: a bad one is `malformed regex literal: MESSAGE in #"regex"` at the literal, a compile error. The def is constant (`(re-pattern "literal")` of `fib.regex.api` is in the constant grammar of syntax §3.19 when the library accepts the text), so the compiler makes the `Regex` at compile time as static data: a literal costs nothing at run time, allocates nothing per search, and is shared by every task. A literal inside `quote` or a `defmacro` body is not rewritten; one in a macro's template is (the expanded program has it). Any `(def x (re-pattern "literal"))` is made at compile time the same way |
| `re-find` | adapt | `(re-find re s)` | `Regex str -> (Option Match)` | 4 | `Match` has `start`, `end` (byte offsets of the whole match), `whole`, `groups` (`(Vec (Option str))`, group 1 on, `nil` for a group that took no part) and `spans` (the offsets of every group, `2g` and `2g+1`, -1 for none), whatever the pattern: Clojure's result is a string without groups and a vector with them, a result type that depends on the pattern (§5 T5). `re-find-from` (new) takes a byte offset at a character boundary |
| `re-matches` | adapt | `(re-matches re s)` | `Regex str -> (Option Match)` | 4 | the whole string must match: of the matches that cover it, the one a backtracking matcher would find first |
| `re-seq` | adapt | `(re-seq re s)` | `Regex str -> (Vec Match)` | 4 | eager, not an `LSeq`; after an empty match the search goes on one character later, as `Matcher.find` does (but never into the middle of a character); `re-count` (new) is the number of matches, made without making them |
| `re-groups` | adapt | `(re-groups m)` | `Match -> (Vec (Option str))` | 4 | Clojure's vector: the whole match, then the groups; `re-group` (new) is one group, `match-start`, `match-end` its offsets. A `Matcher` (`re-matcher`) is tranche 5 |
| `replace-all`, `replace-first` (new) | new | `(replace-all re s template)` | `Regex str str -> str` | 4 | `Matcher.replaceAll`/`replaceFirst`: `$n` is group n (further digits are taken while that group exists), `\x` is `x`, anything else after `$` or a lone `\` traps; `re-quote-replacement` (new here: `str/re-quote-replacement` waits for the string facade) makes a template that inserts its argument as it is |
| `re-matcher` | adapt | `(re-matcher re s)` | `Regex str -> Matcher` | 5 | a struct with `Cell` state, single-threaded; `(re-find m)` advances it; not delivered |
| `Pattern` instance | new | `(impl Pattern Regex ..)` | | 4 | `str/split`, `str/index-of` and the rest of `fib.string` take a `Regex` as a pattern |

**The syntax** is the subset of `java.util.regex` that ordinary programs use, with Java's meaning: literals, `.` (any character but LF, CR, NEL, LS, PS), classes `[a-z0-9_]` with ranges and `^`, the escapes `\d \D \w \W \s \S`
(ASCII, as Java's default), alternation `|`, groups `( )` and `(?: )`, the quantifiers `* + ? {n} {n,} {n,m}` greedy and lazy (`?` after), the anchors `^ $ \A \z \Z` and the word boundaries `\b \B` (ASCII word characters), and the
escapes `\t \n \r \f \a \e \0ooo \xhh \uhhhh` and `\` before any non-letter. `^` is the start of the text and `$` its end or before a final line terminator, as without `MULTILINE`. A pattern may use any character of the
text: classes work on Unicode scalar values, offsets are bytes. **Refused with a message** (Java accepts them; a backtracking matcher with a step budget, tranche 5, is the place): backreferences, lookahead and lookbehind, flags (`(?i)`,
`MULTILINE`, `DOTALL`), named groups, possessive quantifiers, `\Q..\E`, `\p{..}`, `\G`, `\R`, nested classes and `&&` in a class; a repetition count over 1000 (a counted repetition is compiled as that many copies).

**The engine.** Two matchers over one program (a list of `SET`, `SPLIT`, `JMP`, `SAVE`, `ASSERT`, `LOOPCHK` and `MATCH`, characters partitioned into classes by the boundaries of the sets). The **Pike VM** runs the threads side by side in priority order, so the match it
finds is a backtracking matcher's (leftmost, then the first by priority: greedy and lazy quantifiers, the first alternative that works); it knows the groups; its time is at most (length of text) * (length of program), whatever the pattern, and its
memory a thread list. A loop whose body can match nothing is compiled as Java matches it: an iteration that matches nothing is accepted and ends the loop. The **lazy DFA** finds where a match lies in a pattern with no assertion and no such loop:
forward over the program with a thread started at every position, in first-match mode (when a `MATCH` is reached the threads below it are dropped), it gives the END of the match a backtracking matcher takes; backward over the reversed program, anchored at that
end, longest-first, it gives the START (the smallest start of a match that ends there is the start of the leftmost match). One table lookup per character, time linear in the text, states and transitions made as the text first needs them, at most
`min(10000, 2000000 / classes)` states, and a search that needs more is done by the Pike VM instead. `re-find` with groups takes the span from the DFA and runs the Pike VM anchored at its start.

**Immutable program, separate matcher state** (batch 6). A `Regex` is immutable: the pattern, the forward and reverse programs (`Prog`: instruction arrays, character classes, the class membership table) and, for a pattern with no assertion whose
DFA is small, the whole DFA as two tables (`Tab`: a row of one entry per character class for each state, the entry of the start state). `re-pattern` builds the tables by exploring every state and every class once (at most 128 states and 16384
entries per direction; a pattern that does not fit keeps `ok` false for that direction). So a `Regex` is `Send` and shared by tasks, and a search over a table reads memory and writes none: no cache, no state per search. A pattern whose DFA is
larger than the budget uses the lazy DFA, which is not part of the `Regex` but of a `Matcher` made for one search, or for one `re-seq`, `re-count`, `replace-all`, `replace-first` call (and for each `find-in` of `str/split`); it starts with room for 16 states and doubles up
to the cap. A lazy DFA starts cold, so for a text shorter than 256 bytes such a pattern is searched by the Pike VM instead (the same answer). Measured (batch 6, `docs/shootout/regex-literals.md`): for many tiny searches a table is
as fast as the old shared cache (the pattern `\d+` on 12 bytes: 81 ns against 92 ns), while a pattern with no table pays its cold start on every call (`a[ab]{14}c` on 44 bytes: 9 microseconds against 0.16 with the old cache) and a long
text amortises it. What building the tables costs `re-pattern` at run time is in the same note; a `#"..."` literal pays it at compile time.

**What the differential test found of Java** (`cases/stdlib/6100-ref-regex-vs-java-util-regex.fib`, `scripts/regex-diff/RegexDiff.java`): the answers are Java's on every pair, with two exceptions that are Java's, not the library's: for a group inside two nested repeats
Java keeps the capture of an earlier iteration of the outer one (`((a)*b)*` on `abaab` gives group 2 as the first iteration's), where the library keeps the last, as Perl does; and Java steps into the middle of a supplementary character after an empty match,
finding an empty match there that no text position holds. The test compares only the extent of the match for the first kind and keeps supplementary characters out of its random texts (they are in `6001`).

### 4.10 `fib.math` (alias `math`)

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `clojure.math/E` | keep | `math/e` | `f64` | 4 | a `def` |
| `clojure.math/PI` | keep | `math/pi` | `f64` | 4 | a `def` |
| `clojure.math/sqrt` | keep | `(math/sqrt x)` | `f64 -> f64` | 4 | IEEE-exact, so a builtin both tools agree on (§7 L11); `f32` too; **today** `lib/fib/math.fib` is a function over libm's `sqrt` (an `extern` in `fib.math.libm`, called inside `unsafe` in the library only), `f64` only, case 4204; replaced by the builtin when L11 lands (docs/shootout/gaps.md) |
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
| `clojure.math/floor` | keep | `(math/floor x)` | `f64 -> f64` | 4 | exact; a builtin candidate; `floor-i64` returns an integer |
| `clojure.math/ceil` | keep | `(math/ceil x)` | `f64 -> f64` | 4 | exact |
| `clojure.math/rint` | keep | `(math/rint x)` | `f64 -> f64` | 4 | half to even |
| `clojure.math/round` | adapt | `(math/round x)` | `f64 -> i64` | 4 | half up (toward positive infinity) as Java's, saturating; NaN is 0 |
| `clojure.math/signum` | keep | `(math/signum x)` | `f64 -> f64` | 4 |  |
| `clojure.math/to-degrees` | keep | `(math/to-degrees x)` | `f64 -> f64` | 4 |  |
| `clojure.math/to-radians` | keep | `(math/to-radians x)` | `f64 -> f64` | 4 |  |
| `clojure.math/floor-div` | keep | `(math/floor-div a b)` | `t t -> t \| Bits t, Num t` | 4 | traps on zero and on min / -1 |
| `clojure.math/floor-mod` | keep | `(mod a b)` | `t t -> t \| Num t, Ord t, Unit t` | 4 | alias of `mod` |
| `clojure.math/to-int-exact` | adapt | `(int x)` | `i64 -> i32` | 2 | the checked narrowing |
| `clojure.math/copy-sign` | keep | `(math/copy-sign x s)` | `f64 f64 -> f64` | 4 | from the bit casts |
| `clojure.math/get-exponent` | keep | `(math/get-exponent x)` | `f64 -> i64` | 4 | from `f64->bits` |
| `clojure.math/ulp` | keep | `(math/ulp x)` | `f64 -> f64` | 4 |  |
| `clojure.math/next-after` | keep | `(math/next-after x d)` | `f64 f64 -> f64` | 4 |  |
| `clojure.math/next-up` | keep | `(math/next-up x)` | `f64 -> f64` | 4 |  |
| `clojure.math/next-down` | keep | `(math/next-down x)` | `f64 -> f64` | 4 |  |
| `clojure.math/scalb` | keep | `(math/scalb x n)` | `f64 i64 -> f64` | 4 |  |
| `clojure.math/IEEE-remainder` | keep | `(math/ieee-remainder x y)` | `f64 f64 -> f64` | 4 |  |
| `clojure.math/random` | adapt | `(math/random)` | `-> f64` | 4 | the global generator (§2.8); `(rng/rand r)` takes an explicit `Rng` |
| `clojure.math/add-exact` | alias | `(math/add-exact ..)` | `t t -> t \| Num t` | 4 | alias of `+`, which already traps on overflow |
| `clojure.math/subtract-exact` | alias | `(math/subtract-exact ..)` | `t t -> t \| Num t` | 4 | alias of `-`, which already traps on overflow |
| `clojure.math/multiply-exact` | alias | `(math/multiply-exact ..)` | `t t -> t \| Num t` | 4 | alias of `*`, which already traps on overflow |
| `clojure.math/negate-exact` | alias | `(math/negate-exact ..)` | `t -> t \| Num t` | 4 | alias of `neg`, which already traps on overflow |
| `clojure.math/increment-exact` | alias | `(math/increment-exact ..)` | `t -> t \| Num t` | 4 | alias of `inc`, which already traps on overflow |
| `clojure.math/decrement-exact` | alias | `(math/decrement-exact ..)` | `t -> t \| Num t` | 4 | alias of `dec`, which already traps on overflow |
| `Math/sqrt` | alias | `(Math/sqrt ..)` | as `math/sqrt` | 4 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/pow` | alias | `(Math/pow ..)` | as `math/pow` | 4 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/abs` | alias | `(Math/abs ..)` | as `abs` | 1 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/floor` | alias | `(Math/floor ..)` | as `math/floor` | 4 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/ceil` | alias | `(Math/ceil ..)` | as `math/ceil` | 4 | the module `Math` (§4.16, [R] A11 e19) |
| `Math/round` | alias | `(Math/round ..)` | as `math/round` | 4 | the module `Math` (§4.16, [R] A11 e19) |
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
| `clojure.set/rename-keys` | adapt | `(set/rename-keys m kmap)` | `(Map k v) (Map k k) -> (Map k v) \| Hash k, Eq k` | 5 | a collision: **the last wins**, as Clojure's (replicated, §9.1 Q33) |
| `clojure.set/rename` | adapt | `(set/rename xrel kmap)` | `(Set (Map k v)) (Map k k) -> (Set (Map k v))` | 5 |  |
| `clojure.set/index` | adapt | `(set/index xrel ks)` | `(Set (Map k v)) c -> (Map (Map k v) (Set (Map k v)))` | 5 |  |
| `clojure.set/map-invert` | adapt | `(set/map-invert m)` | `(Map k v) -> (Map v k) \| Hash v, Eq v` | 3 | duplicate values: the last wins, as Clojure's (replicated, §9.1 Q33) |
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
| `println` | adapt | `(println a ..)` | macro `a ... -> unit \| Show a` | 1 | one `Show` argument or several, separated by a space ([R] A10 pm); in value position it is a function twin: `println` and `print` over `Show`, `str` over `ToStr`, `pr` and `prn` over `Debug` (§2.7; [R] A10 twin ran the `Show` form: `(run! println xs)`); writes to `*out*`; **landed** (R5, `605a26e`): `(println a b ..)` is `(fib.prelude/println S)`, `S` the `(fib.prelude/show x)` of each argument joined by one space, **a literal `nil` argument printing the word `nil`**, `(println)` writing the empty line, and `println` of a `str` variable allocating one more object than the one-`str` function did, because `show` copies the string (syntax §4.4; cases 750 to 759; the text written is judged by `crates/fibref/tests/print_macros.rs` and its `fibc` twin, since a case reads results and not stdout) |
| `print` | adapt | `(print a ..)` | macro `a ... -> unit \| Show a` | 1 | no newline; the macro writes through the prelude's `fib.prelude/print-str`, **a one-`str` function that writes and returns `unit`**, which is not this table's `print-str` (below) |
| `pr` | adapt | `(pr a ..)` | macro `a ... -> unit \| Debug a` | 2 | strings quoted and escaped at every depth (`Debug`); implemented in tranche 1 beside `prn` (R5), though its row says tranche 2 |
| `prn` | adapt | `(prn a ..)` | macro `a ... -> unit \| Debug a` | 1 | `pr` and a newline; needs `fib.core` in scope for `fib.core/debug` (syntax §4.4) |
| `print-str` | adapt | `(print-str a ..)` | macro `a ... -> str \| Show a` | 2 | `print` to a string: the arguments joined with a space (`(print-str 1 2)` is `1 2`, `(str 1 2)` is `12`: the first version mapped it to `str`). **A name clash until this macro lands:** since R2 the prelude has a public `(print-str s: str) -> unit` that writes `s` without a newline (the target of the `print` macro); a `:use`d `fib.print` that defines Clojure's `print-str` shadows it in the program, and the macros name it `fib.prelude/print-str`, so they are unaffected |
| `pr-str` | adapt | `(pr-str a ..)` | macro `a ... -> str \| Debug a` | 2 | `pr` to a string; `debug-str` is not offered |
| `flush` | keep | `(flush)` | `-> unit` | 4 | the prelude writes unbuffered, so a no-op until buffering exists |
| `printf` | adapt | `(printf "fmt" a ..)` | macro `-> unit` | 4 | the directives are checked against the argument types at compile time |
| `format` | adapt | `(format "fmt" a ..)` | macro `-> str` | 4 | `%s` over `ToStr` (Clojure's `%s` is `str`), `%d` an integer, `%f` a float, `%x`; a literal format string is checked at compile time and a non-literal at run time (§2.9); locale independent |
| `format-f64` (new) | new | `(fmt/format-f64 x digits)` | `f64 i64 -> str` | 4 | module `fib.fmt`: `x` as fixed-point text with `digits` places, as C's `%.*f`; stand-in for the `%f` directive of `format` until it lands; over glibc's `strfromd` (an `extern` in `fib.fmt.libc`, called inside `unsafe` in the library only), so it rounds the exact binary value where Java's `%.9f` rounds the shortest decimal text half up (they differ only on a value whose shortest text ends in a 5 exactly at the last place); case 4205 (docs/shootout/gaps.md) |
| `pprint` | adapt | `(pprint x)` | `a -> unit \| Pretty a` | 5 | long tail |
| `read-line` | adapt | `(read-line)` | `-> (Option str)` | 4 | `nil` at end of input; `fib.io` |
| `dbg` (new) | new | `(dbg e)` | macro `a -> a \| Show a` | 1 | the prelude's debugging macro, kept: prints `dbg POS: e = <text>` to stderr and returns the value (`crates/fibref/src/expand/prelude/forms.rs`), the text being `fib.prelude/show` of the value, so a string is not quoted; it would be `Debug` only if the owner decides so (§2.7; case 611); not a Clojure name |
| `slurp` | adapt | `(slurp path)` | `str -> str` | 1 | the whole file; traps `slurp: cannot read ..` where Clojure throws `FileNotFoundException` (§1.2 P5); the builtin `read-file`, which it wraps, returns the `(Option str)`, and `try-slurp` (new) a `(Result str str)` |
| `try-slurp` (new) | new | `(try-slurp path)` | `str -> (Result str str)` | 1 | the typed twin of `slurp` (§3 N13) |
| `spit` | adapt | `(spit path s)` | `str str -> bool` | 1 | alias of the builtin `write-file`, which exists today; `:append true` appends |
| `println-str` | adapt | `(println-str a ..)` | macro `a ... -> str` | 2 | `print-str` and a newline |
| `prn-str` | adapt | `(prn-str a ..)` | macro `a ... -> str \| Debug a` | 2 | `pr-str` and a newline |
| `newline` | alias | `(newline)` | `-> unit` | 1 | writes a newline to `*out*` |
| `print-table` | adapt | `(print-table rows)` | `(Vec (Map keyword v)) -> unit \| Show v` | 5 | over rows of one value type |
| `tap>` | adapt | `(tap> x)` | `(dyn Show) -> bool` | 5 | a global tap list in an `Atom`; `add-tap`, `remove-tap` |
| `add-tap` | adapt | `(add-tap f)` | `(fn :send ((dyn Show)) unit) -> unit` | 5 | registers a tap |
| `remove-tap` | adapt | `(remove-tap f)` | as `add-tap` | 5 | removes a tap |
| `read-string` | adapt | `(read-string s)` | `str -> Form` | 4 | the reader of `fib.syntax` (spec/bootstrap.md); traps on malformed text, as Clojure throws; `try-read-string` (new) returns a `(Result Form str)`; `(read-as T s)` through a `Read` protocol for typed data |
| `try-read-string` (new) | new | `(try-read-string s)` | `str -> (Result Form str)` | 4 | the typed twin of `read-string` (§3 N13) |
| `read` | adapt | `(read r)` | `Reader -> (Result Form str)` | 5 | over a reader value; `read+string` also returns the text read |
| `read+string` | adapt | `(read+string r)` | `Reader -> (Result (Pair Form str) str)` | 5 | as `read` |
| `*out*` | adapt | `*out*` | `(dyn Writer)` | 5 | a dynamic var (§2.11, C11); `println` writes to `*out*` |
| `*in*` | adapt | `*in*` | `(dyn Reader)` | 5 | a dynamic var (§2.11, C11); `read-line` reads from it |
| `*print-length*` | adapt | `*print-length*` | `(Option i64)` | 5 | a dynamic var (§2.11, C11); `println`, `pr` and `str` of a collection stop after that many elements |

### 4.15 `fib.async`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `swap-vals!` | adapt | `(swap-vals! a f)` | `(Atom a) (fn (a) a) -> (Pair a a)` | 4 | old and new |
| `reset-vals!` | adapt | `(reset-vals! a v)` | `(Atom a) a -> (Pair a a)` | 4 |  |
| `compare-and-set!` | adapt | `(compare-and-set! a old new)` | `(Atom a) a a -> bool` | 4 | **by identity** for objects (a pointer compare-and-swap, as Clojure's) and by value for scalars; `Eq` is not consulted |
| `delay` | adapt | `(delay e)` | macro `a -> (Delay a)` | 4 | a closure and a `Cell`; forced once; `@d` is `force`; a top-level `(def d (delay ..))` waits for C9 (§5 M1, §9.2 Q41) |
| `force` | adapt | `(force d)` | `(Delay a) -> a` | 4 |  |
| `cancelled?` (new) | new | `(cancelled?)` | `-> bool` | 5 | the poll of the running task's interrupt flag, which `future-cancel` sets (§2.11); not a Clojure name |
| `promise` | adapt | `(promise)` | `-> (Promise a) \| Send a` | 5 | an `Atom` and a wait; needs a blocking wait primitive |
| `deliver` | adapt | `(deliver p v)` | `(Promise a) a -> bool` | 5 | true when this call delivered |
| `future` | adapt | `(future body ..)` | macro `-> (Task a)` | 1 | a macro over `spawn`; `@f` is `(join f)` (the `Deref` instance of a `Task`, types §2.9, **Proposed**, owner's rule 2026-10-01; case 658); the closure is `:send`, so a `Cell` capture is rejected ([R] A11 t41: `cell cannot be shared between threads: closure capture c has type (Cell i64)`) |
| `future-call` | adapt | `(future-call f)` | `(fn :send () a) -> (Task a)` | 1 | `(spawn f)` |
| `future-done?` | adapt | `(future-done? t)` | `(Task a) -> bool` | 5 | alias `done?` |
| `pmap` | keep | `(pmap f c)` | `(fn :send (a) b) c -> (Vec b) \| Reducible c a` | 1 | eager, one task per element today; chunked over `ncpu + 2` tasks with the order kept in tranche 3 ([R] A11 e15: `[1 4 9 16 25 36 49 64 81 100]`); lazy over an `LSeq` of futures in tranche 5 |
| `seque` | adapt | `(seque n c)` | `i64 c -> (LSeq e) \| Reducible c e` | 5 | a bounded producer/consumer over an `Atom` queue and a task |
| `add-watch` | adapt | `(add-watch r k f)` | `(Ref a) k (fn :send (k a a) unit) -> unit` | 5 | a `Ref` of an `Atom` and a watch list `(Atom (Vec (fn :send ..)))`; runs under both tools ([R] A11 e18: 42, t42: 50) |
| `remove-watch` | adapt | `(remove-watch r k)` | `(Ref a) k -> unit` | 5 | see `add-watch` |
| `set-validator!` | adapt | `(set-validator! r f)` | `(Ref a) (fn :send (a) bool) -> unit` | 5 | a `:send` closure called before the value is stored; a false result traps `Invalid reference state`, as Clojure throws |
| `get-validator` | adapt | `(get-validator r)` | `(Ref a) -> (Option (fn :send (a) bool))` | 5 | see `set-validator!` |
| `future-cancel` | adapt | `(future-cancel t)` | `(Task a) -> bool` | 5 | Clojure's: sets the task's interrupt flag, which blocking calls observe and answer by ending the task (a trap of the task, which `try-join` returns since L28 stage 1; §5 S16); no memory-safety failure forbids it (§2.11); `(cancelled?)` is the poll for a computation |
| `future-cancelled?` | adapt | `(future-cancelled? t)` | `(Task a) -> bool` | 5 | reads the flag |

### 4.16 `fib.sys`

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `Thread/sleep` | adapt | `(Thread/sleep ms)` | `i64 -> unit` | 4 | module `Thread`; `(sleep ms)` is the alias |
| `System/currentTimeMillis` | adapt | `(System/currentTimeMillis)` | `-> i64` | 4 | module `System`; `(now-ms)` is the alias |
| `System/nanoTime` | adapt | `(System/nanoTime)` | `-> i64` | 4 | monotonic; `(nano-time)` is the alias |
| `System/getenv` | adapt | `(System/getenv name)` | `str -> (Option str)` | 4 | an `Option` where Java returns null; `(getenv name)` is the alias |
| `System/exit` | adapt | `(System/exit n)` | `i64 -> a` | 4 | or return `n` from `main`; `(exit n)` is the alias |
| `*command-line-args*` | adapt | `*command-line-args*` | `-> (Vec str)` | 2 | a `def` of the builtin `(args)` (L15); the empty `Vec` where Clojure has `nil` |
| `clojure-version` | adapt | `(clojure-version)` | `-> str` | 4 | the library's version string; `(version)` is the alias |

### 4.17 Not offered

The names that Clojure has and this library does not offer, with the reason. None is a memory-safety reason: they are the JVM, no run-time
type information (§5 T3), a result type that depends on a value (§5 T5) or is a union (§5 T8), legacy structs, and the name `defstruct`, which
is fibber's record form. The first version's 282 rows were 192 reversed into the tables above (§10.4.6), 1 duplicate (`#"..."`), and 89 that the first
rewrite kept; the second revision moved 44 type predicates of them to §4.18 as checker forms (§10.4.8 F13), so these are the 45 that stay.

| Clojure name | Verdict | Instead | Reason |
|---|---|---|---|
| `iterator-seq` | omit | - | Java objects: no JVM here; fibber's `Reducible` protocol is the native thing |
| `enumeration-seq` | omit | - | Java objects |
| `xml-seq` | omit | - | walks `clojure.xml`'s dynamic map trees and the JVM XML parser (heterogeneous values and the JVM) |
| `resultset-seq` | omit | - | Java objects |
| `struct-map` | omit | - | legacy structs hold values of any type under keyword keys (§5 T4); superseded by records in Clojure itself |
| `struct` | omit | - | legacy; see `struct-map` |
| `create-struct` | omit | - | legacy; see `struct-map` |
| `accessor` | omit | - | legacy; see `struct-map` |
| `defstruct` | omit | - | the name is fibber's record form (syntax §3.7); Clojure's legacy basis is superseded by records |
| `num` | omit | - | no boxed `Number` in a typed language (§5 T3) |
| `class?` | omit | - | no reflection (§5 T3) |
| `instance?` | omit | - | no run-time type information: a `match` on an enum is the run-time test (§5 T3) |
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

### 4.18 Static predicates (checker forms)

The predicates that Clojure asks of a run-time tag (`string?`, `vector?`, `seq?`, `number?`, `satisfies?`) were omitted by the first rewrite because the answer "is a constant of the static type" (§5 T3). A constant is an answer, and a checker form that folds to it keeps Clojure's text in ported code: `(string? x)` is the constant `true` or `false` of `x`'s static type, so `(filter string? xs)` over a `(Vec str)` is all true, and over the library's dynamic enums `Val` (§4.13) and `Form` (macros) it is a variant test, which is where Clojure code asks the question that has a run-time answer. They are checker forms (§7 L31, **[sketch]**: no program of the review exercises them, and `(string? "a")` is `unbound name string?` today, [R] A12 pred1). A generic function is checked once at its generalised type, so a test of the *type* is for monomorphic code and `Val`/`Form`; code that must treat types differently uses a protocol or a `match` (§5 T3).

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `map-entry?` | adapt | `(map-entry? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `Pair`; over `Val` a variant test |
| `sorted?` | adapt | `(sorted? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `SortedMap` or `SortedSet`; over `Val` a variant test |
| `char?` | adapt | `(char? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `char`; over `Val` a variant test; over `Form` a variant test |
| `boolean?` | adapt | `(boolean? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `bool`; over `Val` a variant test; over `Form` a variant test |
| `seq?` | adapt | `(seq? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a seq type: `List`, `VSeq`, `LSeq`, a recipe; over `Val` a variant test; over `Form` a variant test |
| `coll?` | adapt | `(coll? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type with `Collection`; over `Val` a variant test; over `Form` a variant test |
| `list?` | adapt | `(list? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `List`; over `Val` a variant test; over `Form` a variant test |
| `vector?` | adapt | `(vector? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `Vec`, `SubVec`; over `Val` a variant test; over `Form` a variant test |
| `map?` | adapt | `(map? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `Map`, `SortedMap`; over `Val` a variant test; over `Form` a variant test |
| `set?` | adapt | `(set? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `Set`, `SortedSet`; over `Val` a variant test; over `Form` a variant test |
| `string?` | adapt | `(string? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `str`; over `Val` a variant test; over `Form` a variant test |
| `keyword?` | adapt | `(keyword? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `keyword`; over `Val` a variant test; over `Form` a variant test |
| `symbol?` | adapt | `(symbol? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a `Form` symbol; over `Val` a variant test; over `Form` a variant test |
| `number?` | adapt | `(number? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type with `Num`: the integers, the floats, `Ratio`, `BigInt`, `BigDecimal`; over `Val` a variant test; over `Form` a variant test |
| `integer?` | adapt | `(integer? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is an integer type or `BigInt`; over a `(Ratio t)` a run-time test, `true` iff the denominator is 1, so `(integer? (/ 6 3))` is `true` as Clojure's; over `Val` a variant test; over `Form` a variant test |
| `int?` | adapt | `(int? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a fixed-width integer type; over `Val` a variant test; over `Form` a variant test |
| `float?` | adapt | `(float? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `f32` or `f64`; over `Val` a variant test; over `Form` a variant test |
| `double?` | adapt | `(double? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `f64`; over `Val` a variant test; over `Form` a variant test |
| `decimal?` | adapt | `(decimal? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `BigDecimal`; over `Val` a variant test |
| `ratio?` | adapt | `(ratio? x)` | `a -> bool` | 3 | over a `(Ratio t)` a run-time test of the denominator, so `(ratio? (/ 6 3))` is `false` as Clojure's ([R] A13 div: `[true false]`); the constant `false` for any other static type; over `Val` a variant test |
| `rational?` | adapt | `(rational? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is an integer type, `BigInt` or `Ratio`; over `Val` a variant test |
| `fn?` | adapt | `(fn? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a function type `(fn ..)`; over `Val` a variant test |
| `ifn?` | adapt | `(ifn? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a function type, or a type callable by L21 (`Map`, `Set`, `Vec`, `keyword`); over `Val` a variant test |
| `associative?` | adapt | `(associative? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type with `Assoc`; over `Val` a variant test |
| `sequential?` | adapt | `(sequential? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is the sequential family: `Vec`, `SubVec`, `List`, `VSeq`, `Range`, `LSeq`, recipes; over `Val` a variant test |
| `counted?` | adapt | `(counted? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type whose `Reducible.size` is O(1): `Vec Map Set Array Range Option SubVec`; over `Val` a variant test |
| `reversible?` | adapt | `(reversible? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type with `Reversible`; over `Val` a variant test |
| `indexed?` | adapt | `(indexed? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type whose `Reducible.nth` is O(1): `Vec Array Range Option SubVec`; over `Val` a variant test |
| `seqable?` | adapt | `(seqable? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type with `Reducible`; over `Val` a variant test |
| `ident?` | adapt | `(ident? x)` | `a -> bool` | 3 | over a `keyword` or a `Form` symbol the constant `true`; the constant `false` for any other static type |
| `simple-ident?` | adapt | `(simple-ident? x)` | `a -> bool` | 3 | over a `keyword` or a `Form` symbol a function of its text: `true` iff it has no `/` (§4.3 `namespace`); the constant `false` for any other type |
| `qualified-ident?` | adapt | `(qualified-ident? x)` | `a -> bool` | 3 | over a `keyword` or a `Form` symbol a function of its text: `true` iff it has a `/`; the constant `false` for any other type |
| `simple-keyword?` | adapt | `(simple-keyword? x)` | `a -> bool` | 3 | over a `keyword` a function of its name: `true` iff it has no `/` (§4.3 `namespace`); the constant `false` for any other type |
| `qualified-keyword?` | adapt | `(qualified-keyword? x)` | `a -> bool` | 3 | over a `keyword` a function of its name: `true` iff it has a `/`; the constant `false` for any other type |
| `simple-symbol?` | adapt | `(simple-symbol? x)` | `a -> bool` | 3 | over a `Form` symbol a function of its text: `true` iff it has no `/`; the constant `false` for any other type |
| `qualified-symbol?` | adapt | `(qualified-symbol? x)` | `a -> bool` | 3 | over a `Form` symbol a function of its text: `true` iff it has a `/`; the constant `false` for any other type |
| `uuid?` | adapt | `(uuid? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `Uuid`; over `Val` a variant test |
| `inst?` | adapt | `(inst? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `Inst`; over `Val` a variant test |
| `bytes?` | adapt | `(bytes? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `(MArray i8)` or `(Array i8)`; over `Val` a variant test |
| `record?` | adapt | `(record? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is a type made by `defrecord`; over `Val` a variant test |
| `delay?` | adapt | `(delay? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `(Delay a)`; over `Val` a variant test |
| `future?` | adapt | `(future? x)` | `a -> bool` | 3 | a constant of the static type: `true` iff the argument's type is `(Task a)`; over `Val` a variant test |
| `satisfies?` | adapt | `(satisfies? P x)` | `P a -> bool` | 3 | a constant: `true` iff the type of the argument has an instance of the protocol `P`, which the compiler knows (types §4); `(satisfies? P x)` is the question Clojure asks at run time |
| `extends?` | adapt | `(extends? P T)` | `P T -> bool` | 3 | a constant: `true` iff the type `T` has an instance of the protocol `P` |

### 4.19 `fib.unix`: the sys primitives (package S5)

The groundwork of an io library written in fibber: unix file descriptors, small enough that the interpreter
(`crates/fibref/src/eval/sys.rs`, through the `libc` crate) and the compiled runtime (`rt/sys.lir`,
through the C library) give the same answers, so a library built on them is judged by both tools (method rule 6;
`extern` is refused by the interpreter and cannot be). The module is `lib/fib/unix.fib`, not `fib.sys`, which §4.16
keeps for the Java names of tranche 4. The case block is 3600 to 3649.

**Names (decided).** A builtin lives in one flat table, so the builtins carry the prefix `sys-`; the library names
(`fd-open`, `getenv`, ..) are functions of `fib.unix`, which a program `:use`s (it is not implicit).

**The builtins** answer an `i64` (or, where noted, another type) and never trap: a result below zero is the negated
errno. A descriptor is an `i64`. Bytes are an `(Array i8)`, as `str-bytes` and `str-from-bytes` use them, because
bytes read from a descriptor need not be UTF-8 and a `str` must be.

| Builtin | Signature | Note |
|---|---|---|
| `sys-open` | `str i64 i64 -> i64` | open(2): the path, the flags, the mode; the descriptor. A path with a NUL is 22 (EINVAL) |
| `sys-close` | `i64 -> i64` | 0 |
| `sys-read` | `i64 i64 -> (Array i8)` | the first 8 bytes are the status as a little-endian `i64` (bytes read, or the negated errno), then the bytes read; at most `n`, `n` above 2^30 read as 2^30, `n` below 0 is 22; a status of 0 is end of file. `fib.unix/fd-read` unpacks it (no builtin can return a `Result` without the checker knowing its layout, and an array can be built without mutation) |
| `sys-write` | `i64 (Array i8) i64 i64 -> i64` | write(2) of bytes `off` to `off+n` of the array: the number written, which may be short; a range outside the array is 22 |
| `sys-seek` | `i64 i64 i64 -> i64` | lseek(2): the new offset |
| `sys-pipe` | `-> i64` | pipe(2): the read end shifted left 32 bits, or'd with the write end |
| `sys-dup` `sys-unlink` `sys-mkdir` `sys-rmdir` | `i64 -> i64`, `str -> i64`, `str i64 -> i64`, `str -> i64` | 0 or the descriptor |
| `sys-isatty` | `i64 -> bool` | isatty(3); false for a descriptor that is not open |
| `sys-errno-text` | `i64 -> str` | strerror(3) (`2` is `No such file or directory`); bytes that are not UTF-8 become U+FFFD |
| `sys-getenv` | `str -> (Option str)` | nil for an unset variable and for a name with a NUL; the value's bytes made UTF-8 as `from_utf8_lossy` does |
| `sys-clock-now` `sys-wall-now` | `-> i64` | CLOCK_MONOTONIC and CLOCK_REALTIME in nanoseconds |
| `sys-sleep` | `i64 -> i64` | at least that many nanoseconds, resumed after a signal; 0; nothing for a time that is not positive |

The functions of `fib.unix`, as table rows (the `covers:` lines of cases 3600 to 3609 name them):

| Name | Verdict | Spelling | Signature | T | Note |
|---|---|---|---|---|---|
| `fd-open` (new) | new | `(fd-open path flags mode)` | `str i64 i64 -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-close` (new) | new | `(fd-close fd)` | `i64 -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-read` (new) | new | `(fd-read fd n)` | `i64 i64 -> (Result (Array i8) i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-write` (new) | new | `(fd-write fd bytes)` | `i64 (Array i8) -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-write-range` (new) | new | `(fd-write-range fd bytes off n)` | `i64 (Array i8) i64 i64 -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-seek` (new) | new | `(fd-seek fd off whence)` | `i64 i64 i64 -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-pipe` (new) | new | `(fd-pipe)` | `-> (Result (Pair i64 i64) i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-dup` (new) | new | `(fd-dup fd)` | `i64 -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-isatty` (new) | new | `(fd-isatty fd)` | `i64 -> bool` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-unlink` (new) | new | `(fd-unlink path)` | `str -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-mkdir` (new) | new | `(fd-mkdir path mode)` | `str i64 -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `fd-rmdir` (new) | new | `(fd-rmdir path)` | `str -> (Result i64 i64)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `errno-text` (new) | new | `(errno-text e)` | `i64 -> str` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `getenv` (new) | new | `(getenv name)` | `str -> (Option str)` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `clock-now` (new) | new | `(clock-now)` | `-> i64` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `wall-now` (new) | new | `(wall-now)` | `-> i64` | 1 | `fib.unix`, over the `sys-` builtin of the same call |
| `sleep-ns` (new) | new | `(sleep-ns ns)` | `i64 -> i64` | 1 | `fib.unix`, over the `sys-` builtin of the same call |

An error of the C library is returned as it is: `EINTR` is not retried by `sys-read` and `sys-write` (the library
above loops if it wants to). Both tools take the flags as an `i32` and the mode as an `unsigned`.

**The library** `fib.unix` has the Linux numbers `O_RDONLY 0 O_WRONLY 1 O_RDWR 2 O_CREAT 64 O_EXCL 128 O_TRUNC 512
O_APPEND 1024`, `SEEK_SET 0 SEEK_CUR 1 SEEK_END 2`, `stdin 0 stdout 1 stderr 2`, and, each answering `(Result t i64)`
whose error is the errno itself: `(fd-open path flags mode)`, `fd-close`, `(fd-read fd n) -> (Result (Array i8) i64)`
(an empty array is end of file), `(fd-write fd bytes)` and `(fd-write-range fd bytes off n)` (bytes written; the
library above loops on a short write), `(fd-seek fd off whence)`, `(fd-pipe) -> (Result (Pair i64 i64) i64)`
(read end, write end), `fd-dup`, `(fd-isatty fd) -> bool`, `(fd-unlink path)`, `(fd-mkdir path mode)`,
`(fd-rmdir path)`, `(errno-text e) -> str`, `(getenv name) -> (Option str)`, `(clock-now)`, `(wall-now)` and
`(sleep-ns ns)`. Sockets are the next package (S6).

### 4.20 `fib.bigint`: BigInt (package BIG, the shootout suite)

The arbitrary precision integer of §2.8, `lib/fib/bigint.fib` over `lib/fib/bigint/{mag,magmul,magdiv,big}.fib`; a program
`:use`s `fib.bigint` (it is not implicit, so a program that does not need it does not compile it). The case block is 6000
to 6099: 6000 to 6002 are the differential cases against `java.math.BigInteger` (`scripts/bigint/BigDiff.java` and
`tl.bigdiff` draw the same seeded operands from `tl.rng`; each case embeds the digests the Java program printed), 6010 to
6016 are the unit, trap and law cases with expected values computed by Python's integers.

**Representation.** A `BigInt` is a sign (-1, 0, 1) and a magnitude, an `(Array i64)` of 31-bit limbs, least significant
first, with no zero limb at the top; zero is the empty magnitude. Limbs hold 31 bits, not 32, because the arithmetic
traps on overflow and a limb product plus two limbs must fit a signed `i64`. Multiplication is schoolbook below 40 limbs of
the shorter operand and Karatsuba above (an operand twice as long as the other is cut into pieces of the shorter one's
length; a single-limb multiplier takes the `mag-mul-small` loop); division is Knuth's Algorithm D (a single-limb divisor
takes a short division); text is converted nine digits at a time (quadratic).

| Clojure name | Verdict | Fibber | Signature | T | Note |
|---|---|---|---|---|---|
| `parse-bigint` (new) | new | `(parse-bigint s)` | `str -> (Option BigInt)` | 5 | an optional `+` or `-` and one or more decimal digits; `nil` for anything else; `(bigint s)` is the trapping form |
| `gcd` (new) | new | `(gcd a b)` | `BigInt BigInt -> BigInt` | 5 | the non-negative greatest common divisor, `(gcd 0 0)` is 0; Euclid over Algorithm D |
| `expt` (new) | new | `(expt a n)` | `BigInt i64 -> BigInt` | 5 | `a` to the `n >= 0`, by squaring; a negative `n` traps `expt: negative exponent` |
| `isqrt` (new) | new | `(isqrt a)` | `BigInt -> BigInt` | 5 | the floor of the square root of `a >= 0` (Newton from above); a negative `a` traps |
| `big-shift-left` (new) | new | `(big-shift-left a n)` | `BigInt i64 -> BigInt` | 5 | `a * 2^n`; a negative `n` shifts right |
| `big-shift-right` (new) | new | `(big-shift-right a n)` | `BigInt i64 -> BigInt` | 5 | the floor of `a / 2^n`, as Java's `shiftRight`: a negative `a` rounds toward minus infinity; the bit operations on negative numbers (`bit-and`, ...) are not offered |
| `big-bit-length` (new) | new | `(big-bit-length a)` | `BigInt -> i64` | 5 | the bits of the magnitude, 0 for zero (Java's `bitLength` differs for negatives) |
| `big-mul-small` (new) | new | `(big-mul-small a m)` | `BigInt i64 -> BigInt` | 5 | `a * m` for `0 <= m < 2^31` in one pass over the limbs; any other `m` is `(* a (bigint m))`; `(* a b)` with a one-limb `b` takes the same loop |
| `big-fits-i64?` (new) | new | `(big-fits-i64? a)` | `BigInt -> bool` | 5 | whether `(long a)` converts |
| `big-even?` (new) | new | `(big-even? a)` | `BigInt -> bool` | 5 | `even?` needs `Bits`, which BigInt does not implement |
| `big-odd?` (new) | new | `(big-odd? a)` | `BigInt -> bool` | 5 | |

The rows `bigint` and `biginteger` of §4.3 keep their text (a `ToBig` protocol: `BigInt`, the four integer widths and `str`
convert). The instances of BigInt are `Num` (`+ - * quot rem neg`; `quot` truncates, a zero divisor traps `BigInt: divide by
zero`), `Eq`, `Ord`, `Hash` (of the limbs, not Clojure's), `Unit` (so `inc dec abs mod zero? pos? neg? max min compare` are the
library's generic ones), `Show`, `ToStr` (the digits), `Debug` (the digits and `N`), `ToLong` (traps `long: value out of range`),
`ToDouble` (nearest, ties to even, an infinity beyond the range) and `(Div (Ratio BigInt)) BigInt` (`/` is exact: `(/ 6 -4)` is
`-3/2` in lowest terms with a positive denominator, so `(Ratio BigInt)` is the tranche 5 ratio of §2.8). **Not delivered:**
`+'` `-'` `*'` `inc'` `dec'` (the reader ends a symbol at `'`, so the names cannot be written; docs/shootout/gaps.md), the reader
literal `1N`, `bigdec`, and bases other than 10.

## 5. Deviations from Clojure

The first version of this page had 82 deviation rows. Under the rule (§1.1) a deviation needs a memory-safety failing
program or a static-typing fact the language already decided. Of the 82, 32 had neither and are reversed, 21 are adapted to
a typed twin that keeps Clojure's text (some as the language change of §7 that makes them Clojure's), 11 stand on a decided
typing fact (group T below), and 18 were already Clojure's behaviour (§10.4.2 gives every row). The memory-safety group M is new:
the first version gave one memory-safety reason, for `add-watch`, and it was false (§4.15); the first rewrite gave six, and the rule check
showed that four of them were refuted by programs that run (§10.4.8 R1 to R5), so two stand, one narrowed by the third revision (M2 is the array of objects; the array of scalars is T11). What remains is below, in five
groups: **M** memory safety, **T** decided static typing, **D** decisions the owner made, before the rule and under it on 2026-10-01, **C** consequences of
the owner's decisions under the rule, **S** differences that exist only until an item of §7 lands (a stage limit, not a
deviation of the design). No deviation stands on the cost side (the second revision's group **K**, decided by the owner: §5.7, §9.1 Q34).

### 5.1 Memory safety (M): the failing program

| # | Clojure | Here | The failing program, or why none can be run |
|---|---|---|---|
| M1 **[sketch]** | a `def` value may be a mutable cell, a lazy seq, a delay or an array (`(def c (long-array 3))`, `(def fibs (lazy-cat ..))`), shared by every thread | `def` may hold an `Atom` (so `(def c (volatile! 0))` works, §2.11), never a type that holds a `Cell` or a `Weak`: an `MArray`, an `LSeq`, a `Delay`, a `Matcher`; a global lazy value or `delay` waits for the run-once cell of C9 (§9.2 Q41) | a `Cell` in a top-level `def` is reachable from every task without being captured, so two tasks write one `Cell` (for an `LSeq`, the realisation of a node) and the capture check does not see it. **The failing program cannot be written today**, because `def` takes a constant initialiser: `(def c: (Cell i64) (cell 0))` and `(def a: (Atom i64) (atom 0))` give the same `def c: initialiser is not a constant expression` ([R] A13 def1, def2); the check that exists rejects the closure form of the same race, `cell cannot be shared between threads: closure capture c has type (Cell i64)` ([R] A11 t41), which is the evidence that the rule applies. With L15 the rule becomes a type test, "the type of a `def` contains no `Cell`/`Weak`", in L15's own commit, and the case of §8.1 item 8 is written then; until L15 lands M1 is a sketch in the sense of §1.1, as the removed M3 and M5 were: a deviation whose program does not yet exist |
| M2 | two threads may write one array | one task may share an array and write it in place (`(aset a i x)` on an `MArray`); two tasks may not hold one | `(let ((a (MArray (cell (array 2 0))))) (let ((t1 (spawn (fn () (aset a 0 1)))) (t2 (spawn (fn () (aset a 0 2))))) ..))` is `cell cannot be shared between threads: closure capture a, field c of MArray has type (Cell (Array i64))` ([R] A12 aset3): for an array of **objects** the two writes would both release the old element, a double release (a consequence of the counted representation; no program shows it, because the check rejects the program); for an array of scalars the slot has no release and the race is a lost update, which is not one of P0's four failures, so that half of the deviation stands on a decided typing fact and is T11. Inside one task the sharing is Clojure's and runs: `[9 4 0]` and `[y x]` (A12 aset1, aset2), and the write through a cell that two closures share gives 3 and 49 (A12 p1, p10) |

Four rows of the first rewrite are gone, each refuted or unproven: **M3**, an uninitialised object slot (no program could show it; `make-array` of an object type is an `(MArray (Option T))`
filled with `nil`, §5 T1); **M4**, the UTF-16 unit (Clojure's counts run and audit clean as user code; the unit is `char`'s, §5 T10); **M5**, `future-cancel` (Java's interrupt is cooperative, §5 S16);
**M6**, `locking` (a lock built from two `Atom`s runs, 600, and the program the first rewrite cited was rejected whatever `locking` is, §2.11). §10.4.8 R3 to R5 give the programs.

### 5.2 Static typing already decided (T): and the typed twin that keeps Clojure's text

| # | Clojure | Here | The decided fact, the program that shows it, the twin |
|---|---|---|---|
| T1 | `nil` is a value of every type | `nil` is `Option`'s empty variant (types §1.5): `first last peek get find` and `parse-long`, `index-of` return `(Option v)`; `keys` of an empty map is an empty recipe | `(conj nil x)` is `no implementation of Collection for (Option a)` ([R] A11 n7): the result is a `List` for `nil` and a `Vec` for a vector. Twins: a condition accepts `(Option T)` (§2.4), a literal `nil` first argument of `conj assoc merge` expands (§2.4), `(get (get m :a) :b)` after L16, `some->`. `(first xs)` of a `List` returning `Option` is **Decided** (Q28) |
| T2 | a `fn` literal may have several arities and a rest parameter; `partial`, `comp`, `juxt` return functions of any arity | a closure value has one type with one arity (types §1.4); a multi-arity or rest-parameter literal takes the arity of its expected type (`%&` the same, L22), and a multi-arity function that is stored or returned is not offered; the combinators are arity-reading checker forms (§2.2, L22) | `(partial add3 1)` for a three-parameter `add3` is `cannot unify (fn :send (i64 i64 i64) i64) with (fn (a b) c)` ([R] A11 arity). Twin: the checker reads the arity from the argument's type; a multi-arity `defn` is overloading of a name (L1); a hand-written transducer `(fn [rf] (fn ([] ..) ([r] ..) ([r x] ..)))` cannot be ported as it stands, it is an `Xf` (§2.1 rule 7) |
| T3 | `vector?`, `seq?`, `instance?`, `type`, `class`, `satisfies?`, `eval` ask a run-time tag | no run-time type information: the answer is a constant of the static type; `eval` has no static result type (§1.2) | a typed `vector?` exists only where an impl does: `no implementation of IsVec for i64` ([R] A11 n16). Twins: the checker forms of §4.18, which fold to the static answer (a variant test over `Val` and `Form`), a protocol constraint in the signature, a `match` on an enum, `isa?` over a hierarchy value |
| T4 | a vector or map holds values of any types: `[1 "a"]`, `{:a 1 :b "x"}`; a record is a map with extra keys; `get-in` takes a path computed at run time | one element type per `Vec`, one key and one value type per `Map`; a struct is not a map | `[1 "a"]` is `cannot unify i64 with str` ([R] A11 t91, k8), `{:a 1 :b "x"}` the same (t92). Twins: the entry is a `Pair`/`Triple` (§2.3, L25 for a literal), a map literal of mixed values is `(Map keyword Val)` (L25), `Val` and a runtime `get-in-v` ([R] A11 e17), `defrecord` with `(assoc r :k v)` by L21 |
| T5 | a result type that depends on a value: `(/ 7 2)` is a ratio, `+'` a `Long` or `BigInt`, `re-find` a string or a vector by the group count, `flatten` any nesting | one result type per function: integer `/` is always a `(Ratio t)` (`quot` truncates; Q40 **Decided**), `+'` returns a `BigInt`, `re-find` returns a `Match`, `flatten` is one level or over a recursive enum | `(/ 7 2)` is `3` today ([R] A11 k9, §5 S17) and `7/2` with the `Div` protocol ([R] A13 div, A12 ratio). Twins: `(Ratio t)` whose predicates read the value (`(ratio? (/ 6 3))` is `false`, A13 div) and `BigInt` ([R] A11 t50), `Match` with `whole` and `groups`, `Val` |
| T7 | `=` across types of one family is true, across families false | `=` has one type (types §2.12); within a family the checker elaborates to `seq=` (§2.7, L24) | `(= [1] #{1})` is a `cannot unify`, which is what Clojure's `false` would hide |
| T8 | an `if` or `or` may return different types in its arms: `(or (even? x) (get m k))`, `(if c 1 "a")`; `trampoline`'s result is a value or a function | no union types: arms unify | `(if c 1 "a")` is `cannot unify i64 with str`. Twins: recipes and closures erase at a join (L24, C1), `seq-of`; `trampoline` is not offered (§4.17) |
| T9 | `(comp (map f) (filter p))` composes transducers, which are functions generic in the accumulator | a transducer is an `Xf` value, a factory of steppers; `xf` composes them, and after C1 `comp` dispatches on `Fn` and `Xf` (§2.1 rule 7) | `let` does not generalise (types §2.4), and an impl head on a function type is rejected: `(comp (xmap inc) (xfilter odd?))` is `cannot unify (Xf i64 i64) with (fn (a) b)` ([R] A10 comp) |
| T10 | a `char` is a UTF-16 unit, so a string's seq has an element per unit and `(count "😀")` is 2 | a `char` is a Unicode scalar (types §1.1, **Decided**), a string's seq has one element per scalar, and `count`, `nth`, `get`, `subs` and `index-of` count scalars | `(dotimes [i (count s)] (nth s i))` needs `count` and `nth` to agree: with `count` in UTF-16 units and `nth` by element, `(count "a😀z")` is 4 and the loop traps `nth: index out of range` ([R] A12 unit); Clojure's units are replicable and safe as user code (A12 p2, p2b), so the cause is `char`, not memory safety. Twin: a program that needs UTF-16 offsets for Java interop writes `u16-len` over `str-chars`; the *order* of strings is code-point order, the order of their characters (§2.7, D5) |
| T11 | two threads may write one array of `long`s or `double`s | an `(MArray t)` of scalars cannot be held by two tasks | `(Cell T)` is never `Send` (types §2, **Decided**) and an `MArray` holds one: `cell cannot be shared between threads: closure capture a, field c of MArray has type (Cell (Array i64))` ([R] A12 aset3). The race on a scalar slot is a lost update, not a double release (M2 is the object case); Clojure's `long[]` written by two threads loses updates too **[K]**. Twin: an `Atom` per slot, or the `Mutex` of C9 |

### 5.3 Decisions the owner made (before the rule, and under it on 2026-10-01)

| # | Clojure | Here | Status |
|---|---|---|---|
| D1 | `throw`, `try`, `catch`, `finally`, `ex-info` | `trap` is the only abort, and a trap in a task ends the process (types §2.11, **Decided** 2026-09-28) | no memory-safety failure prevents exceptions (§2.10); the rule reverses the decision (§9.1 Q35: exceptions, after the library is viable), and the types amendment is the owner's to sign |
| D2 | mixed numeric operands promote | no implicit conversion between numeric variables (types §1.1 D3); literals adopt (L19) | a statically typable lattice with no safety content (§2.8); the rule lifts it (§9.1 Q36, L26 b); the owner signs the §7 item |
| D3 | `(abs Long/MIN_VALUE)` is `Long/MIN_VALUE` | overflow traps at every width, `abs` included (types §2.12, **Decided**, §9 Q15) | Clojure's `+` throws on overflow too; this is the one overflow it ignores |
| D4 | `(/ 7 2)` is the ratio `7/2` | integer `/` truncates toward zero, Rust's (types §2.12, **Decided** 2026-09-27: "Integer arithmetic has Rust's semantics") | **reversed by the owner (Q40, Decided, 2026-10-01)**: integer `/` is exact, a `(Ratio t)`, through the `Div` protocol whose result type the instance determines (the typing question the third check raised, §2.8), and `quot` is the builtin's name (§7 L30); the compiler's one use, three case files, the generator and two unit tests migrate (§8.3) |
| D5 | strings order by UTF-16 code unit: `(compare "😀" "\uFFFF")` is negative | strings order by code point, the order of their `char`s: `(< "\uFFFF" "😀")` is true ([R] A13 ordstr) | **Decided**, owner-invited, 2026-10-01 (the owner may overrule; §9.1 Q42): not a safety fact; a `char` is a scalar (types §1.1, **Decided**) and a string order that disagreed with its characters' order would be inconsistent; the builtin instance needs no change; it differs from Clojure only for a character above U+FFFF against one in U+E000..U+FFFF |

### 5.4 Consequences of the owner's decisions under the rule

| # | Clojure | Here | Why |
|---|---|---|---|
| C1 | `(= 1 1.0)` is `false`, and `(== 1 1.0)` is `true` | `(= 1 1.0)` is `true`: the literal `1` adopts `f64` (L19, **Decided**), so `=` between two numbers is `==` | the owner's L19; `==` is an alias of `=` (§4.1) |
| C2 | the printed order of a map of more than 8 entries is the JVM hash's | the order of this library's hash; changes once with the new integer hash (§9 Q15, **Decided**) | the hash is part of the spec; tests compare with `=` or sort (§2.7) |

### 5.5 Until a §7 item lands (a stage limit, not a deviation of the design)

| # | Clojure | Today | Lands with |
|---|---|---|---|
| S1 | `(if (get m k) ..)`, `(or (get m k) 0)`, `(when c 5)` as a value | `cannot unify (Option i64) with bool` ([R] A11 t01, t02), `cannot unify unit with i64` (t82); the `Truthy` library and macros run (t04, 127) | L20 |
| S2 | `(m k)`, `(#{1 2} x)`, `(v 0)`, `(:k m)`, `(filter #{1 2} xs)` | `cannot unify (Map i64 i64) with (fn (a) b)` (A11 t51, n5), `(Set i64)` with `(fn (a) bool)` (n6), `keyword` (k1), `(Vec i64)` (k2) | L21 (the keyword half is tranche 2, case `1000`, `open:` L14; the collections are tranche 3, case `901`: §8.2.1 item 6), or C1's `Fn` |
| S3 | `partial`, `comp`, `constantly`, `juxt`, `apply` of any arity | the one-free `partial`, unary `comp` and `constantly` (A11 arity..arity4) | L22 |
| S4 | `(into {} [[1 2]])`, `(some #(.. (Option bool)) ..)`, `str/replace` with a function; a protocol or multimethod that dispatches on every argument | a protocol is dispatched on one head per parameter today (types §4), a stage limit and not a typing fact: two instances of one head overlap, `overlapping instances: OrHit for (Option a) is already implemented`, `... Collection for (Map k v) ...` (A11 t70, t71) | L23 |
| S5 | `(if flag (filter p v) v)`, `(= [1] (list 1))` | `cannot unify (Vec i64) with (Filtered (Vec i64) i64)` ([R] A10 join), `cannot unify Range with (Vec i64)` (A11 n4); `seq-of` and `seq=` run | L24 |
| S6 | `(+ x 1)` for `x: i32`, `(+ n 2.5)` for a variable | `cannot unify i64 with i32` (A11 n3c), `cannot unify f64 with i64` (n1) | L26, §9.1 Q36 |
| S7 | `(get m k 0)`, `(nth c i d)`, `(reduce f c)`, `(sort cmp c)`, `(map f c1 c2)`, `(defn f ([x] ..) ([x y] ..))` | `get takes 2 argument(s), got 3`; `get`, `nth`, `sort` and `map` take their clauses (Y11/Y11b); the stand-ins `reduce1` and `range-by` remain | L1 |
| S8 | `(def counter (atom 0))`, `(def stopwords #{"a" "the"})`, a global `rand` | `def counter: initialiser is not a constant expression` (A11 t56) | L15 |
| S9 | `(throw ..)` caught by `try` | `trap` (A11 t40, e9). Still so: stage 2 of L28 (`try`/`catch`/`finally` by unwinding) is not built; what is built is stage 1, a task's trap isolated at its thread entry and returned by `try-join` (`fib.async`, cases 911, 915 to 926), which catches a trap at a task boundary only | L28 stage 2, §9.1 Q35 |
| S10 | a lazy seq crosses a thread, or is a `def` | an `LSeq` holds a `Cell`: `cell cannot be shared between threads: closure capture s, field box of Cached has type (Cell (Option (Vec i64)))` (A11 e8d); the `Atom` memo crosses (e8e) but may run its thunk twice; a `def` of an `LSeq` is M1's type test | C9 (the run-once cell; §9.2 Q41 recommends tranche 3) |
| S11 | `(count "aé€😀z")`, `(subs s 1 3)`, `(map f "abc")` are O(1) or O(n) scalars | today `count` of a `str` is `no implementation of Reducible for str` (A7); the user-code impl runs (A11 e3a, e3b) | tranche 1 (O(n)), C10 for O(1) |
| S12 | `~x`, `#"re"`, `2r1010`, `^:private`, `::kw` | `unknown reader syntax`; `,x` is the unquote | E14 |
| S13 | `(println (some 3))` prints `3`, and `(str ["a"])` is `["a"]` | `(some 3)` (A11 t87); the prelude's `show` is one text for `str` and `println` | the prelude change of §2.7 (`ToStr`, `Show`, `Debug`) |
| S14 | `(->> c (map f) (filter p) (reduce + 0))` allocates nothing per element | the fusion rewrite is an expander pass that does not exist: today the chain is written with the recipes by hand, or fused by the `fuse` macros of the prototype ([R] A13 lz1, lz2) | E16 |
| S15 | `(with-redefs [f g] ..)` on any function | no program of the review exercises it; `f` is a direct call today | L29 |
| S16 | `(future-cancel f)` interrupts a blocked thread | a task's trap now ends the task and not the process (L28 stage 1, delivered: `try-join` returns `(Err (Trap message))`, `join` and `@t` trap in the joiner with the message, cases 911, 915 to 917). `future-cancel` itself is **not delivered**: no cancel primitive or interrupt flag exists, a blocking call has nothing to observe, and the flag is polled with `(cancelled?)` only once it does | the interrupt flag and the blocking calls that read it |
| S17 | `(/ 7 2)` is `7/2` | the builtin `/` truncates, `3` ([R] A11 k9), and a generic `(/ x (+ x x))` at `i64` is `0` ([R] A13 quot); `(Ratio t)` and the `Div` protocol run as library code ([R] A13 div). **Closed by the flip**: `/` is `Div`'s, `(/ 7 2)` is `7/2` (case 877) | L30 |
| S18 | `(str 1e21)` is `1.0E21`, `(str (/ 1.0 0.0))` `Infinity` | `1000000000000000000000.0` and `inf` ([R] A13 fl); the library function of A13 fltfmt gives Clojure's text over today's `show` | C12 |

### 5.6 Sharp edges of Clojure that this page replicates

These are not safety matters, so the rule says to replicate them: `compare` built on `<` (a NaN compares equal to everything,
[R] A11 nan), `compare` of vectors by length first, `(take-nth 0 c)` repeating the first element, `(range 0 1 0.1)`
accumulating (11 elements, [R] A11 range) and step 0 repeating the start, `update-keys`, `set/map-invert` and
`set/rename-keys` keeping the last of a collision, `partition` dropping an incomplete last group, **`sort-by`'s key function running twice per comparison**
(`sort-by-cached` is the one-call form, §4.4), **chunking** (the fused path's one latitude, §2.1 rule 2), **`hash-map`'s hash order against the array map's insertion
order** (§2.7) and, if Clojure's `(hash 0.0)` differs from `(hash -0.0)` (not verified, **[K]**), that too. **Chunking:** a `map` over a chunked source (`Vec`, `Range`, `Array`, `SubVec`)
runs `f` for a chunk of 32 elements before the first is consumed, so `(first (map f (range 100)))` calls `f` 32 times in Clojure **[K]**; the stage ran it for `map`:
`take` of 1, 32, 33 and 40 elements over a range of 100, 1 over a range of 10 and 5 over an empty range call `f` `[32 32 64 64 10 0]` times under both tools ([R] A12 chunk),
with one 32-slot buffer per traversal of the mapping stage and none per element (**[H]** for the other adaptors: `filter`, `keep` and `mapcat` chunk the same way in Clojure and were not written).
The rule says to replicate all of them, and the owner may strike any row: each is a one-line change either way (§9.1 Q33). The one place where a replicated behaviour gives up determinism is a `sort` over floats that
contain a NaN (§2.7).

### 5.7 The cost side (K): none

The second revision's K1 (a recipe consumed once where Clojure's lazy seqs are cached) was the one deviation on the cost side. The owner
decided it for Clojure's behaviour on 2026-10-01 (§9.1 Q34): sequence functions return memoised lazy seqs, and the zero-cost push loop
is an optimisation under the syntactic rule of §2.1 rule 2, which changes no observable behaviour. No deviation stands here.

## 6. Where the library lives

### 6.1 What exists today [R] A9

**Updated by tranche 0 (E7, E12, commit `8adeaa6`) and R1 (commit `5ea989f`):** modules are found under the main file's directory, then each `-I DIR`, each
directory of `FIB_LIB`, then the roots embedded in the binary (`crates/fibref/src/roots.rs`, cases `cases/modules` 013 to 016); `(:export-from ..)` exists; two
`:use`d modules that export one name make a bare reference an error, as syntax §5 says (cases 009 to 011; E12 is closed and the paragraph below is the state of
`36ed472`); the loader reads the **implicit modules** (`modules::IMPLICIT_LIB`, **`fib.core fib.seq fib.coll fib.print` since the flip**, M) first and lets every module that is not the library's own see
them as it sees the prelude (`fibref expand` omits them like the prelude, `--implicit` and `--implicit-lib` print or replace them, bootstrap §5.1). Own definitions
come first, then `:use`s, then the implicit layer, then the prelude and the builtins; `fib.x/name` resolves from any module that sees `fib.x`; two implicit modules
that export one name make a bare use `x is exported by both A and B; write A/x or B/x`, and the gate `lib_disjoint` fails the build before that can happen.
`lib/prelude.fib` is 696 lines.

`fib.prelude` is `lib/prelude.fib` (504 lines at `36ed472`, with the `hash-combine` of §2.7), compiled into the binary, read, expanded and checked
together with the program on every run. Other modules loaded from **the main file's directory only**
(`a.b` is `a/b.fib` beside the main file): there was no library root, no environment path, no cached
interface and no re-export; there is still no cached interface. **Two `:use`d modules that exported one name did not make an error: the first
`:use` won, silently.** `y.m1` and `y.m2` both export `peek`; `(:use y.m1 y.m2)` then `(peek 3)` is 3 and
`(:use y.m2 y.m1)` is 4, under both tools ([R] A10 use). `spec/syntax.md` §5 says the name is an error when
referenced unqualified, so the spec and the code disagreed; this page changed neither: §7 E12 and §9
Q27 asked the owner to make the checker do what §5 says, and tranche 0 did. A local definition shadows a `:use`d one, the prelude
included (a program that defines its own `Box`, `Entry` or `Pair` works, [R] A6).

### 6.2 Layout (proposed)

```
lib/prelude.fib      keeps Vec (trie), Map and Set (HAMT), List, Box, println and eprintln and the private raw writes through
                     tranche 1 (PC-1, below); Pair, Triple and Result join it with L4
lib/fib/core.fib     a facade (PC-2, below) of core/{base,protocols,num,opt,alias,text}: Option helpers, Pair Triple Step Result,
                     Unit and the numeric functions, Ratio and Div, compare, the combinators,
                     Eq Ord Hash Show Debug for the collections                                      implicit :use
lib/fib/seq.fib      a facade of seq/{types,protocols,lseq,vseq,lazy,hash,sources,chars,consumers,recipes,adaptors,seqable,sort}:
                     Reducible, Cursor, sources, LSeq and VSeq, lazy-seq, the lazy sequence functions and their recipes,
                     consumers, sort, group-by, frequencies, Xf                                          implicit :use
lib/fib/coll.fib     a facade of coll/{protocols,seqs,map,ops,chars}: the key-addressed protocols, the instances of Vec, List, Map and Set
                     for them, vec and into (the bulk builders, §2.5), SubVec, get-in family, merge  implicit :use
lib/fib/print.fib    a facade of print/{show,io}: println print pr prn str format printf (the macros), Show/Debug helpers  implicit :use
lib/fib/sorted.fib   SortedMap SortedSet (B-tree), Queue                                             require
lib/fib/string.fib   clojure.string                                                                  require as str
lib/fib/char.fib, regex.fib, math.fib, set.fib, walk.fib, data.fib, random.fib (global generator and `rng/`), io.fib, sys.fib,
                     async.fib, bigint.fib, multi.fib (defmulti), meta.fib, dyn.fib (binding), test.fib
lib/Math.fib, Long.fib, Integer.fib, Double.fib, Character.fib, System.fib, Thread.fib
                     the Java static names Clojure code writes, implicit aliases (E15)
```

**Facades and parts (PC-2, tranche 1 plan §4).** Each of the four implicit modules is a facade, `(ns fib.seq (:export-from fib.seq.types ..))`
(`:export-from` is tranche 0, E7), over part files that one agent owns each, so that parallel work never edits one file; a name is
exported by exactly one part. A module whose `ns` starts `fib.` gets no implicit uses and writes its `:use` lines by hand, in the order
`fib.core`, `fib.seq`, `fib.coll`, `fib.print`, then its own parts; a part may `:use` a facade above it or a part before it in its own
facade, never a part after it, which the checker refuses as the import cycle it is (`cases/stdlib/015`). **Implicit names reach library modules only as the names
the module itself `:use`s** (R1): a macro template inside a part cannot name that part's own facade, because the facade loads after its parts (a macro in
`fib.seq.lazy` whose template says `fib.seq/lazy-node`, expanded in another part of `fib.seq`, is `module fib.seq is not loaded` or an unbound name), so a part that
needs a name of its facade `:use`s the part that defines it, or the macro is written to use another route (L6's `lazy-seq`). **`Vec`, `Map`, `Set` and `List`
stay in `lib/prelude.fib` through tranche 1 (PC-1).** The type checker, `fibc` and the vector literal find `Vec` by name in the
prelude (`crates/fibref/src/types/init.rs`, `crates/fibc/src/names.rs`), `fibc`'s `rt/vec.lir`, `fibref`'s `eval/vecs.rs` and
`fibc`'s `lower/quote.rs` read its layout, and moving them would put `fib.seq` and `fib.coll` in an import cycle (`fib.seq` needs the
`VSeq` that `fib.coll` placed and `fib.coll` needs the `Reducible` that `fib.seq` placed). So the types the two share, `VSeq` and
`LSeq`, are in `fib.seq.types`, and `fib.coll` holds the protocols and the instances for the prelude's types.

Rules. The four implicit modules are `:use`d by every module as `fib.prelude` is now; their export sets
are disjoint, and a test that loads all of them reports any name two of them export: because the compiler
checks nothing (§6.1), that test is the only protection against about 400 unqualified names colliding, and it
is a CI gate of tranche 0. Everything else
is `:require`d with an alias, so `str/join` never meets the task-wait builtin `join`. A module is
found under the main file's directory, then under each `-I DIR` (also `FIB_LIB`), then under the
roots embedded in the binary, as `include_str!` embeds the prelude today (§7 E7); a facade module
re-exports with `(:export-from m ..)`. The compiler's own modules (`compiler/`) keep working
unchanged, because a local definition shadows a library name; the one exception was `compiler/util/result.fib`:
a library `Result` and that file's `Result` are two types of one name, so a module that `:use`s it cannot
exchange values with one that does not, and M2 deleted the file and removed `util.result` from the `:use` of
the 32 modules that named it (the prelude's `Result` has the same name, variants and fields: `(Ok v)`, `(Err e)`).

### 6.3 Macros

A macro costs nothing under `fibref` and about 80 to 85 ms under `fibc`, once per distinct macro per
program: one JIT module each ([R] A9: an empty `main` 0.10 s; 1, 5, 20 distinct macros 0.20, 0.50,
1.70 s; `fibref` 0.20 s for 20). So a macro that every program uses is a Rust prelude macro, like `when`
and `->`: **`str`, `println`, `print`, `prn`, `pr`, `swap!`, the variadic folds of `+ - * < > <= >= = max
min merge conj assoc dissoc bit-and bit-or bit-xor`, the literals `{..}` (`array-map`) and `#{..}`
(`hash-set`), `if-not`, `when-not`, `defn`, `defn-`, `some` (by argument count), `update`, and `list`** (E1),
each specified by a case that quotes its expansion. The list is thirty-two, two swapped against the first version (**status after tranche 1, commit `605a26e`:** the Rust registry has 46 rows, the 19 that
existed at `36ed472` (`when unless cond and or if-let when-let list plet while dotimes for-each range -> ->> doto assert dbg derive`) and 27 new: `str println
print prn pr` (R5), `+ - * < > <= >= = max min bit-and bit-or bit-xor`, `conj assoc dissoc merge` and `swap!` (R6a), `defn defn- update` and the 33rd, `reduce`
(R6b, PC-6); five of the thirty-two are **not done**: `if-not`, `when-not`, `some` (by argument count, which needs L1) and the literals `{..}` (`array-map`) and
`#{..}` (`hash-set`, which needs the reader of E8); each landed macro's expansion is quoted by Rust unit tests (`crates/fibref/src/expand/tests/prelude_*.rs`, `hygiene.rs`) and by cases in `cases/stdlib`,
and mirrored in `compiler/expand/prelude/*.fib`, syntax §4.4 has its rows):
**`comp` and `partial` leave** (they are arity-reading checker forms, L22, which a macro cannot be because it cannot
see `f`'s arity) and **`defn`, `defn-`, `some` and `update` join** (each is on every program's path, and a fibber macro
would cost 80 to 95 ms each); `and` and `or` are core forms that the checker elaborates, the expander passing them through, and `when` and the one-armed `cond` expand to the one-armed `if`, which the core form `if` takes (L20; plan PC2-11, §8.2.1 item 11). The
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

**Tranche 2's policy** (plan PC2-2, §8.2.1 item 2): a macro is Rust, with a mirror in `compiler/expand/prelude/`, when almost every program of the compiler uses it or stage 2 must expand it before it can run macros; every other macro is fibber, in a part of the library, and its body uses prelude names and builtins only until E2 lands. The Rust macros of tranche 2 are the five open ones of E1 and the folds and printers that are one rewrite each (15 rows, §8.2.1).

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

**Measured at the flip (M1, 2026-10-02; one run each, other jobs on the machine).** `fibc run` of `(defun main () -> i64 0)`: 0.094 to 0.107 s before
(five runs of the committed binary), 0.113 to 0.125 s after (five runs, the `dev` build): the four implicit facades, thirty files, are read, expanded and checked with every program
(about 20 ms), and the library's instances are monomorphised only where used. `fibc cases cases/ownership` (240 cases, both tools): 65 s before and 75 s after;
`fibref cases cases/ownership`: 20 s after. The expansion dump of a program expands the same thirty files first: 23 ms in Rust, 40 to 100 ms in the self-hosted expander, and the byte-for-byte test
of the expander (`bootstrap_expand`) went from 292 s to 681 s for that reason. Tranche 2's package Z2 makes the self-hosted expander expand the implicit library once per process (plan PC2-12, §8.2.1 item 12): the output stays byte for byte the same and the cost per program becomes the program's own.

### 6.5 Names the prelude already uses

| Name | Today | Resolution |
|---|---|---|
| `some` | `Option`'s constructor and pattern | stays; Clojure's `(some pred c)` is the Rust macro of E1, which picks by argument count (§6.3; not an L1 clause, §8.2.1 item 3), and `find-map`, `find-first` are extras |
| `any?` | not defined | Clojure's unary `any?`; the quantifier is `some` |
| `empty`, `cons` | the `List` variants | variants became `Empty`, `Cons` (**landed**, R2, commit `5ea989f`, cases `cases/modules` 020 to 024); `empty` is `Emptyable`, `cons` a function (Clojure's names are the functions) |
| `next` | the `Iter` method | removed with `Iter`; Clojure's `next` is `Seqable` (§2.1 rule 8) |
| `join` | the task-wait builtin | stays; the string join is `str/join` |
| `range` | a macro for two arguments and a function for one | after the flip: `(range n)` is the library function `fib.seq/range`, returning `Range`, and the macro rewrites `(range a b)` to `(fib.seq/range-by a b 1)`; the three-argument form is spelled `range-by` until arity overloading (tranche 2, L1) makes `range` one function |
| `map count first rest nth get assoc conj` | exist at narrower types (`map` on `Vec` only; `first`, `rest` on `List`) | widened by `Reducible`, `Seqable` and the protocols of §2.3 |
| `derive`, `defstruct` | a prelude macro, a core form | `derive` is also Clojure's hierarchy form, told apart by the first argument's kind (§4.2); `defstruct` stays fibber's record form (§4.17) |
| `println` | the one-`str` function | a macro over `Show` in head position (**landed**, R5); a function generic over `Show` serves value position (`fib.print`, L11), and `fib.prelude/println`, which the macro calls, stays reachable ([R] A6, A10 twin) |
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
| `Show (Option a)`, `Show` of a `Map` | `(some 3)`, and key-order | printed as Clojure prints them (§2.7; `Option` and `List` landed with R2, `Map` and `Set` in `fib.print`); no case pins printed text (the headers pin results and traps; 21 case files call `show` or `println`), so the change costs no case |

### 6.6 What stage 2 mirrors

M6 step 2 (the expander) is next, and every Rust-side item of this page is something stage 2 must reproduce
byte for byte. The first version listed only E1; this is the whole list, so the owner can sequence it.

| Item | What stage 2 must do | Tranche | Notes |
|---|---|---|---|
| E1 macros (§6.3) | the 32 Rust macros, each specified by a case that quotes its expansion (28 of the 33 are in the registry and mirrored in `compiler/expand/prelude/`, §7.5) | T0 | `list` expands to qualified names; `for-each`/`range`/`dotimes` counting loops stay; `defn` converts bracket parameters |
| E3, E4 | bracket binding forms; flat `cond` (six call sites in three files of `compiler/`: `jit-demo.fib`, `syntax/lexer.fib` x3, `lair/call.fib` x2, besides the Rust macro `logic.rs` and two Rust test files) | T2 | additive; replacement |
| E8, E14 | `#(..)` and `#{..}` read as forms; `~x`, `~@x` with the comma as whitespace, `x#`, `#'x`, `#"re"`, radix, `1N 1M 1/2`, `##Inf`, `#?`, `#tag`, `^`, `::kw` | T2 (E14a, without `x#` and `#"re"`: §8.2.1 item 4), T3 (E14b), T5 (E14c) | four pinned reader tests turn from errors into reads (`compiler/tests/reader/harness-067-hash-paren.fib`, `harness-068-hash-brace.fib`, `reader-218-dispatch-brace.fib`, `reader-220-dispatch-paren.fib`), the comma change touches every `,x` of `lib/`, `compiler/` and the cases, and `spec/bootstrap.md` §2 defines no dump line for a form the reader synthesises (the position of the generated `fn`): it must be added in the same commit (§9 Q38) |
| L1, L2, L7, L8, L3b, L6, L9, L14, L15, L21 | arity clauses (no constructor clause: `some` is a Rust macro, §8.2.1 item 3), rest parameters `& xs`, irrefutable patterns in parameters, refutable and prefix `let`, tuple patterns, or-patterns, `{:keys ..}` with `:or`, keyword and collection in call position, `def` initialisers and the module init | T2, T3 | each changes the expander or the checker that M6 steps 2 to 4 reproduce |
| L20, L22, L24, L25, L26, L31 | truthiness (`if`/`and`/`or`/`when`/`cond`), arity-reading forms, joins and families, heterogeneous literals, literal widths and promotion, static predicates | T2, T3, T4 | the checker; each is a rule over the types the checker has already solved, so stage 4 (the checker) carries them |
| L16, L17, L23 | determined variables in an impl context; `derive Debug`/`ToStr` and the derive-by-default decision; multi-parameter dispatch (§9 Q39) | T2, T3 | the checker; the Rust `derive` module |
| E16 | the fusion rewrite of §2.1 rule 2: after macro expansion, the collection argument of a terminal consumer becomes recipes; every other sequence-function call is its lazy form | T1 | the expander; a pass over resolved forms, specified by the `fuse` macros of A13 lz1 |
| L30, C12 | `Num` without `/`, `quot` the builtin (on floats the truncated quotient), `fdiv`; the text of a float | T1, T0 | the checker's builtin table and the lowering; `float_text` and `fib.show-fp` (§7.4) |
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
The rule (§1.1) is what adds L20 to L31, E14, E15, B5 and C8 to C11: each is a place where Clojure's text is
feasible and no memory-safety failure forbids it. They change types or syntax the owner decided, so the owner signs each
(P8); L16, L19, Q15 and Q28 are already **Decided** (owner, 2026-10-01; §9.1 says which of them the rule settles and which edits of the other specs each needs, §7.4), and the owner's four decisions of 2026-10-01 add E16 (the fusion rewrite) and C12 (the text of floats), rework L30 (`/` as `Div`'s method) and withdraw L13.

**Status.** §7.5 says which items have landed, with the commit and the cases; the **Evidence** columns below are the state of `36ed472` and are kept as the failing
programs that justify each item.

### 7.1 Stage-1 divergences (method rule 6: the interpreter and the compiler must agree)

| # | Change | Evidence | Size | Needed by |
|---|---|---|---|---|
| B1 | `fibc` lowers a protocol method that has its own type variable; `fibref` accepts it | `(defprotocol (Fold s e) (fold (self f: (fn (r e) r) :borrow init: r) -> r))`: `fibc` says `unsupported: a type variable reached the lowering: Gen(1)`, `fibref` returns 45 (A6). Cause: `method_target` passes only the impl's variables | small | C1 (a method that takes an `Fn` argument), accumulator-typed methods |
| B2 | quasiquote expands to a prelude-qualified `concat`; `` `() `` works under `fibc` | with `(defun concat (a b) ..)` in scope, every quasiquote macro is `concat takes 2 argument(s), got 3` (A6); `` (defmacro u () `()) `` is `macro u failed: concat of nothing` under `fibc` and 1 under `fibref` (A6) | tiny | T1: the library defines `concat` |
| B3 | protocol method symbols are mangled with the defining module | a user `(defprotocol (Collection s e) (conj ..))` with an `impl` for `(Vec a)` and any use of the prelude's `conj` on a `Vec` gives `fibc`: `compile failed: duplicate definition of @m.Collection.conj.$Vec.t0_.str`, while `fibref` accepts the program and the user's `conj` wins (result 0): a divergence in both directions (A6, [R] A10 b3) | small | **T0**: tranche 1's `Collection`, `Seq` and `Indexable` collide with the prelude's until it lands |
| B4 | polymorphic recursion: `fibc` never finishes monomorphising, `fibref` runs | `(defun len (xs: c) :where ((Reducible c e)) -> i64 (if (empty? xs) 0 (+ 1 (len (drop 1 xs)))))` and `(len [1 2 3])`: `fibc` is `memory allocation of 577136 bytes failed` under `ulimit -v 4000000`, `fibref` is `result: 3` ([R] A10 poly). Both tools must reject a function that calls itself at a strictly larger type, with a message that names it (`len recurses at (Dropped c e): polymorphic recursion is not supported; use loop or a List`) | small | T2 (before `rest` ships; without a bound the monomorphiser takes the machine down) |
| B5 | `fibc` lowers a struct constructor passed as a function value; `fibref` already runs it | `(map Pair xs ys)`-style use of a constructor as a value: `fibc` says `unsupported: a constructor as a value` and `fibref` returns 2 ([R] A11 t86); `->Name` and `(map ->Point xs ys)` need it | small | T2 (`->Name`, `vector` as a function value) |

### 7.2 Language and expander (small to medium each; L16 and L19 are Decided, L20 to L31 are the changes the rule requires)

| # | Change | Evidence | Needed by |
|---|---|---|---|
| L1 | **Arity overloading, in Clojure's shape**: `(defn name ([a: T] -> R body) ([a: T b: T] -> R body))`, resolved by argument count in the expander; each clause has its **own** `-> R`, `:where` and `:private`, because `get` returns `(Option v)` in one arity and `v` in the other. Clojure's parameter vector `[x y]` tells a clause `([x] ..)` from a single-arity `[x] body`, so **no `:arity` marker is needed** for `defn` (the first version's marker was for `defun`'s parenthesised parameters, whose clause form collides with L7's patterns; `defun` keeps one clause); two clauses of one count are an error; a protocol method and clauses of other arities may share a name; **no clause is a constructor's** (`some` is `Option`'s constructor of one argument and Clojure's `some` of two, and it is the Rust macro of E1 that tells them apart, §8.2.1 item 3); `(gensym)` is a zero-argument clause; **scope and value (X9c, stage 2 only)**: a call whose bare head is bound by an enclosing parameter (`fn`, `defun`, an impl method), `let` or `loop` binding (in force after its initialiser) or `match` pattern is the call of that local and is not picked, so `(let ((get (nth c 1))) (get))` is the local's (cases 1900 to 1903, ownership 05); a **bare overloaded name that is not a call's head denotes its clause of the fewest parameters**, `(let ((r range)) (r 2))` is `range$1` (case 1904, 874), because a clause set has no one function type in a typed language and a closure that dispatches on the count would need `(fn [& args])`, which is not offered; when a protocol method of that name has fewer parameters than every clause the bare name is the method; a local of the name is always the local; a clause is also named explicitly, `range$3`. The Rust seed has neither rule (overload.rs header), so these cases are marked `;; stage: 2` | two `defun`s of one name: `f is already defined`; a protocol method and a `defun` of one name: `gg is already defined` (A6); `(get m 5 0)` is `get takes 2 argument(s), got 3` (§7 D1 makes the message say `use get-or`); a `defun some` of two parameters makes `(some 5)` `some takes 2 argument(s), got 1` ([R] A11 n13), and in a module that `:use`s it every pattern `(some x)` is `some is not a variant or struct` (tranche 2 plan C2-5, probe P17); `(gensym)` is `gensym takes 1 argument(s), got 0` (A11 gensym); `defn` with bracket parameters is a macro over `defun` that runs (A11 e10: 13 under both tools) | T2 (`get/3 nth/3 reduce/2 range/n sort/2 sort-by/3 map/3 subs/2 index-of/2 partition/n repeat/2 gensym/0` (`some/2` is E1's Rust macro)); T1 uses the stand-ins of §2.3, deleted when L1 lands. Moving L1 into T1 for two names was considered and rejected (§10) |
| L2 | rest parameters `& xs`, as Clojure's, in `defn`, `fn` and `defmacro`, binding a `(Vec t)` (`...` stays accepted in `defmacro` until the macros of `lib/` and `compiler/` migrate); settle that `&` alone is reserved first (the pattern `[a & r]` keeps it) | `(defun f (a: i64 & xs) ..)` compiles as a three-parameter function and `(f 1 2 3)` returns 1 (A6): a spec/code disagreement with syntax §1.1 | T3 (user variadics); the library's variadics are macros |
| L3 | `Pair`, `Triple` in the prelude (done in the prototype) | `(. (Pair 1 2) fst)` is `unbound name Pair`; a user-defined generic struct works (A6) | T1 |
| L3b | tuple-like structs (`Pair Triple`) accept vector patterns `[k v]` | `(match (Pair 1 2) ([a b] ..))` is `cannot unify (Pair i64 i64) with (Vec a)` (A6) | T2 (`(fn [[k v]] ..)`, `for [[k v] m]`) |
| L4 | **static protocol methods** (no `self`, dispatched on the type the context expects, as Rust's `Default::default()`) | `(defprotocol Dflt (dflt () -> Self))` is `a method is (name (self qual* x: T qual*) -> type)` (A6); the `Unit` witness works meanwhile (A7) | T3 (`assoc-in` creating levels, generic `sum` of an empty source, `(+)`) |
| L5 | none: `let` does not generalise (types §2.4); the library builds an `Xf` per use or uses the factory form of §2.1 | `Xf` serves two accumulator types (A8) |  |
| L6 | or-patterns `(or p q)`, alternatives binding the same names at the same types | `(match [1 2] ((or [a] [a b]) a) ..)` is `or is not a variant or struct` (A6) | T3 (`case`, `condp`) |
| L7 | irrefutable patterns in `fn`, `defun`, `loop` parameters, as sugar for a `let` in the body | `a fn parameter is sym or sym: type` (A6) | T2 |
| L8 | a refutable `let` pattern traps `let: pattern does not match`; **a vector pattern in a binding position takes a prefix** (binds the first n, ignores the rest, traps if there are fewer), as Clojure's; `let-else`; `match` keeps exact shapes | `a let pattern must be irrefutable` (A6) | T2 |
| L9 | named-field struct patterns `(Name :field p ..)` and `{:keys [a b] :or {a 1}}`, `:strs`, `:syms`: on a struct they bind fields, on a `(Map keyword v)` they bind `(get m :a)` (an `Option`, or `(unwrap-or ..)` for a key with `:or`) | `(match 1 ({:keys [a]} a) (_ 0))` is `braces are not allowed in patterns` (A6) | T3 |
| L10 | (**landed for add, subtract, multiply, negate and the `wrapping` scope macro**; `-inc` and `-dec` are not) wrapping integer builtins `unchecked-add -subtract -multiply -negate -inc -dec` at the operand's width; `lIR`'s `add sub mul` already carry no overflow flags | `(+ 9223372036854775807 1)` is `trap: integer overflow in + at i64` (A6) | T3 (the multiplicative hash finaliser, generators). **Not T1**: the rotate-and-xor `hash-combine` of tranche 1 needs only `shl shr bit-or bit-xor`, which do not trap ([R] A10 hash) |
| L11 | exact math as builtins (`sqrt floor ceil rint copysign`); `extern` accepted by `fibref` | `(unsafe (fptosi i64 (sqrt 49.0)))` with `(extern sqrt :private (f64) -> f64)`: `fibc` prints 7, `fibref` says `unsupported: extern sqrt is not available in the reference interpreter` (A6) | T4 |
| L12 | `Result` in the prelude, and a scope-exit hook (a `Drop`-like protocol) for non-memory cleanup; **not `try!`**: there is no early return, so the threading form is the block macro `try-let` (§2.10), which needs no language change | `Result` was `compiler/util/result.fib` only (deleted by M2); `(defmacro try! (r) `(match ,r ((Ok v) v) ((Err e) (return (Err e)))))` is `unbound name return`, and `try-let` runs, 106 under both tools ([R] A10 try) | T1 (`Result`), T2 (`try-let`), T5 (`with-open`, `line-seq`) |
| L13 | **withdrawn.** The second revision's affine recipes (a recipe consumed once, a second use a compile error, a `FnOnce` closure kind): the owner decided Q34 for Clojure's memoised seqs (§9.1), and the closure kind the check needed is excluded by types §1.4 (**Decided**). The fused loop is E16, an optimisation with no checker rule | A12 aff1, aff2, aff3, q34; A13 ev1, ev2 (the residual claim was wrong by the sequence length) | none |
| L14 | a keyword that unifies with `(fn (S) T)` elaborates to `(fn (x) (. x k))` when `S` is a struct with that field and to `get` when `S` is a `(Map keyword v)`; `(:k x)` in head position is the same rule; `(:k x d)` supplies a default. L21 generalises it to `Map`, `Set` and `Vec`. The narrow rule needs no choice of call-position semantics; the alternative is a reader form `.name` for `(fn (x) (. x name))` | `(:a {:a 1})` is `cannot unify keyword with (fn (a) b)` (A6, [R] A11 k1); `(map :name ps)`, `(sort-by :age ps)`, `(group-by :dept ps)`, `(filter :active ps)` are the commonest Clojure lines, and the group-by-then-count idiom is 46 tokens against Clojure's 29 without it | T2 |
| L15 | `def` initialisers: **any expression**, evaluated once before `main` in module order by an init function per module (Clojure evaluates them at load); a top-level `atom` is allowed; **the type of a `def` may not contain a `Cell` or a `Weak`** (not `Send`: a global `Cell` is reachable from every task, §5 M1; so not an `LSeq`, a `Delay` or an `MArray` until the run-once cell of C9, §9.2 Q41); top-level forms that register (`defmethod`, `add-tap`) run in the init | `(def ok: (Set i64) (set [1 3]))` is `def ok: initialiser is not a constant expression`; so are `(def v: i64 (f 2))` and `(def counter: (Atom i64) (atom 0))` ([R] A10 def, A11 t56, n8b), while `(def v: (Vec i64) [1 2 3])` and `(def m: (Map i64 i64) {1 2})` run; `(def c: (Cell i64) (cell 0))` is the same error (n8); `#{1 2}` becomes `(hash-set ..)` with E8 | T2 (`(def stopwords #{"a" "the"})`, `(def table (zipmap ..))`), T4 (`rand`, `atom`s) |
| L16 | **Decided** (owner, 2026-10-01, by the rule). An impl's context (and a method's own variable) may name a type variable that a constraint determines from the head through a protocol's determined parameter (`(Cursable c k)` determines `k`): the liberal coverage condition, as Jones's functional dependencies give. It gives `Lookup` for `(Option s)` (a lookup through `nil`), bufferless cursors for `Mapped Dropped Taken Zipped`, and push visitors with a bound | `(impl (Cursable (Wrap k)) (Src c) :where ((Cursable c k)) ..)` is `type variable k is not a parameter of the impl head` ([R] A10 cur-impl, A11 t23, t61b); the same rule rejects an impl for the cursor of an adaptor and a push `Each` protocol whose visitor is a protocol parameter (`push1.fib`), and `a method is (name (self qual* x: T qual*) -> type)` rejects a bound on a method's own variable (`push2.fib`, A10 push); a nested head `(Option (Map k v))` is `an instance head is a type constructor applied to distinct variables` (A11 e5) | T2 (`(get (get m :a) :b)`; adaptor cursors without the buffer); C1's push chain |
| L17 | `derive Debug` and `ToStr` (and the text of every type of §2.7); **`defstruct` and `defenum` derive `Eq Ord Hash Show Debug ToStr` by default when every field has them** (settled by the rule: a Clojure record is `=`, `hash` and printable); `(derive ..)` stays for the rest and a qualifier opts out | at `36ed472` `(derive Debug P)` was `cannot derive Debug: only Eq, Ord, Hash and Show` under both tools; `(= (P 1 "x") (P 1 "x"))` is `no implementation of Eq for P` (A10 dd), still true: the default derive is open; derived `Show` prints `(P 1 x)`, and the page's text is `#m.P{:x 1, :y x}` (§2.7). **`derive Debug` and `derive ToStr` landed** (R8, `605a26e`, §2.7, syntax §3.16; the error is now `cannot derive X: only Eq, Ord, Hash, Show, Debug and ToStr`) | T1 (`Debug`), T2 (`defrecord` writes the six derives explicitly; **the default derive is deferred**, plan PC2-5 and D6, §8.2.1 item 5) |
| L18 | builtins `(str-byte-at s i)` (one byte, no allocation, traps out of range) and `(str-find s pat from)` (the first byte offset at or after `from`) in both tools. **Contracts (tranche 1, implemented):** `(str-byte-at s i) -> i8` traps `str-byte-at: index {i} out of range 0..{n}`; `(str-find s pat from) -> (Option i64)` finds an empty `pat` at `from`, traps `str-find: from {from} out of range 0..{n}` unless `0 <= from <= n` and `str-find: from {from} splits a character` inside a multi-byte character, copies nothing, and allocates only its result, one `Option` object (a heap enum, types §8.1), hit or miss (`cases/ownership` 210 to 218) | `(str-bytes s)` allocates a fresh array per call: 100 objects for 100 calls (A10 str), so `str/index-of` from an offset is O(n) copying per call and a tokenizer O(n²) | T1 (every string function of §4.7) |
| L19 | **Decided** (owner, 2026-10-01, by the rule): an integer literal whose value is exactly representable adopts a float type when it unifies with one; a variable never does through this rule. L26 extends the same mechanism to the integer widths | `(* 2 1.5)` is `cannot unify f64 with i64` (A6, [R] A11 n2); `(+ x 1)` with `x: i32` is `cannot unify i64 with i32` (n3c) | T2 (`(* 2 x)`, `(/ x 2)`, `(+ x 1)` on doubles) |
| L20 | **Truthiness, typed.** A test of `if`, `when`, `while`, `cond`, `and`, `or`, `not`, `when-not`, `if-not`, `cond->` and `some->` accepts `bool` or `(Option T)` (truthy when `true` or `(some _)`; `(Option bool)` truthy when `(some true)`, Clojure's `false`); `and` has the type of its last operand; `or` is typed by its last operand (`(or (Option T) T)` is `T`, `(or (Option T) (Option T))` is `(Option T)`, `(or bool bool)` is `bool`); a one-armed `if`/`when`/`cond`/`when-let` is `unit` for a unit body and `(Option T)` with `some` around the body otherwise; any other type as a condition is a compile error. Both tools; `and`, `or` and `when` become checker forms. **[sketch]** | `(if (get m 1) 1 2)` and `(or (get m 2) 7)` are `cannot unify (Option i64) with bool` ([R] A11 t01, t02), `(if 5 1 2)` `cannot unify i64 with bool` (t03), `(when c 5)` `cannot unify unit with i64` (t82); the library form runs: `Truthy` and macros give `if`, `or`, `and`, `when` over `bool` and `Option`, 127 under both tools (t04), a predicate with a `Truthy` result 4 (t05), a macro that wraps in `some` 12 (t83); `(if* (some false) 1 0)` is 1, so `(Option bool)` needs its own rule (e2b); one `or` for two result types overlaps, `overlapping instances: Or2 for (Option a)` (e2, t70); no blanket impl, `an instance head must not be a type variable` (t06); the elaboration is a direct call to an identity (`fibc emit`, A11 e1z); run-time cost nil. small-medium | T2 (`if-let`, `when-let`, `some`, `filter` over `Option`) |
| L21 | **Callable collections and keywords.** A `Map`, `Set`, `Vec` and keyword in call position, and where a `(fn (A) R)` is expected, elaborate to the `get` eta-expansion of §2.11; `(assoc r :k v)` and `(update r :k f)` on a struct with a literal keyword elaborate to `with`; after C1 `Fn` is a protocol and each is one `impl`. **[sketch]** | `cannot unify (Map i64 i64) with (fn (a) b)` ([R] A11 t51, n5), `(Set i64)` with `(fn (a) bool)` (n6), `keyword` (k1), `(Vec i64)` (k2); `(assoc p :x 5)` is `no implementation of Associative for P` (t53); the literal-keyword macros run, 30 31 32 (e14). medium | T2 (keyword), T3 (the collections) |
| L22 | **Arity-reading forms**: `partial`, `comp`, `complement`, `juxt`, `every-pred`, `some-fn`, `memoize`, `fnil`, `constantly` (arity from the expected type) and `apply` (a literal vector spreads; a runtime collection checks its count; a table names the variadic folds) are core forms the checker elaborates from the `(fn (A..) R)` of their function arguments; `some->` and `some->>` (a step that returns an `Option` is kept, a plain value wrapped) are the same mechanism, and so is a **`fn` literal with several arities or a rest parameter, and `%&`**: at a call site whose expected type `(fn (A B) R)` is known (`reduce`, `map`, `sort-by` comparators, `apply`) the literal keeps the clause of that arity (the rest parameter binds the `Vec` of the remaining arguments when they unify); a multi-arity function that is stored or returned has no single type and stays out (§5 T2). **[sketch]** | `(partial add3 1)` is `cannot unify (fn :send (i64 i64 i64) i64) with (fn (a b) c)`, `(comp inc add2)` `cannot unify (fn :send (i64 i64) i64) with (fn (a) i64)`, `(constantly 7)` where `(fn (i64 i64) i64)` is expected `cannot unify (fn (a) i64) with (fn (i64 i64) i64)` ([R] A11 arity..arity4); `apply` as a macro runs for a literal vector and the named folds: 6, 3, 6, `312` (e12); a library cannot tell an `Option`-returning step from a plain one (t06). medium | T3 |
| L23 | **Multi-parameter dispatch** (**owner's decision**: a type-system change): a protocol may declare several dispatch parameters, an instance is keyed by the heads of all of them, an overlap check runs on the product and ambiguity is reported (types §3.3, §4), with L16. As library code it gives `or`/`and` by operand types, `=` across families, `(into {} [[1 2]])` (a two-element `Vec` entry beside the `Pair` entry), `merge` with `nil`, `str/replace` by replacement type, `(Option bool)` in predicate position and the numeric protocols of L26. Every use has a cheaper checker rule (L20, L24) if the owner does not take it (§9 Q39) | `overlapping instances: OrHit for (Option a) is already implemented` ([R] A11 t70), `overlapping instances: Collection for (Map k v) is already implemented` (t71), `an instance head is a type constructor applied to distinct variables` (t61), `type variable k is not a parameter of the impl head` (t23, t61b). large | T3 (recommended: §9 Q39) |
| L24 | **Joins and families.** (a) At an `if` or `match` join whose arms are distinct types that are both `Reducible` of one element type (recipes, `Vec`, `List`), unify at `(dyn (Reducible e))`: C1's erasure rule extended to recipes; (b) `(= a b)` with operand types of one family (sequential, map, set) elaborates to `seq=` or its map and set twins. **[sketch]** | `(if flag (filter p v) v)` is an `(LSeq i64)` against a `(Vec i64)` (for the first version's recipe, `cannot unify (Vec i64) with (Filtered (Vec i64) i64)`, [R] A10 join); `(seq-of ..)` at both arms runs; `(= [1] (range ..))` is `cannot unify Range with (Vec i64)` (A11 n4), `seq=` runs (A10 showseq). medium | T3 |
| L25 | **Heterogeneous literals.** A map literal whose values do not unify elaborates to `(Map keyword Val)` with `to-val` on each value; a two- or three-element vector literal whose elements do not unify is a `Pair`/`Triple` (a homogeneous one stays a `Vec`), so `(into {} [[:a 1]])` and `(map vector ks vs)` follow Clojure; `Val` is `fib.data`. **[sketch]** | `[1 "a"]` and `{:a 1 :b "x"}` are `cannot unify i64 with str` ([R] A11 k8, t91, t92); `Val` with `to-val` runs, `{:name "ann", :inner {:tags ["a" "b"], :x 1}}` and a runtime `get-in-v` (e17). small-medium | T3 (literals), T5 (`Val`) |
| L26 | **Numbers.** (a) literal adoption across integer widths and `f32` (L19's mechanism); (b) operator-level promotion for variables along `i8 < i16 < i32 < i64 < f64`, `f32 < f64` at the numeric builtins and at parameters declared `f64`/`f32` or a wider integer, never narrowing and never at `let`, return or field; (b) lifts types §1.1 D3 and is settled by the rule (§9.1 Q36). A deferred constraint in the checker and in `fibref`; a generic `(defun add (a b) (+ a b))` stays `∀a. (Num a) ⇒ a a → a`. **[sketch]** | `(+ n 2.5)` is `cannot unify f64 with i64` ([R] A11 n1), `(< n 2.5)` (n10), `(f 2)` for `(f x: f64)` (n11), `(/ (reduce-sum xs) (count xs))` into an `f64` (t85), `(* x 2)` with `x: f64` (t63), `(+ x 1)` with `x: i32` (n3c); `(/ (sitofp f64 n) (sitofp f64 m))` is 25 (t84). medium | T3 (a), T4 (b, if signed) |
| L27 | **Metadata**: a `(Option (Map keyword Val))` field on the four collection headers and on structs, ignored by `Eq` and `Hash`; `^{..}` and `^:k` read as `with-meta` forms. **[sketch]** | none: no program of the review needs it; `meta` and friends are Clojure names (§4.3) | T5 |
| L28 | **Exceptions**: `throw`, `try`, `catch`, `finally` by unwinding: either a landing-pad `invoke` in lIR, or a transitively inferred "may throw" effect that makes each such function return a hidden `Result` and reuses the release code of the normal return; the first step is a task's trap returned by `try-join`. **The owner's decision** (amends types §2.11; the rule settles it, §9.1 Q35). **Stage 1 delivered 2026-10-05** (docs/design/exceptions.md 4.1, decisions-2026-10-04.md: mechanism (b) later; `try-join` for `join`'s result, `join` and `@t` trap in the joiner): a trap on a spawned task's thread completes the task as failed (`fib.fail-task`, `rt/core.lir`), `try-join` is `(Result a Trap)`, `ex-info` is plain data (`fib.ex`), `audit: abandoned` counts what the failed task owned; fatal, never isolated: out of memory, a thread that cannot start, a trap on main or a pool worker. **Not delivered:** `try`/`catch`/`finally`/`throw`, unwinding, the trap `kind`, `future-cancel`. **[sketch]** for the rest | `trap` in a spawned task: `trap: boom` then `Aborted` under `fibc`, `trapped:` under `fibref` ([R] A11 t40, e9); lir.md `(trap)` is `llvm.trap` "without unwinding"; the ownership checker already computes the live owned locals of every scope exit (types §6.3). large | T5 (after viability) |
| L29 | **Dynamic vars**: the `:dynamic` qualifier on `def` and a `binding` core form that pushes and pops at the scope exit and is conveyed into `spawn` as a captured `Send` value (runtime C11); `*out*`, `*in*`, `*print-length*`, `with-out-str`, `with-redefs` are libraries over it; `with-redefs` marks, for the whole program, each function that some `with-redefs` names, and calls it through a global `Atom` slot. **[sketch]** | `(def :dynamic *x*: i64 1)` does not exist; no program can show it | T5 |
| L30 | **`/` is `Div`'s method and `quot` the builtin** (§2.8; Q40 **Decided**, owner, 2026-10-01): `Num` loses `/` and keeps `+ - * neg quot rem`; `quot` is the builtin truncating division (today's integer `/`, and on floats the truncated quotient); `fdiv` is the float division, **a method of the builtin protocol `Float`** (instances `f32` and `f64`) that the `Div f64` and `Div f32` instances wrap, and float `quot` is libm's `trunc` of it (§2.8); `(Div a r)` is a library protocol with a determined result type and `(Ratio t)` a library struct with `Num Div Eq Ord Hash Show`, both tranche 1; the reader literal `7/2` comes with E14 (a). Types §2.12 is amended (§7.4); the compiler's one `(/ limit radix)` (`compiler/syntax/number.fib`), cases 102, 103 and 190, the generator (`crates/fibgen/src/gen/nums.rs`) and two unit tests write `quot` (§8.3). Small in the checker and the lowering, a library type | `(/ 7 2)` is `3` today ([R] A11 k9) and a generic `(half 6)` is `0` ([R] A13 quot); the `Div` protocol with instances for `i64`, `i32`, `f64` and `(Ratio t)` runs under both tools: `[7/2 2 -3/2 1/2 0.5 1/2]`, `5/2`, `[true false]`, `[3 2]`, `0.25`, `[true true true]`, `[3 3.0 1 1.5]`, `7/2` ([R] A13 div); the overflow trap at `i64` (A12 ratio) | T1 (S17 until then) |
| L31 | **Static predicates** (§4.18): `string? vector? seq? number? satisfies? ...` are checker forms that fold to the constant `true` or `false` of the argument's static type, and over `Val` and `Form` are variant tests. **[sketch]** small | `(string? "a")` is `unbound name string?` ([R] A12 pred1); no program of the review exercises the fold | T3 |
| E1 | the thirty-two Rust prelude macros of §6.3 (`str println print prn pr swap!`, the variadic folds, `{..}`, `#{..}`, `if-not`, `when-not`, `defn`, `defn-`, `some`, `update`, `list`; 28 of the 33 are in the registry, five of the thirty-two open, §6.3, §7.5), and the retargeted counting loop of `for-each`, `range` and `run!`/`doseq` over a literal range; a macro and the prelude function of one name coexist today (`swap!` over the builtin: 30 under both tools, A10 swap) | `+ takes 2 argument(s), got 3`, `unbound name str`, `println 5` is `cannot unify i64 with str` (A6); the fused loop of `(for-each (range 0 1000) (fn (i) ..))` has no call in `main`, the library spelling has a retain and two indirect calls per iteration (A10 forloop) | T0 |
| E2 | the macro runner compiles all of a program's macros in one module (or runs small ones in the evaluator); calls to functions of required **and used** modules work at expansion time (the macro-time module is the transitive closure of both), **the macro module sees the implicit library as every module does** (found by the flip, §8.3: a macro's body that calls `count`, `nth`, `first` or `map` is `unbound name` at expansion time, so until this lands a library macro's body uses prelude names and builtins only, `vec-count`, `vec-nth`; plan D3, §8.2.1 item 10), and the two tools say the same thing when they do not. **Unproven**: syntax §3.16 says `(var m/f)` works, and no program of the review made it work | 1, 5, 20 macros: 0.20, 0.50, 1.70 s under `fibc` (A9). A macro that calls `(u/bump 3)`, or `((var u/bump) 3)`, of a required module is `macro twice failed: f4.fib:2:46: unbound name u/bump`/`var: no definition named u/bump` under both tools, and with `(:use mu)` and a bare `(bump 3)` `fibc` says `unbound name bump` while `fibref` says `macro twice calls bump, which is not available at expansion time; move bump to a required module`, although the module IS required ([R] A10 mh) | T2 |
| E3 | bracket binding forms accepted beside the parenthesised: `let loop fn doseq for if-let when-let dotimes` | `(let ([a 1 b 2]) ..)` is `malformed let: a binding is (pattern expression) or (name: type expression)` (A6) | T2 |
| E4 | Clojure's flat `cond` replaces `(cond (t e) ..)`; six call sites in three files of `compiler/` are migrated (`jit-demo.fib`, `syntax/lexer.fib` x3, `lair/call.fib` x2) | `(cond (= 1 2) 5 true 6)` is `malformed cond: a clause is (test body+)` (A6); a clause and a flat test cannot be told apart, so it is a replacement | T2 |
| E5 | the expander rejects duplicate literal keys in `{..}` and `#{..}` (Clojure's reader does; a computed duplicate: the last wins) | `(count {1 2 1 3})` is 1 (A6) | T3 |
| E6 | freeing a linked object is iterative: dropping a long `List` must not recurse (small, runtime) | a 300,000-cell `List` overflows the stack under `fibc run` (`thread 'main' has overflowed its stack`), 50,000 cells are fine (A6); an executable exits 139 (design record) | T2 (`List` is the recursion type) |
| E7 | a library root list (`-I DIR`, `FIB_LIB`, embedded roots) and `(:export-from ..)` | modules load from the main file's directory only (A9) | **T0** |
| E8 | reader macros `#(..)` and `#{..}` reading as `(fn (%1 ..) ..)` and `(hash-set ..)`; the M6 reader and its dump are extended in the same commit (E14 adds the rest of Clojure's reader) | `unknown reader syntax #(: only #_ is defined`, `#{` likewise (A6) | T2 |
| E9 | an on-disk cache of each module's checked interface; demand-driven body checking | §6.4: hypotheses | after 20,000 library lines |
| E10 | hazards found: `Hash f64` and `-0.0`/NaN; **`derive Hash` and the prelude's `Hash (List a)` trap on overflow** (`h*31 + hash x`: replaced by `hash-combine`, §2.7); the `List` variants `empty`, `cons` renamed `Empty`, `Cons`, and the `list` macro expands to qualified names | `(get (assoc (map-empty) 0.0 1) -0.0)` is `nil` while `(= 0.0 -0.0)` (A6); `(hash (P2 "hello" "world"))` of a `derive Hash` struct of two strings and `(hash (list "a" "b" "c"))` are `trap: integer overflow in * at i64` under both tools (A10 hash); `(defun empty ..)` then `(list 1 2)` is `cannot unify (fn :send ((Vec i64)) bool) with (List i64)` (A6, A10 list) | **T0** (`Hash f64`, `derive Hash`, `Hash (List a)`), T2 (`List`) |
| E11 | a free symbol in a macro template resolves in the macro's **defining module** (Clojure's syntax-quote qualification), not at the use site; binders stay unrenamed (`gensym` covers them, syntax §3.16 declines hygiene), or at least the `fib.prelude/` treatment extends to every implicit `fib.*` module | a library macro `` `(first ,v) `` gives 1000, the user's `first`, from a module that defines one, and `no implementation of Seq` from one that does not (A10 hyg); the library's fibber macros (`for get-in case update-in`) and the Rust macros that expand to a `fib.coll` method (`conj assoc dissoc merge`) change meaning when the user writes `(defun filter ..)` or `(defun get ..)` | T2 (the first fibber macro that calls another function); one package with E2 and `x#`, first in the Rust queue after the syntax packages (plan PC2-10, §8.2.1 item 10) |
| E12 | two `:use`d modules that export one name: an error when the name is referenced unqualified, as `spec/syntax.md` §5 says; the code takes the first `:use` silently | `(:use y.m1 y.m2)` then `(peek 3)` is 3, `(:use y.m2 y.m1)` is 4, under both tools (A10 use). A spec rule and the code disagree; **reported, not changed** (§9 Q27) | T0 |
| E13 | `if-let` expands to `(match e ((some p) a) (_ b))` so that a refutable pattern falls to the else (§2.4) | `(if-let ([a b] o) ..)` is `non-exhaustive match: missing (some [])` (A10 iflet); the same `match` written by hand returns 9 under both tools | T1 |
| E14 | **Clojure's reader**: `~x` and `~@x` with the comma as whitespace (`[1,2]` is `[1 2]`); `x#` auto-gensym in a quasiquote; `#'x`; `#"re"`; radix literals `2r1010 36rZZ`; `1N 1M 1/2`; `##Inf ##-Inf ##NaN` (desugared to `f64-inf` and `f64-nan`); `#?(:fib x :default y)`; `#tag form`; `^T x`, `^{..}`, `^:k`; `::kw`. The M6 reader and its dump change in the same commit, and **every `,x` of `lib/`, `compiler/` and the cases migrates to `~x`** (§9 Q38) | `unknown reader syntax #(: only #_ is defined` (A6); syntax §1.1: a comma touching a form is the unquote, so `[1,2]` unquotes the 2; the 218 reader tests of `compiler/tests/reader/` pin today's reads. small each, large churn | T2 (E14a: `~x`, `~@x`, the comma as whitespace, the ratio literal `7/2`, with E8; `x#` moves to E2 and E11, which are one package, and `#"re"` to tranche 4: plan PC2-4, §8.2.1 item 4), T3 (E14b: `#'x`, `^T`, `::kw`, `##Inf`, radix literals), T5 (E14c: `1N 1M`, `#?`, `#tag`, `^{..}`) |
| E15 | **Modules and names**: the Java class names `Math Long Integer Double Character System Thread` are implicit aliases of modules of that name; `ns` accepts `:refer`, `:only`, `:exclude` and `:rename` and a top-level `(require ..)`, `(use ..)`, `(alias ..)`, `(refer-clojure ..)` is hoisted into the `ns`; `declare` is a no-op | a module named `Long` gives `Long/MAX_VALUE` and `(Long/bitCount 255)` verbatim, `9223372036854775807` and 8 ([R] A11 e19); `ns` takes `:require` and `:use` only (syntax §5). small | T4 |
| E16 | **The fusion rewrite** (§2.1 rule 2; the owner's requirement under Q34): after macro expansion, in a call of a terminal consumer (the set **T**) the collection argument is rewritten so that every nested call of a sequence function (the set **A**) whose head resolves to the library's becomes its recipe form; in every other position a sequence function is its lazy form, an `LSeq`. Sound by construction (a recipe occurs once, as an argument, and a consumer in T walks it once), no run-time cost, one pass over resolved forms; stage 2 mirrors it (§6.6); the sets are listed in §2.1 rule 2, and **as implemented** (restricted sets, source-first recipes, stages with effectful arguments kept lazy) in the paragraph after it; **landed**, R9, `605a26e` | the two macros `fuse` and `fuse-arg` do it to one form: `[8 2]`, `[0 2]` with 2 calls of `f` over an infinite source, `[0 0 3]` for a bound seq read by two chains ([R] A13 lz1); +5 objects for the fused chain of A1 against +17526 for the same chain bound to a name ([R] A13 lz0, lz2, lz3) | T1 |
| D1 | diagnostics: a type other than `bool` or `(Option T)` where a condition is wanted says `a value of type i64 is always true; write the test`; a transducer where a function is wanted says `compose with xf`; a call of a library function with the argument count of its Clojure clause that the stand-in serves says `use get-or` (`get` with 3 arguments), `use nth-or` (`nth` with 3), `use range-by` (`range` with 3) or `use reduce1` (`reduce` with 2: **one argument too few**, not too many, for the library's three-argument `reduce`) (until L1; `sort` with two arguments is no error since Y11b, so it has no hint); the hint is keyed on the count the call has, only for a function of the prelude or of a module whose `ns` starts `fib.`, and a program's own function of that name gets none (**landed**, R7, `605a26e`, cases 803 to 808; the `reduce` hint is reachable only through a qualified head such as `fib.seq/reduce`, because the `reduce` macro serves `(reduce f c)`; to let the hint see `range` with three arguments the `range` macro now declines it, syntax §4.4); the condition message is **landed** for `i8 i16 i32 i64 f32 f64 char keyword str` (`when`, `unless`, `while`, `cond`, `and`, `or` expand to `if` and say it too; an `Option` or a `Vec` keeps `cannot unify .. with bool` until L20, case 807); the transducer message `compose with xf` is **not done** (there is no `Xf` yet) | `cannot unify i64 with bool` (A11 t03), `cannot unify (Xf i64 i64) with (fn (a) b)` (A10 comp), `get takes 2 argument(s), got 3` | T1 |
| H1 | the case harness reads an object count: a header `allocs: <= N` (a maximum), checked against the `A` lines of `fibc run --trace` (and `fibc itrace`, which gives the same count) | a case header is `spec expect result audit error trap`: no count (§2.5); `fibc run --trace` and `fibc itrace` print identical `A` counts, 2099 for the chain of A1 | **T0** (the count cases of §8.1 item 5 cannot be written without it); **landed** (commit `8adeaa6`, cases 202 to 204): `spec/method.md` rule 3 defines the `allocs: <= N` header, both runners count the `A` lines (the interpreter's trace and the compiled run's `FIB_TRACE=1` trace, which are the same lines), a bound that is exceeded fails and so does a runner that gives no count, and the tests of both `allocs.rs` lower each `NNN-count-*.fib` bound by one and require the failure (a bound of 0 no longer underflows, commit `179cb18`) |

### 7.3 Compiler performance and runtime (medium each; the gates of P2 and P3, and of the rule's items)

| # | Change | Evidence | Size | Gates |
|---|---|---|---|---|
| C6 | `fibc build` optimises at level 2 by default; `fibc run` stays at level 0 with a flag for 2 | `JitOptions::default()` and `Options::default()` are level 0 (read in `crates/lair/src/jit/mod.rs`, `aot.rs`, `fibc/main.rs`); `-O 2` leaves five indirect call sites on the chain (A1); **the change is not tiny**: a main of 17,256 lIR lines takes 0.40 s at `-O 0` and 1.00 s at `-O 2` under `lair run` (A10 timing), so a default of 2 for `run` multiplies the cost of every run by 2.5 | small for `build`, medium with the instance cache of §6.4 | C1's payoff, at build time |
| C2 | **a last use moves**: when a variable's last use on every path is the argument of a call or a `cell`/constructor, `consume` moves instead of retain-then-release; loop variables move into the call before `recur` rebinds | `bump` called in a loop: 1001 arrays for 1000 calls; `explain`: `arg 1 acc: retain` (A4) | medium | P3: in-place `conj`/`assoc`/`update` with no API change |
| C3 | **exclusive `&`**: when `&x` is the only mention of `x` and `x` is not captured, copy-in shares the object without acquiring | `(push! &v i)` 1000 times: 2094 objects, `&` copy-in acquires (A4) | small-medium | `push!`, `map-put!`, builders |
| C4 | array primitives for the library: `(array-push! &a x)` with capacity, `(array-update! &a i f)` that moves the slot out and stores the result, an uninitialised `(Array a)` | `array` needs an element, so no generic empty array exists and the prelude has `VecEmpty` (prelude comment) | small-medium | the in-place trie and tail |
| C1 | **closure types**: a `fn` literal passed to a parameter of a type variable bounded by the built-in `Fn` is a nominal type whose captures are fields, a monomorphisation key; `(fn (A) R)` stays the dynamic type; **an erasure rule at joins** (two closure types, or two adaptors holding them, meeting in an `if` or `match` unify at the erased `(fn (A) R)` of the parameter; §2.2) | functor emulation of a **pull** chain: 0 indirect calls, 0 retains per element, and `-O 2` leaves none on the chain (A2), at the price of 1500 calls of `f` for 1000 elements (A10 d1c); the **push** form cannot be declared today (A10 push) and needs B1 and L16; the impl head on `(fn ..)` is rejected as it stands (A6); with a type per closure `(if flag (map2 (Inc ..) v) (map2 (Dec ..) v))` is `cannot unify (Mapped (Vec i64) Dec) with (Mapped (Vec i64) Inc)` (A10 c1emul) | medium to large (with the erasure rule and L16) | P2 at Rust speed; `sequence`/`eduction` |
| C5 | unboxed `(Option scalar)` and small enums and structs by value (lIR already has by-value aggregates). **The first half has landed in stage 2** (performance batch 4, lever A; types §8.1, docs/design/unboxed-option.md; `cases/ownership` 267 to 270): an `(Option scalar)` is a pair held by value, so `get` on a `Vec` or a `Map`, `peek` and `str-find` on scalar elements allocate no `Option` (measured by 267 and 270; the other `Option`-returning library functions are the same representation, not measured); `(Step i64)`, `Pair` and the other small enums and structs are still heap objects | 100 calls returning `(Option i64)` and `(Step i64)`: 200 allocations (A5); `get` on a `(Map i64 i64)` allocates per lookup; `(first v)` in a loop of 1000 is +1000 objects, `zip` +1003, `keep` +1002 (A10 alloc) | medium-large | `get`, `first`, `reduce-while`, `Step`, `zip`, `keep`; scheduled with tranche 3, whose count cases for these stay `open` until it lands |
| C7 | a `match` or field read on an owned shell that is dead afterwards **moves the payload out**: no retain of the field, no release of it with the shell | `bump` that matches an owned `(E1 n arr)` and writes `arr` through a cell: four chained calls on unique temporaries cost 10 objects (A10 own); a `self :owned` method on a struct with an array field, 1000 times in a loop: 2002 objects, `explain`: `(cell ..) arg 1 (. self a): retain`, `release [self] (exit)`; a struct held in a cell updates in place: 1000 `set-field! &c n ..` allocate nothing | medium | P3 for the existing `Vec` and `Map` (C2 to C4 fix only the caller's side, §2.5); the alternative is the struct `Vec` of §9 Q18 |
| C8 | **one closure environment for a group of mutually recursive local functions** (`letfn`); no cycle is needed, as a local recursive closure already calls itself through its code pointer (ownership.md §6) | two local functions that reach each other through two cells run and `audit: clean=false leak-cycles=4 leaks=0 errors=0` under `fibref` ([R] A11 t55): a leak, not a safety failure | medium | `letfn` (T3) |
| C9 | **a runtime mutex** `(Mutex a)` that owns its value, the identity-keyed reentrant monitor table that `locking` needs (an entry made on the first lock and dropped when the last holder leaves), and a run-once `LSeq`/`Delay` that crosses tasks | two tasks writing one `Cell` is rejected, `cell cannot be shared between threads` ([R] A11 t41); the `Cell` memo cannot cross (e8d), the `Atom` memo can but may run a thunk twice under a race (e8e) | medium | `locking` (T5); the run-once cell for a global `LSeq` or `Delay` (§5 M1), recommended for T3 (§9.2 Q41) |
| C10 | **an ASCII flag in the `str` header**, computed in the UTF-8 validation pass that already scans every constructed `str`: `count`, `nth`, `subs` and `index-of` by character offset are O(1) for ASCII text and a byte walk otherwise; a layout change in both tools **[sketch]** | validation happens at construction: `str-from-bytes: invalid UTF-8`, `str-slice [0, 1) splits a character` ([R] A11 t13, t12); a user `Reducible char` over `str` gives the semantics today (e3a, e3b) at O(n) | medium | the cost of Clojure's string unit (§9.1 Q37) |
| C11 | **a per-task binding slot** in the task header, copied into a spawned task's header from a `Send` capture | none: depends on L29 | medium | `binding`, `*out*` (T5) |
| C12 | **the text of a float is Clojure's** (`Double.toString`: positional for `1e-3 <= \|x\| < 1e7` with at least one digit after the point, otherwise `d.dddE<exp>` with no plus sign, the shortest digits that read back, `NaN`, `Infinity`, `-Infinity`, `-0.0`; **Decided**, owner, 2026-10-01, §2.7) in both tools: `float_text` in `crates/fibref/src/eval/arith.rs` and `fib.show-fp` in `rt/str.lir`; the reader dump prints floats through `show` (the `flt` line of spec/bootstrap.md §2), so the dump and the reader tests that pin float dumps change with it; cases 169, 178 and 187 pin the old texts; the printer's Rust `{:?}` layout is its own and stays | `(show 1e21)` is `1000000000000000000000.0`, `(show 1e7)` `10000000.0`, `(show (/ 1.0 0.0))` `inf` ([R] A13 fl); a library function over `show` gives the fifteen Clojure texts ([R] A13 fltfmt), which is the specification both runtimes implement | small | T0: the text of a float is observable by everything (§5 S18) |

An alternative to C1 that was proposed, specialising a callee on a literal closure passed to a
non-escaping `:borrow` parameter, makes the visitors of `each-while` direct but not a closure stored
in an adaptor struct, so it does not reach the chain's user functions; it is not adopted (§9 Q17).

**What is not needed.** A transient API (the Clojure names are identity wrappers, §4.4); a Perceus-style reuse of matched shells (large; considered only
if C2 to C4 leave the `Vec` header as the measured bottleneck); a rank-2 type; a second sequence protocol (the `Seqable` of §2.1 is the closed-rest form of the one that exists).

### 7.4 Edits to the other specs (tasks, not made here)

method.md says a decision is recorded in the spec chapter it affects, with the owner's sign-off. This page changes none of the other specs (the repository's
other chapters are updated when each item is implemented); each item below lists what that edit is, so that it is not forgotten, and the owner's four decisions
of 2026-10-01 are first (§9.1).

| Item | Edit | Where |
|---|---|---|
| L19 (**Decided**) | an integer literal whose value is exactly representable adopts a float type when it unifies with one; the text "no literal polymorphism ... `(+ x 1)` pins" and the type-error example `(+ (i32 1) 2)` change, and L26 a extends it to the integer widths | types §1.1, §2.12; syntax §1.2 (literals); the cases that pin `cannot unify f64 with i64` |
| L16 (**Decided**) | an impl's context and a method's own variable may name a type variable that a constraint determines from the head (the liberal coverage condition) | types §3.3 (Paterson condition), §4.1; syntax protocol declarations |
| Q15 (**Decided**) | the integer hash becomes a 64-bit finaliser in one commit with the cases that pin its values (169, 178, 198 to 201) and the orders of maps of more than 8 entries; wrapping builtins (L10); `fibc build` at `-O 2` and `fibc run` at `-O 0` (C6) | types §2.12 (the `hash` of an integer is its value); compiler.md (build options); ROADMAP M7 rule 4 |
| Q28 (**Decided**) | `first` of a `List` returns `(Option e)`: cases 01 and 61 are ported | cases 01, 61; the prelude; ROADMAP where case 01 is listed |
| L30, Q40 (**Decided**) | `Num` loses `/` (it keeps `+ - * neg quot rem`), `quot` is the builtin that rounds toward zero (on floats the truncated quotient), `/` is the library's `Div`, integer `/` a `(Ratio t)`: amends "Integer arithmetic has Rust's semantics ... `/` rounds toward zero" (Decided 2026-09-27) | types §2.12; cases 102, 103, 190; `compiler/syntax/number.fib`; `crates/fibgen/src/gen/nums.rs`; two `fibref` unit tests |
| L26 b, Q36 (§5 D2) | mixed numeric operands promote at the operator | types §1.1 D3, §2.12 ("never a promotion") |
| L20, L21, L22 | a condition accepts `(Option T)`; `Map`, `Set`, `Vec`, keyword callable; arity-reading forms; multi-arity literals at an expected type | syntax §3.4 (`if`), §4.3; types §1.4, §2.4 |
| Q35, L28 (§5 D1) | `throw`, `try`, `catch`, `finally` | types §2.11 (abort-only, **Decided** 2026-09-28), syntax |
| Q42 (**Decided**, owner-invited) | strings order by code point: the builtin instance stays; types §2.12 may say so in words | types §2.12 (`Ord` for `str`): no change of behaviour |
| L2, E14 | `& xs` as the rest marker; Clojure's reader (`~x`, the comma as whitespace, `#'x`, `^T`, ...) | syntax §1.1, §1.2; spec/bootstrap.md §2 (the dump); the 218 pinned reader tests |
| Q34 (**Decided**) | sequence functions return memoised lazy seqs; the fusion rewrite E16 is an expander pass; no other chapter changes (`LSeq` is a library type), and ROADMAP M7 may note that the zero-cost path is an optimisation | ROADMAP M7 (a note) |
| C12, Q43 (**Decided**) | the text of a float is Clojure's `Double.toString`: amends "written positionally (never with an exponent, however large or small) ... `NaN`, `inf` and `-inf`" (Decided, owner, 2026-09-30) | types §2.12 (`show` of a float); `crates/fibref/src/eval/arith.rs` (`float_text`); `rt/str.lir` (`fib.show-fp`); cases 169, 178, 187; spec/bootstrap.md §2 (the `flt` dump line) and the reader tests that pin float dumps |
| C1 (the erasure rule), L21, L24 (a), L25 | each coerces where types §1.7 says a `(dyn P)` "is produced only by the explicit primitive `(dyn P e)` (**Decided**, D3: no subtyping, no coercion anywhere in the type system)": two arms unified at `(dyn (Reducible e))`, two closure types at `(fn (A) R)`, a value wrapped by `to-val`, a collection eta-expanded to a function; the owner signs each, and the T-rows of §5 stand until then (§1.1) | types §1.7, §1.1 (D3) |
| C1 | a closure gets a nominal type whose captures are fields, where types §1.4 says "Closure types carry no capture list, no escape summary and no count kinds (**Decided**)"; the owner signs | types §1.4, §6.4 |

**Edits made so far** (2026-10-02; each is marked **Proposed** where the owner has not signed): L30's `quot` and `Float` in types §2.12, §8.12 and syntax §4.3 (R3; the float details **Proposed**); `@t` of a `Task` in types §1.6, §2.9, §3.4, §3.7, §6.2, §8.6 and
syntax §1.2, §3.11 (R13, **Proposed**); the order of `bool`, `char`, keywords and the equality of `unit` in types §2.12 (**Proposed**; stage-1 fix `179cb18`); the iterative drop and share-marking in types §8.2 and compiler.md §3; the layout class `box`
in types §4.3 and compiler.md §7; module-qualified type mangling in compiler.md §2 (**Proposed**); `FIB_TRACE` and `--trace` in compiler.md §1; the implicit modules in syntax §5 and bootstrap §5.1 (R1, **Proposed**); the
hygiene, the list of prelude macros, `derive Debug` and `ToStr`, the `List` variants, the literal rewrite and `Pair` in syntax §1.4, §3.9, §3.16, §3.19, §4.4, §4.5; the fusion modules in bootstrap §5.3. Not made: L19 and L16 in types §1.1, §2.12, §3.3, §4.1, and
every other row above.

### 7.5 Status after tranche 0 and the Rust half of tranche 1 (2026-10-02)

**What "landed" means.** The code is committed, and a case or a Rust test that can fail judges it (method.md rule 3). **What was run to write this section**,
on the clean working tree at `605a26e` (the interpreter's side only, `target/debug/fibref` built after that commit): `fibref cases cases/ownership` is 232 cases,
232 pass, 0 fail, 0 pending; `fibref cases cases/modules` is 22 cases, 22 pass; `fibref cases cases/stdlib` is 505 cases, 479 pass, 0 fail, 0 pending, 0 header
errors, 26 open (the `open-` cases of §7.5.3); and `fibc cases cases/stdlib --only 75 76 77 78 80 81 82 83`, the blocks of R5 to R9, is 71 cases, 71 pass, 0 fail. The compiled run of
the rest of the three suites, the Rust suites (`cargo test`, `cargo fmt --check`, `cargo clippy`) and `bootstrap_expand` were **not run** in this session, which
edited the specs only, and the commit messages that say they were carry no weight (CLAUDE.md). The commits: `8adeaa6` tranche 0 and the expander groundwork,
`dfdd4ba` C6 and C12, `5ecab83` the judge and the library skeleton (P0), `3d63d00` R3 and R4, `5ea989f` R1, R2 and the plan's L4, `4485838` wave 1 of the library
(fifteen packages), `179cb18` stage-1 fixes, `605a26e` R5 to R9, R13 and the follow-ups of the library packages.

#### 7.5.1 Items, by status

| Item | Status | Commit | Judged by |
|---|---|---|---|
| B2 quasiquote heads qualified, `` `() `` under `fibc` | **landed** | `8adeaa6` | `cases/modules` 012 |
| B3 protocol symbols carry the defining module | **landed** | `8adeaa6` | `cases/modules` 007, 008 |
| B4 polymorphic recursion rejected by both tools | **landed** | `8adeaa6` | `cases/ownership` 196, 197 |
| E7 library roots (`-I`, `FIB_LIB`, the embedded `lib/`), `:export-from` | **landed** | `8adeaa6` | `cases/modules` 013 to 016 |
| E10 hazards | **landed** in two steps: `hash-combine` never traps, `derive Hash` and `Hash (List a)` use it, `f64` hashes `-0.0` as `0.0` | `8adeaa6`; `5ea989f` | `cases/ownership` 198 to 201; the `List` variants are `Empty` and `Cons`, `list`, `[..]` and `{..}` are qualified, `cases/modules` 020 to 024, `cases/ownership` 205 |
| E12 two `:use`s exporting one name | **landed**: a bare use is an error | `8adeaa6` | `cases/modules` 009 to 011 |
| H1 `allocs: <= N` | **landed** | `8adeaa6`, `179cb18` (a bound of 0) | `cases/ownership` 202 to 204 |
| C6 `fibc build` at `-O 2`, `fibc run` at `-O 0` | **landed** | `dfdd4ba` | `crates/fibc/tests/cli/opt.rs` |
| C12 floats print as Clojure's | **landed** | `dfdd4ba` | `cases/ownership` 178, 187; `crates/fibc/tests/floats.rs` and `floats/java.rs` |
| E1 the Rust macros | **28 of 33 are in the registry** (27 rows added since `36ed472`, `list` existed): `str println print prn pr`, the folds, `conj assoc dissoc merge`, `swap!`, `defn defn- update`, `reduce`; **open**: `if-not when-not some` and the literals `{..}`, `#{..}` | `605a26e` | `cases/stdlib` 750 to 793; syntax §4.4 |
| E6 freeing is iterative (and `fib.share`) | **landed** | `179cb18` | `cases/ownership` 225 to 231 |
| L3 `Pair`, `Triple` in the prelude (the plan's package L4) | **landed**, with `Result` (L12) and `map-each`, `set-each`; `Entry` is gone | `5ea989f` | `cases/stdlib` 140 to 153; `cases/ownership` 181 |
| L12 `Result`, scope-exit hook | **partly**: `Result` landed; the hook and `try-let` open | `5ea989f` | `cases/stdlib` 142, 143 |
| L17 `derive Debug`, `ToStr`; derive by default | **partly**: `derive Debug` and `ToStr` landed, the default derive (T2) open | `605a26e` | `cases/stdlib` 810 to 817 |
| L1 arity overloading | **landed (X9)**: `defn` clauses are `defun`s `NAME$N`, the expander picks by argument count, the tables cross modules (syntax §4.4); the library's stand-ins are Y11's | X9 | `cases/stdlib` 1750 to 1760, 725 |
| L18 `str-byte-at`, `str-find` | **landed**; `str-find` allocates one `Option` object per call | `3d63d00` | `cases/ownership` 210 to 218 |
| L30 `/` as `Div`, `quot`, `Float.fdiv` | **landed**: `quot` and `Float.fdiv`; `Div` and `(Ratio t)` in the library; `/` left `Num` and the builtin with the flip (M) | `3d63d00`; `4485838` | `cases/ownership` 206 to 209; `cases/stdlib` 010, 059 to 067, 071, 073, 075, 076, 079, 873, 877 |
| E13 `if-let` takes any pattern | **landed** | `605a26e` | `cases/stdlib` 800 to 802 |
| D1 diagnostics | **partly**: the condition message landed; `compose with xf` open (no `Xf`); the five arity hints landed and were **deleted by X9** (the checker's arity error is plain again: `get takes 2 argument(s), got 3`); cases 803 to 806 and 808 stay `reject` with the plain text until Y11 gives the library the clauses, and then change to `accept` (plan PC2-13, §8.2.1 item 13) | `605a26e` | `cases/stdlib` 803 to 808 |
| E16 the fusion rewrite | **landed** (restricted sets, §2.1) | `605a26e` | `cases/stdlib` 820 to 836 |
| `@t` of a `Task` is `join` (**Proposed**, types §2.9) | **landed**; the owner's sign-off is not given | `605a26e` | `cases/ownership` 232 to 236; `cases/stdlib` 658 |
| Implicit modules (§6.2, plan R1) | **landed with an empty list** (`5ea989f`); **the list is `fib.core fib.seq fib.coll fib.print` since the flip (M)**, the prelude's protocols are deleted (§8.3 C) | `5ea989f`; the flip | `crates/fibref/tests/implicit.rs`, `lib_disjoint.rs`; `cases/stdlib` 870 to 877 |
| The judge (plan P0): `covers:` and `open:`, `--only`, `stdlib_table` | **landed**; the check that every row of the tranche is covered was `#[ignore]`d until tranche 1's gate and runs always since tranche 2's Z0 (§7.5.2 item 7) | `5ecab83` | `crates/fibref/tests/stdlib_table.rs` |
| Wave 1 of the library: numbers and `Ratio`, options, hashing and ordering, sources and consumers, recipes, lazy seqs and adaptors, sorting and grouping, collections, strings, `Pattern`, printing, aliases and the Java-named modules | **landed** under `lib/fib/` | `4485838`, `605a26e` | `cases/stdlib` |

Everything else of §7 is **open**: B1, B5, L1, L2, L3b, L4 (static protocol methods), L6 to L11, L14, L15, L16 and L19 (both **Decided** and not implemented), L20 to L29, L31, E2 to E5, E8,
E9, E11, E14, E15, C1 to C5, C7 to C11. (The package ids of `cases/stdlib/README.md`, `L1`, `L4`, `L5a`, `L11`, `L12` and the rest, name the plan's library packages and are not these items.)

The stage-1 divergences that the library work found and `179cb18` fixed are not items of §7: a generic function at a heap-enum `Option` payload (compiler.md §7, the layout
class `box`, case 220), `<` on `bool` and the order of keywords (types §2.12, case 221), a `def` that names an earlier `def` and a `def` of an `Option` scalar (case 222),
`=` on `unit` and arrays of `unit` (cases 223, 224), types of one name in two modules (compiler.md §2, **Proposed**, `cases/modules` 025), and the trace of `FIB_TRACE` and `--trace`
(compiler.md §1).

#### 7.5.2 Where this page and the code disagree (reported; neither is changed here)

1. **Resolved** (R14, the owner's rule of 2026-10-01). `(reduce str c)` and `(reduce concat c)` expanded to `(fib.seq/reduce str "" c)` and `(fib.seq/reduce concat [] c)`, and neither type-checked; they now expand to the `fn` over the macro `str` and to the empty lazy seq (syntax §4.4, cases 794, 795).
2. The row `print-str` (§4.14) is a macro that returns a `str`; the prelude's public `print-str` is a one-`str` function that writes and returns `unit` (R2). The names collide the day the macro lands. **Proposed** (plan PC2-8, §8.2.1 item 8): the prelude's function is renamed `print-raw` in the commit that lands the macro, with its callers (`lib/fib/print/io.fib`, the expander's `print.rs` and its mirror).
3. Row `pr` says tranche 2 and `pr` is implemented (R5); the `reduce` macro's `reduce-nonempty` is in the fusion set T and is no row of §4.
4. **Resolved** (R14, the owner's rule of 2026-10-01: Clojure's resolution, a module's own definitions, then its `:use`s, then the implicit library, then the prelude). A macro that did not decline hid a user function of the same name and arity (`update`, `reduce`, `str`, `print`), and a user `defmacro` named `and` or `or` captured the prelude's own expansions that name them. A prelude macro now declines in a module that defines the name itself or sees a program module export it, and every head the prelude writes is `fib.prelude/NAME`, which is the prelude's macro whatever the module defines (syntax §4.4, cases `cases/ownership` 239-244, `cases/modules` 026-028). §6.3 and §7 E11 still describe the hygiene the library's *fibber* macros would need; none of it is implemented.
5. A parameter declared `:borrow` of a non-scalar type cannot be compared, counted or hashed: `(defun eq (a: str :borrow b: str :borrow) -> bool (= a b))` is `parameter a of eq is
   declared :borrow but escapes`, because a protocol call counts as an escape, and so is `(count v)` on a `(Vec i64) :borrow` while `(str-len a)` is accepted. A closure passed to a call in tail
   position is a heap closure and cannot capture a `:borrow` parameter either, while the same shape inside an `impl` body is accepted. Library `defun`s with `:borrow` collection parameters will meet both (ownership.md).
6. `@t` of a `Task` and the float details of `quot` (a zero divisor does not trap, `-0.0`) are **Proposed** in types §2.9 and §2.12 and were implemented to be tested; nothing downstream may depend on them (method.md).
7. **Resolved** (tranche 2's Z0, 2026-10-02). `spec/method.md` rule 3 says `stdlib_table` fails a row of a delivered tranche that no case covers; the test of that check
   (`every_row_of_the_tranche_is_covered`) was `#[ignore]`d until the tranche's final gate. It was run with `--ignored` against the working tree on 2026-10-02 and passed (`1 passed`),
   and the attribute is removed, so the check runs with every `cargo test -p fibref`; `TRANCHE` stays 1 until tranche 2's gate (§8.1 item 1).
8. The `cargo test` suites need more than the 6 GB of address space that the session's memory rule allows: the evaluator and the checker each reserve a 1 GiB and a 256 MiB stack, and the cases
   that spawn threads fail with `spawn: cannot start a thread` under `ulimit -v 8000000` and pass under 16 GB or more. A harness note, not a defect of the code.
9. The oracle of the expander port carries `lib/` as of its build while the port reads the disk (bootstrap §5.7).

#### 7.5.3 The open cases

`fibref cases cases/stdlib` lists 26 cases as OPEN, each failing as its `open:` label says (a case that passes under a label fails the suite): L1 (300, 725, 906), L15 (726, 907), L20 (677, 720 to 723, 900),
L21 (901), L22 (115, 116, 902), L23 (903), L24 (904), L26 (675, 680, 905), E14 (724, 909), C7 (450), C9 (908), L29 (910), L28 (911). The label of `450` names a bound of 2000 that nobody measured (the
run allocates 7942); whoever lands C7 sets the measured count.

## 8. Implementation plan

### 8.1 How every tranche is judged

method.md applies: nothing is done until a test that can fail says so. A tranche is done when:

1. **Every function of its rows has an `accept` case** in `cases/stdlib/`, run in the interpreter
   (memory audit clean) and compiled, with equal results (method rule 6; `fibc cases cases/stdlib`).
   A case's header lists the names it covers (`;; covers: map filter take`). A test, `stdlib_table`,
   parses §4 of this page and fails if a row with `T` at or below the current tranche has no case that
   covers it, if a case covers a name that is not in the table, or if a case covers a name its code never calls: the table is
   executable. **The first of the three checks runs for the rows of tranche `TRANCHE`** (`every_row_of_the_tranche_is_covered`; tranche 1's `#[ignore]` was removed by tranche 2's Z0 and the gate of a tranche
   raises `TRANCHE`); for the tranche being written `every_row_of_the_next_tranche_is_covered` is `#[ignore]`d and lists the rows still uncovered when run with `--ignored`; the other checks
   always run, and a case covers at most 12 names. A row that is syntax or a name that is also a constructor has no token of its own: `@x` `~x` `~@x` are called by that prefix before a
   form, `#(..)` and `#{..}` by the dispatch, `:k` (the row of `(:k m)`) by a keyword in head position, `->Name` by any `->Upper` token, `print-method` by `Debug`, and `some` only by a call of two operands,
   `(some pred c)`, since `(some x)` is the constructor in nearly every case (`crates/fibref/tests/stdlib_table/syntax.rs`). A case that waits for an item of §7 says so with `;; open: L20` (cases/stdlib/README.md).
2. **Differential tests against a naive reference written in fibber** (`cases/stdlib/`, the cases named `NNN-ref-*`): every
   library function that has an obviously-correct eager version (a `Vec` loop of five lines) is run
   against it over a few hundred generated inputs (a seeded xorshift in fibber; never thousands: the
   machine has been taken down by sweeps before). The data structures get model tests: `Vec`'s trie
   against a plain array across counts 0 to 2000 (crossing the 32 and 1024 boundaries), `Map` and `Set`
   against **an association table indexed by key id** (an `(Array i64)` holding the value of the key whose id is the index, or -1 for absent; `cases/stdlib/support/tl/model.fib`: an association `Vec`
   searched by `=` was built first and measured, nine tenths of the work was its linear search and a case could afford 15 sequences) over random operation sequences with key types whose hashes are `id mod 7`
   (collision buckets), a shared-prefix hash that pushes a bucket down a chain, a chain of single-child nodes and a hash that differs only in the top chunks, sorted collections against a sorted `Vec`, with an invariant checker run after
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
   `allocs: <= N`, §7 H1, landed in tranche 0): a three-stage pipeline over n and over 10n elements
   allocates the same number of objects; a bulk `vec` of n allocates at most n/32 + c; a `conj` loop of n
   allocates at most n + c once C2 to C4 and C7 land, and at most c once `Vec` is a struct (§9 Q18); an
   `assoc` loop of n has a bound of its own, which this page does not guess (1000 `assoc`s allocate 7935
   objects today, A4, re-run by the compilability critic: every path node is a shell with the same field
   problem as `Vec`, so it follows C7). The first is required from tranche 1 and passes today (A1; with the lazy library, +5 objects for the fused chain, A13 lz2); the
   second is required once the `Vec` module's bulk builder lands (**[H]**, §2.5); the others are `open`
   cases until their changes land: a run lists each as OPEN with its failure and its items, never as pending and never as a
   pass, and fails the day one passes (method.md: pending is not pass). The gate of tranche 3 is therefore `conj` at most n + c, not c: the
   first version required a bound that its own §2.5 said the design could not meet.
6. **A mutation review** of each tranche's source, as the reader had (spec/bootstrap.md §3): mutants of
   the fibber source run against the tranche's tests, and each survivor becomes a case. **It is not a gate**
   (owner, 2026-10-02: "like UAT: it doesn't stop development"): a tranche is done when the conventional
   suite and items 1 to 5 pass; the reviews run in the background at low priority (at most one at a time
   per kind, never taking a cargo slot a development package needs), and what they find lands later as
   new cases or tests in small commits, and as defects to fix. The review ran nine
   mutants of the prototype against `t1.fib`: six were caught, **three survived** (`sort` made unstable,
   `update` ignoring the old value, `cycle` of an empty source, which loops forever). With three checks
   added (stability, `update` on a present key, `cycle` of an empty source) and three more mutants
   (`nth` ignoring its index, `sort-by-with` reversed, `compare` of two NaNs), all twelve are caught, two by
   not terminating (an inverted `Filtered` and the `Cycle`), which a harness with a timeout reports as a
   failure ([R] A10 mut).
7. **The compiler is the integration test**: stage 2 (M6) replaces its hand-written helpers with library
   calls as each tranche lands, and `cargo test -p fibc --test bootstrap` (byte-identical reader dumps)
   stays green.
8. **The rule is executable.** Each M row of §5 has a case in `cases/stdlib/` named `NNN-rule-*` whose header expects the rejection or
   the value quoted there (`;; error:` or `;; result:`), so a reason that stops being true fails a test; each S row has
   its failing program as a case in the `open` list, which passes when the item of §7 lands and the program is accepted
   (an `open` case is printed OPEN, never as pending and never as a pass, and fails the run the day it passes). The two audits of §10.4 ran programs for
   every reason (A11); those programs are the first cases.

### 8.2 Tranches

| T | Content | Needs (§7) | Test emphasis | Rows |
|---|---|---|---|---|
| 0 | the library root and re-export; the thirty-two Rust macros of §6.3; `Hash f64`; `derive Hash` and `Hash (List a)` over `hash-combine`; the quasiquote fix; the protocol-mangling fix; the duplicate-`:use` check (if the owner agrees, §9 Q27); the harness's `allocs` header; the disjointness test of the four implicit modules as a CI gate; `fibc build` at `-O 2` and `fibc run` at `-O 0` (C6, Q15); the text of floats as Clojure's (C12, Q43) | E7, E1, B2, B3, E10, E12, H1, C6, C12 | cases for each macro's expansion text; a `hash` of 14 strings and of a two-string struct that does not trap; both tools agree on B3's program | |
| 1 | **what stage 2 needs.** `Pair Triple Step Result Unit`; `unwrap unwrap-or map-opt and-then`; `min max abs inc dec mod compare` (Clojure's, built on `<`); `hash-combine hash-ordered-coll hash-unordered-coll`; `Eq Ord Hash Show Debug ToStr` for `Vec Map Set Pair Triple` (`derive Debug`; `Ord` of a `Vec` by length first); `Reducible`, `LSeq` with `lazy-seq lazy-cat` and the lazy forms of every sequence function of this tranche (the memoised seq, Q34), the fusion rewrite (E16), `Seqable` with `seq rest next` (one impl per head); the sources (`Vec List Array Option Range Map Set Chars`, `VSeq` (with `Collection` and `Show`: `conj` conses at the front, parentheses), and `str` as a `Reducible char` with `Lookup` and `Keyed`: scalar offsets, O(n) until C10, §2.9) and `map filter remove take drop take-while concat mapcat cons`; `reduce reduce1 reduce-while first last nth find-first find-map every? not-any? any? count empty? run! quot rem mod`; `(Ratio t)` and `Div` (`/` on integers is a ratio, `numerator denominator`, L30); `into vec set` (bulk builders); `sort sort-by sort-with sort-by-with reverse` (a comparator is a `Cmp`); `Truthy` and `Payload` (a predicate's result), `Cmp`, `Lookup Assoc Dissoc Keyed Collection Emptyable` with `get get-or assoc dissoc update update-or update-opt fnil contains? includes? empty keys vals key val merge group-by frequencies select-keys`; strings: `str` (`ToStr`), `Pattern` with its `str` and `char` instances, `slurp` and `try-slurp`, and qualified `str/join str/split str/split-lines str/trim str/triml str/trimr str/index-of str/includes? str/starts-with? str/ends-with? str/blank? str/chars`, with `subs subs-from str/split-limit str/index-of-from str-len str-byte-at str-find`, `parse-long`, `digit? whitespace?`; `println print prn`, and their generic twins; `newline`; `spit` (over the builtin write-file); the builtins' names and Clojure aliases as §4.1; `defn defn-` (one clause) | T0, L3 (`Pair`, `Triple` in the prelude), L12 (`Result`), L17 (`Debug`), L18 (`str-byte-at`, `str-find`), L30 (`Div`, `quot`), E16 (the fusion rewrite), E13 (`if-let` over `Option`), D1 | model tests for `Vec Map Set` and `sort`; laws; a hash law over strings; the integration test | 153 |
| 2 | **the Clojure surface.** the rest of the sequence functions (`partition partition-all partition-by interleave zip zip-with seq-of seq= map-indexed keep reductions distinct interpose drop-while second mapv filterv repeat iterate cycle some`), `Cursor` and the buffering `Cursable` adaptors; `Stack` with `peek pop`; `ffirst` and the rest of the `Seqable` instances; `doall dorun repeatedly`, and the chunked realisation of a lazy seq over a chunked source (§5.6); the small insertion-ordered `Map`; Clojure's printed forms; `for doseq cond if-not when-not as->`, `if-some when-some when-first declare defonce`; **truthiness (L20), the keyword in call position (L14, L21), `and`/`or` typed**; `partial` (one free) `partial2 complement constantly`; `with try-let defrecord ->Name vector volatile!`; the arity forms of §4 on L1, deleting the stand-ins; patterns in parameters, bracket forms; the reader additions of E8 and E14 (a), the ratio literal among them | L1, L3b, L7, L8, L14, L15, L17, L19, L20, L21 (keyword), E3, E4, E8, E14 (a), E11, E2, B5, L16 (**Decided**: adaptor cursors without the buffer, `Lookup` for `Option`) | the 60 programs of the design record (20 each from three designs, all ran) ported as cases; expansion texts; a recipe sent to a task; polymorphic recursion rejected by both tools; the failing programs of §5.5 S1, S2, S7, S8 flip from `open` | 81 |
| 3 | **collections.** `get-in assoc-in update-in update-keys update-vals`; `condp case juxt dedupe flatten`; `fib.set`; `fib.sorted` (B-tree, `Queue`); `SubVec`/`subvec`; `sort-by-cached`; `partitionv` and the vector forms; the mutable arrays of §2.11 (`MArray`, `long-array`, `aget`, `aset`, `make-array`) and `try-parse-int`; `rationalize`; `rseq`; `Xf`, `Stepper`, `transduce`, `into/3`, `xf`; the multiplicative hash finaliser; `unchecked-add unchecked-subtract unchecked-multiply unchecked-negate unchecked-inc unchecked-dec` with their `-int` family, and the aliases of §4.1 and §4.3 (`aget`, `long-array`, `transient`); `ex-info` as data; **arity-reading forms and multi-arity literals at an expected type (L22: `partial comp juxt apply some->`), callable `Map` `Set` `Vec` (L21), multi-parameter dispatch (L23: `(into {} [[1 2]])`, `merge` over `Option`), joins and families (L24), heterogeneous literals (L25), literal widths (L26 a), static predicates (L31), `letfn` (C8)**; the reader additions of E14 (b); chunked `pmap`; the in-place paths of `Vec` and `Map` (C2 to C4 and C7, or the struct `Vec`, §9 Q18) | L2 (user variadics), L4, L6, L9, L10, E5, L21 (the collections), L22, L23, L24, L25, L26 (a), L31, E14 (b), C2, C3, C4, C7, C8, C5 (the `first`/`get`/`zip` count cases stay `open` until it lands) | B-tree model with `valid?`; the `conj` count case at n + c moves from `open` to required | 199 |
| 4 | **strings and numbers.** `fib.string` in full (the `Regex` instance of `Pattern`, case mappings, `replace-with`; the character-offset functions and `str` as a `Reducible char` are tranche 1), `fib.char`, `fib.regex` (non-backtracking core), `fib.math`, `fib.random` (the global generator, L15 init), `fib.walk`, `fib.sys`, the Java-name modules (E15), `fib.io`, `format printf`, `read-string`; **variable promotion (L26 b), which the rule settles (§9.1 Q36); the owner signs the item** | L11, L12, L15 (top-level atoms), E15, L26 (b), C10 for O(1) character offsets | strings against a byte-loop reference over generated UTF-8 (every scalar width, boundaries) and against a character-offset reference; regex against a bounded backtracking reference; math bit for bit between the interpreter and the compiled program on 3000 generated patterns, and against libm in a Rust test (exact functions equal, the others within a stated number of ulps); `Rng` against known first values | 113 |
| 5 | **the long tail.** dynamic vars and `binding` (L29, C11) with `*out*` and `with-out-str`; metadata (L27); `defmulti` and hierarchies; `locking` (identity-keyed monitors) and a thread-safe `LSeq` (C9); `BigInt`, `BigDecimal`, `(Ratio BigInt)` and their literals; `fib.data` (`Val`, `diff`), `pprint`, `sequence eduction` (after C1), `iteration`, `subseq`, the relational `set/project index join`, `seque`, `add-watch`, `with-redefs`, `file-seq`, `re-matcher`, lazy `pmap`; the backtracking regex fallback; **exceptions (L28), which the rule settles (§9.1 Q35), after the library is viable** | B1, C1, L12, L27, L28, L29, C9, C11, E14 (c) | as above | 81 |

**Status (2026-10-02; the evidence is §7.5 for tranche 0 and the Rust half, and the lines below for the flip).** **Tranche 0 is done except five of the thirty-two macros of E1** (`if-not`, `when-not`, `some`, `{..}`, `#{..}`): E7, B2, B3, E10, E12, H1, C6 and C12 are
**landed** (`8adeaa6`, `dfdd4ba`), with the disjointness gate of the four implicit modules (`crates/fibref/tests/lib_disjoint.rs`, `5ea989f`) and the harness's `allocs` header; 28 of the 33 macros
are in the registry (27 added). **The Rust half of tranche 1 is done**: L3 (`Pair`, `Triple`), L12's `Result`, L17's `derive Debug` and `ToStr`, L18, L30's `quot` and `Float.fdiv` (and `Div` and `Ratio` in the library),
E13, E16 and D1 except its `compose with xf` message, with the macros of R5 to R7 (`str println print prn pr`, the folds, `defn`, `update`, `reduce`, `swap!`) and `@t` as `join` (**Proposed**); the library half of tranche 1
has its first wave (`4485838`, fifteen packages under `lib/fib/`), the follow-ups of `605a26e`, and the mutation reviews of the fourteen library packages (`8b1e777`). **The flip has happened (M1 and M2, uncommitted when this was written)**: `IMPLICIT_LIB` is `fib.core`, `fib.seq`, `fib.coll`, `fib.print`, the prelude's protocols (`Countable Indexable Seq Collection Traversable Iter Associative`), `iter`, `collect`, `filter-iter`,
`VecIter`, `FilterIter`, the `for-each` function, the Vec-only `map` and `range`/`range-between` are deleted (§8.3 C), `/` is out of `Num`, the macros `for-each` and `range` name `fib.seq/run!` and `fib.seq/range-by`, the cases and the Rust tests are ported,
and `compiler/` is ported and `compiler/util/result.fib` deleted (§8.3, "What the flip did" and "What the port of `compiler/` did"). The run of the tree with the flip: `fibref cases` and `fibc cases` each give `cases/ownership` 240 cases, 240 pass; `cases/modules` 25 cases, 25 pass;
`cases/stdlib` 556 cases, 529 pass, 0 fail, 0 pending, 0 header error, 27 open (the `open-` cases of §7.5.3); `cargo test -p fibc --test bootstrap` 58 passed, `--test bootstrap_expand` 80 passed, `--test capi` 19 passed.
**Tranche 1 was not complete when this was written**, and none of the following had happened: the final gate's
last step (`every_row_of_the_tranche_is_covered` is still `#[ignore]`; run with `--ignored` against the tree of this status it passes, and the step is to remove the attribute, §8.1 item 1); the by-hand mutation reviews of the Rust packages (§6 MUT-P; no report of them is in the tree); the generator's forms for the new features
(R13 and R3 report that `fibgen` does not generate `@t` and generates float `quot` thinly, a planted fault in the model's `floor` being caught once in about 4000 programs; not re-run here); the commit of the flip. M6 stays
paused after step 2a until the library is viable (ROADMAP).

**Update (tranche 2's Z0, 2026-10-02).** The final gate's last step is done: `every_row_of_the_tranche_is_covered` ran with `--ignored` against the working tree and passed, and its `#[ignore]` is removed (§7.5.2 item 7); the flip is committed (`d09d795`). What remains of tranche 1 is not a gate: the by-hand reviews of the Rust packages and the generator's forms for `@t` and float `quot` (owner, 2026-10-02: reviews run in the background, §8.1 item 6). B4, E6 and E10 (`List`), which the Needs column of tranche 2 listed, are landed (§7.5.1): no package of tranche 2 delivers them.

The Rows column counts the survey names assigned to the tranche in §4 (153 + 81 + 199 + 113 + 81 =
627 names, plus 76 names Clojure lacks that the library adds; the other 45 survey names are not offered (§4.17), and
`#"..."` shares the row of `#"regex"`). Tranche 1 includes the core forms and builtins that already exist. The Needs
column was checked against §7's "Needed by" column row by row in the review; the first version omitted B3,
L3, L12, L10, L2, E6, E10 and C5. The rule check found six more, which this revision placed: L21 (the collections, tranche 3), L23 (recommended, §9 Q39, tranche 3), C6
(`fibc build` at `-O 2`, tranche 0), E14 split into (a) tranche 2, (b) tranche 3 and (c) tranche 5, and L31 (tranche 3); the third revision placed E16 and L30 in tranche 1 and C12 in tranche 0 (the owner's decisions), moved `LSeq`, `Pattern`'s `str` and `char` instances to tranche 1 and `repeatedly` to tranche 2, where their rows are, and gave the two-arity tranche-1 rows their stand-ins (§10.4.9 N15); and the second revision moved
`str` as a `Reducible char` and the character-offset functions from tranche 4 to tranche 1, because §4.3, §4.7 and `count` already said so and the tranche-1 rows
needed them (§10.4.8 C1, C2).
After tranche 3 the library is *viable* in ROADMAP rule 5's sense, and the benchmark suite (fibber
and Rust kernels, a ratio per kernel against the aim of within 1.5x) is designed then, not here. The order
is still led by what the compiler needs: tranches 0 and 1 are the same as before the rule, and what the rule
added (truthiness, callables, arity-reading forms, the string unit) lands in tranches 2 to 4 behind the
Clojure surface it makes terse.

#### 8.2.1 Tranche 2: what the plan decided (2026-10-02)

The work packages of tranche 2 (Z0, X1 to X13, P1, Y1 to Y12, Z2, ZF) were planned against the tree of the flip (`d09d795`). The plan
changed some sentences of this page and settled some things it left open. Every decision below is **Proposed**: the lead accepted it as the plan on 2026-10-02 and the owner has not signed
it (method.md). Those that are the owner's own questions say **Proposed (plan recommendation, owner not yet confirmed)**; nothing downstream may depend on their staying.

1. **Case numbers** (PC2-1). The cases of tranche 2 have four digits, `1000` to `5999`, in a block per package (`cases/stdlib/README.md`); the numbers 000 to 999 are used up. A number is the whole run
   of digits, so `--only 100-` does not match `1000-x`.
2. **The macro policy** (PC2-2). A macro is Rust, mirrored in `compiler/expand/prelude/`, when almost every program of the compiler uses it or stage 2 must expand it before it can run macros;
   every other macro is fibber, in a part of the library. The Rust macros of tranche 2 are the five open ones of E1 (`if-not when-not some`, and the literals `array-map` and `hash-set`) and the
   folds and printers that are one rewrite each: `declare vector hash-map max-key min-key vswap! print-str pr-str println-str prn-str` (15 rows). `try-let if-some when-some when-first as-> defonce
   partial2 doseq for lazy-cat defrecord with` are fibber macros. D2, **Proposed (plan recommendation, owner not yet confirmed)**: `try-let if-some when-some when-first doseq for` are Rust, because
   stage 2a of M6 has no macro runner and cannot expand a source that uses a fibber macro (a call of one is the same `MacroNeedsEvaluator` under both tools); until the owner answers they are fibber.
3. **`some`** (PC2-3, PC2-14, §3 N14). `(some pred c)` is the Rust macro that picks by argument count; `(some x)` declines and stays `Option`'s constructor. L1 has no constructor clause, and no
   library function is named like a constructor or a core form. `reduce`'s two-argument form stays its Rust macro; L1 adds the clause beside it for a user's `defn`.
4. **`x#` and `#"re"`** (PC2-4). `x#` (E14 a) is the expander's, not the reader's: `tmp#` already reads as a symbol, and case 724 is a quasi-quote rule, so it moves to E2 and E11. `#"re"` leaves
   tranche 2 for tranche 4, with `Regex` (D8, **Proposed (plan recommendation, owner not yet confirmed)**).
5. **`defrecord` and the default derive** (PC2-5). Tranche 2 ships `defrecord`, which writes the six derives, and B5; the default derive of `defstruct` (L17's second half) is deferred (D6, **Proposed
   (plan recommendation, owner not yet confirmed)**: the default needs the checker, would change the instance set of every type of `compiler/` and the cases, and eight `reject` cases lean on its absence).
6. **L21's keyword half only** (PC2-6). `(:k x)` and a keyword where a function is expected are tranche 2 (case `1000`, `open:` L14); the collections stay in tranche 3 (case `901`).
7. **Chunking last** (PC2-7). The chunked realisation of §5.6 is the last library package (Y12); whether it may slip to tranche 3 without blocking the gate is D4 (**Proposed (plan recommendation,
   owner not yet confirmed)**: it may, if the wave runs late).
8. **`print-raw`** (PC2-8). The prelude's public `print-str` is renamed `print-raw` in the commit that lands the `print-str` macro, so that the two do not collide (§7.5.2 item 2).
9. **Tranche 1's gate first** (PC2-9). Done by Z0: `every_row_of_the_tranche_is_covered` lost its `#[ignore]` (§7.5.2 item 7). The gate of tranche 2 (ZF) raises `TRANCHE` to 2.
10. **E2 and E11 together** (PC2-10, D3). One package, in the first slot of the Rust queue after the syntax packages: every later fibber macro costs about 90 ms under `fibc` until E2 lands, and E11
    changes what a macro template means. Its contract includes that the macro module sees the implicit library (§7 E2).
11. **`and` and `or` as core forms** (PC2-11). The checker elaborates them and the expander passes them through; the core form `if` takes one arm and `when` expands to `(if c body)`; the expansion
    dump of every program that uses `and` or `or` changes in that commit, and so does the self-hosted expander's.
12. **Z2** (PC2-12). The self-hosted expander expands the implicit library once per process (§6.4).
13. **The D1 hints** (PC2-13). The five arity hints of D1 (`use get-or` and four more) are deleted by Y11 in the commit that lands L1's clean-up; cases 803 to 806 and 808 change from `reject`
    to `accept`.
14. **The owner's other open questions**, each **Proposed (plan recommendation, owner not yet confirmed)**: D5, a pattern parameter of a `defun` is spelled `(PAT :as p: T)` and a `fn` literal may
    take a bare `[k v]` item (L7); D7, `Iterate`, `Repeat` and `Cycle` have no `Show` (a compile error names the type) and `Iterate` is a pure struct that recomputes `f` on each traversal of a bound
    value, whose function field has the colour `:local`, so an `Iterate` does not cross a task (case 2229) unless the struct takes a colour parameter, `(Iterate a k)`, which also edits Y3's `IterateCur` (found by Y1); and the end state of a `cond` with no matching clause, which is `nil` (the `cond` row), the trap of tranche 2's X2 lasting until the truthiness package (L20) lands.

### 8.3 Migration of the prelude

A. Add the library modules beside `fib.prelude`; names that collide (`map count range first rest`) are
shadowed by the `:use`, which the module system does silently ([R] A6), and the library's protocols that reuse a
prelude name (`Collection`, `Seq`, `Indexable`) need §7 B3 first, or step C in the same commit as step A.
B. Port the cases that use the names being replaced; **`/` on integers becomes `quot`** when L30 lands: in three case files, 102, 103 and 190 (`grep -rn '(/ '` finds 21 files, but the other 18 divide floats, which stay `/`; the headers of 102 and 103 pin
the traps `integer overflow` and `integer / by zero`, which `quot` keeps), in `compiler/syntax/number.fib` (one use), in the generator `crates/fibgen/src/gen/nums.rs` (integer `/` is one of its operators) and in two Rust unit tests of `fibref`. Counted by grep over the 191 `.fib` files of
`cases/ownership`: `range` in 7, `for-each` in 5, `pmap` in 4, `filter-iter` in 2, `iter` in 2, `(map` in 1,
`collect` in 1, `Countable` in 1, and, found by the review, `first` in 2 (cases 01 and 61: `first` on a `List`
changes type from `e` to `(Option e)`), `(list ` in 4, `(next ` in 2, `map-put!`, `map-del!` and `disj` in 1 each (`append`, `length` and `set-contains?` stay as aliases, §6.5), and `(. e key)`/`(. e val)` on a map entry in case
181 (the `Entry` struct is removed for `Pair`, §2.3); §6.5 lists every prelude name. C. Delete `Traversable`,
`Iter`, `Seq`, `Indexable`, `Countable` (with `Countable str`, which counts bytes), `Associative`, `for-each`,
`iter`, `collect`, `filter-iter`, `VecIter`, `Entry`, and the Vec-only `map`; make the four
implicit modules the implicit `:use`. D. **Deferred past tranche 1 (PC-1, §6.2):** `Vec`, `Map`, `Set` and `List` stay in
`lib/prelude.fib` with their layout (`fibc`'s `rt/vec.lir` and `fibref`'s `eval/vecs.rs` read it, and the checkers find `Vec` by name there);
the library holds their instances, and moving them is a task of its own. No case counts a string
literal and `compiler/` calls `str-len` (19 uses), so making `count` of a `str` count characters costs nothing that
was found; the compiler's own errors list any other use. The printed forms change (an `Option` prints its payload, a seq prints in
parentheses, a small `Map` in insertion order), and no case pins printed text: the headers pin results and traps (§6.5).

**What the flip did (M1, the compiler side, 2026-10-02).** `IMPLICIT_LIB` is `fib.core`, `fib.seq`, `fib.coll`, `fib.print`. `Num` lost `/` (the checker's protocol, the interpreter's
`float_binary`, `fibc`'s `float_binary`; `quot` keeps the integer division and its texts), and the `Ratio` instance of `Num` lost its `/`. The `for-each` macro's
declined path is `(fib.seq/run! f c)` (arguments swapped; any other arity is the macro's arity error) and `range`'s two arguments are `(fib.seq/range-by a b 1)`;
the self-hosted expander (`compiler/expand/prelude.fib`, `modules.fib`) mirrors both and the default implicit list. The prelude lost `Countable Indexable Seq Collection
Traversable Iter Associative`, their instances, `VecIter`, `FilterIter`, `iter`, `collect`, `filter-iter`, the `for-each` function, the Vec-only `map`, `range` and
`range-between`; `push!` and `map-put!` and `str-join`, `str-chars` are written over `vec-conj`, `vec-nth`, `vec-count`, `map-assoc`; the prelude gained `map-count` and `set-count`
(the node counter became `mnode-count`), which the library's `Reducible` instances of `Map` and `Set` use. Ported: `cases/ownership` 01, 09, 10, 13, 16, 31, 61, 83, 91, 92, 95, 131, 183, 202, 203 (bodies only,
headers unchanged; Appendix A of syntax.md follows for the five of the twenty), `cases/modules` 007, 52 cases of `cases/stdlib` that wrote `fib.prelude/count`, `nth` or `conj`,
the Rust tests that embed fibber text, the generator (`map` and `range` are wrapped in `vec`, its `nargs` macro counts with `vec-count`). Found by the flip: a user macro's body is
run in a module that holds the prelude and not the implicit library, so `count`, `conj`, `first`, `map` are unbound there (syntax §3.16); and a generic function whose bound is `Num t` and whose
body writes `/` is not rejected: the checker adds `Div t t` (case 873). 

**What the port of `compiler/` did (M2, 2026-10-02).** With M1's tree the three tools `compiler/read.fib`, `compiler/expand.fib` and `compiler/jit-demo.fib` already built and passed the
differential tests, so the port is small: `compiler/util/result.fib` is deleted and `util.result` is out of the `:use` of the 32 modules that named it (`syntax/{dump,lexer,literal,number,reader}`,
`expand/**`, `lair/**`, `jit-demo`), so that `Result`, `Ok` and `Err` are the prelude's (same type name, variants and fields). No other name of `compiler/` needed a port: the local
variables and parameters that the library's `first`, `rest`, `count` shadow need nothing (§6.1), `Entry` is gone (`CtxEntry` and `entry-*` are the compiler's own), the `for-each` calls of
`expand/ctx.fib` and `expand/dumpctx.fib` take the macro's declined path to `fib.seq/run!` and walk a `Map` as `Pair`s, and `admit.fib` and `program.fib` already used the `Cons` and
`Empty` variants. The port is faithful and not idiomatic: M6 resumes with the library in view. The `Ratio` normaliser of `lib/fib/core/num.fib` was changed (§2.8, cases 093 to 098).

## 9. Open questions for the owner

The owner prefers a recommendation to an open question; each row has one, with the evidence it rests on. "Decide" means
the owner's sign-off turns it **Decided** in the page it changes (§7.4 lists the edit of each other chapter). §9.1 holds what the owner has decided and,
in §9.1b, the answers that the rule derives (the first version's questions the rule settles, including the decisions made before the rule that
it reverses): the owner signs the §7 item for each. §9.2 holds **only what the rule does not settle**: a sequencing question, implementation
choices, and the recommendations the owner has not signed (Q14, Q22, Q41).

### 9.1 Decided

| # | Decision | Owner, date | Settled by the rule? | Where it lands, and the other chapters' edits (§7.4) |
|---|---|---|---|---|
| Q15 | Checked arithmetic: overflow traps by default; wrapping builtins (L10, tranche 3); `fibc build` at `-O 2` and `fibc run` at `-O 0` (C6); the rotate-and-xor `hash-combine` in tranches 0 and 1 so no combiner traps; the integer hash replaced by a 64-bit finaliser in one commit with types §2.12 and the cases' expected orders | 2026-10-01 | checked arithmetic yes (Clojure's `+` throws on overflow); `-O 2` and the finaliser no (the owner's) | §2.7, §2.8, §7 L10, C6; types §2.12, compiler.md, ROADMAP M7 rule 4 |
| Q24 (L19) | An integer literal whose value is exactly representable adopts a float type when it unifies with one; a variable never does through this rule | 2026-10-01 | yes (Clojure's `(* 2 1.5)`) | §2.8, §5 C1, §7 L19; types §1.1, §2.12, syntax §1.2 |
| Q28 | `(first xs)` on a `List` returns `(Option e)`; case 01 (one of the 20 owner-decided cases) and case 61 are ported to it | 2026-10-01 | yes (T1: `nil` is `Option`) | §6.5, §8.3; cases 01, 61 |
| Q30 (L16) | An impl's context and a method's own variable may name a type variable that a constraint determines from the head | 2026-10-01 | no (the owner's choice of mechanism: Clojure has no counterpart) | §2.1 rule 5, §7 L16; types §3.3, §4.1 |
| Q34 | Sequence functions are Clojure's memoised lazy seqs (`LSeq`: a value traversed twice runs its function once); `lazy-seq`, `doall`, `dorun` and chunking follow Clojure; the zero-cost push loop is an optimisation under the syntactic fusion rule of §2.1 rule 2 (E16), which cannot change observable behaviour; the affine-recipe design is deleted | 2026-10-01 | yes (Clojure's semantics); the fusion rule is the owner's requirement for the performance aim | §1.2 P2, §2.1 rule 2, §4.4, §5.7, §7 E16 and L13 (withdrawn), §8.2 tranche 1; no other chapter (§7.4) |
| Q40 | Integer `/` is a `Ratio`: `(/ 7 2)` is `7/2`, `(/ 6 3)` is `2`; `quot`, `rem`, `mod` truncate as Clojure's; `/` is the method of `Div`, whose instance determines the result type, and `Num` keeps `quot`; 64-bit components trap on overflow, `(Ratio BigInt)` is tranche 5 | 2026-10-01 | yes | §2.8, §4.1, §4.3, §5 D4, §7 L30; types §2.12 (§7.4) |
| Q42 | `Ord` on `str` and `char` is code-point order (the UTF-8 byte order, the builtin instance): a `char` is a scalar, so a string order that disagreed with its characters' order would be inconsistent; no runtime change; it differs from Clojure only for a character above U+FFFF against one in U+E000..U+FFFF | 2026-10-01, owner-invited (the owner may overrule) | no: a deviation the owner chose (§5 D5) | §2.7, §2.9, §4.3 `compare`, §5 D5; types §2.12 unchanged |
| Q43 | Floats print as Clojure's `Double.toString` (positional for `1e-3 <= \|x\| < 1e7`, else `d.dddE<exp>`, shortest digits, `NaN`, `Infinity`, `-Infinity`, `-0.0`), amending the decision of 2026-09-30; floats stay `f64`, float literals read as today | 2026-10-01 | yes | §2.7, §5 S18, §7 C12, §8.2 tranche 0; types §2.12, `float_text`, `fib.show-fp`, cases 169/178/187, the reader dump (§7.4) |

Q14 (push as the primitive, with a pull `Cursor`) was recorded as decided by the second revision because L16 is; the owner decided L16, a
type-system rule, and not the choice of push, so Q14 is a recommendation in §9.2 (third revision, §10.4.9 N7).

### 9.1b Settled by the rule (the answers the rule derives; the owner signs the §7 item)

| # | Question of the first rewrite | Answer the rule gives | Why |
|---|---|---|---|
| Q33 | Does the owner exempt any of the sharp edges that the rule replicates? | **Replicate all of them**; the owner may strike any row, each is a one-line change (§5.6). The eight of the first rewrite plus `sort-by`'s two key calls per comparison, chunking, `hash-map`'s hash order (the order of strings is the owner's D5). `Hash`/`Eq` on `-0.0` stays the contract that `=` implies equal hashes until Clojure's `(hash -0.0)` is verified | none is a memory-safety matter (a merge sort over arrays reads only checked indices, [R] A11 nan); the two auditors flagged them "bug-compat" and left the call to the owner, and the rule leaves none: Clojure's behaviour is Clojure's |
| Q35 | Exceptions: amend types §2.11 (abort-only, **Decided** 2026-09-28) so that `throw`/`try`/`catch`/`finally` exist? | **Yes, after the library is viable**: `ex-info` as data now, a task's trap isolated by `join` next, unwinding last (§2.10, L28), and the types amendment written before the work | no memory-safety failure prevents them (a caught exception that skips releases leaks, §1.1); D1 is a decision of the owner's that the rule reverses (§5 D1); unwinding is a large change in lIR, the ownership checker and `fibref` and cannot be prototyped without editing them; ported Clojure code that catches cannot run until it exists |
| Q36 | Do the numeric builtins promote for variables (`(+ n 2.5)` with `n: i64`), lifting types §1.1 D3? | **Yes, operator-level along `i8 < i16 < i32 < i64 < f64`, `f32 < f64`, never narrowing, never at `let`, return or field (§2.8, L26 b)**; a generic `(add n 2.5)` still fails | Clojure promotes at every mixed operation, the lattice is statically typable and has no safety content, and the owner already accepted the literal case (L19: the same D3); today `(+ n 2.5)`, `(< n 2.5)`, `(f 2)` for `(f x: f64)` and `(/ (reduce-sum xs) (count xs))` into an `f64` are `cannot unify f64 with i64` ([R] A11 n1, n10, n11, t85) |
| Q37 | Character offsets in a UTF-8 `str` are O(n). Take the ASCII flag in the `str` header (C10)? | **Yes**: computed in the validation pass that already scans every constructed `str`, it makes `count`, `nth`, `subs` and `index-of` O(1) for ASCII text and a byte walk otherwise; the byte layer (`str-len`, `str-byte-at`, `str-find`, `str-slice`) stays for tokenizers | cost is not a reason (§1.1), so the unit is Clojure's characters (scalars, §5 T10) and the flag is the cost's cure, a representation change in both tools and not a language change |

### 9.2 What the rule does not settle

| # | Question | Recommendation | Why |
|---|---|---|---|
| Q38 | Sequencing the reader changes: `#(..)`, `#{..}` (E8) and the rest of Clojure's reader (E14, including the comma as whitespace and `~x`), against M6 step 1 (the reader port) and the 218 pinned reader tests | **Land E8 and E14 in one commit after M6 step 1 closes, extending the reader dump of spec/bootstrap.md §2 once, and migrate every `,x` of `lib/`, `compiler/` and the cases in the same commit** | the rule requires Clojure's text; each reader change costs two readers (Rust and fibber) and the churn of the macros; doing it once avoids doing it twice |
| Q39 | Multi-parameter dispatch (L23), a type-system change, or the checker rules (L20, L24) alone? | **Take L23**: it removes the rest of the overlaps (A11 t70, t71) as library code, with `(into {} [[1 2]])`, `merge` with `nil`, `str/replace` by replacement type and `(Option bool)` predicates; **keep L20 and L24 as checker rules anyway** because they are cheaper and give better messages | the two auditors named it "the largest lever" (one) and "expensive, with a checker rule for each use" (the other); the rule needs the uses, not the mechanism; it costs an instance lookup keyed by several heads and an overlap check on the product |
| Q14 | Push as the primitive (`each-while`), with a pull `Cursor` for lockstep walks, or pull throughout? | **Push**, with the cursor beside it: it is the fused path of §2.1 rule 2; L16 (Decided) is the mechanism its bufferless static form needs, not a decision for push | pull is cheaper today (3 indirect calls against 6) and runs `f` twice per surviving element (1500 calls for 1000, A10 d1c); push is one loop per source and early exit is a value ([R] A10 push); the lazy seq, the unfused form, is Clojure's own definition over `first`/`rest` and shares nothing with either but the sources |
| Q22 | Do `defstruct` and `defenum` derive `Eq Ord Hash Show Debug ToStr` by default (L17)? | **Yes**, with a qualifier to opt out | a Clojure record is `=`, `hash` and printable, and `defrecord` is `defstruct`; the second revision called it settled, but the owner has not signed L17 (§10.4.9 N7) |
| Q41 | A global lazy seq or `delay` (`(def fibs ..)`, `(def cfg (delay ..))`) is rejected by M1, its type holding a `Cell`, until the run-once cell of C9 exists. Move that part of C9 from tranche 5 to tranche 3? | **Yes**: the run-once cell (a `Mutex`-guarded `Lazy`, `Send` when its value is) in tranche 3, the monitor table of `locking` staying in tranche 5 | `(def c: (Cell i64) (cell 0))` is rejected today ([R] A13 def1) and will be by L15's type test; the `Atom` memo crosses tasks but may run its thunk twice under a race (A11 e8e), which Clojure's `delay` and `lazy-seq` never do |
| Q17 | Closure types (C1) or fusion macros? | **C1; no fusion macros.** | a fused `->>` covers only a visible literal pipeline, costs 80 to 95 ms per distinct macro, and is Rust-side code that stage 2 must re-implement; C1 serves every call site |
| Q18 | Does `Vec` become a struct with a spare-capacity tail, held in a cell and updated through `set-field!`? | **Yes, to be decided before tranche 3, not after its counts**: the struct `Vec` plus C2 and C3. C7 (moving a payload out of an owned shell) is the alternative that keeps the enum, and is a medium ownership-checker change that also serves `Map`. | the evidence moved: C2 to C4 alone do not make the existing `Vec` update in place (2002 objects for 1000 puts on an owned struct with an array field, A10 own), a struct held in a cell updates in place today (1000 `set-field!` allocate nothing), and a required count case cannot wait for a measurement that §2.5 already predicts; the layout is shared by `fibc` (`rt/vec.lir`, `lower/pattern.rs`, `macros/abi.rs`) and `fibref` (`eval/vecs.rs`), so both change together. Subvectors and `rest` want an offset field in the same struct (`SubVec` and `VSeq`, §2.1 rule 8, §4.5): `subvec` then returns a `Vec` itself |
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

§10.4.1 to §10.4.7 record the first rule revision and keep its wording (`Slice`, `:redefinable`, M3 to M6); §10.4.8 records the second revision, which changes some of their outcomes.

#### 10.4.1 Principles and non-goals

| Item | Was | Now | Reason |
|---|---|---|---|
| P1 | Clojure's names first; a deviation where §5 says so, with the reason | adapted. P0 states the rule; P1 is "names, shapes, argument order and behaviour"; every §5 row carries a failing program or a decided typing fact | the clause "unless §5 says otherwise" was used for cleanliness, speed and one-way reasons that the rule does not accept (§1.1) |
| P2 | zero-cost: recipes, never a lazy cell, nothing per element | adapted. recipes for pure adaptors; sequences whose elements come from effects are memoised (`LSeq`); `cache`, `doall`; §9 Q34 asks about the default (superseded: the owner decided memoised seqs with a fusion rule, §10.4.9 O1) | a counting `map` traversed twice makes 6 calls where Clojure makes 3 (A11 t20); the memoised form runs, 5 calls (t21); cost is not a reason |
| P3 | persistent, in place when unique; no transient API | adapted. the same, and `transient` `persistent!` `conj!` `assoc!` `dissoc!` `disj!` `pop!` exist as identity wrappers | Clojure's text must resolve; `(persistent! (conj! (conj! (transient []) 1) 2))` is `[1 2]` (A11 t60) |
| P4 | unboxed elements, a real hash | same. plus: a `Map` of at most 8 entries keeps insertion order (§2.7) | an implementation property, invisible in Clojure programs |
| P5 | types, not truthiness; `or`/`and`/`when` take `bool` only | reversed. a condition accepts `bool` or `(Option T)`; `or`/`and` typed by operands; one-armed `when` is `unit` or `(Option T)` | memory-safe, typed and free at run time (A11 t04, e1z); the rule says Clojure; other types as conditions stay errors (no blanket impl, t06) |
| P6 | one way: no aliases, no run-time predicates | reversed (predicates kept). every Clojure name is offered, as a function, macro or alias; run-time predicates stay out (§5 T3) | "one way" is not a reason; a predicate needs run-time type information the monomorphised program does not carry |
| P7 | no dynamic vars, no global random generator, no metadata, no top-level atom | reversed except a global `Cell`/`Weak`. top-level atoms, `binding`, global `rand`, `defmulti`, metadata are offered (§2.11) | none breaks memory safety; a global `Cell` is reachable from every task (§5 M1, A11 t41) |
| P8 | ships on today's compiler; items the owner signs separately | same. the list of signed items grows to E3 E4 E8 E14 L14 L19 L20 to L26 | the rule requires them (§7) |
| P9 | every function has an executable test | same. §8.1 item 8 makes the rule's reasons executable | method.md |
| NG1 | no `eval`, no reflection, no arbitrary precision, no ratios, no Java interop | adapted. `eval` and reflection kept (§5 T3); `BigInt`, `Ratio`, `BigDecimal` are library types (§2.8); the Java static names are modules (§4.16) | an explicit big or rational type breaks neither safety nor typing (A11 t50); only auto-promotion has a value-dependent result type (§5 T5) |
| NG2 | does not hide cost; re-traversing a recipe re-runs it | same. `count` says whether it is O(1) or a walk; the second half is withdrawn (§10.4.9 N1, O1) | documentation of cost is not a deviation |

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
| Q13 | recipes that re-run, `LSeq` in tranche 5 | adapted. split into Q34, decided by the owner on 2026-10-01 (§9.1) | the collision of the two aims |
| Q14 | push with a pull cursor, conditional on L16 | kept. a recommendation (§9.2): L16 is Decided, the choice of push is not (third revision, §10.4.9 N7) | §9.2 |
| Q15 | checked arithmetic and the hash | decided. owner, 2026-10-01 | §9.1 |
| Q16 | bracket forms additive; flat `cond` | same | Clojure's |
| Q17 | closure types, no fusion macros | same. stays in §9.2 | implementation |
| Q18 | `Vec` a struct with a spare tail | same. stays in §9.2 (offset field added) | implementation |
| Q19 | which macros are Rust | adapted. thirty-two | §6.3 |
| Q20 | heterogeneous data: a `Val` enum | kept. T4; L25 adds the literal | static typing |
| Q21 | transducers as `Xf` with a flush | kept. T9 | `let` does not generalise |
| Q22 | derive `Eq Ord Hash Show Debug` by default | same. recommended (§9.2), `ToStr` added; the owner has not signed L17 (third revision, §10.4.9 N7) | a Clojure record is `=` and printable |
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
reversed 54, adapted 42, kept 21, same 49, decided 5 (Q14 relabelled by the third revision, §10.4.9 N7). Of the 673 survey names of §4 (672 rows; `#"..."` shares one): reversed 239 (47 renames undone, 192
names offered that the first version did not), adapted 117, kept 89 (not offered), same 227 (the row did not change). **In all: reversed 293, adapted 159,
kept 110, same 276, decided 5.** "Reversed" means no deviation remains; "kept" means a deviation stands and §5 gives its reason. Auditor a kept five rows on a
memory-safety reason (`aset`, `locking`, `make-array` of an object type, the UTF-16 unit, `future-cancel`); auditor b kept one (a global `Cell` or `Weak`) and two on the
ownership no-leak promise. §5.1 takes the union and says, for each row, which program was run, which checker rejection is quoted and which has none (M5).

**Where the two auditors differed, and how it was decided.** Re-running the deciding program is how every row below was settled.

| Item | Auditor a | Auditor b | Decision and evidence |
|---|---|---|---|
| lockstep over adaptors (`(zip xs (map inc xs))`) | adapted as the page: needs L16 | adapted: every adaptor `Cursable` by buffering, no language change | **b**: the three impls run, `[[1 2] .. [5 6]]` and the `filter` case ([R] A11 e16); L16 (Decided) then removes the buffer for four types |
| `(count (map f c))` | adapted as the page: invisible for pure `f` | reversed: realise the seq | **b**: an adaptor does not override `size`, `[2 2]` and `[3 4]` (A11 count); the O(1) was a cost reason |
| default for adaptors: memoised or re-running | hybrid: effect sources memoised, `LSeq` early | recipes plus `cache`, and use-count insertion of `cache` | **both, as the recommendation of §9 Q34**: hybrid now, use-count insertion as the faithful form at zero cost (superseded twice: the use-count rule is unsound, §10.4.8 F1, and the owner decided memoised seqs with a syntactic fusion rule, §10.4.9 O1) |
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

#### 10.4.8 Second revision: the rule check's 28 findings

An adversarial checker ran the rewrite against the rule (its programs are in `rcheck/`, re-run in A12) and raised 28 findings: **F1 to F15** deviations without a reason,
**R1 to R6** safety reasons that a safe program refutes or that have no program, **C1 to C6** inconsistencies after the rewrite, and **X0**, the re-run of every `ran:` block (which
reproduced). The second revision resolved each: **accepted** (the page changed), **accepted in part** (the page changed and the rest is rejected, with the reason) or **rejected**.
Result: 27 accepted, 1 accepted in part, none rejected; the rows of §4 changed text in 185 places (44 of them the predicates moved from omit to adapt) and 30 changed tranche, 7 `(new)` rows were added and `Slice` was replaced by `SubVec` and `VSeq`, and
§4's counts were recomputed by `count.py` (keep 145, adapt 427, alias 55, omit 45; tranches 146, 84, 200, 113, 84; 48 `(new)` rows). Facts of Clojure that the checker or this revision
gives from knowledge are marked **[K]** where they matter and were not verified (there is no Clojure on the machine).

| Finding | Verdict | What changed | Evidence |
|---|---|---|---|
| F1 recipes re-run `f`; the use-count cure is unsound; no §5 row | accepted | recipes are affine (§2.1 rule 2, L13); the use-count rule is withdrawn; `seq`, `doall`, `vec`, `cache` make a re-traversable value; K1 in §5.7, S14, Q34 rewritten with the evidence and what differs observably | A12 q34 (`[6]` `[6]`), aff1, aff2 (the checker has no move rule), aff3 (the run-time trap) |
| F2 `reverse`, `sort`, `sort-by`, `take-last`, `partition*`, `split-at`, `split-with` return `Vec`s | accepted | `reverse` is a `List`; `sort`, `sort-by`, `take-last`, `doall` return a `VSeq`; `partition*` yield `VSeq` groups and the `v` forms `Vec`s; `split-at`, `split-with` a `Pair` of `VSeq`s; §2.7 print table | A12 q_sortconj (the first rewrite), vseq |
| F3 one `Slice` for `rest` and `subvec` | accepted | `VSeq` (the seq: `conj` at the front, parentheses) and `SubVec` (a vector: `conj` at the end, brackets) | A12 q_slice, vseq |
| F4 `with-redefs` opt-in for a cost reason | accepted | works on any function; the compiler makes a slot for each function that some `with-redefs` names (§2.11, L29); `:redefinable` is gone | [sketch]: no program exercises it |
| F5 `sort-by` calls the key once | accepted | the key runs twice per comparison; `sort-by-cached` (new) is the one-call form; §5.6 | A12 sortby `[18 5]` |
| F6 records print `(Name f1 f2)` | accepted | `#m.P{:x 1, :y "x"}` with the `pr`/`println` difference at a string field (§2.7); `str` of a record is believed to be the `pr` text **[K]** | the table row |
| F7 `hash-map` and `array-map` are one thing | accepted | two shapes of one `Map`: `hash-map` builds the HAMT, the literal, `array-map`, `into {}`, `zipmap` the array shape (§2.7, §4.4, §4.5) | A11 e13 ran the array shape; the tag is **[H]** |
| F8 functions return a value where Clojure throws | accepted | `Integer/parseInt`, `Long/parseLong`, `Double/parseDouble`, `slurp`, `re-pattern`, `read-string`, `max-key`/`min-key` trap (P5); the typed twins are `try-<name>` (§3 N13); D1 is not the reason and is not cited | rows of §4.3, §4.4, §4.9, §4.14 |
| F9 transducer arities missing | accepted | the list and the `x<name>` stand-ins (§2.1 rule 7); nine ran in A12 xf2, five before, eight are the same shape and were not written | A12 xf2 (and the mutation: `FAIL remove`) |
| F10 chunking not in §5.6 | accepted | replicated: §5.6 lists it, a 32-slot buffer per traversal of a mapping stage, written for `map` | A12 chunk `[32 32 64 64 10 0]`; Clojure's counts **[K]** |
| F11 `Ord str` by scalar, Clojure's by UTF-16 unit | accepted | `Ord str` is Clojure's order (§2.7), one fix-up at the first differing byte; the unit of `count` is a separate question (T10) | A12 u16cmp `[5184 0 1068]` |
| F12 integer `/` is Rust's though a typed twin exists | accepted | `/` on integers is a `(Ratio t)`, `quot` truncates (§2.8, D4, L30, Q40); why `+'` is accepted and the first `/` was not: the twin keeps the value; the compiler's 1 use, three case files, the generator and two unit tests migrate (§8.3) | A12 ratio |
| F13 type predicates omitted though constants; `(int \a)` | accepted | §4.18: 44 predicates as checker forms (L31) and variant tests over `Val` and `Form`; `ToInt` and `ToLong` for `char` | A12 int1, int2, pred1 (the fold is [sketch]) |
| F14 multi-arity `fn`, `%&`, variadic `fn` out while T2 says twinned | accepted in part | a multi-arity or rest-parameter literal (and `%&`) takes the arity of its expected type (L22, §5 T2, §4.2); rejected: a multi-arity function stored or returned has no single type, so a hand-written transducer `(fn [rf] (fn ([] ..) ([r] ..) ([r x] ..)))` is an `Xf` (T2 now says so) | types §1.4 |
| F15 rest parameter is `...` | accepted | `& xs` (L2); `...` accepted in `defmacro` until `lib/` and `compiler/` migrate | A6 (`&` compiles today as a parameter) |
| R1 M2: an aliased write is not a safety failure | accepted | M2 rewritten to the cross-task race; arrays are `(MArray t)`, shared and written in place inside one task, rejected across tasks (§2.11, §5 M2) | A12 p1, p10, p11, aset1, aset2, aset3 |
| R2 M4: only a lone surrogate is a safety matter | accepted | M4 removed; the unit is `char`'s, a decided typing fact (T10), with the failing program | A12 p2, p2b, unit |
| R3 M6: the failing program does not involve `locking` | accepted | M6 removed; `locking` is Clojure's `(locking x body)`; a lock from two `Atom`s runs; the identity-keyed monitor is **[sketch]** (C9); `Mutex` stays as an addition | A12 p3 (600) |
| R4 M1 and `volatile!` | accepted | `volatile!` is an `Atom` (§2.11, §4.1); M1 narrowed to `Cell`, `Weak` and `MArray` in a `def` | A12 p12 |
| R5 M3 and M5 have no failing program | accepted | M3 removed (`make-array` of an object type is an `(MArray (Option T))`, T1); M5 removed (`future-cancel` is Clojure's; the interrupt needs L28 step 2, S16) | none: neither had a program |
| R6 §10.4 does not show the deciding re-runs of the five auditor-a rows | accepted | the table below | A12 |
| C1 the tranche plan omits L21, L23, C6, E14 and others | accepted | L21 and L23 in T3, C6 in T0, E14 split (a) T2 (b) T3 (c) T5, L13 T2, L30 and L31 T3, B1 in T5; `vector`, `empty`, `peek` and `pop`, `hash-map`, `~x`, `*command-line-args*` and 20 math rows moved or noted; a script checks the rows against the Needs columns | `cons.py`, `cons2.py` |
| C2 strings are seqable against the tranche column | accepted | `str` as a `Reducible char`, `Lookup` and `Keyed`, and the character-offset functions are tranche 1 (O(n) until C10); `Keyed` and `Lookup` on `str` and `Array`; `str/last-index-of` takes `from` | A12 keyed |
| C3 stale printer text | accepted | the twins and `str/join` and `format` are over `ToStr`, `Show`, `Debug` as §2.7 says (§2.10, §4.7, §4.14) | |
| C4 other stale lines (nine) | accepted | `first` is a function (rule 8); `subseq` returns `(SubSeq s)`; `math/random` is `(math/random)`; `*in*` and `*print-length*` notes; `not-empty` returns the collection; §2.3 `merge`; `unchecked-long` and the other float conversions; the row then numbered 6 in §5.2 moved to S4 (that number is unused now); `concat` of zero or one argument; §6.1: 491 lines at HEAD, **522** in the working tree (the checker said 524; `wc -l` says 522), where `hash-combine` is a different mixer (§2.7) | |
| C5 decisions recorded only in this page | accepted | §9.1 records each with whether the rule settles it, and §7.4 lists the edit of types, syntax, ROADMAP, compiler.md and the cases; none of them is edited here | |
| C6 §9 holds items the rule settles | accepted | Q33, Q35, Q36, Q37 moved to §9.1b with the rule's answer; Q40 added; §9.2 holds only Q34 (the collision), Q38, Q39, Q17, Q18, Q19, Q26, Q27, Q29 | |
| X0 two `ran:` blocks contain `..` | accepted | `fib.seq` is marked an excerpt; `fib.seqable` is now a complete block that ran (A12 vseq) | A12 |

**R6: the deciding re-runs of the five auditor-a rows** (the table of §10.4.7 gave `future-cancel` only):

| Row | Auditor a kept it on | Re-run | Decision |
|---|---|---|---|
| `aset` | value semantics against an alias | the aliased write through a shared cell is Clojure's and safe (A12 p1, p10); an `MArray` shared by a second binding and a closure (aset2); across two tasks the cell-capture check rejects it (aset3) | **reversed** to Clojure's text; M2 is the race only |
| `locking` | a shared `Cell` | the cited program is rejected whatever `locking` is (t41); a ticket lock from two `Atom`s gives 600 (p3) | **reversed**; M6 removed |
| `make-array` of an object type | an uninitialised slot | no program can show it: the array is an `(MArray (Option T))` filled with `nil`, as `object-array` | **reversed** (T1) |
| the UTF-16 unit | an invalid `str` | UTF-16 `count` and `subs` run as user code, a bound inside a pair traps (p2, p2b); with count in UTF-16 units and `nth` by element the idiom `(dotimes [i (count s)] (nth s i))` traps (unit) | **re-filed** as T10 (`char` is a scalar, types §1.1), not as safety |
| `future-cancel` | an asynchronous kill | Java's interrupt is cooperative too; nothing was run: no cancel primitive exists | **reversed**; the interrupt needs L28 step 2 (S16) |

#### 10.4.9 Third revision: the rule check's 17 findings and the owner's four decisions

A second adversarial check ran the second revision against the rule (its programs are in `rcheck2/`, re-run in A13) and raised 17 findings, **N1 to N8**
major and **N9 to N17** minor. While the third revision resolved them the owner decided four questions the page had carried, **O1 to O4** (2026-10-01;
§9.1 Q34, Q40, Q42, Q43), which settle N1, N3, N4 and N6 and rework the sections they touch. This is the last revision: the design is frozen for
implementation after it, and a change from here goes through §9 and the owner. Result: 17 accepted (N1 and N11 superseded by O1 in the way the
rows say), none rejected; §4 gained 28 `(new)` rows (19 `x<name>` stand-ins, `find-map`, `cancelled?`, `dbg`, `MArray`, `LSeq`, `Ratio`, `subs-from`,
`str/split-limit`, `str/index-of-from`) and recounts by `tbl.py` (keep 147, adapt 426, alias 54, omit 45; 76 `(new)` rows; survey names by
tranche 150, 84, 199, 113, 81); one `ran:` block (`fib.core`, the UTF-16 `cmp-u16`) was withdrawn with its design, and every other `ran:` block's
program was re-run (A13).

| Finding | Verdict | What changed | Evidence |
|---|---|---|---|
| N1 K1's residual claim is false: `every?`, `some`, `not-any?` walk to the last element, `empty?` over a `filter` re-runs the prefix; the `FnOnce` closure rule needs a kind types §1.4 excludes | accepted, then superseded by O1 | the programs reproduce (`[10 8]`, `[1 8]` where Clojure makes 4 calls); the affine design is deleted with the owner's decision: §1.1, §1.2 P2 and NG2, §2.1 rule 2, §5.7, §7 L13 (withdrawn), §9 Q34 | A13 ev1, ev2 |
| N2 rule 8's "any other `Reducible` the memoised `LSeq`" needs a blanket instance, which is rejected | accepted | one `impl` per head, listed in §2.1 rule 8 and the §2.3 `Seqable` row; the instances for `Map`, `Set`, `Array`, `Option` run and the generic `len` is `[2 3 2 1 2 5]` | A13 blanket (`an instance head must not be a type variable`), blanket2 (`no implementation of Seqable for (Map i64 i64)`), seqable |
| N3 what `/` is under a `Num` bound is unsaid; `quot` admits floats; `Int`/`Float` constraints undefined | accepted, settled by O2 | `/` leaves `Num` for `(Div a r)`, whose instance determines the result type, so generic code has one answer; `quot` is the builtin and truncates on floats as Clojure's; `Bits t` is the integer constraint; §2.8, §2.3, §4.1, §4.3, §5 D4, T5, S17, §7 L30, §7.4 | A13 div (`[7/2 2 -3/2 1/2 0.5 1/2]`, `[3 3.0 1 1.5]`), quot (`3.75`, `3`, `0`) |
| N4 the owner's float-text decision is an unrecorded deviation | accepted, settled by O4 | the owner follows Clojure's text: §2.7, §5 S18, §7 C12, §7.4, §8.2 tranche 0 | A13 fl, fltfmt |
| N5 §7.4 omits the decided facts C1, L21, L24, L25 overturn | accepted | §7.4 rows for types §1.7 (coercion) and §1.4 (closure types); §1.1 says the T-rows stand until the owner signs those items | the texts of types §1.4 and §1.7, read |
| N6 `Ord str` in UTF-16 order is a runtime change with no §7 item | accepted, settled by O3 | code-point order, the builtin's: no runtime change; the `cmp-u16` block, the §7.4 row and the tranche-1 item are withdrawn; §2.7, §2.9, §4.3 `compare`, §5 D5, §5.6, T10 | A13 ordstr (`true true false true`) |
| N7 items labelled decided or taken that the owner did not decide | accepted | Q14 is a recommendation (§9.2, §10.4.3), L23 is recommended (§7, §8.2, §9 Q39), Q22 is recommended (§4.2, §9.2, §10.4.3), Q18 reads "to be decided"; §10.4.7's counts: decided 5, kept 110 | the owner's decisions are L16, L19, Q15, Q28 and, on 2026-10-01, O1 to O4 |
| N8 a `def` of an `LSeq` or a `Delay` is forbidden by M1 and promised by §2.1 | accepted | M1 lists every type that holds a `Cell` (`LSeq`, `Delay`, `MArray`, `Matcher`); a global lazy value waits for C9's run-once cell, recommended for tranche 3 (§9.2 Q41); §2.1 rule 2, §2.11, §4.15 `delay`, §5 S10, §7 L15, C9 | A13 def1, def2 (`initialiser is not a constant expression`) |
| N9 §1.1 generalises ownership.md's leak rule | accepted | a leaked *cycle through cells* is the one leak the model allows; the audit counts other leaks as errors; §1.1, §2.10 | `crates/fibref/src/cases/evaluator.rs`, read |
| N10 M1 has no program; M2 covers scalar arrays with no release | accepted | M1 is `[sketch]` until L15 (its program cannot be written today); M2 is the object case, and the scalar case is T11 (`(Cell T)` is never `Send`) | A13 def1, def2; A12 aset3 |
| N11 affine recipes: unspecified interactions (`def`, `seq`, `cache`, `not-empty`) | superseded by O1 | no affine rule remains; `cache` is not a row (a bound seq is memoised by default); `seq` and `not-empty` notes say what they do on an `LSeq` | A13 lz1 |
| N12 stale or contradictory text | accepted | `into` cites S4; `format` checks a non-literal at run time; N12 no longer calls `aget aset alength aclone` aliases; P8's list is complete; `repeatedly` is tranche 2; §6.2 puts `group-by`/`frequencies` with `seq.fib` and `vec`/`into` with the trie; §10.4.7's adaptor row is marked superseded; xoshiro256** state; the prelude is 504 lines at HEAD | `wc -l lib/prelude.fib`, `git show HEAD:lib/prelude.fib` |
| N13 signatures that contradict §2 or themselves | accepted | `partition*` yield `(LSeq (VSeq e))` and the `v` forms `(LSeq (Vec e))`; `split-at`/`split-with` fill two `Vec`s; the typed arrays name their element type; `ident?`'s family is `a -> bool` over keywords and `Form` symbols; `re-find` is not a `Pattern` user; `map`/3 and `into`/3 have signatures | |
| N14 names the plan uses with no row | accepted | rows for `find-map`, the 19 `x<name>` stand-ins, `cancelled?`, `dbg`, `MArray` (with `Reducible Seqable Cursable Lookup Keyed` in §2.3); `cache` is not added (O1) | A13 marray (`[(some 1) nil]`, `[true false]`) |
| N15 tranche needs: `Pattern` in tranche 4 under tranche-1 rows; two-arity tranche-1 rows without stand-ins | accepted | `Pattern`'s `str` and `char` instances are tranche 1 (`Regex` tranche 4); `subs-from`, `str/split-limit`, `str/index-of-from` (new, tranche 1); `(str/join c)` is `(str/join "" c)` and `(gensym)` lands with L1 | |
| N16 `conj` on seq results has no instance | accepted | `Collection s r e` with the result type determined by the instance: `(conj (range 3) 9)` is `(9 0 1 2)`, `into` requires `r = s` | A13 conjseq |
| N17 one-argument collisions in `max-key`, `min-key`, `distinct?` | accepted | Clojure's varargs forms, `(max-key k x)` is `x` and `(distinct? x)` `true`; the collection forms are `(apply max-key k c)` and `(apply distinct? c)` in `apply`'s table | |
| O1 the owner: lazy sequences follow Clojure | **Decided** | sequence functions return memoised lazy seqs (`LSeq`); the zero-cost loop is an optimisation under the syntactic fusion rule of §2.1 rule 2 (sets **A** and **T**, the rewrite **R**/**F**, its soundness, its one latitude, its cost); the affine design is deleted; `LSeq`, `lazy-seq` and the fusion rewrite (E16) are tranche 1; §1.1, §1.2, §2.1, §2.3, §4.4, §5.5 S14, §5.7, §6.2, §6.6, §7, §8.2, §9.1 Q34 | A13 lz1 (`[12 12 3]`, `0`/`3`, `[8 2]`, `[0 2]` with 2 calls, `[0 0 3]`), lz0/lz2/lz3 (+5 against +17526 objects) |
| O2 the owner: `Ratio` | **Decided** | integer `/` is a `(Ratio t)`; the `Div` protocol with a determined result type settles generic code; `quot`, `rem`, `mod` truncate as Clojure's; 64-bit components trap, `(Ratio BigInt)` is tranche 5; the ratio literal with E14 (a); §2.8, §4, §5 D4, §7 L30, §7.4, §8.2, §9.1 Q40 | A13 div |
| O3 the owner (invited): strings order by code point | **Decided**, the owner may overrule | the builtin order stays; `cmp-u16` and its runtime item are withdrawn; §2.7, §2.9, §4.3, §5 D5, §5.6, §9.1 Q42 | A13 ordstr |
| O4 the owner: floats print as Clojure's | **Decided** | `Double.toString`'s text in both tools (C12, tranche 0), amending types §2.12 of 2026-09-30; the §7.4 row names `float_text`, `fib.show-fp`, cases 169/178/187 and the reader dump; §2.7, §5 S18, §7 C12, §7.4, §8.2, §9.1 Q43 | A13 fltfmt (the fifteen texts), fl |

## Appendix A. Evidence

Programs run in the session that wrote this page, with the compiler binaries of that session
(`fibc`, `fibref`, `lair`), each one at a time. They are in the design record (`stdlib/` of the
session's scratch directory, **not in the repository**), so the programs that carry a claim are
quoted here. Counts of heap objects are the `A` lines of `FIB_TRACE=1`; indirect calls, retains and
releases are counted per function in the output of `fibc emit`. **They are the state of `36ed472`**: where a later item has landed (§7.5) the output quoted is the old one, and
the bullets that name `Entry`, `derive Debug`, the printed form of an `Option`, the `List` variants `empty` and `cons`, `/`, `if-let` or `@f` of a task describe what the tool said then.

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
* **twin.** `(defun str (x: a) :where ((Show a)) -> str (show x))` and the same for `println` (over `Show` there; the page's `str` twin is over `ToStr`, §2.7): `(run! println [1 2 3])`
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
* **dd.** (at `36ed472`) `(derive Debug P)`: `cannot derive Debug: only Eq, Ord, Hash and Show` (since R8, `605a26e`, `derive Debug` and `derive ToStr` work and the error lists six protocols);
  `(= (P 1 "x") (P 1 "x"))` with no derive: `no implementation of Eq for P` (still so); `(derive Show P)` shows `(P 1 x)` (still so), `(show (some 3))` `(some 3)` (since R2 it is `3`), a `nil` `nil`.
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
| seq1 (p/seq1.fib; superseded by A12 vseq, which has `VSeq` for `Slice`) | `Seqable` with `seq` and `rest` over `Vec`, `Slice`, `List`; the generic `len` of §2.1 rule 8; `next` | `4`, `3`, `0`, result 0 |
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
| t41, t54, t90 | two tasks writing one `Cell`; `array-set!` with a second holder; `(array 3)` | `cell cannot be shared between threads: closure capture c has type (Cell i64)`; 9 (Clojure: 99); `array takes 2 argument(s), got 1` (the value-semantics reason that t54 served is withdrawn: A12 aset1, aset2; §10.4.8 R1) |
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

**A12. Evidence of the second revision (the rule check's 28 findings).** Programs run with `fibc run` and then `fibref run` (the binaries of the second
revision, `bin3`), one process at a time, `timeout 300`; their directory is `stdlib/rrevise2/` of the session's scratch directory, **not in the repository**, with the
A10 prototype `sl/` unchanged (`rc/` holds the rule check's own programs, which were re-run and not trusted). Unless an entry says otherwise the output is the same under both
tools and `fibref` ends `audit:  clean=true leak-cycles=0 leaks=0 errors=0`. Before the edits `t1r` (result 0), `seq1` (`4 3 0`), `t10` (result 0), `e8c` (`12 12 3`) and the rule check's `q_slice` and
`q_sortconj` were re-run.

| Tag | Program | Output |
|---|---|---|
| p1, p10, p11 | two closures that share one `Cell`; an array in a shared cell written by one closure with `(array-set! &a i x)` and read by the other; an `&a` parameter over a `(cell (array 3 0))` variable | `3`; `49`; `9` |
| p12 | `volatile!` as an `Atom`, two tasks each doing 300 `(reset! v (+ @v 1))` | `1`: the result lies in 1 to 600 and the audit is clean |
| p3 | `locking` as a macro over a lock from two `Atom`s (a ticket lock), two tasks, 300 `(locking lk (reset! n (+ @n 1)))` each | `600` |
| p2, p2b | Clojure's UTF-16 `count` and `subs` as user code over `str-chars`; a bound inside a pair | `4`, `a😀`, `z`, `3`, result 4; with the bound inside the pair `trap: subs: index splits a surrogate pair` (`trapped:`, `audit: clean at the abort`) |
| unit | `(units-len "a😀z")` against `(count (str-chars s))`, and `(dotimes [i n] (nth cs i))` with each | `[4 3]`, `128731`, then `trap: nth: index out of range` for the UTF-16 count |
| u16cmp | `cmp-u16`, the byte loop with the fix-up, against an explicit UTF-16 encoding, over 72 strings of 8 code points (all 5184 pairs); and the plain byte order | `[5184 0 1068]` (pairs, disagreements of `cmp-u16`, disagreements of the byte order); `(cmp-u16 "😀" "\uFFFF")` is `-1`, the byte order `1`; result 0 |
| aff1, aff2 | a non-Copy struct passed to a function twice (a borrowed parameter; a stored, owned one); `fibref explain` | `6` and `6` under both tools; `explain`: `arg 1 w: borrow` twice; `params: w owned (rule 1: stored)` and `arg 1 w: retain` twice. `:owned` on a `defun` parameter is `:owned is not a parameter` |
| aff3 | a recipe with a flag cell (`OnceMapped`) traversed once, then `vec`, then a second time | `12`, `3`, `[12 3 3]`, then `trap: recipe already consumed` (`trapped:` under `fibref`, `audit: clean at the abort`) |
| q34 | a counting `map`, one mention in a loop body and one in a closure called three times | `[6]`, `[6]`: Clojure's cached seq makes 2 calls in each |
| q_slice, q_sortconj | the first rewrite's one `Slice` for `rest` and `subvec`; `sort` returning a `Vec` | `[2 3 0]` and `[2 3 0]` where Clojure has `(0 2 3)` and `[2 3 0]`; `[1 2 3 0]` and `[1 2 3]` where Clojure has `(0 1 2 3)` and `(1 2 3)` |
| vseq | `VSeq` and `SubVec`: `(conj2 (rest v) 0)`, `(conj2 (subvec v 1 3) 0)`, `(conj2 (sort-seq [3 1 2]) 0)`, `(sort-seq [3 1 2])`, `len` at `Vec` and `VSeq`, `to-vec` | `(0 2 3)`, `[2 3 0]`, `(0 1 2 3)`, `(1 2 3)`, `[3 2 0 3]`, `[2 3]`, `[true true true]`, result 0 |
| sortby | `sort-by` as `(sort-with (fn (a b) (cmp (f a) (f b))))` against `sort-by-with`, a counting key over 5 elements | `[18 5]` calls, both results `[1 2 3 4 5]` |
| aset1, aset2 | `(Cell (Array i64))` handles; an `(MArray t)` struct with `aget`, `aset`, `alength` and `Reducible`, shared by a second binding and a closure, over `i64` and `str` | `49`; `[9 4 0]`, `[y x]`, `13` |
| aset3 | two tasks that each `aset` one `MArray` | `cell cannot be shared between threads: closure capture a, field c of MArray has type (Cell (Array i64))` |
| ratio | `(Ratio t)` over `i64` and `i32` with `rdiv` and the numeric protocol | `[7/2 2 -3/2 -7/2]`, `5/6`, `7/2`, `3` for the builtin `/`, then `trap: integer overflow in * at i64` for `1/3037000500 + 1/3037000501` (`trapped:`, clean at the abort) |
| xf2 | nine transducers in the Stepper form (`xremove xtake-while xdrop-while xtake-nth xmap-indexed xdedupe xinterpose xpartition-by xdistinct`) and a composition, ten checks | result 0; with one expectation edited `FAIL remove` and result 1 |
| chunk | `map` over a chunked source with a 32-slot buffer (`cmap`), counting the calls of `f` for `take` 1, 32, 33, 40 over `(range 0 100)`, 1 over `(range 0 10)`, 5 over `(range 0 0)` | `[32 32 64 64 10 0]` |
| keyed | `Keyed` and `Lookup` for `str` and `Array` | `[true false]`, `(some c)`, `[true false]`, `nil` |
| int1, int2, pred1 | `(- (int \a) (int \0))` today; `ToInt` and `ToLong` instances for `char`; `(string? "a")` today | `unbound name int`; `104`; `unbound name string?` |

**A13. Evidence of the third revision (the second rule check's 17 findings and the owner's four decisions).** Programs run with `fibc run`
and then `fibref run` (the binaries of `bin3`), one process at a time, `timeout 300`; their directory is `stdlib/rrevise3/` of the session's scratch
directory, **not in the repository**, with the A10 prototype `sl/` unchanged beside a new module `lz/lazy.fib` (the lazy seq); the rule check's own
programs (`stdlib/rcheck2/`) were copied and re-run, not trusted. Unless an entry says otherwise the output is the same under both tools and `fibref`
ends `audit:  clean=true leak-cycles=0 leaks=0 errors=0`. Object counts are the last `A` line of `fibc run --trace`.

| Tag | Program | Output |
|---|---|---|
| lz1 | `lz/lazy.fib`: `LSeq` as a node realised once by a thunk in one cell (`Pending`, `Forcing`, `Ready`), `Reducible` over it, `lseq-vec`, `lseq-iterate`, `lseq-of`, and `lmap`, `lfilter`, `ltake` written as Clojure writes them; `lz1.fib`: the macros `fuse` and `fuse-arg` (the rewrite of §2.1 rule 2 on one form) and seven checks | `[12 12 3]` (a bound `map` summed twice: `f` once per element), `0` then `[0 2 4]` then `3` (nothing runs until demanded; an infinite `iterate`), `[true false]` (`realized?`), `[(some 10) (some 12) nil]`, `(10 12)`, `[8 2]` (a fused `reduce` over `lmap` and `lfilter`: 2 calls), `[0 2]` then `2` (a fused `vec` of `ltake 2` over an infinite source: 2 calls, no chunk, no seq), `[0 0 3]` (a bound seq read by two fused chains: realised once); result 0 |
| lz0, lz2, lz3 | the input of A1 only; the fused chain of A1 written as recipes; the same chain bound to a name as a lazy seq | `A 3095` (1000 elements); `A 3100`, result 166167000 (+5); `A 20621`, result 166167000 (+17526) |
| seqable | `Seqable` with one impl per head for `LSeq`, `Vec`, `Map`, `Set`, `Array`, `Option`, the generic `len` over each | `[2 3 2 1 2 5]`, `(some 20)`, `0` |
| blanket, blanket2 | a blanket `(impl (Seqable (LS e)) c :where ((Reducible c e)) ..)`; `(seq {1 2 3 4})` with instances for `Vec` only | `an instance head must not be a type variable`; `no implementation of Seqable for (Map i64 i64)` |
| conjseq | `Coll2 s r e` with the result determined by the instance: `Vec` to itself, `LSeq` and `Range` to an `LSeq`; `into2` requiring `r = t` | `(0 2 3)`, `(9 0 1 2)`, `[1 2 3]`, `[0 1 2]`, `[0 1 2]` |
| marray | `(MArray t)` with `Lookup` and `Keyed` by index; `(int-array [1 2])` | `[(some 1) nil]`, `[true false]` |
| div | `(Ratio t)` with `Num Eq Ord Hash Show`; `(Div a r)` with instances for `i64`, `i32`, `f64`, `(Ratio t)`; a generic `half` over `Div`; `mean`; `ratio?` by the denominator; `fquot` (the truncated float quotient) | `[7/2 2 -3/2 1/2 0.5 1/2]`, `5/2`, `[true false]`, `[3 2]`, `0.25`, `[true true true]`, `[3 3.0 1 1.5]` (the builtin `/`, `fquot`, `rem` on integers and on floats), `7/2` |
| quot | the rule check's program: a generic `quot` and `half` over `Num t` with the builtin `/` | `3.75`, `3`, `0` |
| fltfmt | `clj-double-str`, Java's `Double.toString` rule over today's `show`, on fifteen values | `1.0E21`, `1.0E-7`, `1.0E7`, `1.23456789E7`, `0.001`, `1.0E-4`, `100.0`, `9999999.0`, `-0.0`, `Infinity`, `-Infinity`, `NaN`, `-1.23456789E9`, `1.5`, `0.1` |
| fl | `show` of `1e21`, `0.0000001`, `(/ 1.0 0.0)`, `(/ 0.0 0.0)`, `1e7` today | `1000000000000000000000.0`, `0.0000001`, `inf`, `NaN`, `10000000.0` |
| ordstr | the one-character strings U+FFFF and U+1F600, and the characters | `true` (`(< a b)`), `true` (the chars), `false` (`(< b a)`), `true` (`(< "a" "b")`) |
| ev1, ev2 | the rule check's programs: a counting `map` peeked by `every?` then reduced; a counting `filter` peeked by `empty?` then counted | `true` then `[10 8]`; `[1 8]` (Clojure's cached seq makes 4 calls in each) |
| def1, def2 | `(def c: (Cell i64) (cell 0))`; `(def a: (Atom i64) (atom 0))` | `def c: initialiser is not a constant expression`; `def a: initialiser is not a constant expression` |
| the `ran:` blocks | `fib.seq` (A10 t1r), `fib.seq.cursor` (A11 e16), `fib.xf` (A12 t10, xf2), `fib.seqable` (A12 vseq), `fib.coll` (A10 t1r, A11 e6, A12 keyed), `fib.cmp` (A11 t17), `fib.truthy` (A11 t04, some), `fib.array` (A12 aset2), re-run from their own directories | t1r 0; e16 `[[1 2] [2 3] [3 4] [4 5] [5 6]]`, `[[2 1] [3 2] [4 3] [5 4] [6 5]]`, `[1 1 1 1]`, `[[1 1] [2 3] [3 5]]`; t10 0; xf2 0; vseq `(0 2 3)`, `[2 3 0]`, `(0 1 2 3)`, `(1 2 3)`, `[3 2 0 3]`, `[2 3]`, `[true true true]`; e6 `true`, `false`, `[10 20 30]`, `[30 20]`, `trap: assoc: index out of range`; keyed `[true false]`, `(some c)`, `[true false]`, `nil`; t17 0; t04 127; some `(some true)`, `(some 20)`, `nil`, `(some 20)`, `(some true)`; aset2 `[9 4 0]`, `[y x]`, `13` |

The third revision withdrew one `ran:` block, `fib.core` (A12 u16cmp, the UTF-16 `cmp-u16`), with the design it served (§5 D5); the twelve
blocks that remain are unchanged in text, and the program behind each was re-run above. The `fib.seqable` block's `Coll2`/`conj2` now stand for the
three-parameter `Collection s r e` of §2.3 (A13 conjseq is the program for the determined result type); the block itself ran unchanged.
