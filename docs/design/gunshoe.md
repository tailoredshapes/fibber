# fib.gunshoe

Owner decision, 2026-10-31: explicit fixed arrays have no reader macro,
are imported from `fib.gunshoe`, and require `unsafe`. Other operations
whose correctness depends on developer-supplied memory obligations belong
in the same explicit module.

The initial module provides existing low-level operations. A statically
sized `FixedArray` type is separate work; this module does not add that type.

## Boundary

```clojure
(ns example (:require [fib.gunshoe :as gun]))

(defun main () -> i64
  (unsafe
    (let [p (gun/allocate 8)]
      (do (gun/write-i64! p 42)
          (let [value (gun/read-i64 p)]
            (do (gun/deallocate p) value))))))
```

The module is never implicit. Its API consists of ordinary macros, with no
new literal spelling or reader hooks. Calls require lexical `unsafe` at
the expansion site. They preserve ordinary type checking, ownership, and
the existing primitive's validation and COW behavior. They evaluate each
operand once, in source order.

Public macro names differ from runtime primitive names. No runtime
function or imported function is exported under the same name: taking a
macro as a value, passing it as a higher-order argument, or using `var`
must not find an unguarded function through namespace fallback.

Raw primitive aliases use `var` to avoid local variable capture. The
language's namespace alias rules still apply. An empty inline-LIR marker
keeps the caller's unsafe requirement even if `fib.prelude` is explicitly
rebound to another namespace. The marker has no runtime work after
optimization. Atomic macros emit the actual inline LIR at the caller;
gensyms capture operands before fixed LIR variable names are introduced.

This is a caller-side boundary, not an `unsafe` block hidden inside a
runtime wrapper. A library may build a safe abstraction around these
operations by establishing their preconditions inside its own `unsafe`.

## Initial surface

| Family | Operations |
|---|---|
| Raw storage | `allocate`, `deallocate`, `offset` |
| Object addresses | `borrow-address`, `retain-address`, `release-address!` |
| Scalar memory | `read-T`, `write-T!` for `i8 i16 i32 i64 ptr f32 f64` |
| Raw vector memory | `read-T`, `write-T!` for `f32x4 f64x4 f32x8 simd` |
| Uninitialized storage | `uninit-array-i8`, `uninit-array-f32`, `uninit-array-f64` |
| Unchecked Array SIMD | `read-simd-unchecked`, `write-simd-unchecked!` |
| Raw atomic loads | `atomic-read-ORDER-WIDTH`, ORDER `acquire relaxed seq-cst`, WIDTH `i32 i64` |
| Raw atomic stores | `atomic-write-ORDER-WIDTH!`, ORDER `release relaxed seq-cst`, WIDTH `i32 i64` |
| Raw atomic updates | `compare-exchange-i64!`, `fetch-add-i64!`, `exchange-i64!` |
| Ordering and target tools | `acquire-fence`, `release-fence`, `seq-cst-fence`, `pause`, `cache-line-bytes` |
| Address representation | `address->i64`, `i64->address` |
| Inline LIR | `inline`, with the existing `lir` operands and scalar/ptr ABI |

Raw storage needs a live, valid, sufficiently sized and appropriately
aligned address. `allocate` storage must be released with `deallocate`;
object addresses must not be released that way. `borrow-address` does not
transfer ownership; retained addresses transfer one counted reference,
which is released with `release-address!`. Raw writes must respect object
uniqueness and layout.

Uninitialized arrays must be initialized before reading or exposing their
contents to safe code. Unchecked SIMD requires a valid complete lane
range; the Array store still performs the existing uniqueness/COW work.
Raw atomics require natural alignment, valid lifetime, and a correct
publication and memory-ordering protocol. An integer encoding of an
address does not establish any of those properties.

Defined wrapping arithmetic, checked SIMD/register operations, and typed
`Atom` operations remain safe APIs. GPU and exception-runtime internals
keep their target/runtime-specific interfaces. Existing raw builtin and
`fib.atomic` spellings remain available for compatibility; the new module
does not silently change their historical contracts.

## Fixed arrays

The future fixed-array API follows this module boundary. Static extent,
inline storage and SIMD representation need separate contracts. The
earlier SIMD object-field specification/layout disagreement must be
resolved before establishing a public inline-array ABI. Nothing in this
module grants unchecked optimizer facts, alignment promises or noalias
assumptions from developer metadata.
