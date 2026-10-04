# Draft rows for `spec/types.md` (exclusive views)

**Draft, not spec.** Proposed by package D2 (`docs/design/exclusive-views.md` is the rationale and the soundness argument). Section
numbers are those the rows would take in the live file. Everything is **Proposed**. Source for every existing rule cited: the
live `spec/types.md` sections named.

## 1.9 Scoped types (new, after 1.8)

A struct or enum declared `:scoped` (syntax 3.7) is a **scoped type**. `Scoped(T)` is computed structurally as `Send` is (5.1):

```
Scoped((N θ̄))     = N declared :scoped, or Scoped(some field or payload type of N with θ̄ substituted)
Scoped((Array T)) = Scoped(T)            Scoped((Cell T)) = Scoped(T)         Scoped((Atom T)) = Scoped((Weak T)) = Scoped((Task T)) = Scoped(T)
Scoped(scalar) = Scoped(str) = Scoped(Form) = Scoped(ptr) = false             Scoped((fn κ (Ā) R)) = false
Scoped((dyn P)) = false
```

A type variable is not scoped; the rules below are checked on zonked and instantiated types (6.15). A scoped type is not `Send`
(5.1 gains the row `Send(T) = false` when `Scoped(T)`, with the witness text `value of type T cannot be shared between
threads: <path>`). A value of scoped type has no mode of its own beyond the ordinary ones (6.1): it is an object; what differs is
rule S1 (6.15), which forbids consuming it.

`Scoped((Cell T))` exists so that the two cells of scoped content that are allowed, the private cell of an `&` parameter and a view cell
(6.15), are typed; **no expression has a value of type `(Cell S)` with scoped `S`**: `(cell e)`, `(atom e)`, `(weak e)` of a scoped
`e` are S1 errors.

## 2.17 `with-view` (new, after 2.16)

```
f : ∀ā. (fn ((& A1) B2 .. Bn) S)     f a defun with an & parameter, Scoped(S)                         (a lender)
x : (Cell A1)  a private cell (6.15 L1)        e_i : B_i        pat : S ⊢ pat binds x̄ : S̄  (view cells)
Γ, x̄ : (Cell S̄) ⊢ body : R         ¬Scoped(R)
─────────────────────────────────────────────────────────────────────────────────────────────
Γ ⊢ (with-view [pat (f &x e2 .. en)] body) : R
```

A binding's lender call is typed as a call with an `&` argument (2.14). Several bindings nest. A lender called anywhere else:
`lender call outside with-view: f`. The result of the form is `R`; `Scoped(R)` is `with-view value has scoped type R`.

## 5.1 addition (Send)

`Send(T) = false` for scoped `T` (witness path as 5.3). `spawn` and `async` capture of a scoped value is refused by `Send` for
spawn and by S1 (E3) for a heap closure or `async`.

## 6.15 Exclusive views: lends, scoped values, view cells (new)

Source of the machinery reused: `compiler/own/peek.fib` (P1), `own.syntactic` `mentions-amp` (the mention test of 6.6),
`compiler/own/walk/call.fib` (consume positions).

**S1, a scoped value is borrowed, never consumed.** An expression whose type is scoped may not occupy a consume position of 6.3:
E1 to E6, an argument at an owned position of a known callee (6.4), any argument of a call through a closure value, and the
operand of `dyn`, `weak`, `raw`, `raw-retained`. Exceptions: the lender's result at a `with-view` binding, and the body of an
`unsafe` block (the library's obligation). Error: `scoped value consumed: <position> has type T`, `<position>` one of `return`,
`store`, `capture by a heap closure`, `spawn`, `await`, `tail call argument`, `owned parameter p of f`, `dyn`. The check reads the
consume labels the pass already computes (6.3: "the checker walks the tree once, labels each object-typed expression with its
position") and the callee summaries of 6.4; it is applied after zonking and, for generic callees, to the instantiated signature
at the call site.

A borrowed use is not restricted: an argument at a borrowed position, a `let` alias (6.1), a pattern variable (`Derived`), a capture by
a stack closure (6.5). By 6.4 these are valid for the borrower's frame.

**L1, the lent place is a private cell.** For `(with-view [pat (f &t ..)] body)`, `t` is an `&` parameter of the enclosing `defun`, a
`let` cell whose initialiser is directly `(cell e)`, or a view cell; every occurrence of `t` in the enclosing body is the place of `@`,
`set!`, `&t` or `set-field!`, and no `fn` or `async` literal captures `t` (the conditions P1 of 6.3 "Cell peeks"). Error `lent place must be
a private cell: t` with the reason: `used as a value at L:C`, `captured by a closure at L:C`, `an atom`, `not a variable`.

**L2, freeze.** `t` does not occur in the other operands of the lender call, in later bindings or in the body, at any depth, closure
literals included (the mention test of 6.6). Error `t is lent to v and cannot be used here`. Because of L2 the `&t` argument is
a **take** (6.6 conditions 1 to 3 hold: a cell of this frame, no other operand mentions it, the callee is not a builtin);
`fibc explain` prints `&t: lend`.

**L3, lender calls only as initialisers.** A call of a lender (2.17) outside the initialiser of a `with-view` binding is
`lender call outside with-view: f`. Inside `unsafe` it is the library's business.

**L4, view cells.** Each variable bound by `pat` is a **view cell** of the scoped content type, initialised from the lender's result (an
implicit owning temporary of the form, released at its end in reverse binding order, 6.3 scope exit; a variable bound inside a
struct pattern is a retained reference to the part, as for a `let` pattern). A view cell is not a value (as an `&` parameter, 2.14).

- VC1: occurs only as `&v`, or as `@v` that is a *peek* (P2 and P3 of 6.3: a borrowed argument of a non-tail call, or a scalar field read,
  with no sibling operand mentioning `v`). The plan records it as a peek (`BodyOwn.peeks`), mode `Scalar`: no count operation.
- VC2: no `fn` or `async` literal mentions `v`.
- VC3: `(set! v ..)` is refused.
- VC4: the `&` arguments of a call are distinct variables (6.5), and in a call with `&v` no other operand mentions `v`.

**L5, the extent and the tail.** The extent of a lend is the evaluation of the form's bindings after the lender returned and of its
body. The body is not a tail position and a call in it is an ordinary call (6.10 rule (e) is applied as for a call with an `&`
argument that is not forwarded); `recur` in it is `recur not in tail position`; `await` in it is `await inside with-view`. Reason: a
tail call releases the frame's cells before the callee runs (6.10 step 3), freeing the buffer under a window.

**The lender's contract** (obligation of the library, stated so the cases can test it). A lender takes the lent cell through an `&`
parameter, applies the unique-write protocol (6.6) to its content and to every object a window will write through, so that after the
call each has count 1 and none of `SHARED`, `IMMORTAL`, `STACK`, `HAS-WEAK`; returns a value of scoped type holding raw pointers into
those objects and **no count of them**; every access through the window is checked against the window's own extent; windows returned by
one call are disjoint; a writable window is injective.

**Why it is sound.** (b) The buffer of the lent content is freed or replaced only by a release or replacement of the content of `t`'s
cell. L1 makes every occurrence of the cell syntactic and uncaptured, L2 excludes all of them from the extent, the lender's write-back
precedes the body, `t`'s scope encloses the form, and L5 excludes tail calls and suspension, the only ways the frame could end before the
body does. (a, c) The lender's contract plus the unique test make the window the only path to the buffer. (d) Every other holder had a
count, so the lender copied. S1 keeps every window inside the extent (it never gains a count and never reaches a holder that outlives
the form), and VC1 to VC4 keep two names for one window out of a call. The full argument and the cases that fail without each rule
are in `docs/design/exclusive-views.md` 6 and 7.

## 6.14 Error catalogue, rows to add

| Text | Rule |
|---|---|
| `lent place must be a private cell: t (reason)` | L1 |
| `t is lent to v and cannot be used here` | L2 |
| `lender call outside with-view: f` | L3 |
| `with-view needs a call of a function with an & parameter and a scoped result` | 2.17 |
| `with-view value has scoped type T` | 2.17, S1 |
| `scoped value consumed: <position> has type T` | S1 |
| `view cell v can only be read as a call argument` | VC1 |
| `view cell v cannot be assigned` | VC3 |
| `view cell v cannot be captured by a closure` | VC2 |
| `view cell v is passed in-out and used again in the same call` | VC4 |
| `await inside with-view` | L5 |
| `value of type T cannot be shared between threads: closure capture v` | 5.3 |

## 7 Case table, rows to add

One row per case of `docs/design/exclusive-views.md` 7.1 and 7.2: the accepting twin, the rejection and its text; the run-time cases
R1 to R11 with the counts they pin.

## 8.1 and 8.12 (mapping to lIR)

`(Win a)` is an ordinary struct (`ptr` to the object, count header, fields `base: ptr`, `meta`), allocated on the stack when
scope-local (6.11). `with-view` emits the lender call (a take), the body, and the release of the temporaries; it emits no
instruction of its own. A peeked `@v` loads the object pointer from the view cell with no count. The window operations are
`unsafe` library functions on `base` and the `meta` array. Optional (measure first): `noalias` on the window pointer of an `&`
window parameter, justified by VC4.

## 10 Decision record, open items to add

Open (owner): sigils against interior-mutable handles; closures over view cells; a read-only lend; windows over `(Array a)` and
`Vec`; parallel windows. See `exclusive-views.md` 11.
