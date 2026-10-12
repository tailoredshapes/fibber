# The test pyramid

Why: a cold full gate is 453 s wall at `-j12` (stamps of a quiet run: `build/**/*.ok` hold seconds). Most of the CPU goes to tests that check
ONE pass of the compiler but start a process, or an LLVM native build and link, per test. The rule of this file: a test of one pass runs in
memory, in one process, from the previous layer's output; only a thin top runs the whole toolchain.

All times below are measured, on the box that built `main-gate71` (28 cores, load about 5) unless marked "solo".

## 1. The layers as they are

| Layer | What | Tests | Cost in the gate (s, CPU of stamps) | Shape |
|-------|------|-------|-------------------------------------|-------|
| unit modules | `compiler/tests/*/unit-*.fib`, `fib.test.unit` | 78 modules, 3188 assertions | before: units-emit 38, units-own 34, units-types 34, units-rest 11, units-pending 96; now `units-all` (one executable), `units-rest` (control), `units-pending` | one native executable for all, run once |
| golden, reader | `reader-dump`, `reader-print` | 1184 + 1184 inputs | 12 + 9 | one process per input |
| golden, expand | `expand-porter`, `expand-e1`, `expand-r` | 944 + 91 + 160 | 123 + 14 + 26 | one process per input (before this slice) |
| golden, types / own / explain | `types-infer`, `types-top-*`, `own-*`, `explain-amp` | 54, 29+34, 6+2, 7 | 28, 7+8, 2+3, 3 | one process per input |
| golden, emit | `emit-fns`, `emit-defs` | 105, 3 | 84, 3 | one process per input |
| golden, lair | `lair-ast`, `lair-check`, `lair-llvm` | 384, 360, 326 | 7, 6, 6 | one process per input |
| cases | `fibc cases` over `cases/ownership` (376), `modules` (33), `stdlib` (1502) | 1911 | ownership 4 shards about 20 each, stdlib 16 shards about 38 each | compile and run each, natively |
| end to end | sh tests of the driver, stack, repl, serve, lsp, wasm, static, release | about 40 stamps | 10 to 220 each | whole toolchain |

### What the unit time is made of (measured, solo, one group: `UNITS_DIR=types`, 40 modules)

| Step | Seconds |
|------|---------|
| `fibc emit` (read, expand, types, own, emit of the group and of the compiler it imports) | 9.6 |
| `fibc build --time`: front (read, expand, types, own) 5.8, lir.emit 3.0, lir.check 0.3, link 0.13; LLVM objects are cache hits (19 hits, 0 misses) | 12.7 build |
| run of the executable (`--isolate fork`) | 4.1 |
| `fibc run` of the same file (JIT, same suite, same 40 results) | 19.4 |

So the assumption "dominated by LLVM and link" is false for the units: the front end of the group and of the compiler it tests is 9 of 12.7 s, and the
JIT path pays the same front end, so it is slower (19.4 s) than build plus run (16.8 s). All 78 modules in one executable: build 36 s (front 14.5,
emit 14.5), run 7 s, against 38 + 34 + 34 + 11 in the gate for the four groups (one group measured solo: 12.7 build, 4.1 run): the front end scales with the
number of test modules, not with the number of groups, so merging buys little and costs the parallelism. Neither was adopted.

### What the expand-golden time is made of

`expand` on an empty file takes 0.12 s; on one porter input 0.11 s; on two inputs in one process 0.24 s; on 20 inputs 1.8 s. Per file, not per
process: the tool expands the expander's prelude, `lib/prelude.fib` and the four implicit library facades (with what they read) in a fresh context
for every file (`expand.dump` `dump-program`). Process start is under 2% (strace: 58 futex, 3477 mprotect, nothing else of weight).

## 2. The target

```
            end to end   (few: the toolchain as a user runs it; native build, link, run)
          cases          (compile and run a whole program, verdict in the header)
      layered fixtures   (a layer starts from the previous layer's in-memory output)
   in-memory units       (one process; a pass called as a function on values)
reader -> expand -> types -> own -> emit
```

- Unit layer: a test of one pass calls the pass as a function in the process that runs all of them (`fib.test`), on values it builds. No file
  is read, no process is started, no library is loaded again.
- Fixtures: the expensive state that every test of a layer starts from is built once, as a persistent value, and each test takes a copy
  (`serve.snap` `ctx-snapshot`: the expander's context is cells of persistent maps). The types layer starts from the expand layer's
  output for the library, the own layer from the types layer's, the emit layer from the own layer's.
- Top: a few programs through the real `fibc build`, so the toolchain is still tested as built. These stay one process per test on purpose.

## 3. First slice (TEST-1): what was converted

`expand --batch` (compiler/expand.fib, `Batch` in compiler/expand/dump.fib): one process expands many files. The context after the expander's
prelude and `lib/prelude.fib` is built once; the context after the implicit modules (keyed by their files) is built once; each file starts from
a snapshot of the latter and expands only its own modules. The output is each file's section as before plus the line `#### status N`. A dump
with `--prelude`, `--context`, `--implicit` or a macro runner is not shared (it takes the old path). `golden.sh` runs the three expand suites
this way (`GOLDEN_BATCH=0` restores one process per input) and runs every `GOLDEN_CONTROL`-th input (default 40) also as a process of its own,
failing if the two blocks differ by a byte: the control that the modes agree.

| Stamp | Before (s) | After (s) | How measured |
|-------|-----------|-----------|--------------|
| `golden/expand-porter` (944) | 123 (gate); 95.7 per-process, solo | 16.4 batch, solo; whole `golden.sh --only expand-*` 23.6 wall | `agree.sh`-style: per-process loop vs `expand --batch`, outputs compared with `cmp` |
| `golden/expand-r` (160) | 26 (gate); 16.6 solo | 1.9 | same |
| `golden/expand-e1` (91) | 14 (gate); 8.8 solo | 1.3 | same |
| `tools/units-pending` (5 modules) | 96 (gate); 40.2 solo | 15.2 solo | `UNITS_PENDING_JOBS` (default 5) builds at a time |

Agreement: per-process and batch give byte-identical output and status on all 944 + 91 + 160 inputs (40 122 246, 77 225 and 294 555 bytes).
The goldens did not change.

Also found and fixed: `units.sh` decided that a pending unit "passes now" from a line `ok` that the runner of `fib.test` no longer prints, so
a stale pending entry was never reported. It now uses the exit status of the executable (the runner exits 0 only when every scenario held).

### Power of the converted tests (faults planted in the working tree, reverted afterwards)

| Fault | Suite | Result |
|-------|-------|--------|
| `compiler/expand/ctx.fib` `ctx-gensym`: `+ ... 1` to `+ ... 2` | expand-e1, expand-r, expand-porter | all three FAIL (`sym "#k.34"` differs from the golden) |
| `compiler/serve/snap.fib`: gensym counter not copied (`(cell 0)`) | same | all three FAIL: "batch and per-process output differ on ..." (the control catches a snapshot that forgets state) |
| a passing unit moved to `units.pending` | units-pending | FAIL "passes now: move it from units.pending to units.run" (was silently PENDING before the fix) |

## 4. Second slice (TEST-2)

### Where the unit-group time really goes (measured; the first slice said LLVM and link, then front end: both partly wrong)

| Step (all 78 modules in one executable) | Seconds |
|---|---|
| front (read, expand, types, own) | 14.5 |
| of which expansion (serve `check` stats on the `types` group: expand 1.4, load 0.1, check 4.1, lower 0.5, total 6.2) | about a fifth |
| lir.emit | 14.5 |
| LLVM, objects in the cache (warm) | 0 |
| LLVM, cache off, `-O 2` | 225 s build in all |
| LLVM, cache off, `-O 1` | 108 s build in all, and the run is as fast (4.5 s, against 4.7 at `-O 2`) |
| LLVM, cache off, `-O 0` | 51 s build, but the run is 14.6 s |

So the expander's context snapshot (`serve.snap`) cannot be the lever for the units: expansion is a fifth of the front end, and the type and
ownership checks of the whole program and the lowering are not kept by the serve engine (dev-loop.md 8.1, "Not kept"). What is shared by merging is the
front end and the lowering of the compiler code the groups have in common; what is saved by `-O 1` is the cold LLVM cost.

Converted: `units-emit`, `units-own` and `units-types` are gone as stamps; `units-all` builds every unit of `units.run` as ONE executable at `-O 1`
(`UNITS_DIR=run UNITS_MERGE=1 UNITS_OPT=1`), and `units-rest` stays a group built the ordinary way at `-O 2` (the control: the toolchain still builds
an executable the old way). `UNITS_DIR=emit|own|types|rest` still works by hand. The reports agree: the same 78 suites, 3188 assertions,
every row's status and every step's text and status equal (`IDENTICAL`, from the four group reports against the merged one).

| Stamp (solo, warm cache, same box and load) | Before (s) | After (s) |
|---|---|---|
| units-emit (build+run) | 31.9 | merged into units-all |
| units-own | 15.1 | merged |
| units-types | 27.0 | merged |
| units-rest | 5.6 | 5.6 (the control, unchanged) |
| the four together | 79.6 | units-all 45 (build 39, run 6) plus units-rest 5.6 = 50.6 |
| the same in the gate-budget baseline (33 + 21 + 26 + 7) | 87 | not re-measured in the gate |

The target "well under 40 s" for the four stamps is not met: 45 s for the 78 modules and 50 with the control. The remainder is the front end and lowering of
the compiler under test, which only a persistent checker (DV6) would cut.

### `types --batch`, `emit-dump --batch`

The same idea as `expand --batch`, in `compiler/expand/shared.fib` (`Shared`): the context after the preludes, and the context after the implicit
modules together with those modules as expanded (what the checker takes), kept as snapshots under a key of the implicit modules' files; a dump with
`--prelude` takes the old path. `golden.sh` runs the suites of the three tools in one process each, with the per-input control (`GOLDEN_CONTROL`, default 40).

| Stamp (solo) | Before, per process (s) | After, batch (s) |
|---|---|---|
| types-infer (54) | 16.7 | 13.3 |
| types-top-sections (34) | 5.9 | 2.8 |
| types-top-ast (29) | 5.0 | 2.5 |
| emit-fns (105) | 59.1 | 55.2 |
| emit-defs (3) | 2.2 | 2.6 (noise) |

The gain is small for `emit-fns`: its 0.55 s per input is the type check, ownership check and lowering of the library definitions the program
reaches (`FIB_PRUNE=0` lowers all of them), none of which the expansion snapshot keeps. The goldens are byte-identical in both modes (`ok` for each suite, both ways).

Not converted, and why: `reader-*` (8 ms per input) and `lair-*` (15 ms per input) have no front-end cost: the process start is all there is.
`own-*` and `explain-amp` (0.17 s per input, 15 inputs, about 5 s together) would gain at most the expansion, a few tenths of a second: not worth a mode.

### Power (faults planted, reverted)

| Fault | Suite | Result |
|---|---|---|
| `compiler/emit/names.fib:69` `"d."` to `"D."`, `compiler/types/ty.fib:104` `SI64` to `SI32` | units-all | 25 scenarios FAIL (emit-unit-names, -defs, -layout, -mono, -program, own-unit-driver, ...), exit 1 |
| `compiler/types/ty.fib:104` `SI64` to `SI32` | types-infer, batch | FAIL against the golden |
| `compiler/emit/names.fib:69` | emit-fns, emit-defs, batch | both FAIL against the golden |
| `compiler/expand/shared.fib` keeps no expanded implicit modules | types-infer, emit-fns, batch | both FAIL: "batch and per-process output differ on ..." (the control) |
| a snapshot that drops `gensyms` or `macros` of the context | types-infer, emit-fns | NOT caught: the inputs of those suites expand no gensym and no user macro of the library after the snapshot, so the suites do not exercise that state; `expand-*` suites do (slice 1) |

## 5. What remains

| Item | Cost now (s) | Estimated gain (estimates, not measured) | Note |
|------|--------------|----------------|------|
| `emit-fns` | 55 | most of it, with a lowering cache for the library definitions | the same library defs are lowered 105 times |
| units-all | 45 | needs a persistent type and ownership checker (DV6) | `-O 1` and merging are done |
| `reader-*`, `lair-*`, `own-*` | 30 | small | process start only |
| cases (stdlib 16 shards) | about 600 CPU | large, but each case is an end-to-end program on purpose | a `fibc cases` that builds all cases of a shard as one module is future work |

## 6. The rule for a new test

Put a test at the lowest layer that can fail for the fault it is about.

1. A rule of one pass (reader, expander, type checker, ownership checker, emitter): a unit module (`fib.test.unit`) calling the pass on values, or a
   golden input in that pass's suite. Never a program that is compiled and run to see whether the expander did its job.
2. A rule that needs an earlier pass's output: take it from the previous layer's fixture, not from text through the earlier passes.
3. A rule about what a whole program does (verdict, memory audit, allocation bound): a case in `cases/`.
4. A rule about the toolchain as a user runs it (flags, exit codes, files written, linking, targets): an end-to-end sh test, and keep it small.
5. A test of the test machinery (batch against per-process, the pending detector) plants the fault and requires the failure.

Commands: `compiler/tests/golden/golden.sh --fibc F --only expand-porter expand-e1 expand-r` (batch; `GOLDEN_BATCH=0` per process),
`UNITS_DIR=pending FIBC=F compiler/tests/units.sh OUTDIR`, `F build FILE --time --cache-stats` for the phase times.
