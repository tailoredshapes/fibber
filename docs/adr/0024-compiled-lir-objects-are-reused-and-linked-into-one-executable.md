# 0024. Compiled lIR objects are reused and linked into one executable

Status: accepted
Date: 2026-10-11
Source: the owner's request for `fib -> lir -> liro -> executable`, following the compilation-time investigation on main at bbfe80ac.

## Context

Fibber ships a program as one executable. Today the compiler emits a whole program as lIR, lowers that module to LLVM, optimizes it, generates one native object, and links it.
The object is temporary: [native.link](../../compiler/native/link.fib) removes it after linking. Another build repeats that work.

The existing lIR boundary is suitable for separate compilation. Functions have explicit signatures and calling conventions; structs have explicit fields; functions and globals
can be declared without their definitions. [native.lower](../../compiler/native/lower.fib) maps those declarations and bodies directly into LLVM and verifies the result.
We can retain this representation and backend rather than introduce another language between lIR and machine code.

A local Linux x86-64 self-build on 2026-10-11, with shared LLVM 21 and the default `-O2`, took 106.20 seconds. Timing the LLVM calls recorded 39.554 seconds in `default<O2>`
and 37.654 seconds in object-code generation: about 73% of the elapsed build. A separate `F emit compiler/fibc.fib -I compiler -I lib` took 25.68 seconds; a separate
`lairf check` of its output took 2.12 seconds. Those separate commands are not an exact phase decomposition of the build. The emitted lIR was 33,491,110 bytes,
with 23,921 function definitions, including 8,248 generated `drop.*` helpers. These are a dated local baseline, not a CI duration or a promised speedup.

One executable does not require one compilation unit. Reusing native objects targets the measured backend cost while preserving the distribution model.

## Decision

Introduce a reusable compiled-lIR artifact, `.liro`, and make native compilation and executable linking distinct operations:

```text
.fib -> checked, specialized .lir -> cached .liro objects -> one executable
                                      LLVM backend          native linker
```

The first implementation keeps the front end's typing, ownership checking, generic specialization and lIR emission. It partitions checked lIR for native compilation and
reuses objects whose inputs have not changed. A warm object cache therefore saves LLVM lowering, optimization and code generation; it does not claim to save the front end.

### Artifact and compilation unit

A `.liro` contains an ordinary native relocatable object for its target (ELF or Mach-O for the initial Linux and macOS implementation). A required `.liro.meta` sidecar
contains a versioned manifest. The compiler validates the pair before supplying the native object to the existing linker. The finished program has no dependency on
`.liro` files or compilation tools; its declared native-library dependencies remain part of the link contract. LLVM bitcode is not the native-object payload.

The manifest records the unit's identity, content key, object digest, target and backend settings, imports and exports, ABI fingerprints, and runtime and initialization
requirements. Publish the pair in a temporary cache directory and atomically rename the completed directory into place. An interrupted writer never publishes a partial
entry; competing writers for the same key must agree on the validated result.

A unit is an lIR module with a stable group of definitions and all declarations and type layouts needed to verify and compile it. Keep closely related function
specializations, closures and helpers together, with a deterministic owner for each definition. Begin with coarse runtime, library and application groups and measure
finer partitions. Avoid one LLVM invocation for every function by default: thousands of tiny objects could replace optimization cost with compiler and linker overhead.

Calls between units use declared symbols. Definitions crossing a unit boundary receive linkable visibility; definitions used only within a unit can remain internal.
Each unit passes the lIR checker and LLVM verifier before object generation. A build-wide manifest check verifies that every required definition has exactly one selected
owner, that imports agree with exported ABIs, and that the selected units form a complete program. Native-linker diagnostics remain errors, but signature compatibility
is checked before invoking the linker.

### Identity and executable-wide data

Use the emitter's qualified function and type names as the starting point for stable identities. Preserve its representation sharing for unconstrained generic arguments
and its full-type keys for protocol-bound arguments ([emit.mono](../../compiler/emit/mono.fib)). A new specialization is a new input; it does not require recompiling
other specializations unless their bodies or dependency contracts changed.

Several identities are currently local to one emitted program:

- [emit.objects](../../compiler/emit/objects.fib) assigns numeric object type IDs in interning order; runtime headers and the dense `fib.types` and `fib.classes` tables use them.
- [emit.statics](../../compiler/emit/statics.fib) assigns keyword IDs and names such as `str.N`, `clo.N` and `vt.N` from collection order.
- Closure literals, generated helpers, quoted data and initializers also have owners and references that must survive partitioning.
- [emit.program](../../compiler/emit/program.fib) chooses a program-wide failure-return convention when the program catches traps. The same source function can therefore
  have a different native ABI depending on the program's catch mode.

The first implementation re-emits the complete checked program before selecting cached backend units. Numeric type and keyword IDs, generated symbol names, static initializers and catch conventions are explicit inputs in that emitted lIR. Changing an ordinal or a generated reference changes its unit's canonical text and key. A unit that is byte-for-byte identical at this boundary already contains the current program's bindings; external declarations prevent LLVM from folding an unseen global initializer or callee body.

Frontend-independent precompiled Fib libraries would instead need stable bindings rather than accidentally bind another program's ordinal. Give generated symbols stable owners and identities, and collect
executable-wide type tables, keyword tables, static object headers and ordered initialization into a small program-glue unit rebuilt when their inputs change. Emit the
runtime once and preserve initialization order and the uniqueness of shared values across units.

The representation connecting stable type/keyword identities to executable-local numeric IDs must be prototyped before enabling frontend-independent reuse of Fib library artifacts across programs. A native linker
resolves symbol relocations; it does not automatically rewrite arbitrary numeric constants embedded in instructions or static headers. The prototype must choose and test
symbolic slots/descriptors or another explicit binding scheme, including its allocation and keyword-dispatch cost. Keeping the entire current ID assignment in every unit's key is a safe fallback, but invalidates unrelated units. The implemented backend cache instead keys the fully specialized emitted unit, including its numeric constants and imported layouts. Tests rebuild two complete programs with different type/keyword orders and require correct results, clean audits, some unchanged unit hits and some rebuilt units. This proves backend reuse after emission; it does not establish a portable Fib library ABI. A supplied standalone object that references the Fib runtime must own its program tables, and duplicate owners are refused.

### Cache contract

Use a collision-resistant digest of a canonical unit and its complete compilation contract. The key includes:

- lIR bodies, referenced constants, required type layouts, and the interfaces of imported definitions;
- compiler/backend identity, LLVM identity, artifact format, runtime ABI and ownership/calling conventions;
- the resolved target triple, data layout, CPU and features, relocation/code-generation settings, optimization pipeline and level;
- settings affecting generated code, including catch mode, instrumentation and debug information.

Canonicalization removes irrelevant ordering and temporary identities only where doing so preserves semantics. Reachable globals, initializer order and constant values
remain inputs. A signature or layout change invalidates clients that depend on it. A body-only change rebuilds its owner and users of any embedded/inlined copy; a client
that only makes an ABI-compatible external call can keep its object. Version one performs no cross-unit inlining and records no dependency on an unseen callee body.

Cache misses, invalid internal entries, incompatible versions and incomplete entries rebuild from checked lIR. Explicitly supplied incompatible objects fail with a
diagnostic. Neither path silently links an object for another target, layout or ABI. A digest detects accidental corruption; it does not authenticate an object from an
untrusted producer. The initial cache is local to the developer/build account, with no remote artifact exchange.

### Build and optimization modes

The ordinary build links cached and newly compiled objects into one executable without whole-program LLVM optimization. Offer whole-program optimization as an explicit
mode using the current whole-module path initially. Record the mode in the build inputs. Native object payloads alone cannot recover LLVM-level cross-unit inlining.

Add cache-disabled builds and phase/cache statistics: front end, lIR emission/checking, LLVM lowering, optimization, code generation, linking, hit/miss counts, and reasons
for invalidation. Keep statistics separate from normal program output. Compare cold and warm builds as well as executable size and runtime speed before choosing defaults.

Retain canonical `F emit` output as the compiler fixed-point comparison. The bootstrap fixed-point build of F3 must bypass the object cache so a cached object cannot stand
in for compilation by F. Exercise warm-cache correctness separately in CI. Existing golden, native ABI, ownership-audit, Linux and macOS checks remain required.

## Consequences

- Repeated builds can reuse the expensive native compilation of unchanged code while distribution remains one executable. Cross-program library reuse becomes possible
  once stable runtime identities and the specialization contract have executable evidence.
- Editing a function can rebuild its unit and relink instead of reoptimizing every function. Adding a generic specialization need not invalidate existing specializations.
- Cold builds may become slower, and separately compiled code may run slower without cross-unit inlining. Unit size, optimization policy and runtime identity access need
  measurements. No improvement percentage is assumed from the self-build baseline.
- The compiler gains an artifact schema, compatibility checks, cache lifecycle, partitioning and program-glue generation. Retaining the existing lIR/LLVM mapping avoids
  duplicating the backend but does not remove those responsibilities.
- Front-end caching and REPL/JIT object reuse can build on this boundary later. They are separate follow-up work; this proposal first targets native executable builds.

## Governance

The implementation and behavioral tests are described in [compiled lIR objects](../design/compiled-lir-objects.md). The native integration suite runs in the full Linux and macOS gates and applies source mutations to demonstrate that its behavioral checks fail. The fitness rules below also protect the cache boundary, fixed-point bypass and registration of those tests.

| Check | Required observation / planted fault |
|---|---|
| Cold and warm build | Both programs produce the expected output and clean ownership audit; the second build reports real object hits and performs no backend compilation for those hits. Disable lookup and require that the test detects the extra compilation. |
| Function edit | Changing a body changes the result and rebuilds the affected unit, while an unchanged independent unit is a hit. Force the stale key and require failure. |
| ABI/layout edit | Change a signature, struct layout or catch convention; dependent units rebuild or explicit incompatible objects are refused. Suppress invalidation and require failure. |
| Cross-program identities | Re-emit two complete programs with different type/keyword insertion orders and reuse unchanged backend units. Objects, drops, keyword equality/dispatch, closures and vtables must retain their identities, with both hits and rebuilt units. Frontend-independent Fib library linking needs the later binding prototype. |
| Generic specialization | Two programs request overlapping and new specializations, including protocol-bound types; old compatible units are hits and new ones compile, with no wrong dispatch or duplicate owners. |
| Initialization and recursion | Multi-unit recursive calls link; shared values initialize once in the prescribed order. Plant a duplicate owner, missing import, reordered initializer and repeated initializer; each must be detected. |
| Target and mode isolation | Change CPU features, target/data layout, optimization, compiler/runtime identity, catch or instrumentation mode. A mismatching cached unit is never selected. Deliberately omit a key field and require the corresponding check to fail. |
| Interrupted/concurrent writers | Kill a writer before publication and run competing writers. Readers see a complete verified entry or rebuild, never a partial pair. Corrupt the manifest/object and require detection. |
| Bootstrap and platforms | Run a cache-disabled compiler fixed point and the existing full gates on Linux x86-64 and macOS AArch64, plus warm-cache correctness checks on both. |
| Performance | Record cold, unchanged warm, body-edit and ABI-edit timings, backend invocation counts, peak memory, link time, executable size and runtime benchmarks on both platforms. Claims name the command, machine, repetitions and cache state. |

```fibber fitness
(rule "cache inputs include checked canonical lIR and the backend contract"
  (into (must-contain repo "compiler/native/liro/compile.fib" "(checked m)")
        (must-contain repo "compiler/native/liro/compile.fib" "(str contract \"\\n\" (print/print-module m))"))
  (plant-file "compiler/native/liro/compile.fib" "(ns native.liro.compile)\n"))

(rule "compiler fixed-point builds bypass the object cache"
  (must-contain repo "mk/stage2.mk" "FIB_LIRO_CACHE=off $(F) build compiler/fibc.fib")
  (plant-file "mk/stage2.mk" "# cache bypass removed\n"))

(rule "the full gate registers the executable LIRO and mutation checks"
  (into (must-contain repo "mk/tools.mk" "sh-native-liro")
        (into (missing repo ["compiler/tests/native/liro/check.py" "compiler/tests/native/liro/mutants.py"])
              (must-contain repo "compiler/tests/native/liro/check.py" "check_plants(fibc, scratch, env, ROOT)")))
  (plant-remove "compiler/tests/native/liro/mutants.py"))
```

### Delivery and remaining choices

1. Add permanent phase timings and a whole-module `.liro` round trip through the existing object emitter and linker. This proves the artifact/validation mechanism;
   an unchanged whole-program cache hit does not yet prove incremental library reuse.
2. Partition checked lIR into deterministic units and generate declarations and program glue. Begin with conservative keys so uncertain dependencies cause a rebuild.
3. Verify backend reuse after re-emission across different type/keyword orders, closures, vtables and overlapping generic specializations. Frontend-independent stable identity binding remains a separate prototype before cached Fib libraries can replace emission.
4. Run the invalidation plants and platform gates, benchmark partition sizes and build modes, then choose the default policy from the results.

The initial implementation uses one glue group, one runtime group, one walker group and up to sixteen stable name-hash groups. The ordinary host build uses these groups; `--whole-program` selects the existing optimization scope, and cache-disabled bootstrap/release builds retain that scope. Measurements and the tradeoff are recorded in the implementation document. Frontend-independent runtime identity binding and finer grouping remain follow-up decisions. No existing language semantics or supported target floor changes as part of this ADR.

### What this does not check

The fitness rules inspect source and build registration; they do not themselves execute LLVM or validate performance measurements. The registered native suite supplies the execution and mutation evidence. Platform and performance claims must name the run; they do not predict other machines or workloads.
Remote caches, a portable binary ABI for third-party libraries, GPU object linking and JIT object reuse are outside the initial scope.
