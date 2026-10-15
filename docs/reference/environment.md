# Environment variable inventory

Generated from quoted constants in `compiler`, `lib` and `rt`. Entries include test/driver output knobs; presence is not a supported configuration promise. Follow the first source use for defaults and accepted values.

`FIB_LIB` adds module roots; `FIBBER_HOME` selects the package cache. `FIB_TARGET_CPU`/`FIB_TARGET_TRIPLE` choose code generation, `FIB_VECTOR_BITS` overrides SIMD width, and `FIB_THREADS` chooses pool size. Static/C builds also use `FIB_MUSL_DIR` and `FIB_CC`.

| Name | First source occurrence |
|---|---|
| `FIBBER_HOME` | [compiler/pkg/git.fib:13](../../compiler/pkg/git.fib#L13) |
| `FIB_ADR_RERUN` | [lib/fib/test/arch/rules.fib:53](../../lib/fib/test/arch/rules.fib#L53) |
| `FIB_ALLOC_CACHE_MAX` | [rt/alloc.lir:310](../../rt/alloc.lir#L310) |
| `FIB_ALLOC_DECAY_MS` | [rt/alloc.lir:309](../../rt/alloc.lir#L309) |
| `FIB_ALLOC_STATS` | [rt/alloc.lir:344](../../rt/alloc.lir#L344) |
| `FIB_ALLOW_PLAIN_TAIL_CALLS` | [compiler/driver/viac.fib:46](../../compiler/driver/viac.fib#L46) |
| `FIB_ALLOW_UNSUPPORTED` | [compiler/driver/commands.fib:62](../../compiler/driver/commands.fib#L62) |
| `FIB_AST_VERIFY` | [compiler/emit/defs/astmod.fib:10](../../compiler/emit/defs/astmod.fib#L10) |
| `FIB_CATCH` | [compiler/emit/program.fib:50](../../compiler/emit/program.fib#L50) |
| `FIB_CC` | [compiler/driver/viac.fib:24](../../compiler/driver/viac.fib#L24) |
| `FIB_CC_KEEP` | [compiler/driver/viac.fib:123](../../compiler/driver/viac.fib#L123) |
| `FIB_CC_VERBOSE` | [compiler/driver/viac.fib:99](../../compiler/driver/viac.fib#L99) |
| `FIB_CHECK_CACHE` | [compiler/driver/front.fib:114](../../compiler/driver/front.fib#L114) |
| `FIB_C_LINES` | [compiler/driver/viac.fib:47](../../compiler/driver/viac.fib#L47) |
| `FIB_EMIT_EXE` | [compiler/driver/commands.fib:14](../../compiler/driver/commands.fib#L14) |
| `FIB_EXPORTS` | [compiler/emit/compile.fib:194](../../compiler/emit/compile.fib#L194) |
| `FIB_JS_ARGV0` | [compiler/lir2js.fib:60](../../compiler/lir2js.fib#L60) |
| `FIB_JS_RT` | [compiler/lir2js.fib:46](../../compiler/lir2js.fib#L46) |
| `FIB_JS_UNLINK` | [compiler/lir2js.fib:59](../../compiler/lir2js.fib#L59) |
| `FIB_KERNEL_PTX` | [compiler/driver/commands.fib:137](../../compiler/driver/commands.fib#L137) |
| `FIB_KERNEL_TARGET` | [compiler/driver/commands.fib:77](../../compiler/driver/commands.fib#L77) |
| `FIB_LAIR_WORKERS` | [compiler/native/cases.fib:19](../../compiler/native/cases.fib#L19) |
| `FIB_LIB` | [compiler/driver/testlive.fib:72](../../compiler/driver/testlive.fib#L72) |
| `FIB_LINK` | [compiler/fibc.fib:87](../../compiler/fibc.fib#L87) |
| `FIB_LINK_MODE` | [compiler/fibc.fib:85](../../compiler/fibc.fib#L85) |
| `FIB_LIRO_CACHE` | [compiler/fibc.fib:70](../../compiler/fibc.fib#L70) |
| `FIB_LIRO_IMPORT` | [compiler/native/liro/partition.fib:122](../../compiler/native/liro/partition.fib#L122) |
| `FIB_LIRO_IMPORT_DEPTH` | [compiler/native/liro/partition.fib:123](../../compiler/native/liro/partition.fib#L123) |
| `FIB_LIRO_JOBS` | [compiler/native/liro/parallel.fib:10](../../compiler/native/liro/parallel.fib#L10) |
| `FIB_LIRO_MODE` | [compiler/fibc.fib:71](../../compiler/fibc.fib#L71) |
| `FIB_LIRO_PARTITION` | [compiler/native/liro/parallel.fib:14](../../compiler/native/liro/parallel.fib#L14) |
| `FIB_LIRO_SPLIT` | [compiler/native/liro/partition.fib:64](../../compiler/native/liro/partition.fib#L64) |
| `FIB_LIRO_STATS` | [compiler/fibc.fib:80](../../compiler/fibc.fib#L80) |
| `FIB_LIRO_UNITS` | [compiler/native/liro/partition.fib:37](../../compiler/native/liro/partition.fib#L37) |
| `FIB_LIVE` | [compiler/driver/testlive.fib:69](../../compiler/driver/testlive.fib#L69) |
| `FIB_LIVE_DEPS` | [compiler/fibc.fib:74](../../compiler/fibc.fib#L74) |
| `FIB_LIVE_EMIT` | [compiler/emit/program.fib:85](../../compiler/emit/program.fib#L85) |
| `FIB_LIVE_LIBS` | [compiler/fibc.fib:123](../../compiler/fibc.fib#L123) |
| `FIB_LIVE_OWN_UID` | [compiler/serve/livesock.fib:80](../../compiler/serve/livesock.fib#L80) |
| `FIB_LIVE_QUEUE` | [compiler/emit/program.fib:266](../../compiler/emit/program.fib#L266) |
| `FIB_LIVE_ROOTS` | [compiler/driver/front.fib:68](../../compiler/driver/front.fib#L68) |
| `FIB_LIVE_SOCKET` | [compiler/serve/live.fib:278](../../compiler/serve/live.fib#L278) |
| `FIB_LIVE_WITH` | [compiler/fibc.fib:76](../../compiler/fibc.fib#L76) |
| `FIB_LOG` | [lib/fib/log/core.fib:126](../../lib/fib/log/core.fib#L126) |
| `FIB_LOG_ASYNC` | [lib/fib/log/config.fib:32](../../lib/fib/log/config.fib#L32) |
| `FIB_LOG_FILE` | [lib/fib/log/config.fib:31](../../lib/fib/log/config.fib#L31) |
| `FIB_LOG_FILE_KEEP` | [lib/fib/log/config.fib:32](../../lib/fib/log/config.fib#L32) |
| `FIB_LOG_FILE_MAX` | [lib/fib/log/config.fib:31](../../lib/fib/log/config.fib#L31) |
| `FIB_LOG_FORMAT` | [lib/fib/log/config.fib:31](../../lib/fib/log/config.fib#L31) |
| `FIB_LOG_LEVEL` | [lib/fib/log/core.fib:128](../../lib/fib/log/core.fib#L128) |
| `FIB_LOG_LEVELS` | [lib/fib/log/core.fib:129](../../lib/fib/log/core.fib#L129) |
| `FIB_LOG_OVERFLOW` | [lib/fib/log/config.fib:30](../../lib/fib/log/config.fib#L30) |
| `FIB_LOG_QUEUE` | [lib/fib/log/config.fib:33](../../lib/fib/log/config.fib#L33) |
| `FIB_LSP_ISOLATE` | [compiler/lsp/server.fib:43](../../compiler/lsp/server.fib#L43) |
| `FIB_MUSL_DIR` | [compiler/native/linkstatic.fib:66](../../compiler/native/linkstatic.fib#L66) |
| `FIB_NO_PROJECT` | [compiler/fibc.fib:133](../../compiler/fibc.fib#L133) |
| `FIB_PRUNE` | [compiler/emit/compile.fib:82](../../compiler/emit/compile.fib#L82) |
| `FIB_RESULT_STDOUT` | [compiler/emit/compile.fib:26](../../compiler/emit/compile.fib#L26) |
| `FIB_SCHED_SKIP` | [rt/sched.lir:143](../../rt/sched.lir#L143) |
| `FIB_SPIN` | [rt/sched.lir:142](../../rt/sched.lir#L142) |
| `FIB_SPINNERS` | [rt/sched.lir:163](../../rt/sched.lir#L163) |
| `FIB_SPIN_HINT` | [rt/sched.lir:157](../../rt/sched.lir#L157) |
| `FIB_STACK_MB` | [rt/thread.lir:264](../../rt/thread.lir#L264) |
| `FIB_STATIC` | [compiler/driver/static.fib:15](../../compiler/driver/static.fib#L15) |
| `FIB_STATIC_CASES` | [compiler/driver/staticrun.fib:10](../../compiler/driver/staticrun.fib#L10) |
| `FIB_STATIC_LD` | [compiler/native/linkstatic.fib:90](../../compiler/native/linkstatic.fib#L90) |
| `FIB_TARGET_CPU` | [compiler/llvm/target.fib:105](../../compiler/llvm/target.fib#L105) |
| `FIB_TARGET_FEATURES` | [compiler/llvm/target.fib:196](../../compiler/llvm/target.fib#L196) |
| `FIB_TARGET_TRIPLE` | [compiler/driver/commands.fib:59](../../compiler/driver/commands.fib#L59) |
| `FIB_THREADS` | [rt/deque.lir:145](../../rt/deque.lir#L145) |
| `FIB_TIME` | [compiler/driver/front.fib:148](../../compiler/driver/front.fib#L148) |
| `FIB_TRACE` | [compiler/driver/commands.fib:51](../../compiler/driver/commands.fib#L51) |
| `FIB_VECTOR_BITS` | [compiler/native/liro/context.fib:34](../../compiler/native/liro/context.fib#L34) |
| `FIB_VIA` | [compiler/driver/viac.fib:22](../../compiler/driver/viac.fib#L22) |
| `FIB_WASM_MAX_MEMORY` | [compiler/native/linkwasm.fib:75](../../compiler/native/linkwasm.fib#L75) |
| `FIB_WASM_NAMES` | [compiler/native/linkwasm.fib:80](../../compiler/native/linkwasm.fib#L80) |
| `FIB_WASM_STACK` | [compiler/native/linkwasm.fib:74](../../compiler/native/linkwasm.fib#L74) |
