# fib.compress.lz4 against liblz4 1.10.0 (COMPRESS-1, COMPRESS-2: fast, SIMD, parallel)

Machine: the Ryzen 7 5800X (WSL2 Ubuntu, `ssh tsmar@192.168.7.83`, 8 cores / 16 threads; one core pinned with `taskset -c 3` for single-thread rows; `ulimit -v 16000000`), binaries copied there, scratch only.
Corpus: Silesia (`silesia.zip`, sha256 `0626e25f45c0ffb5dc801f13b7c82a3b75743ba07e3a71835a41e3d9f63c77af`, 211 MB, 12 files, each as ONE block for the block rows; the 12 files concatenated into one
211 MB input for the frame and scaling rows). Reference: lz4 v1.10.0 (`537512904744b35e232912055ccf8ec66d768639ff3abe5788d90d792ec5f48b`) built with `gcc -O3` (generic x86-64): `lz4bench` over
`LZ4_compress_fast`, `LZ4_compress_HC`, `LZ4_decompress_safe` into a buffer allocated once, and the `lz4` CLI (`-T` multithreaded). Ours: `scripts/lz4-bench.fib` built with
`FIB_TARGET_CPU=znver3 fibc build -O 3`. Medians of 5 (HC 9 and 12: 3). Tools: `scripts/fetch-lz4-tools.sh`. Aggregate = total input bytes / total time over the files.
Commands: `lz4-bench MODE FILE LEVEL REPS THREADS BSIZE` (modes in the file's header) and `lz4bench MODE FILE LEVEL`; the loops are `~/.cache/fibber-scratch/COMPRESS1/{final1,scal,cutoff,big2g-shm}.sh`.

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

| mode | COMPRESS-1 ours | now ours | liblz4 | slower by now | size now (ours / liblz4) | target |
|---|---|---|---|---|---|---|
| fast, acceleration 1 | 583 | **731** | 871 | 1.19x | 47.60% / 47.60%, **byte-identical** | within 1.3x: met |
| fast, acceleration 4 | 655 | 856 | 1039 | 1.21x | identical | |
| fast, acceleration 16 | 873 | 1173 | 1479 | 1.26x | identical | |
| HC 3 | 139 (old parser) | 110 | 135 | 1.22x | 38.38% / 38.38%, **byte-identical** | within 2x, ratio within 1%: met (0%) |
| HC 9 | 34 | 40 | 49 | 1.23x | 36.75% / 36.75% | met |
| HC 12 | 11 | 21 | 16 | **0.79x (faster)** | 36.73% / 36.46% (+0.7%) | ratio within 1%: met |
| decompress (fast blocks, reused buffer) | 2528 | **3427** | 5400 | 1.58x | | within 1.5x: **missed by 0.08** |
| decompress (HC 9 blocks) | 3002 | 3356 | 5395 | 1.61x | | |
| frame decompress, 64 KiB blocks, content checksum | ~1300 | **2545** | | (liblz4 has no frame bench here; the CLI decodes at 1.8 GB/s) | | within 1.3x of block decode: met (1.35x) |

Per file the decoder is 1.21x (x-ray) to 1.74x (osdb) slower than liblz4 (dickens 1.44, mozilla 1.56, nci 1.67, sao 1.66). A block that is one long literal run decodes at memcpy parity (8 MB random: 22.7 GB/s
against 22.2). **What is left**: the decoder keeps about 45 instructions per sequence against liblz4's 30 and has no profile to say where its extra cycles go (store-forwarding stalls between the literal chunk
and a match that reads it are the suspect: liblz4 has the same pattern with 8-byte chunks). HC 3 to 9 are slower than liblz4 by about the same factor as the fast encoder.

Levers, each measured alone on the Ryzen (MB/s, aggregate; noise about 3%):

| lever | before | after |
|---|---|---|
| decoder: one set of checks, small-distance patterns inline, cold slow path | 2528 | 2845 |
| encoder = liblz4's hash and table (blocks byte-identical) | 2845 (decode of the old encoder's blocks) | 3441 (decode of liblz4's blocks: the blocks changed, not the decoder) |
| `put-fast` inline | 583 (compress) | 637 |
| 5-byte hash, 4096-entry table | 637 | 737 |
| vector xxHash32 (alone, `x` mode: 10.8 GB/s; 4 times the scalar's) | frame decode 1264 | 1423 |
| content size recorded by default: the decoder allocates once, exactly | frame decode 1423 | 2200 |
| `array-uninit-i8` for results, parallel decode without verification, 8 threads | 6787 | 12364 |

## 3. Parallel: scaling on the concatenated Silesia (211 MB)

Frame compress and decompress, independent blocks, `Options.threads` = T, medians of 5; speedup against T = 1 in brackets. `fd`: decompress with verification (block, content and header checksums), `fdn`: with
verification off (`Options.verify` false), `fdp`: the frame has no recorded content size (the placement mode). For a reference: liblz4's CLI, `lz4 -1 -BI -B7 -T{1,2,4,8,16}` (file to /dev/null, which includes its
I/O threads): compress 636, 1184, 1695, 1385, 1254 MB/s; decompress 1796 whatever T (the CLI's decoder is single threaded).

| blocks | mode | T=1 | T=2 | T=4 | T=8 | T=16 |
|---|---|---|---|---|---|---|
| 64 KiB | compress | 623 | 1189 (1.9x) | 2146 (3.4x) | 3534 (5.7x) | 4802 (7.7x) |
| 1 MiB | compress | 681 | 1323 (1.9x) | 2365 (3.5x) | 3490 (5.1x) | 5185 (7.6x) |
| 4 MiB | compress | 705 | 1344 (1.9x) | 2435 (3.5x) | 3838 (5.4x) | 4910 (7.0x) |
| 64 KiB | decompress, verified | 2545 | 4726 (1.9x) | 8104 (3.2x) | 5846 (2.3x) | 5714 (2.2x) |
| 1 MiB | decompress, verified | 2571 | 4944 (1.9x) | 8107 (3.2x) | 5906 (2.3x) | 5628 (2.2x) |
| 4 MiB | decompress, verified | 2474 | 4970 (2.0x) | 7921 (3.2x) | 5979 (2.4x) | 5237 (2.1x) |
| 64 KiB | decompress, no verification | 2553 | 6136 (2.4x) | 10679 (4.2x) | 10784 (4.2x) | 10969 (4.3x) |
| 1 MiB | decompress, no verification | 2567 | 6397 (2.5x) | 10472 (4.1x) | 12364 (4.8x) | 11235 (4.4x) |
| 4 MiB | decompress, no verification | 2430 | 6494 (2.7x) | 11093 (4.6x) | 12259 (5.0x) | 9488 (3.9x) |
| 1 MiB | decompress, no content size (placement) | 2261 | 2735 (1.2x) | 3218 (1.4x) | 2920 (1.3x) | 2160 (1.0x) |

Python `lz4.block` (liblz4 through CPython, one block, development machine, loaded): dickens 456 MB/s compress, 1733 decompress; mozilla 648 / 1237.

**Targets and where they fall short.**

* Parallel compress at 8 threads: **5.1x to 5.7x** (target 6x); 7.0x to 7.7x at 16 threads (8 cores and their SMT siblings). The sequential parts are the allocation and zero fill of nothing (the buffers are
  `array-uninit-i8`), the header, the final copy of the blocks into the one result array (parallel above 8 MiB) and thread start: about 10 ms of 60. The rest is the machine: 8 threads land on 8 vCPUs of 16.
* Parallel decompress at 8 threads, verification off: **4.8x** (1 MiB blocks) and **5.0x** (4 MiB blocks): at the target for large blocks, a little under for small ones. What bounds it: memory bandwidth
  (each output byte is written once, each compressed byte read once, about 30 GB/s moved at 12 GB/s of output on this machine).
* Parallel decompress WITH the content checksum: **3.2x at 4 threads (8.1 GB/s), 2.3x at 8**: xxHash32 is sequential over the whole content and cannot be combined from per-block hashes, so it is
  pipelined (a worker that finishes a job hashes every finished job in order while the others decode) and bounded by one core of it (about 10 GB/s hot, 6 to 8 GB/s on data another core just wrote). It
  is still 3 to 4 times faster than liblz4's CLI decode (1.8 GB/s) with the same checks. A caller who has another way to know the data is sound sets `verify` false (5.0x).
* Without a recorded content size the result cannot be written straight into place (the block sizes are not known until they are decoded): the PLACEMENT mode decodes into per-job buffers and copies them
  (a second pass): 1.2x to 1.4x. Frames made by this library record the size by default.

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

| encoder | identical | compressed bytes ours / liblz4 |
|---|---|---|
| fast, acceleration 1, 2, 4, 16, 64 | **100%** (3937, 1233, 1308, 1266, 1228 of the same) | 1.0000 |
| HC 3, 6, 7, 8 | 100% | 1.0000 |
| HC 4, 5 | 99.8%, 99.7% | 1.0000 |
| HC 9 | 93.6% (liblz4 analyses repeating patterns above 128 attempts: not ported) | 1.0000 |
| HC 10, 11, 12 | 71%, 73%, 70% (liblz4 runs its OPTIMAL parser: not ported; ours is the lazy parser with a deeper chain) | 0.997, 0.993, 1.003 |

The fast encoder matches because it IS liblz4's algorithm: the five-byte hash into 4096 entries, the empty slot read as position 0 of a fresh table (liblz4's `LZ4_compress_default` starts from a zeroed table, so
index 0 is a candidate: the block API starts its table at that base), the skip acceleration, the "try the position after a match at once", the last five literals. This was found with a Python
simulation of `LZ4_compress_generic` (`scripts/lz4-sim.py`) that agrees with liblz4 on every one of 374 small inputs; the HC parser (`fib.compress.lz4.hc3`) is a port of `LZ4HC_compress_hashChain` and
`LZ4HC_InsertAndGetWiderMatch`. On Silesia-size blocks the sizes are identical for fast and HC 3 to 9 (aggregate table above); HC 3 on mozilla (51 MB) and samba (21 MB) is byte-identical.

## 5. Differential tests, hostile input, mutants, contract

* `scripts/lz4-diff.py lz4-diff 30000 31`: both directions against liblz4 1.10.0, 30,000 seeded inputs (text, runs, records, random, zeros, patterns, mixtures; frames at every level, checksum, block size,
  independence, content-size combination; blocks fast and HC): **60,000 checks, 0 failures**. `scripts/lz4-interop.sh`: `examples/lz4.fib` against the `lz4` CLI both ways, 8 files x 14 flag sets (including
  `-T4`): **112 combinations x 2 directions, 0 failures**. 122 frames and 40 blocks of the CLI's and liblz4's own output decode (`lib/fib/compress/lz4/vectors.fib`).
* Specs (`fibc test specs`): `compress-lz4-spec` (the contract, **17 scenarios**: the 15 of COMPRESS-1 plus "the result does not depend on the thread count" and "errors and limits do not depend on the thread
  count"), `compress-contract-shape-spec` (11 faults, each caught), `-edge-`, `-prop-`, `-safety-` (the decoder's slack proof with canaries, 1500 blocks x 2), `-xxh-` (1,000,000 random lengths, vector against scalar
  against streamed), `-par-` (14 scenarios: the engine, thread-count independence over random inputs and block sizes, jobs of 1, 2 and 5 blocks over 2 to 16 workers, 12-block hostile frames, 10,000 mutants,
  a 7 MiB stream over several batches, a dirty-heap run), `-hostile-` (200,000 mutants, 30 frames, 11 blocks, a bomb).
* `scripts/mutant-lz4.sh`: **32 planted faults, all killed**: the 20 of COMPRESS-1, two on the decoder's slack (output and input limits: the safety spec's canaries), a fast-path small-distance pattern, a wrong rotation in the
  vector xxHash, and eight in the parallel code (blocks written out of order, windows overlapping by one byte, a job claimed and never run, max-output not enforced under workers, thread-dependent output,
  an error in a worker swallowed, a worker trap that kills the process, the content checksum skipped). `scripts/mutant-uninit-i8.sh`: the builtin's length check removed: killed by case 8508.

## 6. Hello world

`fibc emit` of `(ns main) (defun main () -> i64 (do (println "hello") 0))`: the lIR is byte-identical (154,322 bytes) with this tree and without it (explicit modules; the one change outside the library is the
builtin `array-uninit-i8`, a row in the compiler's table).

## 7. Wasm

Cases 8470 to 8475 build with `--target wasm32-wasi` and return 0 under node's WASI (COMPRESS-1); the parallel cases need threads and are not run there (`spawn` is native only).
