# 0010. `simd/movemask`, `ctz` and `clz` are builtins

Status: accepted
Date: 2026-10-06
Source: JSON-2 (docs/design/json.md, docs/shootout/json.md): fib.json stage 1 emulated the movemask by a widening multiply and summed it, 700 MB/s.

## Context

A structural scan of text (JSON, CSV, UTF-8 validation) compares a block of bytes against byte classes and needs the result as a bitmask in a general register,
where `ctz`, `popcount` and shifts work on it. The language had the vector compares (a mask) and `popcount`, but no way from a mask to an integer and no count of
trailing zeros: the first was emulated in four vector operations per 16 bytes, the second as `popcount((x & -x) - 1)` (three scalar operations instead of one `tzcnt`).

## Decision

Three builtins, no new lIR instruction for the first (a `bitcast` from `<N x i1>` to `iN` is already an lIR instruction and LLVM selects `pmovmskb`, `vpmovmskb`,
`kmov` or the NEON sequence for it) and two intrinsic names for the others (`cttz`, `ctlz`, class `i1` of `lir.intrin`, like `abs`):

- `(simd/movemask m)`: a mask of 1 to 64 lanes as an `i64`, lane 0 the lowest bit, zero-extended.
- `(ctz x)` and `(clz x)`: trailing and leading zero bits of an integer or of each lane of an integer vector; a zero has as many as the type has bits.

`(Simd i8 64)` already exists (512 bits is the limit; `i8x64` is its sugar): no change.

## Names

`movemask`, `ctz` and `clz` are not names of anything defined in `compiler/` or of a core form (checked with `grep -rw` over `compiler/`); the
only definition of `ctz` in the tree was `lib/fib/json/stage1.fib`, which this change removes. A program that defines its own `ctz` or `clz` is captured by the
builtin, as for any builtin name.

## Consequences

- Library code that uses them compiles only with a compiler that has them: the seed fibc 0.1.9 does not, so `fib.json`'s SIMD path needs the next release. The
  compiler itself does not use them, so the seed still builds stage 2 and the fixed point is not affected.
- The JavaScript backend implements `cttz`/`ctlz` (`$cttz`, `$ctlz`); it has no vector mask moves beyond what it had.
