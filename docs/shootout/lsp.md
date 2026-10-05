# Language server: latency, size and memory

Measured 2026-10-05 on the 28-core build host, the server built from `compiler/lsp.fib` by the v0.1.5 seed, `ulimit -v 8000000`.
Commands: `node compiler/tests/lsp/load.js SERVER BYTES [CHANGES]` (one big valid buffer of defuns), and for a real file a request of each kind
at one position after `didOpen` (`SARGS="-I compiler"` gives the server its roots). Times are wall clock at the client, one run, not medians.

## What costs what

One whole check of a typical buffer is about 560 ms, and nearly all of it is the library, which every check rebuilds (phases of
`scope.fib`, measured with a small program over the same passes): prelude 7 ms, load 15, expand 80, lower 56, infer 150, ownership 270.
The on-demand library mode (`own-check-lowered-with` with the library flags) is 2.3 times faster (175 ms against 400 ms for the same buffer)
but its program holds fewer names (a completion of 55926 bytes against 64481), so the results are not the same and it is not used.
A cached library table, built once per server process and shared by every check, needs the type checker and the ownership pass to take a
checked library as their starting point (today `infer-lowered-with` and `analyse` walk every module of the program in one run over one mutable
`Globals`); that is the lever that would take an edit under 200 ms, and it is not done.

What is done: the server keeps the last four whole runs, keyed by the text run (`LsCache` in `lsp.analysis`; two for a buffer over 100 KB), so a
request on the text the diagnostics were made from does not check again, and the diagnostics of a buffer no longer build the program that
completion needs (`diagnose`). A buffer whose error is in a module it requires goes straight to the last rung of the ladder.

## Typical files (after `didOpen`, same text)

| file | bytes | didOpen (first check) | completion | hover | definition | documentSymbol |
|---|---|---|---|---|---|---|
| `compiler/lsp/scope.fib` (checks) | 12 KB | 562 ms | 18 ms | 0 ms | 1 ms | 3 ms |
| `cases/ownership/193-...fib` (checks) | 9 KB | 356 ms | 15 ms | 4 ms | - | 2 ms |
| `compiler/lsp/features.fib` (a required module is pending) | 8 KB | 650 ms | 351 ms (first), then 4 ms | 4 ms | - | 0 ms |

Before the cache every request re-checked: completion of `scope.fib` 508 ms. A first completion on a buffer that does not check costs one more
check per rung it needs (a broken buffer 400 ms to 1.2 s); an edit costs one check (360 to 980 ms by size) whatever follows.

## Large buffers (`load.js`, valid defuns, `-I compiler`)

| buffer | didOpen | completion | hover | definition | documentSymbol | didChange (each) | server RSS |
|---|---|---|---|---|---|---|---|
| 100 KB | 744 ms | 81 ms | 2 ms | 21 ms | 78 ms | 780 to 980 ms | 80 MB, then 180 MB (four runs kept) |
| 500 KB | 3.3 s | 440 ms | 4 ms | 117 ms | 485 ms | 2.5 to 2.8 s | 230 MB, then 536 MB (two runs kept) |
| 1 MB (measured before the item cap) | 13 ms (not checked) | 0.75 s | 62 ms | 247 ms | 0.77 s | 12 ms | 160 MB |
| 10 MB | 135 ms (not checked) | 2.7 s | 0.6 s | 2.3 s | 1.6 s | 120 ms | 0.98 GB |

Over `max-analysed-bytes` (512 KiB) a buffer is one diagnostic ("not checked") and the requests answer from the reader's scope and the
library; completion and document symbols stop at 20000 items (a 10 MB file made answers of 20 and 46 MB and took 5 and 14 s before the cap).
The bound is on size, not on time: a `match` of n arms is checked in about n squared steps (10000 arms 8.4 s, 50000 over 60 s) and a `let` of
50000 bindings takes 14 s, so a buffer under the bound can still take minutes; a client needs its own timeout.

## Stack

The reader refuses source nested deeper than 1000 levels with a diagnostic, and depth 990 of `(+ 1 ..)`, `(let ..)`, `(Vec ..)` checks. A macro
can make depth from a flat source: `and` or `or` over about 9000 arguments overflowed the 8 MiB stack in the ownership checker (SIGSEGV, found
by `fuzz.js`). The server now re-executes itself once with the soft stack limit at 1 GiB: `and` over 500000 arguments (2.5 MB, 28 s) checks;
over 2000000 it still overflowed, which the size bound keeps out of reach (512 KiB is at most about 100000 arguments).

## Handler isolation

`FIB_LSP_ISOLATE=1` runs each handler in a task and asks it with `try-join` (docs/design/exceptions.md section 8): a trap becomes `-32603` with the
trap's message. The task copies in the open documents and the roots and gets an empty cache (a `Cell` cannot cross to a task: the compiler
refuses the closure that captures the server), so every request checks the library again. `scope.fib`, completion after `didOpen`: 17 ms default,
793 ms isolated (a check is 546 to 795 ms); the spawn and join itself is not visible in those numbers. Off by default for that reason.
