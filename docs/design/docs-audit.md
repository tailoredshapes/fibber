# Documentation audit for 1.0.0 (DOCS-0)

Status: audit, 2026-10-09, tree `f4423d5c` (VERSION 0.1.13). Read-only: this file is the only change. Evidence is `file:line` or quoted output.
"Ran" means a command was run in this session. Scratch: `/tank/data/fibber-scratch/docs0`.

**Toolchain caveat.** No `fibc` was installed and the release asset URL returned 404 without credentials. Every run below used a scratch stage 2 that
reports `fibc 0.1.13` (`/tank/data/fibber-scratch/gpu4/F1`, built 2026-10-08 from a GPU worktree, not from `f4423d5c`), with `-I lib` of this tree.
Results that depend on a newer compiler are marked. Re-run section 4's script with `build/F` before trusting any pass rate to the digit.

## 1. Stale or contradicted statements

Severity: H = a newcomer is misled or two docs contradict; M = outdated status; L = cosmetic.

### 1.1 README.md and ROADMAP.md

| # | Where | Says | Reality (evidence) | Sev |
|---|-------|------|--------------------|-----|
| 1 | README.md:27 | "stays 0.x until the owner says 1.0.0" | Policy still true, but ROADMAP.md:663 `[ ] 1.0.0 readiness: the owner's call, not decided by any test` contradicts spec/method.md ("nothing is done until an executable test says so"). 1.0 needs an executable definition (section 2, 3.9). | M |
| 2 | README.md:212 | "The current release and bootstrap seed are **0.1.5**" | `VERSION` is 0.1.13, `SEED` names v0.1.13 (SEED:1-2). README.md:28 itself says 0.1.13: the page contradicts itself. | H |
| 3 | README.md:251 | "No darwin-arm64 release has been published" | README.md:24 "from 0.1.7, `fibc-VERSION-darwin-arm64.tar.gz`"; SEED:5 `url.darwin-arm64=.../v0.1.13/fibc-0.1.13-darwin-arm64.tar.gz`. | H |
| 4 | ROADMAP.md:28, 33, 677 | "no darwin-arm64 release yet", open work "the darwin-arm64 release", "Waiting on the owner: publishing a darwin-arm64 release" | Same as 3. Three places. | H |
| 5 | ROADMAP.md:630 | "starts at 0.1.0 and stays 0.x" | See 1. | M |
| 6 | ROADMAP.md:649, 656 | "v0.1.5 is the `SEED` now" and, 7 lines later, "v0.1.3 is the current `SEED`" | `SEED` is v0.1.13. The page contradicts itself twice. | H |
| 7 | ROADMAP.md:12 | "State of play (2026-10-05)" | Four days and ~20 packages old; omits GPU, WebGPU, wasm, C backend, static/musl, TLS, crypto, compress, json, dns, otel, autodiff, work-stealing pool, make coordinator. | H |
| 8 | ROADMAP.md:22-23 | "the compile server, REPL and editor services ... are designed, not built (DV2 onward)" | `fibc serve`, `--server`, `repl`, `lsp` are in `fibc --help` (ran: "serve [--socket PATH] keep the front end's state in a process", "repl", "lsp serve the language server"); dev-loop.md:3 says sections 8-11 record what was built. | H |
| 9 | ROADMAP.md:35 | ports scheduled include "`fibc lsp`" | `lsp` is a command of F1 and `compiler/lsp/` has 9 files; `compiler/tests/lsp/` exists. | M |
| 10 | README.md:195-197 | "since 0.3.0 the pack is the grammar, configuration and snippets only (a language server, `fibc lsp`, is planned)" | `editors/vscode/README.md:15` "The extension starts the language server of the compiler, `fibc lsp`". `editors/vscode/package.json:5` is version 0.2.0, README.md:193 says 0.3.0. Three-way contradiction. | H |
| 11 | README.md:353 | "HTTPS is the next package: until a TLS transport is registered an `https` URL is a typed error" | `lib/fib/tls/http.fib` exists ("fib.tls as a `fib.http.transport` Transport and an \"https\" Factory"); lib/fib/tls/README.md:21 shows `tls/https-options`. TLS is built (docs/design/tls.md:3 "built (TLS-1) ... UNAUDITED"). | H |
| 12 | README.md:254-297 (8 lines), ROADMAP.md (1) | Install/dependency URLs `ssh://git@localhost:2222/tailoredshapes/...` for lacewing, fib-hocon, meshql-fib, fib-db-*, fib-gpu-* | Unreachable by any reader outside the author's LAN. `grep -c localhost:2222 README.md` = 8. A 1.0 README cannot send people to a localhost URL. | H |
| 13 | README.md:7-9 vs 1 | Pitch is "a small working language" milestone plus self-hosting goal | Fine historically; the page has no "what is fibber for", no feature summary, no pointer to a tutorial. Describes what shipped over six weeks as a changelog ("Recent compiler and library changes include", README.md:216-300). | M |
| 14 | README.md:200-210 | Status: "passes are checked against golden outputs recorded from the Rust oracles" | Golden files are now stage 2's own (CLAUDE.md "recorded once from the Rust oracles, since stage 2's own"; `compiler/tests/golden/README`). | L |
| 15 | README.md:380-396 | "currently one known failure: the `def`-initialized atom case 1707" | `grep -vc '^#' scripts/ci-stage2.expected` = 1 (cases/stdlib/1707-...fib FAIL). Still true; but a 1.0 with a known-failing case needs it listed under known limits. | L |
| 16 | README.md:390-398 | "spec/method.md rules ... remain historical text pending consolidation" | spec/method.md:10-28 still states rules 1-2 as if the interpreter existed (`fibref cases` with a "retired" parenthesis at :27). The spec's own method file is the stale one. | M |
| 17 | README.md:401-427 | Performance table "dated 2026-10-04", tree stamp `97a9ee6a6019c82f` | Five days of perf work since (SCHED, ALLOC, in-place). Honest label, but the numbers are not the current ones. | M |
| 18 | README.md:34-36 | Install snippet `fibc run hello.fib` | Ran (F1): prints `hello from fibber`, exit 0. OK. | ok |
| 19 | README.md:124 (exceptions example) | ns main, `try` ... | Ran: `done`, exit 0. OK. | ok |
| 20 | README.md:50-57 | "`fibc build` needs ... `cc`"; "it needs libc, libm, libstdc++, libgcc_s, libz and libzstd" | Not re-run (needs the tarball). Needs a release-tarball check in `make release`; today only `ldd` text in prose. | L |

### 1.2 Commands and options vs `fibc --help`

Ran `fibc --help` (F1). Commands printed: `run build targets emit emit-dump explain check lsp help --version cases test serve --server repl new deps`.

| Item | Docs | Reality |
|------|------|---------|
| `itrace` and `gen` | `--help` last paragraph: "itrace and gen are commands of the Rust compiler (the test harness)" | Stale text in `compiler/fibc.fib` help; both commands are gone. H (the tool's own help). |
| `fibc build --static`, `--link`, `--link-mode`, `--export`, `--exe` | README.md:176-184 documents `--static`, `--link curl=static`, `--link-mode static` | All exist in `compiler/driver/args.fib` (`grep -o '"--[a-z-]*"'`: `--exe --export --link --link-mode --static`) but none appears in `--help` (only `--via`, `--cc`, `--emit`, `--kernel-target` do). Help is incomplete. |
| `fibc check` | Not in README or ROADMAP (0 hits) | Exists, with a clear contract (exit 0 / 3 / 2). |
| `fibc serve`, `--server`, `repl` | 0 hits in README and ROADMAP; only dev-loop.md | Exist. |
| `--emit c`, `--via c`, `--emit ptx`, `--emit wgsl` | README mentions ptx and wgsl (GPU bullets), never `c` or `--via c` | All in `--help` build usage; docs/design/lir2c.md:3. |
| `fibc targets` | README.md:84 only | Ran: 11 rows (x86_64/aarch64 linux gnu+musl, aarch64-apple-darwin, arm64-apple-ios, wasm32-wasip1, wasm32-unknown-unknown, riscv64, nvptx64, wgsl). README never says wasm32 builds run (docs/design/wasm.md:3 "wasm32-wasip1 runs since WASM-2"). |
| `fibc adr` | Makefile `adr` target; compiler/adr.fib:1-2 "as a standalone tool until it joins the driver as `fibc adr`" | Not a `fibc` command. Docs (docs/adr/*.md bodies, executable-adrs.md) say `fibc adr` (e.g. docs/adr/0014:`fails \`fibc adr\` at once`). Name is aspirational. M |
| `make help` | 0 hits in README | Ran: prints ~50 public targets (gate, quick, release, adr, specs, bench, mutants, k8s-*). README documents only `make k8s-*` (README.md:62) and mac. |

### 1.3 Features that exist and are missing from README/ROADMAP

Checked by `grep -c` of README.md / ROADMAP.md (README, ROADMAP): `fib.json` (0,0); `fib.json.schema` (0,0); lz4 / `fib.compress` (0,0); `fib.crypto` (0,0); `fib.regex` (0,1); `fib.dns`, `fib.otel`, `fib.log` (README 2), `fib.merkle`, `fib.sql`, `fib.autodiff`, `fib.bigint`, `fib.decimal`, `fib.fmt` (all 0); wasm32 run (1 passing mention); `--via c` (0); static musl (4, README only); work-stealing pool (README bullet only); `defkernel`/`fib.gpu` (README bullets only, no ROADMAP); `make help` (0); `FIB_*` env vars (README names `FIB_TARGET_CPU`, `FIB_VECTOR_BITS`, `FIB_THREADS` only; code reads about 60, section 2.5).

Library modules in `lib/fib/` with no mention in README/ROADMAP/spec/stdlib.md (exact `fib.NAME` grep, all three zero): `compress`, `crypto`, `dns`, `otel`, `sql`, `merkle`, `autodiff`, `datum`, `rng`, `keyword`. (`tls`: README 1 hit, the "cannot yet connect over TLS" line.) ADR 0016 names facades by "a spec or a case", so a case suffices; no user doc is required.

HOCON, GraphQL (lacewing), meshql, SQLite/Postgres drivers, gherkin, gpu-cuda, gpu-webgpu are separate repositories (README.md:254-297); `fib.hocon` is not in this tree. `fib.json.schema` is in the tree (lib/fib/json/schema.fib; docs/design/json-schema.md:3).

### 1.4 docs/ and docs/design/ status headlines

"Status" line contradicted by the tree. Checked by presence of the code named.

| File:line | Status says | Reality | Action |
|-----------|-------------|---------|--------|
| design/exclusive-views.md:3 | "design proposal, nothing implemented" | `lib/fib/view.fib` (201 lines), `lib/fib/view/tiles.fib`, spec/syntax.md:1692 §3.21 Decided, ADR 0022, README.md:240. | mark implemented |
| design/parallelism.md:3 | "nothing here is implemented in the compiler, runtime, library or spec" | `lib/fib/parallel.fib` + `parallel/` (6 files), `fork-task` in builtins.fib, README.md:226-233, ADR 0021, docs/shootout/parallel.md. | mark implemented (list what was not: cancellation, design 476) |
| design/exceptions.md:3 | "design (EXC0) ... Nothing in compiler/, lib/, rt/ or spec/ is changed" | `lib/fib/ex.fib`, builtins `catch-run caught-message caught-object throw-object catch-active? finally-enter finally-leave`, ADR 0009, README.md:96-117. | mark implemented |
| design/numerical-library.md:3 | "proposal and implementation brief" | `lib/fib/tensor/` 28 files, lib/fib/tensor/README.md. | mark implemented, link README |
| design/test-harness.md:3 | "design with a working prototype" | `lib/fib/test/`, `fibc test` in `--help`, 65 `specs/*.fib` files. | mark implemented |
| design/executable-adrs.md:3 | "design with a working prototype" | `compiler/adr.fib`, `make adr`, 23 ADRs. | mark implemented |
| design/fibber-interpreter.md:5, fibref-port.md:3, fibgen-port.md:3 | "design only", "design", "plan" | `compiler/fibref/`, `compiler/gen/` exist. Interpreter design is superseded (ROADMAP.md:22-24, ADR 0005). | mark superseded/built per file |
| design/lair-in-fibber.md:3 | "design ... Nothing here is code" | `compiler/native/`, `compiler/llvm/`: shipped in the tarball (README.md:50-52). | mark built |
| design/lair-interfaces.md:3 | wave-1 contract for packages R, K1, ... | The packages landed or were dropped. | mark historical |
| design/autodiff.md:3 | "design with a working prototype" | `lib/fib/autodiff.fib`, `lib/fib/autodiff/` (8 files), `specs/ad-core-spec.fib`, `ad-ops-spec.fib`. ROADMAP.md:33 still says "design not written". | mark implemented, fix ROADMAP |
| design/simd-and-tensors.md:3 | "design, revised 2026-10-04 ... Proposed" (1139 lines) | `fib.simd`, `fib.tensor` shipped. | split: keep rationale, mark built |
| design/vectorisation.md:3, design/numerical-performance.md | "measurements and proposals" | Dated measurements; fine as history. | mark historical |
| design/gpu.md:3 | "phase 1 built (GPU-2)" | README.md:292-296 adds atomics (GPU-3) and WebGPU; ran: `examples/gpu/gpu.fib` rejected on F1 with `unbound name gpu/atomic-max-f32` (F1 predates the builtins at HEAD, so toolchain-vintage, not a doc fault; re-check with HEAD's F). | refresh status |
| design/tls.md:3 | "built (TLS-1) ... UNAUDITED" | Accurate; it must also lead the security page (3.8). `tls.md:188` "The server side (design only, not built)" accurate. | keep |
| design/tls-send.md:3 | "proposal for the owner to choose from" | Nothing in `lib/fib/tls` mentions Send; README.md:273 agrees (sslmode=require refused). | keep, link from limits page |
| design/dev-loop.md:3 | "design 2026-10-04; sections 8 to 11 record what was built" | Accurate; sections 1-7 still read as future. | add per-section status |
| design/aarch64.md:3 | "design and spike, 2026-10-05" | `aarch64-apple-darwin` is a supported row in `fibc targets` and shipped as a release. | refresh |
| design/avx512.md:3, build.md:3, compress.md:3, dns.md:3, json.md:3, json-schema.md:3, macro-names.md:3, merkle.md:3, packages.md:3, static-linking.md:3, targets.md:3, webgpu.md:3, wasm.md:3, lir2c.md:3, crypto.md:3 | built / implemented | Consistent with the tree. | none |
| docs/chatgpt-recommendations.md:3 | "Status: proposal" (496 lines, 2026-10-04) | Authored by an external model; most items were adopted, rejected or superseded. | archive under `docs/history/` |
| docs/rust-legacy.md | the Rust is retired | Accurate. | keep, link from history |
| docs/superpowers/specs/ | (directory) | Not referenced by any doc; unknown purpose. | confirm or remove |

### 1.5 spec/

| File:line | Finding |
|-----------|---------|
| spec/syntax.md:1790 | "Twenty-four" core forms, list ends `defkernel`. `with-view` and `with-view-ro` are headed §3.21 as a core-form section (spec/syntax.md:1692) but are absent from this list, and `compiler/expand/qualify.fib:28-29` lists them among macros. Spec and code agree on neither; stop-and-report item for DOCS-1 (CLAUDE.md: "When a spec rule and code disagree, stop and report"). |
| spec/syntax.md:1834 | "Prelude macros (normative list for the reference implementation)": there is no reference implementation any more. |
| spec/\*.md | Rust citations remain: `crates/` hits: bootstrap 35, compiler 18, stdlib 22, lir 7, syntax 7, types 2; `fibref` hits: stdlib 78, bootstrap 51, types 22, syntax 8, compiler 14. Rules are fine; the pointers are dead. |
| spec/method.md:10-28 | Rules 1-2 describe an interpreter that no longer exists (see 16). |
| compiler/types/builtins.fib:1-19 | Header comment describes the table as generated from `crates/fibref/src/types/builtins.rs`. Stale. |
| docs/adr/0014 text | Builtins check: 194 `BuiltinSig` rows (`grep -c '(BuiltinSig "'`); 7 names lack a whole-token spec row (the substring check found `store-i16 store-i32 store-f32 store-f64 store-f32x4 store-f64x4 store-f32x8`); they are the ADR's listed exceptions. spec/syntax.md:4.3 table lists the early builtins only (no `simd/*`, `unchecked-*`, `catch-run` family, `gpu/atomic-*`, shuffles): ADR 0014 holds names in "spec/*.md" anywhere, not in the §4.3 table, so §4.3 is incomplete by design. |
| spec/stdlib.md §4 | 1615 table rows; ADR 0023 lists 239 unbound rows (`wc -l docs/adr/0023-unbound-rows.tsv`; 200 of them reason "tranche"). The page is the plan of a library, not its reference. A reader cannot tell a promised name from a shipped one without that tsv. |

## 2. The surface a 1.0 would freeze

Classes: **S** stable-candidate (freeze at 1.0 after the listed gate), **E** experimental (ships, named as such, may change in a minor), **I** internal (no promise; may change in a patch).

### 2.1 Language

| Item | Class | Reason / gate |
|------|-------|---------------|
| Reader syntax (spec/syntax.md §1) | S | Decided; spec/syntax.md header "signed off". Open: §1.6 "Proposed" rules need a verdict. |
| Core forms (24 listed at syntax.md:1790): `defun def fn let if do match loop recur defstruct . defenum defprotocol impl & async await unsafe extern quote defmacro ns var` | S | Decided, 375 ownership cases and ~1484 stdlib cases. Pin the count and the spelling; resolve the `with-view` listing (1.5). |
| `defkernel` (§3.22), kernel subset (types §2.17) | E | New (GPU-2, 2026-10-07); two backends; subset still growing (atomics, shuffles added GPU-3). |
| `with-view`, `with-view-ro`, `with-tiles` (§3.21, `fib.view`) | E | New (2026-10-06); ADR 0022 restricts to contiguous buffers; surface may widen. |
| Ownership model (spec/ownership.md, Decided) and checker diagnostics | S | Core promise. Diagnostic wording: I. |
| Type system: generics, protocols, `Send`, colours (types.md) | S for generics/protocols/`Send`; E for colour parameters on `impl` (types §10 notes late decisions) | |
| Macros (`defmacro`, `Form`, quasiquote, §3.16) and the `Form` builtins (`struct? struct-fields struct-params struct-field-types enum? enum-params enum-variants gensym concat`) | S | Phase-separated JIT decided 2026-09-27. A dependency's macros run at compile time (README.md:166). |
| Reader macros for regex literals | S | docs/shootout/regex-literals.md. |
| Unsafe and `extern` (§3.15); ADR 0011 confines native access to drivers and `fib.os`/platform | S for the form; the rule "user code does not call `extern`" is policy, not enforced | Needs a guide (3.6). |
| SIMD lane values, `<<..>>`, `simd/*` builtins | E | Target-dependent widths; `FIB_VECTOR_BITS` override; AVX-512 dispatch "designed, not built" (avx512.md:3). |
| Multi-module programs, `ns`, `:require`/`:use`/`:export-from`, resolution order (syntax §5) | S | |
| Traps and `fib.ex` `try`/`catch`/`finally`, `throw` (ADR 0009) | S | Out-of-memory, stack overflow, thread start failure, trap inside `finally` unwinding stay fatal (README.md:116). Document as known limits. |

### 2.2 Builtins (compiler/types/builtins.fib, 194 rows)

| Group | Names | Class |
|-------|-------|-------|
| cells, atoms, weak, freeze | `cell set! atom swap! reset! compare-and-set! freeze private-copy frozen? weak` | S |
| tasks | `spawn fork-task join task-failure trap` | S (`fork-task`: new 2026-10-05, S after the pool is stress-gated; SCHED-2 is still landing, `FIB_SPINNERS`) |
| arrays | `array array-len array-get array-with array-copy array-set! array-blit! array-uninit-f32/f64/i8` | S (`array-uninit-*`: E) |
| in-place primitives | `array-take! array-push! array-pop! cell-update!` | I (ADR 0015: callable only from a short list of library files; spec/types.md §2.13.1) |
| strings | `str-len str-bytes str-from-bytes str-concat str-slice str-byte-at str-find str-eq starts-with? str->keyword` | S |
| conversions and bit casts | `char->i32 i32->char f64->bits bits->f64 f32->bits bits->f32 ctz clz` | S (ADR 0010) |
| program surroundings | `args read-file write-file not` | S |
| unix primitives | `sys-open sys-close sys-read sys-write sys-seek sys-pipe sys-dup sys-isatty sys-unlink sys-mkdir sys-rmdir sys-errno-text sys-getenv sys-clock-now sys-thread-stripe sys-wall-now sys-sleep` | I (reached through `fib.os`; ADR 0014 notes they have no direct case) |
| unsafe memory | `ptr+ load-*/store-* alloc free raw raw-retained release-raw` | I-for-users, S-for-drivers (needs `unsafe`) |
| unchecked arithmetic | `unchecked-add -subtract -multiply -negate` | I (library use; overflow policy is the language's) |
| catch machinery | `catch-run caught-message caught-object throw-object catch-active? finally-enter finally-leave` | I (`fib.ex` is the surface) |
| SIMD | `simd/* lane with-lane hsum hmin hmax simd-load* simd-store* load-simd store-simd load-f32x4/x8 load-f64x4 ...` | E |
| GPU | `gpu/*` (global-id, barrier, shared, atomics, shuffles, ballot, program-ptx, host-index-set!) | E |
| macro-time | `gensym struct? struct-fields ...` | S |

### 2.3 Library (implicit and explicit)

Counts are approximate top-level non-private definitions (`grep`/python over `lib/fib/MOD/**`), not exported names; the exact exported set needs a generated index (3.7).

| Module | Defs | Class | Reason |
|--------|------|-------|--------|
| prelude (88 defs) + `fib.core` (94) + `fib.seq` (142) + `fib.coll` (34) + `fib.print` (13) + `fib.string` (57) | ~430 | S | Clojure-shaped; spec/stdlib.md; ADR 0016/0023. Gate: the 239 unbound rows (ADR 0023 tsv) each become implemented or `omit` and the rows leave the page, so the reference means what it says. |
| `fib.ex`, `fib.parallel` (`fork pmap pfor preduce pscan`), `fib.async`, `fib.adder`, `fib.math`, `fib.char`, `fib.keyword`, `fib.rng`, `fib.datum` | | S except `fib.parallel` internals (E: scheduler knobs `FIB_SPIN*`) | |
| `fib.bigint`, `fib.decimal`, `fib.fmt` | | S (decimal 58 defs; check rounding docs) | |
| `fib.regex` | 211 | S | literals compile at compile time |
| `fib.json` (499, with `json.schema` draft 7) | | S | JSONTestSuite and JSON-Schema-suite expected files in `specs/` |
| `fib.os` (146) | | S on Linux; E on macOS | "Darwin backend ... not yet part of any automated gate" (README.md:343) |
| `fib.http` (243), `fib.dns` | | E | HTTP/1.1 only, Windows not built (http.md:160), no HTTP/2 client |
| `fib.tls` (284) | | **E, UNAUDITED** | docs/design/tls.md:3; client only; session not `Send` (tls-send.md); reads RFC 8446 traces, not a third-party audit. Ship disabled by default or behind a documented warning. |
| `fib.crypto` (219) | | E | A protocol and contract, drivers in other repos; no hand-written primitives (ADR 0012). Hash/AEAD protocol surface is S-candidate after a driver pair passes the contract. |
| `fib.compress` (506, lz4 now; zlib/gzip/zstd later per compress.md:3) | | E | |
| `fib.log`, `fib.otel`, `fib.sql`, `fib.db` (+ `fib.db.contract`, `fib.db.memory`), `fib.merkle` | | E | design docs still carry "prototype" language (observability-and-databases.md:8) |
| `fib.test` (`fibc test`, 163) | | S for `feature/scenario/given/upon/then` as documented | test-harness.md |
| `fib.simd` (7) | | E | |
| `fib.tensor` (165) | | E | README.md:337 "not NumPy parity"; reductions along last axis slower; dtype/rank set may change |
| `fib.autodiff` | | E | |
| `fib.gpu` (187) and `fib.gpu.device` protocols | | E | |
| `fib.view` (`with-view`, `with-tiles`) | | E | |
| `fib.logic`, `fib.logic.fd` | | E | "Parallel search remains future work" (README.md:354) |
| `lib/Integer.fib Long Double Math Character` | | S (Clojure interop shims) | |

### 2.4 CLI

| Command | Class | Note |
|---------|-------|------|
| `run`, `build -o`, `emit`, `explain`, `check`, `cases`, `test`, `new`, `deps`, `--version`, `help`, `targets` | S | exit codes: `check` 0/3/2, `emit-dump` 0/1/2/3, `test` 0/1/2 are the documented ones; freeze them. |
| `build --static`, `--link`, `--link-mode`, `--target`, `-O`, `-L`, `-l` | S | musl pieces are downloaded by `make musl` or shipped with `WITH_MUSL=1`; release ships them only on request. |
| `build --emit obj|asm|llvm` | S; `--emit c`, `--via c` | E; `--emit ptx`, `wgsl`, `--kernel-target` | E |
| `lsp`, `serve`, `--server`, `repl` | E | protocol is one JSON line per request (dev-loop.md §9-11); LSP feature set limited. |
| `emit-dump` | I | compiler-development tool |
| `adr` (`build/adr`, not in `fibc`) | I | |
| exit/trap texts (`trap: this program needs x86-64-v3 ...`) | E: message text is not promised, the fact of a trap is |

### 2.5 Environment variables

Read from code: `grep -rhoE '"(FIB|FIBBER)_[A-Z0-9_]+"' compiler lib rt` (about 60 names).

| Variables | Class |
|-----------|-------|
| `FIB_LIB`, `FIBBER_HOME`, `FIB_TARGET_CPU`, `FIB_TARGET_TRIPLE`, `FIB_TARGET_FEATURES`, `FIB_VECTOR_BITS`, `FIB_THREADS`, `FIB_STACK_MB`, `FIB_MUSL_DIR`, `FIB_CC`, `FIB_NO_PROJECT` | S (documented: `FIB_TARGET_CPU`, `FIB_VECTOR_BITS`, `FIB_THREADS`, `FIBBER_HOME`, `FIB_LIB`) |
| `FIB_STATIC`, `FIB_LINK`, `FIB_LINK_MODE`, `FIB_VIA`, `FIB_KERNEL_TARGET`, `FIB_ALLOW_UNSUPPORTED`, `FIB_ALLOW_PLAIN_TAIL_CALLS`, `FIB_EXPORTS`, `FIB_CC_KEEP`, `FIB_CC_VERBOSE`, `FIB_C_LINES` | E (mirrors of flags) |
| `FIB_LOG`, `FIB_LOG_LEVEL(S)`, `FIB_LOG_FORMAT`, `FIB_LOG_FILE`, `FIB_LOG_FILE_MAX`, `FIB_LOG_FILE_KEEP`, `FIB_LOG_ASYNC`, `FIB_LOG_QUEUE`, `FIB_LOG_OVERFLOW` | E (`fib.log`) |
| `FIB_ALLOC_STATS`, `FIB_ALLOC_DECAY_MS`, `FIB_ALLOC_CACHE_MAX`, `FIB_SPIN`, `FIB_SPINNERS`, `FIB_SPIN_HINT`, `FIB_SPIN_YIELD_AFTER`, `FIB_SCHED_SKIP`, `FIB_PIN`, `FIB_CATCH`, `FIB_AUDIT`, `FIB_TRACE` | I (tuning and test knobs; `FIB_TRACE=1` is used by `fibc cases`) |
| `FIB_WASM_STACK`, `FIB_WASM_MAX_MEMORY`, `FIB_WASM_NAMES`, `FIB_JS_*`, `FIB_WEBGPU_NO_SUBGROUPS`, `FIB_KERNEL_PTX` | E |
| `FIB_AST_VERIFY`, `FIB_LAIR_WORKERS`, `FIB_LSP_ISOLATE`, `FIB_EMIT_EXE`, `FIB_RESULT_STDOUT`, `FIB_STATIC_CASES`, `FIB_STATIC_LD`, `FIB_PRUNE`, `FIB_SYM`, `FIB_ADR_RERUN`, `FIB_SEED_CACHE` | I |

### 2.6 File formats and artifacts

| Format | Class | Reason |
|--------|-------|--------|
| `.fib` source | S | |
| `deps.fib` / `deps.lock` (packages.md) | S | MVP implemented, `:override`, `:aliases`; lock records commit + git tree id. Freeze before any package is published. |
| `*-spec.fib` scenario files (`fibc test`) | S | |
| lIR text (spec/lir.md, 1041 lines, `fibc emit`) | **E** | new surface; `emit` output is regenerated every release (README.md:386 "emit equals the seed's only while prelude is the one the seed embedded"); `cases/lir` has 7 files. |
| Release tarball layout `fibc-VERSION-PLATFORM/{bin,share/fibber/lib,LICENSE,README.txt}` | S | |
| `SHA256SUMS`, `SEED`, `VERSION` (ADR 0017) | I | release engineering |
| PTX header launch ABI (gpu.md), WGSL output, JS glue `fib-webgpu.mjs` | E | |
| `--emit c` C translation unit | E | |
| `serve` JSON-line protocol | E | |
| Case headers (`cases/**` verdict headers) | I | test infrastructure |
| ADR blocks ```fibber spec / fitness / measure``` | I | |

### 2.7 Platform support promise

`fibc targets` (ran): supported rows x86_64 linux gnu/musl, aarch64 darwin / linux gnu/musl, wasm32-wasip1, wasm32-unknown-unknown, nvptx64, wgsl; arm64-apple-ios and riscv64 present (README.md:86-87: riscv64 parked). Evidence of use: Linux x86-64 gated every merge; macOS arm64 has `make mac-check` but is not in the default gate (README.md:343); aarch64 Linux and iOS "have not been run" (README.md:251-252). A 1.0 promise: Linux x86-64-v3 + macOS arm64 S; musl static S; wasm32-wasip1 E; the rest E or unsupported. The CPU floor (x86-64-v3, glibc 2.33) belongs on the first page.

## 3. What a newcomer lacks

Present today: README (install, a few sections, then a changelog), spec (normative, 10k+ lines, formal), 40+ design files (history and rationale), 23 ADRs, 6 library READMEs (`tensor compress os crypto logic tls http`... `lib/fib/*/README.md`: tensor, compress, os, crypto, logic, http, tls), `examples/` (10 entries, no index), `editors/vscode/README.md`. No tutorial, no guide, no generated reference, no CHANGELOG, SECURITY, CONTRIBUTING (ran `ls CHANGELOG* SECURITY* CONTRIBUTING*`: no match).

| # | Need | Today | Gap |
|---|------|-------|-----|
| 3.1 | Tutorial path (install, hello, values/collections, functions, a first project with `fibc new`, tests with `fibc test`, build a binary) | Install snippet only (README.md:34-36); `fibc new` shown in a dependency section | None. Highest value. |
| 3.2 | Ownership and borrowing guide | spec/ownership.md (formal, 282 lines) and 375 cases; README tagline | Needs: borrow vs count, what `fibc explain` prints, the 20 Appendix A cases as narrative, in-out `&`, closures, cells/atoms/weak, `Send`. |
| 3.3 | Modules and `ns` | spec/syntax.md §5, `cases/modules` | `:require/:use/:export-from`, search order (`-I`, `FIB_LIB`, `lib`), "Names that look alike" (README.md:67-86, keep and move). |
| 3.4 | Protocols, `defrecord`, `defstruct`, `defenum`, `match` | syntax §3.6-3.10; `defrecord` is a library macro (no spec §3 entry; grep `defrecord` syntax.md) | A guide with `dyn`, `derive`, orphan rules. |
| 3.5 | Errors and traps | README.md:90-117 (one example) + ADR 0009 + exceptions.md (546 lines, "design") | Trap vs `Result` vs `try`; what stays fatal; exit codes; `FIB_CATCH`. |
| 3.6 | Concurrency (`spawn`, `async`, `fork-task`, `fib.parallel`, atoms, `Send`, `freeze`, tiles) | parallelism.md (design, headline says unimplemented), README bullet | Task tiers table (thread vs pool vs stackless), determinism rule (ADR 0021), `FIB_THREADS`. |
| 3.7 | FFI: `extern`, `unsafe`, drivers, ADR 0011, `:lib` linking, `--link`, static | README.md:176-184 (one paragraph); ADR 0011; static-linking.md | A "write a driver" guide with `fib-db-sqlite` as the worked example (an external repo URL that works). |
| 3.8 | Packaging and dependencies | README.md:137-168 | Decent; move to a page; add publishing a library, version policy, trust note about macros. |
| 3.9 | Library reference | spec/stdlib.md §4 (plan table, 239 unbound rows), 7 library READMEs | A generated per-module reference with signatures, one line each, "since" and stability class. Source of truth: the module exports. |
| 3.10 | Stability and deprecation policy | None. `grep -n -i "stability\|deprecat\|semver"` over spec/ and docs/\*.md: no policy (only unrelated hits) | Required for 1.0: what S/E/I mean (section 2), semver rules, how a name is deprecated, the lIR text and diagnostic wording promises. |
| 3.11 | Security and known limits | tls.md ("UNAUDITED"), crypto ADR 0012, packages.md macro trust, README CPU note | One page: TLS unaudited/client-only, no hand-written crypto, dependency macros execute at build, `unsafe`, traps that stay fatal, no Windows, macOS not in the default gate, 1707 known failure, ssh-only dependencies, scratch images lack CA certificates (README.md:185). Plus SECURITY.md (reporting). |
| 3.12 | Tooling guide | `--help` (incomplete), dev-loop.md | `run -O 0` vs `build`, `serve/--server`, `repl`, `lsp`, editors, `explain`, `check`, `make help` for contributors. |
| 3.13 | Platform guide | README.md:76-91, aarch64.md, targets.md, wasm.md, static-linking.md | One matrix: target, status, how to build, what does not work. |
| 3.14 | CHANGELOG | git log only (1028 commits) | Start at 1.0 with a "since 0.1.0" summary. |
| 3.15 | Examples index | `examples/` 10 entries, no README; ran: `tensor.fib`, `logic.fib`, `exclusive-views.fib` run; `gpu/gpu.fib` needs HEAD's compiler | `examples/README.md` that doubles as a test list (4). |

## 4. Executable checks that exist, and a proposal for doc examples

### 4.1 Existing

| Check | What it holds to the code | Where |
|-------|---------------------------|-------|
| ADR blocks: ```fibber spec / fitness / measure``` (23 fitness blocks, 2 spec, 1 measure by fence count) | ADR claims run by `make adr` (`build/adr --strict`, in the full gate `FULL_STAMPS`, mk/gate.mk:10) | docs/adr/*.md, compiler/adr.fib |
| ADR 0014 | each `BuiltinSig` name appears in spec/\*.md and in a case | |
| ADR 0016 | each library facade named by a spec or case | |
| ADR 0017 | `VERSION`, `version.fib`, `SEED`, docs agree | `scripts/check-version.sh` |
| ADR 0023 | each non-`omit` spec/stdlib.md §4 row is bound, or listed in the tsv (239 listed) | docs/adr/0023-unbound-rows.tsv |
| `cases/` headers | 375 ownership, 1484 stdlib `.fib` cases with verdicts in headers; `make cases` vs `scripts/ci-stage2.expected` | spec/method.md |
| `specs/` (65 `.fib` files, `make specs`) | library behaviour through `fib.test` | |
| golden outputs | pass outputs | compiler/tests/golden |
| `runtime-drift`, `version.ok`, fixed point | generated and shipped files | mk/gate.mk |
| Doc code examples | **nothing** | zero ```fib-tagged blocks anywhere (fence census below) |

Fence census over README, ROADMAP, docs, spec, lib, editors, cases, compiler (`grep -rhoE '^ *```[a-zA-Z0-9_ -]*'`): untagged 970, `lisp` 151, `clojure` 64, `sh` 22, `fibber fitness` 23, `fibber spec` 2, `fibber measure` 1, `rust` 3, `llvm` 2, `text` 1, `markdown` 1, `make` 1. No info string says "this runs".

### 4.2 Pass rate of today's examples

Method: extracted every untagged / `clojure` / `lisp` fenced block that begins with `(` (after comments) and contains a `def*`, `ns`, `let`, `do` or `println` form, from README, ROADMAP, `docs/*.md`, `docs/design/*.md`, `lib/fib/*/README.md`, spec/stdlib.md, spec/syntax.md, editors README (script `/tank/data/fibber-scratch/docs0/extract.py`); 90 blocks, plus the README hello one-liner = 91. Blocks with `(defun main` or `(ns ...` ran as is ("complete", with a stub `main` added if only `ns`); the rest were wrapped in `(ns main (:use fib.core fib.seq fib.coll fib.print fib.string))` ("fragment"). Each ran with `F1 run -I lib` (script `run.py`; results `res.json`, `run.out`). Exit 0 = pass.

| Group | n | Pass | Notes |
|-------|--:|-----:|-------|
| All sampled | 91 | 31 (34%) | |
| Complete programs | 41 | 22 | 22 pass |
| ... of which expected to be rejected | 4 | 4 | spec/syntax.md Appendix A cases 12, 13, 14, 18 (lines 2348, 2363, 2376, 2464) are written as rejected programs; they are rejected. Counting them as correct: 26 of 41 (63%). |
| ... the other failures | 15 | 0 | see below |
| Fragments (wrapped) | 50 | 9 (18%) | not meaningful as a doc-quality measure: design sketches and signatures with no context. |

Appendix A of spec/syntax.md is the only block family that is complete and correct: 20 of 20 behave as stated at the exit-code level (16 run and exit 0, 4 are rejected as the headings say; the printed values were not compared with the text). README: the two executable blocks pass (hello, exceptions example), `lib/fib/logic/README.md:7` and `lib/fib/os/README.md:30` pass.

The 15 unexpected failures of complete programs, by cause (first error lines, ran; counts by cause are approximate, a block can fit two):

| Cause | Count | Examples |
|-------|------:|----------|
| Module from an external repo or driver not in `lib` | 6 | `module sqlite is not at ...` (observability-and-databases.md:377); `fib.crypto.openssl` (lib/fib/crypto/README.md:22, lib/fib/tls/README.md:13); `my-provider`, `open-mydriver` placeholders (crypto README:89, db-contract.md:12) |
| Pseudo-code / placeholder names | 4 | `unbound name expensive-id`, `unbound name prop` (test-harness.md:96), `std-map` (:168) |
| Top-level expression in a non-program block | 3 | `expression at top level` (autodiff.md:246, relational-search.md:56, numerical-library.md:77, compress README:11) |
| Syntax/ type drift | 2 | spec/syntax.md:1954 `invalid keyword ::=` (a grammar fragment); `gpu.md:105` needs `gpu` module and HEAD compiler |

Fragment failures show drift too: `docs/design/json.md:15` "type variable b is not a parameter of Json"; `docs/design/http.md:32` "unknown type Error"/"type Target takes 6 argument(s)" (the signatures changed); `docs/design/compress.md:29` "a method is (name (self qual* x: T qual*) -> type)"; `docs/design/crypto.md:42` "unknown type Kdf". These sketches were right when written and are wrong now: exactly what a check catches.

Command examples (README names, ran with F1 `-I lib`): `run examples/tensor.fib` prints `shape: [2 3]` ...; `run examples/logic.fib` prints `Splits: ...`; `run examples/exclusive-views.fib` prints timings; `build examples/http-server.fib -I lib` builds; `run examples/gpu/gpu.fib -I examples/gpu` is rejected by F1 (`unbound name gpu/atomic-max-f32`, F1 predates HEAD; unverified at HEAD). Pass rate of README-named commands: 4 of 5 on this compiler.

### 4.3 Proposal: `make doc-examples`

1. **Marker.** An info string on the fence: ```` ```fib run ````. Variants: `fib run` (compile and run, exit 0), `fib check` (compile only, for API sketches that cannot run), `fib reject "text"` (must be rejected with a diagnostic containing the text: the Appendix A reject cases), `fib frag` (wrap in the standard prelude `(ns main (:use ...))` plus a `main` stub, then check), no marker = not checked. Output: the next fence ```` ```text out ```` is compared byte for byte with stdout. A hidden-setup line is `;; doc:hide` (compiled, not rendered). Placeholders such as `my-provider` are rejected: a runnable block uses a real in-tree driver (`fib.crypto.memory` or a test provider) or is `fib check` with an in-tree stub.
2. **Tool.** Extend `compiler/adr.fib` (it already extracts ```` ```fibber spec ```` blocks, runs them, and has `--only`, `-j`, `--stamp`, `--strict`, `--format json`; mk/checks.mk:3-8) with a `docs` mode over `README.md docs spec lib/fib/*/README.md examples/README.md`, rather than a second extractor. Pure shell/python is the fallback (the extractor in scratch is 25 lines), but staying in fibber keeps the self-hosting rule.
3. **Make wiring.** `mk/checks.mk`: `build/docs.ok: build/adr $(DOC_INPUTS) $(F)`; target `doc-examples` in `make help`; add to `FULL_STAMPS` in mk/gate.mk and to `quick` only for README and tutorial files (budget: each block is one `fibc run`, ~0.1-0.4 s at `-O 0`, so 200 blocks cost under a minute with `-j8`). Stamp on file hash, as `adr` does, so unchanged docs cost nothing.
4. **Strict mode.** `--strict` fails a doc whose front-matter says `examples: required` (tutorial and guide files) and has no runnable block, and fails an unrecognised marker.
5. **Link and reference checks in the same target.** Relative links resolve; every `ADR NNNN` / `spec/x.md §n` / `docs/design/x.md` reference points to a file; `fibc --help` text equals the generated `docs/reference/cli.md` (diff); the env-var table equals `grep` of the code names (a fitness block, ADR style); each `lib/fib/*.fib` facade has a reference page entry (ADR 0016 extended).
6. **Migration.** Start with the 26 blocks already correct (the 22 that pass, 16 of them Appendix A, plus the 4 Appendix A rejections), mark them; fix or demote the other 15; leave fragments unmarked unless rewritten. Aim: every block in tutorial and guide files carries a marker (the docs are new, so there is no backlog); design docs keep unmarked sketches and gain a "sketch, not checked" line.
7. **Honesty rule (spec/method.md).** A doc statement of the form "X prints Y" is only allowed next to a checked block; the gate lists unchecked-example counts per file in its report so a doc cannot hide in the tail.

## 5. Proposed documentation tree and order

`README.md` shrinks to one screen: what it is, the memory-model paragraph, the CPU/OS floor, install (both platforms, one command), hello, links. The "Recent changes" section moves to `CHANGELOG.md`; "Build and validate" moves to `docs/contributing/`. Existing files keep their names; design docs gain a status line per 1.4 and a `docs/design/README.md` index; retired ones move to `docs/history/`.

```
README.md                       one-screen front door (above)
CHANGELOG.md                    from 0.1.0 summary, then per release
SECURITY.md                     how to report; scope; pointer to docs/limits.md
CONTRIBUTING.md                 clone, make quick/gate, ADRs, what "done" means (spec/method.md)
docs/README.md                  map of the docs: learn, guide, reference, internals, history
docs/tutorial/
  01-install-and-hello.md       install (linux x86-64, macOS arm64), run, build, a binary
  02-values-and-collections.md  numbers, strings, Vec Map Set, seq pipeline, REPL
  03-functions-and-types.md     defun, types, defstruct, defenum, match, Option
  04-ownership-by-example.md    borrow vs count, `fibc explain`, the first rejection
  05-a-project.md               fibc new, modules, fibc test, deps
  06-concurrency.md             tasks, atoms, fib.parallel
  07-ship-it.md                 build -O, static binary, scratch image, wasm
docs/guide/
  ownership-and-borrowing.md
  modules-and-namespaces.md
  protocols-records-and-enums.md
  errors-traps-and-try.md
  concurrency-and-parallelism.md
  macros.md
  unsafe-ffi-and-drivers.md     extern, ADR 0011, writing a driver
  packages-and-dependencies.md
  numerics-simd-tensors.md      fib.simd, fib.tensor, AD
  gpu-kernels.md                defkernel, backends, drivers (experimental)
  testing.md                    fib.test, fibc test, cases, specs
  tooling.md                    run/check/serve/repl/lsp/editors/make help
  targets-and-deployment.md     matrix of targets; static, C, wasm, cross
docs/reference/
  language.md                   core forms: pointer into spec/syntax.md + one-line table
  builtins.md                   generated from builtins.fib (name, signature, class)
  library/INDEX.md              generated: module, stability class, one line
  library/<module>.md           generated per module from exports + hand notes (core seq coll string json http os ... )
  cli.md                        every command, flag, exit code; equals `fibc --help` (checked)
  environment.md                every FIB_* variable and class
  file-formats.md               deps.fib/lock, spec files, lIR (experimental), tarball layout
  diagnostics.md                error texts and what they mean
docs/policy/
  stability.md                  S/E/I, semver, deprecation, what a 1.x may change
  platform-support.md           tiers and gates behind them
  limits.md                     known limits and security (3.11)
docs/design/                    as now plus index and status lines
docs/adr/                       as now
docs/history/                   roadmap archive, chatgpt-recommendations, rust-legacy, interpreter/fibref/fibgen designs
spec/                           normative; Rust citations replaced (1.5)
examples/README.md              index; every example listed has a smoke run in the gate
```

### Order to write

1. Decide: classification of section 2 (owner), the 1.0 definition (an executable list: gates green on both platforms, 239 unbound rows resolved, known failures zero or listed), the `with-view` spec fix (1.5). Nothing downstream can be final before this.
2. Fix the H rows of section 1 (README 212, 251, 353, 195; ROADMAP 28/33/677/649/656/22; `--help` itrace/gen and `--static`; replace `localhost:2222` URLs with published ones or remove). Small, immediate, no new structure.
3. `make doc-examples` (4.3) with the 62 already-correct blocks marked. Land before prose so new prose is born checked.
4. `docs/policy/stability.md`, `limits.md`, `platform-support.md`: shortest and gate 1.0.
5. Generated references (`builtins.md`, `cli.md`, `environment.md`, `library/INDEX.md`) with their equality checks; they make the surface explicit and expose gaps.
6. Tutorial 01-07 (each runnable); then ownership guide (most distinctive feature), errors, modules, concurrency.
7. Remaining guides (FFI/drivers, packages, numerics, GPU, deployment, tooling).
8. README rewrite, CHANGELOG, SECURITY, CONTRIBUTING; move history; add status lines to design docs; replace Rust citations in spec.
9. Review pass: a newcomer runs the tutorial from the release tarball on both platforms; record the result.
