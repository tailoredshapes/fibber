> SUPERSEDED on 2026-10-04 by docs/design/dev-loop.md (the interpreter is a development tool, not a rule 6 oracle).

# A fibber interpreter (`fibi`): design

Status: design only (INT0, 2026-10-04). No compiler code is written or committed by this package. Everything under
"measured" below was run in this session with a throwaway spike under `~/.cache/fibber-scratch/INT0/` (`spike.fib`, `spike2.fib`,
run with a stage 2 `F` from `gate-fibber`, `FIB_LIB` = this tree's `lib/`); every claim about existing code names its file.
Nothing here is **Decided**: section 9 lists the decisions the owner makes, each with a recommendation.

## 0. Why, and the short answer

Method rule 6 (spec/method.md) says every case and generated program runs in the reference interpreter and compiled, and that
results, memory audits and traces agree. The reference interpreter is the Rust `fibref` (`crates/fibref/src/eval`, `heap`), frozen at
tag `seed-1` (ROADMAP "Method since 2026-10-02"). It cannot run today's library (`array-push!` is unbound) or anything added since,
so rule 6 lapses: the stage-2-only cases carry `;; stage: 2` (compiler/mirror-pending/X9c-stage2-only.md), the Option-of-scalar
cases pass in stage 2 only (docs/design/unboxed-option.md "What the Rust tools do", spec/compiler.md §8 item 13), and
`compiler/driver/verdict.fib` judges a case from the compiled run alone (its header: "stage 2 has no interpreter, so there is no
second side to disagree with").

Recommendation in one paragraph. Write `fibi`, a reference interpreter in fibber that **evaluates the checked, ownership-annotated
program** (the typed program of `types.program` and the plan of `own.program`, the same data the emitter consumes) directly, with its
own boxed-object heap, its own audit and its own free trace in the compiler's A/F/S/D format. Reuse the whole front end (reader,
expander, checker, ownership checker) and the macro runner unchanged; reuse `driver.header`, `driver.verdict`, `driver.child` and
`driver.harness` for the case harness. Independence is in the back half: nothing under the new `compiler/interp/` may require `emit.*`,
`native.*`, `llvm.*` or `lir.*`, and a script checks that. It is a pre-decoded tree-walker (one prepare pass resolves slots and
attaches each node's plan operations), planned at about 2 to 8 million node evaluations a second (section 5), about 7,000 lines of
fibber in nine packages (section 8).

## 1. What it interprets, and why that gives independence

### 1.1 What rule 6 compares

`fibc cases` (spec/compiler.md §5) runs one checked program two ways. The checked program is the output of the front end: reader,
expander, type checker, ownership checker. Both executions consume that one object, `Checked` (`own.driver`; used at
`compiler/driver/front.fib` `front-check`, fields `typed` and `owned`). What can differ between the two executions is everything after
it: how counts are changed, when objects are freed, how values are represented, what the builtins do, how calls dispatch, how threads
run. So the independence rule 6 wants lives in the **back half**: the second implementation must share no code with the emitter, the
runtime `fib.rt` (`crates/fibc/rt/*.lir`, `compiler/emit/runtime.fib`), `lair` or LLVM. It need not, and cannot, be independent of the
front end: a front-end bug is the same in both executions and the audit cannot see it, except where it shows as a memory error (below).

### 1.2 The options

**(a) The typed, ownership-annotated program, evaluated directly.** The interpreter reads `TypedProgram` (`compiler/types/program.fib`:
`expr-types`, `binding-types`, `resolutions`, `instantiations`, the `Globals` tables) and `OwnedProgram` (`compiler/own/program.fib`:
per body a `BodyOwn` with, for every expression, its mode and the operations to run `after` it; for every call its `CallOwn`: callee,
tail or not, how the head and each argument are passed, the write-backs, the jump operations; `allocs`, `stack-temps`, `peeks`,
`reuse`, `guard-fail`). It performs exactly the retains, releases and stack-ends the plan lists, plus those that are part of each
primitive's definition (the Rust evaluator's header, `crates/fibref/src/eval/mod.rs`, states this protocol; `own/explain.fib` prints
the same plan as text). Its heap is its own: boxed objects with explicit counts, an audit, a trace.

**(b) lIR, interpreted.** An lIR interpreter (the language of spec/lir.md) is independent of `lair` and LLVM (the back end of the
back end) but not of the emitter: the lIR it runs is the emitter's output (`compiler/emit/`, 8,732 lines in 42 files), so an emitter
bug (a missing release, a wrong layout) is executed faithfully and no second opinion is ever formed. It would be a good test of
`lair` (and `cases/lir` already has that purpose through `lair cases` and `lairf cases`), but that is lair's rule, not rule 6's.

**(c) Both, eventually.** Possible later; each answers a different question (a: "do the ownership checker, the emitter and the runtime
agree on what the program does?"; b: "does lair compile lIR as lIR means?"). Not needed for rule 6.

### 1.3 Recommendation: (a)

Rule 6 compares two executions of the same checked program, and the audit of rule 2 is by its definition an audit of the *plan*: the
interpreter evaluates the plan's counts against a heap that checks them (use after free, double free, a count below zero, a leak at
exit). Only (a) makes the interpreter an independent check of both consumers of the plan:

- the **ownership checker**, because the audit fires when the plan under-retains or over-releases (rule 2; this is the memory-safety
  claim of the whole language);
- the **emitter and runtime**, because the traces must match line for line (spec/compiler.md §4) and the results must be equal.

(b) cannot check the ownership checker semantically (it would only see what the emitter lowered of it) and cannot check the emitter
at all. (a) also keeps the interpreter small: no lIR parser, no instruction semantics, no layouts. What (a) shares with the compiled
path, and so cannot check: the reader, expander, type checker, ownership checker's *decisions* (it checks that they are memory-safe,
not that they are the intended ones), the macro runner (v1), and the case headers. Section 7 says which of these have other oracles.

Two non-obvious requirements follow from "evaluate the plan":

1. It must evaluate the **plan**, not just the typed tree. The Rust evaluator does so (`Interp::flow` runs `after` for every
   expression; `call` reads `CallOwn`; `eval/interp.rs`, `eval/call.rs`, `eval/expr.rs`). A tree-walker that did plain counting
   (retain on every binding) would be independent of the plan and so could not show that the compiled run frees at the same points,
   which is exactly the claim of types §6.12 (the elisions are invisible to the multiset of frees). It would also differ in the trace
   wherever the plan makes a *decision* (stack allocation, moves, steal, the all-owned bodies of §8.4). The plan is the semantics of
   counting; the interpreter executes it.
2. It dispatches **dynamically**: every object carries its type id, protocol calls (`Callee::Method`) pick the instance from the
   receiver (types §4.5: "the interpreter ... needs no monomorphisation"). The emitter monomorphises by layout class
   (`emit/mono.fib`, spec/compiler.md §7). That is a real independence gain: the layout-class keys and representatives
   (a bug source, see docs/design/unboxed-option.md "Generic code") are exercised by one side only.

### 1.4 How independence is enforced, not claimed

- A module rule: no module under `compiler/interp/` has `emit.`, `native.`, `llvm.`, `lir.` or `lair.` in a `:require` or `:use`.
  `scripts/check-interp-independence.sh` greps for it and is a gate step (a test that can fail: planting one `:require emit.ir` makes it
  fail; the package's acceptance test does that).
- The interpreter's builtins are written from the spec (syntax §4.3, types §2.9 to §2.13, §8.12), not copied from `rt/*.lir`. Only
  the **host boundary** is forwarded to the host's own builtins (section 3.3): there is nothing there to evaluate independently, only
  the C library.
- Builds by two builders. `fibi` is written in the subset the seed (`scripts/fetch-seed.sh`, v0.1.5, a Rust-built `fibc`) compiles,
  as `compiler/fibc.fib` is; a `fibi` built by the seed and one built by stage 2 must give the same output on the corpus (a miscompile
  of the interpreter by stage 2 would otherwise make "interpreter and compiler agree" mean "the compiler agrees with itself").
  The seed's front end is frozen (it cannot check the current `lib/`), but it builds `fibi`'s source the way it builds stage 2, whose
  source already uses `try-let`, destructuring and `defrecord`; `scripts/gate.sh --full` does that for stage 2 now. The seed builds
  `fibi`, not the other way round: `fibi` then runs the seed-incompatible programs.

## 2. Value representation, heap, audit, trace

### 2.1 What the Rust interpreter is made of (sized)

| Part | Files (crates/fibref/src) | Lines |
|---|---|---|
| value model, objects | `eval/value.rs` 149, `object.rs` 145, `option.rs` 79, `vec_view.rs` 82, `vecs.rs` 146, `forms.rs` 220 | 821 |
| thread state, frames, plan ops, world | `eval/interp.rs` 233, `plan.rs` 124, `world.rs` 189, `ast.rs` 59, `fx.rs` 67, `error.rs` 117, `mod.rs` 75 | 864 |
| the forms | `expr.rs` 263, `pattern.rs` 147, `call.rs` 293, `closure.rs` 219, `alloc.rs` 178 | 1,100 |
| builtins | `native.rs` 144, `arith.rs` 450, `float_bits.rs` 150, `floattext.rs` 170, `strtod.rs` 298, `strings.rs` 170, `arrays.rs` 159, `cells.rs` 194, `io.rs` 45, `sys.rs` 256, `raw.rs` 264, `unsafe_ops.rs` 77 | 2,377 |
| threads, tasks, scheduler | `sched.rs` 369, `threads.rs` 219, `task.rs` 205 | 793 |
| macros at expansion time | `macros.rs` 238 | 238 |
| whole run | `pipeline.rs` 322 | 322 |
| **eval total** | `cat eval/*.rs` | **6,515** (tests in `eval/tests/`: 1,313 more) |
| audited heap | `heap/{mod,access,audit,cascade,error,event,graph,immortalise,object,scc,scope,shared,store,unique,value}.rs` | **2,122** |
| case harness and verdict (already ported in part) | `cases/*.rs` and `cases/*/` | 2,986 |

The memory audit is the heap (2,122 lines): `Heap::alloc/retain/release`, the planned release cascade (`cascade.rs`, whose order the
compiled runtime's iterative drop matches on purpose, spec/compiler.md §3 `fib.drain`, case 228), stack scopes (`scope.rs`),
write-unique (`unique.rs`), share marking (`shared.rs`), immortalisation (`immortalise.rs`), and `finish` (`audit.rs`, `graph.rs`,
`scc.rs`), which classifies every object live at exit as `LeakCycle` (kept alive only by a cycle through a cell or atom, with
`count == live Ref fields`), `Leak`, or `ImmutableCycle` (a bug), and reports dangling refs and unended scopes. The audit errors are
the 17 variants of `heap/error.rs` (`UseAfterFree`, `ReleaseOfFreed`, `WriteToImmutable`, `NotUnique`, `StackUseAfterScope`,
`StackRefInHeap`, `WeakToStack`, `SharedStack`, `ImmortalHoldsMortal`, ...).

### 2.2 The fibi value and heap

**Values.** `Val`, one fibber enum: the scalars (`VUnit VBool VInt(n, width) VFloat(x, width) VChar VKw VPtr VTag`), `VObj(id)` (a
counted, stack or immortal object of the audited heap, `ObjId` an `i64`), and `VNil`/`VSome(v)` for an `Option`. As in the Rust
`eval/value.rs`: scalars unboxed in the *model*, every object an id into the heap. In fibber a payload-carrying variant is itself a heap
object of the host (docs/design/lair-interfaces.md §1 item 7: "a payload-free variant allocates", and a variant with a payload does
too); measured below that this costs about 2.5 times a bare i64 on a toy and is not where the time goes.

**Option.** The representation follows the spec as it now stands (spec/types.md §8.1 table; docs/design/unboxed-option.md): an
`(Option T)` with `T` an object that is no `Option` is `VNil`/`VSome(obj)` and allocates nothing (as the Rust interpreter already
does, `eval/value.rs`, `eval/option.rs`); an `(Option T)` with `T` a scalar **has a value and allocates nothing** (this is the
change from the Rust interpreter, which boxes it: it makes the compiled and interpreted `A` counts equal again and retires the
stage-2-only `allocs:` cases); `(Option unit)`, `(Option (dyn P))`, `(Weak (dyn P))` inside an Option and an Option of an Option are
heap enums (tag + payload), as in stage 2. The plan still lists retains and releases on a scalar Option (it plans it as `AlHeap`,
unboxed-option.md "How the emitter decides"); fibi treats a retain or release of a value that carries no object as a no-op, exactly
as the emitter does (`lcx-retain` acts on `ptr` and `dyn` only). This is the one place where the interpreter's behaviour is *derived
from the emitter's design document*; it is a representation decision recorded in the spec, not shared code.

**Objects.** An object table `(Array Slot)` indexed by `ObjId`; a slot has `kind` (Immutable, Cell, Atom, ...), `count`, `flags`
(IMMORTAL, STACK, SHARED, ROOMY: types §8.2, spec/compiler.md §3), `tid` (the type id, for dispatch), and its fields as `(Array Val)`.
The table is updated in place when uniquely held (types §6.6 applies to fibi's own data: the `Collections are persistent but updated in
place when uniquely held` rule of the library), threaded through the evaluator as one `&World` cell, no global state
(CLAUDE.md "No thread-local or global mutable state" for Rust; the same is the fibber convention, docs/design/lair-interfaces.md §1
item 6).

**The audit.** Port of `heap/` with the same errors, the same `finish` classification, and the same events, written against the spec
sections they implement (types §6, §6.6, §6.11, §8.2; ownership.md §6), with a unit test per audit check that shows it firing
(CLAUDE.md "Every audit check has a test that shows it firing"; the Rust ones are in `heap/tests`). New since `seed-1`, each needing a
heap rule and a firing test: `steal` (`PsSteal`: the field of a unique shell moves, the shell's slot is nulled and the shell's release
skips it; docs/design/in-place-update.md §1), `PsMoveIn`, `peeks` (a cell read with no count, types §6.3 "Cell peeks"), the ROOMY
flag and `array-push!`/`array-pop!`/`array-take!`, and `cell-update!`.

**The free trace.** `Event`s as in `heap/event.rs`; `fibi trace FILE` prints, to standard error or to a file, the canonical lines of
spec/compiler.md §4 (`A n k`, `F n`, `S n k`, `D n`, `T`) after the last `Immortalised` or `InitDone` event (`traced`, `heap/event.rs`:
`defs` and the values the compiled program holds as static data or makes before `main` are traced by neither side), with ordinals
renumbered in order of first allocation, immortal objects skipped, class `o`, `c` or `a`. The compiled side also prints `I k`
(a restart marker the harness uses, `compiler/driver/child.fib` `tally-of`); fibi prints none and the comparer starts both at the first
line after init.

### 2.3 Known divergences and how the comparison treats each

Sources: spec/compiler.md §4 (comparison), §8 items 1 to 13, spec/types.md §6.12, docs/design/unboxed-option.md,
docs/design/in-place-update.md, compiler/mirror-pending/X9c-stage2-only.md. "Rule" is how `compare-interp` (section 6) treats it.

| # | Divergence | Rule |
|---|---|---|
| 1 | `Option` of a scalar: stage 2 holds a value; the frozen Rust interpreter boxed (§8 item 13). | **Removed** by design: fibi holds it as a value (section 2.2). Lines equal. A case that said `;; stage: 2` only because of this loses the label. |
| 2 | Boxed `Option` whose `nil` is null in a generic body (§8 item 5): the Rust interpreter allocates the `nil` object only where the plan decides the site. | Fibi follows the spec table and the emitter's rule for `opt`/`box`; any remaining mismatch is a **failure of the compare**, not a tolerated divergence, and is the reason the comparison exists. |
| 3 | Stack objects: both sides emit `S n k` and `D n` (§4). The compiled side stores the ordinal in the count word under trace mode (§8 item 1). | Compared line for line. |
| 4 | Retains and releases are not traced: the compiled program elides pairs (types §6.12). | Not compared; only A, F, S, D (the multiset of frees at each scope exit, step end, callee exit and jump is the claim). The interpreter's own counts are audited. |
| 5 | Free order inside one cascade. | Compared line for line; fibi ports the planned cascade of `heap/cascade.rs`, which matches `fib.drain` (case 228). |
| 6 | Allocator, size classes, free lists, addresses, the 16 bytes in front of an object in trace mode. | Invisible: ordinals count allocations, not addresses. Not compared. |
| 7 | Threads (§4, §8 items 2, 3, 8, 9, 12): the interpreter's schedule is deterministic; the compiled run uses a pool and OS threads. A `T` line on either side makes the run threaded. | Threaded: compare by **tally** (objects live at exit per class equal; compiled allocated at least as many per class as fibi; the surplus is freed, which the live tally shows). A trap in a threaded run: compare only the message and a non-zero exit (§8 item 12). |
| 8 | `async` and `await`: the interpreter drives an awaited task to completion on the awaiter's stack (§8 item 3); the compiled run parks and wakes through a queue. | Results and frees agree by §8.8's choice; a pool start writes `T` on the compiled side, so any program with a task is compared by tally if either side shows `T`. |
| 9 | `swap!` retries under real threads (§4): the compiled program may allocate more. | Tally rule above; the `allocs: <= N` bound is checked against the **interpreter's** `A` count when threaded (it cannot be lower than the compiled one except through the retries). |
| 10 | A trap aborts: both traces stop at the abort; the live objects are not leaks (§4, method.md rule 3 `trap`). | Single-threaded: traces equal up to the abort, trap text contained in both, exit non-zero. |
| 11 | `audit: leak-cycle`: the compiled trace cannot tell a leaked cycle from another leak (compiler/driver/verdict.fib header). | The interpreter's classification is the authority (`LeakCycle` needs count = live refs); the compiled side only has to show an equal trace. This is better than today's verdict, which says "something leaked and nothing erred". |
| 12 | `def` initialisers: the compiler evaluates them at compile time (JIT, spec/compiler.md §8 item 4; `emit/defs/jit.fib`) and holds the result as immortal static data; fibi evaluates them at start in checking order and immortalises. | Both untraced; **values** compared through the program's output (a `def` whose value is wrong shows when `main` prints it). Closures other than named functions, `dyn`, cells and atoms in a `def` stay unsupported on both sides (§8 item 4). |
| 13 | Floats: text of `show` on a float (§8 item 11); `strtod`/`strtof` semantics. | Both implement the decided rule; fibi from the spec with its own digit search (not the runtime's `%.800e`/`strtod` loop), compared by cases 178, 187 and a differential test over a few thousand values. |
| 14 | `Form` values and `quote`: the compiled program holds quoted forms as static data (`lcx-quote`, `emit/lower/mod.fib` `EQuote`). | Compared through macro and quote cases' results; fibi builds a `Form` object (`eval/forms.rs` 220 lines in Rust) on `quote`. |
| 15 | The seed `fibref`'s own gaps (no scope-aware overloading, no value for a bare overloaded name: X9c) are front-end facts, shared. | Not a divergence between fibi and the compiler. |
| 16 | `extern`: fibi runs only a whitelist (section 3.4); the compiler links any C symbol. | A case outside the whitelist is `unsupported` on fibi (pending, listed, never a pass). The corpus has two such cases (`strtod`/`strtof`), both in the whitelist. |
| 17 | Depth: the Rust evaluator traps `too deep` at its stack budget (`eval/interp.rs`); native stack overflow crashes the compiled run. | A depth failure on fibi is reported as `unsupported: depth`, never as a pass; section 5.3. |

## 3. Builtins, host support, threads, macros, externs

### 3.1 The count

`compiler/types/builtins.fib` has **79** rows (`grep -c '(BuiltinSig "'`); the Rust table had 75 (`crates/fibref/src/types/builtins.rs`),
the four new ones being `cell-update!`, `array-take!`, `array-push!` and `array-pop!` (all of which the Rust interpreter cannot run).
Grouped:

| Group | Builtins | n |
|---|---|---|
| cells, atoms, weak, tasks | `cell set! atom swap! reset! weak spawn join cell-update!` | 9 |
| arrays | `array array-len array-get array-with array-copy array-set! array-take! array-push! array-pop!` | 9 |
| strings | `str-len str-bytes str-from-bytes str-concat str-slice str-byte-at str-find str-eq starts-with? concat` | 10 |
| conversions of bits and chars | `char->i32 i32->char f64->bits bits->f64 f32->bits bits->f32` | 6 |
| host I/O | `args read-file write-file` | 3 |
| fd and system | `sys-open sys-close sys-read sys-write sys-seek sys-pipe sys-dup sys-isatty sys-unlink sys-mkdir sys-rmdir sys-errno-text sys-getenv sys-clock-now sys-wall-now sys-sleep` and one more row, 16 in all (clock, wall clock, sleep, getenv among them) | 16 |
| reflection (macro time) | `gensym struct? struct-fields struct-params struct-field-types enum? enum-params enum-variants` | 8 |
| unsafe and raw memory | `ptr+ load-i8 load-i16 load-i32 load-i64 load-ptr store-i8 store-i16 store-i32 store-i64 store-ptr alloc free raw raw-retained release-raw` | 16 |
| control | `trap not` | 2 |

(9 + 9 + 10 + 6 + 3 + 16 + 8 + 16 + 2 = 79.) Beyond the table: the methods of the **built-in instances** of the built-in protocols
(`Num`, `Float`, `Eq`, `Ord`, `Bits`, `Hash`, `Show`, `Deref`, `types/builtins.fib` `builtin-protocols`) on scalars, `str`, keywords and
tags, and the nine conversions (`trunc zext sext fptrunc fpext fptosi fptoui sitofp uitofp`): in Rust `eval/arith.rs` (450 lines).
Arithmetic in fibber has no wrapping and no unsigned types (docs/design/lair-interfaces.md §1 item 8), and overflow **traps** at the
host, so fibi must test for overflow explicitly at each width and trap with the spec's message (types §8.12) rather than let its own
`+` trap: this is the largest piece of new care in the builtins package. FNV-1a (64 bit) for `hash` of text and keywords, an integer
hashing to itself (types §2.12; `arith.rs` `fnv`), so that `Map` and `Set` iteration order (hash order, stdlib §2.7) is identical.

### 3.2 Which need the host

Only the following touch the world outside the interpreter's own heap, and each is a C-library call that a fibber program makes with
`extern` (the mechanism of syntax §3.15; the compiler already uses it in `compiler/driver/native.fib` and `driver/proc.fib`):

- the 16 `sys-*` rows: `open read write close lseek pipe dup isatty unlink mkdir rmdir strerror getenv clock_gettime nanosleep`.
  `lib/fib/unix.fib` wraps them; the Rust `eval/sys.rs` (256 lines) is the only Rust module that calls libc.
- `args` (the interpreted program's arguments, which fibi gets after `--`), `read-file`, `write-file`.
- the library's three `extern`s: `write` (`lib/prelude.fib:687`, which `println` and `eprintln` use through `unsafe` raw buffers),
  `sqrt` (`lib/fib/math/libm.fib`), `strfromd` (`lib/fib/fmt/libc.fib`).
- `strtod`/`strtof` for the one case that declares them (`cases/` has two `extern` lines: `strtod`, `strtof`).

Because the interpreter is itself a fibber program, **forwarding is cheap and honest at this boundary**: `(sys-open ..)` interpreted is
`(sys-open ..)` called on the host, `write` an `(extern write ...)` in fibi's source, an interpreted `strtod` an `(extern strtod ...)`.
That is "shared by construction" and it is tolerable: these builtins evaluate nothing; cases 3600 to 3649 already compare the two
runtimes' `sys-*` behaviour. All `unsafe` memory (`ptr+ load-* store-* alloc free`) lives in fibi's own arena of `(Array i8)` (the
Rust `eval/raw.rs`, 264 lines), because a raw address from the host would be a pointer into the compiled interpreter's heap and could
not carry the audit (`raw`, `raw-retained`, `release-raw` need the arena to track the counts they hold). The `write` extern copies
the bytes out of the arena, as `raw.write` does.

### 3.3 Threads and atoms

`spawn`/`join`, `async`/`await`, `atom`, `swap!`, `weak`. The Rust interpreter runs every program thread on an OS thread but only one
at a time: a baton (`Turn`, `eval/sched.rs`) hands the whole `World` to the next thread at a fixed list of scheduling points with a
quantum of 1000, so a run is deterministic and fair, and a thread that spins on an atom lets the others run (`eval/sched.rs` header,
`threads.rs`). It needs a stack per program thread because a tree-walker keeps the program's state on the host stack.

Fibber cannot copy that directly. A fibber thread (`spawn`) captures only `Send` values; a `Cell` is not `Send`; the `World` is a cell of
mutable data; and there is no way back from a `raw` pointer to an object (syntax §3.15 gives `raw`, `raw-retained`, `release-raw` and
no inverse). So sharing one mutable `World` between host threads cannot be written safely in fibber without moving the entire heap
into raw memory, which would be a rewrite of the audited heap on bytes.

What the corpus needs: of about 1,350 cases, 33 contain `(spawn`, 18 `(async`, 56 `(atom`, 11 `(weak` (a `grep -rl` over `cases/`). The
spec lets the executor run a task when it is awaited (syntax §3.12; spec/compiler.md §8 item 3: "The interpreter keeps driving a task to
completion on the joiner's stack, which §8.8 allows"). Options:

- **A. Lazy run-to-completion on the host stack** (recommended for v1). `async` creates a task record; `await` and `join` run it, on the
  caller's stack, if it is not done. `spawn` is the same: the body runs when joined, or at the end of `main` (`§6.8`: main joins every
  thread), or, if the spawner reads an atom that a thread is meant to write, **never**: that program hangs, and the step budget (section
  5.3) reports it as `unsupported: needs preemption`, pending and listed. Deterministic, no host threads, `T` printed per spawn. Covers
  every case that does not spin on an atom for another thread's write. Each `swap!` is one attempt that cannot lose (as in the Rust
  schedule, "loses none that the program's own `f` does not force", spec/compiler.md §4).
- **B. An explicit-stack machine** (a CEK-style evaluator whose continuation is a heap-allocated list of frames). Each program thread
  is one such stack; the scheduler is a loop with the Rust quantum. Gives the Rust interpreter's exact schedule, spin-waits included, and
  removes the host-stack limit of section 5.3. Costs: the evaluator's shape is a machine, not recursion, about 30 percent more code
  and I estimate a factor of 1.5 to 2.5 in speed against recursion (unmeasured).
- **C. Host threads with one `World` in an atom.** Not buildable as above.

Recommendation: A, with the census of package P0 (section 8) counting the cases that A leaves pending. Take B only if more than about 5
of the 51 task cases need preemption; B is also the answer if the depth question of 5.3 turns out worse than expected, and the decision
is cheap to defer because the evaluator's external interface (section 8.2) is the same for both.

### 3.4 Macros

`defmacro` bodies run at expansion time. The macro runner (`compiler/macros/runner.fib`, 276 lines, `expand.maker`'s `RunnerMaker`) runs
them by checking the macro's module, emitting lIR (`emit.compile`'s `compile-macro`) and JIT-running it through `native.*`. The
expander takes a `RunnerMaker` as data, so `fibi`'s main can pass any maker.

- **v1: reuse `macros.runner` unchanged**, as the brief says. Consequence: a `fibi` built so links LLVM (`macros.runner` requires
  `native.api`, `emit.compile`), and the *expansion* of a program is shared by both sides, not independently checked. That is
  consistent with section 1.3 (the expansion is front end), and `crates/fibc/tests/macros.rs` showed the Rust side comparing the two
  runners; stage 2's compare scripts (`compiler/tests/macros/`) are the oracle for the JIT runner today.
- **v2 (package P9, optional): a `RunnerMaker` backed by the interpreter.** The macro module is checked with the prelude and *run by
  fibi*; `gensym` and the seven reflection builtins call the expander's context as the Rust `eval/macros.rs` (238 lines) did, and `Form`
  values are built and read natively (`eval/forms.rs`, `vecs.rs`, `vec_view.rs`: 448 lines in Rust). It buys three things: a second
  runner to compare expansions form for form with the JIT runner (the Rust did this, and the `fibref expand` 8 reflection-error inputs
  ROADMAP lists as open differ exactly here); a `fibi` with **no LLVM dependency** (it would build with the seed and run on a machine
  without LLVM 21); and an interpreter that runs the library macros (`doseq`, `for`, ...) without a JIT start per macro.

### 3.5 `extern` declarations

A user `extern` names a C symbol; fibber cannot call a symbol chosen at run time (the extern is a declaration in source). The compiler
has a way: `compiler/native/call.fib` calls an address through a shim compiled by lair (`bsearch` trick, `pthread_create`), which needs
`native.*`, so LLVM, and is the opposite of independence. Recommendation: **refuse as `fibref` did** (`eval/unsafe_ops.rs`
`call_extern`: only `strtod`, `strtof`, `write`; anything else is `unsupported: extern NAME is not available in the reference
interpreter`), with a whitelist that fibi's source declares: `write`, `sqrt`, `strtod`, `strtof`, `strfromd`, and further libm and libc
functions added as a case needs them, each an `(extern ...)` line in one file `interp/hostc.fib` and a row in the whitelist table. This
covers the library and the whole corpus. Where a case needs more, the harness reports it pending.

## 4. The interface

Name: **`fibi`**. At the retirement of the Rust (ROADMAP stage 10) the tool takes the name `fibref` (spec/method.md says "reference
interpreter" and the CLAUDE.md layout calls it `fibref`). Its source is `compiler/fibi.fib` (a program of the same style as
`compiler/fibc.fib`: `-I compiler -I lib`, `FIB_LIB` roots, `driver.args` conventions), built `fibc build compiler/fibi.fib -I compiler
-I lib -L DIR -l LLVM-21 -o fibi` (v1; no `-l LLVM-21` with P9).

| Command | Does |
|---|---|
| `fibi run [-I DIR].. FILE [-- ARG..]` | the front end, then evaluate `main`; print `result: N` and the audit summary as `fibref run` did, exit status as `fibc run` (3 rejected, 4 unsupported, 5 internal, trap 134 with `trap: MESSAGE` on stderr, otherwise `main`'s value) |
| `fibi trace [-I DIR].. FILE` | run, write the free trace of spec/compiler.md §4 to standard error and the result to standard output (a drop-in for `fibc run --trace`; the old `fibc itrace`, `crates/fibc/src/harness/interp.rs`, did the same) |
| `fibi audit [-I DIR].. FILE` | run and print the full audit report (leaks classified, dangling refs, open scopes, the live tally) |
| `fibi cases DIR [--only PREFIX..] [-j N]` | the case harness (section 4.1) |
| `fibi explain [-I DIR].. FILE` | the ownership plan as text: **the same function** `own.explain/explain` (`fibc explain` and `fibi explain` print identical text, a cheap sanity check of the shared plan) |
| `fibi compare DIR [--only PREFIX..] [-j N] --with FIBC` | section 6 |
| `fibi help`, `fibi version` | |

Options are the shared ones (`driver.args`: `-I DIR`, `FIB_LIB`; `--no-runner` as `fibc`).

### 4.1 `fibi cases DIR`

The same semantics as `fibc cases` and `lairf cases`: every `*.fib` of the directory, and the `main.fib` of every subdirectory that has
one, in name order (`driver/harness.fib` `list-cases`, `select-cases`, `render`, which are reused as they are); the header parsed by
`driver.header` (keys `spec expect result audit allocs roots error trap covers open stage`); the verdict judged by `driver.verdict`
`judge`. What changes is the *outcome*: fibi's own, run in-process or in a child. A child per case is what `fibc cases` does
(`driver/child.fib`: a trap must not take the harness down, a runaway case has a 300 s alarm); fibi does the same by re-running itself as
`fibi trace` with output in files, reusing `driver.proc`'s fork/exec and `-j N` scheduling.

`driver.verdict`'s `OutCompiled result audit allocs` gains a real audit (`leaks`, `errors`, and a **leak-cycle** count the compiled side
could not provide), so `audit: leak-cycle` is judged properly, and `allocs: <= N` is checked against fibi's own `A` count. A header
with `;; stage: 2` (known key, `driver.header` `known-keys`) is simply run: fibi *is* stage 2's reference, so the label, which exists
because the Rust interpreter could not run the case, becomes unnecessary for the cases that were labelled for that reason (the compare
script reports each such case; removing the labels is a separate, reviewed change). A `reject` case needs no run: the front end's
message is the same on both sides, as it already was (spec/compiler.md §5 item 4).

### 4.2 Built into the driver, or separate

Recommendation: **a separate program, `compiler/fibi.fib`**, not a subcommand of `fibc`:

- the independence rule (1.4) is a statement about a module tree, and a separate tool with its own `interp/` is the natural unit;
- `fibc` links LLVM; `fibi` (P9 done) does not, and the seed can build it;
- the gate runs both and compares, which is only meaningful when they are two executables;
- they share only the front end modules and the driver helpers, which are already modules (`driver.args`, `driver.header`,
  `driver.verdict`, `driver.harness`, `driver.proc`, `driver.child`, `driver.front`).

`fibc` stays as it is. Its `cases` keeps judging compiled runs; a flag `fibc cases --interp-with FIBI` is not added (the compare is
`fibi compare`, section 6).

## 5. Speed

### 5.1 What was measured

Spike (throwaway, scratch only): a toy AST `Ex` (constants, locals, `+ - <`, `if`, call) and the `fib(27)` program (635,621 calls, about
9 nodes per call, so about 5.7 million node evaluations), a tree-walker compiled by stage 2 (`F run`, which is `-O2`):

| Variant | Time | Nodes per second |
|---|---|---|
| (B) walker over `i64` words, a fresh frame `(Array i64)` per call | 33.7 ms | about 170 million |
| (A) walker over a boxed `Val` enum (`VInt`, `VBool`), a frame `(Array Val)` per call | 84.6 ms | about 67 million |
| (A') as (B) plus one persistent `Map i64 i64` lookup per node (the plan's `after` list by expression id) | 167 ms | about 34 million |

So the bare dispatch of a tree-walker in fibber, compiled by our compiler, is tens of millions of nodes a second; a Map lookup per node
roughly doubles it, and a boxed `Val` per result costs about 2.5 times. (The numbers above are for `main` run through `fibc run` on this
machine; `fib.unix/clock-now` brackets only the evaluation.)

### 5.2 The planning figure

A real node does much more than the spike: the locals lookup, the plan's `after` operations (retain, release, end-stack), the
heap's validity checks and event append on every count change, the trace, a `Val` allocation per result. I plan for **2 to 8 million
node evaluations a second** (a factor of 10 to 20 below the toy), and the number to confirm at package P2's first milestone is the
mean over the `cases/ownership` suite. A compiled program runs at roughly 0.3 to 1 billion simple operations a second, so the
interpreter is **about 100 to 300 times slower than native on compute-bound code**.

Two things that the spike says about the design: the interpreter must **not** use a `Map` for frames or per-node plan lookups at run
time (a `Map BindingId Val` per frame is what the Rust `Frame` did, `eval/interp.rs`, and it is the same shape as (A') only worse),
and it should not allocate a closure per node. Hence the recommendation in 5.4.

### 5.3 The corpus

The corpus is 1,347 case files in `cases/{ownership,stdlib,modules}` (306, 1,013, 28; `ls`). Compiled, `fibc cases` takes about 0.67 s
a case on average (ROADMAP "The performance cycle": 642 s for 963 cases), of which 0.4 s is the front end checking the library, which
fibi pays too and shares. For comparison, measured here with a stage 2 `F`: `cases/stdlib/6010-bigint-...` `emit` 0.24 s, `run` 0.39 s;
`6100-ref-regex-vs-java-util-regex` `emit` 0.80 s, `run` 2.27 s (the difference is lair JIT plus the run itself; the native run of such a
case is a fraction of it, so I cannot split it without a profile).

What I can say: the median case runs a few thousand to a few hundred thousand nodes and finishes in tens of milliseconds, where the front
end's 0.4 s dominates, so **most cases cost about what they cost compiled**. The tail is the `ref-` cases (differential tests against a
model, thousands of random inputs: 6100, 6010, the `tl.rng` suites) whose native run is tens to hundreds of milliseconds and whose
interpreted run at 100 to 300 times is seconds to a few minutes each. My estimate for a full corpus run is **10 to 40 CPU-minutes**,
**1 to 4 minutes wall at `-j 12`** (the gate's default, scripts/gate.sh; the machine has 28 cores), against the 70 s of the compiled
stdlib suite. That is a guess to replace by the census.

**P0 includes the census**: for every case, the compiled run's `A` line count and the wall time of `fibc run` minus `fibc emit`, a ranking
of the 30 heaviest, so that the corpus estimate is a measurement before P2 is written. If the total comes out above about 60 CPU-minutes,
the fast path below is the answer, and the heaviest cases are marked with a header key `time: long` (a harness hint, not a verdict) so
the quick gate can skip them.

**Stack depth.** The Rust evaluator ran on a 1 GiB thread stack (`eval/pipeline.rs` `STACK_BYTES`). A tree-walker in fibber uses the
host stack at perhaps 2 to 5 KB per interpreted call (an estimate: nested `flow` frames per node level), so the default 8 MB stack gives
a source depth of about 2,000 to 4,000, too small for the recursion tests (the non-tail recursion cases of `cases/ownership`). Plan:
`fibi` re-executes itself once under a raised stack limit (`setrlimit` through `extern`, then `execv`; the `driver.proc` module already
has `execv`), or runs the evaluator on a `pthread` whose stack it chooses (`native/call.fib` `call-on-stack` shows the call). The first
census run gives the supported depth; a shortfall is `unsupported: depth`, pending, and is one of the two reasons to pick design B of 3.3.

**A hang** is a failure of its own: the child has the 300 s alarm (`driver/child.fib`), and each interpreter thread has a **step budget**
(`QUANTUM`-like, counted at calls and back edges, as the Rust `tick` does) that turns an endless spin into `unsupported: needs
preemption` (A of 3.3).

### 5.4 The fast path: a prepared tree, not closures

Recommendation: write the evaluator as a **pre-decoded tree-walker**. One `prepare` pass per body, run once on first call, turns
(`Expr`, `BodyOwn`) into a `Node` enum tree in which: locals are frame-array slot numbers (no `BindingId` lookups), each node holds its
`after` operations inline as a small vector (no `Map ExprId` lookup), a call node holds its `CallOwn` and a resolved callee index, and
patterns are pre-compiled to test sequences. Evaluation is then one `match` per node over arrays: the (A) variant of the spike plus the
heap's work. Closure compilation (a fibber `fn` per node) gains nothing over an enum `match` here: our closures are heap objects with
captured environments and an indirect call, and the spike's enum walker already dispatches at tens of millions of nodes a second.
Expect the prepared tree to be 3 to 5 times faster than a walker over `Expr` with `Map` frames; the plan of the order (section 8) builds
the straightforward walker over the prepared tree directly, not the slow one first.

`prepare` is the one piece of translation between the plan and the evaluation; it is small (about 600 lines) and its only job is to
change representation, so it adds a bug surface, but one the audit and the compare find at once (a missing operation is a missing free).

## 6. The comparison harness

Today's gate (`scripts/gate.sh`, `scripts/ci-stage2.sh`) runs `F cases DIR` over the three directories (quick: ownership, modules and
about every 10th stdlib case plus the expected failures; full: all) and compares the non-passing set with
`scripts/ci-stage2.expected`. It never runs a second implementation.

### 6.1 `fibi compare` and `scripts/compare-interp.sh`

`fibi compare DIR --with FIBC` runs, for each case, in children and with `-j N`:

1. `FIBC run --trace -I ROOT.. FILE` (the compiled run: stdout result, stderr trace, exit status; the existing `driver.child` already
   starts and reads it), and
2. `fibi trace -I ROOT.. FILE`,

and judges the pair with `interp/compare.fib`, a pure function over two outcomes (so it has unit tests with planted differences):

| Check | Passes when |
|---|---|
| status | both accepted, both trapped (the header's text occurs in both messages), both rejected with the same message, or both `unsupported` |
| result | the result lines are equal |
| trace, single-threaded | the A, F, S, D line sequences are identical, line for line; the first differing line is named |
| trace, threaded (a `T` on either side) | the tally rule of 2.3 item 7; on a trap, message and status only |
| audit | fibi's audit is clean, or `leak-cycle` exactly when the header says so; a use-after-free, double free or negative count fails whatever the compiled run did |
| allocs | the header's `allocs: <= N` holds for the interpreter's count **and** the compiled count (rule 3: `fibc cases` counts the compiled trace; `fibref cases` the interpreter's; today a bound only the compiled count meets is a `;; stage: 2` case, 2.3 item 1) |

`scripts/compare-interp.sh FIBC FIBI DIR...` is the thin wrapper in the style of `compiler/tests/*/compare.sh`: it picks the right `-j`,
takes the suite lock (`take_suite_lock` in `scripts/lib/stage2.sh`) so it never overlaps a benchmark, runs `fibi compare` per directory,
prints the table of the non-agreeing cases and exits 0 when the set equals `scripts/ci-interp.expected` (the same bookkeeping as
`ci-stage2.expected`: an unexpected disagreement fails; an expected one that now agrees fails until its line is removed). The expected
file begins with the cases of 3.3 and 3.5 that are pending by design.

### 6.2 Mutants, so the compare can fail

A comparison that cannot fail is worse than none (CLAUDE.md). The repository already has mutation scripts for the compiled side
(`scripts/mutant-unique.sh`, `mutant-amp-param.sh`, `mutant-elem-get.sh`, `mutant-impl-escape.sh`, `mutant-peek.sh`,
`mutant-regex-literal.sh`). The package that adds the compare adds, for each, the requirement that **`fibi compare` fails** on a mutated
stage 2 (the compiled run frees at a different point or answers differently while the interpreter does not), and the mirror: a mutated
interpreter (one release dropped from `prepare`'s output) must make `fibi cases` fail with an audit error. Those are the acceptance tests of
P5 and P6.

### 6.3 The new gate stage

`scripts/gate.sh` gains one stage after the cases: **interp** (quick: `fibi cases` over ownership and modules in full and the same
stdlib sample as the compiled run; then `compare-interp.sh` over the same set; full: all three directories). Timing line `interp N s`,
failure reported as `FAIL interp`. The fixed point (`stage3_check`) is unchanged; the fibi build is added to `stage2_ensure`
(`scripts/lib/stage2.sh`) and cached by the stamp of `compiler/` and `lib/` like F. The independence check of 1.4 runs in the same stage.

## 7. What the interpreter does not check

Stated plainly so no claim outgrows the tests (spec/method.md): the reader, expander, type checker and the ownership checker's decisions
(the audit only shows a plan that is unsafe; a plan that is safe but leaks more than the intended one is caught only when a case's
`allocs:` bound says so); the macro runner (v1); `reject` verdicts; the library's own correctness (a library bug is a bug of the
program in both executions; the differential cases against Clojure are its oracle, MEMORY.md "Clojure oracle"); lair and LLVM below
the emitter beyond what an equal result and trace imply. Each has its own oracle (compare scripts against the Rust dumps in
`compiler/tests/`, the Clojure jars, `cases/lir`). What it does add: a second implementation of counting, of the heap, of every builtin,
of dispatch, tasks, closures, tail calls, stack allocation, `Option` and the free order, plus the first real classification of
`leak-cycle`.

## 8. The plan

### 8.1 Packages

The pattern is that of docs/design/lair-interfaces.md: one package fixes the interfaces as stubs that compile (bodies
`(trap "todo: MODULE FUNCTION")`), owners replace the bodies of their files only, a signature change is reported to the lead first.
Names are prefixed (`I...`: `IVal`, `IObj`; variants carry the enum's prefix) because type and variant names are global across linked
modules (lair-interfaces.md §1 item 2; the compiler already has `Module`, `Block`, `Env`, `Header`, `Jit`, `Options`). A payload-free
variant allocates (item 7): constants are shared. Positions are the front end's `Pos`. No global mutable state.

Sizes: the Rust figure of section 2.1, times the project's 0.7 port ratio (ROADMAP "Method since 2026-10-02"), adjusted for what is
dropped and what is new since `seed-1`. Files stay under 500 lines and functions under 50 (CLAUDE.md "The ports").

| Pkg | Content | Files (under `compiler/interp/` unless noted) | Rust basis | Estimate | Depends on |
|---|---|---|---|---|---|
| **P0** | contract and census: the interface stubs of 8.2; `scripts/check-interp-independence.sh`; the census of 5.3; `tests/` skeleton that builds a program requiring every module | `value.fib` (types only), `world.fib`, `error.fib`, `compiler/tests/interp/skeleton.sh`, `scripts/census-cases.sh` | | 400 | |
| **P1** | **heap and audit core**: objects, counts, flags, cascade order, stack scopes, write-unique, share marking, immortalise, `finish` with leak classes, events and the trace writer; the new heap rules (steal, move-in, peek, ROOMY, in-out arrays) | `heap/{store,object,alloc,release,cascade,scope,unique,shared,immortal,audit,graph,scc,event,error}.fib`, `trace.fib` | `heap/` 2,122 | 1,500 (+ tests 400) | P0 |
| **P2** | **evaluator**: `prepare` (slots, per-node plan operations, resolved callees, compiled patterns); expressions, patterns, calls, tail calls, `recur`, closures and function values, dispatch by type id, `&` private cells and write-backs, placement (heap, stack, nothing), the `Option` representation; `def` evaluation and immortalisation; the run entry | `prepare.fib`, `eval/{expr,pattern,call,closure,alloc,option,plan,run}.fib`, `defs.fib` | `eval/` forms and value model 2,785 | 2,650 (+ tests 400) | P0, P1 (interface) |
| **P3** | **builtins**: arithmetic and the nine conversions with explicit overflow at each width and the traps of §8.12; bits; strings and arrays from the spec; float bits, float `show` and parse; `hash`; cells and atoms operations; the reflection builtins' hooks; sys/args/file forwarders; `unsafe` arena; the extern whitelist | `builtins/{arith,float,fmt,strtod,str,array,cell,sys,raw,extern,hostc,reflect}.fib` | `eval/` builtins 2,377 | 1,600 (+ tests 250) | P0 |
| **P4** | **tasks and atoms**: 3.3 A (lazy run), `T` lines, `swap!`, weak, step budget; interface to swap in B | `task.fib`, `sched.fib` | 793 | 450 | P1, P2 |
| **P5** | **driver and harness**: `fibi.fib`, `run trace audit explain`, summary text, `cases` via `driver.harness`/`verdict` with the new outcome, the depth re-exec | `compiler/fibi.fib`, `driver/` additions `interp.fib`, `outcome.fib` | `main.rs` 420 + `cases/` shared | 700 | P0; P1 to P3 to be useful |
| **P6** | **compare**: `compare.fib` (the table of 6.1, unit tests with planted differences), `fibi compare`, `scripts/compare-interp.sh`, `scripts/ci-interp.expected`, the mutants of 6.2, the gate stage 6.3 | `driver/compare.fib`, scripts | none (new) | 450 (+ scripts 200) | P5, a stage 2 F |
| **P9** (optional) | macros on fibi: a `RunnerMaker` backed by the interpreter; `Form` natives | `macros.fib`, `forms.fib` | 238 + 448 | 500 | P2, P3 |
| | | | | **about 7,350 + 1,050 tests (v1: P0 to P6); +500 with P9** | |

The sum is higher than 0.7 times the Rust eval and heap (about 6,000) because of what is new since `seed-1` (the value `Option`,
steal and the in-place primitives, peeks, the all-owned bodies, `prepare`, explicit overflow checking and the independent float text),
and lower in what is avoided (`strtod`, 298 lines, becomes a handful of lines over libc; macros are deferred).

### 8.2 Interfaces fixed up front (P0)

The signatures, each a stub in P0, no package changes another's:

```
;; value.fib
(defenum IVal IUnit (IBool b: bool) (IInt n: i64 w: i64) (IFloat x: f64 w: i64) IChar IKw IPtr ITag
                   (IObj id: i64) INone (ISome v: IVal))
;; heap/ (P1): all functions take and return the heap as a value in a Cell passed as &h
(alloc  (&h kind: i64 tid: i64 fields: (Vec IVal)) -> (Result i64 AuditError))
(retain (&h id: i64) -> (Result unit AuditError))    (release (&h id: i64) -> (Result unit AuditError))
(alloc-in-scope ..) (end-scope ..) (write-unique ..) (mark-shared ..) (immortalise ..)
(read-field (&h id: i64 i: i64) -> (Result IVal AuditError)) (write-field ..)
(finish (h: Heap) -> AuditReport)  (trace-lines (h: Heap) -> (Vec str))   ;; spec/compiler.md §4
;; eval (P2)
(run-program (c: Checked args: (Vec str) budget: i64) -> RunResult)       ;; RunResult: result, trap or error, the heap
;; builtins (P3): one function per group, uniform shape
(call-builtin (&h id: i64 args: (Vec IVal) pos: Pos placement: Placement) -> (Result IVal RunError))
;; tasks (P4)
(spawn-task ..) (join-task ..) (await-task ..) (tick ..)
```

Fixed also: `RunError` kinds (`Trap`, `Gap` (the plan lacks something: report, do not invent), `Unsupported`, `Internal`, `Depth`), the audit
error variants (the 17 of `heap/error.rs`), the trace line format, the exit statuses of `fibc` (2, 3, 4, 5, 134), and the rule of the
Rust header: *a plan that lacks what the interpreter needs stops the run with a gap rather than the interpreter inventing a rule*.

### 8.3 Order and independence between packages

P0 first (a day of work; it also holds the census that sizes the speed risk). Then **P1, P3 and P5's driver half run in parallel**, the
three sharing only the types of P0: P1 (heap) is testable alone with a script that drives it directly (`compiler/tests/interp/heap-*.fib`:
unit programs that call alloc, release, cascade and show each audit error firing, as `heap/tests` did); P3 (builtins) is testable alone
over `IVal` values against the spec's tables of §2.12 and §8.12 and, for strings and floats, differentially against the host's
operations on generated inputs; the driver half is testable against a stub evaluator that returns a fixed result. P2 follows as soon as
P1's interface is real (P2 can start earlier against the stub), then P4, then the tail of P5 (`cases`), then P6. The whole chain runs on the
corpus incrementally: `fibi cases cases/ownership --only 0` gives pass and PENDING from the first day P2 runs `main = 0`, and a PENDING is
never a pass (CLAUDE.md "Pending is not pass").

Independence between packages: P1 and P3 do not require P2; P2 requires P1 and P3 through the interfaces only; nothing requires P6;
**no package requires `emit.*`, `native.*`, `llvm.*`, `lir.*`** (1.4). P5 and P6 require the driver modules and `macros.runner` (v1).

### 8.4 Reuse and rewrite

Reused unchanged: `syntax/` (1,829 lines), `expand/` (8,731), `types/` (10,558), `own/` (4,164, including `own.explain`), `macros/runner.fib`
(276, v1), `driver/{args,header,verdict,proc,child,harness,front,version}.fib` (the ones that do not touch lair), `expand.maker`, the case
corpus and its headers. Rewritten from the spec in fibber: everything under `compiler/interp/`. Ported from the Rust, with the 0.7
ratio and the spec as the authority (when the Rust and the spec disagree: stop and report, CLAUDE.md): `heap/`, `eval/`. New:
`prepare`, `compare`, the value `Option`, the in-place primitives' semantics, the whitelist, the census, the independence script.

### 8.5 Tests, per package

Each package's acceptance is an executable test that can fail: P1, a unit program per audit error (a firing test each), a planted
double free in a driven heap; P2, `fibi cases cases/ownership` all `pass` or listed PENDING with a reason, a planted dropped release
failing a case, golden outputs of `prepare` against `fibc explain` (the operations list per call equals the plan's); P3, a table-driven test
per builtin family against the spec, plus a 64,000-value float text test against the model of `crates/fibc/tests/floats.rs` (the same
model, which shares no code with either); P4, cases 11, 24, 32, 38, 43, 44, 148, 174 to 176 (the task cases of spec/compiler.md §8 item
3); P5, `fibi cases` over the three directories; P6, the mutants of 6.2, and the whole corpus compare with `ci-interp.expected`.

## 9. Risks, and decisions for the owner

| # | Decision or risk | Recommendation |
|---|---|---|
| D1 | What `fibi` interprets: (a) the checked program with its plan, (b) lIR, (c) both. | **(a)**: only it checks both the ownership checker (audit) and the emitter and runtime (trace), and it is the smallest. Revisit (b) only for a lair-specific need. |
| D2 | Option of a scalar: follow stage 2 (value), as the spec table now says (types §8.1). | **Follow stage 2**; it deletes divergence 1 and the reason for some `;; stage: 2` labels. |
| D3 | Macros: reuse the JIT `macros.runner` (v1) and add an interpreter-backed runner later (P9), or build P9 first. | **v1 reuse**, P9 after the compare is green. P9 is what lets `fibi` build without LLVM and what gives a second expansion oracle. |
| D4 | Threads: A (lazy run-to-completion, spin-waits pending), B (explicit-stack machine, exact schedule), host threads (not buildable). | **A**, with the census deciding; **B** if more than about 5 of the 51 task cases need preemption or if the depth shortfall below forces it. |
| D5 | `extern`: refuse with a whitelist (write, sqrt, strtod, strtof, strfromd), or call through the lair shim. | **Refuse with a whitelist**; the shim needs LLVM and removes the independence the tool is for. |
| D6 | Separate program `fibi`, or a subcommand of `fibc`. | **Separate** (4.2); name `fibi` now, `fibref` at the retirement of the Rust. |
| D7 | Strings and arrays: implement from the spec over the interpreter's own bytes, or forward to the host's builtins for speed. | **From the spec** for evaluation semantics (slice bounds, UTF-8 checks, find, concat); forward only the host boundary (3.2). Specialise a hot builtin only after measuring (MEMORY "Specialising is fine"). |
| D8 | Gate placement: quick gate (sample), full gate (all). | A quick sample plus a full run in `--full`, with `time: long` hints for the tail if the census shows a tail. |
| D9 | Labels `;; stage: 2` that exist only for the Rust interpreter's gaps: remove them when `fibi` passes. | Remove in a separate reviewed change, after P6 is green. |
| R1 | **Speed**. 2 to 8 million nodes a second; 100 to 300 times slower than native. If the census gives more than 60 CPU-minutes the corpus is a nightly job. | Prepared tree from the start (5.4); census in P0; `time: long`; the compare runs `-j 12`. |
| R2 | **Stack depth** on the host stack: 2,000 to 4,000 source frames at 8 MB (an estimate, to measure). | Re-exec under a raised `RLIMIT_STACK`; B if it falls short. |
| R3 | A **shared miscompile**: stage 2 compiles `fibi` and the program. | Build `fibi` by the seed as well and compare outputs on the corpus (1.4). |
| R4 | **Arithmetic overflow** care (no wrapping, no unsigned): the new place for a subtle bug. | Table-driven tests at each width at the boundaries (the stdlib has `cases/stdlib` limb-boundary cases, e.g. 6010) and differential runs against the host. |
| R5 | **Drift**: the interpreter is a second thing to update with every language change; the Rust was frozen because mirroring cost more than the work it copied (ROADMAP). | The gate stage makes a feature without an interpreter implementation fail the gate, which is rule 6; the cost is paid per feature and shows up in review, not later. Features that touch the plan (new `Pass`, `Op`, `Alloc`) add a line to `prepare` and a heap rule; the plan's data types are a closed set (`own/program.fib`), so the interpreter's `match` over them is checked for exhaustiveness by the type checker. |
| R6 | The 0.7 ratio is the project's, not measured for an interpreter; the estimate may be low (the new work) by 20 to 30 percent. | Packages are independent; P1 to P3 report their actual size after the first week. |
| R7 | The interpreter inherits front-end bugs (1.3, 7). | Stated; each front-end pass keeps its own oracle. |

## 10. Sources read

`CLAUDE.md`; `spec/method.md`; `spec/compiler.md` §§3 to 9; `spec/types.md` §§4.5, 6.12, 8.1, 8.8; `docs/design/{unboxed-option,in-place-update,
lair-interfaces}.md`; `compiler/mirror-pending/{README,X9c-stage2-only}.md`; `crates/fibref/src/eval/*` (6,515 lines; read: `mod`, `interp`,
`expr`, `call`, `native`, `value`, `sched`, `threads`, `unsafe_ops` headers and bodies), `heap/{mod,error,audit}.rs` (2,122 lines in all);
`compiler/types/{ast,program,builtins}.fib`, `compiler/own/{program,explain}.fib`, `compiler/driver/{commands,front,harness,verdict,header,child}.fib`,
`compiler/fibc.fib`, `compiler/macros/runner.fib`, `compiler/expand/{maker,regex}.fib`, `compiler/native/call.fib`; `scripts/{gate,ci-stage2}.sh`;
ROADMAP.md "The performance cycle". Measured: the spike (section 5.1) and the three emit and run times (5.3), with a stage 2 `F` from
`~/.cache/fibber-scratch/gate-fibber/F` on this tree's `lib/`.
