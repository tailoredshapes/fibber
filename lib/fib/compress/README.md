# fib.compress and fib.compress.lz4

`fib.compress` is the **protocol** for compression: a `Codec` value with a name, capabilities, one-shot `compress` / `decompress`, and
streaming `Compressor` / `Decompressor`. `fib.compress.lz4` is the first codec, written in fibber (no extern, no C library): the LZ4 block
format and the LZ4 frame format, with the fast encoder, LZ4 HC (levels 3 to 12), dictionaries, and a decoder that is safe on hostile input.
Other formats (zlib, gzip, zstd, snappy) are drivers in their own repositories that implement the same protocol: `docs/design/compress.md`
says how. Everything here is an **explicit module**: nothing is in `fib.core` or the prelude, so a program that does not ask for it does
not carry it (hello world is the same size with and without this tree).

```clojure
(ns main (:require [fib.compress :as z] [fib.compress.lz4 :as lz4]))

(def codec (lz4/lz4))
(def opts (z/default-options))                       ; level 1, content checksum on, 64 KiB independent blocks, max-output 256 MiB

(match (z/compress codec (str-bytes "hello hello hello hello") opts)
  ((Ok frame) (match (z/decompress codec frame opts) ((Ok bytes) ..) ((Err e) ..)))
  ((Err e) (println (. e kind) (. e message))))        ; kinds: :corrupt :truncated :too-large :unsupported :invalid-argument :checksum :io
```

## One shot

| call | gives |
|---|---|
| `(z/compress codec bytes opts)` | `(Result (Array i8) CompressError)`: an LZ4 frame |
| `(z/decompress codec bytes opts)` | the bytes, or an error; never more than `(. opts max-output)` |
| `(z/max-compressed-size codec n)` | the most `compress` can return for `n` bytes (preallocate with it) |
| `(z/supports? codec :dictionary)` | whether the codec lists a capability |

Options: `(with-level o 9)` (1, 2: fast; 3 to 12: HC; 0 and below: fast with acceleration `1 - level`), `(with-acceleration o 8)`,
`(with-dictionary o bytes)`, `(with-checksum o false)` (content checksum), `(with-block-checksum o true)`, `(with-block-size o 262144)`
(64 KiB, 256 KiB, 1 MiB, 4 MiB), `(with-independent o false)` (linked blocks: better ratio), `(with-record-size o true)` (the content size goes
in the header), `(with-max-output o n)`. `max-output` is a **limit on the result**: more is a `:too-large` error and nothing past it has been allocated,
so a decompression bomb costs at most `max-output`. The default is 256 MiB; there is no "unlimited".

## Streaming, bounded memory

```clojure
(let [k (z/codec-compressor codec opts)]               ; (Result (dyn Compressor) CompressError)
  ;; (z/compressor-feed k chunk) -> output bytes ready (maybe empty); any chunk size, 1 byte is fine
  ;; (z/compressor-finish k)     -> the rest: end mark and content checksum
  ..)
(let [k (z/codec-decompressor codec opts)]
  ;; (z/decompressor-feed k chunk) -> DecStep (output needs-input frame-end): output is at most one block;
  ;; while needs-input is false feed an empty array to get more. (z/decompressor-finish k) -> Ok only at a frame end.
  ..)
```

`(z/compress-stream codec opts read write)` and `(z/decompress-stream ..)` do the loop for any reader and writer closures; `fib.compress.fd` has
`compress-fd` / `decompress-fd` over file descriptors (files, pipes, sockets, stdin); `fib.compress.http` makes a codec an HTTP `Decoder`:

```clojure
;; compress a file to .lz4 and back, 64 KiB at a time
(match (files/open-file "in.txt" ..) ((Ok in) (match (files/open-file "in.txt.lz4" (with ... (create true) (truncate true))) ((Ok out) (compress-fd codec opts in out)))))
;; over a socket: (compress-stream codec opts (transport-reader t) (transport-writer t))
;; as a Content-Encoding decoder:
(-> (client-options) (with-decoder (fib.compress.http/decoder "lz4" (dyn z/Codec :send codec))))
```

`examples/lz4.fib` is a complete `lz4`-compatible command (`lz4f -9 -BX in out`, `lz4f -d in.lz4 out`); `scripts/lz4-interop.sh` runs it against the
reference `lz4` both ways.

## Registry and detection

```clojure
(def reg (z/registry [(dyn z/Codec :send (lz4/lz4))]))   ; a Map the caller builds: no global table
(z/detect reg bytes)                                     ; the codec whose frame magic starts the bytes, or nil
(z/decompress-auto reg bytes opts)                       ; detect, then decompress; :unsupported when none matches
```

## Raw blocks

The block format has no header and no length: you keep the sizes. `(lz4/block-compress bytes accel)`, `(lz4/block-compress-hc bytes level)`,
`(lz4/block-decompress bytes max-output)` (the size is not known: the buffer grows up to the limit), `(lz4/block-decompress-sized bytes size)` (one allocation),
`(lz4/block-decompress-into bytes dst off cap)` (into your buffer: no byte outside `dst[off, off+cap)` is written), `(lz4/compress-bound n)`.

## LZ4 conformance

| feature | status |
|---|---|
| block format (token, extensions, offsets to 65535, last-5-literals and 12-byte end rules, minimum match 4) | encode and decode |
| frame format v1.6.x: magic, FLG/BD, version 1, reserved bits checked (corrupt), block sizes 64 KiB to 4 MiB | encode and decode |
| independent and linked blocks | encode and decode (one-shot, streaming) |
| block checksum, content checksum (xxHash32), header checksum | written on request, verified whenever present |
| content size | written on request (`record-size`; streaming needs `size-hint`), checked on decode, and refused when larger than `max-output` or than the input can expand to |
| dictionary (last 64 KiB) | prefix of the window of every block (independent) or of the first (linked); the dictionary ID field is read, not used |
| concatenated frames | decode (one-shot and streaming) |
| skippable frames (0x184D2A50 to 5F) | skipped on decode; never written |
| legacy frames (0x184C2102) | **one-shot decode only** (the streaming decoder answers `:unsupported`); never written |
| LZ4 HC levels 3 to 12 | hash chain with lazy matching; NOT liblz4's parser (no optimal parser at 10 to 12): see docs/shootout/lz4.md for the ratio |
| `LZ4_compress_destSize`, partial decompression, `LZ4_setStreamDecode` API | not provided |

## Tests

`specs/compress-lz4-spec.fib` (the codec contract), `-edge-` (published vectors from the reference CLI, boundaries), `-prop-` (properties with shrinking),
`-hostile-` (malicious frames, a bomb, 200,000 mutants), `specs/compress-contract-shape-spec.fib` (the contract can fail: faults); `scripts/lz4-diff.py` (both
directions against liblz4 on thousands of inputs), `scripts/mutant-lz4.sh` (planted faults), `scripts/lz4-interop.sh` (the CLI), `scripts/lz4-bench.fib`.
