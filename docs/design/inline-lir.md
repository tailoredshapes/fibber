# Inline lIR: `(unsafe (lir ...))`, ordered atomics and the cache line

Status: **built** (2026-10-20 and 21: steps 2 to 4 of section 10, and step 5 for fib-disruptor); section 11 says where the built form differs from the proposal; the owner has decided the two open questions of section 9 (a live swap re-emits a changed site; the `inline` modifier is accepted). Written from `fib-disruptor` (a ring buffer between threads, `/tank/repos/tailoredshapes/fib-disruptor`), whose
hot path had to be lIR in a separate shared library. The owner's rule for this design: *the best practice is the easiest thing; a foot gun is always allowed, as long
as the hoops make it obvious that is what you are doing.*

## 1. What the disruptor showed

fib-disruptor needed acquire/release atomics on raw memory, a cache-line layout per target and a spin-wait hint. fibber has none of them: its atoms are `seq_cst`
(a store is an `xchg` on x86-64) and sit where the allocator puts them, the `gpu/atomic-*` builtins are `monotonic` only, and a name starting `llvm.` is reserved in lIR.
So the ring was written in lIR (`native/ring.lir`), built by `lair` and `clang` into `libfibring.so`, and called through `extern`. What that cost, measured:

| cost | evidence |
|---|---|
| no inlining across the call | a hot loop with the slot arithmetic written inline: 660 M events/s; the same through `put!`/`word` wrappers that call out: about 450 M |
| a build step in every consumer | `scripts/build.sh` needs a `lair` binary and clang; a plain `fibc run` of the project fails without `LD_PRELOAD=libfibring.so` |
| the ring is an `i64` | a `ptr` may not cross a thread (types 5.3) and there is no int-to-ptr, so every entry converts inside the lIR |
| instructions the language cannot name | `pause`/`yield` (per ISA), the cache-line size (per target) |
| the builtin list grows by one per need | `gpu/atomic-add-i32`, `-min-`, `-max-`, `-umin-` .. each one a case in `types/builtins.fib`, `own/kernel.fib`, `emit/lower/gpu.fib` and the spec |

An `extern` to a library of one's own is also not what `docs/adr/0011` allows outside a driver, so the disruptor is a "driver" only by that adr's letter.

## 2. The rule, applied

* **The easy path is a library.** `fib.atomic` (section 6) gives `load-acquire`, `store-release`, `cas`, `fetch-add`, `fence`, `spin-hint` and `cache-line` as functions over
  `ptr`, written with the form below. Nobody writing a queue should need `lir`.
* **The raw form is allowed and loud.** It needs `unsafe` (a lexical hoop that already means "the checker has stopped helping"), the word `lir`, and every operand and
  the result typed at the site. `fibc explain unsafe` lists every site of `unsafe`, `extern` and `lir` in a program (section 7), so a reviewer can find them all.
* **What stays checked even there:** the lIR checker (`lir.check`, already a whole-module checker with the error texts of spec/lir.md) runs on the body at compile time and
  its diagnostics point into the `.fib` file. Types, dominance, orderings (`atomic-load cannot have ordering release`), calls. What it cannot check (that the ordering is
  *enough*, that a pointer is valid and aligned) is the foot gun, and is the author's.

## 3. The form

```
(lir [operand ..] -> type "body")        ; only lexically inside (unsafe ..)
```

* `operand` is a fibber name bound to a scalar (`bool`, `i8`..`i64`, `f32`, `f64`, `char`) or `ptr`: the same positions an `extern` allows (spec/syntax 3.15, types 6.13).
  An object is never an operand, so no count crosses and ownership has nothing to track: the form is `Scalar` for ownership, like `load-i64`.
* `type` is a scalar, `ptr` or `unit`.
* `body` is the blocks of an lIR function, as lIR text, in a string. Inside it each operand is a parameter of its own name and type, in the order written. A string and not
  data because lIR's upper-case words and `(GLOBAL name)` are not fibber forms (the old spelling's `@name` was even a fibber cell read, `@x`), and a body of several blocks and `phi`s is ordinary lIR that people already write (`rt/*.lir`). Strings may
  span lines, and the diagnostics carry the line and column inside the string to the `.fib` position.

```
(defun load-acquire-i64 (p: ptr) -> i64
  (unsafe (lir [p] -> i64 "(BLOCK entry (RET (ATOMIC-LOAD ACQUIRE I64 p)))")))

(defun wait-for (p: ptr want: i64 spins: i64) -> i64            ; a loop is blocks and a phi, as in any lIR
  (unsafe (lir [p want spins] -> i64
    "(BLOCK entry (BR look))
     (BLOCK look
       (LET ((s (PHI I64 (entry spins) (again s2)))
             (v (ATOMIC-LOAD ACQUIRE I64 p)))
         (BR (ICMP SGE v want) got again)))
     (BLOCK again (LET ((s2 (SUB s (I64 1)))) (SPIN-HINT) (BR (ICMP SLE s2 (I64 0)) got look)))
     (BLOCK got (RET v))")))
```

### 3.1 How it is lowered

Each site becomes an **internal lIR function** in the program's module, `fib.lir.<site>`, with the operands as parameters, and a `call` at the site. The statics table
already adds items to the module (`emit.statics/statics-item`, as `gpu/shared` does) and `lir.parse` already parses an item from text, so this reuses the parser and the
checker whole: nothing is spliced into a basic block, so a body of any number of blocks is as easy as one instruction. To make the call free, lIR gets one function
modifier, `inline` (`alwaysinline`), spelled with the existing modifiers (spec/lir 4.3) and used for these functions; LLVM then sees the body in the caller, which is what the
disruptor lost. A site in a generic function is keyed by its monomorphic types, so one site is one function per instantiation.

### 3.2 What the compiler does around it

* Ownership: an opaque scalar operation with memory effects. It is never a *peek*, never elided and never reordered across the compiler's own stores (types 6.3); a loop that
  contains one does not hoist it. The operands are read at the call like any scalar argument.
* Types: operands must be bound; a result must be the declared type; `lir cannot take an object: p has type (Vec i64)`.
* Live redefinition (docs/design/live-repl.md): a site is a function the swap **re-emits** like any other (**Decided**, owner, 2026-10-20: re-emitting is the expected behaviour, not a
  refusal). The site function is named by the site, so a swap that changes a body replaces that function and the callers of the changed function; one that does not change it leaves it.
* The macro runner and the interpreter path (`fibc run`) use the same emitter, so there is one behaviour, with no separate native library to find at run time.

## 4. New lIR instructions the library needs

Two, both per target, both in lIR (so the form needs no special case):

| form | meaning |
|---|---|
| `(SPIN-HINT)` | `void`: x86-64 `pause`, AArch64 `yield` (`hint #1`), RISC-V `pause` (Zihintpause), nothing where the target has none. Emitted as the target's intrinsic, which the reserved `llvm.` names keep out of user lIR |
| `(CACHE-LINE)` | `i64` constant: the **destructive interference size** of the target, the distance two threads' writes must be apart not to share a line. It is not necessarily the cache line the OS reports (the folklore is that Intel's adjacent-line prefetcher doubles it; the first measurement, docs/shootout/cache-line, found **64 enough** between this i7-14700KF's cores for a store-and-load test, and did not test the read-polling case that folklore is about) |

`(cache-line)` is also a fibber builtin of the same name beside `(has-fma)` (types 1.9, `compiler/lir/target.fib`'s `Target` record gets a field), so a macro or a
`defstruct` padding can use it. The value per target is **measured, not copied**: a false-sharing sweep (two threads, each incrementing its own word, at strides 8 to 256) on
one machine of each target family in `docs/shootout/`, and the table in `compiler/lir/targets.fib` cites the run. Until a family is measured it takes the conservative 128
(x86-64 and AArch64) or 64 (the rest). A code path that must also run where the build target differs (a library compiled for one CPU and run on another) stores the value it was
built with in its own header, as fib-disruptor will.

## 5. Why not the alternatives

* **More builtins** (`atomic-load-acquire-i64` .. ): what `gpu/*` did. Every ordering, width and operation is a new row in four files, `llvm.` intrinsics and the cache line still
  have no spelling, and a queue wants a fused sequence (claim, write, publish) that no single builtin is.
* **Linking lIR objects** (`lair build --emit liro`, `:lib`): what fib-disruptor does today. It works, and the costs are in section 1. It stays available for libraries that
  are genuinely separate.
* **Inline assembly:** per ISA by construction, not checked, not portable to the GPU or wasm targets that lIR already reaches. lIR is the portable layer and has the checker.

## 6. `fib.atomic`

A library module, not a driver (it declares no native library), over `ptr` and written with `lir`; each function is a few lines. Orderings are named in the function, not passed:

`load-i32 load-i64 load-ptr` with `-acquire` `-relaxed` `-seq-cst`; `store-*` with `-release` `-relaxed` `-seq-cst`; `cas-i64` (success and failure orderings spelled out);
`fetch-add-i64`; `fence-acquire` `fence-release` `fence-seq-cst`; `spin-hint`; `cache-line`. `fib.view` and the scheduler's own atoms are not rewritten onto it (they have their
measured orderings, spec/types 2.10). fib-disruptor's `ring.lir` moves into fibber as `src/disruptor/ring.fib` over this library, whose hot path inlines.

## 7. Auditing

`fibc explain unsafe FILE` (a new report beside `explain ownership`): every `unsafe`, `extern` and `lir` site of the program and its libraries, with the file, line, function, and for `lir`
the operands, the result and the instructions used (so "which atomics, with what ordering, does this program have" is a command and not a grep). A test in `compiler/tests/` pins the
report; and the arch test of adr 0011 gets the rule that a `lir` site in `lib/` is in a module that says so in its header comment. User code has no such rule: the hoop is `unsafe` and
the report.

## 8. Cases that must hold (`cases/`)

| case | verdict |
|---|---|
| `(unsafe (lir [p] -> i64 "(BLOCK entry (RET (ATOMIC-LOAD ACQUIRE I64 p)))"))` | accepted; x86-64 emits a plain load (`compiler/tests/emit`, objdump check); AArch64 `ldar` |
| `lir` outside `unsafe` | `lir may only be used inside unsafe` |
| an object operand `(lir [v] ..)` with `v : (Vec i64)` | `lir cannot take an object: v has type (Vec i64)` |
| a result type that is not a scalar, `ptr` or `unit` | rejected, names the type |
| a body that fails the lIR checker (`atomic-load release`) | the checker's text, at the `.fib` line and column of the error inside the string |
| a body of several blocks and a phi | accepted, runs (the `wait-for` above) |
| the same site in a generic function at two types | two functions, both run |
| a loop around a `lir` load does not hoist it | the loop sees a value another thread stores (two-thread case) |
| `(SPIN-HINT)` on each target | emits `pause` / `yield`; `lair` on a target without one emits nothing and checks |
| `(CACHE-LINE)` | a constant equal to the target's table row; differs across `--target` |
| `fibc explain unsafe` | lists every site of a program that has an `extern`, an `unsafe` and a `lir` |
| a live swap of a `lir` body | the running program takes the new body (`fibc run --live`, then `eval`/`load`) |
| the acceptance case | `fib-disruptor` with no `.so`, no `build.sh` and no `LD_PRELOAD`: its 23 specs pass under a plain `fibc test`, and `bench/bench.fib` at batch 1024 recovers the inline-arithmetic rate (660 M events/s here, within noise) |

## 9. Open questions and risks

* **The self-hosting trap** (CLAUDE.md): `lir` becomes a core form name. `grep` of `compiler/` finds no function named `lir` (modules named `lir.*` are namespaces, not functions); the full
  gate, not the quick one, is the check.
* **Live swaps** of a site whose body changed: **decided**, re-emit (section 3.2). A case in `cases/` swaps a `lir` body in a running program and sees the new behaviour.
* **Memory-order testing.** x86-64 is TSO and cannot fail a missing `release`; the cases above that matter for ordering run on the Mac Studio (aarch64) as well (`scripts/mac-check.sh`), and the
  design claims nothing for AArch64 until they have.
* **`inline` as an lIR modifier**: **decided**, accepted (owner, 2026-10-20). It is a change to spec/lir.md and to the Rust-compatible case harness's skip list (`;; stage: 2`).
* **A string body** loses editor highlighting; a later reader mode that reads lIR from fibber forms (with `@` handled) can sit on top without changing the lowering.

## 10. Order of work

1. This document accepted (or changed) by the owner.
2. lIR: `inline` modifier, `(SPIN-HINT)`, `(CACHE-LINE)` and the `Target` field with the measured table: spec/lir.md, the checker, lair, cases in `cases/lir`.
3. The `lir` form: syntax.md 3.15, types.md, the parser entry (`lir.parse` from a string with a position offset), `own` (a scalar with effects), `emit` (the site function and the call), cases.
4. `fib.atomic` and its cases; `fibc explain unsafe`.
5. fib-disruptor moves onto it; the acceptance case.

## 11. As built (2026-10-21)

The form, the library and the acceptance case are built (fibber commits after `62437696`; spec/syntax.md 3.15a; cases/stdlib 8800 to 8806 and 8810; lib/fib/atomic.fib). Differences from the proposal above:

* **The operands are typed at the site**: `(lir [p: ptr v: i64] -> unit "body")`, not `[p v]`. The type checker lowers a form to an extern it makes on the spot (an `ExternDef` with a `lir` body and
  parameter names) and a call of it, so scalars-and-`ptr`-only, `unsafe`-only and opaque-to-ownership are the extern's own rules, and nothing in the ownership checker changed. The types had to be
  written because the lowering happens before inference.
* **The body is checked at type-check time**, not when the code is emitted: a refusal is a type error at the form, which is also what the case harness needs to count it as a rejection. `types.lirsite` makes
  the function's text and checks it; the emitter makes it again to put it in the module.
* **A body may not call a function** (there is no way to declare one in it yet): sleeping a thread, `sched_yield` and so on stay in the platform layer.
* **The partitioner** (compiler/native/liro/partition.fib) copies every `inline` function into every unit that calls it, always (before, copying small functions across units was an opt-in knob,
  `FIB_LIRO_IMPORT`); without it `alwaysinline` cannot inline a call into another unit, which is where the default build splits the program.
* **`(CACHE-LINE)` is not a fibber builtin**: `fib.atomic/cache-line` wraps the lIR form and LLVM folds the constant. The table is per architecture (lir.targets `cache-line-of-arch`), not per CPU.
* **Measured** (fib-disruptor, one producer and one consumer, an i7-14700KF, two runs each): the ring written in fibber over `fib.atomic` runs 550 to 560 M events a second at batch 64 and 1024 and 40 M
  at batch 1; the extern version ranged 25 to 32 M at batch 1 and 270 to 660 M above. Its 24 specs and 13 mutants run under a plain `fibc test`: no shared library, no build script, no `LD_PRELOAD`.
* **`fibc explain unsafe FILE`** (compiler/types/unsafe.fib) lists every `unsafe` block, `extern` call and `lir` form of the program and its libraries, sorted by place, with the atomics and orderings each `lir` form uses; checked in compiler/tests/driver/cli.sh.
* **The arch rule** is ADR 0026 (docs/adr/0026-...): in the library, `lir` is written only in `lib/fib/atomic.fib` (an exact list, with planted violations); code outside the library is covered by `explain unsafe`.
* **A body may not call a function, on purpose** (decided, 2026-10-21): native access belongs to the platform layer and the drivers (ADR 0011); a body that called `sched_yield` would be an `extern` where the
  rule cannot see it. The platform layer provides the call: `fib.os.time/thread-yield` (`sched_yield`), which fib-disruptor's waits use (one core: 0.34 M to 11.6 M events a second).
* **Weak memory, checked on AArch64** (an Apple M1 Ultra, 2026-10-21; details in fib-disruptor's README): the compiled ring has `stlr`/`ldar`; its 26 specs pass there; weakening `publish` to a relaxed store is
  caught 5 runs in 5 on the M1 and 0 in 3 on x86-64; weakening the consumer's `advance` or the producer's read of the consumers' cursors was not caught by anything in 5 runs: those two orderings are shown by the
  memory model and not by a test.
* **fibber's own cases on AArch64** (the same M1 Ultra, the compiler and `lairf` cross-built for it): `cases/stdlib` 8800 to 8806 and 8810 (8 of 8) and `cases/lir/inline` (8 of 8, `(SPIN-HINT)` becoming `yield` and `(CACHE-LINE)` 128) pass.
* **Not built**: a way for a body to declare what it calls (not wanted, above).
