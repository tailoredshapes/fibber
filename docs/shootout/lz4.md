# fib.compress.lz4 against liblz4 1.10.0 (COMPRESS-1, COMPRESS-2, COMPRESS-3: fast, SIMD, parallel, on the pool)

Machine: the Ryzen 7 5800X (WSL2 Ubuntu, `ssh tsmar@192.168.7.83`, 8 cores / 16 threads; one core pinned with `taskset -c 3` for single-thread rows; `ulimit -v 16000000`), binaries copied there, scratch only.
Corpus: Silesia (`silesia.zip`, sha256 `0626e25f45c0ffb5dc801f13b7c82a3b75743ba07e3a71835a41e3d9f63c77af`, 211 MB, 12 files, each as ONE block for the block rows; the 12 files concatenated into one
211 MB input for the frame and scaling rows). Reference: lz4 v1.10.0 (`537512904744b35e232912055ccf8ec66d768639ff3abe5788d90d792ec5f48b`) built with `gcc -O3` (generic x86-64): `lz4bench` over
`LZ4_compress_fast`, `LZ4_compress_HC`, `LZ4_decompress_safe` into a buffer allocated once, and the `lz4` CLI (`-T` multithreaded). Ours: `scripts/lz4-bench.fib` built with
`FIB_TARGET_CPU=znver3 fibc build -O 3`. Medians of 5 (HC 9 and 12: 3). Tools: `scripts/fetch-lz4-tools.sh`. Aggregate = total input bytes / total time over the files.
Commands: `lz4-bench MODE FILE LEVEL REPS THREADS BSIZE` (modes in the file's header; it prints the median in microseconds too) and `lz4bench MODE FILE LEVEL`; the loops are `~/.cache/fibber-scratch/COMPRESS1/{final1,scal,cutoff,big2g-shm}.sh` and, for COMPRESS-3, `~/.cache/fibber-scratch/LZ43/{dagg,dref,hcagg,hcref,pscal,final}.sh` (aggregate = total bytes / total microseconds over the 12 files, one core pinned). Run-to-run noise of the single-thread rows is about 1%; of the parallel rows at 8 and 16 threads about 10% (WSL2 on a shared host): two runs of the same binary gave 12094 and 12259 MB/s at 8 threads.

## 1. Reading the assembly first (perf is blocked; `fibc build --emit asm` is the profiler)

What the assembly of the hot loops showed, and what was done about it:

* **Checked arithmetic** (COMPRESS-1): every `+ - *` on an i64 is `add; jo` / `imul; jo` and a branch to a trap: in the decoder's loop about a third of the instructions. The coders use
  `unchecked-add/subtract` (`u+`, `u-` in `fib.compress.lz4.mem`); every index is bounded by array lengths below 2^40, and the bounds checks are comparisons, not arithmetic.
* **A call per short match.** The first decoder inlined everything except `copy-match`; every match at a distance below 16 was `vzeroupper; call copy-match` (with the live registers spilled around it).
  The new decoder (`blockdec`) has the small-distance patterns inline (distance 8 to 15: three 8-byte chunks; below 8: eight bytes written one at a time, then 8-byte chunks from the least multiple of the period
  that is at least 8) and a cold `slow-seq` function, so the loop holds one set of checks. LLVM did NOT turn the byte loops into `memcpy` calls in any hot path (checked: 0 `memcpy` calls in `decode-block`,
  `enc-run`, `xxh32`; the 136 `memcpy` sites in the program are the runtime's); the long copies are 32-byte `vmovups` pairs.
* **Spills.** The decoder's loop keeps 11 values live; `ilim` and `dlim` live in memory (`cmpq 80(%rsp), %r8`): cheap, not free. The fast path is about 45 instructions per sequence (tokens, two limit
  compares, nibble tests, the offset compare, a 16-byte literal chunk, the match chunks, the index updates); liblz4's is about 30.
* **The encoder's emit was a call** (`put-seq`, nine arguments, six of them through the stack, `pushq`): a call per match. `put-fast` (the common sequence: at most 14 literals, a match of 4 to 18, 16 readable
  bytes) is inline: one token store, one 16-byte literal chunk, one 16-bit offset.
* **The hash.** liblz4 hashes FIVE bytes (`(seq << 24) * 889523592379 >> 52`) into a 4096-entry table at every input size; ours hashed four into 4096 to 8192 entries. Matching liblz4 here made the
  OUTPUT byte-identical (section 4) and the table smaller (16 KB), so the encoder's working set stays in L1.
* **xxHash32**: the scalar stripe loop is four dependent `imul; rol; imul` chains per 16 bytes; as one i32x4 it is one `vpmulld; vpaddd; vpslld/vpsrld/vpor; vpmulld` chain per 16 bytes.
* **Allocation.** `fibc run --trace` shows no allocation inside the block coders (they work on raw pointers into arrays their callers hold); per frame block the sequential decoder allocates one `Pos`
  (a 24-byte object); the parallel coders allocate per JOB (about 1 MiB), never per sequence or block.
* **The zero fill.** `(array n 0i8)` of a 211 MB result is a `memset`: 12 ms on this machine (211 MB at 17 GB/s), a third of a parallel decode that otherwise takes 20 ms. `array-uninit-i8` (a builtin
  added for this: spec row, cases 8506 to 8508, `scripts/mutant-uninit-i8.sh`) removes it; the decoders write every byte they return.

## 2. Single thread: before and after (aggregate over Silesia, MB/s, ours vs liblz4, same machine, same run)

| mode | COMPRESS-1 | COMPRESS-2 | **COMPRESS-3** | liblz4 | slower by | size (ours / liblz4) |
|---|---|---|---|---|---|---|
| fast, acceleration 1 | 583 | 731 | **738** | 872 | 1.18x | 47.60% / 47.60%, **byte-identical** |
| fast, acceleration 4 | 655 | 856 | **858** | 1041 | 1.21x | identical |
| fast, acceleration 16 | 873 | 1173 | **1205** | 1477 | 1.23x | identical |
| HC 3 | 139 | 110 | **108** | 134 | 1.25x | 38.38% / 38.38%, **byte-identical** |
| HC 9 | 34 | 40 | **38** | 48 | 1.27x | 36.75% / 36.75%, **byte-identical** (was 93.6% of inputs) |
| HC 10 | | | **29.9** | 32.6 | 1.09x | 36.61% / 36.61%, **byte-identical** |
| HC 11 | | | **17.5** | 19.3 | 1.10x | 36.48% / 36.48%, **byte-identical** |
| HC 12 | 11 | 21 (lazy parser) | **14.7** (optimal parser) | 16.5 | 1.12x | 36.46% / 36.46%, **byte-identical** (was 36.73%) |
| decompress (fast blocks, reused buffer) | 2528 | 3427 | **3630** | 5380 | **1.48x** | target 1.5x: **met** (by 0.02) |
| decompress (HC 9 blocks) | 3002 | 3356 | **3540** | 5406 | 1.53x | |
| frame decompress, 64 KiB blocks, content checksum, 1 thread | ~1300 | 2545 | **3314** | | | 1.10x of block decode, target 1.3x: **met** |

HC 12 is now the liblz4 algorithm (the optimal parser) and costs the speed the lazy parser's shortcut had: 21 -> 14.7 MB/s for 0.7% of size, which is liblz4's own trade (16.5). Removing the lazy search's early exit
at the match limit (liblz4 has none) cost HC 3 and HC 9 3 to 5% (112 -> 108, 40 -> 38).

### 2.1 The decode gap: where the cycles went (COMPRESS-3)

COMPRESS-2 could not say (perf is blocked on WSL2). Two tools did: counting what the sequences ARE (`scripts/lz4-seq-stats.py`, a parser of the frame's blocks) and the assembly.

| file | sequences | avg literals | avg match | literal run >= 15 | match >= 19 | decode ratio before |
|---|---|---|---|---|---|---|
| dickens | 1.06 M | 3.1 | 6.6 | 2.7% | 0.6% | 1.44x |
| osdb | 0.50 M | 7.2 | 13.2 | 14.5% | 29.6% | **1.74x** |
| nci | 1.23 M | 1.1 | 26.3 | 0.8% | 42.4% | 1.67x |
| sao | 0.24 M | 24.9 | 5.7 | **78.8%** | 0% | 1.66x |
| mozilla | 3.08 M | 5.4 | 11.2 | 6.6% | 8.6% | 1.56x |
| x-ray | 0.05 M | 152 | 5.1 | 38.6% | 0% | 1.21x |

The fast path of the decoder takes a sequence whose literal run is at most 14 and whose match is at most 18; every other sequence went to `slow-seq` (byte loop for the extension bytes, every length and index
checked, a call into `copy-exact`, a packed return), which is 100 instructions where the fast path has 45. The files that were slowest relative to liblz4 (osdb, nci, sao) are the files where 30% to 80% of the
sequences were not on the fast path. liblz4's fast loop reads the extension bytes and wild-copies long runs in the same loop. The fix is the MEDIUM path (`mid-seq`: bounded extension reads, one combined room check
for the input and the output, 32-byte wild copies, a decline to `slow-seq` on any failed check so that every error is the slow path's; its proof is in the header of `blockdec`). Per file, measured at that commit, the decoder was 1.14x
(x-ray) to 1.51x (webster) slower than liblz4 (osdb 1.49, nci 1.50, sao 1.29, dickens 1.40, mozilla 1.44): the spread of 1.21x to 1.74x became 1.14x to 1.51x.

| lever (decode, MB/s, Silesia aggregate, one core) | before | after | kept |
|---|---|---|---|
| (a) assembly read: 45 instructions per fast sequence, 8-bit `divb` in the small-offset path (the period of the pattern), the offset reloaded after the literal store | 3495 | | |
| small-offset period from a packed constant (`period8`), no division | 3495 | 3475 | yes (no gain alone: 0.6% below noise; the division was on a rare path; kept because the medium path uses it) |
| (b) match copy: 8-byte chunks for every offset of 8 or more (liblz4's `memcpy` 8+8+2), to avoid a 16-byte load that straddles two stores in flight | 3495 | **3380** | **no** (worse: the store-forwarding suspect is not it; 16-byte chunks stay) |
| **medium path for long literal runs and long matches** | 3495 | **3695** | **yes (+5.7%)** |
| (d) hash each block right after it is decoded, while hot, instead of after the frame | frame 2686 | 2689 | no (the hash costs the same in L1: it is latency bound at 7 cycles per stripe) |
| **(d') hash the previous block INSIDE the decode loop, one stripe per sequence** | frame 2686 | **3314** | **yes (+23%)**: the two chains overlap in the core's window; the plain decoder pays 2% for the two loop-carried values (3695 -> 3630) |
| (c) software pipelining of the next token, (e) loop alignment and branch order, (f) residual overflow checks | | | not done: no builtin was added (ADR 0014: nothing measured above 5%), `unchecked-*` are already in every index computation (assembly: no `jo` on the fast path; one checked multiply, `imulq $255; jo`, remains in the slow path's extension length) |

What the assembly says is left: the fast path is about 45 instructions per sequence against liblz4's 30 (the token test is `sete/or/jne` where two compares would do, the two limit compares are separate, the
offset is read twice); at about 14 cycles per sequence (dickens) that is roughly 3 instructions per cycle, an estimate, since there are no counters, so the remaining 1.48x looks like instruction count and not a stall. The experiments that could change it (merging the two limit checks into one by unrolling two sequences per
check; a 256-entry token table) were not tried.

## 3. Parallel: scaling on the concatenated Silesia (211 MB), on the pool

Since COMPRESS-3 the jobs are tasks of the work-stealing pool (`fork-task`), not OS threads. Frame compress and decompress, independent blocks, `Options.threads` = T (jobs in flight, at most the pool: 16 here),
medians of 5; speedup against T = 1 in brackets. `fd`: decompress with verification (block, content and header checksums), `fdn`: with verification off (`Options.verify` false), `fdp`: the frame has no recorded
content size (the placement mode). At T = 1 the sequential decoder runs, and it hashes while it decodes (2.1), so the T = 1 column of `fd` is 23% above COMPRESS-2's and every speedup is against a faster base;
the sequential decoder does not look at `verify`, so `fdn` at T = 1 still hashes. For a reference: liblz4's CLI, `lz4 -1 -BI -B7 -T{1,2,4,8,16}` (file to /dev/null, which includes its I/O threads, measured in
COMPRESS-2 on the same machine): compress 636, 1184, 1695, 1385, 1254 MB/s; decompress 1796 whatever T (the CLI's decoder is single threaded).

| blocks | mode | T=1 | T=2 | T=4 | T=8 | T=16 |
|---|---|---|---|---|---|---|
| 64 KiB | compress | 597 | 1117 (1.9x) | 2104 (3.5x) | 3398 (5.7x) | 4899 (8.2x) |
| 1 MiB | compress | 678 | 1278 (1.9x) | 2304 (3.4x) | 3821 (5.6x) | 4697 (6.9x) |
| 4 MiB | compress | 680 | 1268 (1.9x) | 2351 (3.5x) | 3836 (5.6x) | 5145 (7.6x) |
| 64 KiB | decompress, verified | 3302 | 5044 (1.5x) | 7717 (2.3x) | 5910 (1.8x) | 5425 (1.6x) |
| 1 MiB | decompress, verified | 3313 | 5113 (1.5x) | 7646 (2.3x) | 5955 (1.8x) | 5280 (1.6x) |
| 4 MiB | decompress, verified | 3341 | 4984 (1.5x) | 7662 (2.3x) | 6207 (1.9x) | 5752 (1.7x) |
| 64 KiB | decompress, no verification | 3313 (hashed) | 6273 (1.9x) | 10919 (3.3x) | 9862 (3.0x) | 9254 (2.8x) |
| 1 MiB | decompress, no verification | 3231 (hashed) | 6754 (2.1x) | 11459 (3.5x) | 10424 (3.2x) | 9758 (3.0x) |
| 4 MiB | decompress, no verification | 3341 (hashed) | 6556 (2.0x) | 11891 (3.6x) | 12582 (3.8x) | 11884 (3.6x) |
| 1 MiB | decompress, no content size (placement) | 2803 | 2908 (1.0x) | 2819 (1.0x) | 2128 (0.8x) | 1931 (0.7x) |

(Against the decode-only speed of one thread, 3580 MB/s, the 4 MiB no-verification row at 8 threads is 3.5x and the 64 KiB row at 4 threads 3.1x.) Noise at 8 and 16 threads is about 10% (two runs of the same
binary: 12094 and 12259 at 8 threads, 1 MiB, verification off).

**The engine: counter or a task per job.** The jobs were `spawn`ed OS threads self-scheduled from a counter; P-SCHED tested `fork-task` in their place (cases 8501, 8502, 8504 pass both ways) and that is what runs
now. A second shape was built and measured, because the pool makes small tasks cheap (72 ns fork + join): one forked task PER JOB with at most T unjoined, joined in index order and the next job forked at each join.
1 MiB blocks, MB/s, T = 1 / 2 / 4 / 8 / 16:

| engine | compress | decompress (no verification) |
|---|---|---|
| counter, runners are pool tasks (kept) | 687 / 1249 / 2364 / 3917 / 5350 | 2685 / 6851 / 11614 / 12447 / 11643 |
| one task per job, T in flight, in-order join | 692 / 901 / 1645 / 2487 / 3506 | 2582 / 3522 / 7402 / 9912 / 9973 |

The per-job version loses by 28% to 49% at every thread count above 1: a slot sits idle whenever a later job finishes before an earlier one (the join is in order) and every job pays a wake-up of the joiner. About
1 MiB jobs are coarse enough that the counter's imbalance costs nothing. (The old engine, `spawn` per runner: 681 / 1323 / 2365 / 3490 / 5185 compress, within noise of the counter on the pool.)

**Nested use.** A program that compresses or decompresses inside the body of a `pmap` (12 bodies, each with `threads` 8) used to start 12 x 8 OS threads; now the process has the pool's workers and its own thread
(case 8720; the par spec's scenario; mutant `par-nested-threads` puts `spawn` back and both kill it). `threads` is jobs in flight: the engine's own scenario asserts that no more than `min(T, pool)` jobs ever run
at once (mutant `par-unbounded-flight`).

**Targets and where they fall short.**

* Parallel compress at 8 threads: **5.6x to 5.7x** (target 6x); 6.9x to 8.2x at 16 threads (8 cores and their SMT siblings). The sequential parts are the header, the final copy of the blocks into the one result array
  (parallel above 8 MiB) and the task start: about 10 ms of 55.
* Parallel decompress at 8 threads, verification off: **3.0x to 3.8x of a one-thread decode that hashes**, 3.5x of decode-only; memory bandwidth bounds it (each output byte is written once, each compressed byte read
  once, about 30 GB/s moved at 12 GB/s of output).
* Parallel decompress WITH the content checksum: **2.3x at 4 threads (7.7 GB/s) then 1.8x at 8 (5.9 GB/s)**, against a one-thread base of 3.3 GB/s that already overlaps its hash. The claim of COMPRESS-2 that the
  hash cannot be split was checked (below) and the pipeline measured; the bound is the speed at which ONE thread can hash data that other cores wrote.

### 3.1 The content checksum in parallel decode: what was verified, built and measured (COMPRESS-3)

1. **xxHash32 does not combine.** `scripts/lz4-xxh-combine.py` (a Python xxh32 checked against two known vectors) finds, after 32,390 random 16-byte inputs, two different inputs A1 and A2 with the SAME
   xxh32 (0x4b7148a4) and shows that xxh32(A1 || B) = 0xe3d5f696 differs from xxh32(A2 || B) = 0xe04f3fbc for the same B: the hash of a prefix does not determine the hash of what follows, so per-block HASHES cannot
   be combined. The per-block lane STATES cannot either: each of the four lanes is v' = rotl(v + x P2, 13) P1 per 4-byte word, a bijection of 32 bits whose dependence on v is the carry chain of an addition followed by
   a rotation, and the state after block k is the input of block k + 1; there is no per-block summary that is smaller than the block. Stripe-aligned blocks do not help (every block of this library but the last is a
   multiple of 16 bytes already).
2. **The pipeline was built**: the content checksum is on a dedicated runner (runner 0 of W runs `hash every finished job in index order, the moment it is ready; decode a job itself when none is ready; give up after
   0.5 s of no progress`), instead of "whichever worker finishes a job takes the hash lock". Measured, 1 MiB blocks, verified, MB/s at T = 2 / 4 / 8 / 16: dedicated hasher 5260 / 8018 / 6406 / 5778; lock scheme 5251 /
   7963 / 6152 / 5873. **No difference**, so the simpler lock scheme stays.
3. **Why**: timing the hash inside the pipeline (nanoseconds spent in `hash-ready`, 211 MB): T = 2: 20 ms (10.6 GB/s, the single-core figure), T = 4: 24 ms, T = 8: 33 ms (6.4 GB/s); the hasher never waits (0 ms) and is
   busy for 33 of 35 ms. Hashing the same bytes takes 65% longer while seven other cores decode and write them. The explanation that fits (an ESTIMATE: there are no counters here): a stripe is one load and about 9 micro-ops, so the
   224-entry window holds about 25 stripes (400 bytes, 6 cache lines) of loads in flight, and a line that another core's L2 owns takes about 100 ns, which is 6 GB/s. Pinning the 8 threads to the 8 even-numbered logical CPUs (one per core) gave the same hash time, so it is
   not SMT. A `prefetch` builtin (`llvm.prefetch`: spec row, case, mutant, ADR 0014; lowering in three backends) would put 20 or more lines in flight; the prediction is the hash returning to 10.8 GB/s and the verified
   decode at 8 threads to about 10 GB/s (hash 20 ms + start), 1.7x the present figure. NOT built: it is a compiler change (stage 2 and its fixed point) for one lever, and the task said to add a builtin only for a
   measured gain; this one is predicted, not measured.
4. **The ceiling**: one core of xxh32 is 10.8 GB/s (211 MB in 19.6 ms); that is the most the verified decode can reach whatever the thread count, against 12.4 GB/s of unverified output, so the limit is 10.8 / 12.4 = 87% of
   the unverified speed at best. 10.8 GB/s is below the 12 GB/s that the decoders produce at 8 threads, so the checksum WILL bound a verified decode: at best by 13%, today by 52%.

### Where parallelism starts to pay (`Options.threads` 0 = automatic)

Slices of the Silesia, 64 KiB and 1 MiB blocks, medians of 15, MB/s; T = 1 / 2 / 4 / 8:

| input | compress 64 KiB blocks | compress 1 MiB blocks | decompress |
|---|---|---|---|
| 1 MiB | 405 / 319 / 322 / 331 (loses) | 450 / 405 / 352 / 385 (loses) | 2173 / 2387 / 2391 / 2365 (+10%) |
| 2 MiB | 415 / 658 / 683 / 658 (+60%) | 458 / 731 / 749 / 808 (+60%) | 2375 / 2895 / 2813 / 3068 |
| 4 MiB | 417 / 704 / 1272 / 1335 (3.2x) | 456 / 771 / 1396 / 1463 (3.2x) | 2379 / 3458 / 4148 / 4086 |
| 8 MiB | 416 / 740 / 1337 / 2310 (5.6x) | 461 / 809 / 1430 / 2509 (5.4x) | 2383 / 3695 / 5384 / 5269 |
| 32 MiB | 556 / 1018 / 1824 / 2903 (5.2x) | 631 / 1133 / 2023 / 3139 (5.0x) | 2297 / 3803 / 6109 / 6448 |

So the automatic mode is parallel from 2 MiB of input to compress and 512 KiB of compressed input to decompress (`par-cutoff-compress`, `par-cutoff-decompress` in `fib.compress.lz4`), and one worker below.

### Streams: bounded memory, and what the disk allows

A 2 GB synthetic text file (`big2g`, 2,148,680,553 bytes) through `examples/lz4.fib` (`-1 -BI -B7 -T N`), streaming from file to file, on tmpfs and on the VM's disk; `GNU time -v` peak resident set:

| | wall | peak RSS |
|---|---|---|
| compress, T=1 (reads, codes, writes in turn) | 7.3 s | 20 MB |
| compress, T=2 / 4 / 8 (reads ahead and writes behind in tasks, batches of N MiB of blocks coded in parallel) | 3.3 / 3.2 / 3.1 s | 101 / 158 / 250 MB |
| decompress, T=1 | 4.3 s | 37 MB |
| decompress, T=2 / 4 / 8 | 2.3 / 2.2 / 2.4 s | 147 / 299 / 583 MB |
| liblz4 CLI, compress T=1 / T=8, decompress | 4.0 / 1.5 / 2.0 s | |
| `cp` of the same file on tmpfs | 2.2 s | |

Memory depends on the thread count (the queue and batch are a few MiB per worker), never on the length of the stream, and the round trip is checked with `cmp` in every row. Overlapping the I/O halves the wall
time; beyond 2 threads the stream is bound by the VM's tmpfs write rate (the file copy alone is 2.2 s): the reference CLI's 1.5 s compress writes the same bytes, with its I/O threads; ours has one
reader task and one writer task and a main thread that copies each chunk twice.

## 4. Byte-identical output with liblz4 (python `lz4.block`, LZ4_compress_fast and LZ4_compress_HC), 30,000 seeded inputs, sizes to 400 KB

`scripts/lz4-diff.py` (seed 31), 60,000 checks (both directions), 0 failures; the percentages are of the inputs that the harness drew for each encoder (about 600 per HC level, 1,200 to 3,900 per fast setting).

| encoder | COMPRESS-2 | **COMPRESS-3** | compressed bytes ours / liblz4 |
|---|---|---|---|
| fast, acceleration 1, 2, 4, 16, 64 | 100% | **100%** (3937, 1233, 1308, 1266, 1228 of the same) | 1.0000 |
| HC 3, 6, 7, 8 | 100% | **100%** (599, 592, 598, 608) | 1.0000 |
| HC 4, 5 | 99.8%, 99.7% | **100%** (567, 618) | 1.0000 |
| HC 9 | 93.6% | **100%** (608 of 608) | 1.0000 |
| HC 10, 11, 12 | 71%, 73%, 70% | **100%** (596, 614, 632 of the same) | 1.0000 |

What closed the gap: at level 9 the PATTERN ANALYSIS of `LZ4HC_InsertAndGetWiderMatch` (`fib.compress.lz4.hcpa`: when the chain step is 1 and the bytes at the position are a pattern of 1, 2 or 4 bytes, the search
jumps over the run); at 10 to 12 `LZ4HC_compress_optimal` itself (`fib.compress.lz4.hcopt`: prices in bytes, the backward walk, `FindLongerMatch` with pattern analysis and CHAIN SWAP) and liblz4's search depths
(96, 512, 16384; the lazy parser's 512, 2048, 16384 were not liblz4's). The 4 and 5 gap was the lazy search's early exit at the match limit, which liblz4 does not have. The first run of the port was wrong in one
place only (the chain swap's two result words shared a register with `last_match_pos`: a segfault and 15 inputs whose blocks decoded to other bytes); the harness found it on the first 1,500 inputs.

The fast encoder matches because it IS liblz4's algorithm: the five-byte hash into 4096 entries, the empty slot read as position 0 of a fresh table (liblz4's `LZ4_compress_default` starts from a zeroed table, so
index 0 is a candidate: the block API starts its table at that base), the skip acceleration, the "try the position after a match at once", the last five literals. This was found with a Python
simulation of `LZ4_compress_generic` (`scripts/lz4-sim.py`) that agrees with liblz4 on every one of 374 small inputs; the HC parsers are ports of `LZ4HC_compress_hashChain`, `LZ4HC_compress_optimal` and
`LZ4HC_InsertAndGetWiderMatch`. On Silesia-size blocks the sizes are identical for fast and HC 3 to 12 (aggregate table above); HC 3 on mozilla (51 MB) and samba (21 MB) is byte-identical.
`specs/compress-lz4-hc-spec.fib` and case 8721 keep eight inputs at six levels as the size and FNV-1a hash of liblz4's block (`scripts/lz4-hc-vectors.py` prints them).

## 5. Differential tests, hostile input, mutants, contract

* `scripts/lz4-diff.py lz4-diff 30000 31`: both directions against liblz4 1.10.0, 30,000 seeded inputs (text, runs, records, random, zeros, patterns, mixtures; frames at every level, checksum, block size,
  independence, content-size combination; blocks fast and HC): **60,000 checks, 0 failures** (run on the final tree). `scripts/lz4-interop.sh`: `examples/lz4.fib` against the `lz4` CLI both ways, 8 files x 14 flag sets
  (including `-T4`): see the result line in the commit message of the merge (it is run once on the final tree). 122 frames and 40 blocks of the CLI's and liblz4's own output decode
  (`lib/fib/compress/lz4/vectors.fib`).
* Specs (`fibc test specs`, every file green on the final tree: `compress-lz4-spec` 17 scenarios, `-edge-` 15, `-prop-` 5, **`-hc-` 4 (new)**, `-safety-` **4** (was 2), `-xxh-` 2, `-par-` **16** (was 14), `-hostile-` 9,
  `compress-contract-shape-spec` 14 (11 faults, each caught)). New: the HC spec above; in the safety spec the MEDIUM path at sizes to 6000 bytes with 0, 8, 33 and 40 bytes of room, and GUARD PAGES (two `mmap`ed
  pages, the second `mprotect`ed to nothing: a block placed to END at the guard is decoded, and one decoded into a range that ENDS at it, 2000 blocks each: an over-read or an over-write is a SIGSEGV that kills the
  process, which is how a slack that is too small is caught even when a wild read changes no byte of the result: the canary and garbage-after tests alone let `mid-islack` survive); in the par spec "`threads` is the
  number of jobs in flight" and "calls inside a pmap body gain no threads".
* Cases 8720 (nested use), 8721 (HC bytes at levels 9 to 12), 8722 (300 random hand-built blocks with long literal runs, long matches and small offsets against a byte decoder, with exact room and with 64 to spare);
  floor of `cases/stdlib` raised 1472 -> 1475.
* `scripts/mutant-lz4.sh`: **43 planted faults**, the 32 of COMPRESS-2 and 11 new, all killed: the medium path's input slack (`mid-islack`: killed by the guard pages), output slack (`mid-dslack`), offset check
  (`mid-offset`) and small-offset pattern (`mid-pattern`); the pool path: runners that are OS threads again (`par-nested-threads`: killed by the nested scenario) and one runner per job whatever `threads` says
  (`par-unbounded-flight`); the HC parts: a sequence priced one byte too high (`hc-opt-price`), no pattern analysis (`hc-pa-off`), no chain swap (`hc-swap-off`), level 12 not searching everywhere
  (`hc-opt-full`), a strict skip test (`hc-opt-skip`), each killed by the HC spec. A mutant that SURVIVED on the way and was dropped: `mid-cap` (one byte past the cap in the medium path) is unobservable, since the
  next sequence fails the cap check whatever the byte count, so it is an equivalent mutant, not a gap. `scripts/mutant-uninit-i8.sh`: the builtin's length check removed: killed by case 8508.

## 6. Hello world

`fibc emit` of `(ns main) (defun main () -> i64 (do (println "hello") 0))`: the lIR is byte-identical (154,322 bytes) with this tree and without it (explicit modules; the one change outside the library is the
builtin `array-uninit-i8`, a row in the compiler's table).

## 7. Wasm

Cases 8470 to 8475 build with `--target wasm32-wasi` and return 0 under node's WASI (COMPRESS-1); the parallel cases need threads and are not run there (`spawn` is native only).
