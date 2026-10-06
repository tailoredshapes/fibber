# fib.json shootout

Design: `docs/design/json.md`. Everything here is the output of `scripts/bench/json/run.sh` (single thread, `ulimit -v 16000000`, under `flock /tmp/fibsuite.lock`,
every fibber figure the median of 5 runs, every competitor's figure the median of 5 timed batches of its own program). Figures are MB/s of input text (of output text
for serialise). **The box this was developed on is shared and noisy (load average 12 to 30 while I worked); the table is from the owner's Ryzen 7 5800X (WSL, 16 threads,
quiet, freshly started: Jackson's JIT had little warm-up there, see the note).** The same run on the development box (i7-14700KF, quiet at the time) is in the last section.

## JSON-2: making it fast (AMD Ryzen 7 5800X, WSL2, 16 threads, quiet; 2026-10-06)

Same corpora, same harness (`scripts/bench/json/run.sh`, single thread, `ulimit -v 16000000`, median of 5), fibber-bench built with `FIB_TARGET_CPU=x86-64-v3` from this tree by a stage 2 built
from the same tree (the SIMD path needs the next release: the seed fibc 0.1.9 has no `simd/movemask`, so library code that uses it compiles only with this tree's compiler).
**Two columns for fibber: the default allocator and glibc tuned with `MALLOC_MMAP_THRESHOLD_=1000000000 MALLOC_TRIM_THRESHOLD_=1000000000 MALLOC_TOP_PAD_=268435456`.** On this machine (WSL2: a page
fault is a hypervisor round trip) every large `malloc` is a fresh `mmap` whose pages fault on first touch, and with equal-sized allocations in a loop glibc's dynamic threshold never
stops it (a freed mmapped chunk raises the threshold to its own size, and the next request of that size is still `>=`): the same stage 1 binary runs at 1.3 GB/s by default and 4.9 GB/s tuned;
on bare metal Linux (the i7 dev box) the same switch moves it 2.77 to 2.98 GB/s. simdjson and Jackson reuse their buffers across the loop, so they do not pay this; an application that parses
documents of varying sizes pays it only on the large ones. The library does not change the allocator of the process (a `mallopt` call would be glibc-only and global). Both columns are measured, neither is hidden.

| MB/s | twitter | citm_catalog | canada |
|---|---:|---:|---:|
| **tape (validate + structural pass)** JSON-1 | 694 | 704 | 446 |
| fib.json tape, JSON-2, default allocator | 908 | 1 175 | 652 |
| fib.json tape, JSON-2, tuned allocator | **1 803** | **2 342** | 1 051 |
| tape + 3 fields by path, default / tuned (JSON-1: 692) | 908 / 1 792 | | |
| typed decode, default / tuned (JSON-1: 655) | 826 / 1 530 | | |
| **DOM parse** JSON-1 | 192 | 356 | 156 |
| fib.json DOM, JSON-2, default allocator | 352 | 468 | 175 |
| fib.json DOM, JSON-2, tuned allocator | 438 | 583 | 205 |
| Jackson `readTree` | 591 | 852 | 99 |
| Python `json.loads` | 308 | 324 | 102 |
| orjson (dev box, no pip on this one) | 795 | 868 | 557 |
| simdjson DOM (haswell, this box) | 3 977 | 4 362 | 1 421 |
| simdjson On Demand (first key only: not the same work as the fib.json "3 fields") | 8 933 | 9 623 | 7 783 |
| **serialise** (MB/s of output) JSON-1 | 258 | 160 | 95 |
| fib.json write, JSON-2, default allocator | 330 | 284 | 98 |
| fib.json write, JSON-2, tuned allocator | 412 | 318 | 104 |
| Jackson `writeValueAsBytes` | 760 | 440 | 248 |
| Python `json.dumps` | 455 | 208 | 59 |
| orjson (dev box) | 3 020 | 1 500 | 1 079 |
| **NDJSON 100 MB** JSON-1 (`reduce-lines`) | 77.7 | | |
| fib.json `reduce-lines`, default / tuned | 98.7 / 120 | | |
| fib.json `reduce-lines-par` (DOM per line on 16 threads), default / tuned | 100.6 / 125 | | |
| fib.json `validate-lines` (tape per line, 16 threads), default / tuned | **1 599 / 1 396** | | |
| simdjson `load_many` / Python line loop / Jackson line loop | 658 / 90.6 / 77.0 | | |

**Against the targets** (set before the work): tape at least 1.5 GB/s: met on twitter and citm with the tuned allocator (1.8 and 2.3 GB/s), not by default (0.9 and 1.2) and not on canada (1.05 / 0.65: a float
document, the time is the number scan and the index of 111 k numbers in 2 MB). Within 2 to 4 times of simdjson on parse: the tape is 2.2x, 1.9x and 1.4x behind simdjson's DOM (which builds a tape too)
with the tuned allocator, 4.4x, 3.7x and 2.2x by default. **DOM at least 800 MB/s and on par with Jackson and orjson: not met**: 352 to 583 MB/s, 55 to 60 percent of Jackson on twitter and citm and ahead of it on
canada (175 to 205 against 99), 40 to 55 percent of orjson on twitter and citm. Typed decode at least tape speed: not met (826 against 908, 1 530 against 1 803: it reads each field by key). **Serialiser at least 1 GB/s: not met**: 330 / 284 / 98
(412 / 318 / 104 tuned), 1.0 to 1.8 times JSON-1, below Jackson everywhere and below Python's `json.dumps` on twitter, 5 to 11 times below orjson.

### What each lever gave (default allocator unless said; each measured, kept only when it won)

| lever | before | after | note |
|---|---|---|---|
| 1 `simd/movemask`, `ctz`, `clz` (ADR 0010) | stage 1 emulated: 700 MB/s (dev box) | 2.3 to 2.9 GB/s (dev box), 4.9 GB/s tuned and 1.3 default (Ryzen); a C version of the same block loop: 4.4 to 4.7 GB/s on the Ryzen | the classification alone runs at 8.5 to 9.9 GB/s (Ryzen), the extraction of positions is the rest |
| 2 index-driven tape | 694 / 704 / 446 | 854 / 1 073 / 508 (first cut), 918 / 1 177 / 651 with eight-digit number scans | stage 1 + stage 2 + fallback to the scalar builder for exact errors; 12 planted faults |
| 3 DOM from the tape | 192 / 356 / 156 | 341 / 435 / 138 (exact-size Vecs, in-place small ints), 352 / 468 / 175 (float token pass, key cache) | the allocation per value stays (a `Json` is a heap enum, a `str` is an owned array: no slice type) |
| 4 serialiser | 258 / 160 / 95 | 263 / 223 / 88 (position-passing core, restart on a full block from 4 KiB by eight), 330 / 284 / 98 (4 KiB, then 4 MiB) | string scan 32 bytes at a time: +15 percent on twitter alone; the per-value cost (about 12 ns for a null in an array of 200 000) is the walk over `Vec`s and the enum, not the bytes |
| 5 NDJSON | 77.7 | 98.7 sequential; parallel DOM 100.6; parallel validate 1 599 | the parallel DOM does not scale: the runtime's allocator is global (`fib.mt` takes the slow path for every allocation), and every value is an allocation; validation allocates one array per line and scales |
| 6 typed path | no path | `$.members[1].tags[0]: expected str, found boolean`; a missing required record field no longer traps (JSON-1 bug: `json->Record d -1`) | `JObj` hash index: not done |

**What lost or did nothing** (kept out of the code, recorded here): writing 16 positions at a time instead of eight (no change; four at a time inlines, an eight-at-a-time function is not inlined and costs
a `vzeroupper` per call), the checked `+` and `*` in the block loop (the overflow branches stopped LLVM from inlining the position writer: `unchecked-add` and `shl` fixed it, 2.3 to 2.9 GB/s), a
256-slot key cache for the DOM (no measurable change: kept, it is cheap), a pre-sized 8 MiB write buffer (no change: the cost is not the growth), a `str` copy in the writer's tail (replaced by overlapping
loads and stores for up to 32 bytes). `perf` is blocked on both boxes (`perf_event_paranoid` 4, and gdb cannot attach), so there was no profile: the decisions above are from experiments that remove one piece
at a time.

**Mac Studio (M1 Ultra, aarch64, llvm@21)**: stage 2 built from this tree by the 0.1.8 darwin seed (1 min 24 s); case 7980 (`simd/movemask` at 4 to 64 lanes, `ctz`, `clz`) and the 88 other vector cases (62xx)
pass; `json-simd-spec`, `json-fast-spec`, `json-lines-spec`, `json-prop-spec`, `json-codec-spec`, `json-api-spec`, `json-testsuite-spec`, `json-floats-spec` pass on the NEON lowering. `json-spec`
dies with status 138 (SIGBUS) on the Mac with JSON-1's library as well (checked at 3f71c08): not caused by this work (the DOM parser's recursion to the depth limit against a thread stack).
No aarch64 speed was measured.

Ryzen results of this run (raw): `scripts/bench/json/` printed them with `fibber-bench dom|tape|write|three|typed|lines|linespar|validatepar FILE REPS`.

## Corpora and tools

| | version | where from | sha256 |
|---|---|---|---|
| twitter.json 631 515 B | | simdjson/simdjson `jsonexamples/` | 30721e496a8d73cfc50658923c34eb2c0fbe15ee6835005e43ee624d8dedf200 |
| citm_catalog.json 1 727 204 B | | same | a73e7a883f6ea8de113dff59702975e60119b4b58d451d518a929f31c92e2059 |
| canada.json 2 251 051 B | | simdjson/simdjson-data `jsonexamples/` | f83b3b354030d5dd58740c68ac4fecef64cb730a0d12a90362a7f23077f50d78 |
| big.ndjson 100 895 586 B, 99 lines | | `scripts/bench/json/make-ndjson.py corpus big.ndjson 100`: the three files minified, one per line, round robin | |
| simdjson | 5.0.2 single header, `g++ -O3 -march=native -std=c++17` (g++ 15.2 on the Ryzen box), implementation `haswell` | github.com/simdjson/simdjson releases `singleheader.zip` | zip 42b52273448d41d3b724d1c5a6057b1989c5d0d1d59eabf666325bf4024fcfae; simdjson.h 904b4e1ccda438e98f854f3254d556e43df6f4579f271111cd2ac35a716549cf; simdjson.cpp 35e9895e4b3d53d2182c3c80dea2954411101d7d22a8e9696f7c07de12008ccc |
| Jackson databind | 2.17.2 (core, databind, annotations), Java 21 (`javac --release 21`) | maven central | core 721a189241dab0525d9e858e5cb604d3ecc0ede081e2de77d6f34fa5779a5b46, databind c04993f33c0f845342653784f14f38373d005280e6359db5f808701cfae73c0c, annotations 873a606e23507969f9bbbea939d5e19274a88775ea5a169ba7e2d795aa5156e1 |
| Python json | 3.14.4 on the Ryzen box (3.1x on the dev box) | stdlib | |
| orjson | 3.12.0 | pip in a venv under scratch: **dev box only**; the Ryzen box has no pip or venv (no system installs allowed) | |
| fibber | stage 2 built from this tree, `fibber-bench` built with `FIB_TARGET_CPU=x86-64-v3` | | |

Commands (from the shootout directory D; `scripts/bench/json/run.sh` runs all of them):

```
fibber-bench dom|tape|write FILE REPS        fibber-bench three|typed twitter.json REPS        fibber-bench lines big.ndjson SIZE
simdjson-bench FILE REPS                     python3 bench.py FILE REPS                         java -cp .:JARS JacksonBench FILE REPS
```

## Table: AMD Ryzen 7 5800X (MB/s)

| | twitter | citm_catalog | canada |
|---|---:|---:|---:|
| **parse to a DOM** | | | |
| fib.json `json/parse` | 192 | 356 | 156 |
| Python `json.loads` | 304 | 325 | 102 |
| Jackson `readTree` | 581 | 856 | 91 (167 on the dev box: JIT not warm on the Ryzen box) |
| simdjson DOM | 3 953 | 4 423 | 1 422 |
| orjson (dev box only) | 795 | 868 | 557 |
| **structural pass / on-demand** | | | |
| fib.json `json/tape` (validating, no allocation per value) | 694 | 704 | 446 |
| fib.json tape + 3 fields by path (no DOM) | 692 | | |
| fib.json typed decode into 3 `defjson` records (100 statuses) | 655 | | |
| simdjson On Demand (my harness reads only the first key: not the same work as 3 fields) | 8 930 | 9 693 | 7 736 |
| **serialise** (MB/s of output) | | | |
| fib.json `json/write` | 258 | 160 | 95 |
| Python `json.dumps(separators=(',',':'))` | 448 | 204 | 59 |
| Jackson `writeValueAsBytes` | 766 | 448 | 253 |
| orjson (dev box only) | 3 020 | 1 500 | 1 079 |
| **NDJSON 100 MB** (parse every line, bounded memory) | | | |
| fib.json `json/reduce-lines` | 77.7 | | |
| Python line loop | 90.5 | | |
| Jackson line loop | 75.1 | | |
| simdjson `load_many` | 563 | | |

Ratios (fib.json DOM parse over the competitor): Python 0.63x / 1.10x / 1.53x; Jackson 0.33x / 0.42x / (0.9x to 1.7x); simdjson DOM 1/21, 1/12, 1/9. The fib.json tape over Python's DOM parse: 2.3x / 2.2x / 4.4x;
over simdjson's DOM parse (which builds its tape too) 1/5.7, 1/6.3, 1/3.2. Serialise over Python: 0.58x / 0.79x / 1.6x; over Jackson 0.34x / 0.36x / 0.38x.

**The stated goal was to beat Jackson and Python's json clearly on parse, to approach serde_json and to be within 2-4x of simdjson. That is met only in part**: the DOM parser beats Python on citm and canada
(floats) and loses on twitter, and loses to Jackson throughout; the tape, on-demand and typed paths beat Python by a factor of 2 or more and are 3 to 6 times behind simdjson's DOM parse. serde_json and simd-json were
not built (optional, and the cargo build did not fit the session).

## Where the time goes (no `perf`: by experiment; the dev box)

- DOM 190 MB/s against tape 690 MB/s on twitter: the difference is allocation. A twitter value costs a `JStr`/`JInt` object, a `str` per string and key, a `Vec` growth per array and two per object. Building the DOM from the tape
  (`node->json`) is as fast as the direct parser (198 against 200 MB/s): the scan is not the cost, the heap is.
- Raw loads (`(raw s)` plus 24) instead of `str-byte-at` gave 2.6x on citm (156 to 412 MB/s), 2.6x on canada, 1.3x on twitter. Eisel-Lemire put canada from 67 (strtod on every number) to 176 MB/s.
- twitter is escape-heavy (`\uXXXX` Japanese text): strings with an escape go through a growable buffer, one allocation and two copies (`buffer-str`), which is why twitter is the worst case of the three.
- Serialise: 95 MB/s on canada is Schubfach (about 110 ns a float: 111 k floats per document), 160 MB/s on citm is integers and short strings: each value costs a pattern match with reference-count traffic on the enum.
  Before Schubfach the same canada write ran at under 10 MB/s with `%.17g` and `strtod`.
- SIMD stage 1 (`fib.json.stage1`, not integrated): about 700 MB/s for the structural index alone against about 800 MB/s for the whole scalar validating tape, so it was not used. The cost is the mask-to-integer emulation
  (the language has no movemask): one quote compare plus the emulation runs at 4.4 GB/s, the three classes plus ladder plus position extraction at 700 MB/s. Details and the missing builtin: `docs/design/json.md` section 3.

## The same run on the development box (Intel i7-14700KF, 28 threads, shared but quiet during this run)

| | twitter | citm_catalog | canada |
|---|---:|---:|---:|
| fib.json DOM / tape / write | 189 / 668 / 320 | 331 / 611 / 196 | 175 / 410 / 110 |
| fib.json three fields / typed | 656 / 629 | | |
| Python parse / dumps | 342 / 533 | 336 / 221 | 113 / 68 |
| orjson parse / dumps | 795 / 3 020 | 868 / 1 500 | 557 / 1 079 |
| Jackson parse / write | 605 / 693 | 696 / 516 | 167 / 234 |
| simdjson DOM / On Demand | 4 499 / 9 162 | 4 211 / 9 852 | 1 459 / 8 527 |
