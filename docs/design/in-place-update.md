# In-place update when uniquely held

Status: written as a design (performance batch 1, lever C); levers L1 (last use), L2 (the primitives), L3 (Vec), L4 (Map/Set) and
L6 (`update!`) have since landed (commits `b016d22`, `71faf5e`, `ca8553e`, `f6a113b`, `a50cb1d`, `28d715c`, `17473a1`); L5 (shell reuse) and the
`array-with` change are not in the tree. The sections below are the design as written; where they differ from the code, the code
is the truth, and the specification of what landed is spec/types.md §2.13.1 and spec/stdlib.md §2.5. Everything marked "measured" was run in this
session with the released stage 2 (`~/.cache/fibber-scratch/PERF0/fibc`); everything else is a proposal and a claim about
a future change, to be proved by the tests in section 6.

The owner's line (stdlib.md P3, 2026-10-03): *in place when unique, eager over lazy once use is known, trust LLVM, measure
first*. The API stays persistent. A value that nobody else holds is updated where it lies; a shared value is copied, as
today. The result of every program is the same; only the number of allocations and copies changes.

## 0. What is measured today

`FIB_TRACE` allocation lines (`fibc run --trace`, count of `A` lines), programs in `~/.cache/fibber-scratch/PERFB-C/`:

| Program | Allocations |
|---|---|
| `a.fib`: `(vec (range 100000))`, then 1000 `assoc` in a `loop` on the unique vector | 15479 in all (the build is not 1000-dependent; stdlib.md §2.5 says about 7 objects per `assoc` of a deep path) |
| `b.fib`: 1000 `conj` on `[]` in a `loop` | 2097 (2.09 per `conj`: one new tail array, one new `VecOf`) |

So the figure of stdlib.md §2.5 still holds at `de1c007`: **no update is in place**, though every vector in those loops is
held by exactly one variable. The profile (ROADMAP "Performance", item 2) puts 80% of vec-assoc, 94% of vec-conj-pop
(`array-alloc` 59.7% and `array-slice` 34.3% inclusive) and half of nbody and set-conj in path copying and the release of
the superseded path.

The runtime already has the primitive that decides: `fib.unique?` (`rt/core.lir:334`: no `SHARED`, `HAS-WEAK`,
`STACK`, `IMMORTAL` flag and count exactly 1). It is used by `array-set!` and `set-field!` on a **cell** (the
"unique-write protocol", `compiler/emit/lower/cells.fib` `lcx-unique-write`, `lower/builtins.fib` `array-set`): in place when
unique, else copy, write, store the copy, release the old. A struct or array held by a `let` cell alone already updates with
zero allocations (stdlib.md §2.5, [R] A10 own). The work is to make the *library's own* update paths reach that case.

## 1. "Uniquely held": the runtime fact and the static fact

**Runtime.** Unique means count 1 and none of the four flags *at the moment of the update*, and the update **consumes** the
reference it tests. This is `Rc::make_mut` / `Arc::make_mut`: if the one count belongs to me and I hand it to the update,
nobody else can see the object, so a write is unobservable. If the count is 2 or more, someone else may read the object
later, so copy. The check is sound by construction and needs no static proof: a wrong static guess can only make an
update *copy* (slow), never mutate a shared value (wrong). That asymmetry is why the owner's "trust LLVM, accept runtime
checks" is the right first choice.

**Static.** The ownership pass can prove "Owned and dead after the call". It does not need to *prove uniqueness*; it only
has to **stop manufacturing counts**. Measured causes of count 2 at the update (stdlib.md §2.5, `fibref explain`; I
re-ran `explain` on `b.fib`: bindings `v owns`):

1. A variable's last use is `retain` then a release at scope exit or at `recur`, never a move (types §6.4: "a move at a
   store would be sound only as the parameter's last use on every path ... flow-sensitive reasoning the checker does not do").
   This is C2 in stdlib.md §7.
2. A field of an owned shell is derived of the shell; consuming it retains it (count 2 for as long as the shell lives). C7.
3. `&` copy-in acquires (types §6.6). C3.
4. `array-get` returns a retained element: the child of a unique array has count 2 when read. (Since performance batch 4, lever B, an element
   read of a borrowed or derived array is a part of it and takes no count until it reaches an owned position: spec/types.md §6.3, "Element reads".)
5. `@c` on a cell is an acquire.

**Decision: runtime check first and always; static analysis only to remove 1, 2 and 4.** No analysis result is ever trusted
to license a write; it only removes redundant retains so that the runtime test can succeed. Concretely the static work is
one new fact, **last use** (a variable, or a pattern-bound field, is not read on any path after this point, loops
included), consumed in two places:

- **move at last use**: `consume` of an owning binding at its last use emits no retain and no later release (C2); a loop
  variable's old value is moved into the call before `recur` (the `release [acc] (old loop value)` of `explain` goes away);
- **steal at last use** (C7, below): the consume of a pattern-bound field of an owning scrutinee, at that field's last use,
  when the scrutinee is dead afterwards.

### Steal (C7 made dynamic)

stdlib.md C7 says "a `match` on an owned shell that is dead afterwards moves the payload out". Moving out statically is
wrong when the shell is shared (the field then belongs to two holders). The runtime form:

```
x is a pattern variable bound to field f of owning scrutinee o, consumed at its last use, o not read afterwards
  if fib.unique?(o):  t = o.f ; o.f = null            ; no count change: the count moves to t
  else:               t = o.f ; retain t              ; as today
```

`o` stays alive until its scope exit and is released there (`fib.release` and every `drop.N` already tolerate a null
child: `fib.release` starts `(br (icmp eq p (ptr null)) done check)`). Other variables derived of `o` stay valid because the
shell is not freed early. If `o` is shared, behaviour is exactly today's. If `o` is unique but the field has count 2
(another version shares the tail), `t` has count 2 and the next update copies it: still persistent. Cost in the shared
case: one flag/count test. **Static side condition (the only soundness obligation of the whole design):** after the steal
point there is no read of `o.f` through any alias: `o` is not mentioned again (no second projection, no `:as` alias, no
capture by a closure, no `&o`), and the point is not inside a loop body for an `o` bound outside the loop. A violation reads
a null pointer, so this one rule gets the strictest tests (section 6, T7).

## 2. Primitives

New builtins sit in three places: the type table (`compiler/types/builtins.fib`, rows `BuiltinSig name sig bounds escapes
unsafe-only`), the emitter (`compiler/emit/lower/builtins.fib`, `builtin-array`; the names that are array builtins are
listed in `array-name?` and in `compiler/emit/lower/values.fib:93`), and the runtime (`rt/array.lir`; then
regenerate `compiler/emit/runtime.fib` with `compiler/tests/emit/gen-runtime.fib`, and `compiler/tests/emit/runtime.sh`
must pass). They follow the existing cell idiom, so no closure is called and nothing new is passed by value:

| Builtin | Signature (types text) | Escapes | Meaning |
|---|---|---|---|
| `array-take!` | `(fn ((& (Array a)) i64) a)` | `[EscInOut EscScalar]` | bounds-check `i`; if the cell's array is unique, return element `i` **without a retain** and store null in the slot; else copy the array as `array-set!` does (store the copy in the cell, release the old), then do the same on the copy. The slot must be refilled by `array-set!` before the array is read at `i` again |
| `array-push!` | `(fn ((& (Array a)) a) unit)` | `[EscInOut EscStore]` | append; in place when the array is unique **and** has room (below); else a fresh array with room 32 and the elements retained |
| `array-pop!` | `(fn ((& (Array a))) a)` | `[EscInOut]` | remove and return the last element (traps on empty); in place (length minus one, the slot moved out) when unique; else copy of length minus one, with the old last element retained |
| `array-set!` | unchanged | | already unique-aware |
| `array-with` | unchanged name and type | first escape `EscBorrow` to `EscStore` in the fibber table | optional step: the arg is consumed, and when unique it is written in place and returned. Not needed once the library uses the cell idiom, so left out of the first wave |

`array-take!` is what makes a trie update in place. Without it, `(array-with kids sub (vnode-assoc (array-get kids sub) ...))`
reads the child with count 2 (cause 4) and every level below the first copies. With it, one level is:

```
(let ((c (cell kids)))                       ; kids moved in at its last use: count 1, the cell alone holds it
  (let ((kid (array-take! &c sub)))          ; unique: the child's count moves out, still 1
    (do (array-set! &c sub (vnode-assoc kid (- level 5) i x))   ; in place; the slot's null is overwritten, nothing to release
        @c)))                                ; @c acquires, the cell releases at scope exit: array returned with count 1
```

**Capacity for `array-push!`.** The array header is `(hdr 16 bytes, len i64)` = 24 bytes and `rt/array.lir`,
`rt/vec.lir`, `rt/vecbuild.lir`, `lower/pattern.rs` and `fibref eval/vecs.rs` all assume it. A capacity word would be a
layout change in all of them. Instead: a **header flag bit 4, `ROOMY`** (`fib.hdr` flags have bits 0..3; `fib.unique?` masks
15 so bit 4 does not disturb it; `fib.flags` is already an atomic load). `ROOMY` means "allocated with room for 32
elements, `len` of them used". It is set only by `array-push!`'s growth path (`malloc(24 + 32*esize)`, `len` = used);
every other producer (`array-alloc`, `array-slice`, `fib.vec-drop`, the bulk builder) leaves it clear, so for them
`array-push!` allocates. `free` needs no size. The cost is room for 32 elements (256 bytes for 8-byte elements) per
tail array instead of `len*esize`; the tail array is already re-allocated on every `conj` today. The tail never grows past 32, so a
fixed 32 is the whole rule; a general capacity (doubling) is not needed and not proposed.

**Tail layout is unchanged.** `tailoff = cnt - (array-len tail)` in `vec-nth`, `fib.vec-elem-ptr`, `fib.vec-drop` and
`eval/vecs.rs` stays true: `len` of the tail is the fill. (A padded 32-slot tail with a count-derived fill would avoid the
flag but changes all five readers; rejected.)

**Type table text.** `(& (Array a))` marks an `&` position as `array-set!` does (`"(fn ((& (Array a)) i64 a) unit)"`).
`array-take!`'s result is `Owned` (a retained-or-moved element: types §6.3 row "call of ... primitive returning an object").
In the emitter the arm reads the cell content with `cell-load`, tests `fib.unique?`, and the two branches share a join;
this is `array-set` with a load instead of a store, and the copy branch is `array-set-copy`.

## 3. How the library uses them (persistence for shared values is unchanged)

The only change to a library function is **which builtin it calls**; its type and its meaning do not change. Every use is
the cell idiom, so a shared argument takes the copy branch of the same primitive and yields what `array-with` yields.

- `vnode-assoc`, `vnode-push`, `vnode-pop` (`lib/prelude.fib` 82-190): the path from root to leaf, one cell per level as
  above. Unique path: **zero allocations** except the final shell. Shared path: the same copies as today (the first
  `array-take!` of a shared array copies it, and every level below is then also shared, so also copied).
- `vec-assoc`, `vec-conj`, `vec-pop`: match the `VecOf` shell with steal on `tail` and `root`, update, build the new `VecOf`.
  `(array-with tail i x)` becomes `array-take!` plus `array-set!` on a cell holding the stolen tail; `array-push tail x`
  becomes `array-push!` on a cell holding the stolen tail (when the tail has 32 elements the existing leaf-promotion path
  is unchanged: the full tail becomes a leaf, a fresh `(array 1 x)` is made: that `array 1` could carry `ROOMY` through a
  fourth builtin `array-room`, left as a refinement). `vec-pop`'s `(array-copy tail 0 (- t 1))` becomes `array-pop!`.
- `mnode-assoc` (existing key: `MColl` `array-with`, `MBits` `array-with kids idx`) and `mnode-dissoc`: the same cell
  idiom on `kids` and `items`. **Inserting** a new key changes the array length (`array-insert`, `array-remove`, loops of
  `array-set!` over a fresh array): those stay copies in this design; only an update of an existing key and the descent are
  in place. Set `conj` of a new element is therefore a copy at the one level where the key is added and in place above it.
- `Map`/`Set` `MRes` result struct (`mnode-assoc` returns a fresh `MRes` per level, an allocation each): in place does not
  remove it. Returning the "added" flag by a cell instead of a struct is a separate small refinement of lever L4.
- Cells `(Cell (Vec a))` and the common `(set! c (conj @c x))` idiom: **not in place in this design** (cause 5: `@c`
  acquires, count 2). It is not fixable by the steal rule because `f` could read the cell while the content is moved out.
  A later `update!` form that takes the content out of the cell only when the function argument is a literal that does not
  mention the cell (the syntactic test of types §6.6 "mentions") would; it is its own lever (L6). Programs that need it
  now use `loop`/`recur`, which is what vec-assoc and vec-conj-pop do.

Semantics: no signature, no row of the stdlib table and no result changes. The shared case is the existing code path
running inside the primitive.

## 4. Interactions

- **Borrowed parameters** (default): never consumed, so never updated in place; a `defun` whose parameter feeds an
  update (`vec-conj v x`) is inferred owned by the existing rule ("stored, escapes"), which is what we need and is checked
  by `explain`. Callers passing a variable that is used again retain it first (count 2): the callee copies; correct.
- **In/out `&` parameters** (`push! &v`, `map-put! &m`): copy-in acquires, so the first write copies, and the copy is then
  held by the private cell alone (unique from then on). That is one copy per call, not per element. Fixing it is C3
  ("exclusive `&`: when `&x` is the only mention of `x`, share without acquiring"), a lever of its own (L1b); not needed
  for `loop`-shaped code.
- **Cells and atoms**: a `let` cell alone already updates in place; `@c` acquires (section 3). An **atom** holds a value
  that is `Send`, and everything it holds was made `SHARED` when stored into an atom (types §6.3 line 1530: "marked before the store"; `fib.share`, `rt/atom.lir`): `fib.unique?` is false on a
  `SHARED` object, so a value read from an atom is persistent and every update copies; the copy is a fresh, unshared object
  that the updater owns and can then update in place. No change needed, none wanted.
- **Tasks / sendable values**: `spawn` marks its captures `SHARED` through the type table's `share.N` before the thread runs
  (compiler.md §3, `fib.share`), and counts of shared objects are atomic. Because `fib.unique?` refuses `SHARED`, a value
  that crossed to a task is never written in place by either side. Steal and `array-take!` test the same flag. The `ROOMY`
  flag is set only on arrays created by `array-push!` in the current thread and is cleared on nothing, so it adds nothing to
  the sharing state (a `SHARED` roomy array copies on push). Rule: **in place is single-owner; sharing across a thread
  disables it for that object permanently.** This costs the compiler-in-fibber nothing today (single-threaded), and the
  `join` result is `Owned` and unshared, so a task's finished vector can be updated in place by the joiner.
- **Weak references**: `HAS-WEAK` is in `fib.unique?`' mask: an object that a weak ever pointed to is copied.
- **Immortal and stack objects**: literals, `def` values and named-closure environments are `IMMORTAL`; `STACK` objects have
  no count: both are not unique, so `(assoc [1 2 3] 0 9)` on a literal copies.
- **Audit (`leaks=0`)**: in place allocates less and frees less; every freed object is still freed by the last release. The
  steal leaves a null field and the release of the shell skips it. Required: `audit: clean=true leaks=0` on every case
  that passes today, and a new case whose body traps with a stolen slot outstanding must show no double free (T8). The audit
  counts (`allocs`, a maximum) go down only; they are not equalities.
- **fibref and the frozen Rust (decision for the lead).** The type table and `rt` are shared with Rust through the
  embedded `lib/prelude.fib` and `rt/*.lir`: `fibref` runs the prelude, and stage 1 `fibc` compiles it. A prelude that calls
  `array-take!` therefore needs the three builtins in `crates/fibref/src/types/builtins.rs` and `eval/arrays.rs` (the
  interpreter may implement them as the *copying* forms; its audit then differs from the compiler's only in allocation
  counts, which the harness checks as maxima), and in `fibc`'s Rust `lower/builtins.rs`. Rust is frozen (seed-1), so there
  are three ways, in order of preference:
  (a) a narrow, one-time exception: add the three builtins with copying semantics to fibref and the seed (size not measured; the interpreter
  forms are the copying ones, so no behaviour change for existing programs), so every tool still runs the same prelude; (b) stage the bootstrap: land
  the new builtins in the fibber compiler only, build F' from the old prelude, then switch the prelude, and let `fibref` and
  the seed stop running the post-switch library (they are already outside the stage check, c3b5ca5); this retires the
  interpreter as the stdlib oracle until it is updated; (c) leave the prelude alone and ship the new primitives in a second
  module (`fib.coll`'s Vec rewritten on top) — duplicates the library. This document recommends (a). Lever L2 below
  carries the choice. The same applies to a changed escape kind (`array-with` consuming) and to C2 and C7 in the Rust
  ownership pass: with them only in `compiler/own`, `fibref explain` and `fibc explain` disagree on counts for the same
  program; record it as `compiler/mirror-pending/last-use.md` (the file CLAUDE.md asks for).

## 5. Levers (independent; each lists the files it touches)

Ordered by payoff. Expected speed-ups are from the profile figures above and are **estimates to be measured**, not
results. Levers A and B of batch 1 (allocator fast path, library check once) overlap in effect with these: C removes the
traffic A speeds up, so A's gain shrinks where C lands, and B is orthogonal.

| Lever | Content | Files | Needs | Expected |
|---|---|---|---|---|
| **L7 tests first** | The persistence and count cases of section 6, registered as `open` (they print as failing) until the levers land; the differential generator extension | `cases/stdlib/` (new cases), `cases/ownership/` (count cases), `crates/fibgen` is frozen: use `compiler/tests` scripts and the case headers `allocs`/`covers`/`open` | nothing | none; gates the rest. Can start now |
| **L1 last-use analysis, move and steal** (C2, C7 dynamic; C3 as L1b) | One liveness fact in the ownership pass (variables and pattern fields, loops and `recur` included); `consume` at a last use moves; the steal rule of section 1 (emits a `fib.unique?` branch) | `compiler/own/facts.fib`, `classify.fib`, `taken.fib`, `walk/`, `dump.fib` (new fact in the dump), `compiler/emit/lower/` (consume sites, a `steal` lowering), `compiler/tests/own/compare.sh`, `compiler/mirror-pending/last-use.md`; spec rows types §6.3/§6.4 | none | on its own: no new in-place writes except `array-set!`/`set-field!` through a moved variable (sort, builders: small); it removes the retain/release pairs of every loop (a few %); it is the enabler of all rows below |
| **L2 primitives** | `array-take!`, `array-push!`, `array-pop!`, flag `ROOMY`; type-table rows; emitter arms; runtime functions; (decision (a)/(b)/(c) of section 4) | `rt/array.lir`, `compiler/emit/runtime.fib` (generated), `compiler/types/builtins.fib`, `compiler/emit/lower/builtins.fib`, `lower/values.fib` (name list), `spec/types.md` §8.2 (flag) and builtin table, `spec/stdlib.md`; Rust only under (a) | none | none alone |
| **L3 Vec** | `vnode-assoc/push/pop`, `vec-assoc/conj/pop` over L1+L2 | `lib/prelude.fib` (lines 76-215), users of the tail (none outside the prelude: `rt/vec.lir`, `vecbuild.lir` unchanged since the layout is) | L1 and L2 | **vec-assoc 3.36 s to about 0.5 s (6x)**: the 83% of path copy, release, drop, free and malloc becomes a three-level walk; **vec-conj-pop 1.64 s to about 0.7 s (2.3x)** (a `conj` still allocates one `VecOf`; the 32-slot array copy and its retain walk, 94% inclusive, go); nbody 1.38 s to about 0.6 s (five-slot tail `assoc`, 50.7% `array-slice`); vec-sort 0.92 s: the `rnd` builder by `conj`, 10-20% |
| **L4 Map/Set** | existing-key update and descent in `mnode-assoc/dissoc` over the cell idiom; flag instead of `MRes` | `lib/prelude.fib` (lines 395-500), `lib/fib/coll/map.fib`, `maps2.fib` if they call the nodes | L1 and L2 | set-conj 0.84 s to about 0.4 s when most keys are new (only the descent is in place, and the new-key level still copies: a smaller win than L3); map-assoc-get 1.68 s: assoc is half, expect 1.3x; set-disj is item 1 of the roadmap (independent) |
| **L5 shell reuse** | when the steal finds a unique shell and the clause builds a constructor of the same type, reuse the shell's memory for the result (a reuse token in `drop`/`alloc`): `conj` and `assoc` then allocate **zero** | `compiler/own`, `compiler/emit/lower/` (constructor sites), `rt/core.lir` (`fib.reuse`), `runtime.fib` | L1 | vec-conj-pop another 0.7 s to about 0.45 s; binary-trees and every enum-rewriting loop; fully removes the `VecOf` header allocation that §2.5 says an enum cannot avoid |
| **L6 cells and `update!`** | `(update! &c f)` and the `(set! c (f @c ..))` idiom taking the content out when `f` does not mention `c`; `with` on a unique record | `compiler/expand` (a macro) or a builtin, `compiler/own`, `compiler/emit/lower/cells.fib` | L1 | strings (`str` concat in a loop) and cell-held accumulators; not in the benchmarks above, no figure |

Dependencies: L7 and L1 and L2 are mutually independent and can run as three parallel agents in batch 2 (L2's
primitives can be tested alone through a `let` cell, which is unique today). L3 and L4 are written against the L2
signatures and the L1 rules and are tested after `scripts/batch.sh` integrates; they do not edit the same lines (L3
is `prelude.fib` lines 76-215, L4 lines 395-500). L5 and L6 follow. The batch rule (many changes, one build) holds: L1+L2+L3
is one build and one gate.

## 6. Tests that prove it

The test that must be able to fail is the **shared value is not mutated** family; it must fail under a mutant that makes
`fib.unique?` return true (the library `scripts` mutation runner; the owner's rule: mutation reviews run in the
background and do not gate, but a case that survives the mutant is a bad case and is rewritten).

- **T1 history kept.** Build versions in a `loop`, keep every one in a vector, then check all of them:
  `(loop [i 0 v v0 hist []] ... (recur (+ i 1) (assoc v k i) (conj hist v)))`, then for each `j`, `(nth (nth hist j) k)`
  equals the value written at step `j-1`. Each old version is shared (held by `hist`) when the next `assoc` runs, so it must
  copy. Same for `conj`, `pop`, `dissoc`, `disj`, `assoc` on a `Map`. **This case fails if any update writes a shared
  object.**
- **T2 two children of one parent**: `(let [a (conj base 1) b (conj a 2) c (conj a 3)] ...)`: `a` is used twice, so
  retained; check `b`, `c` and `a` independently. Include a parent with a *full* tail (32 elements) and a deep one (more
  than 1024), so both the tail path and the trie path are exercised.
- **T3 shared tail, unique shell**: `a = (vec ...)`, `b = (assoc a 0 x)` shares `a`'s trie below the copied path; then
  update `b` in a loop and check `a` is unchanged (the steal case where the shell is unique and the field has count 2).
- **T4 captures and structures**: a value captured by a heap closure, stored in a `Pair` or a map, held by a `weak`, a
  literal (`IMMORTAL`), a `def` constant: update each and read the original.
- **T5 tasks and atoms**: a vector in an `atom` updated by two tasks with `swap!`, and a vector passed to `spawn` and
  also kept by the parent: parent and child updates must not see each other. Run 200 times (flaky-looking races are the
  failure mode). `join`ed result updated in place: counts say so (T9).
- **T6 `&` parameters**: `(push! &v i)` loop and a `defun` that writes through `&` and returns: the argument variable's old
  value is observable until the write-back.
- **T7 steal safety**: a clause that reads the shell or another projection of the same field after the consuming use
  (`(match v ((VecOf c s r t) (let ((t2 (array-with t 0 x))) (vec-count v))))` and a read through an `:as` alias, a
  second `match` on `v`, a closure capturing `v`, a loop that reuses `v`); the analysis must decline to steal and the
  result must match the persistent semantics. `compiler/tests/own` compare script: the new fact appears in the dump
  exactly where expected, and nowhere else in the existing programs of `cases/ownership` and `cases/modules` (their emitted
  lIR must be byte-identical unless a case is listed as intentionally changed).
- **T8 audit**: every case of `cases/ownership` and the stdlib sample still `audit: clean=true leaks=0`; a trapping update
  with a stolen slot; a drop of an array that holds a null slot.
- **T9 counts (`allocs`, `covers`, `open` headers; stdlib.md §2.5)**: 1000 `assoc` on a unique 1e5 vector allocates at most
  c (not 7 per update); 1000 `conj` allocate at most n/32 + n (one shell each) before L5 and at most c after it
  (these two are in the `open` list today and print as failing, never pending); `pop` the same; a Map update of existing
  keys allocates at most c + depth. The same programs with the vector kept in `hist` allocate as many as before (the shared
  case did not get cheaper by accident, and did not get dearer).
- **Differential**: the Clojure oracle (real Clojure 1.12 jars; memory: reference_clojure_oracle) over random operation
  sequences on `Vec`, `Map` and `Set` **that keep old versions and re-read them**, independent of the author of the
  change (memory: token cost). `fibgen` is frozen: use the compiler's generator through the existing scripts or a
  `compiler/tests` driver.
- **Mutation**: `fib.unique?` forced true, and the steal's `fib.unique?` forced true: T1 to T5 must fail, with a case name
  per mutant recorded in the lever's note.
- **Benchmarks**: `scripts/bench/quick.sh` for vec-assoc, vec-conj-pop, set-conj, num-nbody, map-assoc-get, vec-sort (checksums
  unchanged: a changed checksum is a failure, not a speedup), baseline `scripts/bench/baseline.tsv`.
- **Self-host**: `fibc build compiler/fibc.fib ...` gives F, F builds F3, `F emit` equals `F3 emit` (the fixed point);
  the emitted lIR of existing programs is *intended* to change (fewer retains), so the byte comparison is replaced by: all
  case verdicts equal, the allocation counts not higher in any case, and each changed program listed.

## 7. Open points for the owner

1. The Rust exception of section 4 (a), (b) or (c). The design is unaffected; the cost and the loss of the interpreter as
   oracle are.
2. Whether `array-with` (existing name) also becomes consuming (section 2): cheap, but it adds a retain/release pair to every
   shared caller; leave it out unless profiles ask.
3. The `ROOMY` bit costs 256 bytes per tail array against about `8*len`; for a vector of a few elements that is waste.
   Alternative: allocate exact on the first push and `ROOMY` only from the 8th element. Measure first.
4. Whether `Vec` should become a struct (stdlib.md §9 Q18) is **not needed** here: L5 gets the same zero-allocation `conj`
   while keeping the enum, and does not change the layout the Rust tools share.
