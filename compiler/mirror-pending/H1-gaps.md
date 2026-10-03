# H1: cases that `F cases` (stage 2) does not pass yet

`F cases cases/stdlib` at the commit that added `compiler/driver/harness.fib` prints
`951 cases: 914 pass, 13 fail, 0 pending, 0 header error, 24 open`; `fibc cases` at seed-1 has 927 pass.
The 13 are the cases that use something the stage-2 ports lack. No case was edited.

| Cases | Why stage 2 fails | What closes it |
|---|---|---|
| 3600 .. 3609 (ten cases of `fib.unix`) | rejected: `unbound name sys-open` (and the other `sys-*`): S5's fd builtins are in `fibref` and stage 1 only | S5's builtins ported to the stage-2 type checker and emitter (not done on purpose in H1) |
| 1850, 1851 | rejected: `apply-to takes 3 or 4 argument(s), got 2`: a `:borrow` after a parameter's type is not part of that parameter in `compiler/expand/params.fib` | X9b item 1 (`mirror-pending/X9b.md`) |
| 1707 | `audit: expected clean, got ... leaks=2`: the audit of a stage-2 run is read from the free trace, which cannot tell an object that a `def` still holds at exit (not a leak, spec stdlib §7 L15) from a leak | an audit for compiled runs that knows the defs: the runtime marking what a def holds in its trace, or the interpreter's immortal set in fibber |

## What the harness decides that the Rust one does not need to

Stage 2 has no interpreter, so a case is judged on its compiled run alone (the Rust harness also runs the interpreter and
requires the two to agree: its rule 6 half is not in stage 2).

- The audit is read from the trace on standard error of `F run --trace`: leaks are `A` lines never freed (after the last
  `I k` marker, as `Trace::parse` does), errors are frees of an object not live. `audit: clean` is no leaks and no errors.
- `audit: leak-cycle` holds when something leaked and nothing erred: the trace cannot tell a cycle through cells from another leak,
  so a case that leaks without a cycle would pass as `leak-cycle`.
  An `open` case whose audit leaks without an error counts as the program's own failure (OPEN, not FAIL).
- The trace is not compared with anything, so the "traces agree" check of rule 6 is not made.
- The child keeps `FIB_LIB` (the Rust harness removes it because its library is embedded).
- A run is limited to 300 seconds by an alarm (as the Rust harness's limit); the status of the signal is read as the timeout.
- Linux and glibc only: the child is `/proc/self/exe`, directory entries are read from `struct dirent` by offset.
