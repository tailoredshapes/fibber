# lair in fibber

Status: design, written for phase 0 package 0B of the lair-in-fibber project (2026-10-04). Nothing here is code. A figure
marked **measured** was produced in this session and its command is quoted; a figure marked **estimate** is arithmetic on
measured figures or on ROADMAP numbers, and is a claim to be proved by the gates in section 3. Rust line counts are from
`wc -l` on the tree at `d0c6719`.

The goal: `crates/lir` (4,505 lines: reader, AST, whole-module checker) and `crates/lair` (6,208 lines: lowering, JIT, AOT,
C interface, case harness, fuzzers) are rewritten in fibber. The port calls LLVM 21 through its C API with `extern`
(`compiler/llvm/`, package 0A). The compiler (`compiler/`) stops using `liblair.so` (`compiler/lair/*.fib`, the stopgap of
spec/compiler.md §9) and links LLVM itself. Rust is frozen except `crates/lair`; this project replaces lair and does not
extend it, so nothing below proposes a change to `crates/lir`, `crates/fibc` or `crates/fibref`.

## 0. What I read and measured

Read: CLAUDE.md, spec/method.md, spec/compiler.md §9, spec/bootstrap.md §1 and §5.3, spec/syntax.md §3.15, spec/types.md §2.14,
ROADMAP.md (M6, "Performance"), `crates/lair/include/lair.h`, every file of `crates/lir/src` and `crates/lair/src` by its
head and public surface (`ast.rs`, `types.rs`, `check/mod.rs`, `check/expr.rs`, `lower/mod.rs`, `jit/mod.rs`, `aot.rs`,
`aot/link.rs`, `llvm/*.rs`, `cli.rs`, `capi/mailbox.rs`, `capi/exchange.rs`, `cases/*.rs`, `fuzz/*.rs`), `compiler/lair/*.fib`,
`compiler/emit/defs/jit.fib`, `compiler/driver/native.fib`, `compiler/driver/proc.fib`, `compiler/types/builtins.fib` (the raw
pointer builtins), `scripts/package.sh`, `scripts/ci-stage2.sh`, `.github/workflows/ci.yml`, `SEED`.

Measured, with the v0.1.4 seed (`~/.cache/fibber-scratch/LAIR0A/seed/.../bin/fibc`), the tree's `lib/`, the release `lair`
(`target/release/lair`) and `ulimit -v 16000000`, script `~/.cache/fibber-scratch/LAIR0B/m.sh`:

```
$ fibc emit -I compiler -I lib compiler/fibc.fib > fibc.lir
	Elapsed (wall clock) time: 0:08.76          Maximum resident set size (kbytes): 527332
$ ls -l fibc.lir; wc -l fibc.lir; grep -c "^(define" fibc.lir
-rw-rw-r-- 1 tmarsh tmarsh 19121917 ... fibc.lir        404274 fibc.lir        14667
$ lair check fibc.lir                  Elapsed 0:00.74    Maximum resident set size (kbytes): 721360
$ lair emit-llvm fibc.lir > fibc.ll    Elapsed 0:01.19    Maximum resident set size (kbytes): 722036
-rw-rw-r-- 1 tmarsh tmarsh 22928787 ... fibc.ll
```

(`fibc emit` of the compiler's own source took 8.76 s here, against the 30 s the ROADMAP reports for another machine and
build; I use the ROADMAP figures only as ratios.) Also measured: `nm -D --defined-only
liblair.so | grep -c ' T lair_'` prints 23 and `lair.h` declares 23 distinct `lair_*` functions; spec/compiler.md §9 says "22
functions" and ROADMAP.md line 241 says 21. The header and the library agree; the two documents are stale
(`lair_jit_new_fast_codegen` is the 23rd). Per CLAUDE.md I report the disagreement and change neither; section 3 fixes
the spec when the C interface is retired.

Also measured: the v0.1.4 tarball holds `bin/fibc` 6,830,184 bytes and `lib/liblair.so` 62,631,824 bytes (LLVM linked in
statically), compressed tarball 25,763,931 bytes.

## 1. Architecture

### 1.1 Decisions, short

1. Two new module trees: `compiler/lir/` (namespace `lir.*`: reader, AST, checker, printer, dump) and `compiler/native/`
   (namespace `native.*`: lowering, passes, target, JIT, AOT, the `lair.h` replacement, case harness, fuzzers). Bindings of
   LLVM-C are `compiler/llvm/` (`llvm.*`, package 0A). I rejected `lair2`: the name would outlive its reason. The
   existing `compiler/lair/` (namespace `lair.*`, the C-interface bindings) stays until the cutover commit and is then
   deleted, which frees the name. The command-line tool is `compiler/lairf.fib` (the binary `lairf`: `check`, `run`,
   `build`, `emit-llvm`, `cases`, `fuzz`, `fuzz-one`, `dump-ast`, the same CLI as `crates/lair/src/cli.rs`), renamed `lair`
   when the Rust one goes.
2. The first version keeps the TEXT interface: `native.api` takes lIR text, as `lair_jit_add_source` and
   `lair_check_source` do. A faithful port is oracle-comparable file for file (section 2). The in-memory hand-over from the
   emitter is stage 9 (section 3), explicitly later.
3. `native.api` has the same function names and signatures as `lair.jit` and `lair.call` (`jit-new`, `jit-new-fast-codegen`,
   `jit-free`, `jit-add-source`, `jit-address`, `jit-c-entry`, `check-source`, `build-executable`, `call-i64`, `call-f64`, the
   mailbox functions, `hook-addresses`), so the cutover of every user is a changed `:require` line, not a rewrite
   (users, by `grep -rln "lair\.\(jit\|call\|fibm\|expand\|ffi\)" compiler`: `emit/defs/jit.fib`, `macros/runner.fib`,
   `emit/macros/front.fib`, `driver/native.fib`, `expand/libdir.fib`, `jit-demo.fib`, `tests/emit/unit-resume.fib`, and the
   `lair.fibm` and `lair.expand` modules themselves).
4. Ownership of LLVM objects: every LLVM handle is an `i64` (the convention of `compiler/lair/ffi.fib`; it also gives
   nullable pointers, since fibber has no null `ptr`). A context, module, target machine or LLJIT is held in a record that is
   passed explicitly and released by an explicit `dispose` call, as `jit-free` is today. No global mutable state
   (CLAUDE.md): LLVM's target initialisation, which Rust guards with a `Once` (`llvm/target.rs`), is called on each
   `machine-host` and `jit-new` instead; LLVM's `LLVMInitializeX86*` functions are idempotent.

### 1.2 Module map: every Rust file to a fibber module

The estimate column applies the ROADMAP's observed ratio for idiomatic ports ("about 0.7 times the Rust's lines", M6 method
note) and is an estimate. Files stay under 500 lines and functions under 50 (CLAUDE.md), so a file is the unit of a package.

`crates/lir` (4,505 lines):

| Rust | Lines | Fibber module (file under `compiler/lir/`) | Est. |
|---|---|---|---|
| `diag.rs` | 49 | `lir.diag` (`diag.fib`): `Pos`, `Diagnostic`, `err`, `render` | 40 |
| `types.rs` | 202 | `lir.types` (`types.fib`): `Type`, `FnType`, `Cc`, size and layout queries | 150 |
| `ast.rs` | 355 | `lir.ast` (`ast.fib`): `Module`, `Item`, `Function`, `Block`, `Expr`, `Kind`, the operator enums | 260 |
| `sexp/{mod,lexer,reader}.rs` | 353 | `lir.sexp` (`sexp.fib` types and reader), `lir.sexp.lexer` | 260 |
| `parse/{mod,names,ty,literal}.rs` | 486 | `lir.parse` (`parse.fib`), `lir.parse.names`, `.ty`, `.literal` | 340 |
| `parse/{items,instr,expr,memory,atomics,control}.rs` | 918 | `lir.parse.items`, `.instr`, `.expr`, `.memory`, `.atomics`, `.control` | 640 |
| `check/{env,fcx,func,cfg,walk,globals,phi,mod}.rs` | 1,021 | `lir.check.env`, `.fcx`, `.func`, `.cfg`, `.walk`, `.globals`, `.phi`, `lir.check` | 710 |
| `check/{expr,arith,aggr,calls,memory}.rs` | 1,091 | `lir.check.expr`, `.arith`, `.aggr`, `.calls`, `.memory` | 770 |
| (new) | 0 | `lir.print` (AST to text that parses back), `lir.dump` (the oracle dump, section 2) | 350 |

`crates/lair` (6,208 lines):

| Rust | Lines | Fibber module (file under `compiler/native/`) | Est. |
|---|---|---|---|
| `error.rs`, `pipeline.rs`, `lib.rs`, `main.rs` | 123 | `native.error` (`LairError`: `Invalid (Vec Diagnostic)`, `Internal`, `Jit`, `Backend`; `render`), `native.pipeline` | 90 |
| `llvm/mod.rs` (Owned context and module, `take_message`) | 94 | `native.owned` | 80 |
| `llvm/passes.rs` | 43 | `native.passes` (`default<On>` through `LLVMRunPasses`, `error-message`) | 50 |
| `llvm/target.rs` | 157 | `native.target` (`machine-host`, `FIB_TARGET_CPU`, opt level, PIC) | 120 |
| `lower/{mod,types,consts,func}.rs` | 539 | `native.lower`, `.types`, `.consts`, `.func` | 380 |
| `lower/{expr,arith,calls,memory}.rs` | 678 | `native.lower.expr`, `.arith`, `.calls`, `.memory` | 470 |
| `jit/mod.rs`, `names.rs`, `prune.rs`, `trampoline.rs` | 622 | `native.jit` (session), `.names`, `.prune`, `.trampoline` | 440 |
| `aot.rs`, `aot/link.rs` | 251 | `native.aot` (`emit`, `build-executable`), `native.link` | 180 |
| `capi/{jit,check,aot}.rs` | 313 | `native.api` (the facade of 1.1 item 3, no C types, no `lair_error`) | 140 |
| `capi/{call,mailbox,exchange}.rs`, `header.rs`, `error.rs` | 1,279 | `native.call` and `native.support` (1.4); the C types, `shield`, `lair_error` and the header test disappear | 350 |
| `cases/*.rs` | 429 | `native.cases`, `.header`, `.verdict`, `.exec` | 300 |
| `cli.rs` | 294 | `compiler/lairf.fib` and `native.cli` | 200 |
| `fuzz/*.rs` | 1,386 | `native.fuzz`, `.mutate`, `.grammar`, `.tree`, `.verdict`, `.deep`, `.rng` (stage 8, optional) | 970 |

Totals: about 3,520 fibber lines for `lir.*`, 2,670 for `native.*` without the fuzzers, 970 for them, plus the LLVM bindings
of 0A. About 7,200 lines in all (estimate). `compiler/` is 36,563 lines today (`find compiler -name '*.fib' -not -path
'compiler/tests/*' | xargs cat | wc -l`, measured), so the port is about 20% more source.

### 1.3 Data representation

Positions. `Pos` is one `i64`, `line << 32 | col`, with `pos-line`, `pos-col`, `pos-new` and `pos-str` ("L:C"). It is a scalar:
no object, no count, no header per node. (Rust: `struct Pos { line: u32, col: u32 }`, 1-based.) A diagnostic is
`(defrecord Diagnostic (pos: i64 message: str))` and prints as `L:C: error: MESSAGE`, as `diag.rs` does.

Types. `Type` is a `defenum`: `(Int bits: i64)`, `Float`, `Double`, `Ptr`, `(Vector lanes: i64 elem: Type)`, `(Named name: str)`,
`(Anon fields: (Vec Type))`, `(Array n: i64 elem: Type)`. The Rust `Box<Type>` becomes a plain field: every fibber object is
already on the heap. A `clone` of a `Type`, which `check/*.rs` does freely, becomes a retain. One fact I did not
establish, flagged for the S1 package: whether a payload-free variant is a shared constant or an allocation. S1 settles it with a
`FIB_TRACE` case; if it allocates, `Float`, `Double` and `Ptr` become tagged `i64` constants instead.

AST. A direct translation, not an arena. `Expr` is `(defstruct Expr (kind: Kind pos: i64))`; `Kind` is a `defenum` of the 40
variants of `ast.rs` (counted with `awk`; the Rust enum is `ast.rs:243` on) with named fields (`(Bin op: BinOp a: Expr b: Expr)`, `(Call callee: Callee args: (Vec Expr) tail: bool)`);
`Item`, `Function`, `Block`, `Binding`, `Callee` likewise; the operator enums (`BinOp`, `UnOp`, `OvfOp`, `CastOp`, `IPred`,
`FPred`, `Ordering`, `RmwOp`, `Scope`, `Linkage`) are payload-free `defenum`s. A tree of owned objects keeps the port reviewable
against the Rust side by side (the rule of spec/bootstrap.md §1: "Modules are named after the Rust module they replace"), and
the checker only reads it. An arena (`Vec` of nodes with `i32` ids) would shrink the heap but changes every function's shape;
section 4 (R4) keeps it as the fallback if the measured memory is too high.

Integer literals. Rust holds them as `i128` because lIR admits `-2^(K-1) <= v <= 2^K - 1` for `K <= 64`
(`parse/literal.rs:37`, test at `:207` for `u64::MAX`), and fibber has no integer wider than `i64` (spec/syntax.md §1.1, the
integer row: "no unsigned types"). A literal is therefore the pair `(bits: i64, neg: bool)`: the low 64 bits of the value in two's complement
and its sign. `int_fits`, `low_bits` (`lower/consts.rs:18`), the `mask`/`u128` comparisons of `check/arith.rs:34-71`,
`check/aggr.rs:98-144` and `calls.rs:166` (duplicate switch cases) are written against the pair; `literal.fib` detects overflow
beyond 64 bits while accumulating digits, because the Rust test requires `99999999999999999999999999999999999999999` to
fail with "out of range". The index lists of `ExtractValue` and `InsertValue` are `Vec<i128>`
too (`parse/instr.rs:176` accepts any integer token) and are compared with field counts and printed in messages
(`check/aggr.rs:69-79`, `check/memory.rs:144`), so they use the same pair. Float literals are `f64`, as in Rust.

Strings. `Str(Vec<u8>)` is a `str` of raw bytes if `str` may hold non-UTF-8 bytes (the compiler's `Array i8` and `str-from-bytes`
exist: `compiler/types/builtins.fib:59-60`); otherwise an `(Array i8)`. S0 decides by a case with bytes above 0x7f.

Maps. `HashMap<String, T>` is the library's persistent `Map`, updated in place when unique (docs/design/in-place-update.md,
landed). `Env` is built once and borrowed; `Fcx` holds a `Map` of local types that is rebuilt by `assoc` per instruction,
which is the checker's hot path (R3).

Mutable checker state. `Fcx` has `&mut self` fields (`emitted`, `defs`, `allocas`, `types`). In fibber they are cells inside the
`Fcx` record that every function takes explicitly (the cursor's three cells in `syntax/cursor.fib` are the pattern named in
spec/bootstrap.md §1). Errors are `(Result T Diagnostic)`, and functions use `try-let`.

The cyclic checker. This is the one structural problem of the port. In Rust, `check/expr.rs` (`ty`) dispatches to `arith.rs`,
`calls.rs`, `memory.rs`, `aggr.rs`, and every one of them calls back into `ty` and `val` for its operands: five files, one
recursion. spec/syntax.md §5 and spec/bootstrap.md §5.3 allow mutual recursion only inside one module, so the five files
cannot be five modules that call each other. Decision: the rules become pure functions of operand types, and the recursion lives
in one dispatcher. `lir.check.expr` types the operands of a `Kind` in the order the Rust does (left to right), then calls
`lir.check.arith/bin-rule op ta tb pos`, `calls/call-rule`, etc., which depend only on `check.fcx` and `check.env`. Where a
rule interleaves operand typing with its own checks so that the order of errors would change (`select`, `phi` through
`lookup_at_end`, the `let` form, `switch`), the porter keeps the interleaving by passing the dispatcher in a field of
`Fcx` (`(. fcx ty)`, a closure set once by `lir.check`, as `Runner` is in `expand.ctx`). The package finishes the list of the
rules that need the field before writing them, and the compare scripts of section 2 decide the order question: a diagnostic
that arrives in a different order than the Rust's is a failure. The same shape applies to `native.lower.*`: `lower/expr.rs`
(`Lx::expr`) is called by `arith`, `calls` and `memory`, so `native.lower.expr` lowers operands first, in the Rust's order, and
passes LLVM values to `lower-bin`, `lower-call`, `lower-load`.

### 1.4 The text reader, and where the AST comes from

The reader (`lir.sexp`, `lir.parse.*`) turns text into `Module`. First version: whole text, whole module, as `lir::parse` does
(`lib.rs:25`: `sexp::read` then `parse::module`), so the diagnostic of a lexical error, which Rust finds before any parse
error, is the same. The lexer works on byte offsets (`str-byte-at`) and makes a `str` only for an atom or a string literal;
it tracks line and column as the Rust lexer does (`sexp/lexer.rs`), and it never builds a position object. This matters:
spec/bootstrap.md §4 measured the compiled Stx reader at 4.6 microseconds per atom and 19.4 s for 8 MiB of one-character
integers, and the lIR reader must be several times cheaper per token than that one, which keeps a `Pos` per node and a token
object per atom (R3).

The in-memory hand-over (wave 5, H1) is in for the per-`def` sessions of the emitter, in a form short of stage 9 as first written.
The emitter still renders text (`compiler/emit/ir.fib`, `text.fib`, the lowering), but `emit.assemble/module-pieces` gives the module so
far as pieces that are the same text every time it is assembled again (the runtime, one object's structs and walkers, one static
string, one function, one `def` constant), and `emit.defs.astmod` reads each piece once with `lir.parse`, keeps the items by the piece's
text, and gives the session an `LModule` of those items and the session's own few lines (`native.api/jit-add-module`). The session
drops what no exported definition reaches (`dead-internals`) before it checks, so a `def` whose initialiser reaches 10% of the module
checks 10% of it. Measured (the compiler's own 139 `def`s): the sessions had read 134 MB of text, 21.8 s of the 25.3 s they took was
`parse+check`; `emit compiler/fibc.fib` went from 24-25 s to 9.7-11.9 s with its output unchanged. What this is not: the emitter does not
build trees from its data (lowering, `defs.data`, the runtime are text), so the trees are the reading of the pieces; a diagnostic of
a session names a position within its piece; and a function no entry reaches is not checked in a `def` session (the whole program is
still checked by `build`, and macro modules still go through `session-add-source`). `lir.print` stays for `fibc emit` and as the
oracle: `FIB_AST_VERIFY=1` makes every session and the final module compare the print of its trees with the print of the whole text
read again, and compiler/tests/native/h1-ast-vs-text.sh runs that over the emit corpus (`--fault` plants a fault in the builder and
requires it to be caught). The stage-9 end state, trees built from the emitter's data, remains the way to avoid even the first read.

### 1.5 The C interface, entry by entry

`crates/lair/include/lair.h` declares 23 functions. This is what each becomes. Fibber cannot export a C-callable function
(syntax.md §3.15 has `extern` only; bootstrap.md and compiler.md §9: "fibber cannot give C a pointer to one of its
functions"), so the library `liblair.so` cannot be rebuilt in fibber; section 1.6 says what that means for third parties.

| `lair.h` entry | Used today by | Replaced by |
|---|---|---|
| `lair_error_text`, `lair_error_free` | `ffi/status`, `ffi/word-result` | nothing: errors are `(Result T str)` from the start; no `lair_error` object, no disposal |
| `lair_jit_new`, `lair_jit_new_fast_codegen`, `lair_jit_free` | `emit.defs.jit`, `macros.runner`, `driver.native`, `jit-demo` | `native.api/jit-new`, `jit-new-fast-codegen` (a target machine at `CodeGenLevelNone`, `jit/mod.rs:104-117`), `jit-free` |
| `lair_jit_add_source` | the same | `native.api/jit-add-source`: `lir.parse` then `lir.check` then `native.jit/add-module` |
| `lair_jit_address`, `lair_jit_c_entry` | the same | `jit-address`, `jit-c-entry` (the `tailcc` trampoline of `trampoline.rs` is lIR text built in fibber and added through the same pipeline) |
| `lair_check_source` | `check-source`, `lair`-less checks of `compiler/` | `native.api/check-source` (`lir.check`) |
| `lair_build_executable`, `lair_build_executable_with` | `driver.native/build-lir` | `native.api/build-executable`: lower, optimise, emit an object, write it, `cc` with `-lm -lpthread`, `-L` and the absolute rpath of `aot/link.rs`, the libraries last |
| `lair_call_i64`, `lair_call_f64` | `lair.call`, `driver.native/call-main`, `emit.defs.jit` | `native.call/call-i64`, `call-f64`; see the function-pointer paragraph below |
| `lair_call_new`, `lair_call_free`, `lair_hook1_address`, `lair_hook2_address`, `lair_call_start`, `lair_call_wait`, `lair_call_hook_arg`, `lair_call_hook_reply`, `lair_call_result`, `lair_call_fault` | `lair.call`, `macros.runner`, `lair.expand` | the mailbox protocol of `capi/exchange.rs` (382 lines of state machine) moved into lIR text, `native.support`, compiled once into the process's JIT and reached by address |

The function-pointer gap, the one thing a direct call does not give. A fibber function cannot call an address
(compiler.md §9: "fibber cannot call a function pointer itself"); that is why `lair_call_i64` exists. Without a C library
of ours, the port needs one primitive that calls an address on the compiler's thread. Options, in my order of preference:

(a) A shim made at session start in lIR text through our own pipeline (a global slot holding callee, argument count and up to
eight arguments; a `void()` function that reads the slot, switches on the count, calls through the pointer with the
`ccc` convention and stores the result), invoked with a libc function that calls a `void (*)(void)` on the calling thread:
`pthread_once(&control, go)` with a fresh four-byte control block. No Rust edit, no change to `fib.rt`, same thread, one global
in the JIT's address space (single compiler thread; the protocol of compiler.md §9 already forbids the module's data being
touched by two threads at once). The fragility is that it relies on glibc running `init_routine` on the caller's thread,
which POSIX specifies.
(b) `fib.rt` gains a part `call.lir` with the same shim as an ordinary function, called by `extern`. Cleanest, but the part
list is `include_str!` in `crates/fibc/src/compile.rs:20-25` and generated into `compiler/emit/runtime.fib`: a Rust edit plus a
regeneration, which "Rust is frozen" forbids without the owner's word (D3 in section 4).
(c) A language builtin `(call-ptr addr arg..)`. The right answer long term; it changes spec, types, ownership and the emitter
and is not on the critical path.

Recommendation: (a) for stages 4 to 7, owner decision D3 on (b) or (c) after. 0A's report of what `extern` cannot express may
change this; the design does not otherwise depend on it, because only `native.call` (about 100 lines) touches it.

The mailbox hooks. A hook is a C function that a macro-time module calls with `(cx, a)` or `(cx, a, b)`; it must block the macro's
thread until the compiler answers. It cannot be fibber (no export), so `native.support` is lIR text (about 250 lines, written
once, embedded as a fibber string) with the worker-thread entry (`pthread_create` with `pthread_attr_setstacksize` 64 MiB, as
`capi/mailbox.rs` does), a mutex and two condition variables, and the state machine of `exchange.rs` including its defined misuse
behaviour (the fault text, `wait` returning -1 once). Its tests are the Rust ones ported: `capi_mailbox.rs` (194 lines) and
`capi_misuse.rs` (339 lines) become `compiler/tests/native/mailbox-*.fib` cases with the same expected outputs.

### 1.6 Does `liblair.so` remain

For the compiler: no, after the cutover (stage 7) `compiler/` does not reference it. For external C consumers: fibber cannot
build a shared library with C entry points, so `lair.h` cannot be re-implemented. The consumers I found in the tree are tests:
`crates/lair/tests/c_consumer.rs` (453 lines) with `tests/c/consumer.c`, `capi.rs` (360), `capi_mailbox.rs`, `capi_misuse.rs`,
`capi_panic.rs`, and `crates/fibc/tests/capi/{basic,macros}.rs` with `compiler/jit-demo.fib`. I know of no consumer outside the
repository; the owner confirms (D2). Plan: the Rust `crates/lair`, with its cdylib and these tests, is kept as legacy and not
changed from the cutover until it is removed, because the Rust `crates/fibc` (stage 1) depends on it
(`crates/fibc/Cargo.toml:20,32`: feature `llvm`, `dep:lair`; sources `src/defs/jit.rs`, `src/macros/{mod,module}.rs`,
`src/main.rs`) and on `lir` unconditionally (`:31`). It is removed together with the Rust `fibc`, not before. The coverage the
C tests give moves to `compiler/tests/native/` as fibber cases (section 2).

## 2. Oracles and tests

The principle of spec/method.md carries over: the Rust `lair` is the oracle until it is deleted; no stage is done because a
document says so, but because a comparison script exits 0. Every comparison is of bytes, and a script that has nothing to
compare is PENDING, never a pass (the style of `compiler/tests/emit/call.sh`: same, different, pending, skipped counts; exit 1
for a difference, 70 for pending).

### 2.1 What is compared with what

| Layer | Oracle (Rust) | Port | Compared |
|---|---|---|---|
| Reader and AST | `lair dump-ast FILE` (new, 2.2) | `lairf dump-ast FILE` | the canonical dump, byte for byte; a read or parse failure compares its diagnostic line |
| Checker | `lair check FILE` | `lairf check FILE` | standard error and exit status, message for message, in order, position included |
| Lowering | `lair emit-llvm FILE` (`LLVMPrintModuleToString` of the verified, unoptimised module, `pipeline.rs:21`) | `lairf emit-llvm FILE` | the whole text, byte for byte |
| Optimisation and target | `lair build --emit llvm -O n`, `--emit asm`, `--emit obj` | `lairf build ...` | the optimised IR, assembly and object file, with `FIB_TARGET_CPU=x86-64-v2` on both so the result does not depend on the machine |
| JIT | `lair run FILE`, `lair cases` | `lairf run`, `lairf cases` | the output and exit status of every run |
| Cases | `lair cases cases/lir` (323 cases: adversarial, audit, instr, mapping, verify) | `lairf cases cases/lir` | the verdict table, line by line |
| Compiler as a user | `fibc cases` with `lair.*` | the same compiler built with `native.*` | the case tables of `scripts/ci-stage2.sh` and the golden outputs of `compiler/jit-demo.fib` |

Each `lairf` command runs the same file; the scripts live in `compiler/tests/native/` (`compare-ast.sh`, `compare-check.sh`,
`compare-llvm.sh`, `compare-object.sh`, `compare-cases.sh`) in the style of `compiler/tests/emit/call.sh`: one file per
argument, a time limit and `ulimit -v 4000000` per run, and the counts at the end.

### 2.2 The dump of the AST (the one Rust edit)

There is no AST dump in Rust, and `{:#?}` is neither compact nor stable. `crates/lair` is the one crate this project may change, so
the oracle tooling is a new file `crates/lair/src/dump.rs` (about 150 lines, with unit tests next to it, and a `dump-ast`
subcommand in `cli.rs`) printing the AST as one line per node in a fixed grammar: `(kind LINE:COL field..)` with names and
literals as their text and the integer pair of 1.3 written as the signed decimal of the value. The fibber `lir.dump` prints the
same grammar. The dump is the oracle of stages 1 to 2 and of the printer: `lir.print(parse(text))` is parsed again and dumped,
and the two dumps must match.

### 2.3 The corpus

The lIR the compiler emits is the representative input: 14,667 functions, 19.1 MB for the compiler's own source (measured
above), and every case of `cases/` is another program. `scripts/lair-corpus.sh` (stage 0) runs the seed `fibc emit` over
`cases/ownership` (263 files), `cases/modules` (28), a fixed sample of every tenth `cases/stdlib` file (992 files, so about 100),
`compiler/fibc.fib`, and each of the `crates/fibc/rt/*.lir` parts alone, into `~/.cache/fibber-scratch/lair-corpus/` (the big
disk, per the owner's rule, never `/tmp`; never committed). The `cases/lir` files (323) are the rejecting and the odd inputs the
emitter never produces; together they exercise the checker's messages, which the emitted corpus does not. Both sets
run through every script. Sweeps follow the owner's limit: no more than 8 jobs at a time, a few hundred files per run.

### 2.4 The fuzzers

The fuzzers (`fuzz/*.rs`, 1,386 lines) exist to find a checker or lowering path that crashes or accepts invalid lIR
(method rule 7). They are ported last (stage 8, optional) and in the meantime used as differential drivers: the Rust
`lair fuzz` writes its mutants (a `--keep DIR` option in `crates/lair`, about 20 lines) and the scripts run each mutant through
`lair check` and `lairf check` and, for those accepted, `emit-llvm`. A mutant on which they differ is a bug in the port, or in
Rust, and goes to `cases/lir/audit` as a case, as the existing findings did. Port decision at stage 8: port `mutate` and
`grammar` only if the owner wants the Rust crate deleted before the Rust `fibc`; otherwise leave them.

### 2.5 The fixed point

Let T be the tree after the cutover commit (its `compiler/` requires `native.*`). Stage 7 requires, in this order:

1. `F0` = T built by the seed (v0.1.4, with its `liblair.so`): `fibc build compiler/fibc.fib -I compiler -I lib -L LLVMLIBDIR -l ...
   -o F0`. `F0` contains the port; the seed's lair compiled it.
2. `F0 emit compiler/fibc.fib` equals `seed emit compiler/fibc.fib` (the existing stage check; the front end is unchanged).
3. `F1` = `F0 build compiler/fibc.fib ...`: now the port compiled the compiler. `F1 emit` equals `F0 emit`, and `F2` = `F1 build ...`
   equals `F1` as files (`cmp`): the fixed point.
4. The cross-check that proves the port is the same code generator: the object the seed built `F0` from and the object `F0` built
   `F1` from are byte-identical (`--emit obj`, both with `FIB_TARGET_CPU=x86-64-v2`), and so is the linked executable if the `cc`
   command line is the same. Any difference is a lowering or pass-order difference to be found with `compare-llvm.sh` before
   anything else.

`scripts/package.sh` already does steps 2 and 3 (its "stage check"); stage 7 extends it with 4.

## 3. The staged cutover

### 3.1 Stages

Each stage ends with its gate (an executable comparison) and a commit; nothing is pushed or tagged by an agent.

| # | Stage | Gate (exit 0 required) |
|---|---|---|
| 0 | Oracles: `dump.rs` and `dump-ast` in `crates/lair`, `--keep DIR` for the fuzzer, `scripts/lair-corpus.sh`, the five compare scripts with the fibber side stubbed | the scripts run and report PENDING for every file; `cargo test -p lair` and clippy pass |
| 1 | Skeleton and reader: `lir.diag`, `lir.types`, `lir.ast` real; every other `lir.*` and `native.*` function present as a `todo:` stub of its final signature (spec/bootstrap.md §5.3 did this); then `lir.sexp`, `lir.parse.*`, `lir.dump`, `lir.print` | `compare-ast.sh`: same on all of the corpus and `cases/lir` (parse failures included) |
| 2 | Checker `lir.check.*` | `compare-check.sh`: same diagnostics on `cases/lir` and the corpus |
| 3 | Lowering `native.lower.*`, `native.owned`, `native.passes`, `native.target` | `compare-llvm.sh`: same text; `--emit obj` same bytes at `-O 0`..`3` |
| 4 | JIT `native.jit.*`, `native.call`, `native.support`, `native.api` | `compare-cases.sh` for the JIT path of `cases/lir`; `compiler/jit-demo.fib` through `native.api` has the golden output of `crates/fibc/tests/capi/basic.rs`; the mailbox cases pass |
| 5 | AOT and link `native.aot`, `native.link` | object bytes equal (stage 3 gate) and every case of `cases/lir` AOT path, plus the rpath test of `crates/lair/tests/link.rs` as a fibber case |
| 6 | Harness and CLI `native.cases`, `compiler/lairf.fib` | `lairf cases cases/lir` equals `lair cases cases/lir`, line for line |
| 7 | Flip: the `:require` lines of 1.1 item 3, the link line of the build scripts, CI, `scripts/package.sh`; the tarball loses `lib/liblair.so` | section 2.5 steps 1 to 4, then `scripts/ci-stage2.sh F1` equals `scripts/ci-stage2.expected` |
| 8 | Fuzzers (optional) | the ported `fuzz` over the seeds the Rust fuzz test uses finds nothing the Rust did not |
| 9 | In-memory hand-over | `emit` builds `lir.ast`; `lir.print` of it equals the text emitter byte for byte over the corpus; the per-def sessions no longer parse |
| 10 | Retirement (with the Rust `fibc`) | section 3.4 |

### 3.2 Which packages are independent

These start together once stage 0 (one agent, small, serial) and the skeleton of stage 1 (one agent, serial, the data types
and stubs; a day) exist. A package owns a list of files and may not edit anyone else's. The interface each one fixes is in the
third column and is part of the skeleton: signatures are final before bodies, as bootstrap.md §5.3 required.

| Package | Files | Fixed interface | Depends on |
|---|---|---|---|
| S1 skeleton | `lir/{diag,types,ast}.fib` real, stubs everywhere | the types of 1.3; `lir.sexp/read : str -> (Result (Vec Sexp) Diagnostic)`; `lir.parse/module : (Vec Sexp) -> (Result Module Diagnostic)`; `lir.check/check : Module -> (Result unit (Vec Diagnostic))`; `lir.check/check-main`; `native.lower/lower : Module str str str -> (Result Owned LairError)` | 0A (bindings exist) |
| R reader | `lir/sexp.fib`, `lir/sexp/lexer.fib`, `lir/parse.fib`, `lir/parse/*.fib`, `lir/dump.fib`, `lir/print.fib` | as above | S1 |
| K1 checker core | `lir/check.fib`, `check/{env,fcx,func,cfg,walk,globals,phi}.fib` | `Env`, `Fcx` (cells inside it), `lookup`, `valid`, `same`, the dispatcher field of 1.3 | S1 |
| K2 checker rules | `lir/check/{expr,arith,aggr,calls,memory}.fib` | pure rule functions of operand types (1.3) | S1; tests against R's output or hand-built ASTs |
| L1 lowering core | `native/{owned,passes,target,error,pipeline}.fib`, `native/lower.fib`, `lower/{types,consts,func}.fib` | `Lx` record; `lower-expr : Lx Expr -> i64` | S1, 0A |
| L2 lowering instructions | `native/lower/{expr,arith,calls,memory}.fib` | `lower-bin`, `lower-call`, `lower-load` taking LLVM values | S1, 0A; tests on hand-built ASTs until R lands |
| C call support | `native/call.fib`, `native/support.fib` (lIR text) | `call-i64`, `call-f64`, the mailbox functions as in `lair.call` | none beyond 0A: developed and tested through the existing `liblair.so` first (the module under test is JIT'd by the Rust lair), then through `native.jit` |
| J JIT | `native/jit.fib`, `jit/{names,prune,trampoline}.fib`, `native/api.fib` | the 12 functions of `lair.jit` | L1, K1 (for `check`), 0A (`llvm.orc`) |
| A AOT | `native/aot.fib`, `native/link.fib` | `emit`, `build-executable` | L1, 0A (target machine); `driver.proc` for `cc` |
| H harness | `native/cases.fib`, `cases/{header,verdict,exec}.fib`, `compiler/lairf.fib`, `native/cli.fib` | the CLI of `cli.rs` | J, A |
| O oracles | `crates/lair/src/dump.rs`, `scripts/lair-corpus.sh`, `compiler/tests/native/*.sh` | the dump grammar | none; first |
| F fuzzers | `native/fuzz*.fib` | the CLI | H (later) |

R, K1, K2, L1, L2, C and O run in parallel. J and A start when L1's signatures are stubbed and finish when it lands. H and F
are last. Two agents on K1 and K2 share `Fcx`; the skeleton fixes its fields so they can agree on paper (spec/bootstrap.md §5.3
did exactly this with `expand.types` and `expand.ctx`).

### 3.3 The seed chain

Today `SEED` names v0.1.4, whose tarball carries `bin/fibc` and `lib/liblair.so`; the seed's `fibc build` uses its own
`liblair.so`, and `scripts/package.sh` builds the output program against a liblair it builds from the tree
(`cargo build --release -p lair`, so a tree that calls entries the seed lacks still links; commit `c1a4710`).

1. The cutover commit (stage 7) is built by SEED v0.1.4. The output program no longer links liblair, so `cargo build -p lair` is
   not run for it; the seed supplies the code generator that compiles T, and the link line gives LLVM (4.2).
2. The release built that way (call it release P, for port) contains the port and no `liblair.so`. P is checked by the fixed point of 2.5.
3. `SEED` is then set to P's URL and sha256. From this commit the release is built by a seed with a port of its own, and no
   step of the build uses `liblair.so`: `scripts/package.sh` loses `LAIR_DIR`, `LLVM_SYS_211_PREFIX` for lair, and the `cc` shim
   keeps only the `$ORIGIN` rpath if any shared library is left (with a static LLVM none is).
4. Only then may `crates/lair` stop being built by anything. The Rust `crates/fibc` still depends on it (1.6), so the removal of the
   crate is stage 10, tied to the removal of the Rust `fibc`; until then CI keeps building and testing it as legacy (`ci.yml` job
   `lair`, `cargo test -p lair`, `cargo test -p fibc`), and no behaviour changes there.

An invariant to keep during the transition: P's compiler must be able to build T's compiler with `-O 0`..`3` and with
`FIB_TARGET_CPU=x86-64-v2`, because `package.sh` sets it and a CPU-baseline regression (the SIGILL of the CI runner build
mentioned in `package.sh`) would only show on another machine; step 4 of 2.5 and the `objdump` check for `ymm`/`zmm` already in
`package.sh` cover it.

### 3.4 Distribution, CI and retirement

Distribution. The release links LLVM statically into `fibc` (D1 below recommends it), so `lib/liblair.so` leaves the tarball and
the README's layout line, `bin/fibc` grows by about what `liblair.so` weighs. The measured baseline: `bin/fibc` 6.8 MB plus
`lib/liblair.so` 62.6 MB. The tarball keeps `share/fibber/lib/`, `LICENSE`, `bin/fibref`. The release must not need LLVM on the user's
machine; it still needs libc, libm, libstdc++ (if LLVM is static, libstdc++ is linked as a shared system library unless
`-static-libstdc++` is passed), libz, libzstd and `cc` for `fibc build`. `FIB_TARGET_CPU` stays (`native.target` reads it as
`target.rs:62-71` does: unset, empty or `host` mean the host CPU; `LLVMCreateTargetMachine` is given a CPU name and an empty
feature string), and `package.sh` keeps `x86-64-v2` as the default for releases.

CI. At stage 7: the `lair` job keeps running `cargo test -p lair` and `-p fibc` as legacy only until stage 10; the `stage2` job
loses its "build lair from the tree" steps (installing a Rust toolchain for one crate, `cargo build -p lair`) and gains LLVM's
static libraries (it already installs `llvm-21-dev`, `libpolly-21-dev`, `libzstd-dev`, `zlib1g-dev`) because it links them into
`F`. `scripts/ci-stage2.sh` loses the `LD_LIBRARY_PATH` of liblair. A new job step runs the five compare scripts over a
smaller corpus (the `cases/lir` files and 30 emitted programs) so the oracle is checked on every push while the Rust oracle
exists. At stage 10 the `lair` job and the Rust `fibc` jobs are removed.

Retirement. When the Rust `fibc` is removed: delete `crates/lair`, `crates/lir`, `compiler/lair/` (the stopgap bindings and
`lair.fibm`/`lair.expand` moved to `native.*` first), `crates/fibc/tests/capi/`, `crates/lair/tests/c/`, fix spec/compiler.md §9
(it describes 22 functions that are gone) and ROADMAP.md (the "21 `lair_*`" line and the lair-in-fibber line of M6); rename
`lairf` to `lair`; `SEED` is a release with the port (already true after step 3 above).

## 4. Risks and decisions for the owner

Each item has a recommendation. R is a risk; D is a decision only the owner can take.

**R1 / D1. LLVM linkage: static or shared.**
- Static: the release needs no LLVM on the user's machine, which the project requires. Precedent: `liblair.so` is 62.6 MB with LLVM
  inside and the tarball is 25.7 MB compressed, so a static `fibc` is about the same size (estimate: 62.6 + 6.8 MB uncompressed;
  0A measures the real figure and the startup time). The link line is one `-l` per component in the order of
  `llvm-config --libs --link-static core target x86 ...` plus the system libraries and `-lstdc++`, given as
  `-L /usr/lib/llvm-21/lib -l LLVMCore ... -l stdc++ -l z -l zstd`; `fibc build -L/-l` and the `lair_build_executable_with` linker
  line pass each `-l` in order but give no `--start-group`, so a circular dependency among the archives would not resolve. 0A states
  whether it does. If it does not, the fallback is a one-line addition to `aot/link.rs`'s argument list in `native.link` (the
  driver of the compiler itself is `native.link`, so this is our code, not Rust's).
- Shared (`-l LLVM-21`): the link line is trivial and the build is fast, but the user's machine needs `libLLVM-21.so`, or the
  tarball ships it (about 100 MB, with its own rpath), which is more than the static case.
- Recommendation: static, X86 only, as `liblair.so` is. Decide on 0A's numbers (binary size, startup time, whether the driver's
  flags suffice).

**D2. Keep `lair.h`'s ABI for third parties or not.** Fibber cannot export C functions, so the port cannot offer the ABI; keeping it means
keeping the Rust crate and its LLVM build forever. I found no user outside the repository's own tests. Recommendation: do not keep
it. Keep `liblair.so` and its tests as legacy until the Rust `fibc` goes (1.6), say so in spec/compiler.md §9 at stage 7, and
let third parties use the `lair` and `fibc` command lines (`lair check|run|build|emit-llvm`). Owner: confirm that no one outside the
tree uses `liblair.so`.

**D3. The function-pointer primitive** (1.5). Recommendation: (a) now, the libc shim, no Rust edit; decide on (b) or (c) after
stage 4 has measured the cost of a call (a `call-i64` is one `pthread_once` and a handful of stores; the mailbox protocol is one
call per `fibm` accessor, thousands per macro).

**R3. Performance of an interpreter-level port of the checker.** Not an interpreter: stage 2 is compiled native code, and the port
is ordinary fibber compiled by LLVM. Measured for Rust on the compiler's own 19.1 MB: parse and check 0.74 s, 721 MB; lowering,
verifying and printing the `.ll` another 0.45 s (`lair check` 0.74 s against `lair emit-llvm` 1.19 s). The ROADMAP's ratios for
stage 2 against Rust are 2.4x to 3.7x for the compiler's own passes (`emit` 37.5 s against 15.9 s; `hello` 0.42 s against 0.115 s) and
"7x to 330x slower" for code that is bound by collections. Estimate for the checker and reader: 4x (0.74 to 3 s), 10x (7.4 s), 30x (22 s).
The budget: the current `lair` share of building the compiler is 25.8 s, almost all LLVM `-O2` (ROADMAP line 557: "-O2 26 s, -O1
22 s, -O0 about 2 s"), so a front half under 10 s (13x) is under 40% added to that and invisible next to LLVM at `-O2`; and
under `-O 0`, which the compiler uses for macro and `def` sessions, it would dominate, so it must also be small per session.
Where it can be slow, in order: (1) the lexer, which handles 19 MB per build and must not allocate per token (1.4); (2) the
per-instruction `Map` operations of `Fcx` (a `Map str Type` keyed by SSA name, one `assoc` per instruction; use the in-place
path, and consider interning names to integers in the reader if the profile shows string hashing); (3) message construction in
the checker, which only runs on failure and costs nothing on the accepted path; (4) 250 LLVM-C calls are direct C calls through
`extern`, so lowering pays marshalling (a C string per name): `NONAME` is one static string and instruction names are empty as
in `llvm/mod.rs:24`, which keeps that cost off the hot path. Recommendation: accept the port at stage 2 only if
`compare-check.sh` on the 19.1 MB file runs in under 10 s and 3 GB (R4); otherwise profile with `scripts/bench/tools/prof.sh`
before stage 3.

**R4. Memory of the AST at 17 to 19 MB of text.** Measured for Rust: 721 MB peak at 19.1 MB (`/usr/bin/time -v`), about 38 bytes of
resident memory per source byte. Fibber objects carry a header and a count, and spec/bootstrap.md §4 measured the compiled Stx
reader at about 113 bytes of resident memory per source byte for token-dense input (906,188 KB at 8 MiB of `(a 1)`). Estimate for
a faithful whole-module port: 1.5 to 3.5 GB for the compiler's own lIR, plus LLVM's own memory (about the Rust's 0.45 s of
lowering). The machine's ulimit for tests is 16 GB so it runs, but it is the largest risk to a clean first version. Mitigations,
in order of cost: (1) the packed `Pos` and payload-free operator enums of 1.3 (done by design); (2) interning of atoms (local
names and labels repeat thousands of times per function) in the lexer; (3) streaming by item, which keeps the diagnostics the
same: pass 1 reads and parses each top-level form, adds its header to `Env`, and keeps only the form's byte span; if a parse error
occurs it is remembered and reading continues without parsing, so that a lexical error later in the file, which Rust reports in
preference (`lir::parse` reads all forms first), still wins; pass 2 re-parses each item from its span with line and column
offsets, checks it, and lowers it, so the AST of one function is live at a time (the checker is per item,
`check/mod.rs:28-46`, and lowering declares every item first and then lowers bodies, `lower/mod.rs:110-131`, so the LLVM text keeps
its order). Peak memory becomes the `Env` plus one function. The API (`native.api`, text in) does not change, which is why
version 1 can be the simple one. Recommendation: build the faithful version, measure `lairf check` on the 19.1 MB file with
`/usr/bin/time -v`, and take the streaming change in stage 2 or 3 if the peak is above 2 GB; the arena is the last resort.

**R5. Compile time of the port itself.** About 7,200 lines on 36,563 (20%) is about 20% more of `fibc emit` (37.5 s on the
ROADMAP's machine today) and of LLVM `-O2` (26 s): about +7 s and +5 s per build of the compiler, roughly 12 s of 56 s. The
bindings of 0A add to this. Mitigation: the per-pass files stay small so the library cache ideas in the ROADMAP
(`library checked once`) apply; the port is not on the path of `hello`, since a user's program is compiled by `fibc` and not by the
JIT of the port, except for macro and `def` sessions, which are small modules.

**R6. Byte-for-byte identity of the lowering.** The oracle is `LLVMPrintModuleToString`, so the port must make the same LLVM-C
calls in the same order (declaration order, attribute order, alignment, which `LLVMBuild*` and flags), and the object bytes depend
on the passes, target machine settings and LLVM's own determinism. A function missing from llvm-sys that Rust declares by hand
(`LLVMOrcCreateNewThreadSafeContextFromLLVMContext`, `jit/mod.rs:36-40`) is declared in `compiler/llvm/orc` the same way. Mitigation:
`compare-llvm.sh` over the corpus at stage 3 before anything uses the lowering; a diff there is cheaper than a diff in an
executable. Recommendation: treat any divergence as the port's bug, never normalise either side.

**R7. Process-wide state and threads.** `llvm/target.rs` has a `Once` for target registry initialisation, and the mailbox is a
thread with a 64 MiB stack. Both are handled in 1.1 item 4 and 1.5; the residual risk is that the fibber runtime's own threads
(`rt/thread.lir`, pthreads) and the macro worker share the process with a JIT whose modules are not thread-safe: the existing rule
(compiler.md §9: compiler thread and worker never run at once) carries over unchanged and the port does not add threads of its
own (ORC's default concurrent compilation is off in the C API defaults `lair` uses).

**R8. `extern` limits for the 250 LLVM-C functions.** 0A reports them and its report supersedes this paragraph; my expectation from
the sources I read: nullable pointer parameters (the filter function and context of
`LLVMOrcCreateDynamicLibrarySearchGeneratorForProcess`, `jit/mod.rs:90-95`) are declared `i64` and passed 0 (the `lair.ffi`
convention); `LLVMBool` is `i32`; enums (`LLVMLinkage`, `LLVMVisibility`, `LLVMCodeGenOptLevel`, `LLVMRelocMode`,
`LLVMCodeModel`, `LLVMCodeGenFileType`) are `i32` constants named in a binding module; out-parameters are 8-byte slots read with
`load-i64`; no LLVM-C function used here takes a struct by value or a callback that must be fibber. If 0A finds otherwise, stage 3
and 4 move.

**D4. Spec and CLAUDE.md disagreement found.** spec/compiler.md §9 says the C interface is 22 functions and ROADMAP.md 21; the header
and the library have 23. I did not change either (CLAUDE.md: when a spec rule and code disagree, report). Recommendation: the
owner approves a one-line correction of both at stage 7, when the section is rewritten anyway.

**D5. Naming** (`compiler/lir/`, `compiler/native/`, `lairf`). Recommendation as in 1.1; the owner may prefer `lair2` or other names;
the cost of changing it is a rename before stage 1 starts and a nuisance after.

**D6. Streaming from day one or not** (R4). Recommendation: no; measure first, as the owner's rule for performance says.

## 5. What is not decided here

The LLVM binding names and the link line (0A); whether `str` may hold non-UTF-8 bytes (S1, by a case); whether nullary variants
are shared constants (S1, by a case); the dump grammar to the last character (stage 0); the list of checker rules that need the
dispatcher field (K2, before writing them); the size of the corpus beyond the counts of 2.3; whether the fuzzers are ported at all
(stage 8). Each has an owner in the package table of 3.2, and none changes the architecture above.
