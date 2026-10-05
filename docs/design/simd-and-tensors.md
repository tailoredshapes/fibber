# SIMD vectors and `fib.tensor`

Status: design, **revised 2026-10-04 with the owner's six decisions** (the box below; each is applied in the section it
concerns, and section 7 is closed). Nothing here is implemented; no `.rs` or `.fib` source changed. Every statement about the existing
compiler cites the file it was read in. Every number marked *measured* was produced in this session on the dev machine
(Intel i7-14700KF, AVX2 and FMA, no AVX-512, one thread, a shared box: **indicative, not final**; the runs that time
something took `/tmp/fibsuite.lock`). Everything marked **Proposed** is a decision for the owner. Scratch programs are in
`~/.cache/fibber-scratch/SIMD-D/` (`dot.lir`, `v5.lir`, `mm/`).

The owner's request: "I also want to make sure we're using SIMD where we can; native tensors would be a big win."

Contents: owner decisions; 0 evidence and what exists; 1 relation to the original design; 2 Part 1, SIMD; 3 Part 2, `fib.tensor`;
4 cases and benchmarks; 5 implementation plan; 6 risks; 7 questions for the owner.

---

## Owner decisions of 2026-10-04 (binding)

1. **Literal spelling.** `<<1.0 2.0 3.0 4.0>>`, the element type a suffix after the closer (`<<0.1 0.2>>f32`), any lane count
   1 to 64 (up to 512 bits). ADR 016's `<f32<..>>` is **omitted**, not kept as an alternative: it costs a second reader state
   (its opener `<f32<1.0` is one symbol today and its closer is asymmetric), and the owner accepted the suffix for the
   reader's sake (2.3).
2. **Broadcast: literals and `splat` only** (revised the same day; the first answer was "ADR 016's rule", a variable broadcasts
   when the vector's type is known). A scalar **literal** operand of a vector operator broadcasts and adopts the vector's element
   type (`(+ 5 <<1 2 3>>)`, `(* 2.0 v)`), extending the rule by which an integer literal in an argument position adopts the
   numeric type it unifies with (`spec/stdlib.md` L19). A scalar **variable or any other expression** does not: write
   `(splat x)`. There is no deferred constraint and no inference-order dependence in the checker; `(+ x v)` with a scalar `x` is
   an error whose text says to use `splat`. Rationale: a simpler checker, no surprises from the order in which inference
   learns a type, and ADR 016's ergonomics kept for the common case, the literal. A mismatched element type is an error
   (fibber has no implicit numeric promotion: **ADR 017 stays dropped**; the spec says so, 1 and 2.4).
3. **Writable views of a unique tensor are worth designing**: a scoped exclusive borrow as a language feature. Package D2
   wrote it: `docs/design/exclusive-views.md` (3.4 below summarises, 5 lists the package).
4. **Integer vector arithmetic is checked by default**, like scalar arithmetic; `wrapping` (ADR 015) is the opt-out; `unchecked-*`
   are provided as `spec/stdlib.md` L10 specifies (2.8).
5. **Rule 6 (interpreter against compiler) no longer judges new features** (the Rust is frozen). A *fibber interpreter* will be
   written (package INT0); until it exists each SIMD operation is tested against a **scalar reference written in the case
   itself** (4.1).
6. **The other liar ADRs have more material; it is used**: lane operations and horizontal reductions (2.6), `vec-get`/`vec-set`/
   `vec-shuffle` (2.6), the rationale for dropping ADR 017 (1), the `<[..]>` idea of ADR 018 against fibber's unique in-place
   update (1), ADR 015's `wrapping` (2.8), and the lIR vector scenarios of `cert/features/vector_types.feature` as test inputs (4.1).

---

## 0. Evidence and what exists today

### 0.1 Today's compiler does not use SIMD (the lead's count, v0.1.5 seed)

| Kernel | packed FP instructions | scalar FP instructions |
|---|---|---|
| mandelbrot | 0 | 25 |
| spectral-norm | 0 | 65 |
| n-body | 10 | 67 |

(`scripts/shootout/{mandelbrot,spectral-norm,n-body}/`; why the loops do not auto-vectorise is the V agent's package,
`docs/design/vectorisation.md`.) The spec has nothing on SIMD, vectors or tensors (`grep -i 'simd\|tensor\|lane' spec/*.md`
finds no definition; `spec/stdlib.md:3035` records "no transpose, needs a typed matrix" as an accepted gap).

### 0.2 lIR and lair already carry vector types

- lIR types: `<N x T>` as one token, N from 1 to 1024, integer, float, `ptr` and `i1` elements (`spec/lir.md` section 2,
  lines 43-100; `vector length must be 1 to 1024`, line 329).
- Instructions that take vectors today (`spec/lir.md` 6.1 to 6.5): `add sub mul`, `fadd fsub fmul fdiv frem fneg`, `and or xor
  shl lshr ashr`, `ctpop`, `sadd-overflow` and kin (`{ <N x iK>, <N x i1> }`), `icmp`/`fcmp` giving `<N x i1>`, `select` with a
  `<N x i1>` condition, `extractelement`, `insertelement`, `shufflevector` with a literal mask, `bitcast`, and `load`/`store` of any
  first-class type with `(align N)`. A vector literal is `(<4 x i32> 1 2 3 4)`; the L2 lowering test covers it
  (`cases/lir/instr/vector.lir`, `cases/lir/audit/t-vecmem.lir`).
- The AST has `KVector`, `KShuffle`, `KExtractElement`, `KInsertElement` (`compiler/lir/ast.fib:97,109-111`), the lowering
  of intrinsics goes through `fx-intrinsic` (`compiler/native/lower/arith.fib:88`, used for `llvm.ctpop` and the saturating
  conversions), and the Rust lair (`crates/lair`) has the same instructions.

### 0.3 What lIR cannot say today (each is a gap this design must fill)

| Needed | In lIR today? | Evidence |
|---|---|---|
| fused multiply-add, `fmuladd` | no | `spec/lir.md` 6.1 lists `fadd fsub fmul fdiv frem fneg ctpop` and the overflow ops only; section 4.1 reserves `llvm.` names ("intrinsics are instructions, not names", 14 item 5) |
| fast-math or `reassoc`/`contract` flags | no | 6.1: "No `nsw`, `nuw` or `exact` flags exist" (the same holds for FP flags: none is in the grammar) |
| `sqrt`, `fabs`, `floor`, `ceil`, `trunc`, `rint`, `min`/`max` | no | not in 6.1; the n-body gap is exactly this (`docs/shootout/gaps.md` "sqrt is not a builtin", `lib/fib/math.fib` goes through libm) |
| vector conversions (`sitofp <4 x i64> -> <4 x double>`) | no | 6.3: conversions are over "integer scalars", "float scalar" |
| masked load/store, gather, scatter | no | not in 6.5 |
| horizontal reductions (`llvm.vector.reduce.*`) | no | not in 6.4 (expressible with `shufflevector` and the lane ops, which is what a library can do) |
| target features, per module or per function | no | module forms carry no target; the target machine is built from the host in `compiler/llvm/target.fib:92-105` |

Measured consequence: a hand-written lIR dot product on four `<4 x double>` accumulators (`~/.cache/fibber-scratch/SIMD-D/dot.lir`,
built with the lairf built from `compiler/lairf.fib`, `-O 2`, host CPU) compiles to `vmulpd`/`vaddpd` on `ymm` registers, and
**zero `vfmadd`** instructions (counts in the `--emit asm` output: `vfmadd` 0, `vmulpd` 12, `vaddpd` 18), because `fmul` then `fadd`
without a `contract` flag is not fused. 4096 elements, 10^6 calls:

| Version | Time |
|---|---|
| scalar loop, one accumulator, as fibber compiles a `loop` today (`dots`) | 1.67 s |
| four `<4 x double>` accumulators (`dotv`) | 0.42 s (*measured*, 4.0x) |

So the vector path of the backend works end to end today; what is missing is a way to *write* it from fibber, and the
instructions of 0.3.

### 0.4 LLVM already legalises odd vector lengths (measured)

`<5 x double>` `fadd` (`~/.cache/fibber-scratch/SIMD-D/v5.lir`, `lairf build ... --emit asm -O 2`):

| Target | Code for `(fadd a b)` on `<5 x double>` |
|---|---|
| host (AVX2) | one `vaddpd %ymm1, %ymm0, %ymm0` for lanes 0-3 and one `vaddsd` for lane 4 |
| `FIB_TARGET_CPU=x86-64-v2` (SSE4.2, no AVX) | two `addpd` (lanes 0-1, 2-3) and one `addsd` (lane 4) |

That is ADR 016's "one full chunk plus a tail" behaviour, produced by the backend with no chunking logic in fibber (section 2.5).
The cost is the calling convention: an unusual width is returned through a hidden pointer and its arguments arrive split in
registers (the `movq %rdi, %rax` and `vmovapd %ymm0,(%rdi)` in the output), so it is cheap inlined and wasteful across a
non-inlined call.

### 0.5 Matmul baselines (*measured*, f64 and f32, single thread, GFLOP/s = 2n^3 / best of 2-10 runs)

Programs: `~/.cache/fibber-scratch/SIMD-D/mm/{mm.c,MM.java,mm.clj,mm.fib}`. The C "blocked" kernel is a hand-written
register-tiled 4x(2 vectors) micro-kernel on GCC vector extensions (`-O3 -march=native`), no packing; "netlib BLAS" is
`/usr/lib/x86_64-linux-gnu/blas/libblas.so.3` (the *reference* BLAS, `ldconfig -p`: no OpenBLAS, no MKL on this machine);
Java is OpenJDK 27 (Temurin) and `jdk.incubator.vector` is present; Clojure is 1.12.0 with a `double-array` `ikj` loop
(Neanderthal needs a native BLAS and is not installed; its jar was not fetched). fibber is the seed fibc 0.1.5, `-O 2`, the
`ikj` triple loop on a flat `(Array f64)` accumulated in a cell (`mm/mm.fib`; time includes initialisation).

| f64 GFLOP/s | n=512 | n=1024 | n=2048 |
|---|---|---|---|
| C `ijk` | 1.93 | 0.93 | |
| C `ikj` (autovectorised) | 20.86 | 19.09 | |
| C blocked + FMA micro-kernel | 49.31 | 53.55 | 48.55 |
| C, netlib reference `dgemm` | 12.22 | 12.83 | 6.50 |
| Java `ijk` | 1.78 | 0.97 | |
| Java `ikj` | 7.60 | 7.66 | |
| Java Vector API, 4x2 register tile | 28.17 | 28.35 | 30.02 |
| Clojure `double-array` `ikj` | 16.44 | 16.24 | |
| **fibber today**, `ikj` | **1.12** | **1.24** | |

| f32 GFLOP/s | n=512 | n=1024 | n=2048 |
|---|---|---|---|
| C `ikj` | 36.79 | 28.23 | |
| C blocked + FMA | 122.77 | 104.53 / 111.70 | 112.45 |
| C, netlib `sgemm` | 24.64 | 22.18 | 23.97 |

(The peak of this core is about 2 FMA ports x 4 f64 lanes x 2 flops x the clock, so the blocked C kernel is within a factor
of 1.5 to 2 of it.) Targets for `fib.tensor` come from this table (section 3.9): a matmul of 1024 on f64 at or above 30
GFLOP/s is 25x today's fibber and as fast as Java's Vector API; 45 or more would be C's blocked kernel. The reference BLAS is
*slower* than our own kernel would be, so the default is our kernels and BLAS stays an option (section 3.10).

---

## 1. Relation to the original design

Source: the liar-era ADRs in `/tank/repos/liar/doc/adr/` and the first synthesised fibber spec. The owner remembered "native
vectors" being in the original design; they were, as a first-class feature.

| Document | What it says | What this design does with it |
|---|---|---|
| ADR 016 `016-simd-vectors.md` (Accepted) | SIMD "as natural to use as other primitives, not a special optimization"; literal `<<1 2 3 4>>` (v4 i64), `<<1.0 2.0>>` (f64), explicit element type `<i8<1 2 3 4>>`, `<f32<1.0 2.0>>`; the ordinary arithmetic operators work elementwise; scalar broadcast `(+ 5 <<1 2 3>>)`; the programmer writes the element count and "the compiler handles chunking and cleanup" (`(+ 4 <<1 2 3 4 5>>)` on a 4-wide machine is one chunk and a tail); compile-time errors for length mismatch and non-numeric elements; promotion via ADR 017; open questions: power-of-two widths or arbitrary, alignment syntax | Adopted as the premise: SIMD is a **first-class value type with a literal and operator overloading** (2.1 to 2.5). Differences: no promotion (ADR 017 is dropped, `spec/types.md` 1.1), a suffix spelling for the element type (2.3), broadcast by a typing rule that is not a numeric promotion (2.4). The two open questions are answered in 2.12 |
| ADR 015 `015-numeric-primitives.md` | default overflow is an error; `(wrapping expr)` wraps C/Rust-style; `(boxed expr)` promotes to a bignum | The same two modes exist for vectors; wrapping is the `unchecked-*` family already specified (`spec/stdlib.md:1517-1519`, section 7 L10) plus a `wrapping` scope macro (2.8) |
| ADR 017 `017-type-promotion.md` | numeric promotion in mixed operations | Dropped by fibber (`spec/types.md` 1.1: "No implicit conversion between any two scalar types"). **Not revived** (owner, 2026-10-04); the rationale follows the table |
| ADR 018 `018-collections.md` | `<[...]>` and `<{...}>`: "the `<...>` wrapper consistently means give me the fast/raw version" (conventional mutable vector and map) | Dropped by fibber: "the `&` + copy-on-write rule gives in-place updates on unique persistent values instead" (`spec/drafts/inference/syntax.md:565-568`, section "What is deliberately not in the language", from commit 2d7c5c5). The in-place rule is what `fib.tensor` relies on (3.4). The `<<...>>` SIMD literal does not share a prefix with `<[`, so dropping 018 does not touch it |
| ADR 019, 024 (LIR as universal backend; type-directed arithmetic codegen) | arithmetic is selected by the operand type at code generation | Same shape as the checker's built-in `Num` instances (`spec/types.md` 2.12) which this design extends to vector types |
| `spec/drafts/inference/syntax.md:565-566` | "No SIMD literals (ADR 016 dropped for the core; can return as a library over lIR vector types)." | Superseded by the owner's later statement (first-class). The line is still right about the layering: **only the type, the literal and the arithmetic are in the language; everything else is the library `fib.simd` over lIR vector instructions** (2.2) |

**Why ADR 017 stays dropped.** ADR 017 promotes along `i8 -> i16 -> i32 -> i64 -> f32 -> f64` for scalars and, lanewise, for
vectors (`(+ <<1 2 3>> <<1.0 2.0 3.0>>)` is `<<2.0 4.0 6.0>>`; `(+ <i32<1 2 3>> <<4 5 6>>)` is an `i64` vector). Its own
"Negative" list is the case against it: *implicit widening could hide precision issues* (`i64` to `f64` loses bits above 2^53, and
`i64 -> f32` is in its chain) and *promotion may require conversion instructions* (on vectors that is a hidden `sitofp` or
`fpext` per operation, and an `i64x4 -> f64x4` conversion has no single AVX2 instruction, section 2.13). fibber's checker has
no subtyping and no coercion anywhere (`spec/types.md` 1.7), so a promotion lattice would be a new, global typing rule for one
feature. The ADR's own escape hatch ("explicit types available when promotion is undesirable") is what fibber makes the only
mode: `(simd/convert f64x4 v)`. What survives from the ADR is its *non-numeric* rows (a string, character, boolean or keyword is not
a lane: `a vector element must be a number`) and its length rule (a length mismatch is always an error, orthogonal to the element
type). The checker's message for a mixed pair is `cannot unify (Simd i64 3) with (Simd f64 3)`, with the hint `convert with
simd/convert` when the two differ only in element type.

**ADR 018's `<[..]>` against fibber.** ADR 018 gave two families: persistent `[..]`/`{..}` and *conventional* mutable `<[..]>`/`<{..}>`
("the `<...>` wrapper consistently means give me the fast/raw version"), with a colour that makes a conventional collection
unusable across threads unless wrapped in an atom. fibber replaced the pair by **one** persistent family that updates in place
when the value is unique (`fib.unique?`, `spec/types.md` 6.6): the programmer gets the conventional speed without a second type,
a second syntax, or a colour, and a value that is shared is copied, never mutated under a reader. That is why `fib.tensor` is an
immutable value whose `aset!` is in place when unique (3.4), and why the one thing that unique update cannot express, a *writable
window into part of a unique buffer*, is the language feature of `docs/design/exclusive-views.md` rather than a `<[..]>`-style
mutable type. ADR 018's closing line, "the `<...>` wrapper is consistent across SIMD and collections", is not kept: `<<..>>` stays
the SIMD literal and nothing else.

**Other liar sources read for this revision.** `.moth/done/cn5cd` (the SIMD acceptance list: `<<..>>` and `<i8<..>>` parse,
arithmetic, scalar broadcast, **horizontal operations**, mapping to lIR vector types; the surface it names is `(lane v i)`,
`(with-lane v i x)`, `(hsum v)`, `(hmin v)`, `(hmax v)`, and it required power-of-two widths, which decision 1 lifts);
`.moth/ready/jx7f5` (`(vec4 i32 1 2 3 4)`, `(vec-get v 0)`, `(vec-set v 2 99)`, `(vec-shuffle v1 v2 [0 4 1 5])`: the same three
lIR instructions `extractelement`, `insertelement`, `shufflevector`, which lIR already has; fibber's names are in 2.6);
`cert/features/vector_types.feature` (lIR-level scenarios: integer and float vector literals, negative lanes, `<N x i1>` vectors,
`add`/`fadd`/`mul` lanewise, `icmp`/`fcmp` giving `<N x i1>`, splat as a literal; reused in 4.1); ADR 015 (`wrapping`, 2.8); ADR
024 (arithmetic by operand type, the same shape as the checker's built-in `Num` rows). ADR 003 is the origin of `&`, and ADR 007
("aliasing allowed", justified by single threading) is the rule fibber does **not** keep for windows: it is why exclusive views need a
design at all (`docs/design/exclusive-views.md`).

What liar actually implemented (per the coordinator's reading of `/tank/repos/liar/liar/src/`): the lexer tokens, `Expr::SimdVector`,
`parse_simd_elements`, the inferred type `<N x T>` and `generate_simd_vector` building an lIR vector literal whose element
type is guessed from the first element. There was no chunking, no broadcast code and no explicit element type beyond the
syntax. This design therefore has to *specify* operators, broadcast and typed literals, not port them.

---

## 2. Part 1: SIMD

### 2.1 Decisions in one table

| Question | Decision (**Proposed**) |
|---|---|
| First-class or library? | Both, layered. **In the language:** the type `(Simd T n)`, the literal `<<...>>`, `+ - * /` and `neg`, `bit-*`, shifts, `=`/`!=` on it, scalar broadcast. **In the library `fib.simd`:** everything else (loads and stores, masks, reductions, shuffles, conversions, `native-lanes`) |
| Named lane types or parametric? | One parametric constructor `(Simd T n)`, with names `f64x4`, `i32x8`, ... as reader-level sugar for it (a regular pattern, not a table), and `<elem>xn` for the target's native lane count |
| Lane count | any `n` from 1 to 64 (not only powers of two): LLVM legalises (0.4). Loops over arrays use `native-lanes` |
| Masks | `(Simd bool n)` = lIR `<n x i1>`, spelled `(Mask n)` |
| Operators | `+ - * /` (floats), checked `+ - *` and `neg` (integers), `bit-and bit-or bit-xor bit-not shl shr sar popcount`; comparisons by name (`simd/lt` ...) because `Ord` returns `bool`. A scalar **literal** operand broadcasts; a variable needs `(splat x)` (decision 2, 2.4) |
| Integer overflow | the scalar rule: lanewise check, trap on any lane; `unchecked-*` and the `wrapping` macro wrap |
| Floats | IEEE lanewise, never trap, the same NaN, infinity and `-0.0` results as the scalar op |
| Memory | `simd/load` and `simd/store!` on `(Array T)`: one bounds check per vector; masked forms for tails; gather and scatter checked per lane |
| Target | `FIB_TARGET_CPU`/host features give a `Target` record that the emitter receives; `native-lanes` is a compile-time constant; the chosen CPU is written into the lIR module so `emit` then `lair build` agree |
| Interpreter | `fibref` is frozen (`docs/design/unboxed-option.md` header): no vector support there. Rule 6 is served by in-program scalar references (4.1) |

### 2.2 The layering: what must be a builtin and what is library code

A lane vector is a **scalar-class value**: it lives in a register (`<n x T>`), is copied, has no count and no mode
(`spec/types.md` 1.1: "Scalars are copied and have no identity, no count and no mode"). It cannot be a library struct: a
struct is a counted heap object, one `ptr` (`spec/types.md` 8.1), and a vector inside a struct would be an allocation and
a load per use. So the **type has to be known to the checker and the emitter**; that is the one unavoidable core change.
Everything that can be written as a function over that type and a few primitives goes to `fib.simd`.

| Piece | Where | Needs |
|---|---|---|
| Type `(Simd T n)`, sugar names, layout class `vec` (`<n x T>` in the table of `spec/types.md` 8.1) | checker + emitter (`compiler/types/`, `compiler/emit/`) | new type constructor; a nat (literal number) argument in type position, checked like a colour argument (`spec/types.md` 1.3: a parameter that is not a type is already a precedent, and monomorphisation keys by type arguments, 4.3) |
| Literal `<<...>>` | reader (`compiler/syntax/lexer.fib`) and a core form `(simd ...)` | 2.3 |
| `+ - * /`, `neg`, bit ops, `=` on vectors | the built-in `Num`, `Float`, `Bits`, `Eq` instances (`spec/types.md` 2.12) | rows keyed by vector type in the arithmetic lowering (`spec/types.md` 8.12 table; `compiler/native/lower/arith.fib`, emitter side `compiler/emit/lower/`) |
| Scalar broadcast of a literal | checker: the literal adopts the vector's element type (2.4) | the L19 rule, no new constraint |
| lane access, insert, constant shuffle, splat, compare, select, bitcast | `fib.simd` functions that are one-instruction **primitives** (`simd/lane`, `simd/with-lane`, `simd/shuffle`, `simd/splat`, `simd/lt`..., `simd/blend`, `simd/reinterpret`) | rows in `compiler/types/builtins.fib` (the `BuiltinSig` table, e.g. line 50) and the emitter; every one maps to an **existing** lIR instruction (0.2) |
| `fma`, `sqrt`, `abs`, `floor`/`ceil`/`trunc`/`round`, `min`/`max`, vector conversions, masked load/store, gather/scatter, fast flags | primitives that need **new lIR instructions** (2.10) | lIR spec 6.1/6.3/6.5, parser, checker, printer, `compiler/native/lower/*`, `crates/lair` (Rust, with its unit tests) and lairf |
| `hsum`, `hmin`, `hmax` (aliases `reduce-add/min/max`), `any`, `all`, `iota`, `lanes`, `lo`/`hi`/`concat`, `reverse`, `tail-mask`, `load-tail`, `store-tail!`, `dot`, `sum`, `axpy` | plain **library code** over the primitives | none (halving shuffles and lane ops; `any`/`all` through `bitcast <n x i1>` to `iN`, which lIR allows for N up to 64) |
| `native-lanes`, `native-has-fma?` | compile-time constants of the target (2.9) | emitter receives a `Target` |

### 2.3 Types, literals, reader

**Types.** `(Simd T n)`: `T` is `i8 i16 i32 i64 f32 f64` or `bool` (the mask), `n` a literal natural from 1 to 64, the whole
value at most 512 bits (`vector wider than 512 bits` otherwise; this is also the bound of `spec/lir.md` 7.3 for a result
returned in registers at a tail call). `n` may be a variable of a generic definition (`(defun f (v: (Simd f64 n)) ...)`:
inference unifies nat arguments by equality, as colour arguments, `spec/types.md` 1.3 "Invariance") and may be `:native`,
which the checker replaces by the target's lane count for `T` at the stage where the `Target` is known (2.9).
Sugar names are read as type names by the checker: `f64x4` is `(Simd f64 4)`, `i32x8` `(Simd i32 8)`, `f32xn` `(Simd f32
:native)`, `(Mask 4)` is `(Simd bool 4)`. A vector is `Send` (a scalar is). It has `Eq` (whole-vector equality, `bool`),
`Show`, and *no* `Ord` and *no* `Hash` (a lanewise `<` returns a mask, 2.6; hashing floats has the `-0.0`/NaN rules of
`spec/types.md` 2.12 and is not needed).

Why parametric rather than only named types. The checker has no const parameters today (`spec/types.md` 1.3 knows type
and colour parameters only), but monomorphisation already keys on type arguments (4.3) and the colour-parameter
machinery shows a non-type argument position is cheap to add. A table of named types (`f64x2 f64x4 f64x8 f32x4 f32x8 f32x16
i64x2 .. i8x16 ..`) is 30 entries, each needing its own `Num`/`Eq`/`Bits` rows, and cannot express "a vector of any length"
(ADR 016's `<<1 2 3 4 5>>`), nor code generic over the lane count. Parametric costs one new kind of type argument.

**Literal.** Today `<` and `>` are ordinary symbol constituents: the symbol rule (`spec/syntax.md` 1, "A symbol is a maximal
run of characters that are not whitespace, `( ) [ ] { } " ; ' `` ` `` `,` `@` `~` `\`") does not exclude them, so `<<1` reads as
the symbol `<<1` and `3>>` as the symbol `3>>`... except that a token starting with a digit is a number read error. No
fibber source uses `<<` or `>>` as a symbol (grep of `lib/`, `compiler/`, `cases/`, `scripts/`: the only fibber hits are the text of a
comment in `compiler/lir/diag.fib:3` and a string in `cases/stdlib/6101-regex-pairs-and-the-api.fib:18`; `->>` is a different
token). **Proposed reader rule:** a token that begins with `<<` and has a following character that is not whitespace, `)`, `]`
or `}` opens a vector literal; the literal closes at the next `>>` that is not inside a nested form; inside it `>>` ends a
token (so `<<a b>>` reads `a`, `b`). It reads as the form `(simd e1 .. eN)`. A bare `<<` or `>>` (followed by whitespace or a
closer) stays a symbol, and `->>` and `->` are untouched. The change is local to `compiler/syntax/lexer.fib`
(the delimiter and prefix reader, lines 26-39, 119) and the Rust reader is frozen, so the stage check
(`compiler/mirror-pending/`) is not involved: `fibref read` simply would not read the literal (rule 6 for the reader is
by the stage-2 compare scripts, which would get a new input).

```
<<1 2 3 4>>              ; (simd 1 2 3 4)          : (Simd i64 4)
<<1.0 2.0 3.0 4.0>>      ; (Simd f64 4)
<<x y z w>>              ; elements are expressions: (Simd f64 4) when x..w : f64
<<0.1 0.2 0.3 0.4>>f32   ; element width by a suffix after the closer: (Simd f32 4); the elements are read at f32
<<1 2 3 4 5>>            ; (Simd i64 5): any length 1..64
```

The element type is the type of the elements (unified: all equal), and a suffix `f32`, `f64`, `i8`..`i64` after `>>` fixes it,
with the literal rule of the numbers it contains: each plain literal is read at that width, a float literal is rounded to `f32`
(as `0.1f32` is), an integer literal for a float width must be exact (the rule of `spec/types.md` 1.1 / stdlib 7 L19),
and a literal with its own suffix that disagrees is a read error (`300i8` already is, `spec/syntax.md` 1). Errors, as ADR
016's table: `(+ <<1 2 3>> <<4 5 6 7>>)` is `cannot unify (Simd i64 3) with (Simd i64 4)`; a non-numeric element is `a vector
element must be a number or a variable of numeric type`.

*Not the ADR's spelling (decided, decision 1).* ADR 016 writes the element type inside the opener, `<f32<1.0 2.0>>`,
`<i8<1 2 3 4>>`. That token is, today, one symbol (`<f32<1.0` and `4>>`), and the asymmetric closer needs a second reader
state. The owner prefers the ADR's look but accepted the suffix, which uses what the reader already has for numbers (`1.0f32`,
`5i8`). The `<f32<..>>` form is therefore **omitted**: it costs a second opener rule for no capability, and two spellings of one
literal would be one more thing for every tool (printer, macros, error messages) to agree on. `Show` prints the suffix form.

The constructor function form `(f64x4 a b c d)` and `(simd/splat V x)` are provided as well, because the literal's elements
are expressions and a macro that builds vectors needs a call form.

### 2.4 Operators and scalar broadcast

`+ - * /` on floats are the lIR `fadd fsub fmul fdiv` on the vector, which are exactly lanewise IEEE: NaN propagates, `x/0.0`
is an infinity or NaN, `-0.0` behaves as the scalar `-0.0` does; no flag that makes a result poison is ever emitted
(`nnan`, `ninf`, `nsz` are not in the grammar and stay out, 2.10). `%`/`rem` on vectors is not provided (`frem` is a libm call
per lane; use `(- x (* y (simd/trunc (/ x y))))` or scalar code). `quot` is not provided on vectors. Integer `/` is not
provided (no hardware instruction; the scalar rule's two traps would cost a branch per lane). Bit operations and shifts are the
`Bits` instances lanewise; a shift count is a vector or a scalar *literal* (a variable count is `(splat n)`) and is masked to the width as the scalar is
(`spec/types.md` 8.12: `n mod w`).

The built-in `Num` instance exists for every vector type of floats and integers (the same status as the scalar ones in
`spec/types.md` 2.12: "protocol methods with built-in instances"), so `(reduce + vs)`, `(+ a b)` and generic code under
`(Num a)` all work.

**Broadcast: literals and `splat` only** (owner, decision 2). ADR 016 broadcasts any scalar (`(+ 5 <<1 2 3>>)`, `(* <<1 2 3 4>> 2)`).
fibber has no implicit conversion between scalar types and no subtyping (`spec/types.md` 1.7: "no subtyping, no coercion
anywhere"), but one place where a scalar already adapts exists: an integer literal in an argument position adopts the numeric
type it unifies with (`spec/stdlib.md` L19). **Rule:** in a binary arithmetic, bit or comparison builtin whose other operand is
`(Simd T n)`, a scalar **literal** operand is read at type `T` and splatted:

- `(* v 2)` on `f64x4` is `(* v <<2.0 2.0 2.0 2.0>>)`; `(+ 5 <<1 2 3>>)` is `<<6 7 8>>`; a float literal against `f32` is rounded, as
  with a suffix; an integer literal against a float `T` must be exact (the L19 rule), and a literal that is not representable in `T`
  (`300` against `i8`, `1.5` against `i32`) is an error;
- a scalar **variable, call, field read or any other expression** is **not** broadcast: `(+ x v)` is the error `scalar operand
  of a vector operator: write (splat x)` (position of the scalar operand); `(simd/splat V x)` and the sugar `(splat x)`, whose
  vector type comes from the other operand or the annotation, are the explicit form. Because no constraint is deferred, the
  result of every operator application is decided where it is written: there is no inference-order dependence, and no
  `cannot infer whether (+ a b) broadcasts` error exists;
- a mismatched element type is an error whatever the operand: `(+ <<1 2>>i32 1.0)` and `(+ <<1 2>> 1.5)` are errors, never a
  promotion (ADR 017 stays dropped, section 1);
- the **result** is the vector type; a comparison's result is the mask (2.6).

Why literals at all, given "no coercion". The tie-breaker of the project is "unless it breaks memory safety, Clojure has the
ergonomics; if it does, Rust has": `core.matrix` broadcasts scalars, Rust's `std::simd` requires `Simd::splat`. The owner chose the
middle: the literal case is the overwhelming one in numeric code (`(* 2.0 x)`, `(+ v 1.0)`), costs the checker nothing beyond the
literal-adoption it already has, and keeps ADR 016's look; the variable case is where ADR 016's rule needed a deferred constraint
(`Broadcast(a, b)`, like `HasField`) resolved by inference order, and where a reader cannot tell from the text whether `(+ a b)` adds
vectors or splats a scalar. Writing `(splat x)` costs one word and says what happens. Simpler checker, no inference-order surprises.

### 2.5 Hardware width: the compiler does not chunk, LLVM legalises

ADR 016: "the programmer specifies the element count, the compiler maps it to hardware, chunking and cleaning up". The
question set to this design was whether lowering `(Simd T n)` straight to `<n x T>` gives that for free. **Yes, measured**
(0.4): `<5 x double>` `fadd` is one 256-bit add plus one scalar add on the AVX2 host and two 128-bit adds plus one scalar
add on `x86-64-v2`. No chunking pass is written in fibber; the emitter writes `<n x T>` and `lair` (LLVM) splits it.

What this means in practice (**Proposed**):

- `n` is arbitrary in 1..64 for a *fixed small vector*, as in `<<1 2 3 4 5>>`. Power-of-two is only a performance advice
  (shuffles and reductions on `n=5` or `n=3` cost more than on `n=4`; a 3-vector `[x y z]` is the physics case and is
  better as `f64x4` with a dummy lane).
- 1..64 and 512 bits is a *cap*, not an LLVM limit (lIR allows 1024 lanes, `spec/lir.md` 3): a `<1024 x double>` would be 256
  unrolled `ymm` operations, which is a compile-time and code-size trap; large data belongs in arrays and tensors.
- An odd or wide vector across a **non-inlined call** goes through memory or split registers (0.4). The emitter should keep
  small vector helpers inlinable; `compiler/native/passes.fib` runs `default<On>` (the inliner), nothing to add. Passing a
  vector to an `extern` is an error (`vector argument to extern`): the C ABI of a vector depends on the target features.
- An **ABI hazard** of a per-function target feature (2.9, multiversioning) is the same fact: vectors do not cross between
  functions compiled for different features. The proposal for v1 has no per-function features.

### 2.6 Operations

Names are `fib.simd` (written `simd/` with the usual alias). `V` is a vector type, `M` its mask, `E` its element.

**Names from liar** (decision 6). ADR 016's issue (`cn5cd`) names the lane and horizontal operations `lane`, `with-lane`, `hsum`, `hmin`,
`hmax`; the later issue `jx7f5` names them `vec-get`, `vec-set`, `vec-shuffle` over the same three lIR instructions. fibber takes the
first set (`simd/lane`, `simd/with-lane`, `simd/hsum`, `simd/hmin`, `simd/hmax`) and the shuffle name `simd/shuffle`, and does not take
`vec-get`/`vec-set`/`vec-shuffle`: `vec` means the persistent `Vec` in fibber, so a SIMD `vec-get` would invite the wrong reading.
`reduce-add`, `reduce-min`, `reduce-max` stay as aliases for the horizontals (the Clojure-shaped name). The bounds rule is below: a
literal lane index is checked at compile time, a dynamic one traps.

**Primitives that map to an existing lIR instruction**

| fibber | meaning | lIR |
|---|---|---|
| `(simd e1 .. en)`, `<<e1 .. en>>`, `(f64x4 a b c d)` | construct | `KVector` when all constants, else `insertelement` chain |
| `(simd/splat V x)` | all lanes `x` | `insertelement` at 0 then `shufflevector` with the zero mask |
| `(simd/lane v i)` | lane `i` | `extractelement`; a literal `i` is range-checked at compile time (`vector index 4 out of range`); a dynamic `i` is compared and traps `lane index out of range` (lIR: a run-time out-of-range index is poison, `spec/lir.md` 510, so it is never emitted unchecked) |
| `(simd/with-lane v i x)` | replace lane `i` | `insertelement`, checked the same way |
| `(simd/shuffle a b [i0 i1 ..])` | lanes `i` of the concatenation of `a` and `b`; indices a vector literal of constants `< 2n`; result length is the index count | `shufflevector` with a literal mask |
| `(simd/lt a b)` `le gt ge eq ne` | lanewise compare, a `(Mask n)` | `fcmp olt ole ogt oge oeq` and `une` for floats (ordered, as the scalar `<` is, `spec/types.md` 8.12: false on NaN, `ne` true on NaN); `icmp slt ..` for integers |
| `(simd/blend m a b)` | lane from `a` where `m` else `b` | `select` with a `<n x i1>` condition |
| `(simd/and m1 m2)` `or` `xor` `not` | mask logic | `and or xor` on `<n x i1>` |
| `(simd/reinterpret V v)` | same bits, other lane type of the same total size (`f64x4` to `i64x4`) | `bitcast` (non-aggregate, same size, `spec/lir.md` 6.3) |
| `(simd/mask-bits m)` | `i64` with bit `k` set when lane `k` is true (`n` at most 64) | `bitcast <n x i1> to iN`, `zext` |
| `(simd/any m)` `(simd/all m)` | `bool` | the same, compared with 0 / all ones |

**Primitives that need a new lIR instruction (2.10)**

| fibber | meaning | LLVM |
|---|---|---|
| `(simd/fma a b c)` | `a*b+c`, one rounding, **exactly** IEEE `fma` | `llvm.fma` (a libm `fma` call on a CPU without FMA: correct, slow) |
| `(simd/muladd a b c)` | `a*b+c` fused where the target has FMA, multiply then add where not (not bit-identical across targets); `(has-fma)` is the compile-time constant of the target | `llvm.fmuladd` (SC1) |
| `(simd/bitcast v)` | lanes reinterpreted as the same-width lane type of the context (`f64xN <-> i64xN`, `f32xN <-> i32xN`) | `bitcast` (SC1) |
| `(simd/sqrt v)` | IEEE `sqrt`, correctly rounded (also the scalar `sqrt` builtin L11, `docs/shootout/gaps.md`) | `llvm.sqrt` |
| `(simd/abs v)`, `(simd/neg v)` | | `llvm.fabs`; `fneg` exists; integer `llvm.abs` (traps on `MIN`, like `neg`) |
| `(simd/floor v)` `ceil` `trunc` `round-even` | exact | `llvm.floor ceil trunc roundeven` |
| `(simd/min a b)` `(simd/max a b)` | the stdlib's `max`/`min`: **a NaN argument gives NaN** (`spec/stdlib.md:1514`, "as Clojure's"), and `max(0.0, -0.0)` is `0.0` | `llvm.maximum`, `llvm.minimum` (these propagate NaN and order `-0.0 < +0.0`) |
| `(simd/min-num a b)` `max-num` | IEEE `minNum`/`maxNum` (a NaN operand loses); the faster instruction | `llvm.minnum`, `llvm.maxnum` |
| `(simd/convert V v)` | `i32x4 <-> f32x4`, `i64x2 <-> f64x2`, `f32x4 -> f64x4` (`fpext`), narrowing (`fptrunc`); float to int **saturates** as the scalar `fptosi` does (`spec/types.md` 8.12) | the cast instructions extended to vectors (`sitofp`, `uitofp`, `fptosi-sat`, `fpext`, `fptrunc`, `trunc`, `sext`, `zext`: LLVM accepts them; lIR 6.3 restricts to scalars) |
| `(simd/load V a i)`, `(simd/load-masked V a i m passthru)` | 2.7 | `load` / `llvm.masked.load` |
| `(simd/store! &a i v)`, `(simd/store-masked! &a i v m)` | 2.7 | `store` / `llvm.masked.store` |
| `(simd/gather V a idx)`, `(simd/scatter! &a idx v)` | 2.7 | `llvm.masked.gather` / `scatter` |

**Library code (no new instruction)**

`(simd/iota V)` (`<<0 1 2 3>>`, a constant), `(simd/lanes v)` (a constant), `(simd/lo v)`, `(simd/hi v)`, `(simd/concat a b)`,
`(simd/reverse v)` (shuffle), `(simd/hsum v)` (alias `reduce-add`), `hmin`, `hmax` (aliases `reduce-min`, `reduce-max`), `(simd/hsum-ordered v)`, `(simd/tail-mask V k)` (the
mask with the first `k` lanes true: `(simd/lt (simd/iota I) (simd/splat I k))`), `(simd/load-tail V a i)` (zero-filled),
`(simd/store-tail! &a i v)`, `simd/dot`, `simd/sum`, `simd/axpy!` (3.7 reuses them).

- **Reductions have a defined order.** `reduce-add` on floats is the *pairwise halving tree*: lane `i` plus lane `i + n/2`,
  recursively (`n` a power of two; otherwise zero-extend to the next power of two first with the identity). This is what
  a `reassoc` reduction would be, made deterministic, so the compiled library and any other implementation agree to the
  bit. `reduce-add-ordered` is left to right from lane 0 and lowers to a sequential chain (`llvm.vector.reduce.fadd` without
  `reassoc`, or the shuffle loop). `(reduce + xs)` over a `Vec` of floats stays the Clojure left fold; **`simd/sum` is not
  `(reduce + ...)`** and the docstring says so. For integers `reduce-add` is the checked tree: `+` on vectors, so any lane
  overflow traps (the scalar rule); `unchecked-reduce-add` wraps. `reduce-min/max` use `min`/`max` above (NaN-propagating).
- `any`/`all` need no instruction: `bitcast` of `<n x i1>` to `iN` is valid for `n` up to 64 in lIR (`spec/lir.md` 3: `i1 .. i64`).

`(= a b)` on vectors is *all lanes equal* as scalars are (a NaN lane makes it false, `0.0` equals `-0.0`) and returns `bool`.
`Show` prints as the literal with the element type visible: `<<1.0 2.0 3.0 4.0>>`, and for non-default element types
`<<1.5 2.5>>f32`, which the reader reads back.

### 2.7 Memory: loads, stores, tails, gather, scatter

All access is to an `(Array T)`, the library's one contiguous buffer (`spec/types.md` 2.13). The array's payload starts
after a 24-byte header (`spec/stdlib.md`/`spec/types.md` 2.13.1, "The array header is `(hdr 16 bytes, len i64)`"), allocated
with malloc, so the payload is 8-byte aligned and not 32-byte aligned.

- `(simd/load V a i)` reads lanes `i .. i+n-1`. **One check per vector**: `0 <= i` and `i + n <= (array-len a)`, computed
  with the checked integer ops, traps `simd/load: index i + n out of range for length len` (the text of `array-get`'s trap
  with the width). No per-lane check. The result of a load is a value; the array is only borrowed for the instruction. It is
  `Derived`-read-safe in the sense of the element-read rule (`spec/types.md` 6.3, "Element reads"): no count is taken for a
  scalar element, which every lane type is.
- `(simd/store! &a i v)` takes `&a` like `array-set!` (`spec/types.md` 2.13): it tests `fib.unique?`, copies the array first when it is
  shared (so a shared array is never written, 2.13.1), checks the range once, and stores. The unique test is in the loop; for a
  hot loop the library kernels do it once (`array-make-unique!`, 3.5) and use the unchecked store inside `unsafe`.
- **Alignment.** Every load and store uses the element's alignment (`(align 8)` for `f64`, `(align 4)` for `f32`, 1 for
  `i8`), which is a promise the array always keeps (`spec/lir.md` 6.5: a smaller `N` than the ABI's is a promise LLVM
  honours with slower code where the target needs it). x86 has had fast unaligned vector loads since Haswell; there is no
  aligned variant in the surface, so there is no way to write an aligned-access promise that is false. ADR 016's open
  question 2 (alignment syntax) is answered by *no syntax*: lines that straddle two cache lines cost, and the
  later remedy is to allocate large array payloads on a 64-byte boundary in the runtime (`crates/fibc/rt/array.lir`), not to
  add syntax.
- **Tails.** `(simd/load-masked V a i m passthru)`: lanes where `m` is false are *not read at all* (LLVM's
  `masked.load` guarantee) and take `passthru`; active lanes are range-checked: the highest active lane `k` is the position of the highest set bit of `(simd/mask-bits m)`,
  and the check is `i + k < len` (one compare). `load-tail`/`store-tail!` are the convenience forms with `tail-mask`. So the tail of a loop over `len` elements is
  *one* masked iteration, with no scalar epilogue and no out-of-range read. `(simd/store-masked! &a i v m)` writes only active
  lanes.
- **Gather and scatter.** `(simd/gather V a idx)` with `idx : (Simd i64 n)` (or `i32`): every index is checked (`idx >= 0` and
  `idx < len`, as vector compares, `reduce-or` of the failures, one trap `simd/gather: index out of range`), then
  `llvm.masked.gather`. `(simd/scatter! &a idx v)` checks the same, and for **duplicate indices the highest lane wins**
  (the lanes are stored in order, which is LLVM's defined order for `masked.scatter`), which the cases pin. There is no
  scatter on AVX2 hardware: LLVM emits scalar stores; gather is `vgatherdpd`-class and slow on some CPUs. They exist for
  completeness and are not in any benchmark kernel.

### 2.8 Overflow: checked, `unchecked-*`, `wrapping`

**Decided (owner, 2026-10-04, decision 4):** integer vector arithmetic is checked by default, exactly like scalar arithmetic; `wrapping` is the opt-out, as in ADR 015 and consistent with the scalar rules; the `unchecked-*` family (L10) is provided for scalars and vectors. There is no vectors-only wrapping default.

Integer `+ - *` and `neg` on vectors are **checked** like the scalar (`spec/types.md` 8.12): `sadd-overflow`/`ssub-overflow`/
`smul-overflow` already take vectors in lIR (`spec/lir.md` 6.1: result `{ <N x iK>, <N x i1> }`), then `reduce-or` of the overflow
vector, then one branch to `fib.trap` (`integer overflow in + at i32x4`). That check is what keeps integer loops from
vectorising (the V agent's blocker list), and the scalar side has the same issue; the answer is the same in both: ADR 015's
**wrapping** mode.

ADR 015 specified `(wrapping expr)`. fibber already specified the function family `unchecked-add`, `unchecked-subtract`,
`unchecked-multiply` (wrapping at the operand's width, `spec/stdlib.md:1517-1519`, section 7 L10, **not landed**) and BigInt, hashes
and generators need it (`docs/shootout/improvements.md` item 7). **Proposed:** land L10 as three lIR-level plain
`add`/`sub`/`mul` (no check, no `nsw`: the lIR instructions already wrap, `spec/lir.md` 6.1) for scalars *and* vectors, by
making the checked/unchecked decision a property of the lowering of `+ - *` (a flag in the emitter's context), and add the
scope macro `(wrapping e)` that rewrites `+ - * neg` inside `e` (not inside a nested `fn`) to the `unchecked-` forms, as
ADR 015's. `boxed` of ADR 015 does not apply to fixed-width lanes (it is `fib.bigint`'s job) and is not provided on vectors.
Float vectors have no overflow mode.

### 2.9 Portability, the target, and the baseline

**Where the target is read today.** `native.target` wraps `llvm.target`: `host-machine` builds a target machine through
`create-machine` (`compiler/llvm/target.fib:92-105`): with `FIB_TARGET_CPU` set (and not `host`) it names that CPU with an
**empty** feature string (so `x86-64-v2` means exactly the baseline, `target.fib:8-9`, `requested-cpu` at 62-65);
unset, it passes `LLVMGetHostCPUName()` and `LLVMGetHostCPUFeatures()` (the full `+avx2,+fma,...` string). It registers
only the x86 target (`init` at `target.fib` calls `LLVMInitializeX86*`), so today there is no AArch64 at all. The pipeline
`default<On>` in `compiler/native/passes.fib` runs after, with that machine.

**The emitter does not know the target.** `fibc emit X.fib` prints lIR (`fibc help`: "emit file: print the lIR module"); the
choice of CPU is made later, by `lair`. But `(Simd f64 :native)`, `native-lanes` and a tail loop's trip count need the lane
count when the *front end* lowers. **Proposed:**

1. A record `Target` (explicit parameter of the emitter and the expander, no global state, per CLAUDE.md): `(Target cpu: str features:
   (Set str) vector-bits: i64 has-fma: bool ...)`, computed in one place (`native.target/target-info`) from the same decision as
   `create-machine`: `FIB_TARGET_CPU` names a CPU (features from a small table in fibber for the CPUs we ship: `x86-64`
   128 bits, `x86-64-v2` 128, `x86-64-v3`/`haswell`/`znver*` 256 and FMA, `x86-64-v4`/`skylake-avx512` 512 but preferred 256
   as LLVM's `prefer-vector-width` does on client cores), or the host's feature string is parsed for `+avx2`, `+avx512f`, `+fma`.
2. **The chosen CPU and features are written into the lIR module** (a new optional top-level form, e.g. `(target (cpu
   "x86-64-v3") (features "+avx2,+fma"))`) and `lair`/`lairf` honour it over the host when present. Without this, `fibc emit`
   on machine A piped to `lair build` on machine B would pick lane counts for A and code for B, and the golden-output
   comparisons of the stage check (`compiler/tests/emit/`, `emit` of `fibc.fib` equal across `fibc` and `F`, CLAUDE.md) would
   differ per machine. Those tests pin `FIB_TARGET_CPU=x86-64`.
3. `(native-lanes T)` is `vector-bits / bits(T)` as a literal at expansion; `(native-has-fma?)` a boolean literal. Both are
   *constants of the module*, so `(if (native-has-fma?) ... ...)` folds and dead code is removed before the lowering.

**Scalar fallback.** Not needed for correctness: every `<n x T>` op legalises on every LLVM x86 target, down to SSE2 (0.4
shows `x86-64-v2`), and scalarises where there is no vector unit. What differs is speed, and the *exact* semantics are
fixed by the lIR instructions, so results are bit-identical across targets for everything except `fma`-contraction
choices made by the *library* (3.8) and the lane-count-dependent order of a reduction written with `native-lanes`.

**The release baseline.** The release `fibc` is built for `x86-64-v2` (CI, `.gitlab-ci.yml`, commit 60734ab) so it runs on old
machines, but a *user program* is compiled by `create-machine` for the host (or `FIB_TARGET_CPU`), so user code that asks for
`native-lanes f64` gets 4 on this machine, with nothing else to do. The baseline only bites if a **prebuilt binary** is to
use SIMD for speed (the library's own kernels inside `fibc`, a shipped tensor tool). That needs *multiversioning*:
(a) a per-function target attribute in lIR (`(define ... (target "+avx2,+fma"))`), (b) the emitter building the function
twice (`native-lanes` is then a per-function constant), (c) a startup dispatch: a table of function pointers filled by a
runtime check of the CPU features (x86 `cpuid` has no lIR form; libgcc/compiler-rt export the `__cpu_model` data that
`__builtin_cpu_supports` reads, linkable as a global), (d) a call through that pointer once per *kernel call*, not per
element. About a work package of its own (5, P10); v1 does not need it, and a CI step that compiles the SIMD cases with
`FIB_TARGET_CPU=x86-64-v2` already checks the baseline code path is correct.

### 2.10 lowering and the lIR instructions to add

Each is one `fx-intrinsic` call in the lowering (`compiler/native/lower/arith.fib:88-109` shows the shape for
`llvm.ctpop` and `llvm.fptosi.sat`) plus the parser/checker/printer/dump rows for the new `LKind` constructors
(`compiler/lir/ast.fib:91-131`) and the same in the Rust lair (`crates/lair`, permanently Rust, with a unit test for each
check, per CLAUDE.md), so the lairf/lair compare scripts (`compiler/tests/native/`, `scripts/ci-lairf.sh`) cover them.

| New lIR form | Rule | LLVM |
|---|---|---|
| `(fma a b c)` | same float or float-vector `T` | `llvm.fma` |
| `(fsqrt a)` `(fabs a)` `(ffloor a)` `(fceil a)` `(ftrunc a)` `(froundeven a)` | float or vector | `llvm.sqrt fabs floor ceil trunc roundeven` |
| `(fmin a b)` `(fmax a b)` `(fminnum a b)` `(fmaxnum a b)` | same float `T` | `llvm.minimum maximum minnum maxnum` |
| `(smin a b)` `(smax a b)` `(umin ..)` `(umax ..)` `(abs a)` | integer or vector | `llvm.smin ...` |
| casts on vectors | 6.3 rows extended: element-wise, same lane count | existing cast instructions |
| `(masked-load T p mask passthru (align N))` / `(masked-store v p mask (align N))` | `mask : <n x i1>`; `T` a vector | `llvm.masked.load/store` |
| `(gather T ptrs mask passthru)` / `(scatter v ptrs mask)` | `ptrs : <n x ptr>` | `llvm.masked.gather/scatter` |
| fast-math flags on `fadd fsub fmul fdiv`: `(fadd reassoc a b)`, `contract` | opt-in flags **never** `nnan ninf nsz` (those make poison) | `reassoc`, `contract` fast-math flags |
| module `(target (cpu "..") (features ".."))` | 2.9 | `TargetMachine` CPU and feature strings |

Why no `nnan`/`ninf`: a value that violates such a flag is *poison*, which is undefined behaviour in the spec's sense;
`reassoc` and `contract` only license reordering and fusing, never poison. `reassoc` is the lever of the V agent's item (c)
(opt-in FP reduction order) and `docs/design/vectorisation.md` should reuse the same flag; **these two documents must agree on
the lIR form**.

### 2.11 Memory safety: what is undefined, and why nothing is

| Hazard | Where | Defused by |
|---|---|---|
| out-of-range vector load/store | `simd/load`, `store!` | one range check per vector, checked integer arithmetic for `i + n` |
| out-of-range masked lane | masked forms | inactive lanes are never accessed (LLVM semantics); active lanes range-checked via the highest set bit |
| out-of-range gather/scatter index | | every index checked, one trap |
| out-of-range dynamic lane index | `lane`, `with-lane` | compared and trapped; lIR's own rule would make it poison (`spec/lir.md` 510) |
| shuffle index out of range | `shuffle` | indices are a literal, checked at compile time (`spec/lir.md` 6.4: `< 2N`) |
| shift amount at or above the width | `shl shr sar` | masked to `n mod w` like the scalar (`spec/types.md` 8.12) |
| integer overflow, `MIN / -1`, division by zero | | `+ - *` checked; `/` and `quot` on integer vectors not provided |
| float to int out of range / NaN | `convert` | saturating forms only (`fptosi-sat`), as scalar |
| poison from fast-math flags | | `nnan ninf nsz` are not in the grammar; only `reassoc`/`contract` |
| misaligned access | | every access carries the element alignment, which the array guarantees; no aligned variant exists (2.7) |
| the array moves or is freed while a vector reads it | | a load is an instruction on a borrowed array; no pointer escapes |
| write through a shared array | `store!` | the unique-write protocol (`spec/types.md` 2.13.1, 6.6): shared arrays are copied first |
| vectors in `extern` calls | | refused |
| uninitialised lanes | `with-lane` on `undef` | no `undef` is ever produced: constructors fill every lane; `load-masked` takes an explicit passthru |

Vectors hold only scalars, so they are no part of the counting rules: no retain, no release, never `Derived`, and the
escape/ownership checker treats them as the scalars of `spec/types.md` 1.1 (the place to edit is wherever the checker
lists the scalar types; a case with a vector in a struct field, a `Vec` element, a closure capture and a task result must
show no count operation in `fibc explain`, as the batch-4 cases do for options).

### 2.12 The ADR's open questions, answered with evidence

1. **Power-of-two widths or arbitrary?** Arbitrary, 1 to 64 lanes and 512 bits: LLVM legalises (0.4: `<5 x double>` is a
   ymm add and a scalar add on AVX2, two xmm adds and a scalar add on `x86-64-v2`). Performance, not correctness, prefers powers
   of two; loops over arrays use `native-lanes`, which is a power of two by construction.
2. **Alignment syntax?** None (2.7): element alignment is always promised and kept; x86 handles unaligned vector access at
   full speed in the common case; the remedy for cache-line splits is an allocator change, not syntax.

### 2.13 The three benchmark kernels in the proposed surface (pseudo-code)

These are written to be compiled one day; they are not run. They use `(Simd f64 :native)` written `f64xn`.
Differences from the reference programs (`scripts/shootout/*/`) are the order of floating-point sums and `fma`: each must
still print what the benchmark requires (mandelbrot: the same bytes; n-body and spectral-norm: the same 9 digits), which
the benchmark harness (`scripts/shootout/run.sh`) already checks for the scalar programs.

**mandelbrot** (reference: `scripts/shootout/mandelbrot/mandelbrot.fib`). Lanes are adjacent pixels of a row; eight
pixels are one output byte, so a byte is `8 / W` vectors (`W` is 1, 2, 4 or 8):

```
(ns main (:use fib.unix) (:require [fib.simd :as simd]))

;; The mask of lanes whose |z|^2 stays at or below 4 through 50 iterations of z = z^2 + c (the reference `inside?`, per lane).
(defun inside-lanes (cr: f64xn ci: f64) -> (Mask :native)
  (let [civ (simd/splat f64xn ci)]                              ; a variable is splat explicitly; literals broadcast (2.4)
    (loop [i 0 zr (simd/splat f64xn 0.0) zi (simd/splat f64xn 0.0)
           tr (simd/splat f64xn 0.0) ti (simd/splat f64xn 0.0)
           live (simd/all-true (Mask :native))]
      (if (or (>= i 50) (not (simd/any live)))
        live
        (let [zi2 (+ (* (* 2.0 zr) zi) civ)
              zr2 (+ (- tr ti) cr)
              tr2 (* zr2 zr2)
              ti2 (* zi2 zi2)
              ;; sticky: once a lane escapes it stays out, whatever inf/NaN follows (the scalar returns at the first escape)
              live2 (simd/and live (simd/le (+ tr2 ti2) 4.0))]
          (recur (+ i 1) zr2 zi2 tr2 ti2 live2))))))

;; One byte: eight pixels from x0, the first the high bit; pixels at or past n are zero bits.
(defun pack (n: i64 bx: i64 ci: f64) -> i64
  (let [w (simd/native-lanes f64) iota (simd/iota f64xn)]
    (loop [g 0 acc 0]
      (if (< g (quot 8 w))                                      ; w <= 8; on w = 1 this is the scalar loop
        (let [x0 (+ (* bx 8) (* g w))
              xs (+ (simd/splat f64xn (double x0)) iota)      ; the x of each lane
              cr (- (/ (* 2.0 xs) (simd/splat f64xn (double n))) 1.5)   ; same operations as the reference; a variable is splat
              in-row (simd/lt xs (simd/splat f64xn (double n)))  ; lanes inside the row (the reference's (< x n))
              m (simd/and in-row (inside-lanes cr ci))
              bits (simd/mask-bits (simd/reverse m))]          ; reverse: lane 0 becomes the high bit of the group
          (recur (+ g 1) (bit-or (shl acc w) bits)))
        acc))))
;; row and main are the reference's.
```

(`simd/all-true` is `(simd/splat (Mask n) true)`.) Expected effect (*estimate*): the reference does 4x fewer iterations per
byte of work on AVX2 for the dependent multiply-add chain; C's 10.3 s and Java's 10.5 s are scalar here, so a packed version
should be about 3 to 4x faster than all three (a benchmark-rules question: the Benchmarks Game accepts any algorithm that
prints the same bytes).

**n-body** (reference: `scripts/shootout/n-body/n-body.fib`; 5 bodies, 50M steps). The reference keeps `x y z vx vy vz m` per
body and updates pairs; in lanes the natural layout is *field-major* (structure of arrays), the bodies padded to a
multiple of `W` with zero-mass bodies placed far apart. Each body `i` is compared with all bodies `j` in vector blocks
(twice the pair interactions, a quarter of the instructions: a wash at 4 lanes for 5 bodies, a win from 8 up; the
honest expected gain is about 1.5 to 2x, not 4x, and this kernel is the one to measure first):

```
(def P: i64 8)                                 ; bodies padded to a multiple of the widest lane count used (a literal, gap 9 of improvements.md)
;; s = x y z vx vy vz m, each P long: field f of body j is at (+ (* f P) j)

(defstruct Three (a: f64 b: f64 c: f64))

;; the acceleration of body i from every body, one vector block of j at a time (reads only: s is borrowed, so the
;; writes after it find the array unique, as pair-velocities of the reference does)
(defun pull (s: (Array f64) i: i64 dt: f64) -> Three
  (let [w (simd/native-lanes f64)
        xi (array-get s i) yi (array-get s (+ P i)) zi (array-get s (+ (* 2 P) i))]
    (loop [j 0 ax (simd/splat f64xn 0.0) ay ax az ax]
      (if (< j P)
        (let [dx (- (simd/splat f64xn xi) (simd/load f64xn s j))  ; a scalar variable is splat
              dy (- (simd/splat f64xn yi) (simd/load f64xn s (+ P j)))
              dz (- (simd/splat f64xn zi) (simd/load f64xn s (+ (* 2 P) j)))
              d2 (simd/fma dx dx (simd/fma dy dy (* dz dz)))
              mj (simd/load f64xn s (+ (* 6 P) j))
              mag (simd/blend (simd/eq d2 0.0)                 ; the lane j = i (and any exact coincidence): no force
                              (simd/splat f64xn 0.0)
                              (/ (simd/splat f64xn dt) (* d2 (simd/sqrt d2))))
              k (* mj mag)]
          (recur (+ j w) (+ ax (* dx k)) (+ ay (* dy k)) (+ az (* dz k))))
        (Three (simd/hsum ax) (simd/hsum ay) (simd/hsum az))))))

(defun advance (&s: (Array f64) dt: f64) -> i64
  (do (loop [i 0]                                              ; velocities, all from the old positions
        (if (< i 5)
          (let [a (pull @s i dt)]
            (do (array-set! &s (+ (* 3 P) i) (- (array-get @s (+ (* 3 P) i)) (. a a)))
                (array-set! &s (+ (* 4 P) i) (- (array-get @s (+ (* 4 P) i)) (. a b)))
                (array-set! &s (+ (* 5 P) i) (- (array-get @s (+ (* 5 P) i)) (. a c)))
                (recur (+ i 1))))
          0))
      (loop [j 0 dtv (simd/splat f64xn dt)]                    ; positions: x += dt*vx, three blocks; dt splat once
        (if (< j P)
          (do (simd/store! &s j (+ (simd/load f64xn @s j) (* dtv (simd/load f64xn @s (+ (* 3 P) j)))))
              (simd/store! &s (+ P j) (+ (simd/load f64xn @s (+ P j)) (* dtv (simd/load f64xn @s (+ (* 4 P) j)))))
              (simd/store! &s (+ (* 2 P) j) (+ (simd/load f64xn @s (+ (* 2 P) j)) (* dtv (simd/load f64xn @s (+ (* 5 P) j)))))
              (recur (+ j (simd/native-lanes f64)) dtv))
          0))))
```

The velocity loop reads positions only, so updating velocities body by body is the same as the reference's pairwise
update in exact arithmetic; the rounding differs, and the program's printed energies (9 digits) are the check. A
velocity update must not read `@s` while holding a derived borrow of it across a write; `pull` takes `s` borrowed and
returns scalars, the shape of `pair-velocities` in the reference.

**spectral-norm** (reference: `scripts/shootout/spectral-norm/spectral-norm.fib`). `A(i,j) = 1 / ((i+j)(i+j+1)/2 + i + 1)`.
For `n` below about 9e7 every intermediate is an integer below 2^53, so the index arithmetic is **exact in `f64`**, and no
integer vector or integer-to-float conversion (which AVX2 cannot do for 64-bit lanes in one instruction) is needed:

```
(defun mul-av (n: i64 v: (Array f64)) -> (Array f64)
  (let [o (cell (array n 0.0)) w (simd/native-lanes f64) iota (simd/iota f64xn)]
    (do (loop [i 0]
          (if (< i n)
            (let [fi (double i)
                  s (loop [j 0 acc (simd/splat f64xn 0.0)]
                      (if (< j n)
                        (let [ij (+ (simd/splat f64xn (double j)) iota (simd/splat f64xn fi))   ; i + j, per lane
                              den (+ (/ (* ij (+ ij 1.0)) 2.0) fi 1.0)                          ; exact for integers < 2^53
                              a (/ 1.0 den)                                                           ; the literal broadcasts
                              m (simd/tail-mask f64xn (- n j))                                  ; all true except in the last block
                              x (simd/load-masked f64xn v j m (simd/splat f64xn 0.0))]                             ; masked-off lanes are not read
                          (recur (+ j w) (simd/fma a x acc)))                                   ; a finite, x = 0 in dead lanes
                        (simd/hsum acc)))]
              (do (array-set! &o i s) (recur (+ i 1))))
            nil))
        @o)))
;; mul-atv: den uses (+ ij ...) with the roles of i and j exchanged (i + j) (i + j + 1) / 2 + j + 1 with fj the vector of j.
```

(The tail needs a mask only for the last block; the check `(- n j)` at least `w` folds the mask to all-true elsewhere,
and LLVM peels it. `fma` makes the sum differ from the reference in the last bits; the 9 printed digits are the check.)
Expected (*estimate*): the division dominates (`vdivpd` ymm is ~4 cycles throughput per 4 lanes); about 2.5 to 3.5x of the
scalar 1.35 s.

---

## 3. Part 2: `fib.tensor`

### 3.1 What a tensor is, and the decisions in one table

| Question | Decision (**Proposed**) |
|---|---|
| Type | `(Tensor T)`: a counted struct (a library `defstruct` with private fields) holding one `(Array T)` buffer, an `offset`, and `shape`/`strides` in one small `(Array i64)` |
| dtype | `f32 f64 i32 i64` in v1 (`i8 i16 bool` later; a mask tensor is `(Tensor i8)` of 0/1 until then) |
| Rank | **dynamic** (0 to 8) in the type; kernels check rank once per call; a statically ranked family can be layered later with the nat arguments of Part 1 |
| Layout | strided, any order; row-major (C) by default |
| Views | `slice transpose reshape diagonal broadcast-to flip`, `row`, `col`: new struct sharing the buffer by a count, **no element copy** |
| Value semantics | a tensor is an immutable *value*; `aset` and friends are in place when the buffer is unique (the project's rule), else copy-on-write. **No aliasing writes** |
| Writable views | none in v1; the language feature is designed in `docs/design/exclusive-views.md` (decision 3), scheduled as P10b |
| Elementwise | eager functions; the library's fusion rewrite turns nested calls and single-use lets into one loop (3.6) |
| Reductions | `(reduce f t)` is Clojure's left fold; `sum`/`prod`/`max` are the SIMD pairwise ones |
| Matmul | our own blocked kernel with a SIMD micro-kernel; BLAS through `extern` optional |
| Unsafe | only inside the library: kernels run unchecked after the bounds are established once when a view is built and once per kernel call |

### 3.2 The type, representation, rank

```
(defstruct (Tensor t)                       ; fields private to fib.tensor
  (buf: (Array t))                          ; the one buffer, shared by views
  (meta: (Array i64)))                      ; [offset, rank, shape_0 .. shape_{r-1}, stride_0 .. stride_{r-1}], strides in elements
```

One metadata array rather than three fields keeps a view at two small allocations (the struct and `meta`) however
high the rank, and `meta` is a scalar array, so counting it is one count. The element offset of index `(i0 .. ik)` is
`offset + sum(ik * stride_k)`.

**Static or dynamic rank.** Arguments for static rank `(Tensor T r)`: shape and strides could be fixed-size
inline data (no `meta` allocation), `(aget t i j)` would be arity-checked at compile time, `mmul` would be defined only for
rank 2, and `transpose` of a 2-tensor is a 2-tensor. Arguments against, which decide it: (a) the result rank of `reshape t
[a b c]`, `sum t axis`, `slice t [i :all]` (a dropped axis), `broadcast-to`, `diagonal` is computed from *values* (a vector
literal's length, an axis number); the checker has no type-level arithmetic, only equality of nat arguments (2.3), so each
of those would need either a rank-specific function family (`slice-1-of-2` ...) or an existential; (b) `core.matrix`, the
Clojure model, is dynamic (`(shape m)` is a vector, `(dimensionality m)` a number) and Clojure ergonomics win the tie-break
when memory safety is not at stake; (c) the per-call cost of a rank check is a compare, once per kernel call, not per
element. Static rank is what Rust's `ndarray` offers (const-generic `Dim`s): its errors are earlier, its signatures
heavier. **Decision: dynamic**, with `(Tensor T)`; rank-1 and rank-2 conveniences `(Vector T)` and `(Matrix T)` may be
added as thin *checked wrappers* (`(as-matrix t)` traps unless rank is 2) once the nat arguments exist. Rank 0 (a scalar
tensor) exists so `(sum t)` is a tensor operation that can be uniform; `(sum t)` returns the element, `(sum t 0)` a rank-reduced tensor.

**dtype.** `T` is any scalar the kernels support: the four of v1. A tensor of objects is not a tensor (use `Vec`). `bool`
masks come with the SIMD masks later. There is no unsigned type in fibber (`spec/syntax.md` 1: "no unsigned types"), so no `u8` image
tensors in v1.

### 3.3 Invariants and construction

The type's fields are private to `fib.tensor` (a `defstruct` with `:private` fields, or the struct kept unexported), so user code
cannot build a `Tensor` that breaks the invariant, and the unsafe kernels rely on it:

> **Invariant I.** `rank <= 8`; every `shape_k >= 0`; `meta` has `2*rank + 2` entries; and for every index tuple in range the
> element offset `offset + sum(i_k * stride_k)` lies in `[0, (array-len buf))`. For an empty tensor (some `shape_k = 0`) `I` is vacuous.

`I` is established when a view is *built*, not when it is read: each constructor computes the minimum and maximum offset
over the index box in `O(rank)` with **checked** arithmetic (so `shape` products and offsets that overflow `i64` trap, they
do not wrap), and traps `tensor: view out of range of its buffer` if either end is outside the buffer. `slice`, `transpose`,
`reshape`, `broadcast-to`, `diagonal`, `flip` produce new `meta` from an existing one by rules that preserve `I` (3.4), and
`(tensor-from-parts buf offset shape strides)` (public, for interop) *checks* it.

### 3.4 Ownership, views, the lifetime rule, mutation

**What a view is.** A view is a new `Tensor` struct whose `buf` field is the *same* `(Array T)` object as the owner's, held
by one count. Slicing a million-element matrix copies no element and allocates one struct and one `meta` (a few dozen
bytes). That is "no allocation of data", not "no allocation": the prompt's "views cost no allocation" is not literally
achievable when a view must be a first-class struct, but a view that does not escape its function is stack-allocated by the
existing rule (`spec/types.md` 6.11 "stack when scope-local"), and its `buf` count is one non-atomic increment and decrement.
**Measure this first** (5, P5): if stack allocation of the view and the retain pair do not show up, no change; if they do,
the optimisation is to treat the `buf` read as a derived borrow of the owner within one expression (the rule of `spec/types.md`
6.3, "Element reads"), which is a small extension of what batch 4 did for `array-get`.

**Why not a `Derived` borrow of the owner.** `Derived(b)` is valid only while `b` is alive and, crucially, cannot be stored:
a value in a struct field, a `Vec` element, a return position is *consumed* and, if derived, retained (`spec/types.md` 6.3:
`Derived(p)` "at a consume position is retained"; "the element cannot outlive the array", 6.3 "Element reads"). A view that
can be returned, stored in a `Vec`, passed to a task or kept in a `loop` variable therefore must hold its own count. This is
the sound choice and the one that costs a count, not a copy. The lifetime rule of a view follows from it:

> **View lifetime.** A view keeps its buffer alive by owning one count of it; the owner tensor going out of scope releases
> its count, not the buffer. The buffer is freed when the last view or owner is released. No view dangles; there is no
> lifetime parameter to write.

**Mutation.** A tensor is a value. Update functions return a tensor and take `&` for the in-place form:

```
(aset t [i j] x)          -> (Tensor T)    ; a new tensor: copies the buffer unless `t` is unique and the buffer is unique
(aset! &t [i j] x)        -> unit          ; in place; `t` a cell; the unique-write protocol (spec/types.md 2.13.1, 6.6)
(add! &c x)               -> unit          ; c += x elementwise, in place when unique
```

In place happens exactly when `fib.unique?` holds for the buffer, i.e. count 1 with none of `SHARED`, `HAS-WEAK`, `STACK`,
`IMMORTAL` (`docs/design/in-place-update.md` 1). The consequence for views is the point of the design:

> **Mutation through a view.** There is none. If `c` is a unique tensor and `v = (slice c ...)` is a view of it, the
> buffer has count 2, so `(aset! &c ...)` copies the buffer first: `v` keeps the *old* values, `c` gets its own buffer.
> A write never changes what a view shows. (Rust has `&mut` slices and the borrow checker; Clojure's `core.matrix`
> mutates shared storage; fibber's rule, "a shared array is never written", picks neither, and gives a semantics with no
> aliasing and no iterator invalidation. It costs a copy only for a program that keeps a view alive across a write to its owner.)

What the in-place programs look like, then: the owner is a `let` cell (as `n-body`'s state array is) or a unique value
passed along, views are made and **dropped** before the write, and the kernels write into an output tensor they own
(3.5). The pattern "fill the blocks of C by writing through block views" is a *library* pattern: the kernel works on the
raw buffer after one `array-make-unique!` (3.5); user code states `(mmul a b)` or `(mmul! &c a b)`.

*Writable views (decision 3: designed).* `&` is a variable's private cell and "there is no field place" (`spec/types.md` 2.14, D1), so a
window of *part* of a cell's content with a lexical extent is new ground. `docs/design/exclusive-views.md` designs it:
`(with-view [v (slice! &t 0 8)] (fill! &v 0.0))`, a scoped exclusive borrow of a private cell whose windows cannot escape the form,
with the owner frozen for its extent, plus draft spec rows. It is a post-v1 package (5, P10b); v1 keeps `(update-slice! &t spec f)`
(copy the window out, apply `f`, write it back) for the user-level need.

**Reading.** `(aget t i j)` is a borrowed element read: the tensor is borrowed, the element is a scalar (no mode), so
no count. In a loop it is `offset + i*s0 + j*s1` and a load with one bounds check against `(array-len buf)`, which LLVM
may hoist if the loop is simple; user loops are *not* the fast path (3.5), library kernels are.

### 3.5 The trusted-kernel discipline

The performance of `fib.tensor` comes from kernels that run **without** per-element checks. fibber's unsafe is lexical
(`unsafe` blocks, the raw-pointer builtins `ptr+ load-i8 .. load-ptr store-* alloc free raw`, `spec/syntax.md` 3.15, `spec/syntax.md` 1587):
user code never needs it, and the library may use it. The discipline that makes the unchecked inner loops safe:

1. **Private representation.** The invariant `I` (3.3) holds for every `Tensor` value because only `fib.tensor` can construct one.
2. **Establish bounds when the value is built** (3.3), never in the loop. A kernel receives tensors whose `I` already holds.
3. **Check once per kernel call** what is not in `I`: ranks and shapes agree (`mmul`: `a.cols = b.rows`), the output is distinct
   from the inputs (it is a different object, or `array-make-unique!` made it so), and the output buffer is unique and of
   the right size: `(array-make-unique! &c)`, a new library primitive that copies a shared array into the cell (or does nothing)
   exactly as the unique-write protocol does but without writing an element; after it no other holder can observe a
   write. These are `O(rank)` compares and are the only checks.
4. **Inside the kernel, raw access.** The kernel takes the buffer's data address (a new unsafe primitive `array-data`, returning
   a `ptr` into the payload at the 24-byte header offset; the kernel holds the tensor borrowed, so the buffer cannot be freed
   during the call) and uses unchecked `simd/load-unchecked`/`store-unchecked` (library-only, `unsafe`) at offsets derived from
   `I`. Every offset is `offset + sum(i_k * stride_k)` for an in-range index, so `I` is the proof; the loops' bounds
   come from `shape`, not from the data.
5. **No pointer survives the call.** Pointers are locals of the kernel.
6. **Soundness is tested by attack**, not claimed (4.1): negative strides, zero-size dimensions, stride-0 broadcasts, a view
   at the very end of its buffer, `shape` products that would overflow, an output that is also an input, and a mutation
   test that makes each check fail and shows the case catching it.

The pieces to add are tiny: `array-data`, `array-make-unique!`, and the unchecked vector load/store (the same lIR instructions as
the checked ones). They are `unsafe`-only builtins (the `BuiltinSig` table has an `unsafe-only` column: `compiler/types/builtins.fib:105`).

### 3.6 API shape and elementwise operations

Names follow `core.matrix` and Clojure where a Clojure name exists (stdlib rule N12, `spec/stdlib.md:1292`):

```
(tensor [[1 2] [3 4]])            ; from nested Vecs: (Tensor i64); (tensor [[1.0 2.0] [3.0 4.0]]) is (Tensor f64)
(tensor f64 [[1 2] [3 4]])        ; the element type named: literals adopt it (the L19 rule)
(zeros f64 [3 4]) (ones f64 [3 4]) (full f64 [3 4] 7.0) (arange f64 0.0 10.0 0.5) (identity f64 3) (rand-tensor f64 [3 4] seed)
(shape t)                         ; a Vec of i64: [2 2]       (rank t)   (count t)  ; total elements, (size t)
(aget t i j)  (aget t [i j])      ; traps out of range; (aget t) of rank 0 is the element
(slice t [0 :all])                ; a row; ranges: [1 3] is [1, 3), (range 0 10 2), :all, and a number drops the axis
(row t i) (col t j) (reshape t [4 1]) (transpose t) (transpose t [1 0]) (diagonal t) (flip t axis) (broadcast-to t [3 4])
(add a b) (sub a b) (mul a b) (div a b)   ; elementwise, with broadcasting; a scalar broadcasts
(mmul a b) (dot a b) (outer a b) (inner a b)
(sum t) (sum t axis) (prod t) (maximum t) (minimum t) (mean t axis) (argmax t axis)
(reduce + t)                      ; Clojure: left fold over elements in row-major order; also (reduce + 0.0 t)
(map f t) (map f a b)             ; elementwise by function (Clojure's map over tensors, eager, returns a tensor)
(seq t)                           ; the elements of the first axis as tensors: Reducible, so (->> t (map ...)) works
(to-vec t)                        ; flat Vec ; (to-nested t) nested Vecs ; (->array t) the contiguous (Array T)
(= a b) (str t) (pr-tensor t)
```

**Broadcasting** is NumPy's rule: align shapes from the trailing axis; an axis of size 1 or a missing axis has stride 0;
other mismatches trap `shape mismatch: [3 4] and [5]`. It is a view operation (`broadcast-to`), so it costs no copy.

**Fusion.** `(add a (mul b c))` as two eager functions allocates a temporary and makes two passes. Options for one loop:

1. *Reuse the lazy-sequence fusion rewrite* (`compiler/expand/fuse.fib`, `fusescan.fib`, `fusetab.fib`, `fuserun.fib`, `fuselet.fib`:
   it recognises nested calls of the library's `map`/`filter`/`reduce` and single-use lets, by name and with shadowing
   checks (`fuse-library`, `fuse.fib:50-60`), and rewrites them into recipe structs and one loop). Add rows to its
   tables for the elementwise tensor functions, and a terminal that materialises (`into-tensor`, `sum`, `reduce`, any
   function that is not elementwise). *For:* nested calls and let-bound single-use intermediates fuse; a literal
   `(fn [x] ...)` argument is inlined into the loop body (the lambda's code is part of the rewritten expression); it is the
   machinery the owner already trusts, and "eager over lazy once use is known" is the stated direction
   (MEMORY: "Specialising is fine"). *Against:* a per-name table, and the rewrite works on expansion, so a tensor function
   passed as a value (`(reduce add ts)`) is not fused (it is still correct and eager).
2. *A lazy expression type*: `add` returns `(Add a b)`, evaluated by `(materialize e)`; Eigen's expression templates, with the
   static dispatch of protocols (`spec/types.md` 4.2) monomorphised and inlined. *For:* no compiler change, works through
   any function boundary. *Against:* closures are not inlinable through it (a closure's type is `(fn κ (A) R)`, not unique per
   literal, so `(map f e)` makes an indirect call per element); an expression that is used twice is computed twice; the
   types of results leak into signatures and error messages; unproven that the monomorphised tree collapses to a flat
   loop in this compiler (risk, 6).

**Decision: option 1 for the default** (eager API, fusion by rewrite, in place when the first operand is unique: the "eager
once use is known" rule), with option 2 *not* built in v1. The rewrite target is one generated function `ew-kernel`: given
output shape, the operand tensors and a body expression of lane-polymorphic arithmetic, it has three loops:

- **contiguous fast path**: all operands row-major dense with equal shape (or a scalar): one flat loop of `native-lanes`-wide
  loads, the body evaluated on vectors (because `+ - * /` work on `(Simd T n)`, the *same body text* works for a vector
  and for a scalar tail element), then one masked tail iteration (2.7);
- **one strided axis**: inner dimension with stride 1 vectorised, stride 0 splatted, other strides gathered (slow) or scalar;
  outer axes by an odometer;
- the **scalar fallback** for rank 0 and tiny inputs.

That the *same body* typechecks for vectors and scalars is what makes the kernel a macro over expression text, and it is
why Part 1's operators work on `(Simd T n)` by the ordinary `Num` instances. Because float order inside an elementwise op
does not change (each lane is one op), elementwise results are **bit-identical** to the scalar loop; only `fma`
contraction would change them, and the rewrite does not introduce one (it fuses *loops*, not operations).

**Reductions.** `(reduce f t)` (Clojure's name) is the left fold in row-major order, exactly `(reduce f xs)`; `sum`,
`prod`, `maximum`, `minimum`, `mean`, `argmax` are the *SIMD* reductions (several accumulators of `native-lanes` lanes, the
pairwise combination of 2.6): fast, deterministic across runs and targets of the same lane count, and **not equal to the
left fold in the last bits for floats**, which the docstring states (NumPy's `sum` is pairwise for the same reason). An
axis reduction `(sum t axis)` of a dense row-major tensor over the *last* axis is a SIMD row sum per row; over an *earlier*
axis it is a vector add of rows into an accumulator row (unit-stride, no horizontal reduce at all).

### 3.7 Matmul and the BLAS question

`(mmul a b)` for rank 2 (and rank 1 against rank 2, `core.matrix` semantics), `(mmul! &c a b)` accumulating-free into an
output. The kernel is the standard blocked GEMM, **our own**, since the owner asked for native tensors and the evidence
(0.5) shows a hand kernel beats the only BLAS on this machine (netlib reference: 12.8 GFLOP/s against 53.6 for a plain
register-tiled kernel):

- loop order `jc` (NC columns of B) / `pc` (KC of the inner dimension) / `ic` (MC rows of A) / micro-kernel, as BLIS/Goto;
- **packing**: copy an `MC x KC` block of A and a `KC x NC` block of B into contiguous, micro-panel-ordered scratch
  arrays (this is where transposed, strided or broadcast operands are normalised, so the micro-kernel only ever sees
  unit-stride panels, and `(mmul (transpose a) b)` costs no extra pass);
- **micro-kernel** `MR x NR` held in registers: for AVX2 f64, `4 x 8` (eight `f64x4` accumulators, two B vectors, four A broadcasts per `k`), for f32 `4 x 16`; FMA
  when `(native-has-fma?)`, `mul` then `add` otherwise (so results then differ in the last bit between targets, as BLAS
  results do; matmul cases compare with a relative tolerance and the elementwise cases are exact, 4.1);
- edge tiles use the masked load/store of 2.7 (no scalar epilogue);
- block sizes from a table keyed by `native-lanes` and a conservative L1/L2 guess (KC 256, MC 128 to 256, NC 2048 or `n`); autotuning is not v1.

The pseudo-code of the micro-kernel (the rest is packing and loops, mechanical):

```
;; c (a 4 x 2W tile at c-ptr, row stride ldc) += sum over k of a-panel[k][0..3] (x) b-panel[k][0..2W)
(defun micro-4x2 (kc: i64 ap: ptr bp: ptr cp: ptr ldc: i64) -> unit
  (unsafe
    (let [w (simd/native-lanes f64)]
      (loop [k 0
             c00 zero c01 zero c10 zero c11 zero c20 zero c21 zero c30 zero c31 zero]     ; eight f64xn accumulators
        (if (< k kc)
          (let [b0 (simd/load-unchecked f64xn bp (* k (* 2 w)))
                b1 (simd/load-unchecked f64xn bp (+ (* k (* 2 w)) w))
                a0 (simd/splat f64xn (load-f64 (ptr+ ap (* 8 (* 4 k)))))                  ; a-panel is packed 4 per k
                a1 ... a2 ... a3 ...]
            (recur (+ k 1) (simd/fma a0 b0 c00) (simd/fma a0 b1 c01) (simd/fma a1 b0 c10) (simd/fma a1 b1 c11)
                           (simd/fma a2 b0 c20) (simd/fma a2 b1 c21) (simd/fma a3 b0 c30) (simd/fma a3 b1 c31)))
          (do (add-tile! cp ldc 0 c00 c01) (add-tile! cp ldc 1 c10 c11) (add-tile! cp ldc 2 c20 c21) (add-tile! cp ldc 3 c30 c31)))))))
```

**BLAS through `extern`.** The interface exists (`extern` with C calling convention, `spec/syntax.md` 3.15; Fortran `dgemm_`
takes every argument by pointer, so `unsafe` code in the library allocates scalars as 8-byte cells and passes `array-data`
pointers). It would be an *optional module* `fib.tensor.blas` with `(mmul-blas a b)` and a link flag (`-l openblas`), selected explicitly,
never by default: the reference BLAS here is slower than our kernel, and a missing library must not change the meaning of
`mmul`. If OpenBLAS or MKL is later present, the module is where a benchmark can decide. `dgemm`'s column-major layout means
`C = A*B` row-major is `dgemm(B, A)` (the C baseline in `mm/mm.c` does exactly that).

### 3.8 Threads

`spawn` requires a `Send` closure and result (`spec/types.md` 1.2 table; 5.2). A `(Tensor T)` of scalars is `Send`. The trap:
**sharing a value with a task marks it `SHARED`** (`spec/types.md` 8.8), and `fib.unique?` is false for `SHARED`, so a tensor
that has crossed to a task can never again be updated in place. A parallel matmul that splits C by row blocks must give each
task *its own* output block (an `(Array f64)` it owns) and concatenate (one copy, `O(n^2)` against the `O(n^3)` work), or the
library must hand tasks disjoint raw regions of one output inside `unsafe` with a trusted `Send` raw-region type. Both are
library-internal, deferred past v1 (5, P10); the design constraint recorded now is "no kernel API may assume the buffers are
unshared after a task has seen them".

### 3.9 Printing, equality, interop

- `(= a b)`: same shape and all elements equal (floats by `=`: NaN unequal, `0.0 = -0.0`). Not `Hash`.
- `(str t)`: `#tensor[[1.0 2.0] [3.0 4.0]]` for up to 64 elements, else `#tensor<f64 [1024 1024]>`; `(pr-tensor t)` is the full text.
  Elements use the scalar `show` (Java's `Double.toString` text, `spec/types.md` 2.12), so a printed tensor reads back with
  `(tensor ...)` on the vector part.
- `(tensor (Array f64))` shares the array (rank 1, no copy: one count). `(->array t)` returns the buffer when the tensor is
  dense, row-major, offset 0 and covers the whole buffer (a count, no copy), else a fresh packed array. `(tensor v)`,
  `(to-vec t)`, `(to-nested t)` convert to and from `Vec`s with copies. A tensor `Reducible`s as its elements in row-major
  order (`spec/stdlib.md` `Reducible`, line 554), so `(reduce + t)`, `(into [] t)` work; `(seq t)` iterates the first axis.

### 3.10 Version 1 and later

**v1:** the type, construction, `shape/rank/aget/aset`, the view operations, printing/equality, interop; elementwise `add sub
mul div neg abs sqrt min max` (no transcendental functions in v1) with broadcasting, the fused kernel and the fusion
rewrite; `sum prod maximum minimum mean argmax` with axes; `dot axpy`; `mmul mmul!` for f32/f64/i32/i64 (the SIMD micro-kernel for the floats; i32/i64 a plain
blocked loop with checked or `wrapping` accumulators); the benchmarks.
**Later:** threads; i8 i16 bool tensors; `einsum`; convolution; FFT; linear algebra (`solve`, `lu`, `qr`, `svd`); transcendental
functions (`exp log sin tanh`: a vectorised polynomial implementation, exact-vs-fast question); sparse; a statically ranked
`(Matrix T)`; writable views (`docs/design/exclusive-views.md`, P10b); GPU (out of scope); BLAS module; autotuned block sizes; multiversioning.

---

## 4. Cases and benchmarks

### 4.1 The cases that must exist

All cases are `cases/stdlib`-style programs with verdicts in the header (`spec/method.md`: nothing is done until an executable test says so). Numbers: SIMD 6200 to 6299, tensor 6300 to 6399
(the regex cases are 6100-6101 and BigInt 6000-6099, commit ea66335). The Rust `fibref` is frozen and cannot run these
(`docs/shootout/improvements.md` item 10). **Decision 5 (owner): rule 6 (interpreter against compiler) no longer judges new features;**
a fibber interpreter will be written (package INT0) and becomes the second tool then. Until it exists, each SIMD operation is
tested against a **scalar reference written in the case itself**, with the ordinary scalar operators and loops, and the case compares
the vector result with it. The lIR scenarios of liar's `cert/features/vector_types.feature` (integer and float vector literals of the
widths `<4 x i32> <2 x i64> <8 x i8> <16 x i8> <4 x float> <2 x double> <8 x i16>`, negative lanes, `<N x i1>` literals, lanewise
`add fadd mul`, `icmp eq` and `fcmp olt` giving `<N x i1>`, a splat written as a literal) are reused as inputs: P0 turns each
scenario into a `cases/lir` case with the expected value from the feature file, and P4 repeats the same inputs through `<<..>>`.

*SIMD (Part 1).*
1. **Every operation against a scalar reference.** For each of `+ - * /`, `neg`, `abs`, `sqrt`, `floor ceil trunc round-even`, `min max min-num
   max-num`, `fma`, lanewise compares, `blend`, `shl shr sar`, bit ops, `popcount`, `convert`: for `f64x2 f64x4 f32x4 f32x8 i32x4
   i64x2 i8x16` and `n = 3, 5` (odd widths), compute on 2000 seeded pseudo-random vectors (an in-case LCG, the seed in the header),
   and on a fixed list of **edge values** (`0.0 -0.0 1.0 -1.0 inf -inf NaN 5e-324 MAX MIN 1.0000000000000002`; integers `0 1 -1 MIN MAX
   MIN+1`), comparing each lane with the scalar operation by **bits** (`f64->bits`) except NaN, which compares by `(!= a a)`,
   and `-0.0` vs `0.0` by bits. `fma` is compared with a reference computed in exact arithmetic (two-product and two-sum) on
   the edge list.
2. **IEEE semantics**: `(/ 1.0 0.0)`, `(/ 0.0 0.0)`, `(sqrt -1.0)`, `(max NaN 1.0)` is NaN and `(max-num NaN 1.0)` is `1.0`, `(min 0.0 -0.0)` is `-0.0`, `lt` is false on a NaN and
   `ne` true, `(= v v)` with a NaN lane is false; each equals the scalar `+ - * / < max` result on the same inputs.
3. **Integer overflow traps**: `(+ <<MAX 1>> <<1 1>>)` traps with `integer overflow in + at i64x2` (exit code and stderr per the case
   header), also when only the *last* lane overflows; the `unchecked-add` and `wrapping` forms wrap and equal the scalar wrap; `neg` of `MIN` traps.
4. **Memory**: `load`/`store!` at `i = len-n` ok and `len-n+1` traps; `i` negative traps; empty array; a masked load whose
   inactive lanes would be past the end does **not** trap and returns the passthru in those lanes, while one active lane past
   the end traps; gather with one bad index traps and
   writes nothing; scatter with duplicates: highest lane wins; `store!` on a shared array leaves the other holder's copy
   unchanged (the in-place protocol); `lane` with index `n` traps.
5. **Tails**: `sum`, `dot`, `axpy!`, a map of `f(x) = x*x + 1` over arrays of length `0 .. lanes+1` and `2*lanes+1`, each against
   the scalar loop; `(simd/load-tail ...)` zero fills; `store-tail!` writes only the tail and the elements after the tail are unchanged.
6. **Reduction order**: `reduce-add` on `f64x4` of `<<1e16 1.0 -1e16 1.0>>` equals the pairwise value pinned in the case, `reduce-add-ordered` equals the left fold; for
   i32x4 near `MAX` the checked reduce traps.
7. **Literal and types**: `<<1 2 3 4>>` is `(Simd i64 4)`; `<<1.0 2.0>>f32` reads at f32 (0.1 is rounded to f32); `<<1 2>> + <<1 2 3>>` fails with the length error; `(+ <<1 2>>i32 1.0)` fails (no
   promotion); `(* v 2)` and `(+ 5 <<1 2 3>>)` broadcast; `(* v s)` with a scalar variable `s` is the `write (splat x)` error whatever the order of inference, and `(* v (splat s))` works; `(+ <<1 2>> 1.5)` fails; a mixed `f32`/`f64` pair fails with no promotion; vectors in struct fields, `Vec` elements, closure captures, a
   `spawn` result, and `dyn`-free generics (`(defun f (a: (Simd f64 n)) ...)`) work and `fibc explain` shows no count operation for them.
8. **Targets**: the whole group is built and run under `FIB_TARGET_CPU` unset (AVX2 here), `x86-64` and `x86-64-v2` (SSE only, so
   `native-lanes f64` is 2) and compared output for output; a *compile-only* run with `skylake-avx512` (this CPU cannot run it) checks the lowering
   does not fail and `--emit asm` contains `zmm` or `ymm` as chosen.
9. **A case must be able to fail**: a mutation harness replaces one lane operation in the library (e.g. `shuffle` indices, `fma` by `mul+add`) and the group must go red.

*Tensor (Part 2).* construction and `shape`; every view operation against an explicit index loop (including negative strides
via `flip`, zero-size, rank 0, stride-0 broadcast, a view at the end of its buffer); view lifetime (a view returned from
a function after its owner is dropped, a view stored in a `Vec`: no leak and no use-after-free under the audit, `fibc` trace
allocation counts pinned); the **no aliasing write** rule (a view keeps the old values after `aset!` on the owner); in-place
`aset!`/`add!` on a unique tensor with allocation count 0 beyond the first copy; elementwise ops vs scalar loops incl. all
broadcasting shapes; fused vs unfused agree bit for bit and the fused version allocates no temporaries (`FIB_TRACE` count);
reductions and axis reductions vs loops (exact for integers, within `n * eps` for floats, and bit-equal to the pinned pairwise value
on small cases); matmul vs the triple loop for all `(m n k)` in `1..2*MR+1` against `1..2*NR+1` (edges), for transposed and sliced operands, rectangular and tall/skinny, i32 and
i64 exactly, f32/f64 with relative tolerance 1e-5 and 1e-13; the trusted-kernel attacks of 3.5 (each must trap at the
constructor or be correct, never read outside the buffer: run them with the buffer allocated by an `array` with a canary
neighbour and check the canary); printing and equality; Vec/Array interop.

### 4.2 Benchmarks (what each is judged against)

Targets use the *measured* column of 0.5. Anything not measured here is to be measured by the package that builds it, on a quiet machine, with
`scripts/shootout/run.sh`-style serialisation.

| Benchmark | Inputs | Baselines | Fibber target |
|---|---|---|---|
| matmul f64, f32 | n = 512, 1024, 2048 | measured in 0.5: C blocked (the ceiling), Java Vector API, Java/Clojure scalar, netlib BLAS (OpenBLAS absent; it is the comparison to add if installed) | f64 1024 at least 30 GFLOP/s (Java Vector API: 28.35), 45 or more is C; f32 at least 2x the f64 figure |
| dot, axpy | 10^3, 10^6, 10^8 elements (cache, L2, memory) | C `-O3 -march=native`, Java (`Vector API` and plain loop; the JIT autovectorises the plain `axpy`), Clojure `double-array` loop; **not measured in this session** | dot within 20% of C at 10^6; at 10^8 memory-bound parity |
| 5-point stencil | 2048x2048 f64, 100 sweeps | same | within 25% of C; needs strided views for the shifted neighbours: `(slice t [1 -1] [1 -1])` etc. |
| softmax | rows of 1000, batch 4096, f32 | same; needs `exp` (so after the transcendental work; measured with a scalar `exp` first) | after `exp`, within 2x of C |
| dense layer forward | `y = relu(W x + b)`, W 1024x1024, batch 128 | same | = matmul rate plus one fused elementwise pass (no temporary) |
| shootout kernels | mandelbrot, n-body, spectral-norm in `fib.simd` (2.13) | the shootout table of `docs/shootout/improvements.md` | mandelbrot 3x faster than the current scalar 10.4 s, spectral-norm 2.5x, n-body 1.5 to 2x |

The tensor program forms the benchmarks take (pseudo-code in the proposed API; each fuses into one loop by 3.6):

```
(defun axpy (a: f64 x: (Tensor f64) y: (Tensor f64)) -> (Tensor f64) (add (mul a x) y))        ; one pass, in place if y is unique
(defun dot (x: (Tensor f64) y: (Tensor f64)) -> f64 (sum (mul x y)))                          ; the fused reduction: no temporary
(defun sweep (u: (Tensor f64)) -> (Tensor f64)                                                ; 5-point stencil of the interior
  (let [c (slice u [1 -1] [1 -1]) n (slice u [0 -2] [1 -1]) s (slice u [2 nil] [1 -1])
        w (slice u [1 -1] [0 -2]) e (slice u [1 -1] [2 nil])]                                 ; five views of u: no element copied
    (mul 0.2 (add (add (add (add c n) s) w) e))))                                             ; one fused pass over the interior
(defun softmax (x: (Tensor f32)) -> (Tensor f32)                                              ; along the last axis
  (let [m (maximum x -1)                                                                       ; shape [rows]
        e (exp (sub x (reshape m [-1 1])))                                                    ; broadcast, fused with exp
        s (sum e -1)]
    (div e (reshape s [-1 1]))))
(defun dense (w: (Tensor f32) b: (Tensor f32) x: (Tensor f32)) -> (Tensor f32)                ; x [batch in], w [in out]
  (maximum-with 0.0 (add (mmul x w) b)))                                                      ; the add and relu fuse into one pass
```

---

## 5. Implementation plan

Packages (a package is one agent-sized unit with its own tests). Dependencies in the third column. The order the owner asked
for is SIMD intrinsics, tensor core, fusion, matmul; because the tensor *core* needs no SIMD (scalar loops first) it can run in
parallel with the first SIMD packages, which is the one place the order is relaxed.

| Pkg | Content | Depends on | Files it touches |
|---|---|---|---|
| **P0** lIR additions (2.10): `fma fsqrt fabs ffloor.. fmin.. smin..`, vector casts, masked load/store, gather/scatter, `reassoc`/`contract` flags, module `target` form | none | `spec/lir.md` (6.1, 6.3, 6.5, 14), `compiler/lir/{ast,parse,check,print,dump}.fib` and `compiler/lir/check/`, `compiler/native/lower/{arith,memory}.fib`, `crates/lir` and `crates/lair` (Rust, unit test each), `cases/lir/instr/`, the compare scripts in `compiler/tests/native/`. Shares `fsqrt`, `reassoc` with `docs/design/vectorisation.md` and with gap L11 (`sqrt`) |
| **P1** types and checker: `(Simd T n)`, nat arguments, sugar names, layout class `vec`, built-in `Num/Bits/Eq/Show` rows, broadcast constraint, scalar-class treatment in the ownership pass | none (checker only); emission needs P0 only for the new instructions, not for `+ - *` | `compiler/types/` (types, builtins, schemes), `compiler/own/`, `compiler/emit/lower/` (arith, types), `spec/types.md` 1, 2.12, 8.1, 8.12 |
| **P2** reader and the literal: `<<...>>`, suffix, `(simd ..)` | none | `compiler/syntax/lexer.fib` and `reader.fib`, `spec/syntax.md` 1 |
| **P3** target info: `Target` record, `FIB_TARGET_CPU` features table, `native-lanes`, the module `target` form honoured by `lair`/`lairf` | P0 (the form) | `compiler/native/target.fib`, `compiler/llvm/target.fib`, `compiler/driver/`, `compiler/emit/` (context), `compiler/tests/emit/` (pin `FIB_TARGET_CPU=x86-64` for golden comparisons) |
| **P4** `fib.simd` library and cases 6200-6299 | P0, P1, P2, P3 | `lib/fib/simd.fib` and `lib/fib/simd/`, `lib/prelude.fib` (the primitive names), `cases/stdlib/62xx` |
| **P4b** the wrapping family: `unchecked-add/subtract/multiply`, `wrapping` macro (L10) | P1 (flag in the lowering) | `compiler/emit/lower/arith`, `lib/`, `spec/stdlib.md` 7 L10 |
| **P5** SIMD kernels of the shootout (2.13) and their measurement | P4 | `scripts/shootout/*/*-simd.fib`, `docs/shootout/` (the first end-to-end evidence; run this **before** P7 and P8) |
| **P6** tensor core: struct, invariants, construction, views, `aget/aset/aset!`, broadcast, printing, equality, interop, scalar kernels, cases 6300+ | none for scalar code | `lib/fib/tensor.fib` and `lib/fib/tensor/`, `spec/stdlib.md` (a new section), `cases/stdlib/63xx`; includes the **stack-allocation measurement** of views (3.4) |
| **P6b** trusted-kernel primitives: `array-data`, `array-make-unique!`, unchecked vector load/store | P0/P1 for the vector ones | `compiler/types/builtins.fib` (rows, unsafe-only), `compiler/emit/lower/builtins.fib`, `crates/fibc/rt/array.lir` (regenerate `compiler/emit/runtime.fib`), `spec/types.md` 2.13.1 |
| **P7** SIMD elementwise and reduction kernels (`ew-kernel`, `sum`, axis reductions) and cases | P4, P6, P6b | `lib/fib/tensor/` |
| **P8** fusion rewrite for tensor functions | P6, P7 | `compiler/expand/{fusetab,fuse,fusescan,fuselet,fuserun}.fib`, `compiler/tests/expand/` (the Rust expander is frozen, so only the stage-2 compare scripts see it; `fibref` stays unfused and must give the same values) |
| **P9** matmul: packing, micro-kernels, blocking, edges, cases, benchmark | P4, P6b, P7 | `lib/fib/tensor/gemm.fib` |
| **P10** later: threads, BLAS module, multiversioning, transcendental functions | after v1 | |
| **P10b** exclusive views: `with-view`, scoped types, library `slice!`/`split!`, from `docs/design/exclusive-views.md` | P6, P6b; owner approval of the spec drafts | `spec/types.md`, `spec/syntax.md`, `compiler/{types,own,expand}`, `lib/fib/tensor/` |

Independence: P0, P1, P2, P6 can start the same day (four agents) and touch different directories; P3 follows P0's form;
P4 is the merge point; P6b is small and can ride with P0 or P6. The 120-call agent budget of this task is about one package: P0
and P1 are each a full package, P4 two.

Verification gates, per package, as the project requires: the relevant `fibc cases` groups, `compiler/tests` compare scripts
(`lairf` against `lair` for P0), the stage check of CLAUDE.md (a new instruction or type must not break stage 2 self-hosting:
the compiler's own source uses no vectors, so the check is that `fibc emit compiler/fibc.fib` is unchanged), `cargo fmt/clippy/test` for the
`crates/` edits of P0, and the seed-built benchmark runs under the lock.

---

## 6. Risks

1. **Fast-math and FP contract.** `fma` has one meaning (exact), so a CPU without FMA runs a libm call per lane: slow, and
   `(native-has-fma?)` must guard every hot use. Contracted vs uncontracted results differ in the last bits across targets;
   tolerances in matmul cases and nothing weaker for elementwise.
2. **Broadcast is literals and `splat` only** (decision 2): no deferred constraint, no inference-order risk; the cost is a `(splat x)` at each variable use.
3. **Reader change.** `<<` and `>>` were symbol characters; no program uses them (3.3 grep), but a user macro that builds `>>` text
   would. The rule keeps a bare `<<`/`>>` as symbols.
4. **Odd lane counts and ABI.** Unusual widths cross calls badly (0.4); the cap of 64 and the advice to use powers of two limit it.
5. **Per-view allocation.** A tensor view is a struct plus a `meta` array; in a tight loop of small slices that dominates.
   Mitigated by stack allocation (to be measured) and by kernels taking whole tensors.
6. **Fusion by rewrite** depends on name resolution (`fuse-library`) and misses function values; and the expression-type
   alternative is unproven. The fallback is an explicit `(ew [a b c] (fn [x y z] ...))` macro that always generates one loop.
7. **`SHARED` after a task** kills in-place updates (3.8).
8. **The interpreter and the Rust tools are frozen** and rule 6 no longer judges new features (decision 5); the in-case scalar references are the
   substitute and they are weaker (same compiler on both sides) until INT0 gives an independent second implementation.
9. **Compile time.** Many monomorphised vector types and unrolled legalisation (`<64 x f64>`) can blow up code size; the 64-lane cap and
   512-bit cap are the guard; measure `compile-time.sh` (`scripts/bench/compile-time.sh`) after P1.
10. **Target table** for CPU features is a maintained list; a wrong entry gives wrong lane counts but never wrong results.
11. **No AVX-512 hardware here**: the 512-bit paths compile but cannot be run on this machine.
12. **Two lowerings** (Rust lair, lairf) must move together for P0 until the Rust lair is retired; the compare scripts are the guard.

## 7. Questions: closed (owner, 2026-10-04)

| # | Question | Answer | Applied in |
|---|---|---|---|
| 1 | Literal spelling; arbitrary lane count? | Suffix after the closer (`<<1.0 2.0>>f32`); any count 1 to 64 (512 bits). `<f32<..>>` omitted | decision 1; 2.3, 2.5, 2.12 |
| 2 | Broadcast of a variable scalar? | **No**: literals and `splat` only (revised); no deferred constraint | decision 2; 2.4, 2.13, 4.1 case 7, risk 2 |
| 3 | Writable views of a unique tensor? | Worth designing: a scoped exclusive borrow | `docs/design/exclusive-views.md`; 3.4, P10b |
| 4 | Integer vector `+` checked or wrapping? | Checked by default; `wrapping` opt-out; `unchecked-*` provided | decision 4; 2.8 |
| 5 | Rule 6 while `fibref` is frozen? | Not the judge for new features; in-case scalar references until the fibber interpreter (INT0) | decision 5; 4.1, risk 8 |
| 6 | More material in liar? | Yes, used: ADR 015, 017, 018, 024, the SIMD moth issues, `vector_types.feature` | decision 6; 1, 2.6, 4.1 |

Small items that remain (none blocks P0 to P2): the exact text of the error for a scalar variable operand (`scalar operand of a vector
operator: write (splat x)` is proposed); whether `(splat x)` without a type is accepted in an operator application where the other operand fixes
the type (proposed: yes; elsewhere `(simd/splat V x)`).
