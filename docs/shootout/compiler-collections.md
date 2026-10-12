# Compiler collection updates and dense tables

Measured on 2026-10-12 on the Linux development host, after
[the structured emitter](structured-emitter.md) landed at `91ac4a12`.

Private emitter buffers, work queues, static indexes, and the unification
store now use `cell-update!`. Reading a collection out of its Cell and
updating it previously kept the Cell's reference alive during the update,
forcing a copy. Moving it out first permits an in-place update when unique.
The Send closure cannot capture the Cell. Held snapshots still force copies.

`types.table` now indexes a growable array by its consecutive integer IDs,
instead of hashing those IDs through a Map. Array updates use the established
copy-on-write primitives. Slots contain `Option a` to permit a generic empty
array initialized with nil; every allocated ID holds `some row`. This also
distinguishes a present nil row from a missing ID. Rows may contain Cells;
table mutation does not impose a Send constraint on them.

## Instruction buffer

[buffer.fib](../../scripts/bench/compiler/buffer.fib) appends 100,000 instructions
with a reused operand and checks the resulting count. The baseline and changed
builder sources were compiled with the same optimized compiler. Five runs
alternated their order under `/tmp/fibsuite.lock`.

| Measurement | Baseline | Changed |
|---|---:|---:|
| Median append time | 18.549 ms | 9.460 ms |
| Allocations in a separate traced run | 319,808 | 129,183 |

This is about 49% less time and 60% fewer allocations for this operation.
It does not measure the cost of forming each instruction or emitting LLVM.

The acceptance case `8771` separately audits 1,024 appends with a bound of
1,400 allocations. The baseline builder allocates 3,203 objects and exceeds
that bound. Snapshot tests hold instructions, phis, allocas, table rows, and
the table's backing array across later updates. Store tests retain the old
node and literal maps across binding and widening.

## Complete compiler-source handoff

Both optimized compilers read the same changed compiler sources, with compiled
object reuse off. The measured command is:

```sh
FIB_LIRO_CACHE=off FIB_TIME=1 /usr/bin/time -v "$COMPILER" \
  build compiler/fibc.fib -Icompiler -Ilib --emit llvm -O 0 -o "$OUTPUT"
```

One warm-up pair was excluded, then three pairs alternated their order under
the suite lock. The table reports medians, with copying changes and dense
tables combined. The baseline is the structured compiler at `91ac4a12`.

| Measurement | Baseline | Changed |
|---|---:|---:|
| Front end | 12.035 s | 11.093 s |
| LIR emission | 10.849 s | 10.783 s |
| LLVM lowering | 1.072 s | 1.008 s |
| Total wall time | 25.56 s | 24.57 s |
| Maximum RSS | 1,465,632 KiB | 1,459,940 KiB |

The wall-time median improves by 0.99 seconds, about 3.9%; the front-end
median improves by about 7.8%. Peak memory changes little. Every run exited
zero and produced identical LLVM after removing only `ModuleID` and
`source_filename` header lines: 41,279,314 bytes, SHA-256
`25646cdb416b07ef35004103412b5b3b61fda464372a72ed140c179e05b393d5`.

This excludes LLVM optimization, machine-code selection, and linking. It
does not establish the same improvement in complete executable builds or CI.
Sparse inference maps and the unification store's Map representation remain
separate candidates.
