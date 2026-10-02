# Mirror pending

M6 resumed on 2026-10-02 (owner: "don't get stuck behind verification").
A Rust package of M7 that changes the expander or the prelude no longer
mirrors the change into `compiler/expand/` in the same commit; it writes
`compiler/mirror-pending/<package>.md` instead, listing for each change
the Rust file and function, what it does now, and the rows or cases that
show it, so that the M6 porters re-sync the expander in one batch (written
in the idiom the new tools allow) and turn `bootstrap_expand` back on.

`bootstrap_expand` is gated by the `mirror` feature of `crates/fibc`
(off by default): `cargo test -p fibc --features mirror --test
bootstrap_expand`. The reader's test, `bootstrap`, is unaffected and stays
on. A library change does not need a mirror entry unless it adds a macro.
The baseline before the gate (the whole library of tranche 2 through
stage 2a) is recorded in ROADMAP M6.

Status 2026-10-02: X3, X4 and PRE are mirrored (E1: compiler/expand/params.fib,
compiler/expand/prelude/{guard,vector,extremum,text,trylet,comp}.fib and the
edited rows; `cargo test -p fibc --features mirror --test bootstrap_expand`: 79
passed in the full run, and the one that failed there, the planted-fault replay of
the whole corpus, passes alone in 282 s: it timed out under load). Their files here
are deleted. S1.md (a change of the Rust checker, not the expander) is the porters'
note for types.infer.general and is done too (P11b); it stays until the types
stage is closed.
