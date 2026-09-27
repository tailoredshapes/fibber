# Synthesis: how spec/syntax.md and spec/types.md were assembled

Inputs: the three drafts in this directory (`minimal/`, `inference/`,
`ownership/`), the three judges' reports (lenses: lisp-user, implementer,
soundness) and the draft authors' own tensions. Output: `spec/syntax.md`
and `spec/types.md`. This file records which draft each part started
from, what was grafted in from the others, and what was rejected and
why.

## 1. Starting points

The judges' totals were ownership 72, minimal 71, inference 58. The two
leaders split by file:

- **types.md §6 (the ownership checker) starts from the ownership draft.**
  Its three modes `Owned / Borrowed(b) / Derived(b)` keyed by binding
  site, the single `consume` rule at five named escape positions E1–E5,
  the table of non-escape positions, and the scope-exit rule are the
  only calculus among the three that handles a value leaving a nested
  scope (the soundness judge's fatal flaw in minimal: case 03 had no
  mechanism) and they give the codegen table (§8.10) almost for free.
- **syntax.md starts from the minimal draft.** Its reader over a typed
  `Form`, its precise core/macro/library criterion with a closed list of
  core forms, the literal-collection desugaring that keeps `[x y]` as
  data inside quotes, the builtin-not-form treatment of cells/atoms/weak,
  and the `&`-is-a-cell representation are the tightest encodings of
  ownership.md the judges saw.
- **types.md §1–§5 (types, inference, dispatch, Send, colours) start from
  the inference draft's algorithm**, with the ownership draft's deferred
  `HasField`/`HasDeref` constraints and instance keying, and with the
  inference draft's site coercions removed.

## 2. Taken from each draft

### From the ownership draft

- The mode calculus and E1–E5 (types §6.1–§6.3), including the
  `match`-scrutinee-as-implicit-binding rule and the join rule.
- The precise definition of an escaping closure with clauses (a)–(c)
  (types §6.5), tightened as the soundness judge asked: clause (c) now
  requires the literal to be the *direct* initialiser of the binding.
- The escape-summary least fixpoint over the call graph (types §6.4),
  sharpened with minimal's `Borrowed(p)`-only rule so that returning a
  sub-object of a parameter does not make the parameter escape.
- The copy-in move refinement for `&` (types §6.6), generalised to a
  syntactic **exclusivity** condition that covers both cell variables and
  private cells, and extended to `&(. x f)` field places so that a
  persistent vector's tail can be updated in place from library code.
- `(Array T)` as the typed substrate of the library collections (types
  §2.13), with the unique-write test moved into two `&`-taking primitives
  (`array-set!`, `set-field!`) because a borrowed argument with count one
  is held by the caller and must never be written in place.
- The `Send` witness path and the exact error texts (types §5.3, §6.14).
- `fibc --explain` (types §9), extended with copy-in decisions and colour
  solutions.
- Source positions surviving expansion (syntax §1.3), typed literal
  suffixes `1i32` (syntax §1.1), the `Fn`-with-flag idea as the colour on
  function types, `Task`/thread marking of results.
- The runtime pseudo-code shape for `retain`/`release`/`drop`/`share`
  and the publication order share-then-store for atoms (types §8.2, §8.6).

### From the minimal draft

- `&` parameters are cells inside the callee, read with `@v`; every cell
  read is an owned (+1) temporary (syntax §3.13, types §6.6–§6.7). This
  is what closes the ownership draft's `&`-slot alias hole (a `let` alias
  of an `&` slot freed by a `set!` in straight-line code) and it makes
  case 08 sound by construction.
- Self tail calls compile and interpret as loops with owning parameter
  slots (types §6.10, §8.9): case 07 gets constant stack without the
  ownership draft's `own` calling convention, which contradicted the
  letter of §4 ("never freed by the callee").
- The reader tables, the `Form` enum, quasiquote as an expander rewrite,
  macros as fibber functions run by the reference interpreter (syntax
  §1, §3.16).
- The core/macro/library criterion and the normative lists (syntax §4).
- Literal collections desugar to `conj`/`assoc` chains after expansion
  (syntax §1.4).
- Protocol method escape kinds declared on the protocol, default
  escaping, `:borrow` as a checked promise (syntax §3.10, types §6.4).
- `(weak x)` forces heap allocation (types §6.7, §6.11): closes the
  stack-weak use-after-free present in both other drafts.
- The 16-byte header with type id and flags, the `fib.types` table with
  per-type `drop` and `trace`, weak boxes found through a global table,
  the spinlock atom protocol, share-marking through `trace`, and the
  interpreter/compiler parity paragraph (types §8).
- Open decision 12's implementation order: heap everything in v1 and let
  stack allocation be an audited optimisation (types §6.11, §10 item 8),
  with the scope-local condition restated so that the mixed-branch hole
  the implementer judge found cannot recur.
- Option as a plain enum with `match` as its eliminator (syntax §3.9).

### From the inference draft

- HM with generalisation at `defun` SCC boundaries and `let` never
  generalising; implicit type parameters; bounds inferred; unannotated
  struct fields as type parameters; the "what must be annotated" table
  (types §3.6–§3.8, syntax §3.7).
- The closure colour lattice: `⊑` constraints at flow sites, `ς ⊒ Caps`
  at literals, least-fixpoint solving, quantified colour variables (types
  §5.4). This fixes minimal's equality-unified κ (which cannot type
  `for-each` or compose a `send` with a `local` closure) and is what lets
  `pmap` take a parameter.
- `async` is a thread boundary (types §2.8, §5.2): matches §3.4's "a
  task" and closes minimal's task-carries-a-cell hole.
- The tail-call soundness condition, kept as the statement of what a
  general tail call would need (types §6.10).
- Monomorphisation keyed by layout class (types §4.3); `STACK` flag as
  defence in depth (types §8.2); the state-machine lowering of `async`.
- Shadowing allowed, top-level macros expanding to several forms,
  `struct-fields` reflection (syntax §3.3, §3.16).
- Arithmetic and comparison as protocol methods (`Num`, `Eq`, `Ord`) with
  built-in scalar instances (types §2.12), so `add`, `min`, `max` are
  generic while lIR's no-promotion rule is kept.
- Object safety for `(dyn P)` (types §4.4).

## 3. Rejected, and why

| Rejected | From | Why |
|---|---|---|
| The inferred `own` calling convention | ownership | Contradicts §4 ("never freed by the callee") and flips per caller; minimal's self-tail-call loop achieves case 07 without it. General tail calls are an open decision (types §10 item 6). |
| `Borrowed(v)` slots for `&` parameters with the retain-around-call patch | ownership | Unsound as written (soundness and implementer judges): `let` and pattern aliases of the slot are not covered. Replaced by minimal's owned reads of a cell. |
| The `@cell` elision "no `set!` between acquire and last use" | ownership | Unsound under cell aliasing (a callee can reach the cell through any argument or field). Elision is now allowed only around primitives that write no cell (types §6.3). |
| `(weak e)` as a non-escape that permits stack allocation | ownership, inference | `deref`/`upgrade` could resurrect a stack object. `weak` now forces heap allocation. |
| `upgrade` as the name of weak dereference | ownership | ownership.md §6 uses `(deref w)`; kept `deref`/`@` through one `Deref` protocol. |
| Single-threaded executor, `async` without `Send` | ownership | ownership.md §3.4 lists "a task" among thread crossings. |
| `Send ptr = yes` | ownership | A raw pointer's target has no header to mark. |
| Reader-level `[...]` → `(vector ...)` | ownership | Loses `[x y]` as data in macros and needs variadic calls; minimal's post-expansion desugar keeps both. |
| Macros restricted to `fib.form` with no reflection; quotes only inside macros | ownership | Makes `derive` impossible and forbids quoted data at run time. |
| Site coercions `T ↝ (Option T)` and `T ↝ (dyn P)`, `nil?` narrowing | inference | Order-dependent acceptance (`(f (some 1) 1)` vs `(f 1 (some 1))`); principal types lost. `if-let` covers the idiom. |
| The Move rule "appears exactly once" | inference | Unsound for occurrences under a repeatedly-called closure and per-branch releases. The scope-exit rule moves structurally instead. |
| Moved temporaries into `retains` parameters | inference | Nothing releases them (leak on cases 06, 07, 09, 11). Calls are never escapes for the caller; temporaries are released after the call. |
| `fib_atom_store` publishing before share-marking | inference | Race on the non-atomic count path. Share, then store. |
| `(Weak T)` as a counted object pointing at a separate control block | inference | Two allocations per weak; the box itself is the counted `Weak` value. |
| Equirecursive types through `Cell` for case 15 | inference (alternative) | Complicates layouts, `Send` and monomorphisation keys; the nominal `Knot` rewrite keeps the verdict. |
| `(i32 42)` core forms, `defconst`, vector patterns, `:when` guards, `#| |#` comments | inference | Suffix literals are lighter; the rest is not needed by any case and can be added later. |
| Module-wide monomorphic inference; explicit `[T]` type parameters; fully annotated generics | minimal | liar's `lib/` would need rewriting with `[T]` on every generic definition and a generic `inc` is impossible; HM at `defun` boundaries costs nothing the checker needs. |
| No shadowing (ADR 006) | minimal, ownership | Breaks nested `let` in macros; the checker keys on binding sites anyway. |
| `pmap`'s `f` declared `:borrow` | minimal | Its body spawns closures capturing `f`, so it escapes; the draft's own checker would reject its prelude. `f` now escapes, which is also what makes case 13 a crossing. |
| Non-final `do` steps must be `unit` | minimal | Case 16 and every `pmap` body discard a value. |
| Every nil test as two nested `match`es | minimal | `if-let` (a macro over `match`) keeps the core the same and case 19 readable. |
| Killing the interpreter's plain counting where the compiler moves | all | Resolved by making the three deviations from plain counting (the tail loop, the copy-in decision, the unique-write test) part of the semantics, implemented identically by both (types §6.12). |
| Case 05 rewritten with a struct of closures | minimal | The list with both closures returning `i64` is closer to the original. |

## 4. How each stated tension was resolved

- **Borrow elision vs unique in-place update** (minimal): resolved by
  the exclusivity-based copy-in move plus `&(. x f)` field places and the
  two unique-write primitives; the interpreter applies the same rule so
  allocations match. Owner sign-off: types §10 items 3, 16; syntax Open
  decisions 3, 4.
- **Case 15 untypable as written** (all): nominal `Knot`; syntax Open
  decision 14.
- **Case 05 heterogeneous list** (all): both closures return `i64`;
  `set!` returns `unit`; syntax Open decision 5.
- **Case 16 discards a non-unit step** (minimal): allowed; syntax Open
  decision 15.
- **Escape kinds not in closure types** (minimal, inference): accepted
  conservatism, calls through function values assume escapes; types §10
  item 5.
- **Protocol kinds cannot be inferred across modules** (minimal,
  inference): declared, default escaping, checked per impl.
- **Guaranteed tail calls vs caller-owned temporaries** (minimal,
  ownership): self tail calls only, as loops; types §10 item 6.
- **nil ergonomics** (minimal): `if-let`, no coercions.
- **"Same frees" vs in-place updates** (ownership): the interpreter
  implements the move rule and the unique-write test (types §6.12), so
  the free traces are identical, not merely the audit verdicts.
- **Case 08 soundness in the compiler** (ownership): owned cell reads
  make the retain-for-call rule unnecessary.
- **Closure colour as a type attribute** (ownership, inference): kept,
  as the lattice; types §10 item 2.
- **Executor model** (ownership): work-stealing permitted; `async`
  captures `Send`; types §10 item 9 asks the owner to confirm the reading
  of §3.4.
- **`weak`/`upgrade` as primitives** (ownership): primitives exposed
  through the prelude, `deref` spelling kept.
- **Field access on unannotated parameters** (ownership, inference):
  must resolve within the SCC; types §10 item 13.
- **Unknown-callee conservatism** (inference): accepted; types §10 item 5.
- **Coercion order** (inference): no coercions; types §10 item 12.
- **`Send (dyn P)`, `Send ptr`** (inference): both false in v1; types
  §10 item 11.
- **`first`/`nth` on empty** (inference): trap; syntax Open decision 13.
