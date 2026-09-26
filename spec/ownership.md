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
- **Parameters are borrowed.** Passing an object to a function costs
  nothing. The callee counts it only if it makes it escape.
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
5. held across an `await` (§8).

A reference that escapes is counted (+1) at the point of escape. Nothing
else is.

## 4. Functions

- Every parameter is a **borrow**: valid for the whole call, never freed
  by the callee.
- Every result is **owned**: the caller receives +1 and is responsible
  for it.
- Returning a parameter, or something reachable from a parameter, is an
  escape (§3.1): the callee retains it before returning.

This is what makes returning part of an argument, or choosing between a
borrowed and a fresh value on different branches, safe (cases 01, 04).

## 5. Mutation

Immutable objects are "updated" by making a new version (`conj`,
`assoc`). Persistent collections share structure between versions; the
counts on the shared nodes keep them alive for as long as any version
needs them (case 02).

A **mutable parameter** `&x` is in-out: the callee may replace `x`, and
the caller's binding holds the new value when the call returns.
Updates go through the object's count:

- count is one → the object is updated in place;
- count is more than one → it is copied first, and the copy is updated.

So a caller that is still using the old value (an iteration, another
binding) keeps seeing the old value, and nothing it holds is ever freed
or moved under it (case 08).

**Proposed:** the same binding may not be passed to two `&` parameters
of one call. With in-out semantics that call has no single meaning.
This replaces liar's ADR 007 ("aliasing allowed").

## 6. Cells and cycles

A **cell** is the one mutable container: `(cell v)`, read with `@c`,
written with `(set! c v)`. Captured mutable variables are cells, so two
closures capturing the same variable share one cell (case 05). Atoms
(§7) are cells that are safe to use from several threads.

Cells are the only way to build a cycle: store something that refers to
a cell back into that cell.

**Proposed:** cycles through cells are not collected and leak. A leak is
not a memory-safety violation. The reference interpreter reports such
leaks separately from other audit failures, and the standard library
provides weak references for structures that need back-pointers. Named
functions are global and capture nothing, so ordinary recursion never
creates a cycle; a local recursive closure calls itself through its own
code pointer and environment rather than capturing itself.

Alternatives, if leaking is unacceptable: forbid storing values that
contain cells into cells (restrictive), or add a cycle collector for
cells only (a partial GC). Needs a decision.

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

**Proposed:** no `dosync` over atoms. Coordinated multi-value updates
put the values in one atom (a map or struct) and update them with one
`swap!`. A true transactional `ref` type can be added later if needed,
with its own rules.

## 8. Async

An async function's frame outlives the call that started it, so borrows
cannot be held across an `await`. Any parameter or local used after an
`await` is retained on entry to the async function. `&` parameters are
not allowed in async functions (case 11).

## 9. Unsafe

`unsafe` allows raw pointers, manual allocation and foreign calls. It
does not turn off counting or type checking for fibber objects. Foreign
code receives borrows valid only for the duration of the call, unless
the foreign interface says the reference is retained.

## Cases

Every rule above is exercised by at least one file in
[`cases/ownership/`](../cases/ownership/). Each case names the section
that decides it.
