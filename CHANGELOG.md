# Changelog

## 0.1.21

- The respelling, step 4 (spec/lir.md 2): every lIR text fibber owns is in the new spelling: the runtime (`rt/*.lir`), the lIR the
  emitter, the live server and the live loader write by hand, the inline `lir` bodies in `lib/` and the cases, the planted faults of the
  scripts, and the lIR quoted in the specs, ADRs and design documents. Every `fibc` and `lairf` the build runs has `FIB_LIR_SPELLING=new`,
  so the old spelling is refused everywhere but one place: the seed, 0.1.20, embeds its runtime in the old spelling and is run with
  `FIB_LIR_SPELLING=old` (`BUILDER_ENV` in mk/config.mk) until a seed built from this tree replaces it. lair keeps `--old-spelling` for
  that seed; it goes once the seed reads the new spelling.
- lIR (the pinned copy moves from lir b3d6ad39 to 714c626f): symbolic ids, `(ORDINAL T SET "NAME")` and `(ROW TABLE SET "NAME" T VALUE)`
  (spec/lir.md 4.7.1), numbered and joined by the program loader, so a library compiled once can name a type id or a table row without a
  program-wide pass; the ownership verifier hands a tail call's result to the caller instead of reporting a leak, verifies every function
  that holds a managed value (one whose only managed value was a call's result was skipped), and consumes a call's owned positions before it
  borrows the others (rule 2); `lairf respell` keeps the arguments of a macro use and skips text it cannot lex; the runtime C function table
  holds the 15 functions it lacked, with the 32-bit narrowing of the ones that take or return a word.
- `scripts/sync-lir.sh` regenerates docs/reference after a sync (the copied files move the lines the pages cite).

## 0.1.20

- lIR has compile-time macros (spec/lir.md 4.6): `(define-macro NAME (P..) block+)` is a function from form objects to a form, written
  in lIR, compiled into the module's own JIT session when it is read and run between reading and parsing; `(require-macros "FILE")`
  loads a file of them. The form objects are `lir.sexp.*` (symbol, integer, float, string, list, each with its file, line, column and
  byte span; `lir.sexp.at` copies a position onto a made form, `lir.sexp.set` is the dual of nth, `lir.sexp.expand` expands a form from
  inside a macro, `lir.sexp.gensym` counts per program read). A top-level `begin` is a splice of items, so one use can define several.
  `(module-macro ..)` runs at the end of the module over the forms it collected, `(macro-import "FILE")` puts a runtime module into the
  macro session as its state, and `(include "FILE")` reads a file's text in place. A module with no macros reads as before.
- `(import "FILE")` (spec/lir.md 4.7): a program is the union of its modules, joined as llvm-link joins them. The importer sees the
  imported module's exported functions, globals, structs and the macros of its own file; each module is loaded once per program, in
  dependency order, identified by its canonical path, and reads under its own readtable.
- Reader macros (spec/lir.md 4.8): `define-reader-macro` binds a character of the module's readtable to an lIR function run at read time
  over a stream (`lir.read.peek`, `next`, `position`, `skip`, `form`, `fail`), `define-dispatch-macro` binds the character after `#`, and
  `define-token-reader` is called after every atom with the atom's text as a symbol; `define-reader-class` sets a character's class and
  the nesting depth is the readtable's (1024 unless set). Forms carry byte spans; a byte order mark is skipped. The bindings a required
  library makes are the requiring module's. A module with no bindings reads as before.
- Ownership in lIR (spec/lir.md 6.14): the managed pointer type, owned parameters, `unsafe`, `mutable` and the ownership form are read,
  typed and printed, and lair verifies them (one dataflow per function, the ten rules of 6.14.5) on every path it compiles:
  `FIB_LIR_OWN=verify|report|off`, `report` the default (one summary line on stderr, the program accepted), `verify` makes a violation an
  error. The forms are erased after the optimisation slot, before LLVM and the C, JS and WGSL printers. A program has one ownership form,
  carried in the interface of the module that holds it.
- `noinline` and `cold`, LLVM's function attributes, on a `define`; `defun` takes `:noinline` and `:cold` after its parameters
  (spec/syntax.md 3.1). lair no longer makes a function noinline for a name ending `-slow`: an attribute is written, not inferred from a
  name.
- `(dependent-library "NAME")`, LLVM's `!llvm.dependent-libraries`, is a module item and replaces the `;; link-lib:` comment and the libs
  argument on every path (AOT, liro, the JIT and the live JIT, the C route, shared libraries). fibber's emitter writes it for each
  `:lib`; `:lib` in fibber source is unchanged.
- `(export-name "NAME")` is a modifier of a define, LLVM's `wasm-export-name`: the WebAssembly backend exports the symbol with no
  `--export` flag. Library versus executable is `lair build --library`; a shared library exports what the lIR made external, with no
  symbol list. lair reads `FIB_EXPORTS` nowhere: the variable is now only `fibc`'s own channel from `--export` to its front end.
- `(align N)` means N on a kernel target too (spec/lir.md 6.9a); fibber's emitter writes a kernel's scalar alignment itself.
- Each `(string ..)` occurrence is its own constant, as spec/lir.md 3 says: lair no longer marks them `unnamed_addr`, so LLVM cannot merge
  two equal ones into one address. The emitter interns a trap's message as one private constant per text (`cstr.N`): the compiler's own
  lIR has 10 of them where it had 11,804 `(string ..)` occurrences, and the binary grows by 752 bytes, not 118 KB.
- The respelling (spec/lir.md 2): every word lIR owns is upper case (`ADD`, `FUNCTION`, `DEFINE`), a reference or a type is a form
  (`(GLOBAL f)`, `(STRUCT S)`, `(PTR (ADDRSPACE 1))`, `(FUNCTION ..)`, `(VECTOR N T)`, `(ARRAY N T)`, `(LITERAL-STRUCT ..)`), the lower
  case is free for the language above, and a name that is a word is refused. The parser reads both spellings; the printer writes the new
  one. The old spelling (`add`, `@f`, `%struct.s`, `(ptr 1)`, `(fn ..)`, `<N x T>`, `[N x T]`) is admitted for this release only: `lairf`
  refuses it by default and admits it with `--old-spelling` or `FIB_LIR_SPELLING=old` (`--new-spelling`, `FIB_LIR_SPELLING=new` refuse
  it); `fibc` admits it. `lairf respell FILE [--in-place|--check]` rewrites a text into the new spelling with its comments and layout
  kept, `lairf words` prints the table of words, and `lairf expand FILE` prints a module with its macros expanded, for a backend with
  no compile-time runtime.
- lIR and lair live in their own repository, https://github.com/tailoredshapes/lir, with lIR's cases (`cases/lir`), tests, tools (lir2c,
  lir2js, the JS backend) and designs (lir-language.md, lir-facts.md, lair-in-fibber.md and the rest). fibc compiles lair from source,
  so fibber keeps a pinned, read-only copy of `compiler/lir`, `compiler/native`, `compiler/llvm`, `compiler/c`, `compiler/lir.fib`,
  `compiler/lairf.fib` and `spec/lir.md`: the file `LIR` names the repository, the commit and the sha256 of the copied files' manifest,
  `scripts/sync-lir.sh [--repo DIR] [COMMIT]` updates the copy, and the quick gate fails when the copy differs from the pin
  (`scripts/lir-drift.sh`). A change to lIR or lair starts in that repository.
- lIR facts in LLVM's own forms (spec/lir.md 6.13): `(add nsw a b)` and `nuw` on add, sub, mul and shl, `(assume c)` and
  `(load (range LO HI) T p)`. `FIB_LIR_FACTS=use|trap|ignore`: `use` (the default) gives them to LLVM, `trap` turns each into a test that
  traps with `lIR fact violated: ..`, `ignore` drops them. The emitter proves the first ones (the Vec tail read, bounded steps), and
  `make cases-trap` runs ownership and the stdlib under `trap` in the full gate.
- `FIB_LIRO_THIN=1`: the partitioned build's units are ThinLTO bitcode with summaries, thin-linked inside `fibc` through a C++ shim over
  LLVM's LTO (Linux and macOS; no LLVMgold.so or linker plugin). The backend cache keys on the unit after importing, so an edit reruns
  only the backends that import what changed. Opt-in; the default build is unchanged. `FIB_LIRO_THIN_JOBS`, `FIB_LIRO_THIN_IMPORT` and
  `FIB_LIRO_THIN_REPORT=FILE` tune and report it (docs/design/compile-performance.md).
- Performance (docs/design/vec-performance.md 8 and 9): a Vec built whole, or by `conj` from empty while unique, is flat (every element
  in its tail), so `nth` is a load and a unique update is in place; a shared update makes the trie once. `vec-nth`, `vec-assoc` and
  `vec-conj` are small fast paths with the trie work out of line, and the Vec read path is emitted inline. `reduce` and `into` over a Vec
  or an Array keep the accumulator in a loop variable, so a unique one is updated in place. A literal or named function passed to a defun
  that calls it, or bound by `let` and only called, is a direct call; stack and immortal closures called directly skip their counts; a
  let-bound literal captured only by stack closures stays on the stack. Fused chains over a range with a literal step, or over a vector
  literal of fn stages, have no fallback branch; a match the checker proved exhaustive falls through to `unreachable`.
- Fixes: the operand of `private-copy` escapes, so a closure it copies is a heap closure (it read a dead frame); the live server answers
  a buffered request line without waiting for more bytes (a client that pipelined two requests hung); `lairf cases DIR.. -j N` reads
  its option instead of walking `-j` as a path.
- Tests and CI: the live test op's status check measures the run, not the entry's compile; the host-threads, live leak and shared
  stackcheck tests run on macOS (a mutex and condvar barrier, `ps` for the resident size, `nm -gU` for Mach-O exports, `LLVM_LIBDIR` for
  the live-mode build); the Linux CI runs the tools with `TOOLS_TIMEOUT=1800`, as the Mac does; case 8641 counts the tasks the workers
  ran instead of assuming every task runs on one; the stdlib cases that wrote fixed paths under /tmp name their files by the process id,
  so two gates at once no longer collide.

## 0.1.19

- Fusion covers more chains: a `map` over a `range` call is a counted loop over the range with the stage in line, and
  chains of `filter`, `remove`, `keep`, `take`, `take-while` and `filterv` (over a Vec, an Array or a range, with or
  without `map` stages) are one loop that builds the result directly, with no intermediate sequence.
- `:lib` on macOS: `fibc run` finds a library in DYLD_FALLBACK_LIBRARY_PATH (else /usr/local/lib and /usr/lib) and in
  `$HOMEBREW_PREFIX/lib`, instead of a Homebrew path compiled into `fibc`; when a library is not found and
  HOMEBREW_PREFIX is unset (ssh commands, launchd), the message says so. The darwin `fibc` again names no
  build-machine path. `scripts/mac-check.sh` step 8 checks it.

## 0.1.18

- Persistent collections keep their uniqueness, so idiomatic updates are in place: a call that reads a value in a sibling
  argument and moves it into another is a two-phase move (`(assoc v i (+ (nth v i) 1))`), `assoc` takes its new value
  owned so `update`, `update-or` and `update-opt` update a unique collection in place, and `with` takes a field whose new
  value reads the old one (`(with e (f (assoc (. e f) i x)))`). Each was a copy of the trie path per update (370 to 420 ns
  on a 1M-element Vec) and is now 20 to 60 ns; cases count heap objects so a copy that comes back fails (types §6.3, §6.4).
- Fusion writes the stage in line: `mapv`, `vec`/`into []` over `map`, chains of `map` and two-source `map` over a Vec or
  Array, with a fn literal or a named function, are counted loops that LLVM auto-vectorises (f64 `mapv` 13 ns to 1.4 ns an
  element; NEON on Apple Silicon). `first` is a terminal of the fused table.
- A stack overflow is a catchable failure in a program that catches and in a shared library (ADR 0027, reversing ADR 0009's
  rule): functions on call cycles check the stack in their prologue; `try` catches it with kind "stack"; library exports set
  `fib_status`/`fib_error`. Programs that never catch keep the signal path at no cost. `FIB_STACK_CHECK=0` turns it off.
- `fibc build --shared --header`: shared libraries with a C ABI, a C header and a module map; a library and a live session are
  multi-threaded from the start.
- `fibc run` (and `repl`, `run --live`, `serve`, `test`) loads the native libraries a program declares with `:lib`; where only
  the versioned file is installed (`libwebp.so.7`), `run` and `build` find it; a library found nowhere is an error naming
  what was searched.
- Library: `*command-line-args*`; the `Math/` statics Clojure programs write (`exp`, `log10`, trig and hyperbolic functions,
  `atan2`, `hypot`, `cbrt`, `PI`, `E`, ...) and `math/pi`, `math/e`; `(MArray t)` with `long-array`, `make-array`, `aget`,
  `aset`, `alength`, `aclone`, `amap`, `areduce`, `to-array`, `into-array` (§2.11). `to-array` of an empty source traps for
  now (case 8847, open).
- Inline lIR in `unsafe`, `fib.atomic`, `(spin-hint)`, `(cache-line)`, the `inline` modifier (ADR 0026); `fibc explain unsafe`.
- Live sessions forget retired texts (a long session's growth fell from about 8 MB a swap to under 2 MB over 100 swaps),
  accept removals on `load`, and build their checked base once (start to first result 7.8 s to 4.2 s on fib-solid).
- Fixes: `(array 0 x)` released its counted fill value; a JIT-compiled thread-key destructor ran after its code was freed.

## 0.1.17

- Live development: `fibc run --live FILE` runs a program whose functions can be redefined while it runs, and `fibc repl --connect PID`
  attaches to it (socket `$XDG_RUNTIME_DIR/fib-live/PID.sock`, JSON lines). `eval` redefines a function or a `def`; `load FILE` and
  `reload` swap what an edited file changed (detected by expanded form); `test` runs tests inside the process (`fibc test --live`,
  `fibc test --connect`); `watch` does it on save; `inspect` and `type` answer from the session's tables; `pause`/`resume`/`retire` free
  old generations at quiescence. A swap takes 0.3 to 0.6 s on a 300-module application (a rebuild and restart: 8 to 10 s) and is refused
  (`live/*` codes) when it would break the checker or ownership. Redefinable `def` is live-only; ahead-of-time builds keep immutable
  defs. A `def` may change type when its readers still check, and multi-word values are redefinable. Design: `docs/design/live-repl.md`,
  `live-incremental.md`, `live-meshql.md`.
- Retirement follows indirect calls (function values, `dyn` methods, library callbacks) to every generation a parked thread may return
  into, fixing a use-after-free of retired code.
- `fib.os.live` and the project rule for live modules (`FIB_LIVE_ROOTS`, `--live-deps`); `FIB_LIVE_EMIT=verify`, `FIB_LIVE_DEBUG=cut`.
- Gate: the live suites (`sh-live-*` stamps) and their mutants (`scripts/mutant-live.sh`).

## 0.1.16

- The type and ownership checker keeps the checked library between programs: `emit --batch`,
  `types --batch`, `fibc serve` and `fibc repl` check the library once and reuse it (a warm server
  check of an edited program 165 ms to 28 ms; REPL input 188 ms to 87 ms). One-shot commands are
  unchanged. `FIB_CHECK_CACHE=off|on|verify` (`verify` checks every program both ways and fails on
  any difference). Design and controls: `docs/design/persistent-checker.md`.
- `fibc run --trace` writes its heap events through a 64 KiB buffer, flushed at exit and before any
  trap or abort, instead of one `write` per event (the stdlib cases run in 144 s instead of 222 s).
- `FIB_TIME=1` prints the phases of the front end (expand, lower, infer, elaborate, own).
- Gate: the persistent-checker controls, `lsp-hardening` and the unit runner are faster and
  parallel; profile in `docs/design/gate-profile.md`.

## 0.1.15

- Compilation: a cache-off build can partition into units and compile them in parallel
  (`FIB_LIRO_PARTITION=1`, `FIB_LIRO_JOBS=N`, `FIB_LIRO_IMPORT=N` copies callees of at most N
  nodes into each unit); the cold self-build of the compiler drops from about 101 s to 35 s with
  byte-identical output at 1 and 8 jobs. Default builds and releases are unchanged (whole-program).
  Measurements and the open questions: `docs/design/compile-performance.md`.
- `expand --batch` expands many files in one process from a snapshot of the library context
  (the compiler's golden checks use it: 944 inputs in 16 s instead of 123 s).
- The gate prints a budget report (wall, parallelism, critical path) and warns on regressions
  against `scripts/gate-budget.tsv` (`docs/design/build.md`, "The gate budget").
- Documentation: `docs/design/test-pyramid.md`; `fibc repl` in the tooling guide.

## 0.1.14

- Scheduler: no spinner cap on Linux (2 on Darwin), a hint word for spinning workers,
  `FIB_SPINNERS` and `FIB_SPIN_HINT`; the thread count is released before its task
  completes (fixes a rare early-exit race). Measurements: `docs/shootout/sched-ryzen.md`.
- Compilation: the emitter builds structured lIR with no text intermediary (ADR 0025);
  compiled-lIR objects are cached and linked per unit (ADR 0024, `--no-cache`,
  `--whole-program`, `--time`, `--cache-stats`); release and bootstrap builds stay whole-program.
- Unread run-time definitions and the unreachable runtime are pruned from emitted lIR
  (`FIB_PRUNE=0` disables); an initialiser that can trap is kept.
- GPU: kernel atomics, block and warp reductions, structured control flow in WGSL,
  warp shuffles (PTX `shfl.sync`, WGSL subgroups), f32 atomics on WebGPU, pinned host
  memory and asynchronous copies (`Pinned`, `Transfer`).
- C backend: no `musttail` with an argument over 16 bytes under clang on AArch64.
- Documentation: a documentation site, the Hard Way course, checked examples, policy
  pages; `fibc repl` is documented in the tooling guide.

## Documentation corrections

The documentation now distinguishes the shipped 0.1.13 surface, historical designs
and proposed 1.0 promises. No 1.0 stability commitment has been made.

## Since 0.1.0 (summary through 0.1.13)

This is a summary of the development line, not a claim that each item first shipped
in 0.1.13. Exact changes and release dates are recorded in git tags and release notes.

Recent compiler and library changes include:

- A work-stealing pool: `(fork-task f)` makes a pool task (`join` as any
  task's), with a Chase-Lev deque per worker, help-while-waiting joins,
  parking on an eventcount and workers started lazily on the first fork
  (`FIB_THREADS`, else the affinity mask and the cgroup quota);
  `fib.parallel` (`fork`, `pmap`, `pfor`, `preduce`, `pscan`) runs on it, so
  nested parallel calls fork tasks, not threads; `spawn` stays the thread
  tier and `async` the stackless one. A trap in a pool task is the task's
  failure. See [the parallelism design](docs/design/parallelism.md), 3.1 and 8.
- Scalar `Option` values stored inline as a tag and payload, including in
  collection elements, closure captures and task frames. Options of ordinary
  objects remain nullable pointers; nested options and other payloads retain
  their boxed representation. See [the representation design](docs/design/unboxed-option.md).
- Last-use moves, moves of fields out of dead owned objects, and reuse of
  unique collection shells and arrays. Persistent updates preserve old
  versions when another holder exists. See [the update rules](spec/stdlib.md#25-mutation-and-uniqueness).
- Fusion of a sequence bound by `let` when it has one eligible consumer,
  with tests for effect order and preservation of memoization when reused.
- SIMD lane values, arithmetic, masks, reductions and target-dependent widths
  through `fib.simd`, with native vector lowering. See [the SIMD measurements](docs/shootout/simd.md).
- FastISel for `fibc run -O 0`, plus recorded development-loop timings.
  The compiler server, incremental checking and session work are described in
  [the development-loop design](docs/design/dev-loop.md); that design is not
  a claim that every planned command exists.
- Exclusive views: `with-view` lends a writable window over an `Array` that the
  checker proves nothing else can reach, so a loop writes in place with no
  copy and no per-element uniqueness test; `with-tiles` hands disjoint windows
  to tasks. Windows run at about the speed of the array loop at `-O 2`.
  See [the design](docs/design/exclusive-views.md).
- aarch64: `--target TRIPLE` (or `FIB_TARGET_TRIPLE`) emits objects and
  assembly for Linux, macOS and iOS triples. On an Apple M1 Ultra the cross-built
  compiler runs, builds itself, and reaches the same fixed point as on x86;
  `scripts/package.sh` builds a macOS tarball. Darwin arm64 releases have been published since 0.1.7.
  iOS is cross-emission only; native aarch64 Linux checks are available through `make k8s-gate K8S_ARCH=arm64`. See
  [the aarch64 design](docs/design/aarch64.md) and [the first numbers](docs/shootout/aarch64.md).
- GraphQL lives in its own repository, lacewing: `lacewing (external repository; use an accessible git URL)` (latest tag `v0.3.0`), pulled in with
  `fibc deps add` (see Projects and dependencies). It is a GraphQL query engine in the shape of Clojure's Lacinia (compiled immutable schema, resolvers as plain
  functions, ordered responses): typed schema construction, schema definition language (SDL) loading with resolvers attached by name, custom scalars, variables,
  input objects, fragments, `@skip` and `@include`, whole-document validation before any resolver runs, and resolvers that see the selections beneath their field.
  **Breaking since `v0.2.0`: `GqlField.resolver` is an `(Option GqlResolver)`** (SDL fields have none until one is attached). `v0.3.0` can resolve a query's fields in
  parallel on native tasks (opt-in, `execute-with`), with the serial response. Not yet: mutations, subscriptions, introspection beyond `__typename`, enums,
  interfaces, unions.
- meshql in fibber: `meshql-fib` (`meshql-fib (external repository; use an accessible git URL)`, `v0.2.0`) gives an entity a REST write surface and a GraphQL read surface
  (on lacewing) from one declaration, with point-in-time reads and federation of graphlettes. It is the fourth implementation of the
  [meshql](https://git.tildarc.com/tailoredshapes/meshql) contract and passes all five tiers (37 scenarios) against SQLite (`fib-db-sqlite`) and PostgreSQL
  (`fib-db-postgres`); `meshql serve CONFIG` reads the HOCON configuration of the other implementations (`fib-hocon`).
- HOCON configuration lives in its own repository, fib-hocon: `fib-hocon (external repository; use an accessible git URL)` (tag `v0.1.0`), module
  `hocon`. It has the parser, substitutions, includes, merging, durations and sizes, a `config->record` macro, and a differential harness
  against Typesafe Config; its README has the features and the measurements.
- Databases: `fib.db` (next.jdbc's shape: `execute!`, `with-transaction`, `with-connection`, `plan`), the driver contract
  `fib.db.contract` and an in-memory fake driver `fib.db.memory` are in the library; drivers are libraries in their own repositories. The
  SQLite driver is `fib-db-sqlite`: `fib-db-sqlite (external repository; use an accessible git URL)`, module `sqlite`. The PostgreSQL driver is
  `fib-db-postgres`: `fib-db-postgres (external repository; use an accessible git URL)` (tag `v0.1.0`), module `postgres`: the wire protocol (version 3.0) written in
  fibber over `fib.os.net`, with no libpq and no C library of its own; it passes `fib.db.contract` against PostgreSQL 14, 16 and 17 and opens a connection from
  a libpq-shaped URL (`pg/open-url`). It cannot yet connect over TLS (`sslmode=require` is refused: `fib.tls`'s `Session` is not `Send`, so it cannot live in a
  connection that moves between tasks; see [the Send-able TLS session design](docs/design/tls-send.md)). See [the contract](docs/design/db-contract.md).
- Gherkin: `fib-gherkin` (`fib-gherkin (external repository; use an accessible git URL)`, module `gherkin`) reads `.feature` files (Cucumber's
  parser, all its languages, pickles, Cucumber Expressions) and runs them on `fib.test` with step definitions written in fibber, so
  `fibc test` prints the scenarios under their Gherkin names and the `.feature` line that did not hold. It is a front end to the system in
  [the test harness design](docs/design/test-harness.md); the design's "plain-text front end: rejected for now" (section 2.4) is this
  library, built because a project asked for feature files.
- GPU kernels (core, GPU-2): `(defkernel name (params) body)` is a core form (spec/syntax.md 3.22); its body is the kernel subset
  (spec/types.md 2.17: scalars, pointers, scalar cells, arithmetic, loops, the `gpu/*` builtins: `gpu/global-id`, `gpu/barrier`,
  `gpu/shared` block-shared memory), checked at the source (a string, an allocation, a closure, a Simd value, a task, recursion, dyn dispatch
  are refused at their positions) and compiled for the host too (`fib.gpu/host-launch` runs a kernel over a grid on the CPU: `fibc run
  examples/gpu/gpu.fib -I examples/gpu`). `fibc build --kernel-target nvptx64-nvidia-cuda` builds the executable with the kernels' PTX
  embedded (`gpu/program-ptx`) and beside it as `OUT.ptx`, with the launch ABI (each kernel's signature in the PTX header); a program with a
  kernel built for a platform with no kernel target is refused (no CPU fallback). The host side is the `fib.gpu.device` protocols
  (`Platform Device Module Kernel Buffer Stream Event`), which a driver implements and a program uses without naming CUDA: `fib-gpu-cuda`
  (`fib-gpu-cuda (external repository; use an accessible git URL)`, tag `v0.1.0`) passes the device contract `fib.gpu.contract` (7 of 7
  scenarios, 3 faults caught). Measured there: fibber's shared-memory GEMM at 17.1 TFLOPS at n = 4096 (47% of cuBLAS, the CUDA C kernel's
  speed), bit for bit the CPU's; [the GPU design](docs/design/gpu.md) has the table and what phase 2 and 3 still owe.
- GPU atomics and reductions (GPU-3): `gpu/atomic-*` builtins (one `atomicrmw`/`cmpxchg` each: PTX `atom.*`, WGSL `atomic<u32>` buffers), `fib.gpu.atomic`, block reductions over shared memory
  (`fib.gpu.reduce`) and grid reductions (`fib.gpu.reduce-kernels`) whose integer results equal the CPU's and whose f32 per-block partials are the CPU's bit for bit, run on the
  RTX 4080 SUPER through PTX and through WebGPU by `compiler/tests/native/gpu-device.sh` ([the GPU design](docs/design/gpu.md) section 12; pinned/async transfers are not done).
- WebGPU (the second kernel backend, WEBGPU-1): `fibc build --target wgsl-unknown-webgpu --emit wgsl` prints the same kernels as WGSL
  compute shaders ([the WebGPU design](docs/design/webgpu.md): the mapping of the kernel subset to WGSL, what is refused by name (i64, f64,
  pointer arithmetic beyond an index, builtins WGSL lacks), the binding model, the trap flag). They run on the native host through
  `fib-gpu-webgpu` (`fib-gpu-webgpu (external repository; use an accessible git URL)`, tag `v0.0.1`, over wgpu-native) and from a fibber program
  built for wasm32 through the JavaScript glue `examples/webgpu/js/fib-webgpu.mjs` over `navigator.gpu` (node with Dawn, Chromium). One
  `defkernel`, five runs: `scripts/webgpu-agree.sh` compares the CPU, CUDA, wgpu-native, Dawn and Chromium results by hash (bit exact where
  the host fuses `fma`; the register-tiled GEMM reaches 9.6 TFLOPS through WGSL on the same GPU where CUDA gives 10.9).

The standard library in `lib/` follows Clojure's names and argument shapes,
within fibber's static types and ownership model. Its specification and
remaining work are in [spec/stdlib.md](spec/stdlib.md). The specifications,
case headers and [ROADMAP.md](ROADMAP.md) contain both historical records
and current rules; dated amendments identify changes.

The explicit [`fib.tensor` numerical library](lib/fib/tensor/README.md) adds
typed dense tensors, checked strided views, broadcasting, eager arithmetic,
fused `axpby`, ordered and axis reductions, boolean masks, and matrix multiplication.
Floating arithmetic uses native eight-lane `f32` and four-lane `f64` kernels;
matrix multiplication uses register tiles and reusable packed panels. Explicit `sum-fast`/`dot-fast`
permit reassociated reductions. After rebuilding stage 2, try
`./F run examples/tensor.fib`; see the
library README for API, safety contracts, and reproducible NumPy comparisons.
Fused `t/dense` layers apply bias and an activation as each output tile is
stored, and `t/softmax` and `t/layernorm` work row by row. On the recorded
single-thread runs, matrix multiplication is within about 1.15x of NumPy with
OpenBLAS, a float32 MLP forward pass is at parity with it, and softmax and
layernorm are faster than NumPy; reductions along the last axis are still
slower ([the comparison](docs/shootout/tensor.md)). This is a dense numerical
foundation, not NumPy feature or performance parity.

The explicit [`fib.logic` relational library](lib/fib/logic/README.md) adds
finite typed terms, persistent unification with occurs checking, fair sequential
search, and `fresh`/`conde` syntax. The same relation can infer missing values or
enumerate answers; try `./F run examples/logic.fib`. Parallel search remains
future work, with this engine serving as its tested reference.

Its [`fib.logic.fd` extension](lib/fib/logic/README.md) adds compact and sparse
finite integer domains, watched propagation for all-different and arithmetic
constraints, batched model setup, and smallest-domain-first search. The port of [tsmarsh/sudoku](examples/sudoku/solver.fib)
uses those constraints; build it with `./F build examples/sudoku.fib -I examples -I lib`.
The [finite-domain design note](docs/design/finite-domains-and-sudoku.md) records
the API, provenance, validation cases, and a comparison with the original
Clojure/core.logic implementation.

The [`fib.os` library](lib/fib/os/README.md) provides typed file and descriptor
operations, directories, TCP sockets, polling, clocks, environment variables,
process identity, executable discovery, system information, secure entropy, and
bounded native memory streams. Shared errors and selected platform backends
keep libc flags, layouts, and symbols out of application code. Linux is tested
natively; the Darwin backend ran on Apple Silicon during the aarch64 work (see
[its design](docs/design/aarch64.md)), but is not yet part of any automated
gate. See the [OS design record](docs/design/os.md).

The explicit [`fib.http` library](lib/fib/http/README.md) provides shared HTTP
messages, a Ring-style HTTP/1.1 server (a task per connection, graceful stop,
slowloris deadlines) and a Hato-style client (connection pool, redirects, typed
errors, deadlines), both native over a `Transport` seam: no libcurl, no library
to link, and they build `--static`. They include binary bodies, repeated
headers, streaming bodies, chunked transfer in both directions, keep-alive and
asynchronous requests. HTTPS uses the explicit `fib.tls` transport via `tls/https-options`.
TLS is client-only and **UNAUDITED**; without a registered TLS transport an
`https` URL is a typed error, never a downgrade. See [security and limits](docs/policy/limits.md). Build the
[server example](examples/http-server.fib) with `./F build examples/http-server.fib -I lib`
and the [client example](examples/http-client.fib) the same way. See the library
README for API contracts and [the design record](docs/design/http.md) for the architecture,
limits and what is deferred.
