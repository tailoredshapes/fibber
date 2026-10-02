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
