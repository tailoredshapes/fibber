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

**Phase 2, SIMD structural indexing.** Implemented in `fib.json.stage1` and measured, **not integrated**: it does not win. Per 64 bytes it classifies with `(Simd i8 16)`
compares, builds the 64-bit quote, backslash and structural masks, removes escaped quotes with simdjson's odd-backslash addition, makes the in-string mask with the
prefix-XOR shift ladder, and extracts positions with a ctz loop. Its output equals a scalar reference at every alignment against the 64-byte blocks (`specs/json-simd-spec.fib`)
and on all three corpora. Speed: about 700 MB/s for the index alone, slower than the whole validating scalar tape (about 800 MB/s). What the language lacked:

| missing | workaround | cost |
|---|---|---|
| mask to bits (`movemask`, `(Simd bool n)` to an integer) | mask to 0/1 lanes (`simd/blend`), widen to i32x16 (`simd/convert`), multiply by 2^lane, `hsum` | 4.4 GB/s for one class when alone (quote compare plus this), but three classes per 16 bytes plus the ladder bring the stage to 700 MB/s |
| count trailing zeros | `popcount((x & -x) - 1)` | cheap (one popcount instruction on x86-64-v3 targets) |
| a 64-lane i1 vector | exists: `(Simd bool 64)` is legal (n is 1 to 64); a vector is at most 512 bits wide, so i8x64 does not exist and 16 or 32 bytes is the unit | four loads per block |
| carry-less multiply | shift/xor ladder of six steps | negligible |
| unsigned 64-bit compare, add with carry | sign-bit flip, `unchecked-add` and compare | negligible |
| shuffle-based nibble classification | `simd/eq` per character (6 compares for the structural class) | the compares are cheap; the mask-to-bits is not |

A `simd/movemask` builtin (lowering to `bitcast <n x i1> to iN`, which LLVM selects as `pmovmskb`) would be small (compiler/types/builtins.fib, compiler/emit/lower/simd*.fib, one row in
spec/types.md 1.9, cases 7980 to 7999) and would need a released seed before the library could use it. It is the one change I would make to the language for this library; I did not add it,
because without it the measurement says the structural index loses, and with it I could not measure it in this session.

## 4. Serialiser

Compact and pretty, into the growable buffer, linear. Integers: digit pairs from a 200-byte table, counted first and written backwards in place. Strings: one SWAR scan and one
`memcpy` per escape-free run (the common case is one `memcpy`); `ascii` escapes non-ASCII as `\uXXXX` with surrogate pairs. Floats: Schubfach (Giulietti; the JDK's `Double.toString` since 19),
`fib.json.dtoa` with the table of `fib.json.pow10` (generated by `scripts/gen-json-pow10.py`): shortest, closest, no libc. The language's own `str` of a float measured 6.9 microseconds per
float (it searches precision upward) and the first version here, `%.15g/16g/17g` with a strtod round-trip test, printed 4.94065645841247e-324 for the smallest subnormal (not shortest) and
cost two or three snprintf calls; Schubfach agrees with Python's `repr` digit for digit on 2 000 000 doubles. Layout: plain notation for decimal exponents -5 to 16 (`100.0`, `0.00001234`), else
`d.ddde-7` / `1e21`; always a `.` or an exponent so a float reads back as a float. NaN and infinities are `null`.

## 5. API

See the header of `lib/fib/json.fib`. Names that would shadow the core (`get`, `nth`, `str`, `read-file`) are `get-key`, `get-index`, `node-str`; `read-file` is the exception and is the
facade's own (`json/read-file`). Streams: `json/documents` (a text of several documents or NDJSON, a bad line is an `Err` and the next line goes on), `json/reduce-lines` (a file, 4 MiB chunks,
bounded memory; the language has no lazy seq over a reader, so a fold is the shape). `json/reduce-events` is a SAX fold over a tape: it holds the tape (16 bytes per value), so for input larger
than memory use `reduce-lines`. `JSON Pointer` (RFC 6901) and `get-in` (steps are keys; on an array a step is its decimal index, because a vector is homogeneous). `json/json->datum` and
`json/datum->json` convert to fib.datum (an array or object becomes its JSON text as a `DStr`: a Datum is scalar); `ToJson` (`json/to-json`) covers i64 f64 bool str keyword Datum `(Vec a)` `(Option a)`
and `defjson` records (`map->json` for string-keyed maps).

## 6. Tests

| what | where | result |
|---|---|---|
| JSONTestSuite (nst/JSONTestSuite 1ef36fa, 318 files): y_ parse, n_ rejected, i_ recorded; DOM and tape must agree | `scripts/json-testsuite.sh` (fetch + sha256 of every file: `scripts/fetch-json-testsuite.sh`, `specs/json-testsuite.sha256`); 48-file subset in the gate: `specs/json-testsuite-spec.fib` | y 95/95, n 188/188, i 35 (6 accepted, 29 rejected: `specs/json-testsuite-i.expected`) |
| edge cases: surrogates, -0, 1e400, 1E2, leading zeros, trailing commas, BOM, NUL, invalid UTF-8, depth | `specs/json-spec.fib` | pass |
| round trips with shrinking (random documents through every writer and reader) | `specs/json-prop-spec.fib` (`fib.json.gen`) | pass |
| floats: 1 M random decimal texts vs strtod; 1 M doubles written and read back; 259 940 edge cases (halfway points, nudged neighbours, subnormals, 17-digit ties) vs Python `float()`; 1 999 801 printed doubles vs Python `repr` | `scripts/json-floats.sh`; hard cases in the gate: `specs/json-floats-spec.fib` | 0 differ in each |
| fuzz: 100 000 mutated documents, no try/catch, DOM and tape agree | `scripts/json-fuzz.fib` | 88 763 rejected, 11 237 accepted, 0 traps, 0 disagreements |
| planted faults (14): wrong escape, SWAR byte class, off-by-one at a block boundary, quote mask after backslashes, in-string carry, float tie, Schubfach bound, Clinger limit, depth, tape depth, leading zero, overlong UTF-8, lone surrogate, duplicate key | `scripts/mutant-json.sh` | 14 of 14 killed |
| parse vs Python's json and Jackson on the corpus | `scripts/bench/json/` (same documents, outputs compared by the shootout's check column) | see the shootout |

## 7. Not done

Streaming SAX over input larger than memory with a tape-free scanner; a lazy seq of documents over a reader; a hash-indexed `JObj`; `JBig` arithmetic; `json/write-to` takes a file descriptor only
(not an arbitrary sink); the typed decoder reports a mismatch without a path; fib.log / fib.otel / fib.db / fib.http do not use fib.json yet (those directories were not touched).
