# fib.compress.lz4 against liblz4 1.10.0 (COMPRESS-1)

Machine: the Ryzen 7 5800X (WSL2 Ubuntu, `ssh tsmar@192.168.7.83`, one core pinned with `taskset -c 3`, `ulimit -v 16000000`), binaries copied there, scratch only. Corpus: Silesia
(`silesia.zip`, sha256 `0626e25f45c0ffb5dc801f13b7c82a3b75743ba07e3a71835a41e3d9f63c77af`, 211 MB, 12 files, each compressed as ONE block). Reference: lz4 v1.10.0
(`537512904744b35e232912055ccf8ec66d768639ff3abe5788d90d792ec5f48b`) built with `gcc -O3` (generic x86-64, no SIMD intrinsics beyond SSE2), driven by `lz4bench` (`LZ4_compress_fast`, `LZ4_compress_HC`,
`LZ4_decompress_safe` into a buffer allocated once). Ours: `scripts/lz4-bench.fib` built with `FIB_TARGET_CPU=znver3 fibc build -O 3`. Medians of 5. Both from `scripts/fetch-lz4-tools.sh`.

Commands: `fibc build scripts/lz4-bench.fib -O 3 -o lz4-bench`; `lz4-bench MODE FILE LEVEL 5` and `lz4bench MODE FILE LEVEL` with MODE c (fast, acceleration LEVEL), h (HC), d (decode of the fast block),
dh (decode of an HC block); `scripts/` + `~/.cache/fibber-scratch/COMPRESS1/runbench.sh` loops over the files. Aggregate = total input bytes / total time over the 12 files.

| mode | ours MB/s | liblz4 MB/s | slower by | ours size | liblz4 size | target | |
|---|---|---|---|---|---|---|---|
| fast, acceleration 1 | 583 | 852 | 1.46x | 45.94% | 45.46% (+1.1%) | within 2x, ratio within 1% | speed met; ratio 1.1% (misses the 1% by 0.1) |
| fast, acceleration 4 | 655 | 1012 | 1.54x | 48.33% | 49.42% | | |
| fast, acceleration 16 | 873 | 1436 | 1.64x | 56.58% | 59.71% | | ours compresses better here |
| HC 3 | 139 | 139 | 1.00x | 38.05% | 36.44% (+4.4%) | within 2x | speed met; ratio +4.4% |
| HC 9 | 34 | 48 | 1.38x | 35.25% | 34.75% (+1.4%) | | |
| HC 12 | 11 | 16 | 1.52x | 35.17% | 34.45% (+2.1%) | | liblz4 runs its optimal parser |
| decompress (fast blocks, into a reused buffer) | 2528 | 5250 | 2.08x | | | within 1.5x, 1.5+ GB/s | **misses**: 2.1x; the 1.5 GB/s floor is met |
| decompress (HC blocks) | 3002 | 5259 | 1.75x | | | | |
| decompress into a fresh array each time (allocation, zero fill) | 2064 | | | | | | |

Per file, decompression ranges from 1.8x slower (osdb) to 3.3x (x-ray). **Where it falls short, and why.** A block that is one long literal run decodes at memcpy speed (8 MB of random bytes: 22.7 GB/s ours, 22.2 liblz4), so
the copy is not the gap: it is the cost of each sequence (token, length checks, offset check, two chunk copies), about twice liblz4's hand-tuned loop; x-ray and sao, which are many short sequences of incompressible
data, are the worst. What was done: unchecked arithmetic in the coders (a third of the speed: every `+` on an i64 traps on overflow), a common-sequence path with one set of checks, 16- and 32-byte vector copies.
What is left: a decoder that keeps the input and output positions in registers across the whole loop (the compiler spills `dlim` and `ilim`), and avoiding the packed `(i, op)` return of the fast path. There is no profiler
(perf is blocked); the analysis is of the assembly (`fibc build --emit asm`) and of the benchmarks above.

HC ratio: our parser is a hash chain with one-step lazy matching; liblz4's looks three matches ahead (3 to 9) and parses optimally (10 to 12): 1.4% to 4.4% larger output at the same level. Fast ratio: within 1.1%
at acceleration 1 (liblz4 uses a 16-bit table below 64 KB and a different position after a match; ours is its algorithm with a differently sized table).

## Streaming and frames (Ryzen, MB/s of input, medians of 5)

| file | block fast | frame fast (64 KiB independent blocks, content checksum) | stream compress + decompress in 64 KiB chunks | block decode, reused buffer | frame decode | frame HC 9 | block HC 9 |
|---|---|---|---|---|---|---|---|
| dickens | 376 | 347 | 207 | 1896 | 1269 | 37 | 24 |
| webster | 463 | 411 | 233 | 2264 | 1137 | 45 | 30 |
| mozilla | 669 | 552 | 278 | 2506 | 1264 | 35 | 29 |

The streaming column is a compress and a decompress through the protocols (copies of chunks into the window and out of it): both directions together, so each direction runs at about twice that. The frame decode is slower than
the block decode by the content checksum (xxHash32, about 4 GB/s) and the 64 KiB block loop. (Frame HC 9 is faster than block HC 9 because 64 KiB blocks keep the hash chains in cache.)

## Cross-check

python `lz4.block` (liblz4 1.10 through CPython) on dickens on the development machine, loaded: compress 436 MB/s, decompress 1710 MB/s, same compressed size as `lz4bench` (6428742 bytes) -- ours: 6432049.

## Differential tests (`scripts/lz4-diff.py`, python lz4 4.4.5 = liblz4 1.10.0)

`scripts/lz4-diff.py lz4-diff 30000 21`: 30,000 seeded inputs (markov text, runs, repeated records, random, zeros, patterns, mixtures; sizes around every format boundary and up to 400 KB), both directions: **60,000 checks, 0 failures**
(ours -> liblz4: fast blocks at acceleration 1 to 64, HC 3 to 12, frames at every level, checksum, block size, independence and content-size combination; liblz4 -> ours: frames at every python option, fast and HC blocks).
Earlier runs of 300, 6,000 and 4,000 inputs with other seeds also passed. `scripts/lz4-interop.sh`: our `examples/lz4.fib` against the `lz4` CLI both ways, 8 files (empty, 1 byte, 3 MB of zeros, random, text, dickens, xml, sao) x 12
flag sets: **96 combinations x 2 directions, 0 failures**. The 122 frames and 40 blocks of `lib/fib/compress/lz4/vectors.fib` (CLI flags, `-D` dictionary, legacy, concatenated, skippable) decode in `specs/compress-lz4-edge-spec.fib`.

**Byte-identical output** with liblz4 (stretch goal), on the same corpus (mostly small inputs, so these are favourable): fast acceleration 1: 60.9% (2316 / 3803); accelerations 2, 4, 16, 64: 62.4%, 61.8%, 69.5%, 72.9%; HC levels
3 to 12: 67% to 74%. On Silesia-sized blocks the output is valid and of the sizes in the table but not identical (the tables differ in size below 64 KB and HC parses differently). Non-identical output is valid LZ4 (liblz4 decodes it).

## Hello world

`fibc emit` of `(ns main) (defun main () -> i64 (do (println "hello") 0))` with `FIB_LIB` of main and of this branch: the lIR is **byte-identical (154,322 bytes both)**; the linked binaries are 28,216 and 28,224 bytes, differing in the
linker's embedded output path, not in code. Nothing here is in `fib.core` or the prelude.

## Wasm

Cases 8470 to 8475 (block bytes equal to liblz4's, frame options, hostile inputs, one-byte streaming, xxHash32, registry and dictionary) built with `fibc build --target wasm32-wasi` (wasi-sdk 34 from `scripts/fetch-wasm-tools.sh`) and run under node's WASI: all six return 0. The big specs were not run on wasm.
