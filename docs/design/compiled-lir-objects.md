# Compiled lIR objects

[ADR 0024](../adr/0024-compiled-lir-objects-are-reused-and-linked-into-one-executable.md) separates native compilation from linking. Ordinary Linux and macOS host builds partition the checked, specialized lIR, compile missing objects and link those objects into one executable. Typing, ownership checking, specialization and lIR emission still run on every build.

```sh
fibc build app.fib -o app
fibc build app.fib -o app --cache-stats --time
fibc build app.fib -o app --cache-dir /tmp/app-liro-cache
fibc build app.fib -o app --whole-program
fibc build app.fib -o app --no-cache
```

The default cache is `$XDG_CACHE_HOME/fibber/liro`, or `$HOME/.cache/fibber/liro`. `FIB_LIRO_CACHE` selects another directory; `off` bypasses the cache and retains whole-program compilation. `FIB_LIRO_MODE=whole` selects whole-program optimization with caching. `FIB_LIRO_STATS=1` and `FIB_TIME=1` enable the corresponding reports. Reports go to standard error. Cache-disabled compiler bootstrap, fixed-point and release builds compile the complete program through LLVM.

A warm build checks and fingerprints its units, validates the saved native objects, then invokes the platform linker. It performs no LLVM lowering, optimization or code generation for hits. The `compiled` counter increments immediately before an actual object-emitter invocation, independently of the miss counter.

## Explicit artifacts

```sh
fibc build app.fib --emit liro -o app.liro
fibc build app.liro -o app
```

The first command writes a native relocatable object and the required `app.liro.meta` sidecar. The second validates both and links them. Moving the pair together is supported. Optimization, target, CPU/features and backend identity must agree with the linking invocation. Native libraries declared by reachable externs are recorded in the sidecar; `-L` supplies their search directories when linking.

At the lIR boundary, `lair build FILE.lir --emit liro -o FILE.liro` also accepts a module without `main`. `lair link FILE.liro.. -o OUT` checks selected owners, calling conventions, signatures and named layouts before linking independently supplied artifacts. Its default optimization level is zero, as for other `lair` commands; use the same `-O` for emission and linking.

A standalone Fib artifact owns its runtime tables. Independently emitted Fib programs cannot be combined by assuming their numeric type and keyword IDs agree. Duplicate owners are rejected, and a standalone object referring to the Fib runtime without owning its program tables is refused. Separately supplied native lIR objects can use declared C interfaces and native libraries.

## Units and keys

Definitions have one owner: globals form the program-glue group, `fib.*` functions form the runtime group, generated drop/trace/share functions form the walker group, and other functions use sixteen stable name-hash groups. Each group receives its referenced declarations and the transitive named layouts it needs. Cross-group definitions have external, hidden visibility. Every group passes the lIR checker and LLVM verifier; calls between groups preserve the declared calling convention. There is no cross-group LLVM inlining.

The SHA-256 key includes canonical lIR and its interface, the compiler executable and loaded LLVM image identities, LLVM version, resolved triple/data layout, CPU/features, PIC relocation, optimization pipeline/level, and code-affecting settings. SHA-256 uses the platform tool (`sha256sum` on Linux, `/usr/bin/shasum` on macOS), with input supplied through a quoted file descriptor path. Compiler changes conservatively invalidate objects.

Type/keyword numbers and generated names remain in the fully specialized lIR. The front end emits them again before lookup. An allocation whose type number changes therefore changes its unit's input; unchanged independent units can still hit. Global initializer changes rebuild glue, and declaration/layout changes rebuild affected clients. This cache does not skip emission or establish a frontend-independent binary ABI for Fib libraries.

## Publication and validation

Each cache entry is a directory named by its content key, containing `unit.liro` and `unit.liro.meta`. The version-one JSON sidecar records the key, object digest and canonical input contract. That contract includes settings, the lIR interface, exported owners, binding policy and native-library requirements.

Writers create and seal a private directory before atomically renaming it into place. A per-key OS file lock serializes publication and releases automatically when a writer dies. Existing valid winners must agree on object bytes. A reader takes a private hard link before hashing the object, so replacement of a damaged entry cannot remove an object that another build has selected. Explicitly supplied objects are copied to private scratch before validation and linking; exported artifacts are copies, so editing them does not modify the cache.

An invalid internal entry is rebuilt from checked source. Missing sidecars, unknown contracts, altered objects or mismatching settings on supplied artifacts produce errors. Unpublished interrupted directories are ignored. This is a local cache; a checksum detects corruption and does not authenticate an untrusted producer. Version one has no eviction policy.

## Executable checks

`compiler/tests/native/liro/check.py` is registered as `sh-native-liro` in the full platform gates. It checks cold/warm backend counts, body and initializer edits, optimization/CPU/catch isolation, damaged entries, explicit artifact round trips, ABI/owner mismatches, native-library requirements, concurrent publishers, a killed writer, and correct execution after changing type/keyword order and layouts. Closure, dyn/bound dispatch and generic cases run with clean ownership traces.

Its source mutations disable lookup, omit a body from the key, omit optimization settings, disable ABI comparison and bypass object checksums. The corresponding behavioral checks must fail. Canonical `F emit` remains the compiler fixed-point comparison, with object lookup disabled while building F and F3.

## Self-build measurements

These are single local runs on 2026-10-11 with shared LLVM 21, default host CPU and `-O2`. The Linux host was an Intel Core i7-14700KF with 64 GiB RAM. The compiler was the stage-two compiler built from this implementation. These are observations, not medians or CI durations.

The repeatable script makes a private compiler/library copy and uses a fresh dedicated cache. It measures an unchanged warm build, changes the body of `driver.version/version-line`, restores it, then changes the return ABI of `native.timing/elapsed-ms` from i64 to i32. Finally it measures a cold whole-program build and runs both resulting compilers on the same `emit` workload, requiring identical lIR digests.

```sh
LLVMLIB=/usr/lib/llvm-21/lib python3 compiler/tests/native/liro/bench.py /path/to/F --out /tmp/fibber-liro-measurements
```

Use a fresh output directory and an otherwise idle host. On macOS, use `LLVMLIB=/opt/homebrew/opt/llvm@21/lib`. The script records commands, logs and JSON measurements in the output directory. Wall time comes from a monotonic clock around the command; peak RSS comes from `/usr/bin/time`. Reported backend counts and link times come from the compiler's own phase reports.

| Linux build | Wall seconds | Peak RSS KiB | Hits / objects compiled | Link milliseconds | Executable bytes |
|---|---:|---:|---:|---:|---:|
| Cold whole-program | 115.816 | 2,091,932 | 0 / 1 | 383 | 11,467,240 |
| Cold separate units | 62.637 | 1,278,168 | 0 / 19 | 517 | 7,325,976 |
| Unchanged warm separate units | 39.506 | 1,216,336 | 19 / 0 | 468 | 7,325,976 |
| Body edit | 39.331 | 1,258,988 | 17 / 2 | 553 | 7,325,976 |
| ABI edit | 56.469 | 1,306,020 | 1 / 18 | 478 | 7,330,136 |

The body edit reuses most units. This ABI edit affects many callers and generated bindings and invalidates almost the whole program. The cache still runs the front end and emits, partitions, checks and hashes lIR on every build; frontend caching and stable frontend-independent bindings remain separate work.

Running `emit compiler/fibc.fib -Icompiler -Ilib` took 33.458 seconds with the separate compiler and 26.381 seconds with the whole-program compiler. Their emitted lIR digests matched. This workload shows the runtime cost of reduced inlining. Bootstrap and release builds therefore keep whole-program compilation, and `--whole-program` remains available for applications whose runtime benefits from it.

The same script ran on an Apple M1 Ultra with 64 GiB RAM, macOS 27.0.1 and Homebrew LLVM 21.1.8. The compiler and cache were native AArch64, with one run per row:

| macOS build | Wall seconds | Peak RSS KiB | Hits / objects compiled | Link milliseconds | Executable bytes |
|---|---:|---:|---:|---:|---:|
| Cold whole-program | 112.149 | 3,557,424 | 0 / 1 | 150 | 10,530,792 |
| Cold separate units | 63.972 | 1,581,840 | 0 / 19 | 139 | 6,163,112 |
| Unchanged warm separate units | 48.644 | 1,265,840 | 19 / 0 | 113 | 6,163,112 |
| Body edit | 48.781 | 1,245,040 | 17 / 2 | 114 | 6,163,112 |
| ABI edit | 62.523 | 1,597,328 | 1 / 18 | 117 | 6,163,160 |

On that Mac, the same compiler `emit` workload took 45.114 seconds with the separate compiler and 39.183 seconds with the whole-program compiler, again with identical lIR digests. These observations support keeping whole-program optimization for shipped compiler executables while offering the faster build path for development.

Both full platform gates passed, including cache-disabled compiler fixed points and the native LIRO mutation suite. The Mac's existing regex ownership audit exceeded its 300-second limit under concurrent load, then passed alone and in its retried shard with the same limit; no timeout or expected-result exception was added. The native case suites retain their recorded open cases and platform exclusions. Linux exercised WebAssembly; static-musl checks were skipped without their toolchain, as were WebAssembly checks on the Mac.

Gate verification also exposed the existing global-atom audit exception: a final counted payload installed by main was held by an immortal root at exit. Normal shutdown now releases the final payloads after joining threads and quiescing the pool. Cases 1707 and 1714 require clean audits, including nested/aliased roots and a task-installed payload; the previous compiler fails 1714 with three leaks.
