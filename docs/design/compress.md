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
file over 500 lines. The 50-line function rule holds except for `decode-block` and the specs' tables.

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

`fib.compress.contract/CompressContract` is a `defcontract` of **15 scenarios**: honesty of the capability list; round trip on 73 corpus inputs (empty, 1 byte, incompressible, all-zero, long runs, text, repeating
patterns, records, at every edge size around 12/13, 15/16, 255/256, 4096, 64 KiB, block sizes); round trip under every option the codec lists; determinism and the size bound; streaming equals one-shot for
chunkings of 1, 7, 4096, 65537 bytes and seeded random sizes; `max-output` exact (the size succeeds, one under is `:too-large`, one-shot and streamed); truncation at **every prefix** is an error, one-shot and
streamed; corruption (a flipped byte under a checksum is an error or the original, without one it never traps and stays within the limit); concatenated frames; empty input; a dictionary (smaller, round trips, needed);
decompressor steps bounded and `needs-input` honest; independent streams; magic numbers and detection; invalid arguments. A scenario that needs a capability the codec lacks is a skip row.

**What corruption is required to do.** LZ4 frames written with a content checksum (the default here) detect any change of the content, so a flipped byte is an error or decodes to the original (a flip in a
don't-care field). A frame without checksums may decode to different bytes: the contract then requires only that nothing traps and the output stays within `max-output`.

**Faults** (`fib.compress.fault/Faulty`, a decorator over a real codec). Each must make the scenario that names it fail, shown in `specs/compress-contract-shape-spec.fib`:

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

liblz4's `LZ4_compress_generic`: a multiplicative hash of the next four bytes indexes a table of positions (4096 entries from 64 KiB of input, 8192 below, 512 below 4 KiB; 4 bytes each); a candidate within 65535 bytes whose four bytes
equal is a match, extended backwards over the pending literals and forwards (eight bytes at a time with `ctz`) to the end less 5; the last 5 bytes are literals and no match starts in the last 12; the step grows by one every 64 failed
attempts (`acceleration` raises the start), and the position right after a match is tried at once. **The table is not cleared between blocks or frames**: positions are stored as `gen + index`, and a candidate is valid only
when `entry - gen` is at least the window start, so the next block (or frame) raises `gen` past every old position (a bump of the base offset, as liblz4's `currentOffset`), and a 16 to 32 KB table is never re-zeroed
(a real clear happens once in a billion bytes). A candidate must also be before the position (stale entries of a reused window), and its bytes are verified, so a bug here costs ratio, not correctness.

### 5.2 LZ4 HC (`hc`)

Levels 3 to 12: a hash chain over every position (32768 head entries and a 65536-entry table of 16-bit distances to the previous position of the same hash) searched to a depth of 4, 8, 16, 32, 64, 128, 256 (levels 3 to 9, liblz4's
table) and 512, 2048, 16384 (levels 10 to 12), with **one-step lazy matching**. This is not liblz4's parser: liblz4 looks three matches ahead at 3 to 9 and runs an optimal parser at 10 to 12, so its output is a
little smaller; the measured difference is in `docs/shootout/lz4.md`. Every match is verified against the bytes.

### 5.3 The decoder (`blockdec`)

One loop. A common-case path takes a sequence with a literal run of at most 14, a match of at most 18 and room in both buffers in a handful of compares (16-byte vector copies for the literals and for a match at distance 16
or more); everything else, and every sequence that fails one of those checks, goes the careful way and says why it failed. Matches at distance 8 to 15 copy 8 bytes at a time; below 8 the pattern is written out for the
first multiple of the period that is at least 8 and the rest copies from that distance. Arithmetic in the coders is `unchecked-add`/`unchecked-subtract` (checked i64 arithmetic cost about a third of the decode speed;
every index is bounded by array lengths well below 2^40, and the bounds checks themselves are comparisons, not arithmetic).

### 5.4 The frame format

`header` builds and parses the descriptor; `frame` and `framedec` are the one-shot coder (a one-shot compress of linked blocks costs nothing extra: the input is the window); `scomp` and `sdec` are the streaming ones, with
a window of 64 KiB of history ahead of the block, slid with `move-down`. Conformance is the table in `lib/fib/compress/README.md`.

## 6. Differential testing

* `scripts/lz4-diff.py` (python `lz4` 4.4.5, liblz4 1.10.0 as the oracle): ours must decompress with the reference, and the reference's must decompress with ours, on seeded random and structured inputs of every size around
  every boundary, blocks (fast, accelerations 1 to 64, HC 3 to 12) and frames (every checksum, block size, independence, content size and level): counts in `docs/shootout/lz4.md`.
* byte-identical output is a stretch goal: the percentage identical to `LZ4_compress_default` and to each HC level is measured and reported; non-identical output is valid and documented (`docs/shootout/lz4.md`).
* `scripts/gen-lz4-vectors.py` writes `lib/fib/compress/lz4/vectors.fib` from the lz4 v1.10.0 CLI (all flag combinations, `-D` dictionary, legacy, concatenated, skippable) and liblz4 blocks: published vectors as data.
* `scripts/lz4-interop.sh` runs `examples/lz4.fib` against the CLI both ways. `scripts/fetch-lz4-tools.sh` fetches and builds the CLI and the Silesia corpus with recorded checksums (ADR 0020).
* `scripts/mutant-lz4.sh`: 20 planted faults, each killed by a spec.

## 7. What is not here

Optimal parsing at HC 10 to 12, `LZ4_compress_destSize`, partial decompression, writing legacy or skippable frames, a streaming decoder for legacy frames, a dictionary for raw blocks (the frame API has it), multithreaded compression,
and a `ratio` limit option (a bomb is bounded by `max-output`, which a caller sets from what it expects). Dictionaries larger than 64 KiB use their last 64 KiB, as the format does.
