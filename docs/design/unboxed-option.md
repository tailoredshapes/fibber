# An `Option` of a scalar is a value

Batch 4, lever A. Status: implemented in stage 2 (`compiler/emit`); the Rust `fibc` and `fibref` are frozen and still
box (see "What the Rust tools do").

## The problem

`(Option i64)`, `(Option f64)`, `(Option bool)` and the other scalars were a heap enum (types §8.1): `(some x)` was a
`fib.alloc` of 32 bytes, tag and payload, and the consumer's scope ended in a `fib.release`. `peek`, `get`, `nth`-like
lookups, `str-find`, `first` and `find` over scalar elements return one per call. A loop of `(unwrap (peek v))` paid an
allocation and a release per element (about 3.5 ns measured, 20 million of them in `vec-conj-pop`).

## The representation

An `(Option T)` whose payload `T` is a scalar that has a value is the lIR value `{ i1 t }`, `t` being `T`'s lIR type:

- the first member is the tag, 1 for `some`, 0 for `nil`;
- the second is the payload, and is the zero of `t` for `nil` (`(zeroinitializer { i1 t })` is `nil`);
- it is a first-class aggregate: it is returned in registers, passed as an argument, held in a `phi`, an `alloca`
  slot or a frame slot, stored in an object field and in an array element, and loaded again. lIR has had such values since
  `dyn` (`{ ptr ptr }`, spec/lir.md §2, §3, §6.4).

In the emitter this is one new `LirTy` variant, `(LirOpt t)` (`compiler/emit/value.fib`), and one new `OptRep`
variant, `OptValue` (`compiler/emit/layout.fib`). `lir-ty` of an `Option` type gives `(LirOpt t)` exactly when
`option-rep` says `OptValue`: the payload's lIR type is one of `i1 i8 i16 i32 i64 float double` or a raw `ptr` (the
types `bool`, the integers, `char`, `keyword`, the floats, a field-less `defenum`, `ptr`). Everything else is as before:

| `(Option T)` with `T` | representation |
|---|---|
| an object that is no `Option` | nullable pointer (`OptNull`) |
| a scalar with a value | the pair `{ i1 t }` (`OptValue`), new |
| `unit`, a `dyn`, a `(Weak (dyn P))`, any `Option` | heap enum (`OptBoxed`) |

So `(Option (Option i64))` is a heap enum whose `some` variant has a field of type `{ i1 i64 }`; `(Option unit)` stays
boxed because `unit` has no value to put in the second member and no lIR type to give it; `(Option (dyn P))` is boxed
because a `dyn` already is two words and the owner decided it so (types §8.1).

## Which positions are unboxed

All of them. The prompt of the lever allowed a smaller scope, with conversions at the boundary of a struct field, a `Vec`
element, a closure capture, a task, a `dyn` (the places where an object layout is involved). That is not needed and
would be less safe: every one of those places is typed by `program-lir` of the field, element, capture or result type, so
once `lir-ty` says `(LirOpt t)` they hold the pair inline, with no code of their own. A boundary is a place where two
representations meet, and a missed conversion is a memory error; with one representation there is none to miss. The size
of the slot changes (a `Vec (Option i64)` holds 16 bytes an element where it held an 8-byte pointer to a 32-byte
object), and `size-align` knows the pair's layout (`{ i1 t }`: the tag byte, then `t` at its alignment).

The positions, each with the code that makes it work:

- **return, argument, let-bound local, `phi`, loop variable**: a value of type `(LirOpt t)`; nothing but the type changed.
- **`match`, `if-some`, `when-some`, `unwrap`, `nil?`** (the library's are `match`es over `Option`): `value-option` in
  `emit/lower/pattern.fib` reads the tag with `extractvalue 0` and a `(some p)` sub-pattern binds `extractvalue 1`.
- **`(some x)`, `nil`**: `construct-value-option` in `emit/lower/objects.fib`: `{ (i1 1) x }` and the zero constant. The
  site's plan (`AlHeap`) is ignored: nothing is allocated.
- **struct field, enum variant field, `Vec`/`Array` element, `Cell`, `Atom`, closure capture, task capture, task result,
  frame slot of a task live across an `await`, protocol method result and argument, vtable function**: the lIR type of the
  slot is `(LirOpt t)` (`program-lir`); loads, stores, `phi`s and the frame are generic over the type's text.
- **counts**: none. `lcx-retain`, `lcx-release`, `lcx-steal` and `end-stack` act on `ptr` and `dyn` values only
  (`lcx-obj-word`), and the walkers of `drop`, `trace` and `share` take only `LirPtr` and `LirDyn` slots as children
  (`emit/objects/walk.fib`), so a pair is no child and nothing is counted.
- **`atom` compare-and-swap**: `same-bits` compares the tags and the payloads (`emit/lower/cells.fib`); `icmp` is not
  defined on an aggregate.
- **builtins that return an `Option`**: `str-find` builds the pair from `fib.str-find`'s offset (`select`, no branch,
  `emit/lower/strfind.fib`). The other builtins that produce options do so through `match`/`some` in the library, or return
  a pointer-represented one (`@w` of a weak box: `upgrade`).
- **`def` constants** (the JIT of `emit/defs/jit.fib`): a def of type `(Option scalar)`, or a struct, enum or array
  holding one, is read out of the JIT's memory: the tag byte and the payload at its offset (`DrOpt`); the entry of a def of
  that type is two functions, `fib.defentry.K` (the tag) and `fib.defentry.K.v` (the payload), because lair's `call` returns
  an `i64` or a `double` only; the constant is `{ (i1 tag) payload }`. Case 222 covers it.
- **global slots of run-time defs** and **a task's result slot**: initial value `(zeroinitializer { i1 t })` (`lir-zero`).

## How the emitter decides

Only by the type: `lir-ty` (layout.fib) is the one place. Nothing in the ownership checker (`compiler/own`) changed, and
the checker's model is unchanged: it still plans an `(Option scalar)` as a heap object (`OrHeapEnum`, `AlHeap`, a retain
and a release for each owned use). That plan is a safe over-approximation of what the emitter does with a value that has no
count: every retain and release the plan names on such a value is a no-op in `lcx-retain` and `lcx-release`, and
`OpEndStack`, which would be an error on a value with no object word, is never planned for an `Option` (types §6.11 says an
`Option` is never a stack candidate; no case of the suites reached that error). A later change may teach `own` that these
are not objects, which would shrink the plans, not change the code.

## Generic code

Bodies are monomorphised and keyed by the layout class of an unconstrained type variable (types §4.3, compiler.md §7). The
class of `(Option i64)` was `box`; it is now a scalar class (the lIR type `(LirOpt i64)`), whose representative type
(`representative` in `emit/mono.fib`) is `(Option i64)` again, a pair when lowered. `box` is left for `(Option unit)`,
`(Option (dyn P))` and an `Option` of an `Option`, and its representative is `(Option (Option i64))`, a heap enum (it
was `(Option i64)`, which is a pair now). The invariant of §4.3 is kept: for every type the class of a variable is, the
representative of that class has the same representation, so `(Option a)` is built as a pair, a nullable pointer or a heap
enum by the body exactly as its caller built it. Case 220 (the stage-1 fix s1a) is the pin, and it found the
representative of `box` the first time it ran.

## What still boxes

`(Option unit)`, `(Option (dyn P))`, `(Weak (dyn P))` inside an `Option`, and every `Option` of an `Option`: a heap enum, as
before. They are not scalars with a value.

## Soundness

An `Option` of a scalar holds no pointer, so it has no count, no identity and no lifetime: copying it copies the value.
Everything that was memory safety for the heap enum (a count that reaches 0, a stale pointer) cannot happen to a pair. What
can go wrong is a representation mismatch between a producer and a consumer; there is one representation per type, chosen
by `lir-ty`, and the lIR checker (whole-module, `lair`) rejects a call or a store whose lIR types differ (case 220 failed
that way, at compile time, before the representative of `box` was fixed). The memory audit of every case in
`cases/ownership` (262 plus the new ones) and `cases/stdlib` ran clean.

## What the Rust tools do

`fibref` and the stage-1 `fibc` are frozen: they still allocate a heap enum, so a case that asserts a small `allocs: <= N`
for a loop of scalar Options passes in stage 2 only and carries `;; stage: 2` (the convention of
compiler/mirror-pending/X9c-stage2-only.md; `fibref cases` reports it as a header error). The semantic cases (268, 269) pass
in all of them: the interpreter is the oracle for the values.

This is a divergence from method.md rule 3 and types §4, which say the interpreter's trace and the compiled run's are the
same lines: for a program that makes an `Option` of a scalar, the compiled count is lower. The `<=` bound is the harness's
guard (a compiled run may allocate less than the bound, never more); spec/compiler.md §8 item 13 records it.

## Not done

- `own` does not know (see above); the `allocs` it plans for a scalar `(some x)` are ignored.
- `Option unit` is boxed. `(Option unit)` is rare; it would need a tag-only `i1` representation.
- No `Option` of a pair of scalars or of a small struct. That would be a general "small struct by value" and is not part of this lever.
