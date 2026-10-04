# last-use: stage 2's ownership plan is no longer the Rust plan (a note, not work)

The Rust is frozen (seed-1); this records a deliberate divergence so nobody reads it as a bug.

Performance batch 2, lever L1 (docs/design/in-place-update.md) gave `compiler/own` a last-use analysis the Rust pass
does not have: `compiler/own/lastuse.fib`, run once per unit on the modes of the admitting walk and consumed by the
emitting walk (`Ctx.dead`, `w-last-use?` in `own/walk/state.fib`, `consume` in `own/walk/call.fib`).

Effect on the plan, for the same program:

- an argument, a `recur` argument, a `set!`/`set-field!` value or a loop initialiser that is the last use of an
  owning binding or owned parameter of the frame is `moved` where the Rust plan says `retain`; the binding's release
  at the scope exit, the `recur` jump (`release [v] (old loop value)`) or the return is then absent on that path;
- at a branch (`if`, `match`) where one arm falls through having handed a binding over and another does not use it,
  the other arm releases it at its own end (`w-branch-merge`), so the exit that follows skips it;
- nothing else changes: the facts, the summaries, the tail-call decisions and the scope-local sets are the same
  (the events do not depend on the passes), so `fibref own --sections facts,summary,taken` still equals stage 2's.

So `fibref own` and `fibref explain` (and `fibc explain` of the seed) disagree with stage 2's on the `body` and
`explain` sections of any program with such a use, and `fibref`'s emitted lIR has more retain/release pairs than
stage 2's. Examples: `compiler/tests/own/last-use/*.fib` with their goldens (written from stage 2; the Rust has no
oracle for them), and `cases/ownership/248..256`. The 15 programs of `compiler/tests/own/golden/` contain no such use,
so their dumps are unchanged by the pass.

Not ported, and not to be: the Rust is the seed, and the seed's plan is the conservative one that this pass improves on.
