# lIR

**Status: Decided** (owner, 2026-09-28; M3). The owner decided the seven
open questions of the first M3 pass on 2026-09-28 (§14, items 1 to 7)
and, the same day, the four additions of the second M3 pass — array
types (§2.1), linkage and visibility (§4.3), external global
declarations (§4.4), volatile and aligned accesses (§6.5) — as they
stood (§14, items 8 to 11). Every rule on this page is decided, and the
implementation in `crates/lir` and `crates/lair` follows it.

lIR is an S-expression assembler for LLVM IR: a 1:1 mapping, no sugar,
no promotion, no fibber vocabulary. This chapter is the language as
fibber needs it: what types.md §8 emits, the hardening §8.11 asks for,
and what the audit of liar's lIR (`lir-audit/`) found missing. It
replaces liar's `doc/lIR.md` for fibber.

The central rule is method.md rule 7: **lIR verifies its input.** A
module is parsed, then checked as a whole by lIR's own checker
(§10), then lowered to LLVM IR, then run through the LLVM verifier. All
four steps run on every path — the checker, `lair build`, `lair run`
and the library API (§11) — and none can be turned off. An invalid
module is an error with a message and a source position; it never
reaches LLVM's code generator, never crashes the backend and never
compiles to silently wrong code. A module that passes lIR's checker and
then fails the LLVM verifier is a bug in the checker: `lair` reports it
as `internal error: LLVM verifier rejected a checked module`, and a
case that sees that text fails.

What lIR does not check is what LLVM IR leaves to run time: a `load`
through a dangling pointer, a data race, an integer division by a zero
that is not a constant, an out-of-bounds `getelementptr inbounds`.
These are undefined behaviour exactly as in LLVM (§6.12), and the code
that emits lIR (fibber's compiler, types.md §8) is responsible for
them. lIR rejects the cases of them that are visible in the module
itself (constant division by zero, a constant shift by the width or
more, a constant out-of-range vector index, a store to a `constant`).

## 1. Lexical syntax

- A module is UTF-8 text. `;` starts a comment to the end of the line.
  Commas are whitespace (so `{ ptr, ptr }` and `{ ptr ptr }` are the
  same).
- Tokens: `(`, `)`, `{`, `}`, `[`, `]`, strings, vector types and atoms.
- A **string** is `"…"` with the escapes `\n \t \r \0 \\ \"` and
  `\xHH` (one byte, two hex digits). Any other escape is an error
  `invalid escape`; a string without its closing quote is `unterminated
  string`.
- A **vector type** is `<N x T>` written as one token (spaces inside
  allowed): `<4 x i32>`.
- An **atom** is any other run of characters that are not whitespace,
  parentheses, braces, brackets, `"` or `;`. `@name` names a global or function,
  `%struct.name` a struct type; any other atom is a keyword, a number or
  a local name.
- Nesting deeper than **512** lists is an error `nesting deeper than
  512` (liar's parser overflowed its stack at a nesting of about 200000,
  `lir-audit/deepadd.txt`).

Integer tokens are decimal, `0x` hex or `0b` binary, with an optional
leading `-`. Float tokens are decimal with an optional fraction and
exponent, or `inf`, `-inf`, `nan`.

## 2. Types

```
type   ::= i1 | i8 | i16 | i32 | i64 | float | double | ptr
         | <N x elem>                       ; elem: an integer type, float, double or ptr; 1 ≤ N ≤ 1024
         | %struct.NAME                     ; a named struct, defined by defstruct
         | { type* }                        ; an anonymous (literal) struct
         | [N x type]                       ; an array of N elements, N ≥ 0 (§2.1)
rtype  ::= type | void                      ; function results only
fntype ::= (fn cc? rtype (type* ...?))      ; a function type, for indirect calls
cc     ::= ccc | tailcc
```

- There is no `bool` (use `i1`), no `f32`/`f64` (use `float`/`double`),
  no typed pointer, no `void` value, and no `undef` or `poison` value
  anywhere in the language.
- Two types are equal only if they are written the same: a named struct
  equals only itself (not an anonymous struct with the same fields, as
  in LLVM), and `<4 x i32>` differs from `<2 x i64>`.
- **First-class** types (those a value, parameter, result, `load`,
  `store`, `phi` or `select` may have) are all of `type`. **Sized**:
  all of them (a struct may not contain itself by value, §4.1).
  **Aggregates**: structs and arrays.
- **Integer** types: `i1 … i64` and vectors of them. **Float** types:
  `float`, `double` and vectors of them.
- A function type's `cc` defaults to `ccc`; `...` makes it variadic.

### 2.1 Arrays

**Decided** (owner, 2026-09-28; §14 item 8). `[N x T]` is LLVM's array type: `N`
elements of the sized type `T`, laid out contiguously, `N` a
non-negative decimal integer (`[0 x i8]` is the flexible trailing
member of a struct, as in C; `array length must be a non-negative
integer`, `array length 5000000000 is too large` above `2^32 - 1`).
`T` may be any type, arrays and structs included; a vector's element
may not be an array (§2). An array is an aggregate: it is a value
(`load`, `store`, `phi`, `select`, a parameter or a result), it is
built by `([N x T] v..)` (§3) or `insertvalue`, read by `extractvalue`
(§6.4), and addressed by `getelementptr` (§6.5). It has no `bits`
(`bitcast cannot convert [4 x i32]`), is not atomic, and is not a
`switch` or `icmp` operand. types.md §8.3 lays a string's bytes and an
array object's elements out as `[0 x T]` at the end of the header
struct, and §8.2 holds the type table as `[N x %struct.fib.typerec]`.

## 3. Values and literals

```
value ::= NAME                  ; a parameter or let-bound name (§5.3)
        | @NAME                 ; the address of a global or function: ptr
        | (iK INT)              ; K ∈ {1, 8, 16, 32, 64}
        | (float FLT) | (double FLT)
        | (ptr null)
        | (<N x T> v₁ .. v_N)   ; N literal elements: numbers, or null for ptr elements
        | (string STR)          ; ptr to a private constant NUL-terminated byte array
        | { value* }            ; anonymous struct value
        | (%struct.S value*)    ; named struct value, one value per field
        | ([N x T] value*)      ; array value, exactly N elements of type T (§2.1)
        | (zeroinitializer T)   ; every bit zero: 0, 0.0, null, and so on through aggregates
        | instruction           ; §6
```

- `(iK n)` accepts `-2^(K-1) ≤ n ≤ 2^K - 1`, stored as its low `K`
  bits: `(i8 255)` and `(i8 -1)` are the same value. Outside that range
  the literal is an error `integer literal n out of range for iK`
  (liar accepted `(i8 300)` as 44, `lir-audit/tc/i8_overflow_lit.lir`).
- `(float x)` and `(double x)` round to nearest; a finite literal that
  rounds to an infinity is an error `float literal out of range for
  double` (liar read `1e999` as infinity).
- A struct or array value is built by `insertvalue`s when a field is
  not a constant; its fields are evaluated left to right. An array
  literal has exactly `N` elements, each of type `T` (`[3 x i32]
  literal: 3 elements expected, found 2`; `[3 x i32] literal: element
  2 has type i64, expected i32`). `(zeroinitializer T)` is a constant
  of any sized `T`; it emits no instruction.
- `(string "…")` may appear in any function and as a global
  initialiser (§9); each occurrence is its own constant.

## 4. Modules

```
module ::= item*
item   ::= (defstruct NAME (type*))
         | (global mod* NAME type init)            ; mutable
         | (constant mod* NAME type init)          ; read-only
         | (declare-global hidden? NAME type)      ; a variable defined elsewhere (§4.4)
         | (declare hidden? cc? NAME rtype (type* ...?))   ; a function defined elsewhere
         | (define mod* cc? (NAME rtype) ((type PNAME)*) block+)
mod    ::= private | internal | external           ; linkage (§4.3), at most one
         | hidden                                  ; visibility (§4.3)
block  ::= (block LABEL instr+)
```

Anything else at the top level is an error `expected a top-level form
(define, declare, declare-global, defstruct, global, constant), found …`
(liar silently ignored top-level expressions, `lir-audit/fuzz2.txt`).
Items may appear in any order and refer to each other in any order.

### 4.1 Module-level rules

- Functions and globals share one namespace; structs have their own.
  A name defined twice is `duplicate definition of @f` or `duplicate
  struct %struct.s` (liar kept the first of two `define`s of one name,
  `tc/dup_fn.lir`).
- Names beginning with `llvm.` are reserved: `name @llvm.x is reserved`.
- Every `%struct.S` used anywhere must be defined: `undefined struct
  %struct.S`. A struct that contains itself by value, directly or
  through other structs, is `struct %struct.S contains itself by value`.
- Every `@f` must be defined or declared in the module: `undefined
  function @f`, `undefined global @g`. Calling a global variable is
  `@g is not a function`.
- A global's initialiser must have the global's type (§9).
- A name is defined or declared once: a `declare` or `declare-global`
  of a name the module also defines is `duplicate definition of @f`.

### 4.2 Calling conventions

`ccc` (the default) is the C calling convention. `tailcc` is LLVM's
`tailcc`: it guarantees a `musttail` call between functions whose
prototypes differ (§7.3), which the C convention does not; types.md
§8.11 asks for it for mutual recursion and calls through closure
values. A function's convention is part of its type. A direct call
takes the convention of the callee's `define` or `declare`; an indirect
call takes it from the function type written at the call. There is no
way to call a function with a convention other than its own, except by
an indirect call with a wrong function type (undefined behaviour, as in
LLVM; §6.12).

### 4.3 Linkage and visibility

**Decided** (owner, 2026-09-28; §14 item 9). A `define`, `global` or `constant`
takes at most one linkage word and at most one visibility word, in
any order before the calling convention (`duplicate modifier private`,
`private and internal exclude each other`):

| Word | LLVM | Meaning |
|---|---|---|
| `external` (the default) | `external` | the symbol is exported: another module, the linker and the JIT's lookup see it |
| `internal` | `internal` | a local symbol of the object file (C's `static`): invisible to other modules, present for the debugger |
| `private` | `private` | no symbol at all: the name exists only inside the module |
| `hidden` | `hidden` visibility | exported to other modules of the same executable or shared object, but not from it (the ELF/Mach-O sense); a call to it needs no PLT |

A `declare` and a `declare-global` take `hidden` only: they name a
symbol defined elsewhere, whose linkage is that definition's (`declare
cannot be private`). `main` is exported: `main must not be private or
internal` (§7.2). A `private` or `internal` function may still be
`tailcall`ed, called through its address and used as an initialiser
in its own module; what it cannot be is seen from outside (§11).

### 4.4 External globals

**Decided** (owner, 2026-09-28; §14 item 10). `(declare-global NAME T)` names a
variable of type `T` that another module or the C library defines: a
`declare` for data. `@NAME` is its address; `(load T @NAME)` reads it
and `store` writes it, under the typed direct-access rule of §6.5. It
has no initialiser and is never `constant` in lIR (`store to constant
@g` never fires on it; a store to a read-only definition elsewhere is
undefined behaviour, as in C). `(declare-global stderr ptr)` reaches
glibc's `stderr`; `errno` is a macro over a function there, not a
symbol, so `(declare __errno_location ptr ())` is the way to it.

## 5. Functions, blocks and names

### 5.1 Blocks and terminators

A function has at least one block (`function @f has no blocks`; liar
emitted an empty body, `tc/empty_fn.lir`). The first block is the
entry block. Labels are unique within a function (`duplicate block
label L`, `tc/dup_block.lir`) and every label used exists (`undefined
block L`).

The **terminators** are `ret`, `br`, `switch`, `unreachable`,
`tailcall` and `indirect-tailcall`. A block's instructions are its
top-level forms; a `let` at the top level contributes the forms of its
body, recursively. The last of them must be a terminator (`block L does
not end in a terminator`, `tc/no_terminator.lir`), no other may be one
(`terminator in the middle of block L`, `tc/code_after_ret.lir`), and a
terminator is never an operand of another instruction or a `let`
binding (`terminator used as a value`).

No block may branch to the entry block: `branch to the entry block L`
(LLVM forbids predecessors of the entry block).

### 5.2 Evaluation order

Operands are evaluated left to right, depth first, each instruction
after its operands; the instructions of a block run in order. `let`
evaluates its bindings in order, then its body; its value is the value
of the last form of the body. A literal or a name emits no instruction.

### 5.3 Names: SSA with dominance

```
(let ((NAME value)*) instr+)
```

Parameters and `let`-bound names are the function's **SSA names**:

- Each name is bound once per function, parameters included (`duplicate
  name x in @f`). There is no shadowing (**Decided**, §14 item 6).
- A name may not be a type keyword, `null`, `void`, an ordering, or one
  of `singlethread`, `weak`, `inbounds`, `volatile`, `align`,
  `zeroinitializer`, `private`, `internal`, `external`, `hidden`
  (`reserved word x used as a name`).
- A name's scope is not the `let` body. A **use** of `x` is valid if
  the binding of `x` is evaluated before the use in the same block
  (else `x is used before its binding in block L`), or if `x` is bound
  in a block that dominates the using block (`x does not dominate its
  use in block L`, `tc/use_nondominating.lir`, `tc/nondom2.lir`; liar
  compiled both to code that read an uninitialised register).
  Parameters dominate every block.
- A name is an atom that is not a number and does not begin with `@`
  or `%` (`invalid name @x`); a binding is `(NAME value)` (`let binding
  must be (NAME value)`).
- In a block unreachable from the entry, a use is valid only if its
  name is bound earlier in the same block, in a reachable block, or is
  a parameter. Unreachable blocks are accepted with this rule, which is
  stricter than LLVM's (**Decided**, §14 item 4).
- A name bound to a `void` value is `void value bound to x`; any other
  use of a void value as an operand is `void value used as an operand`
  (`tc/main_void.lir`).
- An unknown name is `undefined name x` (`tc/undef_var.lir`).

### 5.4 phi

```
(phi T (LABEL value)+)
```

- A `phi` must be evaluated before any other instruction of its block
  (§5.2): as the value of one of the first bindings of the block's
  first `let`, or as an operand of the block's first instruction
  preceded only by literals, names and other `phi`s. Anywhere else it is
  `phi after the first instruction of block L` (`tc/phi_not_first.lir`,
  which liar compiled to wrong code).
- The labels are exactly the block's predecessors, each once: `phi in
  L names M, which is not a predecessor`, `phi in L has no entry for
  predecessor M`, `phi in L has two entries for M`
  (`tc/phi_badlabel.lir`). A block that is the target of several edges
  of one terminator (a `br` with both labels equal) has one
  predecessor.
- Every incoming value has type `T` (`tc/phi_wrong.lir`) and is a
  name, `@name` or a literal: `phi incoming value must be a name, a
  global or a constant` (an instruction there would have to run in the
  predecessor, after its terminator's operands). This is stricter than
  LLVM, which takes any constant expression (**Decided**, §14 item 7).
- When one terminator names the block twice (`(br c j j)`, or a `switch`
  with two cases to one label), LLVM wants one phi entry per edge; lIR
  takes one per predecessor and `lair` repeats it per edge.
- An incoming value from `M` is used at the end of `M`: a name in it
  must be bound in `M`, in a block that dominates `M`, or be a
  parameter (`x does not dominate the end of block M`). A `phi` may therefore name a value bound later in its own
  block when that block is its own predecessor (a loop).

## 6. Instructions

Each row gives the form, the operand rule and the result type. `T`
ranges over the types a row allows; "same T" means every operand has
exactly the one type `T`. A violated rule is an error naming the
instruction and the offending operand. The message forms are:

| Error | Form |
|---|---|
| wrong number of operands | `add expects 2 operands, found 1`; `ret expects at most 1 operand, found 2`; `br expects 1 or 3 operands, found 2` |
| wrong class of type | `add needs integer operands, found ptr` (`tc/ptr_as_int.lir`); `load needs a ptr operand, found i64`; `fptosi needs a float operand, found i32`; `sitofp needs a float result type, found i64` |
| operands that must agree | `add: operand 2 has type i64, expected i8` (`tc/mismatch.lir`), counting operands from 1 |
| conversions | `trunc: i64 is not narrower than i32`; `zext: i8 is not wider than i32`; `bitcast: i64 and i32 differ in size`; `bitcast cannot convert ptr` |
| indices | `vector index 4 out of range for <4 x i32>`; `extractvalue: field index 2 out of range for { i64, i64 }`; `getelementptr: struct field index must be a constant i32` |
| literals | `vector literal <4 x i32> needs 4 elements, found 3`; `vector literal element must be a number`; `vector length must be 1 to 1024`; `%struct.pair literal: 3 fields expected, found 1`; `%struct.pair literal: field 2 has type i64, expected i32` |
| names of things | `unknown instruction frobnicate`; `unknown type i99`; `unknown icmp predicate foo`; `unknown ordering bogus`; `void is only a function result type` |
| ret | `ret type i64 does not match @main's result i32` (`tc/ret_wrong.lir`); `ret without a value in @main, which returns i32` (`tc/ret_void_in_i32.lir`) |

### 6.1 Arithmetic and bitwise

| Form | Operands | Result |
|---|---|---|
| `(add a b)` `sub` `mul` `sdiv` `udiv` `srem` `urem` | same integer `T` | `T` |
| `(fadd a b)` `fsub` `fmul` `fdiv` `frem` | same float `T` | `T` |
| `(fneg a)` | float `T` | `T` |
| `(and a b)` `or` `xor` `shl` `lshr` `ashr` | same integer `T` | `T` |
| `(ctpop a)` | integer `T` | `T` (`llvm.ctpop`) |
| `(sadd-overflow a b)` `ssub-overflow` `smul-overflow` | same integer `T` | `{ T, i1 }` (or `{ <N x iK>, <N x i1> }`): the wrapped result and whether it overflowed (`llvm.sadd.with.overflow` and kin; **Decided**, §14 item 5) |

No `nsw`, `nuw` or `exact` flags exist. `sdiv`, `udiv`, `srem`, `urem`
with a constant zero divisor are `division by constant zero`; a shift
whose amount is a constant `≥` the bit width is `shift amount n is not
less than the width w`. Both are poison or undefined in LLVM.

### 6.2 Comparisons and select

| Form | Operands | Result |
|---|---|---|
| `(icmp P a b)`, `P ∈ eq ne slt sle sgt sge ult ule ugt uge` | same `T`: integer, `ptr`, or a vector of either | `i1`, or `<N x i1>` |
| `(fcmp P a b)`, `P ∈ oeq one olt ole ogt oge ord ueq une ult ule ugt uge uno` | same float `T` | `i1`, or `<N x i1>` |
| `(select c a b)` | `c : i1` (or `<N x i1>` with vector `a`, `b` of `N` elements); `a`, `b` same first-class `T` | `T` |

### 6.3 Conversions

| Form | Rule | Result |
|---|---|---|
| `(trunc T v)` | integer scalars, `T` narrower than `v` | `T` |
| `(zext T v)`, `(sext T v)` | integer scalars, `T` wider | `T` |
| `(fptrunc float v)` | `v : double` | `float` |
| `(fpext double v)` | `v : float` | `double` |
| `(fptosi T v)`, `(fptoui T v)` | `T` integer, `v` float scalar | `T` |
| `(sitofp T v)`, `(uitofp T v)` | `T` float, `v` integer scalar | `T` |
| `(ptrtoint T v)` | `T` integer, `v : ptr` | `T` |
| `(inttoptr ptr v)` | `v` integer | `ptr` |
| `(bitcast T v)` | `T` and `v`'s type non-aggregate, not `ptr`, same bit size | `T` |
| `(fptosi-sat T v)`, `(fptoui-sat T v)` | `T` integer scalar, `v` float scalar | `T`: saturating, NaN to 0 (`llvm.fptosi.sat`, `llvm.fptoui.sat`; **Decided**, §14 item 5) |

`fptosi`/`fptoui` of a value out of range is poison as in LLVM;
`fptosi-sat`/`fptoui-sat` saturate as types.md §8.12 specifies and
fibber emits them.

### 6.4 Vectors and aggregates

| Form | Rule | Result |
|---|---|---|
| `(extractelement v i)` | `v : <N x E>`, `i` integer; a constant `i` must be `< N` | `E` |
| `(insertelement v e i)` | `v : <N x E>`, `e : E`, `i` as above | `<N x E>` |
| `(shufflevector a b m)` | `a`, `b : <N x E>`; `m` a literal `<M x i32>` with elements `< 2N` | `<M x E>` |
| `(extractvalue s k₁ ..)` | `s` an aggregate; each `k` a constant index in range: a field of a struct, an element of an array (`extractvalue: index 4 out of range for [4 x i32]`) | the field's or element's type |
| `(insertvalue s v k₁ ..)` | as above, `v` of the field's or element's type | `s`'s type |

### 6.5 Memory

| Form | Rule | Result |
|---|---|---|
| `(alloca (align N)? T)`, `(alloca (align N)? T n)` | `T` sized; `n` integer | `ptr` |
| `(load volatile? (align N)? T p)` | `p : ptr`; `T` first-class | `T` |
| `(store volatile? (align N)? v p)` | `p : ptr`; `v` first-class | void |
| `(getelementptr inbounds? T p i₀ i₁ ..)` | `p : ptr`, `T` sized; `i₀` integer; each further index steps into the current type: into a struct by a constant `i32` field index in range, into an array by any integer, a constant one in range (`getelementptr: index 4 out of range for [4 x i32]`) except into a `[0 x T]`, whose length is unknown | `ptr` |

- A `store` whose pointer operand is directly `@c` of a `constant` is
  `store to constant @c`.
- **Direct accesses are typed.** When the pointer operand of `load`,
  `store`, `atomic-load`, `atomic-store`, `atomicrmw` or `cmpxchg` is
  written as `@g` (a global of type `G`) or as a name bound directly to
  `(alloca A)` without a count, the accessed type must be `G` or `A`:
  `store of double through p, which is (alloca i32)`, `load of i32
  through @g, a global of type i64`. LLVM accepts such an access and it
  reads or writes past the object (liar compiled
  `lir-audit/tc/store_type.lir` to a program that crashed). Accesses
  through any other pointer (a `getelementptr`, a loaded or passed
  pointer, a counted `alloca`) are not checked (§6.12).
- `getelementptr` into a scalar or a vector is `getelementptr cannot
  index into i64` (LLVM is phasing out indexing into vectors; an array
  is what indexes, §2.1).
- Plain loads and stores use the ABI alignment of their type unless
  `(align N)` says otherwise. **Decided** (owner, 2026-09-28; §14 item 11): `N` is
  a power of two from 1 to 2^30 (`align must be a power of two, found
  3`); a smaller `N` than the ABI's is a promise that LLVM honours with
  slower code where the target needs it, a larger one a promise the
  emitter must keep (an `alloca` with `(align N)` keeps it). `volatile`
  is LLVM's: the access is performed exactly as written, neither
  removed, duplicated nor reordered against other volatile accesses,
  for memory-mapped registers and for a value a signal handler or a
  debugger may change; it is not atomic and orders nothing else (§6.6
  for that). The typed direct-access rule applies unchanged.

### 6.6 Atomics

```
ord ::= unordered | monotonic | acquire | release | acq_rel | seq_cst
(atomic-load  singlethread? ord T p)          → T
(atomic-store singlethread? ord v p)          → void
(atomicrmw OP singlethread? ord p v)          → T, the old value
(cmpxchg weak? singlethread? ord ord? p expected new)   → { T, i1 }
(fence singlethread? ord)                     → void
OP ::= xchg add sub and nand or xor max min umax umin fadd fsub fmax fmin
```

- Without `singlethread` an atomic operation or fence has LLVM's
  default (system) synchronisation scope, which orders memory between
  threads. `singlethread` is LLVM's `syncscope("singlethread")`: it
  orders only against signal handlers on the same thread. (liar's
  `fence` was always `singlethread`: `lir-audit/t/fence_mono.lir`.)
- The atomic type `T` is an integer of 8, 16, 32 or 64 bits, `ptr`,
  `float` or `double` (`atomic operation on i1`; LLVM requires
  byte-sized types). `cmpxchg` takes integers and `ptr` (`cmpxchg needs
  an integer or ptr operand, found double`). `atomicrmw`: `xchg` any
  atomic type; `fadd fsub fmax fmin` a float (`atomicrmw fadd needs a
  float operand, found i64`); the others an integer (`atomicrmw add
  needs an integer operand, found double`).
- Orderings (`atomic-load cannot have ordering release`):
  `atomic-load` not `release` or `acq_rel`; `atomic-store` not
  `acquire` or `acq_rel`; `atomicrmw` and `cmpxchg` not `unordered`; `fence` only `acquire`, `release`, `acq_rel` or
  `seq_cst` (`fence ordering must be acquire, release, acq_rel or
  seq_cst, found monotonic`). `cmpxchg`'s failure ordering (the second)
  is not `release` or `acq_rel` (`cmpxchg failure ordering cannot be
  release`); when omitted it is the success
  ordering with its release part removed (`acq_rel` → `acquire`,
  `release` → `monotonic`).
- Every atomic access is aligned to the ABI alignment of `T`; a
  misaligned pointer is undefined behaviour.
- `cmpxchg` returns the old value and whether the exchange happened;
  `weak` may fail spuriously.

### 6.7 Control flow

| Form | Rule |
|---|---|
| `(ret v)`, `(ret)` | `v` has the function's result type; `(ret)` only in a `void` function (`tc/ret_wrong.lir`, `tc/ret_void_in_i32.lir`) |
| `(br L)` | |
| `(br c L₁ L₂)` | `c : i1` (`tc/brcond_i32.lir`; `tc/brcond_double.lir` crashed liar) |
| `(switch v L_default ((iK c) L)*)` | `v` integer scalar (`switch needs an integer scalar, found double`); each case a literal of `v`'s type (`switch: case has type i64, expected i32`), no two equal (`switch: duplicate case 1`) |
| `(unreachable)` | reaching it is undefined behaviour; it follows a call that does not return |
| `(trap)` | void, not a terminator: `llvm.trap`, which aborts the process with the target's trap signal (`ud2`, SIGILL, on x86-64; `brk`, SIGTRAP, on AArch64) without unwinding or flushing; the block goes on to `unreachable` (**Decided**, §14 item 5). types.md §8.12's `fib.trap` writes its message first and calls `abort`, which flushes nothing either but raises SIGABRT, the signal the audit and the cases expect; `(trap)` is for the paths that have no message |
| `(phi …)` | §5.4 |

### 6.8 Calls

| Form | Rule | Result |
|---|---|---|
| `(call @f a..)` | `@f` defined or declared (`call needs a function name @f, found f`); arguments as §7.1 | `f`'s result |
| `(indirect-call p (fn cc? R (T..)) a..)` | `p : ptr` (`indirect-call needs a ptr callee, found i64`); arguments against the written type (`indirect-call: argument 1 has type i32, expected i64`); a bare result type in place of the function type, liar's form, is `indirect-call needs a function type (fn R (T..)), found i64` | `R` |
| `(tailcall @f a..)` | terminator; §7.3 | — |
| `(indirect-tailcall p (fn cc? R (T..)) a..)` | terminator; §7.3 | — |

### 6.9 let

`(let ((x v)..) body..)` as §5.2 and §5.3. A `let` with no body is
`let without a body`.

### 6.10 What is not in lIR

liar's ADR 021 "safe lIR" is removed: there is no `own`, `ref`,
`refmut`, `rc` type, no `alloc own`, `borrow`, `drop`, `move`,
`rc-alloc`, `rc-clone`, `rc-drop`, `rc-count`, `rc-ptr`,
`heap-struct`, `free` instruction, no `array-*`, `ptr-array-*` or
`heap-array*` instruction. They compiled to nothing checked (`own`
became an ordinary stack slot and `drop` nothing, `lir-audit/borrow/`),
and every one is expressible with `declare`d C functions, `alloca`,
`load`, `store`, `getelementptr` and `icmp`/`br`. Ownership is fibber's
concern (spec/ownership.md), decided before lIR is emitted. Using one
of these names is `unknown instruction`.

### 6.11 Constants in instructions

Where a rule says "constant" it means a literal written in place
(§3), not a name bound to one.

### 6.12 Undefined behaviour that remains

As in LLVM: memory accesses through invalid, misaligned or freed
pointers; data races on non-atomic accesses; `sdiv`/`srem` by a zero or
of the minimum by -1 computed at run time; shifts by a run-time amount
`≥` the width (poison); out-of-range `fptosi`/`fptoui` (poison); a
run-time out-of-range vector index (poison); `getelementptr inbounds`
outside its object (poison); reaching `unreachable`; calling a function
through a pointer with the wrong function type; a `musttail` callee
that reads the caller's `alloca`s. lIR's checker cannot see these;
fibber's compiler emits the checks types.md §8.12 requires.

## 7. Calls, variadic functions and tail calls

### 7.1 Arguments

A call has exactly as many arguments as the callee has parameters
(`call to @f: expected 2 arguments, found 1`, `tc/call_argcount.lir`),
at least as many for a variadic callee (`call to @printf: expected at
least 1 argument, found 0`), each of the parameter's type
(`call to @f: argument 1 has type double, expected i32`,
`tc/call_argtype.lir`, `tc/extern_argtype.lir`).

The arguments past the fixed parameters of a variadic callee may be of
any first-class type except `i1`, `i8`, `i16` and `float`: C promotes
those before a variadic call, and passing them unpromoted is wrong at
the C ABI although LLVM accepts it: `variadic argument 2 of @printf has
type float, which C promotes; pass double` (for `iK`: `pass i32`; for
an indirect call, `of indirect-call`). This is stricter than LLVM
(**Decided**, §14 item 1): a callee reading an `i32` slot that was
written as an `i8` reads three bytes of whatever was there.

### 7.2 main

A module run by `lair run` or built by `lair build` into an executable
must define `main` as `(define (main i32) () ..)` or `(define (main
i32) ((i32 argc) (ptr argv)) ..)`, with the C convention: `main must be
(main i32) with no parameters or (i32 ptr)`, or `no main function`,
and exported: `main must not be private or internal` (§4.3). The
process exit status is `main`'s result, as C gives it (the low 8
bits on POSIX). Output buffered by the C library is flushed at exit on
both paths.

### 7.3 Tail calls

`(tailcall @g a..)` lowers to `musttail call` of `g` followed by `ret`
of its result (or `ret void`); `indirect-tailcall` likewise through a
pointer. A `musttail` call is **guaranteed** not to grow the stack:
LLVM either emits a jump or fails, and lIR's checker rejects every
tail call LLVM would fail on, so a checked module's tail calls are all
jumps. (liar emitted `tail`, a hint LLVM may ignore: `t/tail_many.lir`
overflows the stack.) The rules, checked against the callee's type
(the `define`/`declare`, or the written function type):

1. caller and callee have the same calling convention: `tailcall:
   @f is ccc and @g is tailcc` (for `indirect-tailcall`: `indirect-tailcall:
   @f is tailcc and the callee is ccc`);
2. they have the same result type: `tailcall: @g returns i32, @f
   returns i64` (`indirect-tailcall: the callee returns i32, @f returns
   i64`);
3. neither is variadic: `tailcall to a variadic function` (**Decided**,
   §14 item 2: no ambiguity about which function is tailed);
4. under `ccc`, the parameter types are identical: `tailcall under ccc
   needs identical parameter types (@f has (i64), @g has (i64 i64));
   use tailcc`. Under `tailcc` they may differ;
5. the result is returned in registers: `void`, or a type with at most
   two leaves (structs and arrays flattened), each a scalar or a vector
   of at most 512 bits: `tailcall: a result of type { i64, i64, i64,
   i64 } may be returned in memory, which no tail call can: at most 2
   scalar or vector leaves of at most 512 bits (found 4 leaves, the
   widest 64 bits)`. A result the target returns through a hidden
   pointer needs a temporary in the caller's frame, so LLVM aborts on
   the `musttail` ("failed to perform tail call elimination on a call
   site marked musttail"; x86-64 gives up beyond three integer or four
   floating leaves, `adversarial/tail-result-memory.lir`). The bound
   is what every 64-bit target returns in registers; `{ ptr, ptr }`,
   the dyn value of types.md §8, is within it.

A `tailcall` whose callee takes a pointer to one of the caller's
`alloca`s is undefined behaviour (the frame is gone); lIR does not
check it (§6.12).

## 8. Memory model

lIR follows the LLVM memory model exactly (LangRef, "Memory Model for
Concurrent Operations"): a non-atomic load that races with a store
reads `undef`; the orderings of §6.6 are LLVM's, which match C++11's
(`monotonic` is `relaxed`).

## 9. Globals and constants

```
(global mod* NAME T init)     ; mutable
(constant mod* NAME T init)   ; read-only; a store to it is undefined behaviour
(declare-global hidden? NAME T)   ; defined elsewhere (§4.4)
init ::= (iK INT) | (float FLT) | (double FLT) | (ptr null) | @NAME
       | (string STR) | (<N x E> ..) | { init* } | (%struct.S init*)
       | ([N x T] init*) | (zeroinitializer T)
```

- `@NAME` in an expression is the global's address, a `ptr`; its value
  is read with `(load T @NAME)`.
- The initialiser has type `T` (`initializer of @g has type i64,
  expected i32`). `(string "…")` initialises a `ptr` global with the
  address of a private constant holding the bytes and a NUL: `(load ptr
  @g)` gives a pointer to the text. (liar stored the bytes themselves
  in the slot, so the load returned the text read as an address and
  crashed: `t/gstr.lir`.) `@f` initialises a `ptr` with a function's or
  global's address. Struct, array and vector initialisers nest;
  `(zeroinitializer T)` zeroes a global of any type, a large buffer
  included.
- `constant` was documented by liar's `doc/lIR.md` but not parsed
  (`t/const.lir`).
- `mod*` is the linkage and visibility of §4.3.

With struct and array initialisers and function addresses, the type
table, vtables, literal objects and `def` constants of types.md §8.2
are static data (**Decided**, §14 item 3).

## 10. The checker

`lir::check` runs over the whole module after parsing and before any
LLVM call. It rejects every error named in §1 to §9; each diagnostic
is

```
FILE:LINE:COL: error: MESSAGE
```

with the position of the form at fault. The checker stops at the first
error of each function but checks every function. The main groups:

| Group | What it checks |
|---|---|
| module | §4.1: duplicates, reserved names, undefined and recursive structs, globals and their initialisers, calls against the callee's `define` or `declare`; §4.3: modifiers |
| structure | §5.1: blocks, labels, terminators, branches to the entry block |
| names | §5.3: single binding, reserved words, dominance, void values |
| phi | §5.4: position, predecessors, incoming types and availability |
| types | §6: every operand and result type |
| constants | literal ranges (§3), constant divisors, shifts, vector and array indices (§6), `store` to a `constant`, alignments (§6.5) |
| calls | §7: arity, argument types, variadic promotion, tail-call rules, `main` |

After the checker, `lair` lowers the module and runs the LLVM verifier
(`LLVMVerifyModule`). A verifier failure after a successful check is an
internal error (see the top of this page).

### 10.1 The fuzzer

Method.md rule 7 is attacked by a mutation fuzzer, `lair fuzz`
(`crates/lair/src/fuzz`): it reads every `accept` case under the
directories given, and for each mutant picks a case and applies one to
three random deformations to its forms — a type swapped for another, an
operand dropped or duplicated, an atom renamed to another of the module
(a label for a name, a name for a label), a number changed to a boundary
value, an instruction of the grammar with random operands inserted, a
subtree spliced in from another case, siblings swapped, a form's head
replaced by another keyword, a list replaced by one of its children, a
top-level form dropped. Each mutant runs in a process of its own,
`lair fuzz-one FILE` (`lair run` that marks on stderr when the checker
and then the backend accepted the module), under a time limit and an
address-space cap, with its standard output discarded. A mutant that is
rejected with a diagnostic, or that compiles and then runs, crashes or
loops — its own undefined behaviour (§6.12) — is what rule 7 promises. A
**finding** is anything else: a panic, an `internal error` (the LLVM
verifier rejecting what the checker accepted), a signal or a timeout
before the backend's mark. Each finding's mutant is kept under `-o DIR`
(default `target/lair-fuzz`) with the seed, the index, the source case
and the mutations in its header; `lair fuzz --seed N --count M` with the
same seed gives the same mutants whatever the scheduling, since mutant
`i` is a function of the seed and `i` alone. `cargo test -p lair` runs
300 mutants (`crates/lair/tests/fuzz.rs`); `LAIR_FUZZ_COUNT=20000` and
`LAIR_FUZZ_SEED=N` set a larger budget or another seed. Every finding
is minimised and kept as a case in `cases/lir/adversarial/`, with the
rule it led to.

The first campaign (2026-09-28, seeds 1 to 4, 80,000 mutants, 20,000 of
them through `-O2`) found nothing: 93% of mutants were rejected with a
diagnostic, the rest ran, and every crash or hang at run time was the
mutant's own undefined behaviour. A batch of 384 hand-written modules
aimed at the rules (types at their limits, every calling-convention
and linkage combination, atomics on every type, phi and block
corners) found one: a checked `tailcall` whose aggregate result LLVM
returns in memory made LLVM abort (§7.3 rule 5 came from it). The
same batch showed that the worker had to look `main` up before
marking the backend's acceptance, since ORC generates code lazily;
seed 4 ran after that fix.

## 11. The library API

`crates/lir` has no LLVM dependency: reader, AST, checker.

```rust
lir::parse(src: &str) -> Result<Module, Diagnostic>
lir::check(&Module) -> Result<(), Vec<Diagnostic>>
lir::check_main(&Module) -> Result<(), Diagnostic>          // §7.2
lir::parse_and_check(src: &str) -> Result<Module, Vec<Diagnostic>>
```

`crates/lair` lowers checked modules to LLVM and runs them.

```rust
// JIT: compile modules in-process and call their functions.
let mut jit = lair::Jit::new(lair::JitOptions::default())?;
jit.add_source("macros", src)?;            // parse, check, lower, verify, add
let t: lair::FnType = jit.signature("f")?;  // the lIR type, to check before transmuting
let f: extern "C" fn(i64) -> i64 = unsafe { jit.function("f")? };
let a: usize = jit.address("f")?;           // the raw address
let c: usize = jit.c_entry("g")?;           // a ccc entry to a tailcc function
```

- `Jit::add_source(name, src)` and `Jit::add_module(name, &module)` run
  the whole pipeline of this page; on an error nothing is added and the
  `Jit` stays usable.
- Several modules may be added to one `Jit`. A later module reaches an
  earlier one's functions by `declare`; a `declare` whose type differs
  from the earlier `define` is `declaration of @f does not match its
  definition in module m`, and a second definition of a name is
  `duplicate definition of @f (first in module m)`. A declared function
  defined by no module must be in the host process (the C library and
  what it loaded), else the module is rejected when added: `undefined
  symbol @f: defined by no module of this JIT and not in the process`.
- `signature`, `address`, `c_entry` and `function` accept only
  functions defined in the `Jit` with `external` linkage (not globals,
  not declarations; a `private` or `internal` function is `@f is
  private to module m`). A `private` or `internal` definition is not
  in the cross-module namespace: a later module may define the same
  name, and a `declare` of it finds nothing (`undefined symbol`).
- `Jit::function::<F>(name)` is `unsafe`: Rust cannot check `F` against
  the lIR type, so callers compare `signature(name)` first; `F` must be
  pointer-sized or it is an error. Rust speaks only the C convention,
  so for a `tailcc` function `function` returns, and `c_entry` names,
  a **trampoline**: a `ccc` function of the same parameters and result
  that the `Jit` generates (as an lIR module `name.tramp` through the
  same pipeline) and that calls the `tailcc` function; `address` stays
  the raw address of the function itself. This is the mechanism a
  compiler uses to run a macro (M4, M6): compile the macro-time
  module, take the entry as a function pointer, call it, and add
  further modules that `declare` what the earlier ones defined
  (`crates/lair/tests/jit.rs`, "a compiler runs a macro").
- Function pointers stay valid until the `Jit` is dropped.
- `JitOptions { opt_level }`: 0 (default) runs no IR optimisation; 1 to
  3 run LLVM's `default<On>` pipeline before code generation.

```rust
// AOT: object file, assembly, LLVM IR, or an executable.
lair::aot::emit(&module, name, lair::aot::Output::Object, &opts) -> Result<Vec<u8>>
lair::aot::build_executable(&module, name, path, &opts) -> Result<()>   // `cc` with -lm and opts.libs
```

The checker and the lowering recurse over expressions, at most 512
deep (§1); the checker is tested at depth 504 on a 2 MB thread.

Both paths share one lowering and the same checks; both set the host
target's triple and data layout on the module.

## 12. The `lair` tool

```
lair check FILE.lir              ; parse and check only
lair run FILE.lir [ARGS..]       ; JIT-compile and run main; exit with its status
lair build FILE.lir -o OUT [-O n] [--emit obj|asm|llvm] [-l LIB]..
lair emit-llvm FILE.lir          ; print the verified LLVM IR
lair cases DIR..                 ; run a case suite (§13)
```

## 13. Cases

Each case in `cases/lir/` is a `.lir` file whose header states its
verdict before the implementation decides it (method.md rule 3):

```
;; expect: accept | reject
;; exit: N                  ; accept: main's exit status (default 0)
;; signal: NAME[, NAME..]   ; accept: the process is killed by one of these signals instead
;; out: TEXT                ; accept: one line of standard output each; all lines, in order
;; error: TEXT              ; reject: text the error must contain
;; ir: TEXT                 ; the verified LLVM IR must contain TEXT
;; ir-not: TEXT             ; ... and must not
;; paths: jit | aot         ; restrict to one path (default: both)
;; audit: …                 ; the lir-audit file it comes from and what liar did
```

An `accept` case runs through the JIT (`lair run`, no IR optimisation)
and through AOT (`lair build -O2`, then the executable), each in its own
process; both must give the stated exit status and output, so the two
paths also agree with each other. A case whose program is meant to die
of a signal names it (`SIGILL`, `SIGTRAP`, `SIGABRT`, `SIGFPE`,
`SIGSEGV` or `SIGTERM`) rather than the shell's `128 + N` status, so the
header holds on every architecture; the harness reports an ending that
disagrees as `trapped by SIGILL (4) (expected exit 0)` or `exit 0
(expected SIGILL (4))`. Several names mean any of them: `(trap)` raises
SIGILL on x86-64 and SIGTRAP on AArch64 (§6.7), so `instr/trap.lir`
names both. `exit` and `signal` exclude each other. A `reject` case must be rejected on
both paths with an error containing the text, before LLVM sees it; an
error containing `internal error` never satisfies a case. `cargo test
-p lair` runs the whole suite.

## 14. Decision record

On 2026-09-28 the owner decided the seven open questions of the first
M3 pass (items 1 to 7) and, later the same day, the four additions of
the second M3 pass as they stood (items 8 to 11). Each rule is now
**Decided** in the section that states it; the numbering here is the
one the passes used.

| Item | Question | Decision | Now in |
|---|---|---|---|
| 1 | variadic arguments narrower than C's promotions (`i1`, `i8`, `i16`, `float`) | **rejected**; the emitter widens them explicitly. Receiving an `i32` into an `i8` slot is a way to corrupt the callee's frame, and LLVM would not say so | §7.1 |
| 2 | a variadic callee as a tail call | **rejected**: `tailcall to a variadic function`. There is no ambiguity about which function is tailed | §7.3 rule 3 |
| 3 | adopting `switch`, struct-typed `alloca`s and globals, and constant vtables with function addresses in types.md §8 | **adopted**: `match` dispatches through `switch`, stack objects are `(alloca %struct.T)`, and the type table, literal objects, named-function closures, vtables and `def` constants are static data; the v1 "module initialiser builds it" text is replaced wherever lIR can now hold the object | types.md §8.2, §8.3, §8.4, §8.5, §8.6, §8.10, §8.11; §10 |
| 4 | unreachable blocks | **accepted**, with the stricter dominance rule as specified: a use in an unreachable block names a parameter, a name bound earlier in the same block, or one bound in a reachable block | §5.3 |
| 5 | lIR-level names for LLVM's overflow and saturation intrinsics | **added**: `sadd-overflow`, `ssub-overflow`, `smul-overflow` (`llvm.s{add,sub,mul}.with.overflow`), `fptosi-sat`, `fptoui-sat` (`llvm.fptosi.sat`, `llvm.fptoui.sat`) and `trap` (`llvm.trap`), beside `ctpop`; types.md §8.12 emits them for the checked arithmetic. `llvm.` stays reserved as a *symbol* prefix (§4.1): the intrinsics are instructions, not names | §6.1, §6.3, §6.7; types.md §8.12 |
| 6 | names bound once per function | **kept**: no rebinding, no shadowing (`duplicate name x in @f`) | §5.3 |
| 7 | the typed direct-access rule and the phi-operand rule, both stricter than LLVM | **kept** | §6.5, §5.4 |
| 8 | array types `[N x T]`: in `defstruct` fields, `alloca`, globals and constants, `load`/`store`, `getelementptr`, `extractvalue`/`insertvalue`, the literal `([N x T] v..)`, and `(zeroinitializer T)` for any sized `T` | **adopted** as proposed by the second M3 pass; types.md §8.2 and §8.3 lay the type table and array objects out with them | §2.1, §3 |
| 9 | linkage and visibility on `define`, `global` and `constant`: `private`, `internal`, `external` (the default) and `hidden`, with the JIT keeping `private` and `internal` names out of the cross-module namespace | **adopted** as proposed, so that a compiler keeps a module's helpers out of the export table | §4.3, §11 |
| 10 | external global declarations `(declare-global NAME T)`, for `stderr` and other variables the C library or another module defines | **adopted** as proposed | §4.4 |
| 11 | `volatile` and `(align N)` on `load`, `store` and `alloca` | **adopted** as proposed: `N` a power of two from 1 to 2^30; `volatile` is LLVM's, not atomic | §6.5 |
