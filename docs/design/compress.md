# fib.compress and fib.compress.lz4 (COMPRESS-1)

Status: implemented and tested on this branch; numbers in `docs/shootout/lz4.md`. The user story: other teams need LZ4, and later zlib, gzip, zstd and
snappy, as a library: so the work is a compression **protocol** and a first codec, not an LZ4 binding.

## 1. Shape

Everything is an explicit module (the lesson of the prelude: a top-level definition in `fib.core` makes every program bigger). Hello world is byte for byte the same with and without this tree
(`docs/shootout/lz4.md` has the sizes).

| module | what |
|---|---|
| `fib.compress` | facade: `fib.compress.types` and `fib.compress.ops` |
| `fib.compress.types` | `CompressError`, `Options`, `DecStep`, the protocols `Codec`, `Compressor`, `Decompressor` |
| `fib.compress.ops` | `compress`, `decompress`, `max-compressed-size`, `supports?`, stream helpers over reader and writer closures, chunk helpers, the registry (`registry`, `detect`, `decompress-auto`) |
| `fib.compress.fd`, `fib.compress.http` | adapters: file descriptors; the HTTP `Decoder` seam and `Transport` (these need `fib.os`, `fib.http`; the core does not) |
| `fib.compress.contract`, `.checks`, `.corpus`, `.gen`, `.fault` | the codec contract, its inputs, the fault decorator |
| `fib.compress.lz4` | the `Lz4` codec (and the block API) |
| `fib.compress.lz4.*` | `mem` (raw byte access), `xxh32`, `blockenc` (fast), `hc`, `enc` (encoder state), `blockdec`, `block` (raw-block API), `header`, `frame` (one-shot compress), `framedec` (one-shot decompress), `scomp`, `sdec` (streaming), `sbuf`, `inspect`, `vectors` |

No `extern` anywhere (ADR 0011), no builtins added (ADR 0014/0015: `array-blit!`, `load-simd`/`store-simd`, `ctz`, raw pointer loads and stores are the existing ones), no global mutable state, and no
file over 500 lines. The 50-line function rule holds (ADR 0006 passes).

## 2. The protocol

A **codec is a value**. A program passes it to the code that needs compression; a library that needs compression takes one `Codec` parameter and stays independent of the format.

```clojure
(defprotocol Codec
  (codec-name (self) -> str)                          ; "lz4"
  (codec-capabilities (self) -> (Vec keyword))        ; :frame :block :block-size :levels :streaming :dictionary :checksum :content-size :linked-blocks
  (codec-magics (self) -> (Vec (Array i8)))           ; what a frame starts with, for `detect`
  (codec-max-compressed-size (self n) -> i64)
  (codec-compress (self data opts) -> (Result (Array i8) CompressError))
  (codec-decompress (self data opts) -> (Result (Array i8) CompressError))
  (codec-compressor (self opts) -> (Result (dyn Compressor) CompressError))
  (codec-decompressor (self opts) -> (Result (dyn Decompressor) CompressError)))
```

**Capabilities are a contract both ways**: a capability named must work, and an option for one not named must be an `unsupported` error, never a silent change (`ops/check-options` refuses it before
the codec sees it; the contract checks the codec too). **Errors are values**: `CompressError (kind message)`, the kind one of `:corrupt :truncated :too-large :unsupported :invalid-argument :checksum :io`;
a program branches on the kind. **Options** are one struct (`level acceleration dictionary checksum block-checksum block-size independent record-size size-hint max-output`) with `with-*` builders; a
field a codec has no use for is ignored at its default and refused otherwise.

**`max-output` is required, with a default of 256 MiB** (`default-max-output`). There is no unlimited. It is the decompression-bomb limit: the result is never larger, the error is `:too-large`, and nothing
beyond the limit is allocated (section 7). A caller that expects less says so; a caller that expects more must say so.

### 2.1 Streaming

`Compressor`: `compressor-feed chunk` returns the output that is ready (maybe empty), any chunk size including one byte; `compressor-finish` returns the rest and ends the stream; a feed after finish, or a second
finish, is an `invalid-argument` error. `Decompressor`: `decompressor-feed chunk` returns a `DecStep (output needs-input frame-end)`; **the output of one call is bounded** (at most one block: 4 MiB for LZ4, and never
past `max-output`), `needs-input` is false while the input already held can still give output (the caller feeds an empty array to pull), `frame-end` says a frame has just ended; `decompressor-finish` is `Ok` only at a
frame end with no byte left over: a cut stream is `:truncated`. Memory is bounded by the codec's block size plus the chunk the caller gives, not by the stream. `compress-stream` and `decompress-stream` run the loop
over a reader closure (empty array at the end) and a writer closure; `fib.compress.fd` and `fib.compress.http` supply the closures for fds and `Transport`s.

A stream object holds mutable state (it is a handle, like a `Hasher`): it is used by one task at a time and is not `Send`. Two streams from one codec value share nothing (a contract scenario).

### 2.2 Ownership of buffers

Inputs are borrowed (`(Array i8)` parameters); outputs are fresh arrays the caller owns. A coder takes `ptr`s into arrays that its caller holds for the whole call (`(raw a)`; the arrays are parameters, so they are
alive), never into a temporary. `block-decompress-into` writes only inside `dst[off, off+cap)` (the wild chunk copies are limited to that range). A decompressed result is one allocation when the size is known
(`block-decompress-sized`, a frame with a content size) and grows by doubling up to `max-output` otherwise; the final copy to an exact-size array is the only extra copy.

### 2.3 The registry, detection, and how other formats plug in

`(registry [(dyn Codec :send c) ..])` is a `Map str (dyn Codec :send)` the caller builds: no global table, no registration side effects. `(detect reg bytes)` returns the codec one of whose `codec-magics` starts the bytes;
`(decompress-auto reg bytes opts)` detects and decompresses. A driver (zlib, gzip, zstd, snappy: their own repositories, wrapping a C library or written in fibber) implements `Codec` (and the two stream protocols if
it has `:streaming`), lists its capabilities honestly, and runs `(implements CompressContract "name" (fn () <codec>))` in its own spec files: the contract is the acceptance test, and `Faulty` shows it can fail.
Design notes for them: gzip and zlib share a decompressor and differ in the header, so they are two codecs with a common back end; zstd has a dictionary ID and a content size in its header (`record-size`, `dictionary`);
snappy has frames (a stream identifier, CRC32C) and raw blocks; a format with no magic leaves `codec-magics` empty and `detect` never selects it. A format whose streaming decoder cannot bound its output per call must
not claim `:streaming`: the contract's step-bound scenario fails it.

## 3. The contract

`fib.compress.contract/CompressContract` is a `defcontract` of **18 scenarios**: honesty of the capability list; round trip on 73 corpus inputs (empty, 1 byte, incompressible, all-zero, long runs, text, repeating
patterns, records, at every edge size around 12/13, 15/16, 255/256, 4096, 64 KiB, block sizes); round trip under every option the codec lists; determinism and the size bound; streaming equals one-shot for
chunkings of 1, 7, 4096, 65537 bytes and seeded random sizes; `max-output` exact (the size succeeds, one under is `:too-large`, one-shot and streamed); truncation at **every prefix** is an error, one-shot and
streamed; corruption (a flipped byte under a checksum is an error or the original, without one it never traps and stays within the limit); concatenated frames; empty input; a dictionary (smaller, round trips, needed);
decompressor steps bounded and `needs-input` honest; independent streams; magic numbers and detection; invalid arguments; and, for a codec with `:parallel`, **the result does not depend on the thread count** (compress: the same bytes for 1, 2, 3, 7 and 16 threads, one-shot and streamed; decompress: the same output) and **errors and limits do not depend on it** (a corrupt block, a corrupt tail and a cut frame give one error kind, and `max-output` is exact, for every thread count), and **`verify` decides whether the checksums are checked, the same for every thread count** (a bad content checksum and a bad block checksum: the original with `verify` false, a `checksum` error with it true). A scenario that needs a capability the codec lacks is a skip row.

**What corruption is required to do.** LZ4 frames written with a content checksum (the default here) detect any change of the content, so a flipped byte is an error or decodes to the original (a flip in a
don't-care field). A frame without checksums may decode to different bytes: the contract then requires only that nothing traps and the output stays within `max-output`.

**Faults** (`fib.compress.fault/Faulty`, a decorator over a real codec; **11**). Each must make the scenario that names it fail, shown in `specs/compress-contract-shape-spec.fib`:

| fault | what it does | scenario that fails |
|---|---|---|
| `:ignore-max-output` | decompress gets the default limit instead of the caller's | max-output is enforced exactly |
| `:accept-truncation` | a truncated input is a success (empty); a cut stream finishes | truncated input at every prefix |
| `:drop-checksum` | compress records no checksum | corrupt bytes never trap (a corrupt frame decodes to wrong bytes) |
| `:corrupt-boundary` | an input of 64 KiB or more has a bit flipped in its output | a round trip gives the input back |
| `:reuse-state` | the second compressor of one codec value is the first, finished | two streams are independent |
| `:ignore-dictionary` | compress is given no dictionary | a dictionary makes text smaller |
| `:small-bound` | `max-compressed-size` answers half | the bound |
| `:overclaim` | lists `:rot13` | honesty |
| `:thread-dependent` | compress with more than one thread uses another acceleration | the result does not depend on the thread count |
| `:par-error-kind` | decompress with more than one thread answers `:internal` where the sequential decoder says `:corrupt` | errors and limits do not depend on the thread count |
| `:par-ignore-limit` | decompress with more than one thread gets the default limit instead of the caller's | errors and limits do not depend on the thread count |
| `:verify-sequential` | decompress with one thread verifies the checksums whatever `verify` says (the sequential decoder before DARWIN-2) | verify decides whether the checksums are checked, the same for every thread count |
| `:verify-never` | decompress never verifies, whatever `verify` says | verify decides whether the checksums are checked, the same for every thread count |

The shape spec also runs the contract against a codec that copies its input (no frame, no magic, no limit): it fails truncation, magic numbers, the empty input and the limit.

## 4. Security posture

* **A decoder is a parser of hostile data**; every length and offset is checked before it is used (`blockdec`): the literal run is inside the input, the offset is nonzero and no further back than the start of the
  window (the block, the frame, or the dictionary), the match fits within `max-output` (which is also bounded by the frame's block size), a length extension cannot overflow (it is at most 255 times the input length),
  an overlapping match repeats its pattern, and nothing is read outside the input or written outside `[lo, cap)` plus the chunk slack that the buffer really has. The wild copies run only when the buffer has room for
  the chunk, otherwise the exact path.
* **Frames**: reserved bits are `:corrupt`, an unknown version `:unsupported`, header, block and content checksums are verified whenever present, a block larger than the frame's block size is `:corrupt`, an unknown magic
  (including garbage after a frame) is `:corrupt`. A declared content size is checked against `max-output` BEFORE anything is allocated and against what the input could expand to (255 to 1), so a header that claims
  a terabyte over four bytes costs nothing. The output array grows by doubling, never past `max-output + 32`.
* **Streaming**: a unit (head, block, checksum) is processed only when all of it is there, so memory is bounded by the block maximum from the head (at most 4 MiB). A skippable frame is skipped without buffering it.
* **Bombs**: LZ4 cannot expand more than about 255 to 1, but 100 MB of zeros in 400 KB is real; `max-output` stops it (a spec decompresses it with a 1 MiB limit and expects `:too-large` from the one-shot, streaming and block decoders).
* **No global state, deterministic**: the same input and options give the same output; no codec state is shared between calls (the encoder tables are per call or per stream).
* **Evidence**: 200,000 mutants (bit flips, truncations, length-field, offset and header corruption, inserted and zeroed bytes) of valid frames of every kind end in an error or a result, never a trap or a hang,
  within the limit, and the one-shot and streaming decoders agree on every one; 30 hand-built malicious frames and 11 blocks each give their specific error.
* Not covered: timing side channels (LZ4 is not for secrets), compression oracle attacks such as CRIME and BREACH (compress secrets with attacker-controlled data at your own risk).

## 5. LZ4

### 5.1 The fast encoder (`blockenc`)

liblz4's `LZ4_compress_generic`, and BYTE-IDENTICAL to it (`docs/shootout/lz4.md` 4): a hash of the next FIVE bytes (`(seq << 24) * 889523592379 >> 52`) indexes a table of 4096 positions (16 KB, at every
input size); a candidate within 65535 bytes whose four bytes equal is a match, extended backwards over the pending literals and forwards (eight bytes at a time with `ctz`) to the end less 5; the last 5 bytes
are literals and no match starts in the last 12; the step grows by one every 64 failed attempts (`acceleration` raises the start), and the position right after a match is tried at once. The block API starts from a
table of zeros whose index 0 is the first byte, as `LZ4_compress_default` does (the one place the reference reads an empty slot as a candidate), so the bytes agree. **The table is not cleared between blocks or
frames**: positions are stored as `gen + index`, and a candidate is valid only when `entry - gen` is at least the window start, so the next block (or frame) raises `gen` past every old position (a bump of the base
offset, as liblz4's `currentOffset`), and a 16 KB table is never re-zeroed. A candidate must also be before the position, and its bytes are verified, so a bug here costs ratio, not correctness. The common sequence
(at most 14 literals, a match of 4 to 18) is emitted inline by `put-fast`.

### 5.2 LZ4 HC (`hc`, `hc3`, `hcpa`, `hcopt`)

Levels 3 to 12: a hash chain over every position (32768 head entries and a 65536-entry table of 16-bit distances to the previous position of the same hash) searched to a depth of 4, 8, 16, 32, 64, 128, 256
(levels 3 to 9, liblz4's table) and 96, 512, 16384 (levels 10 to 12). **The output is byte-identical to liblz4 1.10.0 at every level** (30,000 seeded inputs, `docs/shootout/lz4.md` 4; `specs/compress-lz4-hc-spec.fib` keeps
eight inputs at six levels as sizes and hashes of liblz4's blocks; case 8721).
* Levels 3 to 9: a port of `LZ4HC_compress_hashChain`: three matches are found one after the other (`wider`: a chain search that may extend a match backwards), and arranged as liblz4 arranges them (a short
  first match is dropped, a first match that a longer second one would squeeze is trimmed to 18, three ascending matches write the first and carry on with the other two). It is written as a machine over a small
  register file (`st`, 32 words) so that no function passes the 50-line limit. From level 9 (depth above 128) the search runs liblz4's PATTERN ANALYSIS (`hcpa`): when the chain step is 1 and the bytes at the
  position are a pattern of 1, 2 or 4 bytes, the search jumps over the run to its start or to the best aligned place instead of walking it byte by byte.
* Levels 10 to 12: a port of `LZ4HC_compress_optimal` (`hcopt`): at each position the longest match (`wider` in mode 1: pattern analysis and CHAIN SWAP, `LZ4HC_FindLongerMatch`), the prices in bytes of
  reaching each of the next positions by literals or by a match (a table of 4099 entries of {price, offset, match length, literal length} in the encoder's state words), a walk back from the end of the best path and
  the sequences written in order. Level 12 searches at every position, levels 10 and 11 only where the price rises; a match longer than 64 / 128 / 4095 is taken at once.
A match is only ever used after its bytes were compared, so a mistake costs ratio, never correctness. The parts that a size alone would not show (the prices, the skip test, the pattern jump, the chain swap) each
have a planted fault in `scripts/mutant-lz4.sh` that the HC spec kills.

### 5.3 The decoder (`blockdec`) and its safety proof

Two paths. The FAST path takes a sequence whose literal run is at most 14 and whose match is at most 18, when the input and output have room for the fixed-size vector copies; its safety proof is the **slack
condition**, `i <= ilim and op <= dlim`, written out in the file and checked by `specs/compress-lz4-safety-spec.fib` (canaries around the output range, bytes after the input that would be taken for an offset) and by
two planted faults (`slack-dlim`, `slack-ilim`):

* `ilim = min(ilen - 17, 2^31 - 1000)`: the token is at `i`, the 16-byte literal chunk reads `[i+1, i+17)` and the offset (at `i+1+lit <= i+15`) reads up to `i+17`: all inside the input;
* `dlim = min(dlen - 46, cap - 32, 2^32 - 1000)`: the literal chunk writes `[op, op+16)`; the match starts at `op2 = op+lit <= op+14` and its chunks write at most `[op2, op2+32) <= op+46 <= dlen`, and its 18 bytes
  end at `op2 + 18 <= op + 32 <= cap`: inside the buffer and below the cap. The wild bytes written past a sequence's end are inside the buffer after the output's end (overwritten by the next sequence or beyond the
  result); a caller whose buffer continues past the cap passes `dlen = cap`, which the proof then respects (this is what the parallel decoder does: `dlen` is the end of the block's window);
* the offset is checked by ONE unsigned compare: `1 <= off <= op2 - lo` (zero wraps to a huge number), so a match reads only bytes in `[lo, op2)` that were written.

Everything else, and every sequence that fails one of those checks, goes to `slow-seq`, which checks EVERY length and index and says which check failed (`-1` corrupt, `-2` truncated, `-3` over the cap). A length
sum cannot overflow (at most 255 times the input length). Arithmetic in the coders is `unchecked-add/subtract` (checked i64 arithmetic cost about a third of the decode speed; every index is bounded by array lengths
well below 2^40, and the bounds checks themselves are comparisons). Matches at a distance of 8 to 15 copy 8 bytes at a time; below 8 the pattern's first eight bytes are written one at a time and the rest copies
from the distance of the least multiple of the period that is at least 8.

### 5.4 xxHash32

The four accumulators of a stripe are one `(Simd i32 4)`: a stripe of 16 bytes is one vector load, a multiply, an add, a rotate (shift, shift, or) and a multiply with the lane arithmetic wrapping
(`unchecked-multiply` on vectors); about 10 GB/s, four times the scalar loop. `specs/compress-lz4-xxh-spec.fib` checks it against an independent scalar version and the streamed form on 1,000,000 random lengths and
contents, and against liblz4's checksums through the reference vectors; a planted fault (a different rotation) is killed.

### 5.5 The frame format, one-shot and streaming

`header` builds and parses the descriptor; `frame` and `framedec` are the one-shot coder (a one-shot compress of linked blocks costs nothing extra: the input is the window); `scomp` and `sdec` are the streaming
ones, with a window of 64 KiB of history ahead of the block, slid with `move-down`. **The content size is recorded by default** (`record-size` true: a one-shot compress knows it) so that a decoder allocates the
result once and exactly; a streaming compressor records it only when it is given `size-hint`. Conformance is the table in `lib/fib/compress/README.md`.

## 6. Differential testing

* `scripts/lz4-diff.py` (python `lz4` 4.4.5, liblz4 1.10.0 as the oracle): ours must decompress with the reference, and the reference's must decompress with ours, on seeded random and structured inputs of every size around
  every boundary, blocks (fast, accelerations 1 to 64, HC 3 to 12) and frames (every checksum, block size, independence, content size and level): counts in `docs/shootout/lz4.md`.
* byte-identical output is a stretch goal: the percentage identical to `LZ4_compress_default` and to each HC level is measured and reported; non-identical output is valid and documented (`docs/shootout/lz4.md`).
* `scripts/gen-lz4-vectors.py` writes `lib/fib/compress/lz4/vectors.fib` from the lz4 v1.10.0 CLI (all flag combinations, `-D` dictionary, legacy, concatenated, skippable) and liblz4 blocks: published vectors as data.
* `scripts/lz4-interop.sh` runs `examples/lz4.fib` against the CLI both ways. `scripts/fetch-lz4-tools.sh` fetches and builds the CLI and the Silesia corpus with recorded checksums (ADR 0020).
* `scripts/mutant-lz4.sh`: 20 planted faults, each killed by a spec.

## 7. Results

`docs/shootout/lz4.md` has the measurements: single-thread speed against liblz4 (fast 1.19x slower and byte-identical, HC 3 to 12 byte-identical, decode 1.49x, frame decode with the content checksum 1.10x of block decode), the scaling curves to 16 threads, the cutoffs,
the 2 GB streams and what bounds each (memory bandwidth; one core of xxHash32), and the assembly findings that drove the single-thread work.

## 8. Tools

`scripts/fetch-lz4-tools.sh` (the reference, the corpus, with checksums), `scripts/lz4-diff.{fib,py}` (both directions against liblz4), `scripts/lz4-sim.py` (a simulation of `LZ4_compress_generic` that agrees with it),
`scripts/gen-lz4-vectors.py`, `scripts/lz4-interop.sh`, `scripts/lz4-bench.fib`, `scripts/mutant-lz4.sh` (45 faults), `scripts/lz4-hc-vectors.py` (liblz4's HC sizes and hashes for `specs/compress-lz4-hc-spec.fib`), `scripts/mutant-uninit-i8.sh`.

## 9. Threads (`Options.threads`, the `:parallel` capability)

`Options.threads`: 0 (the default) is automatic (the machine's cores, but only for an input large enough to pay: LZ4 compresses in parallel from 2 MiB and decompresses from 512 KiB of compressed input, measured in
`docs/shootout/lz4.md` 3), 1 is sequential, n is n workers whatever the size (never more than the 64 of `fib.parallel.cpu`). **The result does not depend on it**: the same bytes, the same output, the same error kind
and the same exact `max-output` limit (contract scenarios 16 and 17, the property tests of `specs/compress-lz4-par-spec.fib`, planted faults). A codec without `:parallel` ignores the field. `Options.verify` (default
true) says whether a decoder checks the checksums that are in the data: the block checksums and the content checksum; the header checksum is always checked (the header is parsed before anything else is trusted). It is the same for every thread count and for the one-shot and the streaming decoder (contract scenario 18: a frame with a bad content checksum and one with a bad block checksum decode to the original with `verify` false and are a `checksum` error with it true, under 1, 2, 3, 7 and 16 threads; the faults `:verify-sequential` and `:verify-never` fail it; `scripts/mutant-lz4.sh seq-ignores-verify`). Before DARWIN-2 the sequential frame decoder ignored the field and the parallel one honoured it, so a frame with a bad checksum gave data with threads and an error without.

### 9.1 The engine (`fib.compress.par`)

The jobs are tasks of the runtime's work-stealing pool (`docs/design/parallelism.md` 8, `fork-task`): the engine makes no thread of its own, so the threads of a program stay the pool's (`cpu-count`, `FIB_THREADS`)
whatever `Options.threads` says and however deeply a program nests compression or decompression inside a `pmap` body (a joiner that waits runs other tasks: case 8720, the spec scenario of the same name, mutant
`par-nested-threads`, which puts `spawn` back and makes 12 bodies x 8 threads). `Options.threads` is the number of JOBS IN FLIGHT, at most the pool size (0: the pool size); a request for more only queues runners.
The 64-worker cap and the "no stealing pool yet" design are gone.
The engine forks `workers` RUNNERS and the JOBS `0 .. n-1` are SELF-SCHEDULED: every runner takes the next unclaimed index from one atom (a fetch-and-add), runs it, and comes back, so a slow job delays one runner and
the others take what is left. A job is `(fn (i) (Result r CompressError))`. A runner that gets an error says so and stops taking jobs; the others finish the job they are in and stop; EVERY task is joined whatever
happens (no task outlives the call). The error returned is that of the LOWEST failing index: jobs are claimed in increasing order and a claimed job always finishes, so every index below the first failing one has been
run, and the error is the one a sequential run meets first, whatever the worker count. A runner that TRAPS is joined with `try-join` (the pool's trap policy, ADR 0001/0009) and becomes an `:internal` error, not a dead
process. Results come back in index order. A job's output goes into an array its caller owns through an ADDRESS (`addr-of` / `ptr-at-addr`: a `ptr` is not `Send`; the array outlives the call and the ranges written are
disjoint: the caller's contract, stated where it is used).
**Why a counter and not a task per job.** The alternative was measured: one forked task per job, at most `workers` unjoined, joined in index order with the next job forked at each join (it makes the lowest-error rule
trivial). It loses at every thread count (compress, 1 MiB blocks, T = 2 / 4 / 8 / 16: 901 / 1645 / 2487 / 3506 MB/s against 1249 / 2364 / 3917 / 5350 for the counter; decompress without verification 3522 / 7402 / 9912 /
9973 against 6851 / 11614 / 12447 / 11643): the in-order join idles a slot whenever a later job finishes first and pays a wake-up per job. About 1 MiB jobs are coarse enough that the counter's imbalance is nothing.

### 9.2 Compression of independent blocks (`pframe`)

The input is cut into blocks as usual and the blocks into jobs of about 1 MiB; each job codes its blocks into a buffer of its own with an encoder of its own and returns the buffer; the content checksum (sequential by
nature) is job 0 and runs beside the others; the frame is then put together in order (the copy of the buffers into the one result is parallel above 8 MiB). The bytes do not depend on the worker count, or on how jobs
are cut: a block is coded from its own bytes alone (independent: its history is itself, `lo` is its start), so whichever worker codes it, and whether the table carried entries from an earlier block or not, the bytes
are the same; the sequential `frame-compress` is the same function over the same blocks. Linked blocks, a dictionary and the streaming compressor without threads stay sequential (a linked block cannot start before
the one before it ends; priming each job with a dictionary would keep the output independent of the workers but is not done).

### 9.3 Decompression of independent blocks (`pframedec`) and streams

The head is parsed and the block headers scanned sequentially (4-byte size words: O(blocks)) into an index; the jobs decode blocks in parallel. DIRECT mode (the frame records its content size): the result array is
allocated once, exactly, with `array-uninit-i8`, and block k decodes straight into its window `[k * bmax, (k+1) * bmax)`, with `decode-block` given the window's end as `dlen` (the slack proof then keeps even its wild
copies inside the window: disjoint windows). The layout is a guess that every block decodes to exactly `bmax` bytes; a block that does not makes the attempt give way to the sequential decoder. PLACEMENT mode (no
recorded size): each job decodes into a buffer of its own and the buffers are copied into the result (a second pass). EVERY failure of the attempt hands the frame to the sequential decoder, which is what makes the
error kinds identical to the sequential decoder's. `max-output`: direct mode checks the declared size before anything is allocated; placement mode bounds the work by the sum of the stored/maximum block sizes (at most
`max-output` plus one block) before any buffer is made and enforces the exact limit when placing: **a parallel decode never allocates more than `max-output` plus one block**. Block checksums are verified in the jobs;
the content checksum is pipelined (a worker that finishes a job takes the checksum's lock if free and hashes every finished job in order), so it is bounded by one core of xxHash32 and not by the whole decode; it
cannot be parallelised: xxHash32 of a concatenation is not a function of the hashes of its parts (two 16-byte inputs with the same hash give different hashes once the same bytes follow them: `docs/shootout/lz4.md` 3), and the
state after a block is the input of the next, each lane being v' = rotl(v + x P2, 13) P1, which has no closed form over a block. A dedicated hasher (one runner that hashes every finished job at once and decodes a job
itself when none is ready) was built and measured and is NOT faster than the lock scheme: under load the hash of data that other cores wrote runs at 6 GB/s, not 10.8. `verify` false skips it. The SEQUENTIAL frame
decoder instead hashes the previous block while it decodes the next (section 9.4). Concatenated frames, linked blocks, dictionaries and legacy frames decode
sequentially.

**Streams.** With `threads` of 2 or more (not automatic: the length of a stream is not known), independent blocks and no dictionary: the compressor holds input until it has a BATCH (workers x about 1 MiB), codes the
batch in parallel, and returns its blocks in order; the decompressor, finding at least two whole blocks queued, decodes up to that many in parallel and returns their output in one step (a step is then bounded by
workers x about 1 MiB of output instead of one block), consuming nothing unless it succeeds. Memory is a few MiB per worker, never the length of the stream. `fib.compress.fd` overlaps the I/O when threads are asked
for: a reader task reads ahead and a writer task writes behind through bounded queues (the producer sleeps while the queue is full; both tasks are always joined).

### 9.4 The sequential decoder hashes while it decodes

xxHash32 is a chain of dependent multiplies (about 7 cycles per 16-byte stripe whatever the cache: 10.8 GB/s hot or cold) and the decoder leaves most of a core idle, so the one-thread frame decoder (`framedec`) keeps the
output range of the block just decoded as PENDING and `decode-block-h` (`blockdec`) hashes one stripe of it in every turn of the decode loop, the two chains overlapping in the core's window; what is left goes to the
streaming hash after the block. A stored block, a dictionary, or a held-back tail in the hash first hashes the pending range plainly, so the bytes reach the hash in order whatever the path. The plain `decode-block` is the same
function with nothing to hash. Frame decode with the checksum, 64 KiB blocks, one thread: 2686 -> 3305 MB/s (block decode 3620).

## 10. A builtin: `array-uninit-i8`

`(unsafe (array-uninit-i8 n))` is an `(Array i8)` of n bytes whose contents are not defined, the byte twin of `array-uninit-f32/f64` (spec row `spec/syntax.md` 3.15, cases 8506 to 8508, mutant
`scripts/mutant-uninit-i8.sh`, ADR 0014). It removes the zero fill of a result: `memset` of 211 MB is 12 ms, a third of a parallel decode. Every use writes each byte it will return before it returns it (the decoders
return an array in which they wrote every byte, or a copy of the part they wrote), and `specs/compress-lz4-par-spec.fib` runs the decoders after filling the heap with 0xAA to prove it.

## 11. What is not here

`LZ4_compress_destSize`, partial decompression, writing legacy or skippable frames, a streaming decoder for legacy frames, a dictionary for raw blocks (the frame API has it), parallel linked blocks, parallel concatenated frames (one frame at a time is parallel), parallel decode without a recorded size at full speed (placement is 1.2 to 1.4x),
and a `ratio` limit option (a bomb is bounded by `max-output`, which a caller sets from what it expects). Dictionaries larger than 64 KiB use their last 64 KiB, as the format does.
