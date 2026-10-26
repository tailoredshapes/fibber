# lIR is the language; fibber is macros and functions on it

Current status: **Proposed**, with stage 2 decided and under way. The direction is the owner's (below); the stages, the forms and
the order are proposals awaiting the decisions in section 7, except D4 to D10, which the owner took as recommended on 2026-10-25.
Nothing here is implemented except where a stage says so. Findings cite file:line on main at `c1ec7a0a` (2026-10-25); the
measured numbers say where they came from.

**Stage 2, steps 1 and 2 landed** (2026-10-25): spec/lir.md 6.14 is the specification (the managed type `(ptr 1)`, the kinds,
the forms, `unsafe`, the `ownership` binding, the ten rules, erasure); `cases/lir/own` holds a must-reject case per rule and
must-accept cases that lower and run through a small runtime in lIR; `compiler/lir/own.fib` and `compiler/lir/own/` are the
verifier, run inside `lir.check` on every path, in the mode `FIB_LIR_OWN` names (`report` the default until step 5); erasure is
`lir.own.erase`, the last pass before LLVM or a printer, after the slot `lir.own.opt`. Two refinements over section 5 below, both
in the spec: a stack value is ended by `dealloc-stack` (SIL's `dealloc_stack`), not by `destroy-value`, so that erasure stays a
local rewrite that needs no kinds; and the owner's pointer to Rust's MIR was weighed (6.14, "Why SIL's words and not MIR's"): MIR's
`Drop`, NLL scopes and `StorageLive`/`StorageDead` are here under SIL's names, and MIR's `move`/`copy` operand words are thicker
than an SSA value needs, so `copy-value` stays an instruction and a move is a plain use at an owned position.

## 1. The direction

The owner, 2026-10-24: "lir is a lisp version of llvm ir. It's a lisp because fibber is just a transformation into lir, and if
lisps do anything well it's transform s-expressions into s-expressions. The next thing they do well is transform s-expressions
into standard data formats." And: "We added a few constructs to make writing borrows and rc much easier and safeguards to resist
side stepping them, but … lir is the language, fibber is what makes it usable and efficient, but it should be a series of lir
macros and functions that transform fibber into lir."

The owner, 2026-10-25, on whether fibber is to lIR as Java is to bytecode: "I want to double down on LIR being the language and
fibber being macros and functions built on top of it." And: "It means lir needs macros and functions (or at least we can define
defun) and the only compiler is a lir compiler and the libraries are included as lir. I think we're a long way from that."

The end state, then:

1. **lair is the only compiler.** It reads lIR, expands lIR macros, verifies, and lowers to LLVM (or C, JS, WGSL).
2. **lIR has compile-time macros and functions.** A `defun`-style definition is available, as a macro if not as a core form.
3. **fibber is a library** of lIR macros and functions that lair loads: a module written in fibber is handed to that library,
   which returns lIR.
4. **Libraries ship as lIR**, the prelude and `fib.*` included, not as fibber source expanded again by every program.
5. **Ownership is checked in lIR.** The borrow and count constructs are lIR forms, and a verifier in lair rejects lIR that
   sidesteps them, whoever wrote it (javac infers, the JVM verifier checks).

## 2. Where we are

What already holds:

- **lIR is every backend's only input.** The runtime is lIR source (`rt/*.lir`), and the compiler itself is lIR. The fixed
  point compares `F.lir` with `F3.lir` (43 MB), and facts are LLVM's own forms (spec/lir.md 6.13).
- **lair can run lIR at compile time.** `macros/runner.fib` JIT-compiles a macro's module through `native.api`/`native.call`
  in process, and calls C entry points with word-sized arguments.
- **The front of the pipeline already rewrites s-expressions.** The reader (2.1k lines) and the expander (10.3k, fused chains
  included) do.
- **The lIR checker knows nothing of fibber** (compiler/lir); it reads only `FIB_LIR_FACTS`.

What does not hold:

- **Ownership is not in lIR.** spec/lir.md 6.10 removed liar's ADR 021 "safe lIR" and says "Ownership is fibber's concern …
  decided before lIR is emitted". ADR 021 was only ever Proposed, and it failed in five ways:
  - its checker (`lir-core/src/borrow.rs`) was called from one test, never from lair;
  - it was not flow-sensitive, with no merge rule at joins;
  - `drop` and `move` lowered to nothing;
  - `Rc` and `Ptr` parameters were exempt;
  - raw `load`/`store` on owned values were allowed.

  Today the emitter turns `compiler/own`'s plan into plain calls on `ptr` (`emit/lower/ops.fib`). `F.lir` holds 23,311
  `fib.retain`, 49,426 `fib.release`, 5,968 `fib.unique?` and 13,775 `fib.alloc` calls across 31,774 defines (counted in the
  last gate's `build/F.lir`).
- **lair knows fibber.** Section 4, stage 0 lists the places.
- **The emitter is one generator, not a chain of transforms.** `emit/` is 10.8k lines (plus 4.9k of generated runtime text).
  It builds lIR records imperatively from the typed AST and two side tables, keyed by expression id: types (`types/`, 12.3k)
  and the ownership plan (`own/`, 5.4k).
- **lIR has no macros**, and never had them: liar's `doc/lIR.md` says "no macros, no special forms".
- **Libraries are source.** ADR 0024 forbids combining separately emitted lIR, because type and keyword ids are numbered per
  program.
- **"lIR stands alone" is untested.** Normal builds hand lair the in-memory tree, not text (driver/native.fib:143/148/151,
  emit/defs/jit.fib:231, macros/runner.fib:160). The tree-versus-text check (`emit/defs/astmod.fib:18-25`, `FIB_AST_VERIFY`)
  is opt-in, and its test `compiler/tests/native/h1-ast-vs-text.sh` is in no make target.

## 3. Rules for the work

1. **The text is the contract.** Anything lair does must follow from the lIR text and spec/lir.md. No name conventions,
   comments, environment variables or side arguments may carry meaning. The tree a build hands lair must print to the text
   that would parse back to it, and the gate checks that.
2. **LLVM's spelling first.** A new lIR form is LLVM IR's construct written as an s-expression (`noinline`, `!range`,
   `dependent-libraries`, `addrspace`). Where LLVM has no construct (ownership, macros), the exception is named, small, and
   erased before LLVM (decision D4).
3. **A check that can be skipped is no check** (liar's lesson). The verifier runs on every lair path: AOT, JIT, live,
   macros and the C, JS and WGSL backends. A mutant that disables it fails the gate.
4. **Each step lands in two commits** when it moves code:
   - first a byte-identical commit, where `F emit compiler/fibc.fib` has the same digest, the goldens hold and the full gate
     passes;
   - then any commit that changes emitted names, shown to be renaming only by an lIR canonicaliser (decision D3).
5. **Compile time is budgeted.** Each new pass costs no more than `lir.check` does today. From the self-build with
   `FIB_TIME=1`: `lir.check` 822 ms, `front.own` 5700 ms, `lir.emit` 11554 ms. Run time is unchanged by construction, and the
   quick bench confirms it.

## 4. Stages

The sizes are estimates from reading the code, not measurements. An agent-week is one agent's focused work, gated.

**Stage 0: lIR stands alone.** Small to medium; each item is independent.

1. Gate the tree-versus-text check: run `h1-ast-vs-text.sh` (or `FIB_AST_VERIFY` over the self-build and the cases) as a
   stamp. Make it work with pruning on.
2. `-slow` names mean `noinline` (`compiler/native/lower.fib:192-197`). This becomes LLVM's `noinline`/`cold` attributes in
   lIR, with `:noinline`/`:cold` on fibber's `defun`; *in progress on a branch*.
3. `;; link-lib:` comments (emit/assemble.fib:31, native/libraries.fib:12-25) and the separate `links` argument become LLVM's
   `!llvm.dependent-libraries`, as `(dependent-library "curl")`; *landed 2026-10-25* (spec/lir.md 4.9, cases/lir/link).
4. `FIB_EXPORTS` and the `exports` argument (native/target.fib:63-65, linkwasm.fib:68-92, driver/shared.fib:81):
   - exports are already `external` and not `hidden` in the text;
   - wasm gets LLVM's `"wasm-export-name"` as `(export-name "x")`;
   - library versus executable becomes a build option.

   *Landed 2026-10-25* (spec/lir.md 4.3, 7.2, 12; cases/lir/attrs/export-name.lir): lair reads `FIB_EXPORTS` nowhere; the
   environment variable remains fibber's own channel from `fibc build --export` to its front end.
5. The target triple becomes `(target (triple …) (datalayout …))`, mirroring LLVM. Today `FIB_TARGET_TRIPLE` picks a row of
   `lir.targets` (fibber's `types.targets` until 2026-10-26), imported by lower.fib:5, lower/intrin.fib:10, api.fib:9 and llvm/target.fib:11. That row:
   - sets musttail versus plain `tail` (lx.fib:28-31/71-73), so spec/lir.md 7.3's guarantee fails on wasm and riscv;
   - retypes libc declares from `i64` to `i32` by name on 32-bit targets (lower.fib:114-129), which the C and JS backends
     do not do.

   fibber must emit types correct for the target.
6. `(align 1)` meaning natural alignment on kernel targets (lower/memory.fib:88-91) goes: fibber emits the alignment it means.
7. Kernel extraction leaves lair. Today it is a fibber pass in lair (native/kernel.fib, from api.fib:114-199): it finds
   `fib.kernel.NAME` tables, rewrites `fib.trap*` and the stack-cell calls, and renames entries to `kernelcc`. Instead:
   - fibber emits the device module;
   - lair keeps only pruning from the `kernelcc` functions;
   - the WGSL printer stops reading `%struct.fib.cell.*` (wgsl.fib:29, wgsl/wx.fib:186-192).
8. Performance and layering:
   - liro partitions by name prefix (liro/partition.fib:41-82); it moves to LLVM's global `partition` or to call-graph
     clustering;
   - liro's `fib.types` policy (liro/file.fib:36-46) moves to fibc;
   - `FIB_CATCH`, `FIB_LIVE` and `FIB_VECTOR_BITS` leave liro's cache key (liro/context.fib:34);
   - `lir.fact.check` becomes a reserved name.
9. A spec/code disagreement, reported and not changed. String constants get `unnamed_addr` (lower/consts.fib:95), so LLVM
   may merge two equal ones, but spec/lir.md 3 says each occurrence is its own constant (decision D12). *Resolved 2026-10-25*
   as D12 recommends: `unnamed_addr` dropped, the spec kept (spec/lir.md 3, item 22).

**Stage 1: an lIR pass pipeline.** Medium; about one agent-week. This is a pass driver with timing per pass and
`--dump-after`, plus an lIR canonicaliser that renames SSA names and labels. These move onto it:
- prune and the facts strip;
- live stubs;
- the hard-coded `vec-nth` inline marking;
- the runtime's per-OS adaptation, from text replacement to rewrites on parsed lIR;
- async resume, moved onto lIR functions.

**Stage 2: ownership in lIR, verified.** Medium to large; section 5 has the design. It ships in this order:
1. spec text and must-reject cases (from `lir-audit/borrow`); *landed*: spec/lir.md 6.14, `cases/lir/own`;
2. the verifier, with planted mutants it must catch; *landed* in report mode: `compiler/lir/own.fib`, `FIB_LIR_OWN`;
3. the emitter emits the forms, with erasure byte-equal to today's lIR and violations counted but not failing;
4. the violations fixed one per commit;
5. the verifier gates;
6. the facts that now live in lIR signatures leave fibber, while inference stays (decision D10).

**Stage 3: `#lang fibber` in lair.** Medium; a few hundred lines of plumbing. lair reads a module and picks the language from
the raw text before reading, as Racket's `#lang` picks a reader. The two readers clash: `@x` means a global in lIR and a deref
in fibber, and `<N x T>`, `{}`, keywords and characters differ too. For a fibber module, lair calls the front end (fibber text
in, lIR text out), first linked statically into lair, later loaded as a library with the lIR object cache. JIT-compiling
`F.lir` at every start is impractical: LLVM spends about 76 s optimising it at O2. This stage changes packaging (lair drives),
not the compiler's structure.

**Stage 4: compile-time macros in lIR.** Medium; with stages 1 and 3, about 2–4 agent-weeks. It adds:
- a module-level `define-macro` (decision D1);
- an s-expression object format owned by lIR rather than by fibber's `Form` layout;
- an expansion loop in lair between reading forms and parsing them.

`defun`-style sugar is a macro in a library. The first user is the runtime's per-target adaptation.

**Stage 5: the emitter becomes passes.** Large; about double stages 1–4. Three steps:
1. Copy the ownership plan onto the typed AST as explicit `retain`/`release`/`end-stack`/`steal` nodes (Carp, Lean 4 and
   Koka do this). Borrowed stack-closure forms and closure specialisation (`TgKnown`, `NAME#I#CODE#`) then become node
   deletions in cloned bodies.
2. Add a structured-lIR layer (lIR instructions plus `if`/`let`/`loop`) with one flattening pass, and an lIR quasi-quote in
   fibber. Port the template-like modules first: arithmetic, SIMD, builtins, GPU (about 1,500 lines).
3. Make match compilation, closure conversion, monomorphisation and specialisation named stages, nanopass-style (Chez Scheme,
   Racket), each with a printer and a dump.

**Stage 6: libraries as lIR.** Research. A library would carry:
- type declarations and function type schemes;
- protocols and instances;
- the ownership signatures of stage 2;
- macros and constant `def` values;
- symbolic type and keyword ids resolved at link time (replacing ADR 0024's ban).

The crux is generics. fibber monomorphises everything, so a library must ship generic bodies at a typed stage and instantiate
them with lowering passes that are themselves lIR libraries; Rust ships compiler-version-locked MIR for the same reason. A
"generic as an lIR template" collapses into this: its lowering depends on layout, type ids and instances, so the template is
the lowering pass.

**Stage 7: fibber's front end as loaded lIR, and an lIR seed.** Research. Type checking and ownership inference are
whole-program analyses, and running them as loaded libraries risks compile time. `F.lir` is not target-neutral today, because
the OS and pointer size are fixed at emit time, so an lIR seed needs stage 0 item 5 first.

**Order.** Stage 0 comes first, because everything after it relies on the text being the contract. Stages 1 and 2 can run in
parallel. Stage 3 needs stage 0, and stage 4 needs stages 1 and 3. Stage 5 starts once stages 1 and 2 have landed.

## 5. Ownership in lIR (stage 2)

The proposed shape is ownership as SSA value kinds, after Swift SIL's ownership SSA (OSSA). A verifier in lair checks it, and
then it is erased, so LLVM sees exactly today's lIR.

**The managed type.** A counted reference that may be null, which cannot be cast to an integer. It is spelled `(ptr 1)`,
LLVM's `ptr addrspace(1)` with non-integral address space 1, as Julia does it, or `obj` (decision D5). Object fields in a
`defstruct` are declared with it, so the text says what is counted.

**Kinds.**
- `owned`: must be consumed once.
- `guaranteed`: borrowed; depends on the values it was borrowed from.
- `none`: null and immortal constants.
- `stack`: a scope-local object.

A parameter is `owned` or guaranteed (the default). Results are owned, and function types carry the kinds, so indirect calls
are checked too.

**Forms**, each lowering to what the emitter writes today:

| form | lowers to |
|---|---|
| `alloc-object` | `fib.alloc` |
| `alloc-stack` | the entry-block `alloca` |
| `copy-value` | `fib.retain` |
| `destroy-value` | `fib.release`, or the inline field drop of a stack object |
| `is-unique` | `fib.unique?`, which also gives uniqueness evidence |
| `load copy` / `load borrow` / `load take` | load, plus a retain for `copy` or a null store for `take` |
| `store init` / `store assign` / `store borrow` | store, plus a release of the old value for `assign` |

One module form, `(ownership (retain @fib.retain) (release @fib.release) …)`, names what the forms lower to, so lair
hard-codes no runtime names.

**The fence.** `(define unsafe …)` is the only place where the managed type converts to `ptr` and back, and where the count
header is addressable. `ptrtoint` on the managed type is refused everywhere. Ownership is still checked inside unsafe
functions, and the gate counts them against a ratchet file. `rt/*.lir` uses `unsafe` where it touches headers. Inline
`(unsafe (lir …))` in fibber stays scalar or `ptr` only.

**The verifier's rules.** It is one dataflow pass per function after `lir.check`:
1. Each owned value is consumed exactly once on every path; `unreachable` discharges everything.
2. No use after a consume.
3. Predecessors agree at every merge, so there are no drop flags.
4. A borrowed value is used only while the values it depends on are live (scope from liveness, NLL-style; decision D7).
5. A borrowed value never escapes without `copy-value`.
6. A stack value never escapes, and a borrow stored into one ends before what it depends on is consumed.
7. At a tail call, no frame-owned or stack value is live, and borrowed arguments depend only on parameters.
8. A field write or `load take` needs uniqueness evidence, or a `mutable` `defstruct` such as cells and task frames
   (decision D8).
9. Managed-to-`ptr` conversion happens only in `unsafe`.
10. Every call, direct or indirect, matches the callee's signature.

**Who does what.** fibber infers: borrowed or owned, stack or heap, tail calls, peeks and steals. fibber also reports
source-level errors. lair verifies and trusts no producer, macros included, since it checks what expansion produced. Live
mode refuses a swap that changes a signature's kinds. The C, JS and WGSL backends read erased lIR, and kernels refuse managed
values.

**Library signatures.** The kinds on `define` and `declare`, the managed fields and `mutable` in `defstruct` are what a later
caller is verified against, like a JVM method descriptor.

**Risks.**
- Today's output relies on retain and release being run-time no-ops for stack and immortal objects, so some of it will not
  verify until fixed. That is why the plan counts violations first.
- Peeks are unsound in general (decision D9).
- Steals, borrowed values carried around loops and phis of borrowed values need care.

## 6. What to measure

For every stage:
- compile time per phase (`FIB_TIME=1` on the self-build);
- `F.lir` size;
- the quick bench.

For stage 2, also:
- violations per rule while counting;
- the `unsafe` count;
- erasure byte equality for every case and for `F.lir`.

## 7. Decisions for the owner

Grouped by the stage that needs them, each with a recommendation.

Before stage 0:
- **D12.** Should string constants stay mergeable (fix the spec), or does each occurrence stay its own constant (drop
  `unnamed_addr`)? Recommended: drop `unnamed_addr` and keep the spec, because the text is the contract. *Taken 2026-10-25*
  (stage 0 item 9).

Before stage 1:
- **D3.** May the emit goldens' fresh-name order change once the canonicaliser shows a change is renaming only? Recommended:
  yes.

Before stage 2:
- **D4.** Should lIR accept a small ownership vocabulary that LLVM lacks, named after SIL and erased before LLVM, as the
  exception to "new forms mirror LLVM IR"? Recommended: yes. This is the "few constructs to make writing borrows and rc much
  easier".
- **D5.** Should the managed type be spelled `(ptr 1)` (LLVM's addrspace) or `obj`? Recommended: `(ptr 1)`. The type stays
  LLVM's, and only the operations are new.
- **D6.** Should `unsafe` apply per `define` only, or also to a block inside a function? Recommended: per `define`, the
  simplest to count.
- **D7.** Should borrow scopes be inferred from liveness, or written with `begin-borrow`/`end-borrow`? Recommended: inferred.
- **D8.** Should uniqueness evidence for field writes be checked from the start, or should counts be verified first?
  Recommended: counts first, and field writes as the next ratchet.
- **D9.** Should peeks be made checkable, or marked unsafe and counted? Recommended: unsafe and counted first.
- **D10.** Should ownership inference live in fibber's library or in lair? Recommended: fibber's library. lair stays a small
  verifier and the trusted base. An opt-in Perceus-style insertion pass for hand-written lIR can come later.

Before stage 4:
- **D1.** Should spec/lir.md gain a module-level `define-macro`, or should macros sit in a layer above lIR? Recommended: in
  lIR, as the owner's "lir needs macros" says. `defun` would be a macro in a library.

Before stage 5:
- **D2.** Should the intermediate stages be typed records that print as s-expressions, or plain s-expressions everywhere?
  Recommended: typed records. Each stage's shape stays checked, lIR already works this way, and the front end's time is
  already dominated by drops and map lookups.

Before stage 6:
- **D11.** Should symbolic type ids come now, or with the library work? Recommended: with the library work.
