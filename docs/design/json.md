# fib.json

Status: implemented (JSON-1). Library: `lib/fib/json.fib` (the facade, `(:require [fib.json :as json])`) over `lib/fib/json/*.fib`. Numbers
and commands: `docs/shootout/json.md`. Tests: `specs/json-*.fib`, `scripts/json-*.sh`, `scripts/mutant-json.sh`.

The language needed no new builtin. Everything below was measured on this tree's stage 2 with the released v0.1.8 seed's `fibc` building
the compiler, so the library needs no release first. Measurements quoted here come from the shared 28-core box, which was at load 12 to 30
while they ran: ratios between two variants of the same program are trustworthy, absolute MB/s are not (the final table is the Ryzen box).

## 1. Data model

```
(defenum Json (JNull) (JBool b) (JInt i: i64) (JFloat f: f64) (JBig text: str) (JStr s) (JArr items: (Vec Json)) (JObj keys: (Vec str) vals: (Vec Json)))
```

- **Numbers.** An integer literal that fits i64 is `JInt`; any other number is `JFloat` (strtod-exact). Option `numbers` 1 keeps an integer outside
  i64 as `JBig` text, 2 keeps every non-i64 number as text. `-0` is `JInt 0`, `-0.0` is `JFloat -0.0`. A number that overflows a double (`1e400`) is
  an error (code 12) unless `out-of-range` is set (then infinity); underflow is 0 or a subnormal as strtod. Why error: JSON has no infinity,
  so a value that cannot be written back is better refused at the door; the JSONTestSuite `i_` files that exercise this are recorded.
- **Objects** are two parallel vectors in source order. Lookup is a scan of `keys` (`json/get-key`). Duplicate keys: `duplicates` 0 (default) last
  value wins at the key's first position, 1 keep every pair, 2 error. While parsing, an object with up to 16 keys finds duplicates by scan; the 17th key
  builds a `(Map str i64)` index, so a hostile object of a million keys is linear (a scan would be 10^12). The finished `JObj` carries no index:
  lookup by scan stays the rule for the values users read (objects of up to a few dozen keys); a caller that looks up in a big object builds its
  own map (`(zipmap keys vals)`), because an index inside the value would be paid by every parse and every `=`.
- **Strings** are `str`, so valid UTF-8 always: `str-from-bytes` traps on invalid bytes, `read-file` answers nil. Bytes therefore enter through
  `parse-bytes` / `json/read-file`, which validate (fib.json.utf8: strict RFC 3629, ASCII eight bytes at a time) and answer code 6 with the offset.
  A lone surrogate escape (`"\ud800"`) cannot be a `str`: it is an error (code 9). BOM: error (code 13) unless the `bom` option skips it.
- **Errors are values**: `JsonError` code, message, byte offset, line, column (found by one scan, only on the failing path). The input never
  traps a reader: `scripts/json-fuzz.fib` runs 100 000 mutated documents with no try/catch.
- **Depth** limit 1024 by default (`max-depth`); size limit 1 GiB (`max-size`). 1 000 000 nested `[` returns code 7. The DOM parser recurses (one
  frame per level, so the limit is also its stack bound); the tape builder keeps an explicit stack and costs a word per level.

## 2. Two fronts

(a) `json/parse`: the DOM, recursive descent over the `str` through `(raw s)` loads (24 bytes of header, rt/str.lir), position and failure code in
in/out cells so a value costs no tuple and no `Result`.

(b) `json/tape`: one validating pass builds a tape (`fib.json.tape`): two i64 words per value, in order: kind and start offset, then the end offset
(strings, numbers) or the index after the container (so a sibling is one jump). Nothing else is allocated; strings and numbers are read from the
text when asked (`node-str` slices, `node-i64`/`node-f64` convert; `field`, `elem`, `path` walk by skip pointers). The tape holds a whole valid document: the
grammar, escapes (including surrogate pairing) and depth are checked while it is built. From it come: `node->json` (a subtree as `Json`), `reduce-events` (SAX),
and the typed decoder. A tape is not simdjson's: it is built by a scalar state machine, not from a structural index (section 3).

(c) **Typed records.** `(jc/defjson Person [name: str age: i64 tags: (Vec str) boss: (Option Person)])` writes the struct, its `ToJson` instance, and
`decode-Person` / `json->Person`, which read the tape directly: a field is found by key and converted into the record, no `Json` exists. Types: i64 f64
str bool `(Vec T)` `(Option T)` and other `defjson` records. A mismatch is `Err` code 16. It is a macro over a field list rather than a `derive` over an existing
struct because macros see syntax, not types (`defrecord` is built the same way).

## 3. Speed architecture

Findings that shaped it (each measured, commands in `docs/shootout/json.md`):

- `subs` is O(n) per call (it counts characters): a parser built on it took 26 s for twitter.json. `str-slice` takes byte offsets and is O(length).
- `array-push!` copies once the array has 32 elements or more (spec/types.md 2.13.1): 580 ns per push at 1 M pushes, quadratic beyond. So the serialiser's
  buffer is an `(Array i8)` with a length cell and a `memcpy` doubling (`fib.json.buffer`); `os/read-file-bytes`, which appends with `array-push!`, is avoided
  (`json/read-file` is the builtin `read-file`, with a chunked fd read only for its failure path).
- `/` on integers is a Ratio; `quot` and `rem` are the integer operations.
- A bounds-checked `str-byte-at` per byte against an unchecked raw load: DOM parse of citm_catalog 156 to 412 MB/s, canada 67 to 176 (with Eisel-Lemire), twitter 98 to 130.
  The unsafe is confined to `fib.json.number/at`, `fib.json.buffer`, `fib.json.escape` and the tape; every index is checked against `n` before the load.
- Reading eight bytes at a time (SWAR: bytes below 0x20, `"` and `\` found by the exact has-byte-less / has-byte-equal word tricks) in the string scans.

**Numbers.** Clinger's exact path (mantissa below 2^53, power of ten up to 22), then Eisel-Lemire with a 128-bit power-of-five table (`fib.json.pow5`, generated by
`scripts/gen-json-pow5.py`; no 64x64 to 128 multiply exists in the language, so `mul128` builds it from 32-bit limbs, and unsigned compares flip the sign bit),
then libc `strtod` on a NUL-terminated copy (long mantissas, near-ties it cannot decide). Verified bit for bit against strtod and against Python's `float()`
(section 6).

**Phase 2, SIMD structural indexing (JSON-2, integrated).** The language got `simd/movemask`, `ctz` and `clz` (ADR 0010; the 64-byte `i8x64` already existed, 512 bits is the limit),
and `fib.json.stage1` classifies 64 bytes per step: byte compares on an `(Simd i8 64)` (LLVM splits it to the target's width), each turned into a 64-bit mask by one `simd/movemask`,
escaped quotes removed with simdjson's odd-backslash addition, the in-string mask by the prefix-XOR ladder, positions written by `ctz`, four at a time. The index holds, in order,
every unescaped quote, every `{ } [ ] : ,` outside a string and the first byte of every other token (a number or a literal); a control byte inside a string and a string still open at the end
are flagged. It is malloc'ed (no zero fill, `realloc` growth). Alone it runs at 4.9 GB/s on the Ryzen (1.3 GB/s with the default allocator: first-touch page faults, docs/shootout/json.md), against 700 MB/s when the mask-to-bits
was emulated. It equals a scalar reference at every alignment against the 64-byte blocks (`specs/json-simd-spec.fib`).

`fib.json.tapefast` is stage 2: it walks only the index (a string is its two quote entries, a number is scanned eight digits at a time, a comma costs a compare), builds the same tape as
the scalar builder, and on ANY failure answers "use the scalar builder" (`fib.json.tapebuild` then finds the same failure and gives its exact code and offset), so an error message cannot
change and a bug in the fast path can only make a valid document slow or let an invalid one through; `specs/json-fast-spec.fib` compares the two builders (same verdict, same tape word for
word) on the suite subset, random documents and their one-byte mutations. Below 512 bytes the scalar builder runs (nothing to amortise); targets without 256-bit vectors still run the same
code (LLVM legalises the vectors), a wasm or JS target gets the scalar builder only if a `has-simd` gate is added: not done.

`fib.json.domfast` builds the DOM from that tape (3 allocations fewer per container: the child count is read off the tape, so a Vec is made at its exact size by `vec-from-array`;
integers of up to 18 digits are read in place; floats go to `finish-float` with the mantissa already gathered; duplicate keys are found by comparing slices of the text, no `str` is
made; a key cache of 256 slots shares the `str` of repeated keys), and falls back to the recursive-descent parser on any failure. Strings are NOT zero-copy: a `str` is an owned
array in this language, there is no slice type, so `str-slice` copies; a document-lifetime view would need a new type (an ownership question, not a library one).

**Lines.** `fib.json.lines` finds newlines 32 bytes at a time and parses each line (the DOM builder for lines of 512 bytes and more, else the scalar parser) sequentially or with
`pmap-range` on all cores; errors are placed in the whole text. `validate-lines` runs the tape builder per line in parallel.

| parse vs Python's json and Jackson on the corpus | `scripts/bench/json/` (same documents, outputs compared by the shootout's check column) | see the shootout |

## 7. Not done (after JSON-2)

Streaming SAX over input larger than memory with a tape-free scanner; a lazy seq of documents over a reader (`reduce-lines` and `reduce-lines-par` fold; neither is a seq); a hash-indexed `JObj`
(an index would have to live in the type: `JObj keys vals` is two vectors, lookups scan); zero-copy strings (no slice type); a `defjson` encoder that writes straight to the buffer without a DOM;
`has-simd` gating for wasm and JS targets; `JBig` arithmetic; `json/write-to` takes a file descriptor only (not an arbitrary sink); fib.log / fib.otel / fib.db / fib.http do not use fib.json yet (those directories were not touched).
