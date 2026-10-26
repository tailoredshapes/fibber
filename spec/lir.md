# lIR

> Source references: `crates/` and Rust `fibref`/`fibgen` paths in dated
> sections refer to git tag `seed-1`, not files in this checkout. See
> [the legacy source map](../docs/history/source-map.md) for retrieval and
> current compiler counterparts. These historical citations do not reinstate
> an interpreter oracle; [method.md](method.md) defines current validation.


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
  Commas are whitespace.
- Tokens: `(`, `)`, `{`, `}`, `[`, `]`, strings and atoms. No form of lIR is
  written with braces or brackets (§2: the types and values they wrote
  in the old spelling are spelled out); the reader reads them as lists
  with their bracket, for reader macros (§4.8).
- A **string** is `"…"` with the escapes `\n \t \r \0 \\ \"` and
  `\xHH` (one byte, two hex digits). Any other escape is an error
  `invalid escape`; a string without its closing quote is `unterminated
  string`.
- An **atom** is any other run of characters that are not whitespace,
  parentheses, braces, brackets, `"` or `;`: a word of lIR (upper case,
  §2), a number, or a name. `@`, `%` and `<` are characters of an atom
  like any other.
- Nesting deeper than **1024** forms is an error `nesting deeper than
  1024` (liar's parser overflowed its stack at a nesting of about 200000,
  `lir-audit/deepadd.txt`); a module's readtable may set the depth,
  and its reader macros' runs count as levels (§4.8).

Integer tokens are decimal, `0x` hex or `0b` binary, with an optional
leading `-`. Float tokens are decimal with an optional fraction and
exponent, or `INF`, `-INF`, `NAN`.

## 2. Types

**Spelling.** (owner, 2026-10-25; §14 item 21.) lIR spells names out: it has
no sigil or shorthand of its own. Every word lIR owns is written in upper
case: the heads of items and instructions, modifiers, calling conventions,
types, literal heads, flags, predicates, orderings and fields (`DEFINE`,
`ADD`, `INTERNAL`, `TAILCC`, `I64`, `NULL`, `ACQ_REL`, `RETAIN`). They are
LLVM's words, upper-cased; where lIR had abbreviated one it is LLVM's again
(`ALWAYSINLINE`, `PRESERVE_MOSTCC`), a function type is `FUNCTION`, and
`KERNELCC` stays lIR's (LLVM's word for it is NVPTX's, and WGSL has none).
Two rules follow.

1. **A reference or a type is a form, never a sigil.** `(GLOBAL NAME)` is
   the address of the global or function `NAME` (LLVM's `@NAME`); the item
   `(GLOBAL mod* NAME T init)` defines one, and the two are told apart by
   position (an item or an operand) and by their operand count. `(STRUCT
   NAME)` is the struct type a `DEFSTRUCT` defines (LLVM's `%NAME`, which
   lair names `%struct.NAME`) and `((STRUCT NAME) v..)` its value. `(PTR
   (ADDRSPACE 1))` is LLVM's `ptr addrspace(1)`. `(VECTOR N T)` and `(ARRAY
   N T)` are LLVM's `<N x T>` and `[N x T]`, and a literal is headed by its
   type as every literal is: `((VECTOR 4 I32) 1 2 3 4)`, `((ARRAY 2 I8) (I8
   1) (I8 2))`. The anonymous struct, LLVM's literal struct `{ T, .. }`, is
   `(LITERAL-STRUCT T..)` as a type and `(LITERAL-STRUCT v..)` as a value:
   LLVM calls it a literal structure, and it cannot be taken for `(STRUCT
   NAME)`, which has exactly one operand, a name, and refers to a
   definition, where `LITERAL-STRUCT` has any number of types (in a type
   position) or values (in a value position) and names nothing. `...`
   (variadic) is the one piece of punctuation left.
2. **The lower case is free.** lIR owns no lower-case word, so a name may
   be any atom that is not a number and is not one of lIR's words (§5.3):
   `add`, `let`, `null` and `block` are names, `ADD` is not (`reserved
   word ADD used as a name`, `invalid label RET`). Case is not folded:
   `Add` is a name. Labels, locals, parameters and the names of globals,
   functions and structs are kept exactly as written. A language built on
   lIR, as a library of macros, has the lower case to itself: a macro named
   in lower case cannot capture a form of lIR, so no macro name is
   reserved (§4.6).

The words are one table, `compiler/lir/words.fib`: each word's text and
its canonical name, which is its lower case except for the three respelled
(`FUNCTION` is `fn`, `ALWAYSINLINE` is `inline`, `PRESERVE_MOSTCC` is
`preservemost`). The parser, the printer, the name rule and the macro
reader all read it, and `lair words` prints it. `compiler/tests/lir/words.sh`
holds it equal to the upper-case atoms of this chapter's grammar, the code
blocks that hold `::=`, comments left out, less the placeholders, which are
upper case and are not words. The placeholders: `NAME PNAME LABEL STR
INT FLT N T LO HI`. In prose, and in lIR's messages, a word is named by
its canonical name (`load`, `inline`).

**The old spelling (transitional, for one release).** Until the tree's
lIR is converted and a seed reads the new spelling, the parser can also
read the old one and gives it the same tree: lower-case words, `@f`,
`%struct.S`, `(ptr 1)`, `(fn ..)`, `inline`, `preservemost`, `inf` and
`nan`, the vector type `<N x T>` as one token, `[N x T]` and `{ T.. }`;
with it the old rules come back (the reserved words of the old §5.3, `@`
and `%` refused at the start of a name, and the macro names of the old
§4.6). `lair` reads the new spelling only unless it is given
`--old-spelling` or `FIB_LIR_SPELLING=old` (`--new-spelling` and `new` the
other way), and `lair cases` and `lair fuzz` pass the choice on to the
processes they start (`lair fuzz` and `fuzz-one` admit the old spelling
unless told otherwise: their generator writes it). `fibc` admits the old
spelling, since its own lIR texts (the runtime, the templates) and the
inline `lir` bodies of `lib/` stay old until the seed reads the new; the
build exports
`FIB_LIR_SPELLING=old` for its `lair` runs (`mk/config.mk`). The printer
writes the new spelling only, and messages keep the old (`@f`,
`%struct.s`) until they move with the case headers. The old spelling is
dropped in the release after the seed that reads the new.

```
type   ::= I1 | I8 | I16 | I32 | I64 | FLOAT | DOUBLE | PTR
         | (PTR (ADDRSPACE 1))             ; a managed reference: LLVM's ptr addrspace(1), non-integral (§6.14)
         | (VECTOR N elem)                 ; elem: an integer type, float, double or ptr; 1 ≤ N ≤ 1024
         | (STRUCT NAME)                   ; a named struct, defined by defstruct
         | (LITERAL-STRUCT type*)          ; an anonymous (literal) struct
         | (ARRAY N type)                  ; an array of N elements, N ≥ 0 (§2.1)
rtype  ::= type | VOID                     ; function results only
ptype  ::= type | (OWNED (PTR (ADDRSPACE 1)))     ; a parameter's type, with its ownership kind (§6.14.2)
fntype ::= (FUNCTION cc? rtype (ptype* ...?))     ; a function type, for indirect calls
cc     ::= CCC | TAILCC | KERNELCC | PRESERVE_MOSTCC   ; kernelcc: a GPU kernel's entry (§6.9a); preservemost: §6.6b
```

- There is no `bool` (use `i1`), no `f32`/`f64` (use `float`/`double`),
  no typed pointer, no `void` value, and no `undef` or `poison` value
  anywhere in the language.
- Two types are equal only if they are written the same: a named struct
  equals only itself (not an anonymous struct with the same fields, as
  in LLVM), and `(VECTOR 4 I32)` differs from `(VECTOR 2 I64)`.
- **First-class** types (those a value, parameter, result, `load`,
  `store`, `phi` or `select` may have) are all of `type`. **Sized**:
  all of them (a struct may not contain itself by value, §4.1).
  **Aggregates**: structs and arrays.
- **Integer** types: `i1 … i64` and vectors of them. **Float** types:
  `float`, `double` and vectors of them.
- A function type's `cc` defaults to `ccc`; `...` makes it variadic.

### 2.1 Arrays

**Decided** (owner, 2026-09-28; §14 item 8). `(ARRAY N T)` is LLVM's array type: `N`
elements of the sized type `T`, laid out contiguously, `N` a
non-negative decimal integer (`(ARRAY 0 I8)` is the flexible trailing
member of a struct, as in C; `array length must be a non-negative
integer`, `array length 5000000000 is too large` above `2^32 - 1`).
`T` may be any type, arrays and structs included; a vector's element
may not be an array (§2). An array is an aggregate: it is a value
(`load`, `store`, `phi`, `select`, a parameter or a result), it is
built by `((ARRAY N T) v..)` (§3) or `insertvalue`, read by `extractvalue`
(§6.4), and addressed by `getelementptr` (§6.5). It has no `bits`
(`bitcast cannot convert [4 x i32]`), is not atomic, and is not a
`switch` or `icmp` operand. types.md §8.3 lays a string's bytes and an
array object's elements out as `(ARRAY 0 T)` at the end of the header
struct, and §8.2 holds the type table as `(ARRAY N (STRUCT fib.typerec))`.

## 3. Values and literals

```
value ::= NAME                  ; a parameter or let-bound name (§5.3)
        | (GLOBAL NAME)         ; the address of a global or function: ptr
        | (I1 INT) | (I8 INT) | (I16 INT) | (I32 INT) | (I64 INT)
        | (FLOAT FLT) | (DOUBLE FLT)
        | (PTR NULL)
        | ((VECTOR N T) v₁ .. v_N)   ; N literal elements: numbers, or NULL for ptr elements
        | (STRING STR)          ; ptr to a private constant NUL-terminated byte array
        | (LITERAL-STRUCT value*)    ; anonymous struct value
        | ((STRUCT NAME) value*)     ; named struct value, one value per field
        | ((ARRAY N T) value*)       ; array value, exactly N elements of type T (§2.1)
        | (ZEROINITIALIZER T)   ; every bit zero: 0, 0.0, null, and so on through aggregates
        | instruction           ; §6
FLT   ::= a float token | INF | -INF | NAN   ; §1
```

- `(IK n)`, `K` one of 1, 8, 16, 32 and 64, accepts `-2^(K-1) ≤ n ≤ 2^K - 1`, stored as its low `K`
  bits: `(I8 255)` and `(I8 -1)` are the same value. Outside that range
  the literal is an error `integer literal n out of range for iK`
  (liar accepted `(I8 300)` as 44, `lir-audit/tc/i8_overflow_lit.lir`).
- `(FLOAT x)` and `(DOUBLE x)` round to nearest; a finite literal that
  rounds to an infinity is an error `float literal out of range for
  double` (liar read `1e999` as infinity).
- A struct or array value is built by `insertvalue`s when a field is
  not a constant; its fields are evaluated left to right. An array
  literal has exactly `N` elements, each of type `T` (`[3 x i32]
  literal: 3 elements expected, found 2`; `[3 x i32] literal: element
  2 has type i64, expected i32`). `(ZEROINITIALIZER T)` is a constant
  of any sized `T`; it emits no instruction.
- `(STRING "…")` may appear in any function and as a global
  initialiser (§9); each occurrence is its own constant: two equal
  strings have two addresses, on both paths and at every optimisation
  level. lair writes the constant `private` and never `unnamed_addr`
  (LLVM merges only `unnamed_addr` constants; until 2026-10-25 lair
  marked every string so and the spec and the code disagreed: decision
  D12 of docs/design/lir-language.md, **Decided** owner 2026-10-25, the
  code follows the spec; §14 item 22; `cases/lir/instr/string-own-constant.lir`).

## 4. Modules

```
module ::= item*
item   ::= (DEFSTRUCT MUTABLE? NAME (type*))       ; mutable: its fields may be written without uniqueness evidence (§6.14.5 rule 8)
         | (GLOBAL mod* NAME type init)            ; mutable
         | (CONSTANT mod* NAME type init)          ; read-only
         | (DECLARE-GLOBAL HIDDEN? NAME type)      ; a variable defined elsewhere (§4.4)
         | (DECLARE HIDDEN? cc? NAME rtype (ptype* ...?))  ; a function defined elsewhere
         | (DEFINE mod* cc? (NAME rtype) (param*) block+)
         | (TARGET (CPU STR) (FEATURES STR)?)        ; the CPU and features to generate code for (§4.5)
         | (DEPENDENT-LIBRARY STR)                   ; a native library the code needs linked: LLVM's !llvm.dependent-libraries (§4.9)
         | (OWNERSHIP (RETAIN (GLOBAL NAME)) (RELEASE (GLOBAL NAME)) (UNIQUE (GLOBAL NAME)) (ALLOC (GLOBAL NAME))
                      (STACK-INIT (GLOBAL NAME)) (STACK-END (GLOBAL NAME)) (HEADER (STRUCT NAME)))
                                                   ; what the ownership forms lower to (§6.14.6)
         | (DEFINE-MACRO NAME (PNAME* ...?) block+)        ; a compile-time macro (§4.6)
         | (REQUIRE-MACROS STR)                         ; the macros of a library file (§4.6)
         | (IMPORT STR)                                 ; another module of the program (§4.7)
         | (MACRO-IMPORT STR)                           ; a module compiled into the macro session only (§4.6)
         | (MODULE-MACRO NAME)                          ; the macro applied to the rest of the module's forms (§4.6)
         | (INCLUDE STR)                                ; a file's text read in place (§4.6)
         | (DEFINE-READER-MACRO STR (PNAME) block+)     ; a reader macro on a character (§4.8)
         | (DEFINE-DISPATCH-MACRO STR (PNAME) block+)   ; a reader macro on the character after # (§4.8)
         | (DEFINE-READER-CLASS STR class)              ; a character's class (§4.8)
         | (DEFINE-READER-DEPTH N)                         ; the nesting the reader allows (§4.8)
         | (DEFINE-READER-BOM SKIP)                        ; a byte order mark at the start of a text read after it is skipped (§4.8)
         | (DEFINE-TOKEN-READER (PNAME PNAME) block+)      ; the module's token reader (§4.8)
         | (BEGIN item*)                                   ; a splice: each form a top-level form in its place (§4.6)
class  ::= WHITESPACE | CONSTITUENT | TERMINATING
param  ::= (type PNAME) | ((PTR (ADDRSPACE 1)) OWNED PNAME)    ; a parameter; `owned` is its ownership kind (§6.14.2)
mod    ::= PRIVATE | INTERNAL | EXTERNAL           ; linkage (§4.3), at most one
         | HIDDEN                                  ; visibility (§4.3)
         | ALWAYSINLINE                            ; a function the optimiser always inlines (§4.3)
         | NOINLINE | COLD                         ; LLVM's function attributes of those names (§4.3), `define` only
         | SHARED                                  ; a global in block-shared memory of a kernel target (§6.9a), `global` only
         | UNSAFE                                  ; the fence of §6.14.4 is open inside the function, `define` only
         | (EXPORT-NAME STR)                       ; LLVM's "wasm-export-name" attribute (§4.3), `define` only
block  ::= (BLOCK LABEL instr+)
```

Anything else at the top level is an error `expected a top-level form
(define, declare, declare-global, defstruct, global, constant), found …`
(liar silently ignored top-level expressions, `lir-audit/fuzz2.txt`). The
message names the forms of the first M3 pass; `(TARGET ..)` (§4.5) is a top-level form too, and
is left out of the text so that the Rust `lair`, the oracle of the compare scripts, says the same;
so are `define-macro` and `require-macros` (§4.6), `import` (§4.7) and the six reader forms of §4.8, which are not items but directions to the
reader, and a macro use whose macro is not yet defined gets this message too.
Items may appear in any order and refer to each other in any order; a macro must be defined
before it is used (§4.6).

### 4.1 Module-level rules

- Functions and globals share one namespace; structs have their own.
  A name defined twice is `duplicate definition of @f` or `duplicate
  struct %struct.s` (liar kept the first of two `define`s of one name,
  `tc/dup_fn.lir`).
- Names beginning with `llvm.` are reserved: `name @llvm.x is reserved`.
- Every `(STRUCT S)` used anywhere must be defined: `undefined struct
  %struct.S`. A struct that contains itself by value, directly or
  through other structs, is `struct %struct.S contains itself by value`.
- Every `(GLOBAL f)` must be defined or declared in the module: `undefined
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
| `ALWAYSINLINE` (canonical name `inline`) | `alwaysinline` | **Decided** (owner, 2026-10-20; docs/design/inline-lir.md): a `define` only (`global cannot be inline: inline is for a function`, `declare cannot be inline`), once (`duplicate modifier inline`), alongside any linkage word; the optimiser inlines every call of it it can see. Inlining is not a language rule: a call through a pointer or from another module is a call, and an `inline` function that is recursive is not inlined into itself |
| `noinline` | `noinline` | **Decided** (owner, 2026-10-25: lIR spells what LLVM IR spells, and lair reads no front end's convention out of band): a `define` only (`declare cannot be noinline`, `global cannot be noinline: noinline is for a function`), once (`duplicate modifier noinline`), not with `inline` (`inline and noinline exclude each other`: LLVM's verifier refuses `alwaysinline` with `noinline`); the inliner never inlines the function. For the out-of-line half of a fast path, which LLVM would otherwise inline into its one caller (the runtime's `fib.alloc-slow`, `fib.retain-slow`, `fib.release-slow`, `fib.stack-slow`) |
| `cold` | `cold` | **Decided** (owner, 2026-10-25): a `define` only (`declare cannot be cold`, `global cannot be cold: cold is for a function`), once; the function is rarely called: when LLVM weighs the edges of a function, a block that a call of it post-dominates is cold too, so it is laid out off the hot path. For a trap or error path |
| `(EXPORT-NAME "NAME")` | `"wasm-export-name"="NAME"` | **Decided** (owner, 2026-10-25: the text is the contract; §14 item 20): a `define` only (`declare cannot be export-name`, `global cannot be export-name: export-name is for a function`), once (`duplicate modifier export-name`), among the other modifiers in any order; `NAME` a non-empty string (`export-name expects one string`, `.. found X`, `export-name: the name must not be empty`). LLVM's string attribute of that name: the WebAssembly backend marks the symbol exported in the object and `wasm-ld` exports the function under `NAME` (clang's `__attribute__((export_name))`); every other backend ignores it, and the C, JS and WGSL printers too. What a native shared library exports is not this but the linkage and visibility of this table: the `external` definitions that are not `hidden`, and nothing else (lair writes no version script or symbol list; until 2026-10-25 it took one beside the module). A front end writes it on each function a wasm host is to call |

A function's name carries no attribute: lair puts `alwaysinline`, `noinline` and `cold` on a function
when its `define` says so and never otherwise (until 2026-10-25 it made every function whose name ended
`-slow`, or held `-slow.`, `noinline`; a front end now writes the word, syntax.md §3.1). The C backend
writes them as gcc's `__attribute__((always_inline))`, `((noinline))` and `((cold))`; the JS and WGSL
backends have no use for them and ignore them. Cases: `cases/lir/attrs`.

A `declare` and a `declare-global` take `hidden` only: they name a
symbol defined elsewhere, whose linkage is that definition's (`declare
cannot be private`). `main` is exported: `main must not be private or
internal` (§7.2). A `private` or `internal` function may still be
`tailcall`ed, called through its address and used as an initialiser
in its own module; what it cannot be is seen from outside (§11).

### 4.4 External globals

**Decided** (owner, 2026-09-28; §14 item 10). `(DECLARE-GLOBAL NAME T)` names a
variable of type `T` that another module or the C library defines: a
`declare` for data. `(GLOBAL NAME)` is its address; `(LOAD T (GLOBAL NAME))` reads it
and `store` writes it, under the typed direct-access rule of §6.5. It
has no initialiser and is never `constant` in lIR (`store to constant
@g` never fires on it; a store to a read-only definition elsewhere is
undefined behaviour, as in C). `(DECLARE-GLOBAL stderr PTR)` reaches
glibc's `stderr`; `errno` is a macro over a function there, not a
symbol, so `(DECLARE __errno_location PTR ())` is the way to it.

### 4.5 The target

**Decided** (owner, 2026-10-04, SIMD wave 2, package P0; §14 item 12). A module may name the CPU
and the CPU features its code is generated for:

```
(TARGET (CPU "x86-64-v3") (FEATURES "+avx2,+fma"))
```

`cpu` is required; `features` is optional and defaults to the empty
string. Both are LLVM's names for the host's architecture (the x86
backend today). Why it is in the module: a program's vector code, and the
number of lanes its front end chooses, depend on the target; `fibc emit`
on one machine and `lair build` on another must agree, so the choice is
written where both can read it.

- The module's CPU and features win over the host's and over
  the environment variable `FIB_TARGET_CPU` (§12). Without a `target`
  form the host's CPU and features are used, or the CPU `FIB_TARGET_CPU`
  names, as before.
- Every function the module defines carries `"target-cpu"` and
  `"target-features"` attributes with the two strings. LLVM's code
  generator, its optimiser's cost model (the vector width it picks) and
  the JIT all read the attributes before the machine's own CPU, so the
  module's target wins over the host's and over `FIB_TARGET_CPU` in every
  path (`compiler/tests/native/p0-target.sh` counts `vfmadd` and `ymm`
  in the assembly), and `emit-llvm` shows the attributes.
- `lair run` executes the code on the host: the features the module
  asks for must all be in the host's, else `target: the host does not
  support +avx512f` (a program that would die of SIGILL is refused
  instead). `lair build` and `emit` may name any CPU and features: the
  executable is for whoever runs it.
- At most one `target` form (`a module has at most one target form`).
- Rules, checked by the parser and §10 before LLVM is called: `target needs (cpu "NAME")`; `target: unknown
  field NAME` (anything but `cpu` and `features`); `target: expected (cpu "NAME") or (features "LIST"), found
  X` (a field that is not a list); `target: cpu expects one string` and `target: features expects one string`;
  `target: cpu must be a non-empty string of letters, digits, ., _ and -`; `target: features must be a
  string`; `target: each feature must be +name or -name, found "avx2"` (the name has letters, digits, `.`,
  `_` and `-`; an empty item or one with a space is found too); `target: duplicate cpu` and `target:
  duplicate features`; `a module has at most one target form`.
- LLVM itself ignores, with a warning on standard error, a CPU or
  feature name it does not know; lIR does not carry LLVM's tables, so it
  cannot reject them. A spelling mistake therefore costs speed, never
  correctness: the semantics of every instruction are the same on every
  target (§6.12 for what is not).
- Per-function targets (multiversioning) are not in lIR.

### 4.6 Macros

**Accepted by default** (docs/design/lir-language.md decision D1, taken by default in the gloss design of 2026-10-25; the owner may
override; §14 item 15). lIR has compile-time macros: the one named exception to "a new lIR form is LLVM IR's construct written as an
s-expression", because LLVM has no such thing and the languages built on lIR (gloss) are libraries of them. Everything else here keeps
LLVM's spelling: a macro's body is ordinary lIR, and what it returns is parsed as the text would be.

```
(DEFINE-MACRO NAME (PNAME* ...?) block+)
(REQUIRE-MACROS "FILE")
```

**Definition.** `(DEFINE-MACRO NAME (P₁ .. Pₙ) block+)` defines the macro `NAME`. Its body is the function
`(DEFINE (lir.macro.NAME PTR) ((PTR P₁) .. (PTR Pₙ)) block+)`: every parameter is a `ptr` to an s-expression object (below), and it
returns one. The body is checked and compiled when the form is read, as any `define` is (§10; the module's own `define`s are not visible to
it: a macro runs before the module exists), and its errors are reported where they are in the body, prefixed `macro NAME:`. With `...`
after the last name, `Pₙ` is bound to the list of the arguments past the first `n - 1` (`(DEFINE-MACRO defun (name params arrow ret body ...) ..)`),
as `...` ends a variadic parameter list in a `declare`; `...` must follow a name (`... must follow a parameter name`) and be last
(`... must be the last parameter`). A macro has at most 8 parameters, what the host's call bridge passes (`a macro takes at most 8 parameters`);
the names are valid names (§5.3), each once (`duplicate parameter P`). `NAME` is a valid name, so never one of lIR's words (§2: a macro
named in lower case cannot capture a form, and one named in upper case is refused as every such name is), and is defined once (`duplicate
macro NAME`).

**Use.** Once defined, `(NAME a₁ .. aₘ)` anywhere in a later form of the module, at the top level or inside one, is a use: the macro is called
with the arguments as objects (unexpanded) and the use is replaced by the form it returns. A use before the definition is not a use (the
top-level error of §4, or `unknown instruction NAME` inside a function). The argument count is `n`, or at least `n - 1` with `...`
(`macro NAME expects 1 argument, found 2`, `.. expects 4 or more arguments, found 3`).

**The expansion rule.** lair reads a module's forms as data; each top-level form is expanded, then parsed as §4 says. A form is expanded
outermost first: a use is replaced by its expansion; while the head of the result names a macro it is expanded again; then the children
of what is left are expanded, left to right. Nothing of the macro's own result is kept from expansion, and nothing is expanded twice. A
result whose head is a macro 512 times in a row is `macro NAME: the expansion does not end after 512 steps`. A result that is not valid
lIR is the parser's or the checker's ordinary error: a form the macro made carries no position of its own and is reported at the use; a
form the macro passed through (an argument, or part of one) keeps its position. The module a macro-free text parses to is the same as
before this section existed, and the reader hands each of its forms to the parser without a walk.

**Splices.** A top-level form headed `begin`, written or returned by a macro used at the top level, is a splice: its elements are each
expanded and parsed as a top-level form, in order, in its place. So a macro may expand to several items (a `defrecord` is a `defstruct`,
a constructor and the accessors), a spliced `define-macro` defines its macro for the forms after it, a splice in a splice is flattened,
and `(BEGIN)` is nothing. The word is Scheme's: a top-level `begin` splices its definitions there (Common Lisp's top-level `progn` is the
same); LLVM has no spelling for it, so it is lIR's own, as `define-macro` is, and nothing new is invented. A macro use is applied before
anything in its result is expanded, so the splice is seen whole. Inside a function `begin` is no instruction: a macro used there whose
result is a splice is `macro NAME: a splice (begin ..) is a top-level form, not an instruction` at the use, and a written one is `unknown
instruction begin`. A spliced reader form (§4.8: `define-reader-macro`,
`define-dispatch-macro`, `define-token-reader`) binds as a written one does, for the text read after the top-level form that spliced it;
the elements of the splice were read before it ran, so a binding it makes reaches them no more than a written one reaches its own form
(`cases/lir/macros/splice-define-reader.lir`). The cases are `cases/lir/macros/splice-*.lir` and `reject-splice-in-body.lir`.

**Scope.** A macro is visible to the forms after its definition in the module, the bodies of later `define-macro`s included (so a macro
may expand to a use of an earlier one, and its own body may use one), and its function `(GLOBAL lir.macro.NAME)` is declared in the bodies of
later macros, which may call it (`cases/lir/macros/position.lir`); a body may call its own function too, which is the module's own
`define`, so a macro recurses over the forms it is given directly (`cases/lir/macros/self-recursive.lir`). A body holds blocks only: it
declares nothing else, and calls nothing of the C library or of the module. `(REQUIRE-MACROS "FILE")` defines, in the requiring module, the
macros of the library `FILE`, a text of `define-macro` and `require-macros` forms and nothing else (`expected define-macro or
require-macros, found declare`): a library is not a module, there is nothing to link its items into (ADR 0024). `FILE` is read as written
when absolute, else beside the requiring file (`cases/lir/macros/x.lir` requires `"../../../lib/lir/defun.lir"`), or relative to the
working directory for a text with no file (`jit-add-source`, §11). A library is loaded once per module, however often it is required, identified by its canonical path (realpath(3): `.`, `..` and symbolic
links resolved by the OS, the authority on which file is which; a lexical normalisation would be wrong across a link), so two spellings
of one file are one library (`cases/lir/macros/require-twice-spelled.lir`), while every message names the path as written; a
library that requires the module or a library being loaded is `require-macros: FILE requires itself`; an error in a library is
`require-macros: FILE:L:C: error: ..` at the require. This is the smallest import there is: one form, one file, no names, no renaming,
no search path; a library of macros is just a file of them, and what a module needs it names by path. `(IMPORT "FILE")` (§4.7) is the
same shape for a module: its functions, globals, structs and macros.

**Hygiene.** None in v1: a macro's result is read as if written in place, so a name it binds can capture or collide with one of the
use's; `lir.sexp.gensym` makes a fresh name for a binding the macro introduces (`cases/lir/macros/gensym.lir`).

**The form type.** A form of lIR is one of five kinds: a symbol, an integer, a float, a string or a list, each with its position (file,
line, column) and its span (start and end, byte offsets). That is the reader's output (§1: what the text reads as), what a macro takes and returns, and what a **reader macro**
(§4.8) returns: their results and a macro's are the same type, interchangeable. lair holds the type in two representations that convert without loss: its own value (`lir.sexp/Sexp`, what the reader
makes and the parser takes) and the objects a macro's code sees, laid out thus, owned by lIR and by no front end (fibber's `Form` is not
them):

```
(DEFSTRUCT lir.sexp (I64 I64 I64 PTR I64 I64))                   ; the start of every object: tag, line, column, file, start, end
(DEFSTRUCT lir.sexp.symbol (I64 I64 I64 PTR I64 I64 PTR I64))    ; 1, position, span, bytes (a NUL after them), count
(DEFSTRUCT lir.sexp.integer (I64 I64 I64 PTR I64 I64 I64))       ; 2, position, span, value
(DEFSTRUCT lir.sexp.float (I64 I64 I64 PTR I64 I64 DOUBLE))      ; 3, position, span, value
(DEFSTRUCT lir.sexp.string (I64 I64 I64 PTR I64 I64 PTR I64))    ; 4, position, span, bytes (a NUL after them), count
(DEFSTRUCT lir.sexp.list (I64 I64 I64 PTR I64 I64 PTR I64 I64 I64)) ; 5, position, span, items (an array of ptr), count, capacity, bracket (0 `()`, 1 `{}`, 2 `[]`)
```

An atom of the text is an integer when it is an integer token (§1) whose value fits an `i64`, a float when it is a float token
(`inf`, `-inf`, `nan` included), else a symbol: `(GLOBAL f)`, `(STRUCT s)`, names, keywords and `...` are symbols, and so is a vector type `(VECTOR N T)`,
a symbol of exactly that text (one reads back as the type). A string is its bytes; `(LITERAL-STRUCT ..)` and `[ .. ]` are lists with their bracket.
The position is the form's: its line and column, 1-based, and its file as a NUL-terminated path (the module's, or the library's for a
form in one); line and column 0 and a null file for an object a macro made. The span is the text the form was read from: the byte
offset in its file of its first character and of the byte after its last, counted from 0 (a list ends after its closing bracket, a
string after its closing quote, whatever its escapes; `cases/lir/macros/span.lir`); a form a reader macro returned spans the text the
run consumed (§4.8). A form no text made, an object a macro made, has no span: start and end 0, and it gets none when the macro
returns (the expander gives it the use's position, not the use's span), unless `lir.sexp.at` or `lir.sexp.at-position` gave it one; a
form a macro passed through keeps its span. Spans are offsets
below 2³²; the reader keeps one word per form for them. Going back, an integer prints in decimal, a float as the
shortest decimal that reads back to it or `inf`, `-inf`, `nan`. The layout is the contract: a macro may read the fields with
`getelementptr`, but the functions below are the way, and they are declared in every macro's body without a `declare` (they and the
stream's functions of §4.8 are the only names that are):

| Function | Does |
|---|---|
| `(lir.sexp.tag s) : i64`, `(lir.sexp.line s) : i64`, `(lir.sexp.column s) : i64`, `(lir.sexp.file s) : ptr` | the tag and the position of any object |
| `(lir.sexp.start s) : i64`, `(lir.sexp.end s) : i64` | the span of any object: the byte offsets of its first character and of the byte after its last, 0 and 0 for a made one |
| `(lir.sexp.at made orig) : ptr` | `made` given `orig`'s position and span (line, column, file, start, end); `made`. A macro that rebuilds a form (a definition without a marker) keeps where it was written, and the parser's errors in the copy are reported there (`cases/lir/macros/at.lir`, `reject-at-position.lir`) |
| `(lir.sexp.at-position made line column start end) : ptr` | `made` given the position `line:column` and the span `start..end` by value, its file kept (a made form's is null; `lir.sexp.at` gives it another's); `made`. A form made where no object stands gets the place it has in the text: a reader macro that reads `7/2` as three forms puts each at its own column and bytes, and a prefix reader macro spans its head over the prefix character alone (`cases/lir/readers/prefix-span.lir`); the parser's errors in it are reported there (`cases/lir/macros/reject-at-position-value.lir`) |
| `(lir.sexp.symbol cstr) : ptr`, `(lir.sexp.string cstr) : ptr` | a symbol or a string of a NUL-terminated text (`(STRING "DEFINE")`), copied |
| `(lir.sexp.string-bytes p n) : ptr` | a string of the `n` bytes at `p`, any bytes, a NUL among them included, copied with a NUL after them (null for `n < 0`): a string a NUL-terminated text cannot give, made without a write into an object (`cases/lir/macros/string-bytes.lir`) |
| `(lir.sexp.integer v) : ptr`, `(lir.sexp.float v) : ptr` | an integer of an `i64`, a float of a `double` |
| `(lir.sexp.float-text cstr) : ptr` | a float of a decimal text (`strtod`, rounded once): the way a reader macro makes a float from digits it read, since a symbol's text is never read as a number for it (§4.8) |
| `(lir.sexp.list bracket) : ptr` | an empty list with that bracket |
| `(lir.sexp.push l x) : ptr` | `x` appended to the list `l`, which grows; `l` |
| `(lir.sexp.count l) : i64`, `(lir.sexp.nth l i) : ptr`, `(lir.sexp.bracket l) : i64` | a list's count, its `i`th item (null out of range), its bracket |
| `(lir.sexp.text s) : ptr`, `(lir.sexp.len s) : i64` | a symbol's or a string's bytes (NUL-terminated) and count |
| `(lir.sexp.int s) : i64`, `(lir.sexp.flt s) : double` | an integer's or a float's value |
| `(lir.sexp.is s cstr) : i1` | whether `s` is the symbol with that text |
| `(lir.sexp.gensym cstr) : ptr` | a fresh symbol `PREFIX.N`, `N` counting up through the program: one counter per program read (the loader's, given to every module's session, ADR 0013: no global state), so the names two modules of a program make never coincide (`cases/lir/macros/gensym-program.lir`) |
| `(lir.sexp.expand form) : ptr` | the form expanded through the module's macros as the expander would (outermost first, then the children), as an object; null when that expansion fails, and its error is then the use's (below) |
| `(lir.sexp.set l i x) : ptr` | item `i` of the list `l` replaced by `x`, the dual of `nth`; `l` (out of range: nothing) |
| `(lir.sexp.fail at message) : ptr` | the expansion fails with `macro NAME: MESSAGE` at the object `at` (null: at the use); returns null, for the macro to return |

**Expansion from a macro, and macro-time state.** A lowering macro delegates: `lir.sexp.expand` hands a form back to the expander, which
expands it through the macros the module has at that point (a use of the calling macro included) and returns the result as objects; a
macro runs on the worker of its module for this (the one thread its reader macros run on, §4.8), parked while the expander answers,
and a nested use of the module's macros runs on the same worker inside that hook (`call-nest`, §11; a macro of an imported module on its
own module's worker). When the
nested expansion fails, the macro gets null and the nested error, where it is, is the use's error whatever the macro then returns
(`cases/lir/macros/expand-delegate.lir`, `reject-expand-fails.lir`); a reader macro (§4.8) runs while the text is read and has no
expansion to call: null (`cases/lir/readers/expand-null.lir`). 
**macro-import.** `(MACRO-IMPORT "FILE")` compiles the lIR module `FILE` into the macro session of the module that writes it (as the
library `lir.sexp` is), once per session by its canonical path, and declares its structs, its external `define`s and `global`s in the
bodies of the macros defined after it, as `lir.sexp.*` are; the module is read and expanded as a module of its own (its `import`s are
macro-imported into the session the same way, their interfaces its declarations) and checked as one, and an error in it is
`macro-import: FILE:L:C: error: ..` at the direction; a module that imports itself through others is `macro-import: FILE imports itself`.
Its globals are the session's state for the module's expansion: written by one macro run, read by the next of the same module, gone with
the session at the end of the module (`cases/lir/macros/macro-import.lir`); a macro of an imported module runs in its own module's session
(§4.7) and sees that module's, and an importer never sees them. So a type checker is ordinary runtime lIR that thin macros call. The three
directions that bring a file: `require-macros` brings macros (and reader macros) to the module's text; `import` (§4.7) brings a module into
the program and its interface to the importer; `macro-import` brings a module into the macro session only: it is neither linked into the
program nor exported, and a library may hold it beside `define-macro`.

**module-macro.** `(MODULE-MACRO NAME)`, Racket's `#%module-begin` as one direction: `NAME` is a macro defined before it with a `...`
parameter (`module-macro: NAME is not a macro`, `module-macro: NAME takes no ... parameter`); from the form after the direction to the end
of the module's text the reader collects the top-level forms, read under the readtable and not expanded, directions included (an `import`,
`define-macro` or `module-macro` written there is data the macro sees), `include` too, the one direction that also acts in the
collected region: it is collected, and then the file is read there under the readtable and its forms are collected after it, each object
naming the file it was read from, a nested include acting the same way, under include's once and cycle rules (below; `module-include.lir`,
`module-include-rearm.lir`, `reject-module-include-cycle.lir`), so the macro knows every file included, one with no forms too
(`module-include-names.lir`: an `(INCLUDE "FILE")` is the trace of a file of comments only; the forms of an included file name it
resolved, `FILE` beside the including file, and the include names it as written; the form written is the trace, as a C preprocessor's
line marker is, and no word or function is added for it), and an include the macro splices back reads nothing, its file read already; at the end of the text `NAME` is applied to them as its `...`
argument and its result is one top-level form, expanded as any is, a `begin` splice usually, whose spliced directions then take effect in
order (`cases/lir/macros/module-count.lir`, `module-splice.lir`); a result that is no top-level form is the parser's error at the direction
(`reject-module-result.lir`). A library may hold the direction, so a `require-macros` arms it and the text after is the macro's
(`module-library.lir`); a library that arms one when one is armed already (a library required by the library that armed) is
`module-macro: the module has a module macro already` at its direction (`reject-module-twice.lir`), and once one is armed a direction in the
module's own text is data the macro sees. A `module-macro` the result splices arms again and takes the rest of the splice, applied in turn.
Each module of a program (§4.7) has its own.

**include.** `(INCLUDE "FILE")` reads the file's text in place, under the module's readtable, its forms as if written there, each a
top-level form of the module (a direction, a macro use, an item), the objects a macro gets naming `FILE` as their file; once per module
by canonical path, in the set `require-macros` uses, a file that includes itself being `include: FILE includes itself`; an error in it is
`include: FILE:L:C: error: ..` at the include (`cases/lir/macros/include.lir`, `reject-include-self.lir`, `reject-include-error.lir`).
While a module macro is armed, written or spliced, the forms are collected for it rather than taken as top-level forms (module-macro,
above), so a whole program's files reach one macro: a wrapper that arms the macro and includes them is the program. A diagnostic in the
macro's result is reported in the module's file, at the line and column the form carries.

A macro that returns null without `fail` is `macro NAME returned no form`; one that returns an object that is not one of the kinds, a
symbol whose text is not UTF-8 (a symbol is the parser's atom; a string is any bytes), an object at no place a text has (a line and a
column from 1, both 0 for none; a span whose start is at most its end, below 2³², both 0 for none: what `lir.sexp.at-position` could
otherwise set), or a list deeper than 512, is `macro NAME returned ..` (`cases/lir/macros/reject-symbol-not-utf8.lir`,
`reject-at-position-span.lir`). The objects live until the module is parsed and are never freed before that; a
macro that traps ends lair as it would a program. The functions are implemented as an lIR module of lair's (`compiler/native/macros/lib.fib`),
compiled once into the macro session of a module that defines a macro; a module that defines none makes no session.

**Where expansion runs.** On every path that reads lIR text: `lair check`, `run`, `build`, `emit-llvm`, `dump-ast` (which prints the
expanded module: the way to see what a macro made), `lair cases`, and the library's `check-source` and `jit-add-source` (§11). A module a
front end hands lair as a tree (§11, `jit-add-module`, `build-executable-module`) has no macros: the tree is the parsed form, and
`define-macro` is not an item of it; fibber's emitter makes such trees, so nothing of fibber's compile goes through this section. A
backend without a compile-time runtime (lir2js, lir2c: the reader and the checker without LLVM, so nothing to run a macro's body) consumes
**expanded lIR**, the text `lair expand FILE.lir` prints (§12): the program read, expanded, its imports joined (§4.7) and parsed, printed through the printer one
top-level form per line, with no `define-macro`, `require-macros` or `import` left in it; a module with none prints as parsed. Their
differential checks (`compiler/tests/js/diff.sh`, `compiler/tests/c-backend/diff.sh`) feed a file that defines or requires macros, or imports, through
`lair expand` first, and when the expansion is refused that refusal is the verdict the backend is compared with: a `reject` case a macro
refuses is never seen by the backend. A file with no macros goes to the backend as it is, so that a diagnostic's position is the file's. The
first library is `lib/lir/defun.lir`: `(defun NAME ((P T)..) -> R block+)` is `(DEFINE (NAME R) ((T P)..) block+)`, in lIR over
these objects. The cases are `cases/lir/macros/`.

### 4.7 Imports

**Accepted by default** (gloss item 3, 2026-10-25: "lIR is LLVM IR as s-expressions, as thin as possible, with helpers for memory safety,
macros, extern hooks and importing dependencies"; the owner may override; §14 item 16). A module names the modules it uses:

```
(IMPORT "FILE")
```

**What it is.** A program is the union of its modules, joined as `llvm-link` joins LLVM modules: one namespace of exported names, a
`declare` resolved by a `define` of the same name in any module, internal names kept apart. LLVM has no `import` statement (a module
names its libraries, `llvm.dependent-libraries`, and a linker joins the rest), so the form is lIR's, kept to one word and a path, as
`require-macros` is: `FILE` is read as written when absolute, else beside the importing file, or relative to the working directory for a
text with no file. A module name with a search root is the next step, not this one: a path serves gloss's layers and a program's own modules,
and `require-macros` already takes one. The modules a program joins are each one compilation unit of the backend as before (ADR 0024
partitions the program, not the module), so a `define inline` of one module inlines into another, as a C `static inline` in a header does.

**Loading.** The importing module's forms are read in order; at an `import`, the file is read, expanded and parsed as a module of its own
(its `require-macros` and `import`s loaded first, depth first, in the order written), checked (§10) with the interface of what it imports,
and admitted to the program under the rules below; then the importer goes on. Every module is loaded once per program, however many
modules import it (a diamond holds one copy of its base, `cases/lir/imports/diamond.lir`), and a second `import` of a file in one module is
nothing. The named module is admitted last. A module that is being loaded, imported again through others, is a cycle: `import: FILE imports
itself` at the import that closes it, and as that error travels up each import adds its file and position (`import: b.lir:4:1: error:
import: a.lir imports itself`). An error in an imported module is `import: FILE:L:C: error: ..` at the import (the first one); a file that
cannot be read is `import: cannot read FILE`; `import` takes one string (`import expects one string, the file`, `import expects a string,
found 42`). A module is identified by its canonical path (realpath(3), as a library is in §4.6): two spellings of one file are one module,
loaded once (`cases/lir/imports/spelled-twice.lir`), and a cycle closes through any spelling (`reject-cycle-spelled.lir`); every message
names the path as written.

**The interface.** The importer sees, at the position of the import, as items of its own: a `(DECLARE HIDDEN? cc NAME R (T..))` for
every `define` of the imported module whose linkage is `external` (the default), with the define's calling convention, types and
visibility, as a C header declares a function (`inline`, `noinline` and `cold` are the definition's, §4.3, and the optimiser still sees the
body); a `(DECLARE-GLOBAL HIDDEN? NAME T)` for every `external` `global` or `constant`; every `defstruct`, copied (`mutable` kept); and
the imported module's `ownership` form (§6.14.6), so that the importer's managed values are verified against it. Nothing else: not the
imported module's own `declare`s and `declare-global`s (a module declares what it uses), not its `target`, and not its `internal` or
`private` names, which a use in the importer leaves undefined: `undefined function @helper: internal to FILE` (`undefined global` likewise,
`cases/lir/imports/reject-internal.lir`). An import is not transitive: what the imported module imports is its own; a module imports what it
uses, and a struct an imported signature names must be imported too if the importer names it. The imported names are in the importer's
namespace as its declarations are: defining or declaring one of them is `duplicate definition of @f` (§4.1).

**Macros.** The macros the imported module defines in its own file with `define-macro` are defined in the importer at the import, before
the forms after it (`duplicate macro NAME` if the importer has one); the macros it took through `require-macros` or `import` are not
(`cases/lir/imports/reject-transitive-macro.lir`). An imported macro runs in its module's session: it is used, `(NAME ..)`, and is not
`(GLOBAL lir.macro.NAME)` in the importer's macro bodies (§4.6 declares those for the importer's own session). `lir.sexp.gensym` counts per
program (§4.6): a name an imported macro makes and one the importer's own macro makes never coincide.

**The program's rules.** Each module is admitted in load order, the named module last, and its items meet the names the program has:

- a function or global with `external` linkage defined by two modules is `@f is also defined in FILE` at the second definition;
- a `defstruct` of a name another module defines is `%struct.S is also defined in FILE` (structs are nominal and share one namespace; a
  module's own structs carry its own prefix);
- a `declare` or `declare-global` of a name the program defines or declares with another type is `@f is defined as TYPE in FILE` or
  `@f is declared as TYPE in FILE` (`(FUNCTION I32 (I32))`, the text of §2); one of the same type is the same symbol: the program keeps the
  definition, or the first declaration, and drops the rest (`printf` declared by every module is one `declare`);
- an `internal` or `private` name the program already has, in any linkage, is the module's own: it is renamed `NAME.N`, the smallest `N`
  from 1 that neither the program nor the module has, throughout the module, as `llvm-link` renames (`lair dump-ast` shows the program
  after renaming: `helper` and `helper.1` in `cases/lir/imports/diamond.lir`);
- one module carries the program's `target` form (§4.5): a second is `target: FILE also has a target form`;
- one module carries the program's `ownership` form (§6.14.6), the one whose interface the others see: a second is `ownership: FILE
  also has an ownership form`, at the second form, whether the module writes one beside the one it imports
  (`cases/lir/own/reject-import-ownership-twice.lir`) or two modules each carry one;
- `main` is any module's, under §7.2.

Nothing numbers anything: a name is a name. **ADR 0024.** fibber's emitter numbers a program's object types (`fib.types`' rows) and its
keywords in interning order and writes the numbers into the lIR as constants; two emitted programs give one number two meanings, so their
lIR cannot be joined, and precompiled Fib libraries wait for symbolic ids (the ADR's prototype). `import` meets the problem thus in v1: lair
joins modules by name and knows no id; the modules it joins are written as lIR (gloss's layers, a program's own modules), which have none;
and the only lIR that carries numbered ids is a whole emitted program, which defines `fib.types`, `fib.classes`, the runtime and `main`, so
two of them are `@fib.types is also defined in FILE`. That is the limit, stated: a module with numbered ids imports, and is imported by
nothing; when ids are symbolic the limit goes and no rule here changes.

**Where it runs.** On every path that reads lIR text, with the macros (§4.6): `lair check`, `run`, `build` (the JIT and the object cache
compile the program, ADR 0024), `emit-llvm` and `dump-ast` (which show the program), `lair cases`, `check-source` and `jit-add-source`
(§11, imports relative to the working directory). A module a front end hands lair as a tree has no imports; fibber's emitter makes such
trees, and nothing of fibber's compile goes through this section: a module with no `import` is read, expanded, parsed and checked as before,
and the program's tables are never built for it. A macro library (`require-macros`) imports nothing (`expected define-macro or
require-macros, found import`). The cases are `cases/lir/imports/`: a module imported by others is a case of its own too (run alone, it is
`no main function`).

### 4.8 Reader macros

**Proposed** (the lead, 2026-10-25, for gloss item 6: the owner's direction that lIR is LLVM IR as s-expressions, as thin as possible, and
that a language brings its own syntax as reader macros, "Gloss/fibber might have [], but that doesn't mean the next language will"; §14
item 17). A module has a **readtable**, Lisp's shape: a character bound to an lIR function that runs at read time, takes the stream and
returns a form. lIR's own syntax (§1) is untouched: a module that binds nothing reads exactly as before, and the reader hands each of its
forms on without consulting anything.

```
(DEFINE-READER-MACRO "C" (STREAM) block+)      ; at the character C, the macro reads what follows and returns the form
(DEFINE-DISPATCH-MACRO "C" (STREAM) block+)    ; at #C, the same, the stream after both characters
(DEFINE-TOKEN-READER (STREAM TOKEN) block+)    ; after every atom: the atom's text as a symbol, and the macro returns the form to use
(DEFINE-READER-CLASS "C" CLASS)                ; C is whitespace, constituent or terminating to the lexer from here on
(DEFINE-READER-DEPTH N)                        ; forms nest N deep at most (1024 unless set; at most 4096)
(DEFINE-READER-BOM SKIP)                       ; a byte order mark at the start of a text read after this is skipped
```

**Character classes.** lIR's own whitespace (§1: space, tab, the other ASCII controls 9 to 13, comma, Unicode's White_Space) and its
own atom delimiters (whitespace, the brackets, `"`, `;`) are a choice of lIR's, and a language's choice may differ: fibber refuses the
Unicode spaces and ends a symbol at `'`. So the readtable holds **classes**: `(DEFINE-READER-CLASS "C" whitespace)` makes the character
`C` (a string of one UTF-8 character, any scalar: `"\xC2\xA0"` is U+00A0) whitespace between forms, `constituent` makes it part of an
atom it stands in and nothing between forms (an NBSP so classed reaches the token reader inside an atom, which may refuse it), and
`terminating` ends an atom (so `a@b` with `@` terminating is the atom `a` and then `(GLOBAL b)`, which the reader macro of `@` reads: a bound
prefix character glued to an atom reaches its macro when its class is terminating). A class changes what is skipped between forms and
where an atom ends, and nothing else: a character at the start of a form keeps lIR's own meaning there (`(`, `{`, `[`, `"`, `<`) unless a
reader macro is bound to it, and a terminating character that starts an atom (an unbound `@` before `name`) is the atom's first
character, as it always was. `(`, `)` and `;` cannot be reclassified (`define-reader-class: cannot change (, which is lIR's own`); the
class is one of the three words (`define-reader-class: the class is whitespace, constituent or terminating`). A later class replaces
an earlier; a character with no class set is what lIR's own lexer makes of it. A module that sets no class lexes exactly as before.

**Depth.** Forms nest 1024 deep unless the module says otherwise: `(DEFINE-READER-DEPTH N)`, `N` from 1 to 4096
(`define-reader-depth expects a count from 1 to 4096`), and the `N+1`th level is `nesting deeper than N` at its opening character. A
list is a level, and so is a reader macro's or a dispatch macro's run (it reads what is inside it: `'x` and `[x]` are one level each,
as fibber counts them); the token reader's run is not. fibber's 1000 (`TooDeep` at the 1001st frame) is `(DEFINE-READER-DEPTH 1000)`,
which gloss's layer 2 sets; a comparison harness that wraps a module in one more form sets one more.

**The byte order mark.** U+FEFF (the bytes EF BB BF) at offset 0 of a text is, to lIR's own lexer, a character like any other (§1:
it is no Unicode White_Space), so it starts an atom. A language may skip it: `(DEFINE-READER-BOM SKIP)` makes the reader pass a byte
order mark at offset 0 of every text read after the form under the table (an included file, a required library), the byte offsets of
spans counting its three bytes and the columns of line 1 not (the first form of a file that starts with one is at 1:1, its span from
3), as fibber's reader does; one anywhere else is still the lexer's (an atom's character, which the language's token reader may
refuse, as fibber refuses one with InvalidCharacter). The form is read after the text that holds it began, so it never reaches that
text's own start: a wrapper or a library sets it for the files it includes (`cases/lir/readers/bom.lir`, `reject-bom-elsewhere.lir`).
`SKIP` is the one choice (`define-reader-bom expects skip`); lIR's own reading is the default, and there is nothing to set back.

**Definition.** Each body is compiled as a macro's is (§4.6: blocks only, checked when read, the `lir.sexp.*` functions and the
stream's below declared in it, errors prefixed `reader macro C:`, `dispatch macro #C:` or `token reader:`), under a name of its own in
the module's macro session; a reader macro may call an earlier `(GLOBAL lir.macro.NAME)`. `"C"` is a string of one printable ASCII character
(`define-reader-macro: the character must be a string of one printable ASCII character`). A reader macro may not be bound to `(`, `)`
or `;`, and whitespace is not printable (`define-reader-macro: cannot bind (, which is lIR's own`): those are the reader's own, so that a
list and a comment are always a list and a comment. Any other character may be bound, `"`, `<`, `{`, `[`, `@`, `%` and `#` included, and
the module's own forms then read under the new table: a module that binds `[` has no array types `(ARRAY N T)` unless its macro makes them,
and one that binds `@` reads its globals through its macro. A dispatch macro may be bound to any printable character, `(`, `)`
and `;` included (`#(`, `#;`), since the `#` is read first. A later binding of a character replaces the earlier
(`cases/lir/readers/rebind.lir`), and a later token reader the earlier.

**Reading.** At each form the reader looks at its first character. A bound character runs its reader macro with the stream just after
it. `#`, once any dispatch macro is bound and `#` itself is not a reader macro, runs the dispatch macro of the character after it with the
stream after both; `#` before an unbound character is `no dispatch macro for #C` at the `#`, `C` the character whole, a multibyte one
included (`#é`, `cases/lir/readers/reject-dispatch-multibyte.lir`), and at the end of the text `unexpected end of input after #`. With `#` bound as a reader macro, that macro runs and the dispatch macros are not reached. Anything else is lIR's own
syntax; an atom (§1: its delimiters are the reader's and the classes', so `7/2`, `1_000` and `1.5f32` are one atom each) is read as it
always was, and when the module has a token reader it is then called with the stream after the atom and the atom's text as a
**symbol** object, whatever the text (`42` and `1.5` arrive as the symbols `42` and `1.5`: a number of lIR's is the token reader's to
read, and the parser reads an atom by its text, so a token reader that returns the symbol `42` as it is still gives the parser the
integer 42); what it returns is the form. **A symbol stays a symbol.** An atom of the text is classified by its text once, when it first
becomes an object (§4.6: an integer token an integer, a float token a float, else a symbol); a symbol object, one a macro made with
`lir.sexp.symbol` or the token reader was given, keeps its kind at every later macro boundary, whatever its text: `.5`, `inf` and `1e3`
made as symbols are symbols to the next macro, and the parser reads their text as it reads any atom's. A macro that wants a float from
digits it read makes one with `lir.sexp.float-text` (§4.6). The form a macro returns is at the position of the first
character (the `'` of `'x`, the `#` of `#(..)`); a form it read with `lir.read.form` keeps its own position
(`cases/lir/readers/positions.lir`). Its span is the text the run consumed, from the macro character (the `#` of a dispatch macro, the
atom's first for the token reader) to where the run left the stream, and so is that of every form in it the macro made (the `quote` of
`(quote x)`) unless the macro gave it a place (`lir.sexp.at`, `lir.sexp.at-position`: the `quote` over the `'` alone,
`cases/lir/readers/prefix-span.lir`); a form it read with `lir.read.form`, and the token reader's atom, keep their own
(`cases/lir/readers/span.lir`). A macro that returns null without a failure read **no form**: the reader goes on as if nothing
stood there, inside a list or at the top level, which is how `#_` discards the next form (`cases/lir/readers/discard.lir`). A failure
recorded with `lir.read.fail` is the module's error, at the position the macro gave or at the character; an error in a form the macro
read through `lir.read.form` (a lexical error, an unclosed list, a nested macro's failure) is the module's error where it is, whatever
the macro then returns (`cases/lir/readers/reject-nested-error.lir`). Under a readtable that has anything set, the first error is the
module's error: lIR's own lexer does not go on over the rest of the text looking for a lexical error of its own to report instead (as it
does for a module with no readtable, §1), since that text is the macros' to read (`cases/lir/readers/reject-fail-wins.lir`).

**Scope.** A binding applies to the forms after the form that made it, in the module and in a library it is made in; the bindings a
library makes with `require-macros` (§4.6) are the requiring module's from the require on: a library's reader forms go into the one
readtable the module and its libraries share (its classes, its depth and its byte order mark too), so the smallest import carries syntax as it carries macros
(`lib/lir/readers.lir`, `cases/lir/readers/required.lir`). A library may hold the six reader forms beside `define-macro` and
`require-macros`. The readtable is the module's and ends with it; another module starts with none. An `import` (§4.7) crosses no
readtable: an imported module is read under its own table, with the bindings its own text and its own required libraries make, and
none of the importer's (`cases/lir/readers/reject-import-leak.lir`: the importer's `?` is `undefined name ?` in the module it imports);
and an import gives the importer none of the imported module's bindings, as it gives it the module's macros
(`cases/lir/readers/import-own-table.lir`: `!` means one thing in the importer and another in the module it imports). Syntax travels by
`require-macros` only.

**The stream.** `STREAM` is a `ptr` to the module's stream, an object laid out thus and reached through these functions, declared in
every macro's body:

```
(DEFSTRUCT lir.read.stream (PTR I64 I64 I64 I64 PTR PTR PTR PTR I64 I64))  ; text, length, offset, line, column, file, mailbox, hook, failure message, failure position, failed
```

| Function | Does |
|---|---|
| `(lir.read.peek s) : i64` | the byte at the stream, -1 at the end of the text; nothing is consumed |
| `(lir.read.peek-at s n) : i64` | the byte `n` bytes from the stream (`n < 0`: behind it), -1 outside the text; nothing is consumed: a macro that looks further than the next byte asks for it, and no macro reads the struct's words (`cases/lir/readers/peek-at.lir`) |
| `(lir.read.next s) : i64` | that byte, consumed: a newline starts the next line at column 1, a UTF-8 continuation byte leaves the column, any other byte advances it |
| `(lir.read.position s) : i64` | the position of the stream, `line << 32 \| column`, what `lir.read.fail` takes |
| `(lir.read.offset s) : i64` | the byte offset of the stream in its text, what a span is made of (`lir.sexp.at-position`) |
| `(lir.read.skip s) : i64` | whitespace, commas and comments (§1 and the classes, the reader's own definition) consumed; the byte after them, -1 at the end |
| `(lir.read.form s) : ptr` | the next form of the text, read through the readtable, discards passed over: what a `[` macro calls until it sees `]`. Null when it cannot be read (the error is then the module's, above, and `lir.read.failed` says so), and null with nothing recorded when a discard stood before a closer or the end of the text (`[1 #_2]`: the closer is left for the macro to look at, `cases/lir/readers/discard-closer.lir`) |
| `(lir.read.failed s) : i1` | whether a failure is recorded on the stream: the macro's own (`lir.read.fail`) or a form's that `lir.read.form` could not read; a macro that got null tells no form from a failure by it |
| `(lir.read.fail s at message) : ptr` | the read fails with `reader macro C: MESSAGE` at the position `at`, or at the character for 0; returns null, for the macro to return |

The reader's cursor is the stream's: the macro runs with the stream where the reader was, and when it returns the reader goes on from
where the macro left the stream. The text is UTF-8 bytes; a macro that wants characters reads their bytes. `lir.read.form` and
`lir.read.skip` are the reader's own code run on the macro's behalf (through the mailbox of §11's call bridge), which is why they, and
not the macro, know what whitespace and a form are. A form that runs another macro runs it on the same worker thread, inside the hook
that asked for the form (`call-nest`, §11): one thread per module whatever the nesting, its 64 MiB stack holding every level, so the
depth above is the only limit on how deep macros read through macros.

**Hygiene.** None, as §4.6: a reader macro's result is read as if written in place.

**Where it runs.** On every path that reads lIR text, as §4.6 says, every module of a program (§4.7) under its own table; `lair expand`
prints a module read under reader macros as the plain lIR it read to, with no reader form left in it, and the backends' differential
checks feed a file that holds one through `lair expand` as they do a file of macros. A module a front end hands lair as a tree has no
readtable. The first library is `lib/lir/readers.lir`: `'x` reads as `(quote x)` and `[a b]` as `(vector a b)`, in lIR over the
stream; what `quote` and `vector` mean is then the module's, by `define-macro`. The cases are `cases/lir/readers/`. The stream's
functions are an lIR module of lair's (`compiler/native/macros/readlib.fib`) compiled into the macro session beside the form library;
the reader (`compiler/lir/sexp.fib`) consults the table per form and hands a bound character to the runner
(`compiler/native/macros/readers.fib`).

### 4.9 Dependent libraries

**Decided** (owner, 2026-10-25: the text is the contract; nothing lair does follows from a comment, a name convention, an environment
variable or an argument beside the module; §14 item 19). A module names a native library its code needs linked:

```
(DEPENDENT-LIBRARY "z")
```

It is LLVM's named metadata `!llvm.dependent-libraries = !{!{!"z"}}`, what clang's `#pragma comment(lib, "z")` makes and what an ELF
`.deplibs` section carries, written as an s-expression. `STRING` is the name as the linker's `-l` takes it (`z` for `libz`; `:FILE`
names a file, as `-l:FILE` does). Any number of forms, anywhere among the items; a name written twice is one library. Semantics: the
library's symbols satisfy the module's `declare`s, so lair lowers the form to that metadata on every path and links from it:
`lair build` adds `-lNAME` after the `-l` of its command line (static or dynamic by the build's options, `FIB_LINK`, `FIB_LINK_MODE`,
`FIB_STATIC`, docs/design/static-linking.md 3; `c m pthread dl rt util` are the C library's own and never a link word), `lair run`,
the JIT and a live session load the library into the process before the names rules look for its symbols (§11), a liro object records it
for `lair link`, and `(IMPORT "FILE")` (§4.7) keeps the imported module's forms in the union, as `llvm-link` concatenates the metadata.
A library that cannot be found or loaded is an error naming it, on the path that needed it. A `declare` the library would satisfy but
no form names is as before: undefined at link, or found in the process by the JIT.

Rules, checked by the parser: `dependent-library expects one string` (none, two, or a form that is not a string, `found X`);
`dependent-library: the name must not be empty`. The C backend prints nothing for the form (the C route of `fibc` links from it); the JS
and WGSL backends have no linker and ignore it; the printer writes it back as read, and `dump-ast` shows it as `dependent-library`.
Until 2026-10-25 lair read the library list out of comments `;; link-lib: NAME MODULE` before the module, and from an argument beside
the tree on the in-memory path; a comment is lexically nothing (§1) and now carries nothing. Cases: `cases/lir/link`.

## 5. Functions, blocks and names

### 5.1 Blocks and terminators

A function has at least one block (`function @f has no blocks`; liar
emitted an empty body, `tc/empty_fn.lir`). The first block is the
entry block. A label is an atom that is not empty and not one of lIR's
words (§2: `invalid label RET`; `entry` and `ret` are labels). Labels are
unique within a function (`duplicate block
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
(LET ((NAME value)*) instr+)
```

Parameters and `let`-bound names are the function's **SSA names**:

- Each name is bound once per function, parameters included (`duplicate
  name x in @f`). There is no shadowing (**Decided**, §14 item 6).
- A name may not be one of lIR's words (§2), an atom that is the text
  of a word, `-` before it or not (`reserved word ADD used as a name`).
  A lower-case atom is never a word: `add`, `null`, `void` and `let` are
  names, which position tells from the forms (`(ADD a b)` against a
  name `add`).
- A name's scope is not the `let` body. A **use** of `x` is valid if
  the binding of `x` is evaluated before the use in the same block
  (else `x is used before its binding in block L`), or if `x` is bound
  in a block that dominates the using block (`x does not dominate its
  use in block L`, `tc/use_nondominating.lir`, `tc/nondom2.lir`; liar
  compiled both to code that read an uninitialised register).
  Parameters dominate every block.
- A name is an atom that is not a number and not `...` (`invalid name
  5`); `@` and `%` are characters of a name like any other. A binding is
  `(NAME value)` (`let binding must be (NAME value)`).
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
(PHI T (LABEL value)+)
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
  name, `(GLOBAL name)` or a literal: `phi incoming value must be a name, a
  global or a constant` (an instruction there would have to run in the
  predecessor, after its terminator's operands). This is stricter than
  LLVM, which takes any constant expression (**Decided**, §14 item 7).
- When one terminator names the block twice (`(BR c j j)`, or a `switch`
  with two cases to one label), LLVM wants one phi entry per edge; lIR
  takes one per predecessor and `lair` repeats it per edge.
- An incoming value from `M` is used at the end of `M`: a name in it
  must be bound in `M`, in a block that dominates `M`, or be a
  parameter (`x does not dominate the end of block M`). A `phi` may therefore name a value bound later in its own
  block when that block is its own predecessor (a loop).

## 6. Instructions

The instructions by their words; the sections below give each form's operands and result.

```
instr   ::= (binop flag* a b) | (FNEG a) | (CTPOP a) | (ovf a b)                           ; §6.1
          | (fn1 a) | (fn2 a b) | (fn3 flag* a b c)                                     ; §6.1
          | (ICMP ipred a b) | (FCMP fpred a b) | (SELECT c a b)                         ; §6.2
          | (conv T v)                                                                  ; §6.3
          | (EXTRACTELEMENT v i) | (INSERTELEMENT v e i) | (SHUFFLEVECTOR a b m)
          | (EXTRACTVALUE s k..) | (INSERTVALUE s v k..) | (reduce flag* a? v)            ; §6.4
          | memory | atomic | control | kernel | own                                    ; §6.5 to §6.9a, §6.14
binop   ::= ADD | SUB | MUL | SDIV | UDIV | SREM | UREM | FADD | FSUB | FMUL | FDIV | FREM
          | AND | OR | XOR | SHL | LSHR | ASHR
ovf     ::= SADD-OVERFLOW | SSUB-OVERFLOW | SMUL-OVERFLOW
flag    ::= REASSOC | CONTRACT | ARCP | AFN | NSZ        ; fast-math, on the float forms that take it (§6.1)
          | NSW | NUW                                    ; facts, on add, sub, mul and shl (§6.13)
fn1     ::= FSQRT | FABS | FFLOOR | FCEIL | FTRUNC | FROUND | FROUNDEVEN | ABS | CTTZ | CTLZ
fn2     ::= FCOPYSIGN | FMIN | FMAX | FMINNUM | FMAXNUM | SMIN | SMAX | UMIN | UMAX
fn3     ::= FMA | FMULADD
ipred   ::= EQ | NE | SLT | SLE | SGT | SGE | ULT | ULE | UGT | UGE
fpred   ::= OEQ | ONE | OLT | OLE | OGT | OGE | ORD | UEQ | UNE | ULT | ULE | UGT | UGE | UNO
conv    ::= TRUNC | ZEXT | SEXT | FPTRUNC | FPEXT | FPTOSI | FPTOUI | SITOFP | UITOFP
          | PTRTOINT | INTTOPTR | BITCAST | FPTOSI-SAT | FPTOUI-SAT
reduce  ::= REDUCE-ADD | REDUCE-MUL | REDUCE-AND | REDUCE-OR | REDUCE-XOR | REDUCE-SMIN | REDUCE-SMAX
          | REDUCE-UMIN | REDUCE-UMAX | REDUCE-FADD | REDUCE-FMUL | REDUCE-FMIN | REDUCE-FMAX
          | REDUCE-FMINNUM | REDUCE-FMAXNUM
memory  ::= (ALLOCA (ALIGN N)? T n?) | (LOAD VOLATILE? (RANGE LO HI)? (ALIGN N)? T p)
          | (STORE VOLATILE? (ALIGN N)? v p) | (GETELEMENTPTR INBOUNDS? T p i..)
          | (MASKED-LOAD T p mask passthru (ALIGN N)?) | (MASKED-STORE v p mask (ALIGN N)?)
          | (GATHER T ptrs mask passthru (ALIGN N)?) | (SCATTER v ptrs mask (ALIGN N)?)
control ::= (RET v?) | (BR LABEL) | (BR c LABEL LABEL) | (SWITCH v LABEL ((T c) LABEL)*)
          | (UNREACHABLE) | (TRAP) | (PHI T (LABEL v)+) | (LET ((NAME v)*) instr+)
          | (CALL (GLOBAL NAME) a*) | (TAILCALL (GLOBAL NAME) a*)
          | (INDIRECT-CALL p fntype a*) | (INDIRECT-TAILCALL p fntype a*)
          | (SPIN-HINT) | (CACHE-LINE) | (STACK-ADDR) | (ASSUME c)
kernel  ::= (SREG NAME) | (BARRIER) | (SHFL-DOWN v l) | (SHFL-UP v l) | (SHFL-XOR v l)
          | (SHFL-IDX v l) | (BALLOT c)
own     ::= (ALLOC-OBJECT (STRUCT NAME) tid n?) | (ALLOC-STACK (STRUCT NAME) tid) | (DEALLOC-STACK s)
          | (COPY-VALUE v) | (DESTROY-VALUE v) | (IS-UNIQUE v) | (ADDRSPACECAST T v)
          | (LOAD COPY T a) | (LOAD BORROW T a) | (LOAD TAKE T a)
          | (STORE INIT v a) | (STORE ASSIGN v a) | (STORE BORROW v a)
```

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
| `(ADD a b)` `sub` `mul` `sdiv` `udiv` `srem` `urem` | same integer `T` | `T` |
| `(FADD a b)` `fsub` `fmul` `fdiv` `frem` | same float `T` | `T` |
| `(FNEG a)` | float `T` | `T` |
| `(AND a b)` `or` `xor` `shl` `lshr` `ashr` | same integer `T` | `T` |
| `(CTPOP a)` | integer `T` | `T` (`llvm.ctpop`) |
| `(SADD-OVERFLOW a b)` `ssub-overflow` `smul-overflow` | same integer `T` | `(LITERAL-STRUCT T I1)` (or `(LITERAL-STRUCT T (VECTOR N I1))` for a vector `T` of `N` lanes): the wrapped result and whether it overflowed (`llvm.sadd.with.overflow` and kin; **Decided**, §14 item 5) |

#### Floating-point functions and integer min/max

**Decided** (owner, 2026-10-04, SIMD wave 2, package P0; §14 item 12). Each
form takes a float or a vector of floats (`F`), or an integer or a vector of
integers (`I`), and acts on every lane. Each is one LLVM intrinsic and has
that intrinsic's exact semantics.

| Form | Operands | Result | LLVM |
|---|---|---|---|
| `(FMA a b c)` | same float `F` | `F`: `a*b+c` with one rounding, always | `llvm.fma` |
| `(FMULADD a b c)` | same float `F` | `F`: `a*b+c`, fused or not as the target prefers (the result may differ by one rounding between targets) | `llvm.fmuladd` |
| `(FSQRT a)` | float `F` | `F`: the correctly rounded root; negative gives NaN | `llvm.sqrt` |
| `(FABS a)` | float `F` | `F` | `llvm.fabs` |
| `(FFLOOR a)` `(FCEIL a)` `(FTRUNC a)` | float `F` | `F`: rounded to an integral value toward -inf, +inf, zero | `llvm.floor` `ceil` `trunc` |
| `(FROUND a)` | float `F` | `F`: to nearest, ties away from zero | `llvm.round` |
| `(FROUNDEVEN a)` | float `F` | `F`: to nearest, ties to even | `llvm.roundeven` |
| `(FCOPYSIGN a b)` | same float `F` | `F`: the magnitude of `a` with the sign of `b` | `llvm.copysign` |
| `(FMIN a b)` `(FMAX a b)` | same float `F` | `F`: if either operand is NaN the result is NaN; `-0.0` is less than `+0.0` | `llvm.minimum` `llvm.maximum` |
| `(FMINNUM a b)` `(FMAXNUM a b)` | same float `F` | `F`: if one operand is NaN the result is the other; the order of `-0.0` and `+0.0` is not specified | `llvm.minnum` `llvm.maxnum` |
| `(SMIN a b)` `(SMAX a b)` `(UMIN a b)` `(UMAX a b)` | same integer `I` | `I`, compared as signed or unsigned | `llvm.smin` `smax` `umin` `umax` |
| `(ABS a)` | integer `I` | `I`: the absolute value; `abs` of the minimum value is the minimum value (never poison) | `llvm.abs` with `is_int_min_poison` false |
| `(CTTZ a)` `(CTLZ a)` | integer `I` | `I`: the count of trailing (leading) zero bits; a zero has as many as `I` has bits (never poison) | `llvm.cttz`, `llvm.ctlz` with `is_zero_poison` false (JSON-2; the mask-to-integer move is `bitcast` of `(VECTOR N I1)` to `iN`) |

`fmin`/`fmax` are the NaN-propagating pair and `fminnum`/`fmaxnum` the
NaN-ignoring pair; the names follow LLVM's `minimum`/`minnum`. A scalar
`fsqrt` of a constant is not folded by the checker.

#### Fast-math flags

**Decided** (§14 item 12). `fadd fsub fmul fdiv frem`, `fma`, `reduce-fadd`
and `reduce-fmul` accept flag words between the name and the operands:

```
(FADD REASSOC a b)          (REDUCE-FADD REASSOC (DOUBLE 0.0) v)          (FMUL REASSOC CONTRACT a b)
```

An operand count above the form's own is read as flags: the leading words
beyond the operands. The flags are LLVM's fast-math flags, all opt-in per
instruction:

| Flag | Licence |
|---|---|
| `reassoc` | reassociate and, for a reduction, reorder the lanes: `(a+b)+c` may become `a+(b+c)` |
| `contract` | fuse a multiply and an add into a `fma` (a `fmul` and an `fadd` that both carry `contract`) |
| `arcp` | replace a division by multiplication by the reciprocal |
| `afn` | approximate a function (an intrinsic that is lowered to a library call may be replaced by a faster approximation) |
| `nsz` | treat `-0.0` and `+0.0` as interchangeable |

Each only licenses a rewrite that changes which of several results is
computed: a value is never turned into poison. `nnan` and `ninf` (a NaN or
an infinity where the flag says there is none *is* poison) and `fast`
(which implies them) are not in lIR: `fast-math flag nnan is not in lIR: a
value that violates it is poison`. Flags may appear in any order; repeating
one is `duplicate fast-math flag reassoc`; a word that is none of these is
`unknown fast-math flag foo`; a flag on a form that does not take them is
the form's usual operand-count error; a flag position that holds a form, not a word, is `expected a
fast-math flag, found X`. The measured effect is the point: a
sum of 4096 doubles in a loop takes 83 ms strict and 14 ms with
`reassoc` on its `fadd` (docs/design/vectorisation.md), because only then
may LLVM keep several partial sums in a vector register.

`add sub mul shl` take LLVM's no-wrap words `nsw` and `nuw` in the same
position, `(ADD NSW a b)` as LLVM writes `add nsw i64 %a, %b`: they are
facts (§6.13). No `exact` flag exists. `sdiv`, `udiv`, `srem`, `urem`
with a constant zero divisor are `division by constant zero`; a shift
whose amount is a constant `≥` the bit width is `shift amount n is not
less than the width w`. Both are poison or undefined in LLVM.

### 6.2 Comparisons and select

| Form | Operands | Result |
|---|---|---|
| `(ICMP P a b)`, `P ∈ eq ne slt sle sgt sge ult ule ugt uge` | same `T`: integer, `ptr`, or a vector of either | `i1`, or `(VECTOR N I1)` |
| `(FCMP P a b)`, `P ∈ oeq one olt ole ogt oge ord ueq une ult ule ugt uge uno` | same float `T` | `i1`, or `(VECTOR N I1)` |
| `(SELECT c a b)` | `c : i1` (or `(VECTOR N I1)` with vector `a`, `b` of `N` elements); `a`, `b` same first-class `T` | `T` |

### 6.3 Conversions

A conversion works on a scalar, or on a vector element by element: the
operand and the result type must then both be vectors **of the same
number of lanes** (**Decided**, §14 item 12; `sitofp: <4 x i64> and
<2 x double> must both be scalars or both be vectors of the same
length`). The class and width rules below apply to the element types.

| Form | Rule (on the elements) | Result |
|---|---|---|
| `(TRUNC T v)` | integers, `T` narrower than `v` | `T` |
| `(ZEXT T v)`, `(SEXT T v)` | integers, `T` wider | `T` |
| `(FPTRUNC T v)` | `T` float, `v` double (`float` or `(VECTOR N FLOAT)`) | `T` |
| `(FPEXT T v)` | `T` double, `v` float | `T` |
| `(FPTOSI T v)`, `(FPTOUI T v)` | `T` integer, `v` float | `T` |
| `(SITOFP T v)`, `(UITOFP T v)` | `T` float, `v` integer | `T` |
| `(PTRTOINT T v)` | `T` integer, `v : ptr` | `T` |
| `(INTTOPTR T v)` | `T` is `ptr`, `v` integer | `T` |
| `(BITCAST T v)` | `T` and `v`'s type non-aggregate, not `ptr` or a vector of `ptr`, same bit size (the sizes of whole vectors; the lane counts may differ) | `T` |
| `(FPTOSI-SAT T v)`, `(FPTOUI-SAT T v)` | `T` integer, `v` float | `T`: saturating, NaN to 0 (`llvm.fptosi.sat`, `llvm.fptoui.sat`; **Decided**, §14 item 5) |

`inttoptr` is written `(INTTOPTR PTR v)` for a scalar and `(INTTOPTR (VECTOR N
PTR) v)` for a vector of integers; the result type is the pointer type
itself, as in the former scalar form. `fptosi`/`fptoui` of a value out of
range is poison, lane by lane, as in LLVM; `fptosi-sat`/`fptoui-sat`
saturate as types.md §8.12 specifies and fibber emits them, on vectors
too (the first M3 pass kept them scalar; §14 item 12 lifts that).

### 6.4 Vectors and aggregates

| Form | Rule | Result |
|---|---|---|
| `(EXTRACTELEMENT v i)` | `v : (VECTOR N E)`, `i` integer; a constant `i` must be `< N` | `E` |
| `(INSERTELEMENT v e i)` | `v : (VECTOR N E)`, `e : E`, `i` as above | `(VECTOR N E)` |
| `(SHUFFLEVECTOR a b m)` | `a`, `b : (VECTOR N E)`; `m` a literal `(VECTOR M I32)` with elements `< 2N` | `(VECTOR M E)` |
| `(EXTRACTVALUE s k₁ ..)` | `s` an aggregate; each `k` a constant index in range: a field of a struct, an element of an array (`extractvalue: index 4 out of range for [4 x i32]`) | the field's or element's type |
| `(INSERTVALUE s v k₁ ..)` | as above, `v` of the field's or element's type | `s`'s type |

#### Horizontal reductions

**Decided** (§14 item 12). A reduction folds the lanes of one vector into one
scalar. `V` is a vector `(VECTOR N E)`.

| Form | Operands | Result |
|---|---|---|
| `(REDUCE-ADD v)` `reduce-mul` `reduce-and` `reduce-or` `reduce-xor` | `V` of integers (`(VECTOR N I1)` included: `reduce-or` is "any", `reduce-and` is "all") | `E` (`llvm.vector.reduce.add` and kin) |
| `(REDUCE-SMIN v)` `reduce-smax` `reduce-umin` `reduce-umax` | `V` of integers | `E` |
| `(REDUCE-FADD a v)` `(REDUCE-FMUL a v)` | `a : E` the start value, `V` of floats | `E`: `a + v[0] + v[1] + ..` in lane order, each step rounded, unless the form carries `reassoc` (then the order is free) |
| `(REDUCE-FMIN v)` `(REDUCE-FMAX v)` | `V` of floats | `E`: NaN if any lane is NaN (`llvm.vector.reduce.fminimum`, `fmaximum`) |
| `(REDUCE-FMINNUM v)` `(REDUCE-FMAXNUM v)` | `V` of floats | `E`: NaN lanes are ignored unless all are (`llvm.vector.reduce.fmin`, `fmax`) |

The integer reductions are exact in any order. `reduce-fadd` without `reassoc`
adds the lanes one after another, as written in a scalar loop, and is the
reference a vectorised sum is compared to.

### 6.5 Memory

| Form | Rule | Result |
|---|---|---|
| `(ALLOCA (ALIGN N)? T)`, `(ALLOCA (ALIGN N)? T n)` | `T` sized; `n` integer | `ptr` |
| `(LOAD VOLATILE? (ALIGN N)? T p)` | `p : ptr`; `T` first-class | `T` |
| `(LOAD (RANGE LO HI) (ALIGN N)? T p)` | as `load`, and the range rules of §6.13 (a fact: LLVM's `!range`) | `T` |
| `(STORE VOLATILE? (ALIGN N)? v p)` | `p : ptr`; `v` first-class | void |
| `(GETELEMENTPTR INBOUNDS? T p i₀ i₁ ..)` | `p : ptr`, `T` sized; `i₀` integer; each further index steps into the current type: into a struct by a constant `i32` field index in range, into an array by any integer, a constant one in range (`getelementptr: index 4 out of range for [4 x i32]`) except into a `(ARRAY 0 T)`, whose length is unknown | `ptr` |

- A `getelementptr` with a **vector index** (**Decided**, §14 item 12): `(GETELEMENTPTR T p idx)`
  where `p : ptr` and `idx : (VECTOR N T)`, `T` an integer type, is the one and only index; the result is `(VECTOR N PTR)`, lane `i` being
  `p` plus `idx[i]` elements of `T`: the addresses of a gather or scatter. `getelementptr: a vector
  index must be the only index`, `getelementptr: a vector index needs integer elements, found <4 x
  double>`. A vector base pointer is not in lIR.
- A `store` whose pointer operand is directly `(GLOBAL c)` of a `constant` is
  `store to constant @c`.
- **Direct accesses are typed.** When the pointer operand of `load`,
  `store`, `atomic-load`, `atomic-store`, `atomicrmw` or `cmpxchg` is
  written as `(GLOBAL g)` (a global of type `G`) or as a name bound directly to
  `(ALLOCA A)` without a count, the accessed type must be `G` or `A`:
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
  `(ALIGN N)` says otherwise. **Decided** (owner, 2026-09-28; §14 item 11): `N` is
  a power of two from 1 to 2^30 (`align must be a power of two, found
  3`); a smaller `N` than the ABI's is a promise that LLVM honours with
  slower code where the target needs it, a larger one a promise the
  emitter must keep (an `alloca` with `(ALIGN N)` keeps it). `volatile`
  is LLVM's: the access is performed exactly as written, neither
  removed, duplicated nor reordered against other volatile accesses,
  for memory-mapped registers and for a value a signal handler or a
  debugger may change; it is not atomic and orders nothing else (§6.6
  for that). The typed direct-access rule applies unchanged.

#### Masked loads and stores, gather and scatter

**Decided** (§14 item 12). `V` is a vector type `(VECTOR N E)` and a mask is `(VECTOR N I1)`. A lane whose mask
bit is 0 is not accessed at all: it can neither fault nor race, so a masked access is how a tail
of a loop reads and writes only the elements that exist.

| Form | Operands | Result |
|---|---|---|
| `(MASKED-LOAD V p mask passthru (ALIGN A)?)` | `p : ptr` to `N` contiguous elements; `mask : (VECTOR N I1)`; `passthru : V` | `V`: lane `i` is the element at `p + i` where `mask[i]` is 1, else `passthru[i]` |
| `(MASKED-STORE v p mask (ALIGN A)?)` | `v : V`, `p : ptr`, `mask` as above | void: stores lane `i` where `mask[i]` is 1 |
| `(GATHER V ptrs mask passthru (ALIGN A)?)` | `ptrs : (VECTOR N PTR)`; `mask`; `passthru : V` | `V`: lane `i` is the element at `ptrs[i]` where `mask[i]` is 1, else `passthru[i]` |
| `(SCATTER v ptrs mask (ALIGN A)?)` | `v : V`, `ptrs : (VECTOR N PTR)`, `mask` | void: stores lane `i` at `ptrs[i]` where `mask[i]` is 1 |

- `A`, a power of two from 1 to 2^30 as for `load`, is the alignment of the address `p` of a
  masked access, or of each address of a gather or scatter. Without it the ABI alignment of `V` (masked)
  or of `E` (gather, scatter) is promised, exactly as a plain `load` does; a smaller `A` is
  honoured.
- Errors: `masked-load needs a vector type, found i32`; `masked-load needs a ptr operand, found
  i64`; `masked-load: operand 2 has type <8 x i1>, expected <4 x i1>` (operands counted from 1, the
  type not being one); `masked-load: operand 3 has type <4 x i64>, expected <4 x i32>`;
  `gather: operand 1 has type <4 x i64>, expected <4 x ptr>`; `scatter needs a vector operand, found i32`;
  the same shapes for `masked-store` and `scatter`. A vector of `ptr` is not a valid `V`: `masked-load
  needs a vector of integers or floats, found <4 x ptr>` (a pointer vector is moved with plain `load`
  and `store`).
- Two lanes of a `scatter` that name one address: which value is left in memory is not specified.
- Undefined behaviour (§6.12): an unmasked lane that accesses invalid or misaligned memory. A masked-off lane
  is never accessed.

### 6.6 Atomics

```
ord   ::= UNORDERED | MONOTONIC | ACQUIRE | RELEASE | ACQ_REL | SEQ_CST
atomic ::= (ATOMIC-LOAD SINGLETHREAD? ord T p)                  ; → T
         | (ATOMIC-STORE SINGLETHREAD? ord v p)                 ; → void
         | (ATOMICRMW op SINGLETHREAD? ord p v)                 ; → T, the old value
         | (CMPXCHG WEAK? SINGLETHREAD? ord ord? p expected new) ; → (LITERAL-STRUCT T I1)
         | (FENCE SINGLETHREAD? ord)                            ; → void
op    ::= XCHG | ADD | SUB | AND | NAND | OR | XOR | MAX | MIN | UMAX | UMIN | FADD | FSUB | FMAX | FMIN
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

### 6.6a Spinning and the cache line

**Decided** (owner, 2026-10-20; docs/design/inline-lir.md, section 4): two nullary forms for code that waits on another thread's store and lays data out for
more than one core.

| Form | Type | Meaning |
|---|---|---|
| `(SPIN-HINT)` | void, not a terminator | the target's hint that this thread is spinning: `pause` on x86-64, `yield` on AArch64 (`llvm.x86.sse2.pause`, `llvm.aarch64.hint(1)`); nothing on a target that has none. It orders no memory and is not a fence; the optimiser may not remove it |
| `(CACHE-LINE)` | `i64`, a constant | the **destructive interference size** of the target in bytes: how far apart two threads' writes must be not to share a line. 128 on x86-64 and AArch64, 64 elsewhere (a conservative table, `lir.targets` `cache-line-of-arch`; the measurements that would replace a row are in docs/shootout/cache-line). It is the size of the target the module is generated for, so a library built for one CPU and run on another keeps the value it was built with |

Both take no operands (`spin-hint expects 0 operands, found 1`). Cases: `cases/lir/inline`.

### 6.6b The stack address

**Decided** (owner, 2026-10-22; docs/design/stack-check.md, section 3): one nullary form for code that must know where its stack is.

| Form | Type | Meaning |
|---|---|---|
| `(STACK-ADDR)` | `i64` | the stack pointer of the executing function, as an integer (`llvm.stacksave` cast to `i64`). It is read after the function's frame is allocated, so the frame is already below the value. It is not a constant (two functions at different depths read different values), it reads no memory the program can name, and it needs no frame pointer |

It takes no operands (`stack-addr expects 0 operands, found 1`). The calling convention word `preservemost` (LLVM `preserve_most`, number 14; `ccc` where the target has none, WebAssembly) goes where `tailcc` goes: the callee saves what it uses and keeps every other register, so a call on a cold path (the stack check's `fib.stack-slow`) makes its caller spill nothing. It is for runtime helpers called from lIR; a host cannot call one. The C backend gives `__builtin_frame_address(0)` and the JS backend the constant 2^47 (a JS runtime has no stack pointer; this is above every address a native stack has, so nothing that tests it for a low stack ever fires; a program that compares two depths gets equal ones) (neither has a use for it). Cases: `cases/lir/inline`.

### 6.7 Control flow

| Form | Rule |
|---|---|
| `(RET v)`, `(RET)` | `v` has the function's result type; `(RET)` only in a `void` function (`tc/ret_wrong.lir`, `tc/ret_void_in_i32.lir`) |
| `(BR L)` | |
| `(BR c L₁ L₂)` | `c : i1` (`tc/brcond_i32.lir`; `tc/brcond_double.lir` crashed liar) |
| `(SWITCH v L_default ((T c) L)*)` | `v` integer scalar (`switch needs an integer scalar, found double`); each case a literal of `v`'s type (`switch: case has type i64, expected i32`), no two equal (`switch: duplicate case 1`) |
| `(UNREACHABLE)` | reaching it is undefined behaviour; it follows a call that does not return |
| `(TRAP)` | void, not a terminator: `llvm.trap`, which aborts the process with the target's trap signal (`ud2`, SIGILL, on x86-64; `brk`, SIGTRAP, on AArch64) without unwinding or flushing; the block goes on to `unreachable` (**Decided**, §14 item 5). types.md §8.12's `fib.trap` writes its message first and calls `abort`, which flushes nothing either but raises SIGABRT, the signal the audit and the cases expect; `(TRAP)` is for the paths that have no message |
| `(PHI …)` | §5.4 |

### 6.8 Calls

| Form | Rule | Result |
|---|---|---|
| `(CALL (GLOBAL f) a..)` | `(GLOBAL f)` defined or declared (`call needs a function name @f, found f`); arguments as §7.1 | `f`'s result |
| `(INDIRECT-CALL p (FUNCTION cc? R (T..)) a..)` | `p : ptr` (`indirect-call needs a ptr callee, found i64`); arguments against the written type (`indirect-call: argument 1 has type i32, expected i64`); a bare result type in place of the function type, liar's form, is `indirect-call needs a function type (fn R (T..)), found i64` | `R` |
| `(TAILCALL (GLOBAL f) a..)` | terminator; §7.3 | — |
| `(INDIRECT-TAILCALL p (FUNCTION cc? R (T..)) a..)` | terminator; §7.3 | — |

### 6.9 let

`(LET ((x v)..) body..)` as §5.2 and §5.3. A `let` with no body is
`let without a body`.

### 6.9a Kernel forms (accepted by the owner on 2026-10-07; GPU-1, docs/design/gpu.md; the warp forms `laneid`, `warpsize`, `shfl-*`, `ballot` accepted on 2026-10-08, GPU-5)

Additions for a **kernel target** (lir.targets `kernel-row?`: `nvptx64-nvidia-cuda`), whose module is the kernels of a program and whose
assembly is PTX; nothing of them runs on a CPU target, and lair refuses them there before LLVM sees them.

| Form | Rule |
|---|---|
| `(DEFINE KERNELCC (NAME VOID) (..) ..)` | the entry of a kernel: LLVM's `ptx_kernel` (71). `void` result, parameters scalars or `ptr`. Nothing in lIR calls it (a driver launches it by name); a `kernelcc` function on a target that is not a kernel target is `@NAME is kernelcc: only a kernel target .. lowers a kernel`; `tailcall` never targets it. `fn-type-str` prints it `(FUNCTION KERNELCC VOID (..))` |
| `(SREG R)` | `i32`: the special register `R` of the kernel's index space, one of `tid.{x,y,z}` (the thread's index in its block), `ctaid.{x,y,z}` (the block's in the grid), `ntid.{x,y,z}` (the block's size), `nctaid.{x,y,z}` (the grid's); any other `R` is the parse error `sreg: no special register R`. Lowered to `llvm.nvvm.read.ptx.sreg.R` |
| `(BARRIER)` | void, not a terminator: every thread of the block waits until all have reached it (`bar.sync 0`; LLVM 21's `llvm.nvvm.barrier.cta.sync.aligned.all` with barrier 0: `llvm.nvvm.barrier0` is gone from LLVM 21). Not a phi operand |
| `(GLOBAL SHARED NAME T init)` | a global in the block-shared memory of a kernel target (LLVM address space 3, NVPTX `.shared`; accepted by the owner's GPU decision, implemented by GPU-2): `(GLOBAL NAME)` is its generic address (an `addrspacecast` constant), so `load` and `store` through it are the ordinary forms and the backend makes `ld.shared`/`st.shared`; its content at block start is undefined (the initialiser is for the grammar: lair gives LLVM `undef`, NVPTX refuses an initialiser there); `internal` to the module. `shared` goes with `global` only (`constant cannot be shared`, `define cannot be shared`), once (`duplicate modifier shared`); on a target that is not a kernel target lair refuses it (`@NAME is shared: only a kernel target ..`). Emitted by `gpu/shared` (types.md 2.17) |
| `(SREG laneid)` `(SREG warpsize)` (GPU-5) | `i32`: the thread's lane in its warp, and the width of the warp (`llvm.nvvm.read.ptx.sreg.laneid`, `.warpsize`; 32 on NVIDIA). Added to the list of `sreg` names; any other is still `sreg: no special register R` |
| `(SHFL-DOWN v lane)` `(SHFL-UP v lane)` `(SHFL-XOR v lane)` `(SHFL-IDX v lane)` (GPU-5, intrinsic forms of lir.intrin, class `warp`) | `v` an `i32` or a `float`, `lane` an `i32`; the result has the type of `v`: the `v` of the lane `own + lane` / `own - lane` / `own ^ lane` / `lane` of the same warp. Every lane of the warp must execute it together (the full mask: `shfl.sync .. -1`). A source out of range answers the thread's own `v` (PTX: clamp 31, `up` clamp 0); another type is `shfl-down needs an i32 or float value, found T`, a lane that is not `i32` `shfl-down needs an i32 lane, found T`. Lowered to `llvm.nvvm.shfl.sync.{down,up,bfly,idx}.{i32,f32}` with the member mask all ones; the WGSL printer maps them to `subgroupShuffle*` (webgpu.md 3.8) |
| `(BALLOT c)` (GPU-5) | `c` an `i1`, the result an `i32`: bit k is the vote of lane k, the same in every lane (`llvm.nvvm.vote.ballot.sync`, mask all ones); `ballot needs an i1 operand, found T`. WGSL: `subgroupBallot(c).x`, the lanes 0 to 31 |

On a kernel target `atomicrmw` and `cmpxchg` (§6.6) are valid as they are, on a global pointer and on a `shared` global (GPU-3): `add sub and or xor xchg max min umax umin` on `i32` and `i64`, `fadd` on `float` (PTX `atom.global.add.f32`, `atom.shared.add.f32`), and (GPU-5) `fadd fsub fmax fmin` on `float` for WGSL as a compare-and-exchange loop (webgpu.md 3.7; PTX expands `fmax`/`fmin`/`fsub` itself); the address space comes from the pointer's origin (LLVM's InferAddressSpaces), `monotonic` is `atom.relaxed`. No new form was needed for atomics; types.md 2.17 lists the builtins that emit them (`gpu/atomic-*`), and webgpu.md 3.7 says what WGSL takes of them.

`llvm.` stays reserved as a symbol prefix (§14 item 5): the two forms are instructions, as the fma family is. `(ALIGN N)` on a `load` or
`store` means `N` on a kernel target as on every other (§6.5): a front end writes the alignment it means (the kernel subset indexes
elements, never bytes, and a byte-aligned access on a GPU is four accesses of a byte, so fibber's emitter writes a scalar's own size on a
kernel target, docs/design/gpu.md 3.3). Until 2026-10-25 lair read `(ALIGN 1)` on a scalar as the scalar's natural alignment there
(native.lower.memory `kernel-align`); **Decided** (owner, 2026-10-25: the text is the contract, and `align 1` means 1 in LLVM), that
reading is gone.

### 6.10 What is not in lIR

liar's ADR 021 "safe lIR" is removed: there is no `own`, `ref`,
`refmut`, `rc` type, no `alloc own`, `borrow`, `drop`, `move`,
`rc-alloc`, `rc-clone`, `rc-drop`, `rc-count`, `rc-ptr`,
`heap-struct`, `free` instruction, no `array-*`, `ptr-array-*` or
`heap-array*` instruction. Using one of these names is `unknown
instruction`. ADR 021 was only ever Proposed, and it failed in five
ways (the audit is `lir-audit/borrow/`, re-established as
`cases/lir/audit/borrow-*.lir`):

1. its checker (`lir-core/src/borrow.rs`) was called from one test and
   never from `lair` or its code generator, so nothing it rejected was
   ever refused;
2. it walked blocks in source order with one map and no join at a CFG
   merge, so it was not flow-sensitive (`drop_in_branch_use_other`,
   `leak_in_branch` could not be decided);
3. its forms lowered to nothing: `alloc own` became an `alloca` (an
   "owned heap value" on the stack), `drop` a no-op, `move` and `borrow`
   the identity (`borrow/uaf.lir` printed `after drop: 42`);
4. `rc` and `ptr` parameters were exempt ("RC has its own tracking",
   which did not exist), so `borrow/rc.lir` read freed memory after a
   double `rc-drop`;
5. raw `load` and `store` on owned values were allowed, so an alias of an
   owned pointer outlived its drop (`use_after_drop_via_alias`); the safe
   and unsafe worlds were never separated by type, and a front end that
   emitted plain `ptr` was asked nothing.

Ownership is in lIR again, differently (§6.14; **Decided**, owner,
2026-10-25: lIR is the language, lair the only compiler, and a front end
that emits counts emits them as lIR's forms, which lair verifies). The
answers to the five failures: the verifier runs inside `lir.check` on
every path, is a dataflow over the CFG with a merge rule, every form
lowers to the runtime call the front end wrote before, counted
references are the primary case, and the managed type `(PTR (ADDRSPACE 1))` is
distinct from `ptr`, with a named, counted escape.

### 6.11 Constants in instructions

Where a rule says "constant" it means a literal written in place
(§3), not a name bound to one.

### 6.12 Undefined behaviour that remains

As in LLVM: memory accesses through invalid, misaligned or freed
pointers; data races on non-atomic accesses; `sdiv`/`srem` by a zero or
of the minimum by -1 computed at run time; shifts by a run-time amount
`≥` the width (poison); out-of-range `fptosi`/`fptoui` (poison); a
run-time out-of-range vector index (poison); an unmasked lane of a masked
load, store, gather or scatter that accesses invalid memory (§6.5); `getelementptr inbounds`
outside its object (poison); a false fact (§6.13: an operation that wraps
although it carries `nsw` or `nuw`, which is poison; an `assume` whose
operand is false; a load whose value is outside its `range`); reaching `unreachable`; calling a function
through a pointer with the wrong function type; a `musttail` callee
that reads the caller's `alloca`s. lIR's checker cannot see these;
fibber's compiler emits the checks types.md §8.12 requires.

### 6.13 Facts

**Decided** (owner, 2026-10-24: "broadly down for anything that lets lir
use more of llvm"; design and measurements in docs/design/lir-facts.md).
A fact is something the code that emits lIR knows about a value and LLVM
cannot see for itself. lIR is LLVM IR written as S-expressions, so each
fact is LLVM's own form in LLVM's spelling and position, nothing of
fibber's: the no-wrap words on the instruction, `llvm.assume`, and the
`!range` metadata of a load. A claim in the front end's terms (an index in
the bounds of some array, a count that does not change) is the front
end's to prove and to emit as these forms; lIR has no form for it.

| Form | Meaning | LLVM |
|---|---|---|
| `(ADD NSW a b)`, `(ADD NUW a b)`, `(ADD NUW NSW a b)`; `sub`, `mul`, `shl` alike | the operation does not wrap as signed (`nsw`) or as unsigned (`nuw`); for `shl`, shifting the result back gives `a` again (arithmetically for `nsw`, logically for `nuw`) | the flags on the instruction (`add nuw nsw i64`) |
| `(ASSUME c)` | `c : i1` is true here; void, not a terminator | `call void @llvm.assume(i1 c)` |
| `(LOAD (RANGE LO HI) (ALIGN N)? T p)` | the loaded integer is in `[LO, HI)`, read signed | `!range !{T LO, T HI}` on the load |

**A fact only removes behaviours.** A program whose facts are true
computes the same results whether a backend uses a fact or ignores it, so
any backend may drop any fact: the C, JavaScript and WGSL backends do
(lir.facts `strip-module`); lair gives them to LLVM. A false fact is
undefined behaviour (§6.12), unlike a fast-math word, which only chooses
among results.

The words are written as fast-math words are (§6.1): between the name and
the operands, in any order, each at most once. The operand rules are the
unflagged form's (same integer `T`, scalar or vector; a constant shift
amount below the width).

The checker rejects a malformed or misplaced fact and one whose literal
operands already make it false; it cannot establish a fact about run-time
values:

| Error | Form |
|---|---|
| a repeated word | `duplicate flag nsw` |
| another word on `add sub mul shl` | `unknown integer flag exact` (a fast-math word too: `unknown integer flag reassoc`); `nsw` on a float form is `unknown fast-math flag nsw`; on any other integer form it is the form's operand-count error |
| two literal operands that break the flag | `add nsw wraps on its literal operands` (`(ADD NSW (I64 9223372036854775807) (I64 1))`), `sub nuw wraps ..`, `shl nsw wraps ..` |
| `assume` | `assume needs an i1 operand, found i32`; `assume of a literal false`; as a value: `void value used as an operand` |
| `range` | `range must be (range LO HI) with two integers`; `range needs an integer load type, found double` (a scalar integer only); `range needs LO < HI, found 5 5`; `range bound 129 is out of range for i8` (`-2^(K-1) <= LO < HI <= 2^(K-1)`, signed; `HI = 2^(K-1)` is a range open above, which LLVM spells with the minimum as its end); `range covers every value of i8` (LLVM's `!range` may not be the full set); `range on a volatile load` |

**The check mode.** `FIB_LIR_FACTS` chooses what lair does with the facts:
`use` (the default) gives them to LLVM; `ignore` drops them all before
lowering (the reference semantics); `trap` lowers each fact to a test of
it: the operation is computed without its flags and its overflow tested
(`llvm.sadd.with.overflow` and kin; for `shl`, the shift back), `assume`
tests its operand, a range load compares the loaded value with both
bounds. A false fact then calls `llvm.trap` after writing to standard
error a message naming the fact and its place:
`lIR fact violated: (add nsw) in @main, block entry, at 13:14` (the
position only when the module was read from text). Any other value is
refused: `FIB_LIR_FACTS must be use, trap or ignore, found X`. The gate
runs the ownership and stdlib cases under `trap` (`make cases-trap`), so
a fact a front end emits falsely is a trap with its message there, not a
miscompile; the check covers what the suite executes, nothing more. The
liro object cache is keyed on the mode.

Not in lIR: `exact` on divisions and shifts, TBAA, `noalias` scopes,
`!invariant.load` (false under fibber's in-place reuse of a unique
object's memory, docs/design/lir-facts.md 4). Each would be added the
same way, as LLVM's own form.

### 6.14 Ownership

**Decided** (owner, 2026-10-25; docs/design/lir-language.md section 5,
with the decisions D4 to D10 as that section recommends; the design input
is `/tank/data/fibber-scratch/lir-language/ownership-in-lir.md`). The
borrow and count constructs of fibber are lIR forms, and a verifier in
lair rejects lIR that sidesteps them, whoever wrote it: fibber infers
(borrowed or owned, stack or heap, tail calls) and lair verifies, as javac
infers and the JVM verifier checks. The shape is Swift SIL's ownership
SSA: ownership is a property of SSA values, set by the defining
instruction or the parameter's kind, and it is erased before LLVM, so
after erasure the lIR is exactly what a front end wrote before this
section existed (§6.14.7). Rule 2 of lir-language.md section 3 is kept
where LLVM has a word: the type is LLVM's address space, the casts are
LLVM's `addrspacecast`; where LLVM has none, the words are SIL's.

**Why SIL's words and not MIR's** (owner, 2026-10-25: "I think rust has
an IR that we can borrow from"). Rust's MIR is the other IR with a
borrow checker and optimisation before LLVM, and three of its four
devices are here under other names: `Drop` is `destroy-value`, borrow
scopes come from liveness (NLL) rather than from written scopes, and
`StorageLive`/`StorageDead` of a stack slot are `alloc-stack` and its
`destroy-value`. The fourth, `move x` and `copy x` written on every
operand, is thicker than what an SSA value needs: MIR's operands are
places that may be read again, so each use must say whether it ends the
place's value; here an operand is an SSA value whose kind and position
already say it. A use at an owned position (a call's `owned` parameter,
`ret`, `store init`) is MIR's `move`, with no count change and no word;
MIR's `copy x` is the one instruction `copy-value`, an instruction rather
than an operand word because the copy, a retain, is a new value that may
be used more than once. What MIR has and this section refuses is drop
elaboration: rule 3 demands that predecessors agree at a merge, so there
are no drop flags, which is thinner than MIR and is what fibber's join
rule already produces. lIR keeps LLVM's shape, values and `alloca`, not
MIR's places.

**Where it sits.** The owner's constraint (2026-10-25: lIR is "designed
to be as thin as possible to make optimizing it before final
transformation to LLVM IR"): the forms stay in the tree through lIR's own
passes and are erased last. Three functions on the tree, in this order
and each on its own: `lir.own/verify-module` (the rules of §6.14.5, run
inside `lir.check`), `lir.own.opt/optimise-module` (lIR-to-lIR rewrites
on the explicit forms: a `copy-value`/`destroy-value` pair cancelling, an
owned argument made guaranteed, an `is-unique` proven away; today the
identity, the slot where they will live), then `lir.own.erase/erase-module`
(§6.14.7), called by `native.lower` and by the C, JS and WGSL printers
immediately before they read the tree, and by nothing earlier.

#### 6.14.1 The managed type

`(PTR (ADDRSPACE 1))` is a counted reference that may be null: LLVM's `ptr
addrspace(1)`, address space 1 being non-integral (`ni:1`), so a value
of it has no integer. It is first-class: a value, parameter, result,
`phi`, `select`, `load`, `store`, array element or `defstruct` field; not
a vector element. It is a different type from `ptr`: no instruction
accepts one where the other is expected (`call to @f: argument 1 has
type (ptr 1), expected ptr`). The null of it is written `((PTR (ADDRSPACE 1)) NULL)`.
`ptrtoint`, `inttoptr` and `bitcast` never take it (`ptrtoint cannot
convert (ptr 1): a managed reference has no integer value`, `inttoptr
needs a ptr result type, found (ptr 1)`, `bitcast cannot convert (ptr
1)`); `icmp` compares two of them. An atomic operation on it is `atomic
operation on (ptr 1)`: the atoms of the runtime read and write `ptr`
inside `unsafe` (§6.14.4). A `kernelcc` function never takes one (`@k
is kernelcc: a kernel takes scalars and ptr, found (ptr 1)`).

A `defstruct` says which of its fields are counted by giving them this
type: `(DEFSTRUCT P ((STRUCT fib.hdr) I64 (PTR (ADDRSPACE 1))))`. The text says what
is counted, which types.md §8.1 had the fibber type decide.

#### 6.14.2 Kinds

Every value of type `(PTR (ADDRSPACE 1))` has a **kind**, fixed by what defines it:

| Kind | Meaning | Defined by |
|---|---|---|
| `owned` | carries one count and must be consumed exactly once on every path | `alloc-object`, `copy-value`, `load copy`, `load take`, a call whose result is `(PTR (ADDRSPACE 1))`, an `owned` parameter, a `phi` whose incoming values are owned or none |
| `guaranteed` | borrowed: valid while its **roots** are live, never consumed | a parameter without `owned` (its root is itself, live for the whole function), `load borrow` (roots: the base's), a `phi` of guaranteed and none values (roots: the union), a `select` (roots: the union; an owned arm is itself a root) |
| `none` | needs no count work: usable as owned and as guaranteed any number of times | `((PTR (ADDRSPACE 1)) NULL)`; `(ADDRSPACECAST (PTR (ADDRSPACE 1)) p)` in an `unsafe` function (the author vouches for it) |
| `stack` | a scope-local object of this frame, ended by exactly one `dealloc-stack` | `alloc-stack` |
| `interior` | the address of a field or element of an object, usable only as the address operand of a load, store, atomic or further `getelementptr` | `getelementptr` whose base is `(PTR (ADDRSPACE 1))` (roots: the base's; an owned or stack base is itself the root) |

The **roots** of a value are the owned, stack or guaranteed-parameter
values it was derived from. A parameter is written `((PTR (ADDRSPACE 1)) OWNED p)`
or `((PTR (ADDRSPACE 1)) p)`; in a `declare` and in a function type the kind wraps the
type, `(OWNED (PTR (ADDRSPACE 1)))` against `(PTR (ADDRSPACE 1))` (`owned needs (ptr 1), found
i64`; `owned is a parameter kind, not a type` anywhere but a parameter).
A result of type `(PTR (ADDRSPACE 1))` is always owned. The kinds are part of a
function's type: a `tailcall` under `ccc` needs them identical (§7.3), an
`indirect-call` is checked against the kinds it writes, and a `(LET ((y
x)) ..)` of a managed `x` names the same value.

#### 6.14.3 Forms

| Form | Rule | Result |
|---|---|---|
| `(ALLOC-OBJECT (STRUCT T) tid)` | `T` a struct whose first field is the header struct (§6.14.6); `tid : i32`, what the runtime's `alloc` receives | `(PTR (ADDRSPACE 1))`, owned, and **fresh** (§6.14.5 rule 8) |
| `(ALLOC-OBJECT (STRUCT T) tid n)` | as above; `T`'s last field is `(ARRAY 0 E)` and `n : i64` the number of its elements | `(PTR (ADDRSPACE 1))`, owned, fresh |
| `(ALLOC-STACK (STRUCT T) tid)` | as `alloc-object`, in this frame | `(PTR (ADDRSPACE 1))`, stack |
| `(DEALLOC-STACK s)` | ends the stack value `s` (SIL's `dealloc_stack`, MIR's `StorageDead`): a form of its own, so that erasure needs no kinds | void |
| `(COPY-VALUE v)` | `v : (PTR (ADDRSPACE 1))`, any kind but interior | `(PTR (ADDRSPACE 1))`, owned |
| `(DESTROY-VALUE v)` | consumes `v`: owned or none | void |
| `(IS-UNIQUE v)` | `v : (PTR (ADDRSPACE 1))`; on the true edge of a `br` on it, `v` has uniqueness evidence | `i1` |
| `(LOAD COPY (PTR (ADDRSPACE 1)) a)` | `a` an interior address, or any `ptr` in an `unsafe` function | `(PTR (ADDRSPACE 1))`, owned |
| `(LOAD BORROW (PTR (ADDRSPACE 1)) a)` | `a` an interior address | `(PTR (ADDRSPACE 1))`, guaranteed, rooted as `a` |
| `(LOAD TAKE (PTR (ADDRSPACE 1)) a)` | as `load copy`; the slot is left null; `a`'s object must be mutable here (rule 8) | `(PTR (ADDRSPACE 1))`, owned |
| `(STORE INIT v a)` | `v : (PTR (ADDRSPACE 1))` consumed; `a` as for `load copy`; the slot held nothing counted | void |
| `(STORE ASSIGN v a)` | as `store init`, and the slot's old value is released; `a`'s object must be mutable here | void |
| `(STORE BORROW v a)` | `v` guaranteed or none, not consumed; `a` an interior address of a stack object (rule 6) | void |
| `(ADDRSPACECAST PTR v)`, `(ADDRSPACECAST (PTR (ADDRSPACE 1)) p)` | in an `unsafe` function only (rule 9); `v : (PTR (ADDRSPACE 1))` is used, not consumed, and the `ptr` made of it is untracked; the `(PTR (ADDRSPACE 1))` made of `p : ptr` is `none` | `ptr` or `(PTR (ADDRSPACE 1))` |

A plain `load` of `(PTR (ADDRSPACE 1))`, or a plain `store` of a `(PTR (ADDRSPACE 1))` value, is
refused everywhere (rule 9: `a managed value is loaded with copy, borrow
or take`, `a managed value is stored with init, assign or borrow`), which
is what keeps an alias from outliving a count. The words after `load`
and `store` are read by their count: `(LOAD COPY (PTR (ADDRSPACE 1)) a)` has three
operands and a plain load two, `(STORE INIT v a)` three and a plain store
two, so a local named `copy` or `init` stays a name. `(GETELEMENTPTR T p
i..)` with `p : (ptr 1)` is `(PTR (ADDRSPACE 1))` (LLVM keeps the address space) of
kind interior; outside `unsafe` it may not address the header (rule 9).
The type operand of the managed loads is `(PTR (ADDRSPACE 1))` (`load copy needs (ptr
1), found ptr`).

#### 6.14.4 The fence: `unsafe`

`(DEFINE UNSAFE ..)` marks a function in which the managed world meets
raw memory: `addrspacecast` between `ptr` and `(PTR (ADDRSPACE 1))`, the managed
loads and stores through a plain `ptr`, and the address of the header are
allowed there and nowhere else. Ownership is still verified inside an
`unsafe` function: only those three things are unlocked. `unsafe` is a
`define` modifier (`declare cannot be unsafe`, `global cannot be unsafe:
unsafe is for a function`), once (`duplicate modifier unsafe`), and the
gate counts the `unsafe` functions of the runtime against a ratchet
(lir-language.md stage 2, step 5). `ptrtoint` of `(PTR (ADDRSPACE 1))` is refused in
`unsafe` too: it is a type error, not a fence.

#### 6.14.5 The verifier

`lir.own` runs inside `lir.check` (§10) after the type rules, on every
path lair compiles (AOT, the JIT, live mode, macros, the C, JS and WGSL
backends): a check that can be skipped is no check (lir-language.md
section 3, rule 3). It is one forward dataflow per function over the
CFG `lir.check` built, keeping for each owned and stack value whether it
is live or consumed at each point, and for each fresh value whether its
uniqueness evidence holds. A function with no `(PTR (ADDRSPACE 1))` value and no
ownership form costs one walk; a call whose result is `(PTR (ADDRSPACE 1))`
defines such a value, so a function whose only managed value is a call's
result is verified. The rules, each with its diagnostic
(`FILE:LINE:COL: error: ownership rule N: ..` at the form at fault):

1. **Linearity.** An owned value is consumed exactly once on every path
   from its definition to a `ret` or a tail call. A tail call's result is
   the function's result, handed to the caller as `ret`'s operand is: it is
   not a value of the frame, and nothing in the frame consumes it. The
   consuming uses are
   `destroy-value`, an `owned` argument position of a call, `ret`, `store
   init` and `store assign`, and being the incoming value of a `phi` (the
   edge consumes it). A second consume on one path is `ownership rule 1:
   owned x is consumed twice`; a path reaching `ret` with it live is
   `ownership rule 1: owned x is leaked at ret`, or on an edge into a
   block its definition does not dominate `ownership rule 1: owned x is
   leaked on the edge from A to B`. `unreachable` discharges everything,
   the process having stopped. A stack value is linear too, its one
   consume being `dealloc-stack` (`stack s is ended twice`, `stack s is
   not ended at ret`).
2. **No use after consume.** No use of a value is reachable from a
   consuming use of it without a new definition: `ownership rule 2: x
   used after it was consumed`. Within one instruction the consuming
   positions take effect before the others, so `(CALL (GLOBAL f) x x)` with the
   first parameter owned is this error.
3. **Merge agreement.** At every block entry the predecessors agree on
   live or consumed for every value, so there are no drop flags:
   `ownership rule 3: x is live from A and consumed from B at block L`. A
   `phi` of `(PTR (ADDRSPACE 1))` whose incoming values are owned on one edge and
   guaranteed on another is `ownership rule 3: phi in L mixes owned (from
   A) and guaranteed (from B) values`; a front end retains on the
   guaranteed edge instead.
4. **Borrow containment.** Every use of a guaranteed or interior value
   happens while all of its roots are live (the scope is inferred from
   liveness, NLL-style, with no `begin-borrow` or `end-borrow`): a use
   reachable from a consume of a root is `ownership rule 4: x used after
   its root r was consumed`.
5. **Guaranteed values never escape.** A guaranteed value is never
   consumed, never returned, never stored except by `store borrow`, and
   never passed at an `owned` position: `ownership rule 5: guaranteed x
   consumed by destroy-value` (`.. returned`, `.. stored`, `.. passed at
   owned position 2 of call to @f`). `copy-value` first is the way out,
   which is fibber's `consume(Borrowed) = retain`.
6. **Stack values do not escape.** A stack value may be used as
   guaranteed; it is never returned, stored except by `store borrow`,
   passed at an owned position, copied (`ownership rule 6: stack s
   copied`, `.. returned`, `.. stored`, `.. passed at owned position`),
   a `phi` operand (`stack s is a phi operand`) or the operand of
   `destroy-value` (`stack s consumed by destroy-value: a stack value is
   ended by dealloc-stack`; and `dealloc-stack of owned x` the other way
   round). `(STORE BORROW g a)`
   into a stack object `s` is accepted only if `s` is ended before every
   consume of `g`'s roots on every path: `ownership rule 6: stack s still
   holds a borrow of r, which is consumed here`.
7. **Tail calls.** At a `tailcall` or `indirect-tailcall`, after its
   owned arguments are consumed, no owned or stack value of the frame is
   live (`ownership rule 7: x is live at the tail call`), and every
   guaranteed argument has only guaranteed parameters and none as roots
   (`ownership rule 7: argument 1 depends on the frame through x`;
   types.md §6.10's frame independence).
8. **Mutation needs evidence.** A `store` of any kind, an atomic write,
   or a `load take` through an interior address of a managed object
   needs the object to be **mutable here**: it is a stack object, its
   struct is `(DEFSTRUCT MUTABLE ..)` (cells, atoms, task frames), or it
   has uniqueness evidence: it is fresh from `alloc-object`, or the
   block is reached through the true edge of a `br` whose condition is
   `(IS-UNIQUE x)`, and the evidence has not been lost. Evidence is lost
   at the first `copy-value` of the value, at a call it is an argument of
   (the callee may have copied it), at a `store borrow` of it and at a
   merge with a path without it (D8: evidence is checked from the start,
   and the gate's ratchet for it comes after the counts'). `ownership
   rule 8: store into x without uniqueness evidence`, `load take from x
   without uniqueness evidence`.
9. **The fence.** `addrspacecast` outside an `unsafe` function
   (`ownership rule 9: addrspacecast outside unsafe`), the managed loads
   and stores through a plain `ptr` outside one (`load copy through a
   ptr outside unsafe`), the header addressed outside one (`the header of
   x is addressed outside unsafe`: a `getelementptr` whose type is the
   header struct or whose second index is 0 into a struct whose first
   field is it), a plain `load` or `store` of a managed value (§6.14.3),
   and an interior address anywhere but as an address (`interior address
   x returned`, `.. stored`, `.. passed to @f`, `.. consumed`).
10. **Signatures.** A call is checked against the callee's kinds: an
    owned position consumes, a guaranteed one borrows, a `(PTR (ADDRSPACE 1))` result
    is owned. The one place a module can write a signature that is not
    the callee's is an indirect call whose callee is written `(GLOBAL f)`:
    `ownership rule 10: indirect-call through @f writes (fn void ((ptr
    1))), but @f is (fn void ((owned (ptr 1))))`. The functions of the
    `ownership` form must have the types §6.14.6 gives.

The verifier does not reclaim cycles among `mutable` objects, prove the
bounds of an interior address, check sharing between threads, or prove
that the counts are the minimal ones: those stay fibber's (ownership.md
§6, types.md §6.12).

**The mode.** `FIB_LIR_OWN` chooses what lair does with a violation:
`report` (the default while fibber's emitter is being moved onto the
forms, lir-language.md stage 2 steps 3 and 4) counts the violations per
rule and prints one summary line to standard error, `lir.own: N
violations in M functions (rule 1: A, rule 4: B)`, and accepts the
module; `verify` makes each violation an error of the module, as any
other diagnostic of §10; `off` runs nothing. Any other value is refused:
`FIB_LIR_OWN must be verify, report or off, found X`. The errors of
§6.14.1 to §6.14.4 and §6.14.6 (types, the grammar, the binding) are
errors in every mode. `report` is temporary: step 5 of the stage removes
it, and the planted mutants of `cases/lir/own` fail the case suite with
a verifier whose rule is disabled.

#### 6.14.6 The binding: what the forms lower to

A module that uses `(PTR (ADDRSPACE 1))` or any form of §6.14.3 names, once, the
runtime functions the forms lower to, so that lair hard-codes no runtime
name (`the module uses (ptr 1) but has no ownership form`, `a module has
at most one ownership form`):

```
(OWNERSHIP (RETAIN (GLOBAL fib.retain)) (RELEASE (GLOBAL fib.release)) (UNIQUE (GLOBAL fib.unique?)) (ALLOC (GLOBAL fib.alloc))
           (STACK-INIT (GLOBAL fib.stack-init)) (STACK-END (GLOBAL fib.stack-end)) (HEADER (STRUCT fib.hdr)))
```

Each field is required (`ownership form needs (retain @f)`, `ownership:
unknown field x`, `ownership: duplicate retain`). A program joined from
imports (§4.7) has one: the module that carries it (the runtime) exports
it in its interface, a module that imports it uses the forms without
writing one (`cases/lir/own/accept-import-binding.lir`), and a second
form anywhere in the program is `ownership: FILE also has an ownership
form`. The functions are
`unsafe` defines or declares (`ownership: @f must be an unsafe define or a
declare`) with these types (`ownership: retain must be (fn void (ptr)),
@f is (fn void (i64))`): `retain`, `release` and `stack-end` `(FUNCTION VOID
(PTR))`, `unique` `(FUNCTION I1 (PTR))`, `alloc` `(FUNCTION PTR (I64 I32))`,
`stack-init` `(FUNCTION VOID (PTR I32))`. They take `ptr`, since they are
called after erasure. `header` names a defined struct: the first field of
every object struct, addressable only in `unsafe` (rule 9).

Not yet in lIR: a `global` or `constant` in address space 1, whose
address would be a `none` value (the immortal literal objects, vtables
and `def` values of types.md §8.2). Until it is specified, an immortal
object reaches a managed position through `addrspacecast` in an `unsafe`
function.

#### 6.14.7 Erasure

After the verifier, and before LLVM, the C, JS or WGSL printer, lair
erases the section: the result is a module of §2 to §6.13 that checks
under §10 and is what the front end wrote before this section. `(GLOBAL RETAIN)`
and the others are the module's `ownership` form; `S` is the size of
`(STRUCT T)` by LLVM's layout for the target (each field at the natural
alignment of its type, the whole rounded up to the largest, pointers the
target's width, as emit.layout computes it); fresh names are ones the
function does not bind.

| Form | Erased to |
|---|---|
| `(PTR (ADDRSPACE 1))`, `(OWNED (PTR (ADDRSPACE 1)))`, `((PTR (ADDRSPACE 1)) NULL)` | `ptr`, `ptr`, `(PTR NULL)`, everywhere a type or literal is written |
| `(x (ALLOC-OBJECT (STRUCT T) tid))` | `(x (CALL (GLOBAL ALLOC) (I64 S) tid))` |
| `(x (ALLOC-OBJECT (STRUCT T) tid n))` | `(x (CALL (GLOBAL ALLOC) (ADD (I64 S) (MUL (I64 E) n)) tid))`, `E` the size of the element of `T`'s last field |
| `(x (ALLOC-STACK (STRUCT T) tid))` | `(x (ALLOCA (STRUCT T)))` in the entry block, before everything else there, and `(CALL (GLOBAL STACK-INIT) x tid)` where the form stood |
| `(x (COPY-VALUE v))` | `(CALL (GLOBAL RETAIN) v)` where the binding stood, and `v` for `x` after it |
| `(COPY-VALUE v)` as a statement or an operand | `(CALL (GLOBAL RETAIN) v)`, or `(LET ((t v)) (CALL (GLOBAL RETAIN) t) t)` |
| `(DESTROY-VALUE v)` | `(CALL (GLOBAL RELEASE) v)` |
| `(DEALLOC-STACK s)` | `(CALL (GLOBAL STACK-END) s)` |
| `(IS-UNIQUE v)` | `(CALL (GLOBAL UNIQUE) v)` |
| `(x (LOAD COPY (PTR (ADDRSPACE 1)) a))` | `(x (LOAD PTR a))` then `(CALL (GLOBAL RETAIN) x)`; as an operand `(LET ((t (LOAD PTR a))) (CALL (GLOBAL RETAIN) t) t)` |
| `(LOAD BORROW (PTR (ADDRSPACE 1)) a)` | `(LOAD PTR a)` |
| `(x (LOAD TAKE (PTR (ADDRSPACE 1)) a))` | `(x (LOAD PTR a))` then `(STORE (PTR NULL) a)`; as an operand `(LET ((t (LOAD PTR a))) (STORE (PTR NULL) a) t)` |
| `(STORE INIT v a)`, `(STORE BORROW v a)` | `(STORE v a)` |
| `(STORE ASSIGN v a)` | `(LET ((o (LOAD PTR a))) (STORE v a) (CALL (GLOBAL RELEASE) o))` |
| `(ADDRSPACECAST T v)` | `v` |
| `unsafe`, `mutable`, the `ownership` form | dropped |

An `alloc-object`, `alloc-stack`, `copy-value`, `load copy` or `load
take` that is not the value of a binding is bound to a fresh name first.
Erasure is a rewrite of the tree (`lir.own.erase/erase-module`), the last
pass before LLVM or a printer; the liro cache keys on the text before
it, with the forms in it.

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
must define `main` as `(DEFINE (main I32) () ..)` or `(DEFINE (main
I32) ((I32 argc) (PTR argv)) ..)`, with the C convention: `main must be
(main i32) with no parameters or (i32 ptr)`, or `no main function`,
and exported: `main must not be private or internal` (§4.3). The
process exit status is `main`'s result, as C gives it (the low 8
bits on POSIX). Output buffered by the C library is flushed at exit on
both paths. A module built into a library (`lair build --library`, the
`library` of lair's build options; **Decided**, owner, 2026-10-25, §14
item 20) is not run, so the rule does not apply to it: its entries are
what it exports (§4.3), and on wasm it gets wasi-libc's reactor start
and no entry point. Whether a build is a library is a build option of
lair, never read from the environment (until 2026-10-25 `FIB_EXPORTS`
turned the rule off).

### 7.3 Tail calls

`(TAILCALL (GLOBAL g) a..)` lowers to `musttail call` of `g` followed by `ret`
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
   is what every 64-bit target returns in registers; `(LITERAL-STRUCT PTR PTR)`,
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
(GLOBAL mod* NAME T init)     ; mutable
(CONSTANT mod* NAME T init)   ; read-only; a store to it is undefined behaviour
(DECLARE-GLOBAL HIDDEN? NAME T)   ; defined elsewhere (§4.4)
init ::= (I1 INT) | (I8 INT) | (I16 INT) | (I32 INT) | (I64 INT) | (FLOAT FLT) | (DOUBLE FLT) | (PTR NULL) | (GLOBAL NAME)
       | (STRING STR) | ((VECTOR N T) ..) | (LITERAL-STRUCT init*) | ((STRUCT NAME) init*)
       | ((ARRAY N T) init*) | (ZEROINITIALIZER T)
```

- `(GLOBAL NAME)` in an expression is the global's address, a `ptr`; its value
  is read with `(LOAD T (GLOBAL NAME))`.
- The initialiser has type `T` (`initializer of @g has type i64,
  expected i32`). `(STRING "…")` initialises a `ptr` global with the
  address of a private constant holding the bytes and a NUL: `(LOAD PTR
  (GLOBAL g))` gives a pointer to the text. (liar stored the bytes themselves
  in the slot, so the load returned the text read as an address and
  crashed: `t/gstr.lir`.) `(GLOBAL f)` initialises a `ptr` with a function's or
  global's address. Struct, array and vector initialisers nest;
  `(ZEROINITIALIZER T)` zeroes a global of any type, a large buffer
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
| facts | §6.13: the words, `assume`'s operand, the range of a load, literal operands that break a fact |
| calls | §7: arity, argument types, variadic promotion, tail-call rules, `main` |
| target | §4.5: at most one `target` form and its two strings |
| ownership | §6.14: the managed type and the forms (types), the `ownership` form (module), then `lir.own`, the verifier's ten rules, in the mode `FIB_LIR_OWN` names |

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
lair::aot::build_executable(&module, name, path, &opts) -> Result<()>   // `cc` with -lm -lpthread, opts.lib_dirs, opts.libs
lair::aot::library_dir(dir) -> Result<PathBuf, String>                  // a lib_dirs entry as the link uses it
```

`Options { opt_level, libs, lib_dirs }`. The executable is linked
against libm and libpthread always, then against each name of `libs`
(`-lNAME`, after everything that may need it). Each directory of
`lib_dirs` is a `-L` directory of the linker **and an rpath**: the
executable finds its libraries when it runs, from any directory and
without `LD_LIBRARY_PATH`. The rpath is the directory's absolute,
canonical path (`library_dir`: a relative directory is relative to the
process that links, and a symbolic link is followed), so a directory
that does not exist, is not a directory, or whose canonical name holds
`:` (which an rpath reads as a separator) or `$` (a variable, such as
`$ORIGIN`) is refused as `-L DIR: REASON` before anything is compiled.
A library or symbol the linker cannot find is `linker failed:` and the
linker's own output. `lib_dirs` is used by `build_executable` only:
`emit` writes no executable. Tested by `crates/lair/tests/link.rs`: a
shared library built with `cc`, the executable run without
`LD_LIBRARY_PATH` and from another directory, the same executable
failing once the library is moved.

The checker and the lowering recurse over expressions, at most 512
deep (§1); the checker is tested at depth 504 on a 2 MB thread.

Both paths share one lowering and the same checks; both set the host
target's triple and data layout on the module.

## 12. The `lair` tool

```
lair check FILE.lir              ; parse and check only
lair run FILE.lir [ARGS..]       ; JIT-compile and run main; exit with its status
lair build FILE.lir -o OUT [-O n] [--emit obj|asm|llvm] [--library] [-L DIR].. [-l LIB]..   ; --library: a library, no main rule (§7.2)
lair emit-llvm FILE.lir          ; print the verified LLVM IR
lair expand FILE.lir             ; print the module with its macros expanded, as lIR text (§4.6): the input of a backend with no compile-time runtime
lair cases DIR.. [-j N]          ; run a case suite (§13), N cases at a time (default FIB_LAIR_WORKERS or 4)
lair words                       ; the words of lIR (§2): each word's text and its canonical name, one per line
lair --old-spelling ..           ; admit the old spelling beside the new (§2, transitional); --new-spelling refuses it, the default
lair respell FILE.lir [--in-place|--check] [--macro-args keep|respell]   ; the text in the new spelling (§2), comments and layout kept; --check: exit 1 unless it is already
```

`lair respell` rewrites a text into the new spelling token by token: it reads the old spelling whatever the mode, walks the forms with
the shape of the grammar, and replaces only the words in a word position (a head, a modifier, a type, a flag, an ordering, a literal
word), the sigils and the bracket types; every other byte is copied, so comments, case headers and layout stay, a name stays as it was
written (a local `add`, a label `entry`), and an `;; error:` header that quotes `@f`, `%struct.s` or `(ptr 1)` is respelled with the
text. The result parses to the same
tree as the input and is its own respelling (`compiler/tests/lir/respell.sh` holds both over every lIR file of the tree); what a macro
builds at macro time (`(lir.sexp.symbol "add")`) is no text and is not seen.

**Macro uses.** The tool cannot know the grammar of a macro, so the arguments of a macro use may be the words of a language built on lIR
(`let`, `and`, `or`, `fn`, `{..}`, `@x`), which §2 leaves in lower case, and respelling them would make them lIR's `LET`, `AND`, `FUNCTION`,
`LITERAL-STRUCT` and `GLOBAL`. By default (`--macro-args keep`) a form that is a macro use is therefore left exactly as written, whole, at
the top level and inside a function or a macro's body alike; the lIR around it is respelled. A form is a macro use when its head names a
macro of the module, or is no word of lIR (so is no item, instruction, type or literal). The macros of the module are the names of its own
`define-macro`s (a splice `(begin ..)` included, §4.6), of the macros of each library it `require-macros` (and of the libraries that library
requires), and of the own macros of each module it `import`s (not those that module took in turn, §4.7); the tool reads the files from the
paths as the compiler does (as written when absolute, else beside the file). A file it cannot read is a note on stderr, and its macros are
unknown: a head that is no word of lIR is still a macro use, but one the library names like a word of lIR (a `let`, in the new spelling) is
taken for lIR's own and respelled. `--macro-args respell` is the tool's former behaviour: every word in a macro use is respelled, which is
right only where the macros take lIR. The bodies of `define-macro`, `define-reader-macro`, `define-dispatch-macro` and `define-token-reader`
are lIR and are respelled as any function's; the names in their parameter lists are names and stay (`compiler/tests/lir/respell/`).

**Text the tool cannot lex.** A module that binds reader macros (§4.8) may hold what lIR's own lexer refuses: a string with an escape lIR
does not know (`#"\d+"`), a `<` that begins no vector type, a `[` that its reader macro closes, a stray bracket. The tool cannot run the
reader macros, so it does not fail: a top-level form that it cannot read is skipped, that is copied as written but for its head word when
that is an item's (`DEFINE`), and the tool says so on stderr, `FILE:L:C: respell: skipped, left as written: REASON at L:C`, and goes on at
the next line that begins with `(`. The exit status is that of the rest of the work. A form left so is respelled by `--macro-args respell`
no more than by default; it is the module's reader that reads it.

`FIB_TARGET_CPU` names the CPU code is generated for when the module has
no `target` form (§4.5): unset, empty or `host` mean the host's CPU and
features; any other value names a CPU with no extra features.
`FIB_LIR_FACTS` is the check mode of the facts (§6.13), `FIB_LIR_OWN`
the mode of the ownership verifier (§6.14.5), and `FIB_LIR_SPELLING`
(`old` or `new`) whether the old spelling is admitted (§2).

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
;; facts: use | trap | ignore   ; the check mode of the facts the case runs under (§6.13)
;; own: verify | report | off   ; the mode of the ownership verifier the case runs under (§6.14.5)
;; audit: …                 ; the lir-audit file it comes from and what liar did
;; stage: 2                 ; only the fibber `lair` implements this; the Rust `lair` skips the case
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
| 12 | the instructions SIMD code needs (SIMD wave 2, package P0, owner decisions of 2026-10-04): `fma fmuladd fsqrt fabs ffloor fceil ftrunc fround froundeven fcopysign fmin fmax fminnum fmaxnum smin smax umin umax abs`; fast-math flags `reassoc contract arcp afn nsz` (never `nnan`, `ninf` or `fast`, which make poison) on `fadd fsub fmul fdiv frem fma reduce-fadd reduce-fmul`; the horizontal `reduce-*` family; conversions, saturating conversions and `ptrtoint`/`inttoptr` on vectors; a vector index in `getelementptr`; `masked-load`, `masked-store`, `gather`, `scatter`; the module form `(target (cpu ..) (features ..))` | **adopted** as designed in docs/design/simd-and-tensors.md 2.9, 2.10, with `nsz` allowed (it never makes poison) and `fmuladd` and `fcopysign` added; implemented by the compiler in fibber only (the Rust `lair` is the legacy oracle and does not have them: its harness skips the cases marked `;; stage: 2`) | §4.5, §6.1, §6.3, §6.4, §6.5 |
| 13 | kernel forms for a kernel target: `kernelcc`, `(sreg R)`, `(barrier)`, and (until 2026-10-25, see §6.9a) `(align 1)` on a scalar read as natural alignment there | **Accepted** (the owner, 2026-10-07): implemented in the parser, checker, printer and lair, exercised by compiler/tests/native/gpu-emit.sh and j-unit; docs/design/gpu.md. `fib.gpu` is core, not a driver: `defkernel` is the deliberate opt-in, and a build for a platform with no kernel target rejects the program with a clear message. GPU-5 added to the same family `(sreg laneid)`, `(sreg warpsize)`, the intrinsic forms `shfl-down shfl-up shfl-xor shfl-idx` and `ballot` (§6.9a): **Accepted** (the owner, 2026-10-08: "Accept"), with the target-defined behaviours as written there | §2, §6.9a |
| 14 | facts: `nsw`/`nuw` on `add sub mul shl`, `(assume c)`, `(load (range LO HI) T p)`, each LLVM's own form in LLVM's spelling (lIR is LLVM IR as S-expressions; a front-end abstraction is the front end's to lower into these), and the check mode `FIB_LIR_FACTS=use|trap|ignore` | **Decided** (owner, 2026-10-24); docs/design/lir-facts.md | §6.1, §6.5, §6.12, §6.13, §10 |
| 15 | compile-time macros in lIR: a module-level `define-macro` whose body is lIR over s-expression objects that lIR owns (`lir.sexp.*`), expansion by lair outermost-first between reading and parsing, `require-macros` of a file of definitions, no hygiene (gensym), `defun` as the first library macro (lib/lir/defun.lir). The named exception to "LLVM's spelling first" (docs/design/lir-language.md D1): LLVM has no macros, and the languages on lIR are libraries of them | **Accepted by default** (the gloss design, 2026-10-25, takes D1's recommendation; the owner may override) | §4, §4.6, §12, cases/lir/macros |
| 16 | `(import "FILE")`: a program is the union of its modules as `llvm-link` joins LLVM modules; the importer sees the imported module's exported functions as `declare`s, its exported globals as `declare-global`s, its structs, and the macros of its own file; loaded once per program in dependency order, cycles refused, errors at the import with the module's file and position; exported names defined once, declarations agree in type, internal names renamed `NAME.N` on collision; no transitive export, no search root (a path, as `require-macros`); fibber's numbered ids (ADR 0024) are met by the duplicate rule (an emitted program is imported by nothing) | **Accepted by default** (gloss item 3, 2026-10-25; the owner may override) | §4, §4.7, cases/lir/imports |
| 17 | reader macros: a per-module readtable in Lisp's shape (`define-reader-macro` on a character, `define-dispatch-macro` on the character after `#`, `define-token-reader` after every atom), each an lIR function of the stream (`lir.read.*`) returning a form or no form, run at read time through the macro session; `(`, `)`, `;` and whitespace cannot be bound, everything else can and the module then reads under its table; a required library's bindings are the requiring module's, an import's are not (each module of a program reads under its own table); lIR's own syntax unchanged and a module with no bindings reads as before; `lib/lir/readers.lir` (`'x`, `[a b]`) the first library. The owner's direction of 2026-10-25: lIR as thin as possible, a language brings its own syntax as reader macros | **Proposed** (the lead, 2026-10-25) | §4, §4.8, cases/lir/readers |
| 18 | ownership in lIR: the managed type `(ptr 1)` (LLVM's `ptr addrspace(1)`), kinds on SSA values and parameters, SIL's forms `alloc-object alloc-stack dealloc-stack copy-value destroy-value is-unique`, `load copy|borrow|take`, `store init|assign|borrow`, `addrspacecast` inside `(define unsafe ..)`, the `ownership` form naming what the forms lower to (one per program, in the interface of the module that carries it), a verifier of ten rules inside `lir.check` on every path (`FIB_LIR_OWN`, `report` the default until the emitter emits the forms), erasure as the last pass before LLVM or a printer after an optimisation slot on the explicit forms; MIR's words weighed and recorded | **Decided** (owner, 2026-10-25: lIR is the language, lair the only compiler; docs/design/lir-language.md stage 2, D4 to D10 as recommended) | §2, §4, §4.7, §6.10, §6.14, §10, §12, §13, cases/lir/own |
| 19 | `(dependent-library STRING)`: LLVM's `!llvm.dependent-libraries` as a module form; lair lowers it to that metadata and links and loads from it on every path, and reads no `;; link-lib:` comment and no list beside the module any more | **Decided** (owner, 2026-10-25: the text is the contract) | §4, §4.9, cases/lir/link |
| 20 | `(export-name STRING)` on a define, LLVM's `"wasm-export-name"`; a native library exports its `external` non-`hidden` definitions by their linkage and visibility, with no symbol list beside the module; library versus executable is lair's build option `--library`, not the environment variable `FIB_EXPORTS` | **Decided** (owner, 2026-10-25: the text is the contract) | §4, §4.3, §7.2, §12, cases/lir/attrs |
| 21 | lIR spells names out and every word it owns is upper case (owner, 2026-10-25: "lir should prefer spelled out names not abbreviations or macros. So @ should be 'global'"; "I'm also ok with using CAPS for lir"): `(GLOBAL f)` for `@f`, `(STRUCT S)` for `%struct.S`, `(PTR (ADDRSPACE 1))` for `(ptr 1)`, `FUNCTION` for `fn`, `ALWAYSINLINE` for `inline`, `PRESERVE_MOSTCC` for `preservemost`, `INF`/`NAN`, `(VECTOR N T)`, `(ARRAY N T)` and `(LITERAL-STRUCT ..)` for LLVM's bracket syntax; names as written; a name that is a word refused; the reserved words and the reserved macro names gone; one table of the words (`compiler/lir/words.fib`) held equal to the grammar by `compiler/tests/lir/words.sh`; `lair respell` converts a text. The audit and the plan: `/tank/data/fibber-scratch/lir-language/respell-audit.md` | **Decided** (owner, 2026-10-25) | §1, §2, §3, §4, §5.1, §5.3, §6, §9, §12 |
| 22 | string constants: each `(STRING ..)` is its own constant, so lair writes no `unnamed_addr` and LLVM merges none (D12: the code follows the spec) | **Decided** (owner, 2026-10-25) | §3, cases/lir/instr/string-own-constant.lir |
