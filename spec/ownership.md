# Ownership

**Decided:** borrow first, count second.

## 1. Values

- **Scalars** (integers, floats, booleans, characters) are copied. They
  have no identity and no lifetime beyond the binding holding them.
- **Objects** (strings, structs, collections, closures, cells) live on
  the heap, or on the stack when the compiler proves that is safe. Every
  object that can escape carries a reference count.

Objects are **immutable** unless they are a **cell** (§6). Immutable
objects can only refer to objects that already existed when they were
built, so immutable objects cannot form cycles.

## 2. The semantics is counting; borrowing is how it is made cheap

The meaning of a program is defined as if every reference to an object
were counted:

- binding or storing a reference adds one;
- a reference going away (its scope ends, or the object holding it is
  freed) subtracts one;
- an object is freed when its count reaches zero, releasing everything
  it refers to.

Under that definition no program without `unsafe` can use freed memory,
free twice, or leak anything except a cycle through cells (§6).

The compiler then removes the counting wherever it can prove it makes no
difference. That is the "borrow first" part, and it is an optimisation,
not a rule the programmer has to satisfy. Its guaranteed minimum:

- **Scope-local objects are not counted.** An object created in a scope
  that never escapes it (§3) is freed when the scope ends, with no count
  operations, and may be allocated on the stack.
- **Parameters are borrowed** unless the checker infers them **owned**
  (types §6.4): a parameter that the function returns, stores, captures
  in a closure on the heap or hands to another thread is owned, and so
  is one whose count a tail call must carry, the caller's frame being
  gone while the callee runs. The caller hands an owned parameter's
  count over and the callee releases it or hands it on; a borrowed
  argument costs nothing to pass, and the callee counts only the parts
  of it that it returns or stores. The reference interpreter follows
  the same convention (types §6.12).
- **Unique updates happen in place.** Updating an object whose count is
  one reuses it instead of copying (§5).

The reference interpreter implements the plain counting semantics. The
compiler must produce the same results and the same frees, just with
fewer count operations. This is how "borrow first" is checked.

## 3. Escape

A reference **escapes** its scope when it is:

1. returned from the function;
2. stored into an object (a struct field, a collection, a cell);
3. captured by a closure that itself escapes;
4. passed to another thread (`plet`, `pmap`, a task);
5. held across an `await` (§8);
6. passed as an argument of a tail call (§4): the caller's frame is
   gone while the callee runs.

A reference that escapes is counted (+1) at the point of escape. Nothing
else is.

## 4. Functions

- A parameter is a **borrow** unless the checker infers it **owned**
  (§2). The callee never frees a borrowed parameter: it is valid for
  the whole call. An owned parameter arrives with one count and is the
  callee's to release or hand on. Which one applies is inferred and
  printed by the checker (types §6.4), never written. It changes no
  result and no verdict, but it decides whether an argument's count
  dies at the callee's exit or after the caller's call, so it is a
  calling convention that the reference interpreter follows too (types
  §6.12): the two free the same objects at the same points.
- Every result is **owned**: the caller receives +1 and is responsible
  for it.
- Returning a parameter, or something reachable from a parameter, is an
  escape (§3.1), and the result carries its own count. A parameter that
  is returned is owned: where it is itself the function's value, its
  count is moved out to the caller; where it meets another value in a
  branch join, the join retains it and the parameter is released at the
  exit (case 04). A part of a parameter is retained before it is
  returned (case 01).

This is what makes returning part of an argument, or choosing between a
borrowed and a fresh value on different branches, safe (cases 01, 04).

## 5. Mutation

Immutable objects are "updated" by making a new version (`conj`,
`assoc`). Persistent collections share structure between versions; the
counts on the shared nodes keep them alive for as long as any version
needs them (case 02).

A **mutable parameter** `&v` is copy-in, copy-out. The callee's `v` is
a private cell, initialised from the caller's variable (written
`(f &x)` at the call site) and read with `@v`; `v` itself is never a
value. When the call returns, `x` is assigned whatever `v` holds. The
call does not write `x` before the write-back, so `(f &x @x)` is fine:
the plain borrow stays valid until the write-back (case 17). Updates go through the object's count:

- count is one → the object is updated in place;
- count is more than one → it is copied first, and the copy is updated.

So a caller that is still using the old value (an iteration, another
binding) keeps seeing the old value, and nothing it holds is ever freed
or moved under it (case 08).

**Decided:** the `&` arguments of one call must name distinct
variables. `(bar &x &x)` would write back to `x` twice at return; with
copy-on-write inside `bar`, whichever parameter was updated second wins
and the other update is silently lost. No ordering gives that call one
meaning, so it is a compile error (case 12). The check is on names; no
alias analysis is done. A cell can still reach one call under two
different names (an alias of the cell, a capture, a struct field that
holds it). Such a call is memory-safe, since every copy-in holds its own
count, but its meaning is order-dependent: the callee may write the cell
through the other name during the call, and the write-backs run in
parameter order, so the later one wins. This replaces liar's ADR 007 ("aliasing
allowed"). Aliasing of *objects* stays fine because mutation copies
when the count is above one: `(bar &x &y)` with `x` and `y` holding the
same object gives each variable its own result.

**Decided:** an `&` parameter may not be captured by a closure that
escapes the call (case 18). The closure would keep mutating a private
cell after the write-back has already happened.

## 6. Cells and cycles

A **cell** is the one mutable container: `(cell v)`, read with `@c`,
written with `(set! c v)`. Captured mutable variables are cells, so two
closures capturing the same variable share one cell (case 05). Atoms
(§7) are cells that are safe to use from several threads.

Cells are the only way to build a cycle: store something that refers to
a cell back into that cell.

**Decided:** an object unreachable except through a cycle of cells is
garbage. The implementation is not required to reclaim it; if it does,
the timing is unspecified. A leaked cycle is not a memory-safety
violation, and the reference interpreter reports such leaks separately
from every other audit failure (case 15).

The standard library provides weak references for structures that need
back-pointers: `(weak x)` makes a weak reference to `x`, and `(deref w)`
returns a strong reference, or `nil` once the object is gone (cases 19,
20). Only objects that ever had a weak reference pay for one.

Named functions are global and capture nothing, so ordinary recursion
never creates a cycle; a local recursive closure calls itself through
its own code pointer and environment rather than capturing itself.

A cycle collector over cells only (trial deletion; only cells can be
cycle roots) can be added later as an implementation improvement
without changing this rule. The audit's leak reports from real programs
decide whether it is worth its cost. Forbidding cell-containing values
inside cells was rejected: it rules out parent pointers and most graphs.

## 7. Threads

An object reachable from more than one thread is **shared**. Crossing a
thread boundary (§3.4) marks the object and everything reachable from
it shared, and shared objects use atomic count operations. Objects that
never cross stay on the cheaper non-atomic path.

Only objects that are immutable, or atoms, may cross (liar's "closure
colour" becomes a check on what a closure captures). A cell that is not
an atom may not cross.

An atom holds a counted reference. `@a` returns a retained reference to
the current value. `swap!` and `reset!` replace it and release the old
one; a reader still holding the old value keeps it alive through its own
count (case 10).

**Implementation obligation:** reading an atom's pointer and retaining
it must be one atomic step with respect to `swap!` releasing it.
Otherwise the value can be freed between the two. The implementation
may use a per-atom lock or deferred reclamation; the spec does not
choose.

**Decided:** atoms never take part in transactions. Values that must
change together live in one atom (a struct or map) and change with one
`swap!`, which is already all-or-nothing (case 16).

`dosync` is reserved for a separate transactional `ref` type, deferred
until after the core. Building it on atoms does not work: atomicity has
to cover readers too, so every `@a` in the program would have to
consult the transaction system; `swap!` would mean something different
inside and outside a transaction; and retries would re-run whatever
else the block contains. A `ref` may only be changed inside `dosync`
and reads of refs always see one consistent snapshot. Counting makes
that snapshot cheap to keep: an old version stays alive exactly as long
as a transaction is still reading it.

## 8. Async

An async function's frame outlives the call that started it, so borrows
cannot be held across an `await`. Any parameter or local used after an
`await` is retained on entry to the async function. `&` parameters are
not allowed in async functions (case 14). Case 11 tests the retain on
entry.

## 9. Unsafe

`unsafe` allows raw pointers, manual allocation and foreign calls. It
does not turn off counting or type checking for fibber objects. Foreign
code receives borrows valid only for the duration of the call, unless
the foreign interface says the reference is retained.

## Cases

Every rule above is exercised by at least one file in
[`cases/ownership/`](../cases/ownership/). Each case names the section
that decides it.
