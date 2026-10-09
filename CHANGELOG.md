# Changelog

## Unreleased documentation corrections

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
