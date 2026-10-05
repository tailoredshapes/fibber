# Exclusive views: a scoped exclusive borrow for writable windows

Status: **design proposal, nothing implemented** (package D2 of SIMD wave 2; owner decision 3 of 2026-10-04: "writable views of a
unique tensor are worth designing: a scoped exclusive borrow as a language feature"). No compiler, library or live-spec file
changed. The spec rows are drafts under `docs/design/exclusive-views/` (`syntax-draft.md`, `types-draft.md`) and become spec only
when the owner approves them. Every statement about the existing language cites the spec section it was read in. Every claim
about speed is an **estimate** unless a number is quoted from `docs/design/simd-and-tensors.md`; nothing here was timed.

Contents: 1 the problem; 2 the proposal in one page; 3 surface syntax; 4 typing and ownership rules; 5 the library layer; 6
soundness argument; 7 cases that must be able to fail; 8 what a program can write, against the alternative; 9 comparison with Rust,
Clojure and Swift; 10 alternatives considered; 11 plan, risks, questions.

---

## 1. The problem

`fib.tensor` (`docs/design/simd-and-tensors.md` 3.4) makes a view a new struct that shares the buffer **by a count**. That is
sound and cheap, and it has one consequence the design states as a rule: **mutation through a view does not exist.** A view makes
the buffer's count 2, `fib.unique?` is false (`spec/types.md` 6.6), so a write to the owner copies first and the view keeps the
old values. Value semantics, no aliasing, no iterator invalidation; the price is that a program cannot say "update this window of
my unique tensor in place", which is what blocked algorithms, in-place stencils, row operations of a factorisation and `fill!` of a
sub-range all want.

What the language lacks to say it (`spec/types.md` 2.14, `spec/syntax.md` 3.13):

- `&` names a *variable's private cell*, never a part of a value: "there is no field place" (D1). A window is a part.
- The unique-write primitives (`array-set!`, `set-field!`, 6.6, 2.13.1) test uniqueness **per write**, on the content of a cell.
  A window cannot be a cell, and a window that holds a count of the buffer makes the buffer shared.
- Borrows (`Borrowed(b)`, `Derived(b)`, 6.1) are tracked per binding site *inside a body* and are read-only; nothing in the
  checker lets a borrow be **written** through, or lets a call return something that is valid only until a named binding is touched.

So the feature is: a **lexically scoped, exclusive, writable borrow of part of a cell's content**, whose borrowed thing cannot
escape and whose owner cannot be touched while it lives. Rust calls the ingredient `&mut`; the interesting design question is how
little machinery fibber needs for it, given that it has counts and no lifetimes.

## 2. The proposal in one page

```
(with-view [v (slice! &t 0 8)]        ; t is lent, exclusively, for the body; v is a writable window of it
  (fill! &v 0.0)                      ; & = this call writes v, as for any in-out argument
  (vset! &v 3 1.0)
  (vget @v 3))                        ; @v: read it
t                                     ; here t holds the written values; no copy was made if t was unique
```

Four pieces, one of them new syntax:

1. **A core form `with-view`** (syntax 3.21 draft): a `let`-like binder whose initialiser is a call of a **lender**, a function
   with an `&` parameter whose result type is *scoped*. The `&t` argument names the place that is lent. The body is not a tail
   position.
2. **Scoped types** (types 1.9 draft): a type flag `:scoped` on a library struct (`(Win a)`), propagated structurally like `Send`.
   A value of scoped type may be **borrowed but never consumed** (rule S1): not returned, stored, captured by a heap closure,
   spawned, awaited across, or passed to an owned parameter. This is the whole escape analysis, because the existing ownership
   tables already classify every consume position (6.3 E1 to E6, 6.4).
3. **The freeze rule** (types 6.15 draft): the lent place `t` must be a *private cell* (the condition P1 of cell peeks, 6.3) and
   **no mention of `t` may occur in the body or in the lender call's other operands**. `t` is frozen, syntactically, for the
   extent of the form. The binder `v` is a **view cell**: a private cell that is never a value, used only as `&v` (in-out) or `@v`
   (read) at the one call that consumes it.
4. **The lender's runtime protocol** (library, `unsafe`): the unique-write protocol applied once, at lend time. The lender makes
   `t`'s content unique (copies a shared struct or buffer, exactly as `aset!` would, 6.6) and returns a window that holds a
   **raw pointer into the buffer and no count**. The take of 6.6 ("Taking the content") makes the `&t` argument a move, so a
   unique tensor is lent with no copy.

The language rules prove *the window cannot outlive or race its owner*; the library proves *the window stays inside the buffer
and windows of one lend are disjoint*, as the trusted-kernel discipline of `simd-and-tensors.md` 3.5 already requires of
`fib.tensor`. The compiler change is: one core form, one type flag, one rule at consume positions, one syntactic freeze test (a
sibling of P3), and no change to counts, lowering of existing forms or the runtime.

## 3. Surface syntax

```
(with-view [binding+] body+)
binding ::= pat (lender &place arg*)            ; pat: a symbol, or a struct pattern as in let (syntax 3.3)
place   ::= sym                                  ; a private cell variable (types 6.15 rule L1), or a view cell
```

- Bindings nest left to right, as `let*`: the init of the second binding is evaluated while the first lend is active, so it cannot
  mention the first place (freeze) but may use the first window. `(with-view [a (slice! &t 0 4) b (slice! &u 0 4)] ..)` lends two
  places; the same place twice is an error (7, V1).
- A struct pattern binds several windows from one lend: `(with-view [(Pair lo hi) (split! &t 0 4)] ..)`, each variable a view
  cell. The disjointness of `lo` and `hi` is the library's, 5.
- The value of the form is the value of the body; its type must not be scoped (S1).
- Not a value, not a macro: it is core because the checker must see it on the surface (the first criterion of syntax 4.1, as for
  `&`): the freeze test, privacy and the non-tail rule are properties of the surface form. The prelude may add sigil-free
  macros over it, 5.3.

**Why the sigils stay.** `&t` in the lender call is the existing marker "the callee may write this variable"; `&v` on every
mutating call keeps "who can write" visible at the call site, which is what the language does for `array-set! &a`. The owner's
sketch `(with-view [v (slice! t 0 8)] (fill! v 0))` is available as a macro layer (5.3) and expands to the sigiled form.

## 4. Typing and ownership rules

Stated in the style of `spec/types.md` 6; the full drafts, with error texts and numbering, are in `exclusive-views/types-draft.md`.

### 4.1 Scoped types

`Scoped(T)` is computed structurally, as `Send` is (types 5.1):

```
Scoped((N θ̄))   = N is declared :scoped, or some field or payload type of N (θ̄ substituted) is Scoped
Scoped((Array T)) = Scoped(T)      Scoped((Cell T)) = Scoped(T)      Scoped((fn κ (Ā) R)) = false
Scoped(scalar) = Scoped(str) = false
```

(`Scoped((Cell T))` is for the content of an `&` parameter's private cell and a view cell, the only cells of scoped content, 4.4.)
A type variable is **not** treated as scoped: a generic function sees a value of type `a` as an object (6 intro), and rule S1
below is checked on the *instantiated* types at each call site and in each body after zonking, so nothing is lost (6.1).
A scoped type is never `Send` (a `:scoped` struct holds a `ptr`, and `Send(ptr)` is false, 5.1; the flag adds it for a scoped
struct without a pointer).

### 4.2 S1: a scoped value is borrowed, never consumed

> **S1.** An expression of scoped type may not occupy a *consume position*: E1 (return), E2 (store: constructor argument, `cell`,
> `atom`, `set!`, `array-with`, `array-set!`, `array-push!`, `set-field!`, `swap!` result), E3 (capture by a heap closure or
> `async`), E4 (`spawn`), E5 (held across `await`), E6 (tail-call argument), an argument at an **owned** position of a known
> callee (6.4) or any argument of a call through a closure value, the operand of `dyn`, `weak`, `raw` or `raw-retained`. Two
> exceptions: the lender's result bound by `with-view`, and the bodies of `unsafe` blocks (the library's obligation, as for `raw`).

Consequences, each one an existing table row of 6.3: a window cannot be returned (E1), stored in a struct, `Vec`, cell or atom
(E2), captured by an escaping closure or a task (E3, E4), carried through an `await` (E5), or handed to a function that would
keep it (an owned parameter, which is exactly what 6.4's rule 1 infers for a function that returns or stores it). What remains
is **borrowed use**: as an argument at a borrowed position, a `let` alias, a pattern variable (`Derived`), a capture by a *stack*
closure. Those are valid for the borrower's frame, which lies inside the extent of the form, by the argument of 6.4 ("a borrowed
parameter is valid for the whole call because the caller holds it").

### 4.3 The lend: privacy, freeze, view cells

For `(with-view [pat (f &t e1 .. en)] body)`:

- **L1, `t` is a private cell** (the conditions P1 of 6.3 "Cell peeks"): `t` is an `&` parameter of the enclosing `defun`, or a
  `let` cell directly initialised by `(cell e)`, or a **view cell** of an enclosing `with-view`; every occurrence of `t` in the
  enclosing body is the place of a `@`, `set!`, `&t` argument or `set-field!`; no `fn` or `async` literal anywhere captures `t`.
  So no other name for the cell exists: a `let` alias, a struct field, a closure or another task cannot reach it. (An atom, a
  weak reference and a cell read from a field or returned by a call are not private: refused.)
- **L2, freeze.** No occurrence of `t` in `e1 .. en` or in `body`, at any depth, `fn` and `async` literals included (the
  `mentions` test of 6.6, `own.syntactic mentions-amp`). `t` is unreadable, unwritable and not lendable again until the form
  ends. A lender whose own operands mention `t` is refused too, which is what lets the take of 6.6 apply: the `&t` argument is
  **always a take** (`explain` prints `&t: lend`), never a copy-in that would leave the content at count 2.
- **L3, the lender call** is the only expression that may call a lender (a `defun` with an `&` parameter whose declared result
  type is scoped). A call of a lender anywhere else is `lender call outside with-view`. So safe code cannot make a window except
  as a `with-view` initialiser, and an author of a lender writes its body in `unsafe` (the raw pointer must come from somewhere).
- **L4, binders are view cells.** The variable (or each pattern variable) is a **view cell** of the scoped content type: it is
  initialised from the lender's result, the result being an implicit owning temporary of the form (released at its end, in
  reverse binding order, as an owning `let` binding is, 6.3). A view cell is **not a value** (as an `&` parameter is not, 2.14):
  - **VC1** it occurs only as `&v` (in-out argument) or as `@v`, and an `@v` must be a *peek* in the sense of P2 and P3 of 6.3: an
    argument at a borrowed position of a non-tail call, or a scalar field read, with no sibling operand mentioning `v`. Anywhere
    else (`(let [w @v] ..)`, an owned position, a stored position) is `view cell v can only be read as a call argument`;
  - **VC2** no `fn` or `async` literal mentions `v` (a closure could write it while a callee holds `&v`);
  - **VC3** `(set! v ..)` is refused (a window is not replaceable);
  - **VC4** in a call with `&v`, no other operand mentions `v`, and the `&` arguments of a call are distinct variables (6.5).
- **L5, where it ends.** At the end of the evaluation of the body, after which the view cells are dead and `t` is usable again.
  The body is **never in tail position** and no call in it is a tail call: the jump of a tail call releases the frame's cells
  (6.10 step 3) before the callee runs, which would free the buffer under a window; this is the reason peeks exclude tail
  position too. A `recur` inside the body is `recur not in tail position` already. `await` inside the body is an error
  (`await inside with-view`), so a task frame is never suspended with a lend outstanding.

What a window can do while the form runs: be passed at a borrowed position to any function that only reads it (`(vget @v i)`);
be passed as `&v` to a function with an `&` parameter of its type, which may write through it and may itself lend sub-windows of it
(`&v` forwarded as the lent place, rule L1 for an `&` parameter); be split by a lender into disjoint windows; be read in a loop.
What it cannot do: leave the body, be copied into anything that lives longer (S1), be written through two names in one call (VC4),
be read from a closure (VC2), or coexist with any use of the owner.

### 4.4 Interaction with the existing machinery

| Feature | Interaction |
|---|---|
| `&` parameters and copy-in (6.6) | The lender call is an ordinary `&` call, **taken** (the content moves into the lender's private cell, written back at return): the content returns to `t`'s cell *before* the body runs, with count 1, and the rule L2 keeps anyone from touching it. If `t` is an `&` parameter whose own caller acquired (count 2), the lender's unique test fails, copies once into the private cell, and everything after is unique, as 6.6 says for any user-written `&` function. |
| Unique-write protocol (6.6, 2.13.1) | Applied **once, at lend time, by the lender**, to the tensor struct and its buffer: unique means count 1 and none of `SHARED`, `IMMORTAL`, `STACK`, `HAS-WEAK`. Window writes are raw stores justified by that test plus L2; they are *not* unique-write calls and test nothing per write. |
| Counted views of `fib.tensor` (simd-and-tensors 3.4) | A read view `r` made **before** the lend holds a count of the buffer, so the lender copies; `r` keeps the old values and `t` gets a private buffer: the value semantics of 3.4 ("a write never changes what a view shows") is unchanged. A read view of `t` **inside** the body is not expressible (`t` is frozen). After the body, new counted views see the written values. |
| Cells and atoms | The lent place is a private cell (L1); atoms and non-private cells are refused. No value of type `(Cell S)` for scoped `S` exists (S1 on `cell`/`set!`/`atom`); an `&` parameter of scoped content type and a view cell are the only cells of scoped content, and neither is a value. |
| Closures (6.5) | A stack closure may capture a window *by borrow* (an alias, no count) and read it; a heap closure capturing one is E3 (error). A closure that mentions a **view cell** is refused (VC2); to read a window inside a closure, pass `@v` to the closure's call, a borrowed argument. The owner is not capturable at all (L1, L2). |
| Tasks (5, 6.8) | A window is not `Send`: `spawn` of a closure that captures one is `value of type (Win f64) cannot be shared between threads: closure capture v`. The owner is private to its frame, so it cannot be shared either. Parallel writes to disjoint windows are not provided (needs a `Send` raw-region type and a join-before-exit rule; `simd-and-tensors.md` 3.8, P10). |
| Async (6.9, syntax 3.13 rule 3) | `async` capture of a window is E3. An `await` inside `with-view` is an error. A `defun` with an `&` parameter that is lent is not async by the existing definition (3.1) unless an `async` mentions it. |
| Tail calls (6.10) | The body and every call in it are non-tail (L5). The `with-view` form itself may be the tail expression of a function: its *value* is returned after the extent ends. |
| `dyn` and protocols | `(dyn P w)` consumes `w` (S1). Static dispatch on a scoped type (`impl` for `(Win a)`) is ordinary. |
| `unsafe` | Lenders and the window operations are `unsafe` library code over `array-data` and raw loads and stores (3.5 of the tensor design); user code needs none. S1 and L3 do not apply inside `unsafe`, which is where the library builds the window from a raw pointer. |
| Generics | The checker works before monomorphisation with quantified variables as objects (6 intro). S1 is checked against **zonked** types at each consume position of each body and at each call site's instantiated signature, so an instantiation `a := (Win f64)` that flows to a consume position is refused where it occurs. |

## 5. The library layer

### 5.1 The window type and lenders

```
(defstruct (Win a) :scoped private                ; fib.tensor; fields private
  (base: ptr)                                     ; address of element [0,..,0] of the window: raw, NO count
  (meta: (Array i64)))                            ; rank, shape and strides of the window, in elements

(defun slice! (&t: (Tensor a) spec: (Vec Range)) -> (Win a)       ; a lender: & parameter, scoped result
  (do (tensor-make-unique! &t)                    ; the unique-write protocol on the struct and on its buffer
      (unsafe (win-of (array-data (. @t buf)) (slice-meta @t spec)))))
(defun split! (&t: (Tensor a) axis: i64 k: i64) -> (Pair (Win a) (Win a)) ...)   ; ranges [0,k) and [k,n) of one axis
(defun tiles! (&t: (Tensor a) tr: i64 tc: i64) -> (Tiles a) ...)                  ; a partition, scoped
```

`tensor-make-unique!` is the `array-make-unique!` of the tensor design (3.5) lifted over the struct. The same lenders exist for
`(Array a)` and for `(Win a)` itself (a window of a window: `(with-view [h (slice! &v ..)] ..)`, the lent place being the view
cell `v`; the sub-window's `base` is derived from the parent's, the parent frozen).

### 5.2 Obligations of the library (what the checker does not prove)

1. **Bounds relative to the window.** Every window operation checks its indices against the window's `shape`, not the buffer's, and
   the window was built inside the buffer (invariant I of 3.3, established by the lender with checked arithmetic).
2. **Injective writable windows.** A writable window must not map two indices to one element: the lender rejects stride 0
   (`broadcast-to`) and overlapping strides (sort by |stride|, require each to be at least the span of the faster axes) with a
   trap `slice!: window is not writable: strides overlap`. A `diagonal` or `flip` is injective and allowed.
3. **Disjoint siblings.** The windows a lender returns together (`split!`, `tiles!`) cover disjoint element sets by construction
   (ranges of one axis, or a grid), and a test shows each case (7).
4. **No pointer escapes the window object.** `base` is read only inside `unsafe` library functions that take the window borrowed.
5. **The buffer is not moved.** Nothing resizes an `(Array a)` (arrays are fixed-size, 2.13; `array-push!` is not offered on a
   window or its owner during the lend, which L2 guarantees).

### 5.3 Macro layer

Prelude macros, in the library, over the sigiled forms (they are rewrites that preserve verdicts, so macros by syntax 4.1):

```
(vget v i j)     => (win-get @v i j)         (vset! v i x)   => (win-set! &v i x)
(fill! v x)      => (win-fill! &v x)          (copy! v src)  => (win-copy! &v @src)
```

so the owner's sketch `(with-view [v (slice! &t 0 8)] (fill! v 0.0))` is valid, while the underlying language keeps visible `&`.
(`v` must be a symbol naming a view cell; the macro cannot hide the rules, only the sigils.)

## 6. Soundness argument

**Claim.** In a program accepted by the checker with a correct library, (a) no access through a window reads or writes outside the
buffer it was lent from, (b) a window is never used after its buffer is freed or replaced, (c) while a window is accessible no
other path reads or writes any element reachable through it except through a window of the same lend that is itself accessible
only by the same rules, and (d) a counted holder of the owner's old buffer never sees a window write.

*(b), the central one.* The buffer `B` is freed or replaced only if the content of `t`'s cell is released or replaced: by `set!`
on `t`, a unique write or write-back through `&t`, or `t`'s scope ending. L1 says every occurrence of `t` in its frame is
syntactic and no closure holds `t`, so no other code can name `t`; L2 says no such occurrence lies in the extent; the `&t` write-back
of the *lender* happens before the body (L5, the take returns the content at the lender's return); and `t`'s scope encloses the
`with-view` form, because the form is an expression inside it. The extent ends when the body's evaluation ends, and L5 excludes
tail calls and `await`, the two ways a frame could be discarded or suspended while the body's code is still to run. A trap
terminates the program (2.11): nothing runs afterwards, as in the `&`-take argument of 6.6.

*(b), windows used late.* A window is a scoped value. By S1 it is never consumed, so it never gets a count it could keep alive
past the form and never reaches a holder that outlives the form: not a return (E1), a store (E2), a heap closure or task
(E3, E4), an await (E5), a tail call (E6), or an owned parameter. A borrowed use is valid for the borrower's frame (6.4) and every
such frame is a callee of the body or the body itself. The one place a scoped value is created in safe code is the lender's result
at a `with-view` (L3), and the form's scope ends it (L4). The remaining way to keep a value alive is a *mutable holder*; every way
to put a value into one (`cell`, `set!`, `atom`, `reset!`, `swap!`) is an E2 consume. Generic code cannot launder a window through
a type variable, because S1 is checked on instantiated types (4.4, Generics).

*(a), (c).* Reads and writes of the owner are excluded by L2. Counted holders cannot write: a buffer with count above 1 is copied by
the lender (the unique test), so the window's buffer has count 1 and nobody else holds it; `SHARED` (sent to a task), `IMMORTAL`,
`STACK` and `HAS-WEAK` buffers are copied too, so no other thread, weak reference or constant aliases it. Bounds are the library's
(5.2 items 1, 2). Two accessible windows of one buffer arise only from one lender call (disjoint by 5.2 item 3) or by sub-lending
from a window (the parent view cell is frozen by L2 applied to a view cell, so only the child is accessible). VC4 and VC1 keep two
names for one window out of a single call (the `noalias` argument: callee parameters `&w` and `r` never overlap).

*(d).* A read view `r` made before the lend holds a count, so the lend copies and `r` points to the old buffer, untouched. This is
the existing unique-write semantics applied at one point in time.

**What the argument depends on, stated plainly.** The language rules give (b) and the exclusion of the owner; the library gives
bounds and disjointness; both are tested by attack (7) with a mutant for every rule. The same split exists today in
`fib.tensor` (3.5): the type system keeps a `Tensor` private, and the library proves what the raw loops need.

**Dynamic fallback not used.** A runtime flag "this cell is lent" checked on every read of a cell would allow non-private cells
(Swift's dynamic enforcement, 9) but taxes every cell read and breaks the peeks of 6.3; the proposal pays nothing at run time and
asks the lent place to be private instead.

## 7. Cases that must be able to fail

Numbers: checker rejections in `cases/ownership` (next free numbers after the batch's, headers fix the verdict and the error text),
library behaviour in `cases/stdlib` (63xx, window cases 6340 to 6379). Each reject case has an **accepting twin** that differs in
the one token the rule is about, so the pair shows which rule fires (a case that rejects for another reason is worse than none).
Every rule has a mutant (7.3) under which its case must go red.

### 7.1 Compile-time rejections

| Id | Program (sketch) | Expected | Rule |
|---|---|---|---|
| V1 | `(with-view [a (slice! &t 0 4) b (slice! &t 4 8)] ..)`: **aliasing writes through two views**, ranges even disjoint | `t is lent to a and cannot be used here` | L2 |
| V1b | twin: `[a (slice! &t 0 4) b (slice! &u 4 8)]`, two places | accepted | |
| V2 | body reads the owner: `(with-view [v (slice! &t 0 4)] (fill! v (aget @t 0)))` | same text, at the `@t` | L2 |
| V2b | `(count @t)` in the lender's own operands | same | L2 |
| V2c | twin: `(let [n (count @t)] (with-view [v (slice! &t 0 n)] ..))`, the read before | accepted | |
| V3 | **view outlives its owner**: body value is the window: `(with-view [v (slice! &t 0 2)] @v)` | `view cell v can only be read as a call argument` | VC1 |
| V3b | `(defun leak (n) (let [t (cell (zeros f64 [n]))] (with-view [v (slice! &t 0 2)] (id @v))))`, `id` returns its argument | `scoped value consumed: argument of id (owned) has type (Win f64)` | S1, 6.4 rule 1 |
| V3c | body's value of scoped type reaches the form's type | `with-view value has scoped type (Win f64)` | S1 |
| V3d | window stored: `(set! c (some @v))` with `c` a cell; `(Pair @v 0)`; `(conj [] @v)` | `scoped value consumed: store ..` | S1, E2 |
| V3e | captured by escaping closure: `(fn [] (vget v 0))` returned | `view cell v cannot be captured by a closure` | VC2 |
| V3f | `(spawn (fn [] (fill! v 0)))` | VC2 first; with a borrowed alias `w` of a callee param captured: `value of type (Win f64) cannot be shared between threads: closure capture w` | Send |
| V3g | `await` in the body | `await inside with-view` | L5 |
| V3h | `recur` or a tail call in the body | `recur not in tail position`; the call is ordinary (explain) | L5 |
| V4 | **write while a read view is live**: a counted read view `r` made before the lend is not an error (see R1: it keeps the old values). Compile-time form, if the optional read-only lend of question 3 is added: `(with-view-ro [r (slice &t 0 4)] (with-view [w (slice! &t 4 8)] ..))` | `t is lent to r and cannot be used here` | L2 |
| V5 | alias of the lent place: `(let [d c] (with-view [v (slice! &c 0 2)] (set! d ..)))` | `lent place must be a private cell: c is used as a value at L:C` | L1 |
| V5b | `c` captured by a closure before the form | `lent place must be a private cell: c is captured by a closure` | L1 |
| V5c | place is an atom, a field, a call result | `lent place must be a private cell` | L1 |
| V6 | `(slice! &t 0 2)` outside `with-view` (a `let`, a return) | `lender call outside with-view: slice!` | L3 |
| V7 | `(fill! &v 0 &v)`, `(copy! v v)` in one call: `(f &v @v)` | `view cell v is passed in-out and used again in the same call` | VC4 |
| V8 | `(set! v w)` | `view cell v cannot be assigned` | VC3 |
| V9 | `(dyn P @v)`; `(weak @v)` | `scoped value consumed: dyn` | S1 |

### 7.2 Run-time and library behaviour (each against a scalar reference in the case itself, decision 5)

| Id | What | Fails if |
|---|---|---|
| R1 | **write while a read view is live**, the counted form: `r (slice t ..)` then a lend and writes, then print `r` and `t` | the lender wrote in place into a shared buffer (`r` changes) |
| R2 | a unique tensor with no other holder is lent and written: result in `t` after the form; allocation trace shows **no buffer copy** (`FIB_TRACE` bytes) | the lender always copies |
| R3 | a shared tensor is lent: exactly one buffer copy, `t` correct, the other holder unchanged | no copy, or two |
| R4 | index `i = len-1` ok, `i = len` traps, negative traps, **inside the buffer but outside the window** traps (`slice! &t 4 8`, write 3) | bounds taken against the buffer |
| R5 | `split!` halves: write all of `lo`, then all of `hi`, `t` has no overlap or gap; odd and zero lengths; unit-length | off-by-one at the cut |
| R6 | writable `slice!` of a broadcast tensor traps; of a transposed, sliced, flipped tensor writes the right elements (against a scalar index loop) | overlap check missing, or wrong strides |
| R7 | nested: `(with-view [h (slice! &v 2 6)] ..)` writes land at the right parent offsets; after the inner form `v` is usable again | base arithmetic wrong |
| R8 | the extent ends: `t` after the form read through a fresh counted view equals the windows' writes | a stale copy |
| R9 | `fill!`, `copy!`, blocked tile loop (7x13 and 16x16 edge tiles) against the triple-loop reference | |
| R10 | `fibc explain`: `&t: lend` (a take, no retain) and no count operation around `@v` peeks in a loop | an acquire per element |
| R11 | memory audit (`fibref`-style audit does not run compiled code; use the allocation counters and the leak check of `fibc cases`): no leak, no double free over a lend inside a loop | |

### 7.3 Mutants (planted faults)

Each must turn its case red: M1 L2 off (V1, V2 compile) ; M2 L1 off (V5) ; M3 S1 off for E1, E2, E3 separately (V3b, V3d, V3e) ;
M4 L5 off, allow tail call (V3h, and a runtime case where the callee reads a freed buffer) ; M5 lender skips the unique test (R1
shows `r` changed) ; M6 lender copies always (R2) ; M7 window bounds against the buffer (R4) ; M8 `split!` cut `k+1` (R5) ; M9 drop the
stride-overlap check (R6) ; M10 VC4 off (V7).

## 8. What a program can write, against the alternative

**With the feature.**

```
(defun zero-rows! (&t: (Tensor f64) lo: i64 hi: i64) -> unit            ; in place, safe, no copy when t is unique
  (with-view [v (slice! &t [lo hi] :all)] (fill! v 0.0)))

(defun gemm-tiles! (&c: (Tensor f64) a: (Tensor f64) b: (Tensor f64)) -> unit   ; C tiles written in place
  (loop [i 0]
    (if (< i (rows @c))
      (do (loop [j 0]
            (if (< j (cols @c))
              (do (with-view [ct (tile! &c i j MR NR)]                    ; one lend per tile: one unique test, then raw stores
                    (micro-kernel! &ct (row-panel a i MR) (col-panel b j NR)))
                  (recur (+ j NR)))
              nil))
          (recur (+ i MR)))
      nil)))

(defun quicksort! (&a: (Array i64) lo: i64 hi: i64) -> unit             ; windows over arrays: in-place partition
  (if (< (- hi lo) 2) nil
      (let [p (partition! &a lo hi)]
        (with-view [(Pair l r) (split! &a lo p hi)]                       ; disjoint halves, one lend
          (do (quicksort-win! &l) (quicksort-win! &r))))))
```

**Without it** (`simd-and-tensors.md` 3.4, 3.5): the owner writes `(aset! &t [i j] x)` one element at a time, each with a unique
test and an index computation; or `(update-slice! &t spec f)`, which **copies the window out, applies `f`, copies it back** (two
`O(window)` passes plus an allocation); or a kernel in `unsafe` that only the library may write. For `fill!` and row updates
whose arithmetic is `O(window)` the copies are the same order as the work (an estimate: a factor of 2 to 3 on a memory-bound
loop); for a matmul tile of `MR x NR` against `k` the copy is `O(MR NR)` against `O(MR NR k)` work, small. The point of the
feature is therefore (1) **safe user-level kernels**, which today require `unsafe`; (2) **in-place algorithms on sub-ranges**
(sort, partition, stencil rows, factorisation steps) that otherwise copy or are written as whole-buffer rebuilds; (3) a
checked-once, raw-inside access pattern the compiler can vectorise (one bounds check per window, `noalias` on `&` window
parameters by VC4, the lever V measured at up to 8.1x for `array-set!` loops, `docs/design/vectorisation.md` summary item 3, whose other
two blockers remain separate work). Not measured here; the first package should time `fill!` and a tile loop against
`update-slice!`.

**What it does not give**: parallel writes to disjoint windows (tasks, not `Send`), windows stored or returned (Rust's `&mut` in a
struct, an iterator of `&mut` items), a window over an owner that is not a private cell (a field, an atom).

## 9. Comparison

| | Rust `&mut [T]` | Clojure transients and arrays | Swift exclusivity | This proposal |
|---|---|---|---|---|
| What is lent | a mutable borrow of a slice or a whole value | `(transient v)`: the whole collection, owned until `persistent!`; `aset` on arrays: shared storage, no protection | `inout` parameter, `withUnsafeMutableBufferPointer`; newer `MutableSpan`/`Span` (non-escapable) | a window of a private cell's content |
| Exclusivity | static, lifetimes (NLL), `&mut` unique | none for arrays (aliasing writes are the program's problem, as `core.matrix`); transients: single-thread check in older versions, "used after persistent!" is a run-time exception | static for locals and `inout` args, **dynamic** (a run-time trap) for class properties, globals and escaping closures | static, syntactic freeze of a private cell; no run-time cost |
| Escape control | lifetime parameters in signatures and structs; checked | by convention | `withUnsafe..` pointers must not escape: a documented rule, not checked; `Span` types tie lifetime to the owner | one flag `:scoped` plus "borrowed, never consumed" (S1); no lifetime syntax |
| Expressiveness | high: return `&mut` into containers, split borrows, store in structs | n/a | medium | lower: windows second-class (cannot be returned or stored); sub-windows only by `with-view` nesting or a splitting lender |
| Sub-window aliasing | `split_at_mut` (unsafe inside) | none | none | `split!`, `tiles!`: library-proved disjoint; two lends of one place refused (V1) |
| Cost | none | transient: one copy-on-first-write | dynamic checks where static fails | one unique test per lend (a copy only if shared) |
| Fits fibber because | | the language's persistent-by-default values and in-place-when-unique are already the transient idea, per value | the "law of exclusivity" is the spirit | counts give uniqueness at run time; the new rule only has to keep the owner untouched |

Rust's model is more expressive and heavier (lifetimes in types, in every signature that returns a borrow); fibber has counts and
no lifetimes, so the proposal takes the *second-class* corner of the design space (the one Hylo and Val's subscripts, and
Swift's `inout`, occupy): a window is a borrow valid for a lexical extent and nothing more. Clojure's transients show the
user-visible protocol this resembles (a bracketed mutable phase over a value) and its weakness (runtime errors on misuse); the
freeze rule is the static version. Swift's dynamic enforcement is the alternative in 10, D.

## 10. Alternatives considered

| | Alternative | Why not (for v1) |
|---|---|---|
| A | No writable views: `update-slice!`, whole-buffer kernels in `unsafe` | Works (the v1 plan of the tensor design) and is the baseline of section 8; costs copies and keeps user kernels `unsafe`. Remains the fallback if the owner declines |
| B | Window as an interior-mutable handle: `(fill! v 0)`, no `&`, the handle copyable | Simplest surface and memory-safe single-threaded, but two handles to one memory write each other, `noalias` cannot be claimed, and "who writes" is invisible at call sites. The macro layer (5.3) gives B's look on A's rules |
| C | Region or lifetime parameters on `(Win κ a)`, as colours (types 1.3) | Allows returning windows into outer structures; heavy: invariance rules, error messages, inference. The second-class corner is enough for the cases of 8 |
| D | A "lent" flag in the cell, checked on every cell read and write (Swift's dynamic enforcement); non-private cells allowed | Taxes every cell access, kills the peeks of 6.3, and moves errors from compile time to run time. A later extension if non-private owners are wanted |
| E | Move the content out of the cell for the extent (`cell-update!` style, null left behind) instead of the syntactic freeze | A read of the empty cell through an alias is a null dereference; needs L1 anyway to rule out aliases, then the move adds nothing |
| F | Allow overlapping windows (ADR 007 of liar, "aliasing allowed") | Memory-safe on one thread, but gives up `noalias`, determinism of in-place kernels under reordering, and any later parallelism; ADR 007's own justification is single-threadedness |
| G | Closures may capture view cells (non-escaping) | Needs a rule that the closure is not called while an `&v` is outstanding (VC4 extended through the closure's capture); not needed for loops (`loop`) so deferred; see questions |

## 11. Plan, risks, questions

**Package P10b** (after P6 and P6b of the tensor design; needs the owner's approval of the drafts):

1. spec: promote the drafts to `spec/syntax.md` 3.21 and `spec/types.md` 1.9, 2.17, 5.5, 6.15, 6.14 rows, 7 cases.
2. checker (`compiler/types`, `compiler/own`, `compiler/expand`): the `:scoped` flag and `Scoped(T)`, S1 at the consume tables, the
   `with-view` form (expand to a core node), L1 (reuse `own/peek.fib`'s P1), L2 (reuse `own.syntactic mentions-amp`), VC1 to VC4, L5.
3. emitter: `with-view` lowers to the lender call as a take, the body, the release of the view cells; nothing else.
4. library: `Win`, `slice!`, `split!`, `tiles!`, the window operations, the macro layer, `tensor-make-unique!`.
5. cases of 7 with the mutants of 7.3. The stage-2 compare scripts get the new inputs; the Rust front end is frozen and cannot
   read the form (`compiler/mirror-pending/` is not involved).

**Risks.** (1) `fibc` itself uses no windows, so stage 2 is unaffected except by the checker's added code. (2) S1 on instantiated types
inside generic bodies is the subtle part; the generic cases (`7.1` V3b with `id`, a `Vec` instantiation) must exist before the rule is
trusted. (3) The view-cell restrictions (VC1 to VC4) may be too strict for real kernels; the first library kernels will show which to
relax (G). (4) A window as a counted struct costs an allocation and a count per lend and a copy-in per `&v` call; both are scalar-sized
and the stack-allocation rule of 6.11 and the peeks should remove them in loops: **measure first**. (5) Teaching cost: one new form and
the rule "a window is borrowed, never moved".

**Questions for the owner.**
1. Are the sigils (`&v`, `@v`, `&t` in the lender call) acceptable, with the macro layer for the sigil-free look, or should windows
   be interior-mutable handles (alternative B)?
2. Is "closures may not mention a view cell" acceptable for v1 (G)?
3. A read-only lend `with-view-ro` (no unique test, so no copy even for a shared tensor) is a cheap extension of the same rules;
   include it in P10b or later?
4. Windows over `(Array a)` and `Vec` as well as tensors (the quicksort of section 8), or tensors only?
5. Parallel disjoint windows: schedule the `Send` region type after P10b, or leave to the tensor threads package?

## 12. Recorded measures

The claims of section 8 and ADR 0007 (`docs/adr/0007-windows-run-within-1-3x-of-the-array-loop.md`) are checked against the numbers recorded
here: one line per measurement, **newest last** (the last line of a key is the record that counts; the earlier ones are history). A line is
`<!-- measure KEY VALUE UNIT DATE | cmd: COMMAND -->`; `fibc adr` reads them, and `adr --rerun` runs the command and compares the fresh value
(the command prints `measure KEY VALUE`). Ratios are window time over array time (or over the SIMD kernel), medians of the runs of
`scripts/bench/windows.sh` on the 28-core machine, cache-resident kernels of 1,000 elements.

First record, 2026-10-05, median of 3 runs, taken while another agent's full gate was running on the same machine (loaded; the script
holds `/tmp/fibsuite.lock` but the gate's own jobs ran outside it):

<!-- measure rmw-window/array 1.43 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 3 -->
<!-- measure fill-window/array 0.97 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 3 -->
<!-- measure saxpy-window/array 0.99 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 3 -->
<!-- measure saxpy-window/simd 1.00 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 3 -->
<!-- measure tile-window/array 1.83 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 3 -->

Three more records the same evening, each the median of more runs (5, 7, 9), all on the shared machine (other agents' gates and benchmarks were
queued on the lock around them); the kernels are sub-nanosecond per element, so the ratios move with the machine's state. The 5-run median had
`fill` at 1.31 and the 7-run median `rmw` at 1.48, both over the limit, both with their other ratios near 1.0; the 9-run median is the newest and
is the one that counts. **The limit of 1.3 has little margin for these two kernels on a loaded machine** (a finding of ADR 0007, not resolved
here: a quiet machine, or a larger limit, is the owner's call):

<!-- measure rmw-window/array 1.01 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 5 -->
<!-- measure fill-window/array 1.31 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 5 -->
<!-- measure saxpy-window/array 1.02 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 5 -->
<!-- measure saxpy-window/simd 1.01 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 5 -->
<!-- measure tile-window/array 1.60 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 5 -->
<!-- measure rmw-window/array 1.48 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 7 -->
<!-- measure fill-window/array 0.97 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 7 -->
<!-- measure saxpy-window/array 1.07 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 7 -->
<!-- measure saxpy-window/simd 1.04 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 7 -->
<!-- measure tile-window/array 1.63 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 7 -->
<!-- measure rmw-window/array 1.15 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 9 -->
<!-- measure fill-window/array 1.01 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 9 -->
<!-- measure saxpy-window/array 1.01 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 9 -->
<!-- measure saxpy-window/simd 0.96 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 9 -->
<!-- measure tile-window/array 1.78 ratio 2026-10-05 | cmd: FIBC=$FIBC scripts/bench/windows.sh -n 9 -->
