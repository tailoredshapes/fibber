# Gate profile (PROF-1)

Where the time of the longest gate stamps goes, measured. Nothing here is a claim that was not run; what was not measured is in "Remains".

## Boxes and method

- **Ryzen**: tsmar@192.168.7.83, WSL2 Ubuntu 26.04, Ryzen 5800X (16 threads), 15 GB, ext4 on a virtual disk, idle (load under 1 at the start of most runs, 2 to 5 where noted after my own previous run). All "Ryzen" numbers are solo, one run at a time. F was built there from the 0.1.15 seed (`make build/F`, 59 s).
- **lilnas**: this box (28 threads), shared; load 3 to 6 from other users during every run. Used as the second table only.
- Installed on the Ryzen WSL (as root through `wsl -u root`): `strace sysstat nodejs llvm-21 llvm-21-dev libllvm21 binutils linux-tools-generic` plus dependencies. Downloaded into `~/prof1` (scratch): node v26.10.0 (the distro node 22 fails two wasm checks that lilnas's node 26 passes), and `make fetch-wasm` (wasi-sdk 34, wasmtime 49, sha256 checked). Nothing outside WSL.
- Each stamp's recipe was copied from `make -n` (env, TMPDIR, FIBC) and run under a 10 Hz sampler of the thread states of its process tree plus `/usr/bin/time -v` (scripts were in the session scratch, `prof1/`, not committed: `prof.py`, `stamp.sh`). `R` = runnable threads, `D` = uninterruptible (disk or lock wait). `strace -f -c -w` was run once per stamp separately (it slows runs 2 to 3 times; only the syscall mix is used).
- `perf`: software events work in WSL2 (`perf record -e cpu-clock`); hardware counters were not tried. `pidstat` was not needed.
- Page cache: run 1 is cold-ish for the binaries and the liro object cache (`~/.cache/fibber/liro`, empty on a new box), run 2 warm. File-system inputs on warm runs were 0 to 100 blocks; on cold runs 25k to 330k blocks (12 to 160 MB). The disk is not a cost anywhere except the first touch.

## Verdict per stamp (before any change)

CPU = user+sys seconds from `time -v`. "1 runnable" = share of samples with exactly one runnable thread.

| stamp | box, run | wall s | CPU s (user+sys) | D share | verdict |
|---|---|---|---|---|---|
| tools/units-all | Ryzen cold / warm | 81.1 / 57.7 | 81.9 / 59.0 | 4.5% / 3.5% | CPU-bound, serialised: one runnable thread 90% of the time. One `fibc build` of the merged executable: front end (expand/infer/own) 15 s and lIR emit 14 s on one core on lilnas; 19 of 19 liro objects were cache hits and compiled nothing. |
| | lilnas | 46.3 | 48.7 | 0.5% | same |
| sh-persist-batch-verify / onoff / single | Ryzen cold, warm | 69.5, 53.0 / 53.4 / 55.6 | 68.9 / 53.0 / 55.1 | under 1% | CPU-bound, serialised (99% one runnable thread). Each share built its own `compiler/emit.fib` tool: 24 s (lilnas) / 27 s (Ryzen) of one core, the same program as `build/golden/tools/emit`. |
| | lilnas | 44.6 / 47.0 / 48.2 | | | same |
| sh-persist-cmd-0 / 1 / 2 / server | Ryzen | 16.5 / 7.1 / 6.9 / 1.5 | 13.9 / 6.9 / 6.8 / 1.3 | 15% in cmd-0 (cold) | CPU-bound, serialised, small |
| tools/lsp-hardening | Ryzen cold(node 22), warm, warm | 88.5, 75.4, 78.3 | 46.7, 34.9, 35.5 (does not count killed servers) | 2.6% | serialised: 12% of samples with nothing runnable. build 24.7 s, then 9 independent hardening cases one after another (46.6 s; the `memory` case alone 31.7 s: 30 `didChange` of a 35 KB buffer at about 1 s each, real server CPU), then the fuzz selftest 8.1 s, which waits out a hang deadline (8000 ms, a fixed timeout by design). |
| | lilnas | 65.3, 63.5 | 29.4 | 0% | same |
| tools/sh-lanes-lanes | Ryzen | 60.2 | 182.3 (120.2 + 62.0) | 4.4% | 90% of the wall is `F cases ... -j 6` at 512 bits, and the sys time is 34% of its CPU: one `write(2)` per heap-trace line (see "The trace"). Builds of the 12 programs are liro cache hits (1 to 2 s each). |
| | lilnas | 99.8 | 210.3 (97.9 + 112.4) | 0.2% | same; `strace -c`: 45 million `write` calls |
| specs | Ryzen | 83.0, 81.7 | 309 / 313 | 0.9 to 3.7% | CPU-bound and parallel (3.8 of 4 runnable). `perf record`: F 47%, JIT code 35%, libLLVM 10%, libc 5%; no hot spot (top symbol `fib.drop` 3.7%). Nothing waits. |
| | lilnas | 62.5 | 235 | 0.2% | same |
| wasm | Ryzen | 85.2, 88.0 | 285 / 289 | 25 to 27% | CPU-bound and parallel (3.4 of 4). The D samples are WSL-specific on this evidence: 23% of the D samples were `F` in `p9_client_rpc` (a 9p file system call, probably a lookup through a Windows path in `PATH`; cause not confirmed), the rest page-cache waits on first read of the toolchain files. |
| | lilnas | 64.1 | 240 | 2.3% | same |

Process spawns (strace, Ryzen): units-all 2306 execve (1000+ from the 0.1 s poll below), lanes 4512, wasm 1151, specs 56 (5015 threads), lsp-hardening 501, persist-onoff 417. `mprotect` is the most frequent syscall in all of them (0.3 to 3 million calls: glibc grows a non-main thread's arena page by page); a trial with `glibc.malloc.arena_max=1` on the 27 s emit-tool build saved 0.4 s of system time, so it is not worth a change.

Fixed sleeps and polling found by grep: `units.sh` job loop (`sleep 0.1`, fixed below), `persist/run.sh` server ping loop (`sleep 0.3`, condition-based, runs once for 1.5 s in all: left), `slots.sh` back-off (only with GATE_SLOTS), `serve/run.py` (0.1 to 0.2 s socket polls, left), the fuzz selftest hang deadline (by design).

## The trace: the largest finding

Every case runs as `fibc run --trace`, and the runtime wrote each heap event ("A n c", "F n") with its own `write(2)`. Per case (Ryzen, 1500 stdlib cases, 6 at a time, `percase.sh`): 359 million trace lines; the case CPU was **1580 s, of which 835 s system time**. Worst cases: `6100-ref-regex-vs-java-util-regex` 56 million lines, 85 s; `7960-tensor-fma-gemm...` 13 million, 28 s. The deal of the 16 stdlib shards (round robin by name) therefore gave shard sums of 46.6 to 138.5 s of wall: that is the 22 s to 64 s variance of the stdlib shards in the baseline. It is CPU (syscalls), not I/O wait: the output goes to a file in the page cache.

Change (commit `febd35c1`): in trace mode the lines go through a 64 KiB stdio stream on fd 2 (`fdopen` + `setvbuf` in `fib.init`; one `fwrite` per line, so threads do not mix lines); `exit()` flushes; each place that writes to fd 2 itself or aborts calls `fib.trace-flush` first. Outside trace mode nothing changes.

| | before | after |
|---|---|---|
| stdlib cases, Ryzen, summed CPU | 1580 s (sys 835 s) | 826 s (sys 122 s) |
| stdlib cases, Ryzen, summed wall (6 at a time) | 1271 s; run took 222 s and 223 s | 807 s; run took 144 s |
| stdlib shards (round robin) sum of case wall | 46.6 to 138.5 s | 43.2 to 59.7 s |
| case 7960 / 7083 / 8642 / 6100, Ryzen | 27.9 / 21.4 / 4.0 / 85.3 s | 9.6 / 13.4 / 4.0 / 7.7 s |
| case 7960 / 7083 / 8642, lilnas | 29.1 / 20.5 / 39.5 s | 7.5 / 8.3 / 3.5 s |
| sh-lanes-lanes, Ryzen solo | 60.2 s (CPU 182 s, sys 62 s) | 44.6 s (CPU 131 s, sys 14.5 s) |
| stdlib shards through make on lilnas (`-j2`, `CASE_JOBS=3`, loaded) | baseline file: 22 to 64 s at 12 jobs | 25 to 33 s |

Checks that this did not change a verdict (lilnas): ownership 375, modules 32, stdlib 1500 through `make build/cases/full.ok` give `ci-stage2: ok (0 expected non-passing)`, 1482 pass and 18 open as before; single-threaded traces are byte-identical to the seed's; runtime-drift, golden emit-defs and emit-fns, sh-emit-runtime, the fixed point (F.lir = F3.lir) and jit-selfhosted-threads pass. A planted fault (the `fwrite` one byte short) fails 16 of 303 ownership cases; the check can fail. Honest gap: removing the flush before the abort of `fib.fatal-bytes` is **not** caught by the ownership cases (the trace of a trapping run is not needed by their audit); the flush is kept for byte order. Not run: static (no musl here), wasm with this runtime, the full gate; the lead's batch gate is the real test. Not measured: the whole-gate effect (the gate-budget rows must be re-recorded).

## Other changes

| change | commit | before | after (solo) | planted failure |
|---|---|---|---|---|
| The three persist batch shares use `build/golden/tools/emit` (`PERSIST_EMIT`), no build of their own | `c58698e3` | Ryzen 53.0 / 53.4 / 55.6 s; lilnas 44.6 / 47.0 / 48.2 s | Ryzen 25 / 26 / 28 s (the tool build, 27 s, is the golden stamp's, once); lilnas 21 / 23 / 25 s. Summed gate work drops by two tool builds, about 48 s of one core. | an emit tool that adds a line to single runs: batch-single "differs", exit 1; one that differs with the cache off: batch-onoff fails; `PERSIST_EMIT` naming a missing file: FAIL, exit 1. Control with the real tool passes. `compiler/tests/make/graph.sh`: 0 failed. |
| `lsp-hardening`: `hardening.sh` runs the fuzz selftest while the server builds, and `hardening.js --parallel` runs its cases side by side (the latency case, a timing assertion, alone at the end). `mutants.sh` and `fuzz.sh` still run it serially. | `7a405361` | Ryzen 78.3 s; lilnas 63.5 s | Ryzen 54.2 and 55.7 s; lilnas 45.2 s. Floor is build 25 s + the `memory` case 32 s. | server that exits at once: FAIL lines, exit 1; `FUZZ_SELFTEST_DEADLINE=1`: selftest FAILED, stamp exit 1; a starved server (`ULIMIT_V`): 2 cases FAIL, exit 1. |
| `units.sh` waits on the group pid instead of `sleep 0.1` polling when one job runs at a time | `f1723f98` | Ryzen warm 59.8 / 56.1 s, sys 9.0 s, 633 processes seen | 56.5 / 55.2 s (noise), sys 6.4 s, 33 processes | a build that fails and a run that fails still exit non-zero |

The `lsp-hardening` and persist changes alter stamp costs: `scripts/gate-budget.tsv` was not edited (as instructed) and needs a quiet re-record. `lsp-hardening` keeps its name.

## Remains, and why

- **units-all** (about 55 to 80 s on the Ryzen): a single-threaded front end and lIR emit of one merged executable. Splitting into groups costs more total CPU (docs/design/test-pyramid.md). A parallel front end is language work, not a gate change. Not measured: the merge at other `-O` levels.
- **lsp-hardening floor**: the `memory` case (30 sequential edits) and the 25 s server build. Not measured: whether the server build can be shared with another stamp (`fibc lsp` is in F).
- **specs, wasm, lanes**: real parallel CPU. wasm's D share and 9p waits are WSL artefacts as far as measured; the cause (PATH through `/mnt/c`?) was not confirmed. The wasm and specs stamps run `-j 4` each; whether a higher `CASE_JOBS` helps the gate depends on the gate's other load, not measured.
- **Duplicated builds not examined**: the other golden tools (`read expand types own explain lairf`, 3 to 59 s each) and the tools that build `emit.fib`-like programs themselves (sh-emit-*, gen-compare, lsp-server). Only the persist and units paths were looked at.
- **Cold-cache cost in the gate**: units-all is 81 s cold and 58 s warm on the Ryzen; because the runtime changed, every liro object is a miss on the first gate after this change. Not measured: the cold effect on the other stamps.
- **cmd-0** persist share is 16.5 s against 7 s for cmd-1/2 on a cold run (disk first touch, 15% D): the three cmd shares could be rebalanced; not measured warm.
- Hardware counters (cache misses, IPC), `pidstat -d`: not collected; I/O wait is inferred from the D-state sampler, `time -v` file-system counters and strace.
- The lilnas rows were taken at load 3 to 6 from other users and are context, not baselines.
