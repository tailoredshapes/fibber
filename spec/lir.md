# lIR

**Status: Proposed** (M3). Every rule on this page is proposed until the
owner decides it (method.md, "Decision records"); the implementation in
`crates/lir` and `crates/lair` follows it so that it can be tested, and
nothing downstream depends on a rule until it is decided.

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
- Tokens: `(`, `)`, `{`, `}`, strings, vector types and atoms.
- A **string** is `"…"` with the escapes `\n \t \r \0 \\ \"` and
  `\xHH` (one byte, two hex digits). Any other escape is an error
  `invalid escape`; a string without its closing quote is `unterminated
  string`.
- A **vector type** is `<N x T>` written as one token (spaces inside
  allowed): `<4 x i32>`.
- An **atom** is any other run of characters that are not whitespace,
  parentheses, braces, `"` or `;`. `@name` names a global or function,
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
rtype  ::= type | void                      ; function results only
fntype ::= (fn cc? rtype (type* ...?))      ; a function type, for indirect calls
cc     ::= ccc | tailcc
```

- There is no `bool` (use `i1`), no `f32`/`f64` (use `float`/`double`),
  no typed pointer, no array type, no `void` value, and no `undef` or
  `poison` value anywhere in the language.
- Two types are equal only if they are written the same: a named struct
  equals only itself (not an anonymous struct with the same fields, as
  in LLVM), and `<4 x i32>` differs from `<2 x i64>`.
- **First-class** types (those a value, parameter, result, `load`,
  `store`, `phi` or `select` may have) are all of `type`. **Sized**:
  all of them (a struct may not contain itself by value, §4.1).
- **Integer** types: `i1 … i64` and vectors of them. **Float** types:
  `float`, `double` and vectors of them.
- A function type's `cc` defaults to `ccc`; `...` makes it variadic.

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
        | instruction           ; §6
```

- `(iK n)` accepts `-2^(K-1) ≤ n ≤ 2^K - 1`, stored as its low `K`
  bits: `(i8 255)` and `(i8 -1)` are the same value. Outside that range
  the literal is an error `integer literal n out of range for iK`
  (liar accepted `(i8 300)` as 44, `lir-audit/tc/i8_overflow_lit.lir`).
- `(float x)` and `(double x)` round to nearest; a finite literal that
  rounds to an infinity is an error `float literal out of range for
  double` (liar read `1e999` as infinity).
- A struct value is built by `insertvalue`s when a field is not a
  constant; its fields are evaluated left to right.
- `(string "…")` may appear in any function and as a global
  initialiser (§9); each occurrence is its own constant.

## 4. Modules

```
module ::= item*
item   ::= (defstruct NAME (type*))
         | (global NAME type init)                 ; mutable
         | (constant NAME type init)               ; read-only
         | (declare cc? NAME rtype (type* ...?))   ; external function
         | (define cc? (NAME rtype) ((type PNAME)*) block+)
block  ::= (block LABEL instr+)
```

Anything else at the top level is an error `expected a top-level form
(define, declare, defstruct, global, constant), found …` (liar silently
ignored top-level expressions, `lir-audit/fuzz2.txt`). Items may appear
in any order and refer to each other in any order.

### 4.1 Module-level rules

- Functions and globals share one namespace; structs have their own.
  A name defined twice is `duplicate definition of @f` or `duplicate
  struct %struct.s` (liar kept the first of two `define`s of one name,
  `tc/dup_fn.lir`).
- Names beginning with `llvm.` are reserved: `name @llvm.x is reserved`.
- Every `%struct.S` used anywhere must be defined: `undefined struct
  %struct.S`. A struct that contains itself by value, directly or
  through other structs, is `struct %struct.S contains itself by value`.
- Every `@f` must be defined in the module: `undefined function @f`,
  `undefined global @g`. Calling a global variable is `@g is not a
  function`.
- A global's initialiser must have the global's type (§9).

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
  name x in @f`). There is no shadowing.
- A name may not be a type keyword, `null`, `void`, an ordering, or one
  of `singlethread`, `weak`, `inbounds` (`reserved word x used as a
  name`).
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
  a parameter.
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
- Every incoming value has type `T` (`tc/phi_wrong.lir`).
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

`fptosi`/`fptoui` of a value out of range is poison as in LLVM; fibber
emits its saturating sequence instead (types.md §8.12).

### 6.4 Vectors and aggregates

| Form | Rule | Result |
|---|---|---|
| `(extractelement v i)` | `v : <N x E>`, `i` integer; a constant `i` must be `< N` | `E` |
| `(insertelement v e i)` | `v : <N x E>`, `e : E`, `i` as above | `<N x E>` |
| `(shufflevector a b m)` | `a`, `b : <N x E>`; `m` a literal `<M x i32>` with elements `< 2N` | `<M x E>` |
| `(extractvalue s k₁ ..)` | `s` a struct; each `k` a constant field index in range | the field's type |
| `(insertvalue s v k₁ ..)` | as above, `v` of the field's type | `s`'s type |

### 6.5 Memory

| Form | Rule | Result |
|---|---|---|
| `(alloca T)`, `(alloca T n)` | `T` sized; `n` integer | `ptr` |
| `(load T p)` | `p : ptr`; `T` first-class | `T` |
| `(store v p)` | `p : ptr`; `v` first-class | void |
| `(getelementptr inbounds? T p i₀ i₁ ..)` | `p : ptr`, `T` sized; `i₀` integer; each further index steps into the current type, which must be a struct: a constant `i32` field index in range | `ptr` |

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
  index into i64` (LLVM is phasing out indexing into vectors; lIR has
  no array type, §2).
- Plain loads and stores use the ABI alignment of their type.

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
an indirect call, `of indirect-call`).

### 7.2 main

A module run by `lair run` or built by `lair build` into an executable
must define `main` as `(define (main i32) () ..)` or `(define (main
i32) ((i32 argc) (ptr argv)) ..)`, with the C convention: `main must be
(main i32) with no parameters or (i32 ptr)`, or `no main function`.
The process exit status is `main`'s result, as C gives it (the low 8
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
3. neither is variadic: `tailcall to a variadic function`;
4. under `ccc`, the parameter types are identical: `tailcall under ccc
   needs identical parameter types (@f has (i64), @g has (i64 i64));
   use tailcc`. Under `tailcc` they may differ.

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
(global NAME T init)     ; mutable
(constant NAME T init)   ; read-only; a store to it is undefined behaviour
init ::= (iK INT) | (float FLT) | (double FLT) | (ptr null) | @NAME
       | (string STR) | (<N x E> ..) | { init* } | (%struct.S init*)
```

- `@NAME` in an expression is the global's address, a `ptr`; its value
  is read with `(load T @NAME)`.
- The initialiser has type `T` (`initializer of @g has type i64,
  expected i32`). `(string "…")` initialises a `ptr` global with the
  address of a private constant holding the bytes and a NUL: `(load ptr
  @g)` gives a pointer to the text. (liar stored the bytes themselves
  in the slot, so the load returned the text read as an address and
  crashed: `t/gstr.lir`.) `@f` initialises a `ptr` with a function's or
  global's address. Struct and vector initialisers nest.
- `constant` was documented by liar's `doc/lIR.md` but not parsed
  (`t/const.lir`).

With struct initialisers and function addresses, the type table,
vtables and literal objects of types.md §8.2 can be static data; the v1
mapping, which builds them in the module initialiser, stays valid.

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
| module | §4.1: duplicates, reserved names, undefined and recursive structs, globals and their initialisers, calls against the callee's `define` or `declare` |
| structure | §5.1: blocks, labels, terminators, branches to the entry block |
| names | §5.3: single binding, reserved words, dominance, void values |
| phi | §5.4: position, predecessors, incoming types and availability |
| types | §6: every operand and result type |
| constants | literal ranges (§3), constant divisors, shifts and vector indices (§6), `store` to a `constant` |
| calls | §7: arity, argument types, variadic promotion, tail-call rules, `main` |

After the checker, `lair` lowers the module and runs the LLVM verifier
(`LLVMVerifyModule`). A verifier failure after a successful check is an
internal error (see the top of this page).

## 11. The library API

`crates/lir` has no LLVM dependency: reader, AST, checker.

```rust
lir::parse(src: &str) -> Result<Module, Diagnostic>
lir::check(&Module) -> Result<(), Vec<Diagnostic>>
lir::parse_and_check(src: &str) -> Result<Module, Vec<Diagnostic>>
```

`crates/lair` lowers checked modules to LLVM and runs them.

```rust
// JIT: compile modules in-process and call their functions.
let mut jit = lair::Jit::new(lair::JitOptions::default())?;
jit.add_source("macros", src)?;           // parse, check, lower, verify, add
let f: extern "C" fn(i64) -> i64 = unsafe { jit.function("f")? };
let t: lair::FnType = jit.signature("f")?; // the lIR type, to check before transmuting
```

- `Jit::add_source` and `Jit::add_module` run the whole pipeline of
  this page; an error is returned, nothing is added, and the `Jit` stays
  usable.
- Several modules may be added to one `Jit`. A later module reaches an
  earlier one's functions and globals by `declare` (functions); a
  `declare` whose type differs from the earlier `define` is an error
  `declaration of @f does not match its definition in module m`, and a
  second definition of a name is `duplicate definition of @f (first in
  module m)`. Declared functions defined in no module resolve to the
  host process's symbols (the C library), and an unresolved one is an
  error at lookup.
- `Jit::function::<F>(name)` returns the address of a defined function
  as `F` (a function pointer type); it is `unsafe` because Rust cannot
  check `F` against the lIR type: callers compare `signature(name)`
  first. `lair::call_i64` and friends wrap the common cases safely.
- Function pointers stay valid until the `Jit` is dropped.
- `JitOptions { opt_level }`: 0 (default) runs no IR optimisation; 1 to
  3 run LLVM's `default<On>` pipeline before code generation.

```rust
// AOT: to an object file, assembly, LLVM IR or an executable.
lair::aot::emit(&module, lair::aot::Output::Object, &opts) -> Result<Vec<u8>, Error>
lair::aot::build_executable(&module, path, &opts) -> Result<(), Error>   // links with `cc`
```

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
paths also agree with each other. A `reject` case must be rejected on
both paths with an error containing the text, before LLVM sees it; an
error containing `internal error` never satisfies a case. `cargo test
-p lair` runs the whole suite.

## 14. Open questions for the owner

1. The variadic-promotion rule (§7.1) is stricter than LLVM. It makes
   fibber's compiler widen `bool`, `i8`, `i16` and `f32` arguments of
   a `:varargs` extern (syntax.md §3.15), as C does. Keep it?
2. Should a variadic extern call ever be a `tailcall`? §7.3 says no;
   syntax.md §3.15 already says an extern call is never a tail call.
3. `switch`, struct-typed `alloca`/`load`/`store`/globals and constant
   struct initialisers with function addresses are in (§6.7, §9);
   types.md §8.11 lists them as conveniences. Adopting them in §8 is
   the owner's call; the v1 shapes of §8 remain valid lIR.
4. Unreachable blocks are accepted (§5.3) with a stricter dominance rule
   than LLVM's (which lets them use anything). Reject them instead?
5. Should `lair` offer lIR-level names for the overflow intrinsics
   (`llvm.sadd.with.overflow`, `llvm.fptosi.sat`) that types.md §8.12
   says fibber may use? Not added: `llvm.` names are reserved (§4.1).
