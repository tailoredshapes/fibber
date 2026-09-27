# fibber types and the ownership checker

Status: signed off. This file grew out of the three drafts under
`spec/drafts/` (see `spec/drafts/SYNTHESIS.md`); on 2026-09-27 the owner
accepted every open decision of the three review rounds as recommended,
with the amendments D1–D7 listed in §10. Every rule here is now
**Decided** unless marked **Proposed**; a "D*n*" beside a rule names
the amendment it comes from. Authority: [ownership.md](ownership.md) is
Decided and wins where the two meet. Surface syntax is in
[syntax.md](syntax.md) ("syntax §n"); "§n" alone is ownership.md.

Organising principle: the type system exists so that every rule of
ownership.md is a property the checker computes from types and syntax
alone, with no alias analysis, no lifetimes and no flow-sensitive
reasoning beyond scope nesting. Ownership is not part of the type
system; it is a separate pass over typed code (§6) whose output (§9) can
be printed and diffed against the reference interpreter's trace.

Contents: §1 type grammar · §2 typing rules per core form · §3 the
inference algorithm · §4 protocol dispatch · §5 threads: `Send` and
closure colours · §6 the ownership checker, rule by rule · §7 case table
· §8 mapping to lIR · §9 what the checker prints · §10 decision record.

---

## 1. Type grammar

```
type   ::= scalar | object | tvar
scalar ::= bool | i8 | i16 | i32 | i64 | f32 | f64 | char | keyword | unit
         | ptr                                   ; only inside unsafe
         | Enum                                  ; a defenum whose variants have no fields
object ::= str | Form
         | (Array type)                          ; primitive fixed immutable array
         | Name | (Name type+)                   ; nominal struct or enum, incl. the library's Vec, Map, Set, List, Option
         | (Cell type) | (Atom type) | (Weak type) | (Task type)
         | (fn colour? (type*) type)             ; function or closure; colour := :send | :local
         | (dyn Proto) | (dyn (Proto type+))     ; dynamic protocol value
tvar   ::= a lowercase symbol that names no type   ; a type variable, scoped to its definition
```

Signatures of functions with `&` parameters, the primitive `array-set!`
among them, are written `(fn ((& T) ...) R)` and are not types (§1.4,
§2.13).

### 1.1 Scalars and literals

Scalars are copied and have no identity, no count and no mode (**Decided**,
§1). `bool` is lIR `i1`; the integers are the lIR integers; `f32`/`f64` are
`float`/`double`; `char` is a Unicode scalar value in an `i32`; `keyword`
an interned id in an `i64`; `unit` has the one value `()` and is erased
in codegen; `ptr` is a raw pointer, usable only inside `unsafe`. No
implicit conversion exists between any two scalar types (**Decided**,
D3; consistent with lIR's no-promotion rule, liar ADR 017 is out).

An integer literal has the width of its suffix, `i64` without one; a
float literal `f64` unless suffixed `f32`. There is no literal
polymorphism (**Decided**, D3: no defaulting search): `(+ x 1)` pins
`x : i64`; `(+ x 1i32)` pins `i32`.

### 1.2 Objects

Everything that is not a scalar is an object: a counted value on the heap
(or on the stack when the compiler proves it never escapes, §6.11),
represented as one pointer, immutable unless it is a `Cell` or an `Atom`
(**Decided**, §1, §6, §7).

| Type | Mutable? | `Send` (§5) | Notes |
|---|---|---|---|
| `str`, `Form`, `(Array T)` | no | `str`, `Form`: yes; `Array`: if `T` is | `Array` is the substrate of the library collections (§2.13) |
| struct, enum with fields | no | if every field is | nominal; a field may be a `Cell`/`Atom` |
| `(Vec T)`, `(Map K V)`, `(Set T)`, `(List T)` | no | if their parameters are | library structs and enums; only their names are known to the compiler, for literals |
| `(Option T)` | no | if `T` is | built-in enum (§1.5; syntax §3.9): `nil` is a reader literal and the representation is special (§8.1) |
| `(Cell T)` | **yes** | **no** | the one mutable container (§6) |
| `(Atom T)` | yes, atomically | yes | requires `Send T` at construction (§7) |
| `(Weak T)` | no | if `T` is | `T` must be an object type |
| `(Task T)` | internal | if `T` is | made by `async` or `spawn` |
| `(fn κ (A..) R)` | no | iff κ = `send` | a closure's colour is decided by what it captures (§5.4) |
| `(dyn P)` | no | no (v1) | pointer plus vtable (§4.4) |

### 1.3 Nominal types, generics, recursion

`defstruct`/`defenum` names are nominal: two structurally identical
definitions are different types; equality is same definition and equal
arguments. Type parameters come from a definition head `(Name a b)`,
from unannotated struct fields (syntax §3.7), or from generalisation at
a `defun` boundary (§3.6). Annotations may mention type variables by
name; polymorphism is prenex (rank 1) only. Bounds on a `defun`'s
variables are protocol constraints and `Send`, inferred from its body
(§3.6); they may also be written in an annotation: `(x: (Vec a))` with
`(Countable a)` is written as a constraint list after the parameters
(syntax §3.1), `(defun f (x: (Vec a)) :where ((Show a)) -> str ...)`
(**Decided**; rarely needed on a `defun`, since bounds are inferred;
required on a generic `impl`, whose context is declared, §2.7).

Recursion is allowed only through a nominal type: a struct or enum may
mention itself, directly or through other definitions, and every field
on such a cycle must be annotated (syntax §3.7). Unification performs
the occurs check, so an inferred cyclic type is the error `cannot
construct the infinite type`. This is why case 15 ties its knot through
a struct.

### 1.4 Function and closure types

`(fn κ (A₁ .. Aₙ) R)` is the type of named functions used as values,
constructors, protocol methods and closures alike; arity is fixed. The
**colour** κ ∈ {`send`, `local`} ∪ colour variables says whether the value
may cross a thread: `send` iff every capture is `Send` (§5.4). Named
functions and constructors are `send`. Colours are never written in
inferred code. In an annotation, an omitted colour means:

- on a `defun` parameter, a `let` annotation or a protocol method
  parameter: a fresh colour variable (any closure is accepted; the body
  constrains it);
- on a struct or enum field: `local` (the type of a field must be a
  function of the type's parameters, and colours are not parameters in
  v1); write `(fn :send (A) R)` to store only sendable closures;
- on a `def` annotation (§2.16): `send`, since the only function values
  a constant expression can build are named functions and constructors,
  which are `send`.

Closure types carry no capture list, no escape summary and no count
kinds (**Decided**): a call through a function value treats every
argument as escaping (§6.4), which is always safe, and hands every
object argument and the closure itself over owned (the closure
convention, §6.4, D5).

A `defun` with `&` parameters has a **signature** `(fn ((& T₁) T₂ ..) R)`
rather than a type: it can be called with `&x` at those positions
and may not be referenced as a value (`function with & parameters is
not a value`). The primitive `array-set!` has one too, written the
same way (§2.13); `set-field!`, whose second operand is a field name,
has neither a type nor a signature but a form rule of its own (§2.13),
and is not a value either. Inside a `defun`'s body an `&`
parameter `v` has type `(Cell T)` and is not a value: it occurs only as
`@v`, `&v` or the target of `set!` (syntax §3.13, §2.14; **Decided**,
D2).

### 1.5 `Option` and `nil`

`(Option a)` is the built-in enum with the field-less variant `nil` and
the variant `(some v: a)`; it is built in rather than declared because
`nil` is a reader literal, the form `(Nil)`, that no `defenum` can
spell (syntax §3.9), and because of its representation (§8.1).
`nil : ∀a. (Option a)`; `(some e) : (Option T)` when `e : T`. There is
no null of any other type, no implicit conversion from `T` to
`(Option T)`, and `match` (with the prelude's `if-let`, `nil?`, `some?`
over it) is its only eliminator. For an object `T` that is not itself an
`Option` the representation is a nullable pointer (§8.1), so the
abstraction costs nothing; `(Option (Option T))` is a heap enum, so
`(some nil)` and `nil` stay the distinct values the semantics says they
are (§8.1). The prelude derives `Eq`, `Ord` (`nil` before `some`),
`Hash` and `Show` for it (syntax §4.4, §3.16).

### 1.6 `Cell`, `Atom`, `Weak`, `Task`

- `(Cell T)`: **Decided** (§6) the one mutable container; never `Send`.
- `(Atom T)`: **Decided** (§7) a cell with atomic operations; well formed
  only if `Send T`, so an atom can always cross.
- `(Weak T)`: **Decided** (§6) a non-owning reference; `@w` has type
  `(Option T)`. A `Weak` value is itself a counted object (§8.7).
- `(Task T)`: the result of `spawn` and of `async`; `join`, `block-on`
  and `await` take it. `Send (Task T) = Send T`. Any number of holders
  may join or await one task; the runtime resumes it on one thread at a
  time (§8.8).

### 1.7 Protocol types

A protocol `P` is not a type. `(dyn P)` is: the type of a value of some
unknown type implementing `P`, carrying its dispatch table (§4.4). It is
produced only by the explicit primitive `(dyn P e)` (**Decided**, D3: no
subtyping, no coercion anywhere in the type system).

### 1.8 Constraints and schemes

```
C ::= (P T₁ .. Tₙ)          ; protocol constraint; T₁ is the dispatch position
    | (Send T)              ; §5.1
    | κ₁ ⊑ κ₂               ; colour order, send ⊑ local (§5.4)
    | κ ⊒ Caps{T₁ .. Tₖ}    ; a closure is at least as local as its captures demand
    | HasField(T, f, R)     ; deferred: T is a struct with field f : R (§3.4); never in a scheme
    | HasDeref(T, R)        ; deferred: T is a Cell, Atom or Weak (§3.4); never in a scheme
σ ::= ∀ā ς̄. C̄ ⇒ T           ; a type scheme
```

---

## 2. Typing rules for the core forms and builtins

Judgement: `Γ ⊢ e : T | C` — under an environment Γ (variables to types,
with each variable's binding kind: parameter, `&` parameter, `let`,
pattern, capture; plus the global tables of structs, enums, protocols,
impls, externs and defun schemes), `e` has type `T` and emits constraints
`C`. Every rule is an inference rule: it generates equations solved by
unification (§3), it does not require types to be known in advance except
where a rule says "resolved". `fresh` is a fresh type variable, `ς` a
fresh colour variable, `inst(σ)` instantiates a scheme with fresh
variables and emits its constraints.

### 2.1 Literals and variables

| Form | Type |
|---|---|
| integer literal | its width (`i64` default); float literal `f32`/`f64` (`f64` default) |
| `"s"`, `\c`, `true`/`false`, `:k`, `()` | `str`, `char`, `bool`, `keyword`, `unit` |
| `nil` (the reader's `(Nil)` form, or the symbol `nil` a macro built; syntax §3.9) | `inst(∀a. (Option a))`; `(nil)` is the error `nil is a constant, not a function; write nil` (§2.2) |
| `x` bound locally | `Γ(x)`, monomorphic; an `&` parameter `v` has `Γ(v) = (Cell T)` but is not an expression: it occurs only as `@v`, `&v` or the target of `set!` (§2.14), else `& parameter v used as a value in f` |
| `f` a global defun, constructor, variant constant or protocol method | `inst(σ_f)`; error if `f` has `&` parameters (`function with & parameters is not a value`) |
| `g` a `def` name (syntax §3.19) | `Γ(g)`, its closed monomorphic type (§2.16); a global like `f`, never a capture |
| `[e₁ .. eₙ]`, `{k v ..}` | rewritten to prelude calls before typing (syntax §1.4) |
| `'form`, `` `form `` | `Form`; inside a quasiquote `,e` needs `e : Form` and `,@e` needs `e : (Vec Form)` |

### 2.2 Calls

```
(f a₁ .. aₙ)     f : (fn κ (T₁ .. Tₙ) R),  aᵢ : Tᵢ          ⇒ R
(g .. &x ..)     g a defun or the primitive array-set!, whose signature (§1.4) has & at that
                 position with value type T;  x a variable (syntax §3.13) of type (Cell T)
                                                                ⇒ ok for that position
```

A protocol method has no `&` parameter (the method grammar of syntax
§3.10 has none), and the `&` operand of `set-field!` is typed by that
primitive form's own rule (§2.13). Arity must match exactly. Head
position may be any expression of function type; a call to a `defun`
or method instantiates its scheme. `&x` at a non-`&` position, or a
plain argument at an `&` position, is a type error (`parameter v of g
is &; pass &x`). A field-less variant is a
constant, not a function: `(V)` with zero arguments is the error `V is
a constant, not a function; write V` (syntax §3.9).

### 2.3 `defun`, `fn`

```
(defun f (p₁ .. pₙ) -> R b)   pᵢ : Tᵢ (annotation or fresh; &pᵢ: Γ(pᵢ) = (Cell Tᵢ)); Γ,p̄ ⊢ b : R'; R' ~ R if annotated
(fn (p₁ .. pₙ) b)             Γ,p̄ ⊢ b : R  ⇒ (fn ς (T₁ .. Tₙ) R)  with  ς ⊒ Caps{Γ(y) | y free in b, y local}
(fn g (p̄) b)                  as above with g : (fn ς (T̄) R) in Γ while typing b (monomorphic); g is not a capture
```

A `defun` is monomorphic inside its call-graph SCC and generalised after
it (§3.6). `main` must have type `(fn () i64)`. A `fn` is never
generalised.

### 2.4 `let`, `do`, `if`

```
(let ((pat₁ e₁) ..) b)   eᵢ : Tᵢ;  patᵢ checked against Tᵢ binding Γᵢ (§2.6, irrefutable);  Γ,Γ̄ ⊢ b : T  ⇒ T
(do e₁ .. eₙ)            each eᵢ typed; ⇒ Tₙ;  (do) ⇒ unit
(if c t e)               c : bool, t : T, e : T  ⇒ T
(loop ((x₁ e₁) .. (xₙ eₙ)) b)   eᵢ : Tᵢ;  Γ, xᵢ:Tᵢ ⊢ b : T with recur enabled at (T₁ .. Tₙ)  ⇒ T
(recur a₁ .. aₙ)         aᵢ : Tᵢ of the innermost enclosing loop  ⇒ fresh (it never yields a value)
```

Loop variables are monomorphic, like `let` bindings. A `recur` that is
not in tail position of its loop body, that is outside any `loop`, or
that sits inside a `fn` or `async` literal nested in the loop, is
`recur not in tail position` / `recur outside loop` (syntax §3.18).

`let` never generalises (**Decided**, D3): `(let ((id (fn (x) x)))
(id 1) (id "a"))` is a type error. Because of this a cell created in a
`let` cannot acquire a polymorphic type, so no value restriction is
needed: `(let ((c (cell nil))) (set! c (some 1)) (set! c (some "a")))`
is a plain type error.

### 2.5 `.`

```
(. e f)    e : T;  emit HasField(T, f, R) with R fresh  ⇒ R
```

Solved when `T` resolves to a struct type `(N ā)`: `R ~` the field's
type under the instantiation. If `T` is still a variable when the
enclosing SCC is generalised: `cannot infer the struct type of e for
field f; annotate it` (§3.4). Any other constructor: `T has no field f`.

### 2.6 `match` and patterns

```
(match s cl₁ .. clₙ)    s : S;  each clause (pat b): pat checked against S binding Γ_pat;  Γ,Γ_pat ⊢ b : T  ⇒ T
```

`Γ ⊢ pat : S ⇝ Γ'`:

| Pattern | Rule |
|---|---|
| `_` | any `S`; binds nothing |
| `x` | binds `x : S` |
| literal | `S ~` the literal's type; requires `(Eq S)` |
| `nil` (also spelled `(nil)` by a macro, syntax §3.9), `(some p)` | `S ~ (Option a)`, `a` fresh; `p : a` |
| `(V p₁ .. pₖ)` | `V` a variant of `(N ā)` with `k` fields; `S ~ (N b̄)`, `b̄` fresh; `pᵢ : Fᵢ[b̄/ā]` |
| `(N p₁ .. pₖ)` | struct `N` with `k` fields, likewise |
| `(p :as x)` | binds `x : S`, then `p : S` |

Exhaustiveness and redundancy are decided after the SCC's types are
solved, on the resolved scrutinee type, by the standard usefulness
matrix (Maranget). If the scrutinee's type is still a variable then,
only `_`/variable clauses are exhaustive. `let` accepts only irrefutable
patterns (syntax §3.3).

### 2.7 `defstruct`, `defenum`, `defprotocol`, `impl`

| Form | Rule |
|---|---|
| `(defstruct (N ā) (f₁: T₁ ..))` | registers `N` of arity |ā|; constructor `N : ∀ā. (fn :send (T₁ ..) (N ā))`; field types well formed and closed under `ā`; recursive occurrences only through annotated fields |
| `(defenum (N ā) (V₁ T̄₁) ..)` | `Vᵢ : ∀ā. (fn :send (T̄ᵢ) (N ā))`, or `∀ā. (N ā)` for a field-less variant |
| `(defprotocol (P s d̄) (m (self x₁: T₁ ..) -> R) ..)` | `m : ∀ s d̄ b̄. (P s d̄) ⇒ (fn :send (s T₁ ..) R)` where `b̄` are the signature's other variables; the functional dependency `s → d̄` is recorded; each parameter's escape kind (`:borrow` or the default, escaping) and count kind (`:owned` or the default, borrowed) are recorded (§6.4) |
| `(impl (P D̄) (K ā) :where (C) (m (self x̄) b) ..)` | registers the instance `∀ā. (P (K ā) D̄) ⇐ C` where `C` is the **declared** context (`:where`; empty when omitted; Paterson condition, §3.3), known before any body is typed (§3.5; §10 item 20); each body is typed against the signature with `s := (K ā)` rigid, `d̄ := D̄`, under the bounds `C`, and must not be more specific; a body whose constraints are not entailed by `C` is `no implementation of P for a; add (P a) to the :where of the impl`; every method present, none extra; one instance per `(P, K)` in the program; each body's escape summary must respect the declared kinds (§6.4) |

### 2.8 `async`, `await`, `unsafe`, `extern`, `quote`, `defmacro`

```
(async b)        Γ' ⊢ b : T with await enabled  ⇒ (Task T);   constraint  send ⊒ Caps{Γ(y) | y free in b}
(await e)        e : (Task T)  ⇒ T;  error outside async or inside a fn nested in an async (a loop body inside the async is inside it): await outside async
(unsafe b)       b : T  ⇒ T;  ptr operations and extern calls enabled inside
(raw e)          e : T, (Object T)  ⇒ ptr;   (raw-retained e)  the same, a consume position (§6.13);   (release-raw p)  p : ptr  ⇒ unit;  all three only inside unsafe
(extern c (T̄) -> R opts)   c : (fn :send (T̄) R), T̄ and R scalars, ptr or unit; callable only inside unsafe
(quote f)        Form
(defmacro m (p̄) b)   m : (fn :send (Form ..) Form) with (Vec Form) for a ... rest parameter; typed like a defun in the macro-time module
```

The `send ⊒ Caps` constraint on `async` is **Decided**: §3.4 lists "a
task" among thread crossings and §7 admits only immutable objects and
atoms across one, so a task's captures must be sendable (§5).

### 2.9 `cell`, `deref`, `set!`

```
cell    : ∀a. (fn :send (a) (Cell a))
deref   : ∀c t. (Deref c t) ⇒ (fn :send (c) t)     ; @e ≡ (deref e)
set!    : ∀a. (fn :send ((Cell a) a) unit)         ; target: an expression of cell type or an & parameter name
```

The target of `set!`, its first operand, is an expression of cell type
(a field path of cell type among them, syntax §3.8) or the name of an
`&` parameter `v` of the enclosing `defun`, which is not an expression
(§2.14) but stands there with the type `Γ(v) = (Cell T)`, as it does as
the operand of `@`; these two positions and `&v` are the only ones in
which `v` may occur.

`Deref` is the built-in protocol `(defprotocol (Deref c t) (deref (self) -> t))`
with the built-in instances `(Deref (Cell a) a)`, `(Deref (Atom a) a)` and
`(Deref (Weak a) (Option a))`; `t` is determined by `c`. `@x` on a
variable of unresolved type is the deferred constraint `HasDeref` (§3.4),
resolved when the head becomes known; unresolved at generalisation it is
the error `cannot infer whether x is a cell, an atom or a weak reference`.

### 2.10 `atom`, `swap!`, `reset!`

```
atom    : ∀a. (Send a) ⇒ (fn :send (a) (Atom a))
swap!   : ∀a. (fn :send ((Atom a) (fn ς (a) a)) a)      ; f's colour unconstrained: f runs on the calling thread; f is :borrow
reset!  : ∀a. (fn :send ((Atom a) a) unit)
```

`set!` on an atom is a type error; atoms are written only by `swap!` and
`reset!`. `(atom (cell 0))` fails `Send (Cell i64)` at `atom`, which is
how "a cell that is not an atom may not cross" holds without inspecting
atom contents at `spawn`.

### 2.11 `weak`, `spawn`, `join`, `trap`

```
weak    : ∀a. (Object a) ⇒ (fn :send (a) (Weak a))
spawn   : ∀a. (Send a) ⇒ (fn :send ((fn :send () a)) (Task a))
join    : ∀a. (fn :send ((Task a)) a)                   ; block-on is the prelude alias
trap    : ∀a. (fn :send (str) a)                        ; aborts; panic is the alias
```

`(Object a)` is the built-in structural predicate "`a` is not a scalar",
kept as a constraint on a variable and solved when its head is known.

### 2.12 Arithmetic, comparison, conversions

Arithmetic and comparison are protocol methods with built-in instances:
`Num` for every integer and float type, `Bits` for the integer types,
and `Eq`, `Ord`, `Hash` and `Show` for every scalar type (a field-less
enum, a scalar by §1, compares and hashes by variant index, orders by
declaration order and shows as its variant name) and for `str`:

```
(defprotocol Num  (+ (self y: Self) -> Self) (- ..) (* ..) (/ ..) (rem ..) (neg (self) -> Self))   ; every integer and float type
(defprotocol Eq   (= (self y: Self) -> bool) (!= ..))
(defprotocol Ord  (< (self y: Self) -> bool) (<= ..) (> ..) (>= ..))                                ; requires Eq
(defprotocol Bits (bit-and (self y: Self) -> Self) (bit-or ..) (bit-xor ..) (bit-not (self) -> Self)
                  (shl (self n: Self) -> Self) (shr ..) (sar ..) (popcount (self) -> Self))          ; integer types only
(defprotocol Hash (hash (self) -> i64))
(defprotocol Show (show (self) -> str))
not : (fn :send (bool) bool)
```

`Self` in a signature stands for the dispatch type, so `(+ a b)` unifies
both operands: `(+ (i32 1) 2)` is a type error, never a promotion.
`(defun add (a b) (+ a b))` is `∀a. (Num a) ⇒ (fn :send (a a) a)`. Integer
division and remainder trap on zero; signed overflow wraps (**Decided**:
it is what lIR emits and what the interpreter can match exactly).
Conversions are primitive forms whose first
operand is a type: `(trunc i8 e)`, `(zext i64 e)`, `(sext i64 e)`,
`(fptrunc f32 e)`, `(fpext f64 e)`, `(fptosi i64 e)`, `(fptoui i64 e)`,
`(sitofp f64 e)`, `(uitofp f64 e)`, `(char->i32 e)`, `(i32->char e)`
(traps on a non-scalar value), each with the obvious operand and result
types. `(derive P Name)`, for `P` one of `Eq`, `Ord`, `Hash`, `Show`,
is a prelude macro over `struct-fields`, `struct-params` and
`struct-field-types` for a struct, and over `enum-params` and
`enum-variants` for an enum (syntax §3.16), that generates one `impl`
with the head `(Name ā)` and the context `(P a)` for each parameter `a`
that some field's type mentions (plus `(Eq a)` for `Ord`, which has no
supertrait to give it, §4.1), so it works for generic structs and
enums too; on an enum the methods are a `match` on `self` nested with
a `match` on the other operand (`Eq`: same variant and equal fields;
`Ord`: declaration order, then the fields lexicographically), and on a
field-less enum the macro expands to nothing, the instances being
built in. The prelude derives all four for `Option` and `List` (syntax
§4.4), so `(= (some 1) (some 1))` resolves through `(impl Eq (Option
a) :where ((Eq a)) ..)` to the `i64` instance (proposed case 48).

### 2.13 Arrays and `set-field!`

`(Array T)` is the primitive fixed-size immutable array on which the
library builds `Vec` (32-way trie), `Map` and `Set` (HAMT) and `str`
operations, exactly as liar's `lib/` does with `heap-array`, but typed
and counted (**Decided**):

```
array      : ∀a. (fn :send (i64 a) (Array a))                     ; n copies of init (each stored: E2)
array-len  : ∀a. (fn :send ((Array a)) i64)
array-get  : ∀a. (fn :send ((Array a) i64) a)                      ; traps out of range; result is a borrow of the element inside the body, owned to the caller (§4)
array-with : ∀a. (fn :send ((Array a) i64 a) (Array a))            ; a new array with one slot changed (E2 for the element)
array-copy : ∀a. (fn :send ((Array a) i64 i64) (Array a))          ; slice [i, j)
array-set! : ∀a. (fn ((& (Array a)) i64 a) unit)                   ; a signature (§1.4); in place iff unique, else copy (§6.6)
(set-field! &x f e)    x : (Cell S);  f a field name, not an expression;  HasField(S, f, F);  e : F   ⇒ unit
                                                                    ; in place iff unique, else copy (§6.6)
```

`array-set!` has a signature, not a type (§1.4): like a `defun` with
an `&` parameter it is called with `&x` at its first position (§2.2)
and is not a value. `set-field!` has neither: its second operand is a
field name, so it is a **primitive form** with the rule above, as the
conversions (§2.12) and `dyn` (§2.15) are (syntax §4.3). Its `&x` is an
`&` argument as at any `&` position (§2.14: `x` a variable of cell
type), whose content type `S` must be a struct with a field `f`; `f` is
never evaluated and never resolved as a variable, so `(set-field! &c x
e)` sets the field `x` whatever a local `x` holds; `HasField(S, f, F)`
is the deferred constraint of `.` (§2.5, §3.4), with its failures, `S
has no field f` and, unresolved at generalisation, `cannot infer the
struct type of @x for field f; annotate it`; `e` is an ordinary operand
and a store (E2, §6.3). At either primitive `&x` takes no copy-in:
the primitive tests and updates the content of `x`'s own cell (§6.6).
A reference to `array-set!` or `set-field!` other than as the head of
a call is `function with & parameters is not a value` (§2.1). The two
are the only primitives that perform a **unique write** (§6.6);
everything else that updates in place is library code over them.

### 2.14 `&` parameters and places

In `(defun f (&v ..) b)`, `Γ(v) = (Cell T)` with `T` the annotation or
fresh; `T` is unified by the uses of `@v`, `&v`, `(set! v e)` and the
primitives. `v` itself is not an expression (**Decided**, D2; syntax
§3.13): the only forms in which it may occur, in `b` and in every
closure literal inside `b`, are `@v` (`(deref v)`), `&v` and `(set! v
e)`; any other occurrence is `& parameter v used as a value in f`. So
no expression ever has the private cell as its value, and the checker
assigns no mode to `v` (§6.6): the private cell is a stack object of
the caller's frame, so `(defun leak (&v) v)` would return a pointer to
memory freed at the write-back, as would `(cell v)`, `(weak v)` and a
non-escaping closure returning `v`. At a call, `&x` requires `x : (Cell
T)` for the parameter's `T`; `x` may be an `&` parameter or any other
variable of cell type (syntax §3.13; there is no field place, D1). Any
other `&` argument is `& argument must be a cell variable`.

### 2.15 `dyn`

```
(dyn P e)    e : S with the instance (P S) resolved at generalisation  ⇒ (dyn P)
```

A method call on a `(dyn P)` receiver has the method's signature with
`self := (dyn P)`; methods whose signature mentions `self` anywhere but
the receiver position are not callable through `dyn` (§4.4).

### 2.16 `def`

```
(def g e)        e a constant expression (syntax §3.19);  Γ ⊢ e : T;  T closed after solving  ⇒  g : T in the global environment
(def g: A e)     as above with T ~ A
```

A `def` is a node of the dependency graph of step 4 (§3.5): it depends
on the earlier `def`s it names and on every same-module `defun` it
names as a value, and every `defun` that reads it depends on it. So it
is typed after the SCCs of the functions it names, whose schemes are
then complete, and before the functions that read it, in an environment
holding constructors, the `def`s it names, the named functions it names
(`inst` of their schemes, §2.1) and the imported schemes of the prelude
calls that the literal-collection rewrite introduces (`vec-empty`,
`conj`, `map-empty`, `assoc`; syntax §1.4): these are the only calls a
constant expression contains. A `def` that names a `defun` which reads
it, directly or through other functions, is the error `def g and defun
f depend on each other` (**Decided**). `T` is
never generalised, and a type variable left in it is `def g has an
unresolved type; annotate it`: a `def` naming a generic function
(`(def twice-fn double)` with `double : ∀a. (Num a) ⇒ (fn (a) a)`) must
fix the instantiation, `(def twice-fn: (fn (i64) i64) double)`, and an
omitted colour in that annotation means `send` (§1.4); the
monomorphiser then emits the specialisation the annotation names
(§4.3). A `def` naming a monomorphic function needs nothing: `(defun
double (x: i64) -> i64 (+ x x))` gives `(def twice-fn double)` the
closed type `(fn :send (i64) i64)` (proposed case 50). A form outside
the constant grammar (a call other than a constructor or the rewrite's
prelude calls, `cell`, `atom`, `weak`, `fn`, `async`, `unsafe`, `@`) is
`def g: initialiser is not a constant expression`. `Send T` holds by
construction, since no constant expression builds a cell and named
functions are `send`, which is why a `def` name may appear in any `fn`
or `async` body without being a capture: like a named function it is a
global, not a free variable (§3.7). Its value is immortal (§8.2), so
reading it is a count-free `Borrowed(g)` (§6.1).

---

## 3. The inference algorithm

**Name (Decided, D3):** constraint-based Hindley–Milner with generalisation
only at top-level `defun` SCC boundaries (Rémy levels), qualified types
whose protocol constraints have one dispatch parameter and functionally
determined extra parameters (Jones-style improvement), deferred
structural constraints for field access and `deref`, and a two-point
colour lattice solved by least fixpoint. Every piece is standard and
decidable; there are no coercions and no subtyping, so every well-typed
`defun` has a principal scheme and acceptance does not depend on solving
order.

### 3.1 Data

- Types as in §1, with unification variables α (union-find nodes carrying
  a *level*: the SCC depth at which they were created) and rigid
  variables (a `defun`'s own quantified variables while checking against
  an annotation or a polymorphic-recursion signature).
- Colour variables ς: a separate sort, never bound by unification (§5.4).
- A **substitution** S (union-find with path compression).
- A worklist of **deferred constraints**: protocol constraints,
  `Send`/`Object`, `HasField`, `HasDeref`, colour constraints.

### 3.2 Unification

Robinson unification with the occurs check over the grammar of §1:

- variable–term: occurs check; bind; propagate the smaller level into
  the bound term's variables (Rémy);
- constructor–constructor: same constructor and arity, unify arguments
  pairwise, else `cannot unify T₁ with T₂`; nominal constructors unify
  only with themselves; `(dyn P)` only with itself;
- function–function: unify parameter lists (same arity) and results;
  colours are **not** unified: at a *flow site* (an application argument,
  a branch join, an annotated `let`, a `set!` value, a constructor
  argument, a return against an annotation) the constraint `κ_from ⊑
  κ_to` is emitted; anywhere else both `κ₁ ⊑ κ₂` and `κ₂ ⊑ κ₁`;
- `(& A)` with `(& B)` only: an `&` position never unifies with a plain
  one;
- a rigid variable unifies only with itself or an unbound unification
  variable.

Occurs-check failure is `cannot construct the infinite type` (case 15's
original spelling would hit it; the case is written nominally).

### 3.3 Deferred constraints

Each deferred constraint is retried whenever a variable it mentions is
bound, and again at the end of the SCC:

| Constraint | Solved when | Failure |
|---|---|---|
| `(P T₁ .. Tₙ)` | head of `T₁` known: look up the unique instance for `(P, head T₁)`, instantiate it, unify its `S` with `T₁` and its determined arguments with `T₂ .. Tₙ` (the improvement `T₁ → T₂..`), then replace the constraint by the instance's declared context (§2.7). `T₁` a rigid variable: must be entailed by a bound. `T₁ = (dyn P)`: satisfied. `T₁` an unbound variable at generalisation: becomes a bound of the scheme if `T₁` occurs in the type, else `ambiguous constraint P a in f; add an annotation` | `no implementation of P for T₁` |
| `(Send T)`, `(Object T)` | evaluated structurally once the head is known (§5.1); a quantified variable's constraint becomes a bound | `cell cannot be shared between threads: …` (§5.3) / `value of type T cannot be shared between threads: …` / `weak requires an object type` |
| `HasField(T, f, R)` | `T` becomes a struct: `R ~` field type | `T has no field f`; unresolved at generalisation: `cannot infer the struct type of e for field f; annotate it` |
| `HasDeref(T, R)` | `T` becomes `Cell`/`Atom`/`Weak`: `R ~ T'`/`T'`/`(Option T')` | unresolved: `cannot infer whether x is a cell, an atom or a weak reference` |
| colour constraints | §5.4, after all type constraints of the SCC | `cell cannot be shared between threads: closure capture n has type (Cell T)` |

Termination: each step binds a variable, removes a constraint, or
replaces a constraint by an instance context whose types are proper
subterms of the instance head (the Paterson condition, checked on every
`impl`), so the worklist empties.

### 3.4 Field access and `deref` without annotations

`(. x f)` and `@x` on an unresolved `x` are legal as long as some use in
the same SCC fixes `x`'s head: a constructor argument, a call with a known
parameter type, a literal, or an annotation. Case 16's `transfer` is
resolved by the `Accounts` constructor in the closure body; case 19's
`add-child` by `(weak parent)` unifying with `(Weak Node)`; case 19's
`depth` has only a field access and a recursive call, so it annotates
`n`. There is no search for "the unique struct that has a field `f`"
(**Decided**).

### 3.5 Per-module algorithm

```
1. read; expand macros (macro-time modules are compiled first, syntax §3.16)
2. rewrite literal collections (syntax §1.4); resolve names against ns/require/use
3. collect declarations: structs, enums (check well-formedness, recursion annotations),
   protocols (record fundeps and escape kinds), impls (register instances with their declared
   contexts, check coherence and the Paterson condition),
   externs, defun names; run the syntactic & checks (§6.5, §6.9) that need no types
4. build the dependency graph of defuns and defs: an edge from a defun to every same-module
   defun it calls or names as a value and to every def it reads, from a def to every
   same-module defun it names as a value and to every def it names; compute SCCs (Tarjan);
   order dependencies before dependents. An SCC holding a def and anything else is the
   error `def g and defun f depend on each other` (§2.16)
5. for each SCC in order — a def by itself: type its initialiser (§2.16): a constant
   expression, a closed monomorphic type, no generalisation; a set of defuns:
   a. bind each defun to a fresh monomorphic function type at level ℓ+1
      (an annotated defun used polymorphically inside its own SCC is bound to its annotation)
   b. generate constraints for each body (§2), unifying eagerly (§3.2)
   c. run the deferred worklist to a fixpoint (§3.3)
   d. solve colour constraints (§5.4)
   e. generalise (§3.6); check ambiguity; report unresolved HasField/HasDeref
   f. run the ownership pass on the SCC (§6): the tail sites of §6.10; escape summaries,
      count kinds and heap closures as one fixpoint within the SCC (§6.4); then the tail-call
      decisions of §6.10 and the scope-local bindings of §6.11; then the same decisions
      for the all-owned body of each member, in which every object parameter is owned (§8.4)
6. type-check impl method bodies against their signatures under their declared contexts (§2.7);
   each impl is its own SCC and may call any defun, whose schemes are complete by now because
   instance contexts are declared, not inferred from these bodies (**Decided**)
7. check main : (fn () i64)
8. export the interface (§3.9)
9. monomorphise from main and from every impl reachable through a (dyn P) (§4.3)
```

Errors name the form whose rule generated the failing constraint; rules
are applied in evaluation order, so the reported position is the first
form that cannot be typed.

### 3.6 Generalisation at `defun` boundaries

After an SCC at level ℓ is solved: every type or colour variable of level
> ℓ that does not occur in Γ is quantified; constraints mentioning only
quantified variables go into the scheme's context; a quantified variable
with no constraints that does not occur in the type is dropped
(monomorphisation instantiates it to `i64`). `let` bindings and `fn`
parameters are never generalised. Polymorphic recursion (a `defun` used
at two instantiations inside its own SCC) requires the full annotation of
that `defun`; the annotation is then its scheme for the recursive
occurrences. Mutually recursive `defun`s get one scheme each.

### 3.7 How each feature is handled

- **Closures.** A `fn` gets fresh parameter types (or annotations); its
  body is typed in the extended Γ; its **capture set** (free variables
  bound in enclosing scopes, computed syntactically after expansion) is
  recorded on the node for §5.4 and §6.5. Its type is monomorphic in the
  enclosing function; type variables in it are quantified with the
  enclosing `defun`. Passing a closure to a parameter of known function
  type pushes the parameter types in by unification, which is how `(fn
  (c) (conj c i))` in case 10 gets `c : (Vec i64)` without annotation.
- **Cells.** `(cell e) : (Cell T)`; `@c` through `Deref`; `set!` unifies
  the stored type with the cell's. Cells stay simple because `let` never
  generalises (§2.4).
- **Atoms.** As cells plus `(Send T)` on construction; if `T` is still a
  variable at generalisation the bound `(Send a)` goes into the scheme,
  so a generic function that makes an atom of its argument requires a
  sendable argument.
- **Weak.** `(weak e)` emits `(Object T)`; `@w` resolves through the
  `Deref` instance to `(Option T)`.
- **Protocols.** A method call emits `(P τ θ̄)` with τ the receiver's
  type and the result typed with `self := τ`, `d̄ := θ̄`; resolution and
  improvement happen in §3.3 step 1; what remains after zonking is a
  direct call (§4.2) or, for a `(dyn P)` receiver, a vtable call (§4.4).
- **Generics.** Implicit: whatever is still a variable at generalisation
  is a parameter. Bounds are inferred. A generic `defun` is instantiated
  freshly at each call and monomorphised per layout-class instantiation
  (§4.3).
- **Structs with unannotated fields** are generic in those fields; a
  constructor application instantiates them.
- **`&` parameters.** `Γ(v) = (Cell T)`; `v` itself is not an
  expression (§2.14); the mode of the position is checked syntactically
  at call sites (§6.5); the unifier keeps `(& A)` distinct from `A`.
- **Async.** `async` bodies are typed like `fn` bodies with the result
  wrapped in `Task` and the colour forced to `send`; `await` unwraps.
- **Macros.** Typed as `defun`s over `Form` in the macro-time module;
  expansion happens before any of the above.
- **`def` names.** Globals like named functions: `Γ(g)` is the closed
  type of §2.16, never generalised and never a capture; typed in
  dependency order, after the functions the initialiser names and
  before the functions that read the `def` (§3.5).

### 3.8 What must be annotated

Inferred without annotation: all local types, `defun` parameter and
result types, closure types and colours, protocol constraints, `Send`
bounds, instantiations at every call, escape summaries, count kinds
(owned or borrowed, §6.4), tail calls (§6.10), escaping closures,
allocation sites (stack or heap, §6.11).

| Situation | Why | Annotation |
|---|---|---|
| recursive struct/enum fields | the occurs check; nominal recursion needs a declared type | the fields on the cycle |
| protocol method signatures | impls are checked against a contract that exists before any impl | full signature |
| a generic `impl` whose bodies need bounds on its type variables | contexts are consulted when defuns are generalised, before any impl body is typed (§3.5) | `:where` on the `impl` |
| `extern` | no body | full signature |
| polymorphic recursion | undecidable in general | the defun's full type |
| ambiguous constraint (`(count [])`-style, `Countable a` with no `a` in the type) | no principal instance | any use that fixes `a` |
| `(. x f)` or `@x` with `x`'s head unknown at the end of the SCC | field names do not determine the struct; `@` does not determine cell/atom/weak | `x: Name` |
| heterogeneous positions (a list of closures of different types) | HM has no subtyping | `(dyn P)`, or a struct |
| `main` | the entry contract | `-> i64` |
| a `def` whose initialiser leaves a type variable (`(def e [])`) | `def`s are closed and monomorphic (§2.16) | `(def e: (Vec i64) [])` |
| a `def` naming a generic function (`(def twice-fn double)`) | a `def` is closed, so the instantiation must be fixed (§2.16) | `(def twice-fn: (fn (i64) i64) double)` |
| a closure that must be sendable by contract (protocol callbacks, extern callbacks, struct fields) | an omitted colour in a field means `local` | `(fn :send ...)` |
| higher-rank use of a parameter (at two types) | rank 1 only | not expressible; restructure |

### 3.9 Across modules

A module exports, per definition: the closed scheme (with colour
variables and their `⊑` constraints, bounds, `&` positions), the escape
summary and count kind of every parameter (§6.4), for a `def` its
closed type (§2.16),
for structs and enums the layout
(§8), for protocols the signatures with escape kinds and fundeps, every
instance, every macro as forms, and the bodies of generic `defun`s
(needed by §4.3). Importing instantiates schemes; nothing is re-inferred
and no inference state crosses a module boundary, so modules are checked
independently once their dependencies are. Interfaces are
deterministic: the solver breaks every tie in source order. Coherence of
instances is re-checked at monomorphisation across all modules.

---

## 4. Protocol dispatch

Both static and dynamic dispatch exist; the type decides which.

### 4.1 Protocols and instances

`(defprotocol (P s d̄) ..)`: `s` is the dispatch parameter, `d̄` are
determined by `s` (one instance per head constructor of `s`, and it fixes
`d̄`). Instances are keyed by `(P, K)`; at most one per key in the whole
program (checked per module and again at monomorphisation). An instance
may carry a context (`(impl Eq (Vec a) :where ((Eq a)) ..)`), declared
on the `impl` and exported (§2.7). Instances for bare type variables and
overlapping instances are rejected (**Decided**; no default methods, no
supertraits in v1). Protocol
parameters are output positions: `(Deref (Cell i64) t)` yields `t = i64`
without annotation.

### 4.2 Static (the default)

A method call whose receiver has a concrete type after inference and
monomorphisation is a direct call to that type's implementation (lIR
`call` to a mangled name). A receiver whose type is a bounded variable
of the enclosing scheme has a concrete type in every specialisation,
because bounded variables are keyed by their full type argument
(§4.3), so this covers every method call outside `(dyn P)`.
Consequences: no dispatch cost, scalars unboxed in generic code, and
retain/release in generic code emitted knowing which values are
objects. The escape kinds a caller relies on
are the ones declared on the protocol (§6.4), so the caller never needs
to see the implementation.

### 4.3 Monomorphisation

Starting from `main`, from exported non-generic functions and from every
instance reachable through a `(dyn P)`, each call to a polymorphic
`defun` or generic `impl` method at instantiation `T̄` creates the
specialisation `f<T̄>` if absent, recursively. The key of a
specialisation has one component per quantified type variable of the
scheme (**Decided**):

- a variable that carries a **protocol bound** in the scheme's context
  (directly, or at the dispatch or a determined position of any of its
  constraints) is keyed by its **full type argument**, so that every
  method call on it inside the specialisation has a concrete receiver
  and is the direct call of §4.2;
- every other variable (unconstrained, or constrained only by `Send`)
  is keyed by the **layout class** of its argument: the scalar lIR
  types (`i1 i8 i16 i32 i64 float double`), `ptr` (every object type
  except the next), `opt` (an `(Option T)` with `T` of class `ptr`: a
  nullable pointer, §8.1), and the two-word `dyn`.

The separate `opt` class is what keeps the `Option` representation rule
of §8.1 intact under monomorphisation: `(Option a)` at `a` of class
`opt` or of a scalar class takes the heap-enum row, and only at `a` of
class `ptr` the null row. So `(Vec (Box i64))` and `(Vec str)` share
one specialisation of `count`, whose element type is unconstrained,
while `(defun describe (x) (str-len (show x)))`, of type `∀a. (Show a)
⇒ (fn (a) i64)`, gets one specialisation per receiver type
(`describe<Tag>`, `describe<(Box i64)>`, proposed case 45) in which
`(show x)` is a direct call; likewise the `=` of `(impl Eq (Vec a)
:where ((Eq a)))` is specialised per `a`, and a HAMT's `(hash k)` per
key type. Keying a bounded variable by layout class alone would leave
`(show x)` in a `ptr`-class specialisation with no implementation to
name, since a class is not a type, so §4.2's "always static" and
layout-class sharing cannot both hold for a bounded variable; the
full-type key is chosen, and the interpreter's dispatch on the header's
type id (§4.5) answers the same. Code size is bounded by the number of
distinct key tuples in use. Retain and release inside a `ptr`-class
specialisation go through the runtime functions of §8.2, which read
the type id from the header, so no per-element-type code is needed.
Whole-program compilation follows: a module's interface carries
generic bodies.

### 4.4 Dynamic: `(dyn P)`

`(dyn P)` is a two-word value `{ptr obj, ptr vtable}` (§8.5), created
only by `(dyn P e)`; the vtable is the global constant for `(P, head
type(e))`. A method call on a `(dyn P)` receiver loads the method's slot
and calls indirectly with `obj` as `self`. Methods whose signature
mentions `self` anywhere but the receiver position (`(conj (self x: e)
-> Self)`) are not callable through `dyn` (object safety); protocols with
determined parameters are usable as `(dyn (P D̄))` with `D̄` fixed. `(dyn
P)` is chosen only where the program writes it: heterogeneous
collections and plugin-style interfaces. It is not `Send` in v1 (§10
item 11).

### 4.5 The reference interpreter

The interpreter carries a type id in every object header (§8.2) and
dispatches every protocol call on it. That is observably identical to
static resolution because an instance is unique per `(P, K)`; the
interpreter needs no monomorphisation.

---

## 5. Threads: the `Send` predicate and closure colours (§7)

### 5.1 The predicate

**Decided** (§7): only immutable objects and atoms may cross a thread
boundary; a cell that is not an atom may not; everything reachable from
a crossing value crosses with it. Hence the type-level predicate
`Send(T) = "no value of type T can reach a Cell that is not inside an
Atom"`, computed structurally:

```
Send(scalar)              = true, except Send(ptr) = false      (a raw pointer's target has no header to mark)
Send(str) = Send(Form)    = true
Send((Array T))           = Send(T);  Send((Vec T)) etc. follow from their definitions as structs/enums
Send((N θ̄))               = ∧ Send(field or payload types with θ̄ substituted)   ; struct or enum
Send((Cell T))            = false
Send((Atom T))            = true          (well-formedness already required Send T)
Send((Weak T))            = Send(T)       (an upgrade on the other thread yields a T)
Send((Task T))            = Send(T)
Send((fn κ (Ā) R))        = (κ = send)
Send((dyn P))             = false         (v1)
Send(a) for a variable    = the constraint (Send a), kept in the scheme
```

For recursive nominal types `Send` is the greatest fixpoint (assume yes
for the type being examined, refute on finding a `Cell`), computed once
per definition as a function of its parameters: `Send (Vec a) = Send a`;
`Send Node = false` if any field is a cell.

### 5.2 Where it is required

`(spawn f)` requires `Send` of `f`'s type (κ = send) and of the result;
`(atom e)` requires `Send` of `e`'s type; `(async b)` requires `send ⊒
Caps{captures of b}` (§2.8, **Decided** by §3.4); `plet` and `pmap` reduce
to `spawn`, and `pmap`'s scheme carries `Send a` for the element type, so
`(pmap f [(cell 0)])` is rejected even when `f` is sendable. Nothing else
in the checker mentions threads.

### 5.3 The witness and the error

When `Send(T)` fails the solver records the path to the offending type:
the first chain of closure capture, field, payload, element or type
argument that reaches it. The message is

```
cell cannot be shared between threads: <path> has type (Cell T)
```

when the offending type is a `Cell`, where `<path>` is `closure capture
n` (case 13), `field children of Node`, `element of (Vec ..)` and so on;
otherwise `value of type T cannot be shared between threads: <path>`
(a `ptr`, a `local` closure captured by a closure that must be `send`, a
`dyn`). The first clause is the canonical text of case 13.

### 5.4 Closure colours (Decided, D4)

Every function type carries a colour κ ∈ {`send`, `local`} or a colour
variable ς, with `send ⊑ local`. Constraints:

```
ς ⊒ Caps{T₁ .. Tₖ}      at each (fn ..) and (async ..): T̄ are the types of its captures
κ₁ ⊑ κ₂                 at flow sites (§3.2), from the flowing function type to the receiving one
κ ⊑ send                at thread boundaries: the receiving type is (fn :send ..), so this is the flow rule
```

`ς ⊒ Caps{T̄}` means ς = `local` if any `Tᵢ` is not `Send`, and nothing
otherwise. A captured closure contributes its own colour (`Send (fn κ ..)
= (κ = send)`), so `κ_captured ⊑ ς`; a captured `(Cell ..)` contributes
`local ⊑ ς`.

Solving, after the SCC's type constraints:

1. Rewrite each `Caps` constraint: for each `Tᵢ` evaluate `Send Tᵢ`. False
   → replace by `local ⊑ ς` and remember the capture that forced it. True
   → drop. Undecided (a quantified `Send a` or a colour variable) → keep
   as the symbolic constraint `ς ⊒ send-of(Tᵢ)`.
2. Least fixpoint: set every colour variable to `send`; repeatedly, for
   every `κ₁ ⊑ κ₂` with κ₁ = `local`, set κ₂ := `local`, until stable.
3. Check: every `κ ⊑ send` with κ = `local` is the error of §5.3, with the
   witness found by walking the `⊑` chain back to the forcing capture.
4. Generalise: colour variables that stayed symbolic are quantified with
   their remaining constraints; a call site instantiates them freshly
   and re-solves with the caller's types.

Consequences: a user never writes a colour; `(defun twice (f) (fn (x) (f
(f x))))` gets `∀a ς₁ ς₂. ς₁ ⊑ ς₂ ⇒ (fn :send ((fn ς₁ (a) a)) (fn ς₂ (a)
a))`, so `comp`, `partial` and `twice` are usable in `pmap` iff their
arguments are; `(pmap f xs)` works for a parameter `f`; a closure over a
cell is `local` and is rejected only when it reaches a boundary. Colours
have no run-time representation.

### 5.5 Run time

`spawn` marks the closure and everything reachable from it **shared**
(§8.8) before the other thread can see it; `async` does the same for the
task's captures at creation; a task's result is marked by the producing
thread before it completes. Shared objects use atomic count operations
from then on; objects that never cross keep the non-atomic path
(**Decided**, §7). Because `Send` holds, the walk meets only immutable
objects, atoms (it marks their current content) and weak boxes (it marks
a live target, since the other thread could upgrade it). A value stored
into a shared atom is marked before the store (§8.6).

---

## 6. The ownership checker: how each rule of ownership.md is decided

The pass runs per `defun` (and per `fn` and `async` body, and per `impl`
method) after inference and before monomorphisation, on a body in which
every expression has a resolved type up to the `defun`'s own quantified
variables; a quantified variable is treated as an object for every rule
below, which is safe (its instantiations are handled by the runtime
functions of §8.2). The pass uses no information beyond the syntax tree,
the types, the capture sets of closures and the **summaries** of callees
(§6.4). It computes, and can print (§9), the **mode** of every
object-typed expression, the kind of every binding and whether it lives
on the stack (§6.11), the count kind of every parameter (§6.4), whether
each closure literal escapes and whether it is on the heap (§6.5),
whether each call in tail position is a tail call (§6.10), and the
retain/release operations to emit. Scalars are ignored throughout: a
rule that says "retain" is a no-op on a scalar-typed expression.

### 6.1 Modes and bindings

Every object-typed expression has one of three modes (**Decided**; the
calculus of the ownership draft):

| Mode | Meaning |
|---|---|
| `Owned` | the value carries a count that belongs to this expression; whoever consumes it must release it or hand it on |
| `Borrowed(b)` | the value *is* the value of binding `b`, with no count of its own; valid while `b` is |
| `Derived(b)` | the value is a part of `b`'s value (reached through fields, elements or pattern variables); valid while `b` is |

`b` is a **binding site**, never an object or a name: the checker reasons
about which binding keeps a value alive, not about aliases, and two
bindings holding the same object are two independent reasons it is
alive. Every binding is one of:

| Kind of binding | Reading it yields | Released at its scope end? |
|---|---|---|
| **owning**: a `let` binding whose initialiser is `Owned`; a capture of a heap closure (§6.5); an **owned** parameter (§6.4), which every object parameter of a `fn` is; a loop variable (§6.10); an implicit temporary | `Borrowed(b)` | yes, unless moved out |
| **borrowed parameter** (a plain parameter of a `defun` or method whose count kind is borrowed, §6.4) | `Borrowed(b)` | no (**Decided**, §4): a frame below holds it |
| **`&` parameter** (its value is its private cell, which is never an expression: syntax §3.13) | never read: `@v` is `Owned`, `&v` forwards the cell, `(set! v e)` writes it (§6.6) | no (the cell is the caller's) |
| **alias of `b'`**: a `let` binding whose initialiser is `Borrowed(b')`; a capture of a stack closure (§6.5); a pattern variable that binds the *whole* scrutinee (a top-level symbol pattern or a top-level `:as`) of a scrutinee whose mode is `Borrowed(b')`; the self-name `g` of a named `(fn g ..)` inside its body, an alias of the closure's `env` (§8.4), which is an owned parameter of the body (§6.4), and every occurrence of which is a use of the literal (§6.5) | `Borrowed(b')` | no count |
| **derived of `b'`**: a `let` binding whose initialiser is `Derived(b')`; a pattern variable bound *inside* a variant or struct pattern (a payload or a field) of a scrutinee whose binding is `b'`; a whole-scrutinee pattern variable of a scrutinee whose mode is `Derived(b')` | `Derived(b')` | no count |
| **global**: a `def` name (syntax §3.19, §2.16) | `Borrowed(g)` | never: its value is immortal (§8.2), so every count operation `consume` would emit on it is a no-op the compiler may omit |

`Derived(b)` always names a *strict* sub-object of `b`'s value, reached
through a field, an element or a variant payload; a pattern variable
that binds the whole scrutinee is the scrutinee's value and takes the
scrutinee's mode (**Decided**). The escape summary of §6.4
relies on exactly this distinction.

### 6.2 Mode of each form

| Form | Mode |
|---|---|
| string literal, `Form` literal | `Owned` (an immortal object: count 0 and the `IMMORTAL` flag, §8.2; count operations on it are no-ops, and `fib.unique?` is false on it, so an update through a place holding it or any part of it copies, §6.6) |
| variable naming a `def` | `Borrowed(g)`, `g` the global binding (§6.1); its value is immortal like a literal |
| struct or variant constructor call, rewritten `[...]`/`{...}` | `Owned` |
| call of a `defun`, protocol method, primitive or closure value returning an object | `Owned` (**Decided**, §4: results are owned) |
| `@c` on a cell or atom; `@w` on a weak | `Owned` (§6.7) |
| `(cell e)`, `(atom e)`, `(weak e)`, `(fn ..)`, `(async ..)`, `(spawn ..)`, `(join ..)`, `(await ..)`, `(swap! ..)` | `Owned` |
| variable `x` | as its binding says (§6.1) |
| `(. e f)` | `Derived(b)` if `e` is `Borrowed(b)` or `Derived(b)`; if `e` is `Owned`, `e` becomes an implicit owning temporary `t` of the current step and the result is `Derived(t)` |
| `(dyn P e)` | the mode of `e` |
| `(let ..)` | the mode of its body, adjusted by the scope-exit rule (§6.3) |
| `(loop ..)` | the mode of its body, adjusted by the scope-exit rule with the loop variables as its owning bindings (§6.10) |
| `(recur ..)` | no value: a tail call to the loop; its arguments are consumed into the loop variables (§6.10) |
| `(do .. e)` | the mode of `e` |
| `(if c a b)`, `(match ..)` | the **join** (§6.3) of the branch modes |
| `(unsafe b)` | the mode of `b` |
| `(set! ..)`, `(reset! ..)`, `(array-set! ..)`, `(set-field! ..)` | `unit` |

### 6.3 Where counts change: escape positions, consume, scopes, joins (§3)

**Decided** (§3): a reference escapes when it is (1) returned, (2) stored
into an object, (3) captured by a closure that escapes, (4) passed to
another thread, (5) held across an `await`, (6) passed as an argument
of a tail call. Each is a position in the syntax, and one rule applies
at every one of them:

```
consume(e):   Owned        → move: no operation; the count travels with the value
              Borrowed(b)  → retain
              Derived(b)   → retain
```

| # | Position | Forms |
|---|---|---|
| E1 | return | the value of a `defun`, `fn`, method or `async` body (after the body's own scope exits) |
| E2 | store | the object arguments of struct and variant constructors; `(cell e)`; `(set! c e)`; `(atom e)`; `(reset! a e)`; `(array n e)` (n copies); `(array-with a i e)`; `(array-set! &a i e)`; `(set-field! &s f e)`; the result of `f` in `(swap! a f)`; the operand of `(raw-retained e)` (§6.13). Stores inside library functions are those functions' business: `conj`'s element is stored by `array-with` inside `conj` |
| E3 | capture | each object capture of a heap closure (a `fn` that is escaping or at a tail site, §6.5) and of every `async`; the retain happens when the closure or task object is created |
| E4 | thread | the argument of `spawn` (consumed; E3 already retained its captures) |
| E5 | await | subsumed by E3: the task's frame owns everything it references (§6.9) |
| E6 | tail call | the head and the arguments of a tail call (§6.10; **Decided**, D5), consumed for the callee's positions because the caller's frame is discarded at the jump; an owning binding of the frame that is an argument is moved rather than retained (its count travels; a `recur`, which keeps the frame, moves only a binding it exits, §6.10), and an argument at a borrowed position emits nothing and must be frame-independent (§6.4). For classifying closures (§6.5) and parameters (§6.4 rule 2), the head and the arguments of every tail site count, admitted or not (§6.10, tail sites) |

Everything else emits no count operation:

| Position | Rule |
|---|---|
| argument at a **borrowed** position of a known callee, in an ordinary call (§6.4) | none (**Decided**, §4). An `Owned` argument is a temporary of the call step, released after the call returns and its write-backs are done |
| argument at an **owned** position of a known callee, or any argument of a call through a closure value, in an ordinary call | `consume`: an `Owned` temporary is moved in (it is then no temporary of the call step: syntax §2), a `Borrowed`/`Derived` one retained; the callee releases it or hands it on (§6.4), and the interpreter does the same (§6.12, exception 5). This is not an escape of the caller's binding: what the callee keeps is what its summary says |
| `let` binding `(x e)` | `e` `Owned`: `x` owns it. `e` `Borrowed(b)`/`Derived(b)` with `b` alive for `x`'s whole scope: `x` is an alias/derived binding of `b`, no count. `e` `Derived(t)` where `t` is a temporary of the initialiser step: the step is a scope (next row), so `x` owns a retained reference |
| **scope exit** of a `let` with body mode `m`, or of a step with temporaries | if `m = Borrowed(x)` and `x` is an owning binding of this scope: **move out**: `x` is not released and the result is `Owned`. If `m = Derived(x)` and `x` owns in this scope: retain the result, then release the scope's owning bindings; result `Owned`. Otherwise release every owning binding of the scope in reverse order; result mode unchanged |
| `match` scrutinee | `Owned`: an implicit owning binding `t` for the whole form; a pattern variable that binds the whole scrutinee is an alias of `t`, one bound inside a variant or struct pattern is derived of `t` (§6.1); the form's value is adjusted by the scope-exit rule with `t` as the scope. `Borrowed(b)`: whole-scrutinee variables are aliases of `b`, the rest derived of `b`. `Derived(b)`: every pattern variable is derived of `b` |
| `let` with a pattern | as a one-clause `match` whose scope is the `let` |
| **join** of `if`/`match` branches | all branches `Borrowed(b)` for one `b`: `Borrowed(b)`. All branches `Derived(b)` for one `b`: `Derived(b)`. Otherwise, including `Borrowed(b)` mixed with `Derived(b)`, `Owned`, and every branch that is not `Owned` gets a retain at its tail (**Decided** by case 04 and for the mixed case: read as `Derived(b)` it hid a `Borrowed(b)` occurrence from the escape summary of §6.4, and read as `Borrowed(b)` the scope-exit rule would move `b` out when the other branch had returned a sub-object of `b`, leaking `b`) |
| non-final `do` step | `Owned`: release at the step's end |
| `defun`, `fn` or `async` body (E1) | `consume` the body's value (every `let` and `match` inside it has already exited by the scope-exit rule, since they are expressions); an owned parameter whose value is the result is moved out, with no retain and no release; then release the temporaries of the final step, then the owned parameters not moved out (§6.4). A tail call in a `defun` or `fn` body replaces all of this by the rule of §6.10. An `async` body has no parameters and no tail calls (§6.10 rule (f)): its consumed value is the task's result, which the completion of §8.8 stores after the body's last step has returned |
| `@c` read | always an acquire (+1) with a matching release at the end of the value's scope or step; **never elided** in v1 (**Decided**). No elision of a cell read is sound on syntax alone: a callee can reach the same cell through any argument, field or capture and write it; a sibling sub-expression of the same step can write it (`(array-get (. @s arr) (do (set! s ..) 0))` freed the array under the call when the read was elided); and `count`, `nth` and `+` are protocol methods with user implementations, so "writes no cell" is not a property the checker can decide by name |
| `&` copy-in and write-back | §6.6 |
| `(weak e)` | no count operation; forces `e`'s binding onto the heap (§6.11) |
| `(raw e)` | no count operation; `e`'s binding must outlive every use of the pointer (**Decided**, §9; the programmer's obligation inside `unsafe`) |

These two tables *are* the escape classification. The checker walks the
tree once, labels each object-typed expression with its position, and
emits the operation from the tables; nothing else emits a count
operation. Case 01: `first`'s body returns `Derived(self)` → E1 retain;
`head`'s `(first xs)` is a tail call whose argument, a borrowed
parameter, emits nothing (§6.10), and its `Owned` result is `head`'s;
`main`'s `h` owns one count and `l`'s list holds the other; both
released at scope exits. Case 03:
the inner `let` binds `a` (`Owned`) and `b`; its body is `Borrowed(a)` →
move out, `b` released; `keep` owns `a`'s vector. Case 04: `pick`'s `if`
joins `Borrowed(x)` with `Owned` → retain on the `x` branch; the result
is `Owned` on both paths.

### 6.4 Borrowed and owned parameters, owned results, summaries (§4)

**Decided** (§4, as amended by D5): every result is owned by the caller.
Every object parameter has a **count kind**, `borrowed` or `owned`:
inferred per `defun` and exported with it (§3.9), declared on protocol
methods (`:owned`, default borrowed; syntax §3.10), and fixed at owned
for closures. The kind is a calling convention that the interpreter
follows as well as the compiler (§6.12, exception 5). It moves where
some objects are freed — an `Owned` argument handed to an owned
parameter dies at the callee's exit or jump, not at the end of the
caller's step — and so the count a unique write sees, which is why
`fibref` and `fibc` must compute the kinds identically: both run this
checker. The counting semantics of §2 holds under either kind, the
two implementations free the same objects at the same points, and no
result or verdict depends on the kind.

- A **borrowed** parameter is passed with no count operation and is
  valid for the whole call because the caller, or a frame below it,
  holds a count on it; the callee never releases it. It never occurs
  as `Borrowed(p)` at E1–E4, in a join retain or under `raw-retained`,
  since rule 1 below would make it owned; what the callee counts is a
  part of it (`Derived(p)` at a consume position is retained, as in
  case 01) or the count it hands to an owned position of an ordinary
  call (retained, §6.3).
- An **owned** parameter arrives with one count: the caller `consume`s
  the argument (moves an `Owned` temporary, retains a `Borrowed`/`Derived`
  one, §6.3); the parameter is an owning binding of the callee's frame
  (§6.1), released at the callee's exit on every path unless it is
  **moved out**, which happens in exactly two places: it is itself the
  function's value (the E1 row of §6.3; a join that returns it retains
  instead, case 04) or an argument of a tail call (§6.10 step 2), and
  there its count travels. Anywhere else it is consumed like any
  `Borrowed` value: stored (E2), captured by a heap closure (E3) or
  passed to `spawn` (E4), it is retained, and the exit still releases
  it, since it stays readable after the store. `(do (set! c b) (set! c
  (Box 0)) (unbox b))` reads `b` after the cell has let go of it, which
  a store that moved `b`'s count would have made a read of freed
  memory, and `(Two b b)` would have put one count behind two fields
  (proposed cases 72, 73). A move at a store would be sound only as the
  parameter's last use on every path, and only once, which is
  flow-sensitive reasoning the checker does not do. Correctness never
  depends on the caller: whoever holds a count releases it or hands it
  on, and an ordinary call is never an escape for the caller.

At the machine level, then, every parameter is owned in the sense that
the callee could release it; `borrowed` is the inference that the
callee never needs to, so that the retain in the caller and the release
in the callee can both be dropped, which is §2's "passing an argument
costs nothing".

**Which parameters are owned.** A parameter `p` of `f` is owned iff at
least one of:

1. `Borrowed(p)` occurs at a consume position other than a tail-call
   argument: E1 (returned, directly or as the value of a scope exit that
   moves `p` out), E2 (stored), E3 (captured by a heap closure), E4
   (`spawn`), a join retain (§6.3), or the operand of `raw-retained`; or
   `Borrowed(p)` is consumed into a loop variable to which this rule
   applies (loop variables, below);
2. `Borrowed(p)` is an argument of a **tail site** (§6.10) at an
   **owned** position of the callee, whether or not rule (e) then makes
   the site an ordinary call: admitted, the call moves the count on
   rather than leaving it to a frame that is about to disappear, and
   reading the site rather than the admission keeps the fixpoint below
   monotone (§6.10, tail sites; proposed case 74);
3. some tail call *to* `f` from the body of a `defun` of `f`'s SCC
   passes, at `p`'s position, a **frame-owned** argument: the caller's
   frame is discarded at the jump, so nothing but `f` can release that
   count. (Every tail site in such a body that calls a member of the
   SCC is a tail call: rule (e) concerns only callees outside it.) A
   `fn` literal or an `async` body nested in such a body belongs to no
   SCC: a `fn` body's tail calls feed no kind back, and rule (e) of
   §6.10 treats every `defun` it calls as outside the current SCC; an
   `async` body has no tail calls at all (§6.10 rule (f)).

An argument expression is **frame-owned** iff it is `Owned` and not
immortal (a fresh object, a call result, a cell read), or it is
`Borrowed(x)` or `Derived(x)` for a binding `x` that the caller's frame
releases at its scope exits or before a tail call's jump: an owning
`let` binding, a loop variable, a `match` temporary, an owned parameter
(every object parameter of a closure body, and its `env`, of which the
self-name is an alias), an implicit temporary, or a capture of a
**heap** closure. Such a capture is reached through `env`, an owned
parameter that the closure body releases at every exit, a tail call's
jump included (§6.10 step 3, §8.4), so it is in effect `Derived(env)`:
when the closure's count is one, releasing `env` frees the closure, and
its drop releases the capture. It is not frame-owned when it is a
borrowed parameter of the caller or derived from one (a frame below
keeps it alive for the whole chain of tail calls), a `def` global, a
literal, or a capture of a **stack** closure (an alias of a binding of
the creating frame, which is below the closure's frame and alive for
the whole call, §6.5). Whether a literal is heap or stack is fixed
before its body's tail calls are decided (§6.5), so the rule is known
inside every body.

The kinds, the escape summaries below and the heap closures of §6.5 are
**one least fixpoint** over the module's call graph, per SCC in
dependency order, starting from `borrowed`, `noescape` and stack. All
three only grow, and each depends on the others monotonically, because
none of them depends on which calls in tail position are admitted: rule
2 and the heap rule of §6.5 read *tail sites*, which are fixed before
the fixpoint starts, and rule (e) of §6.10, the one rule that reads
kinds and heap closures to decide a call, is applied after it (§6.10,
tail sites). Read as one fixpoint with the admission of tail calls, the
three had no consistent solution for some programs: a literal made heap
by a tail call captured a parameter, which became owned, which made the
call ordinary by rule (e), which left the literal stack and the
parameter borrowed (proposed case 74). A heap closure's capture is E3
for the kinds and the summaries alike, a closure at a tail site is heap,
and a tail call that turns a callee's parameter owned (rule 3) can turn
the argument's binding, if it is a parameter of the caller, owned by
rule 2. So the three are one iteration, not three passes, exactly as the
escape summaries alone were before D5. Imported functions, protocol
methods and closure types are fixed inputs: an imported `defun` carries
the kinds its module inferred (§3.9), a method the kinds its protocol
declares, and a closure value owns every parameter and its environment.
A tail call to a callee outside the current SCC cannot change the
callee's kinds; rule (e) of §6.10 makes such a call an ordinary call
when it would need to.

**Closures** (`fn` literals, `async` bodies, named-function values):
every object parameter of a closure is owned, and so is the closure
object itself for the duration of a call through it — the environment
is an argument like any other (§8.4). A call through a function value
cannot know which body it reaches, so one convention serves all, and
D5 fixes it as the owned one so that a tail call through a closure
value discards the caller's frame safely. The parameters of a `fn` are
owning bindings of its body; its callers, direct or through a value,
consume every object argument. The immortal closure of a named
function does not adapt the convention with a frame of its own: its
code is the function's **all-owned body**, the body compiled a second
time with every object parameter owned (§8.4), so a call through it
keeps the closure convention from end to end.

The pass attaches to every `defun` an **escape summary**: per object
parameter `p`, `escapes` or `noescape`. `p` escapes iff an occurrence of
`Borrowed(p)` is at a consume position (E1–E4, a join retain), or is
passed to an `escapes` parameter of a known callee (in a tail call or
not), or to any parameter of a closure value (unknown callee), or is
captured by an escaping closure (§6.5), or is the operand of `weak` or
`raw-retained`, or is consumed into a loop variable that escapes (loop
variables, below). An E6 occurrence by itself is not an escape: the
callee's activation replaces the caller's, the frame below, which holds
every borrowed parameter of the caller, is still waiting, and what the
callee does with the argument is what its own summary says; a `recur`
argument is followed into its loop variable (below). An occurrence of
`Derived(p)` never counts: a derived value is a strict sub-object of
`p`, reached through a field, an element or a variant payload (§6.1),
and is always a heap object because it was stored into its container at
E2. A pattern variable that binds the whole scrutinee is `Borrowed(p)`,
not `Derived(p)`, and a join that mixes the two modes retains its
`Borrowed(p)` branch, which is a consume position (§6.3); so
`(defun same (p) (match p (w w)))`, `(match p ((Node inner) inner)
((Leaf) p))` and `(match p (x (weak x)))` all make `p` escape
(**Decided**). Imported and protocol summaries are fixed inputs. An
explicit `p :borrow` or `p: T :borrow` on a `defun` parameter (syntax
§3.1) fixes the summary at `noescape` and is checked: `parameter p of f
is declared :borrow but escapes`; the exported summary is then the
declared one, part of the interface, and a closure literal passed to `p`
is non-escaping by contract (§6.5; proposed case 40).

**Loop variables.** A `loop` is a local function whose parameters are
its variables (syntax §3.18), so a value consumed into a loop variable
`v` — by an initialiser, or by a `recur` argument naming a binding
outside the loop, which the `recur` retains (§6.10) — is followed
through `v` instead of being lost at that retain. `v` is summarised like
a parameter, in the same fixpoint, with `Borrowed(v)` as the loop's
value, which the loop's scope exit moves out (§6.10), counting as E1. A
`Borrowed(b)` consumed into `v` then counts as `v`'s occurrences do: it
makes `b` escape iff `v` escapes, and makes a parameter `b` owned iff
rule 1 applies to an occurrence of `Borrowed(v)`; and the same again
through a loop variable into which `v` is itself consumed (a swap by
`recur`, a nested loop). So `(defun last-or (default xs) (loop ((best
default) (i 0)) (if (< i (count xs)) (recur (nth xs i) (+ i 1))
best)))`, which returns `default` through `best`, makes `default`
escape and owned, as `(defun id (x) x)` makes `x`. Without the rule
`default` was `noescape`: a caller's scope-local object passed there
came back as a pointer into a dead scope, and a closure literal passed
to such a parameter was a stack closure returned out of the call, even
one capturing an `&` parameter (proposed case 75).

Protocol methods declare their kinds (syntax §3.10; default escaping
and borrowed); every `impl` body is checked against the escape
declaration: `implementation of P/m for T makes parameter p escape;
the protocol declares it :borrow`. A body is compiled with the declared
count kind whatever it would have inferred, which can cost it a retain,
or a tail call by rule (e), but never a verdict, so `:owned` needs no
check. Callers use the declared kinds whichever implementation runs,
which keeps summaries modular under dynamic dispatch and separate
compilation (**Decided**). Externs take scalars only: `(raw e)` is not
an escape of `e`'s binding, `(raw-retained e)` is (§6.13).

What summaries and kinds decide, and nothing else: (a) whether a
closure literal passed to that parameter is escaping (§6.5; at a tail
site it is heap whatever the summary says); (b) whether a closure that
captures an `&` parameter may be passed there (§6.5); (c) whether the
caller's argument object may live on the stack (§6.11); (d) whether a
call in tail position is a tail call (§6.10 rule (e)) and which count
operations a call emits (§8.9). Summaries and kinds of `fn` literals
are computed for their own bodies but never exported: closure types
carry none, so a call through a function value assumes every argument
escapes and hands every argument over owned.

### 6.5 Closures: escaping, heap, capture modes, the two `&` rules (§3.3, §5)

Every use of a `fn` literal — an occurrence of the literal itself, of
the `let` binding `f` it directly initialises, or, for a named `(fn g
..)`, of its self-name `g` inside its body (§6.1) — is one of:

- (a) a direct call at the literal: `((fn ..) args)`; for the
  self-name, a call `(g args)`, in tail position or not, since a self
  tail call hands `env` on unchanged (§8.4) and the frame that holds a
  stack closure, the creating frame below the call, is not the one it
  discards;
- (b) an argument to a `noescape`/`:borrow` parameter of a known callee
  (`swap!`, `for-each`, `map`, `reduce`, a `defun` whose summary says
  so, which follows the parameter through the callee's loop variables,
  §6.4), in a tail call or not;
- (c) the *direct* initialiser of a `let` binding `f` every use of which
  is a call `(f args)` or a use of kind (b), and which no other closure
  captures;
- (d) anything else: E1, E2, E3 (captured by an escaping closure), E4, an
  `escapes` parameter, an argument or the head of a call through a
  closure value (unknown callee), an `async` body; for the self-name,
  also a capture by another closure and any binding of it (a `let`
  initialiser, a `match` scrutinee).

A closure is **escaping** iff it has a use of kind (d): it may outlive
the call that created it. A closure is **heap** iff it is escaping, or
it is the head or an argument of a **tail site** — E6; a call in tail
position that §6.10 may admit as a tail call, fixed before the kinds are
(§6.10, tail sites) — that is, a use of kind (a), (b) or (c) whose frame
may be discarded while the closure is still needed (**Decided**, D5,
generalising the earlier rule for self tail calls): `(defun spin (g n)
(let ((x (make))) (if (= n 0) (g) (spin (fn () (count x)) (- n 1)))))`
has `g : noescape`, but the literal captures `x`, which the `let`
releases before the jump; as a heap closure it retains `x` at E3 and
frees it with itself, after its own body has read it (proposed case 49;
that body's `(count x)` is an ordinary call, §6.10 rule (e)). A tail
site counts whether or not §6.10 then admits it, so that heap or stack
is fixed before the tail calls that depend on it are decided (§6.4,
frame-owned): a literal at a tail site that rule (e) turns into an
ordinary call is heap all the same, which costs an allocation and never
a verdict (proposed case 74). For the self-name, the head of its own
self tail call is not E6 (use (a) above), while an argument of a tail
site of the body is E6, as for any use. A closure that is neither is a
**stack** closure. The check is syntactic and uses only summaries. The
self-name is classified with the literal's other uses, so
`(fn go (n) (Rec go n))`, which stores its own closure (E2), is escaping
and on the heap whatever the uses of the binding that holds it;
classified by that binding alone it would be a stack closure storing a
pointer into its creating frame (proposed cases 59, 60). A `loop` body
(syntax §3.18) is not a closure: its free variables are the enclosing
function's own bindings and it has no capture set.

- **Heap closure:** E3 — every object capture is consumed (retained if
  `Borrowed`/`Derived`, moved if `Owned`, which cannot happen for a
  variable) when the closure object is created; the captures are owning
  bindings of the closure, released by its `drop`. A captured cell is
  retained like any object: in case 05 both closures retain `n`, the
  `let` releases its own count, and the cell lives while either closure
  does. A capture of an `&` parameter is the private cell, which is
  never counted (§6.6).
- **Stack closure:** its captures are alias bindings of the enclosing
  bindings; no count; the closure object lives in the creating frame
  (§6.11, §8.4). Sound because uses (a)–(c) all complete before the
  creating scope exits and a `:borrow` callee never stores or returns
  it; a tail call is the one call that does not complete before the
  scope exits, which is why E6 makes the closure heap.
- **Parameters** of every closure are owned (§6.4), and so is the closure
  object during a call through it; the body releases both at its exits
  unless it hands them on.

**Decided** (§5, case 18): an `&` parameter may not be captured by an
escaping closure. Check: for every escaping `fn` literal whose capture
set contains an `&` parameter `v` of the enclosing `defun`: `& parameter
captured by escaping closure: v in f`. A non-escaping closure may
capture `v` and use `&v`, `@v` or `(set! v ..)` (case 08), and may be
heap: passed at a tail site to a `noescape` position, or called in
tail position, it is released by the callee's release of its parameter
or by its own exit, before the write-back in the frame that created the
private cell, which is the frame below the one the tail call
discarded (at a tail site that is not admitted, the call is ordinary
and the closure is released within it, as at any call).

**Decided** (§5, case 12): the `&` arguments of one call name distinct
variables. Check, before typing: the list of `&` argument variables of
a call has no duplicates: `variable x passed to more than one &
parameter in call to bar`. Purely syntactic; aliasing of *objects* is
fine because both callees start from a copy-in that leaves the count
above one, so each update copies (§5).

### 6.6 `&` in detail: private cells, copy-in, unique updates (§5)

Inside the callee an `&` parameter `v` names its **private cell** and is
never itself an expression (syntax §3.13, §2.14; **Decided**, D2): `@v`
yields an `Owned` reference to the cell's content (an acquire, +1),
`(set! v e)` consumes `e` into the cell and releases the old content,
`&v` forwards the cell. The cell object itself is a stack temporary of
the *caller's* call step and is never counted (§8.6); because `v` has no
value, no expression, store, capture or return can refer to the cell
after the write-back frees it.

**Copy-in** at a call `(f .. &x ..)` of a `defun`, in argument order
(**Decided**, D1): the private cell is initialised with `@x` — an
**acquire**, +1 on the content, which the caller's cell `x` keeps
holding. There is no move: a place is a variable, never a field;
nothing is ever taken out of anything; the object in `x` is shared by
the two cells for the duration of the call, and its count says so. The unique-write
primitives below therefore copy on the first update through an `&`
parameter, and the copy, held by the private cell alone, is unique
from then on. The primitives themselves take no copy-in and no
write-back: `&x` hands them `x`'s own cell, whose content they test
and update (below), so an object that a `let` cell alone holds is
updated in place from the first update (syntax §3.13).

**Write-back** after the call returns, in parameter order: the private
cell's content is moved into the variable with `set!` semantics (the
variable's old content is released), then the private cell is freed
(no count). The interpreter implements exactly this, so the compiler's
allocations match its allocations (§6.12).

**Forwarding at a tail call** (§6.10 rule (b); **Decided**, D5 as
amended by the owner): at a call in tail position, an argument `&v`
where `v` is an `&` parameter of the enclosing `defun`, at any `&`
position of any callee, performs no copy-in and no write-back: the
callee's parameter is the same private cell, which belongs to a frame
below the caller, and the one write-back is that frame's, after the
whole chain returns. The distinct-variables check (§6.5) already forbids
forwarding one cell twice. Any other `&` argument (a `let` cell, a cell
reached another way) makes the call an ordinary call. Without this rule the write-backs of a
recursion's iterations, which syntax §2 places "after the call
returns", would need a frame each.

**Unique write** (**Decided**, §5): `array-set!` and `set-field!` read the
place's content without retaining, test `fib.unique?` (§8.2: the flags
have none of `SHARED`, `IMMORTAL`, `STACK`, and the count is exactly
1), and either write the object in place or build a copy with the
change, store it into the place and release the old object. The flag
test comes first and is what protects static data (**Decided**): an
immortal object (a literal, a `def` value, and every object reachable
from one) has no count, so a value pulled out of a literal (`(match f
((List xs) xs))` on a `Form` literal, §6.2) and acquired into a
private cell is copied by the first `push!`, never written, and the
literal's tail array, immortal too, copies likewise. Without the flag
test an immortal count that happened to read 1 would let accepted code
write into a constant, which in the compiler may live in read-only
data once lIR holds static constants (§8.11), while the interpreter,
whose literals are ordinary allocations, would answer differently
(method.md rule 6; proposed case 37). The audited heap must permit a
write to an immutable object exactly under that test (`write-unique`,
an obligation on fibref; §6.12), so the interpreter allocates its
literals with the `IMMORTAL` flag and count 0, like the compiler's
constants (§8.2). A unique write closes a cycle only through a cell:
the object it writes has count 1, and that one counted reference is
held by the cell of the place written (the variable's cell, or the
private cell of an `&` parameter), so every path of counted references
from another object to it passes through that cell, and so does every
cycle the write closes. `(let ((c (cell (K nil)))) (set-field! &c back
(some c)))`, with `K`'s field `back: (Option (Cell K))`, writes the
unique `K` in place and ties `c → K → c`, a cycle through the cell `c`
that the audit reports as `leak-cycle` (proposed case 80).

Count trace for case 08, compiler and interpreter alike. `main`'s cell
`v` holds `V0` (1); `dup-all`'s `v` is the private cell `P`. `(dup-all
&v)`: the copy-in acquires: `P` holds `V0` (2). `(for-each @v ..)`:
`@v` acquires (3) for the call step; the call is an ordinary call, and
not even a tail site (§6.10 rule (e) through fresh arguments at
`for-each`'s borrowed positions, whatever the kinds), so the closure is
a stack closure (`for-each`'s `f` is `:borrow`) capturing `P` as an
alias. Iteration 1: `(append &v x)`: acquire into `append`'s private
cell `Q` (4); `append`'s `push!` sees a count above one → copies: `Q`
:= `V1` (1), `V0` (3); write-back into `P`: `P`'s old `V0` released
(2), `P` := `V1`. Iteration 2: `Q` := `V1` (2), copy → `V2` (1), `V1`
(1); write-back releases `V1` (0, freed), `P` := `V2`. Iteration 3
likewise frees `V2`; `P` := `V3`. `for-each` returns; the step's
temporary `V0` released (1). `dup-all` returns; write-back stores `V3`
into `main`'s `v`, releasing its old `V0` (0, freed); `P` is freed.
`(count @v)` = 6; `main`'s `let` releases `V3`; nothing live at exit.

Case 17: `(push-count &v @v)`: the copy-in at the first argument
acquires (2), then `@v` acquires (3); inside, `(append &v (count x))`
acquires again (4), `push!` copies (`V1` into `append`'s cell, `V0`
back to 3), and the write-back into `push-count`'s cell releases `V0`
(2); the write-back into `main`'s cell releases `V0` (1) and stores
`V1`; the temporary `@v` is released after the call (0, `V0` freed);
result 4; clean.

### 6.7 Cells, weak references, cycles (§6)

- `(cell e)`: E2 for `e`; the cell owns one count of its content.
- `@c`: an acquire, `Owned` (+1) — the same rule as for atoms
  (**Decided** for atoms, §7, and for cells), so that `(let ((x @c))
  (set! c y) x)` can never read freed memory. Never elided in v1 (§6.3).
- `(set! c e)`: E2 for `e`, then release the old content. Order matters
  for `(set! c (f @c))`: the acquired temporary keeps the old value alive
  through `f`.
- **Cycles.** **Decided** (§6): a cycle through cells is garbage the
  implementation need not reclaim; the checker rejects nothing. The
  audit (`fibref` `heap/audit.rs`) classifies every object still live at
  exit: it is a **cycle leak** iff it is reachable from a cell or atom
  that lies on a cycle of the live-object graph and its count equals the
  number of live references naming it; any other live object is a
  `leak`, a failure; a cycle through no cell is `ImmutableCycle`, a bug
  in the implementation (an immutable object is built referring only to
  objects that already exist, and a unique write, the one way it changes
  afterwards, closes a cycle only through the cell that holds the object
  it writes, §6.6; so every cycle a program can make passes through a
  cell). Case 15: `k` (1 from its `let`) → cell → `[k]` (E2 retains
  `k`: 2) → `k`; the `let` exit releases one (1); the SCC {Knot, cell,
  Vec} contains a cell → `leak-cycle` and nothing else.
- `(weak e)`: no count operation; `e`'s binding is forced onto the heap
  (§6.11: a stack object cannot be observed dead by a weak box); the
  first `weak` of an object allocates its box and sets `HAS-WEAK` with
  an atomic or, since the object may be shared (§8.7, §8.2); on an
  `IMMORTAL` object (a literal, a `def` value, a named function used as
  a value) it allocates a box that is never cleared and leaves the
  object's header alone, so `@w` on it is always `(some ..)`: the
  upgrade tests the flag before the count, which is 0 on an immortal
  (§8.7; **Decided**). `@w`: atomically "retain if still alive" (§8.7); result
  `(Option T)`, `Owned`. Case 20: the inner `let` releases the only count
  → the drop clears the box → `@w` is `nil`. Case 19: the only strong
  edges are `root → cell → [a] → cell → [b]`; parents are weak; `main`'s
  `let`s release `b`, `a`, `root` in reverse order, each freeing what
  only it holds; the boxes die with their last `Weak` value; clean.
- **Immortal objects** (literals and everything reachable from them,
  `def` values, named-function closures, vtables; §8.2) are static
  data — in v1 built once by the module initialiser before `main`,
  §8.2 — not audited allocations: the audit does not track them, they are not
  live objects at exit, and nothing counted is ever reachable from one
  (a constant graph is closed), so they take part in no leak, no cycle
  and no unique write.

### 6.8 Threads (§7)

Decided by `Send` at the type level (§5) and by E4 at the ownership
level. `(spawn f)`: `consume(f)`, mark shared, hand to the runtime; the
thread owns that count on the closure and releases it when the thunk
returns. The task object is created with count **2** (**Decided**): one
is the caller's `Owned` result, one is held by the running
thread and released as its last action, after the result is stored and
the state set to done (§8.8). So a task whose handle is discarded (a
non-final `do` step, syntax Open decision 15) or released before `join`
is freed by whichever holder releases last, never while the thread still
writes into it. `join` retains the result for the caller. A task may be
joined by several threads and awaited by several tasks (`Send (Task T)
= Send T`, §1.6): the counts they hold keep it allocated, and §8.8
makes the runtime, not a count, the guarantee that only one of them
ever resumes it (**Decided**; proposed cases 43, 44). `main`
returning waits for every spawned thread still running, so the audit
runs on a quiescent heap (**Decided**). Case 13 is rejected
during typing (§5.4) before this pass runs. Case 10: `a : (Atom (Vec i64))` is `Send`;
each `@a` acquires under the atom's lock (§8.6, the implementation
obligation of §7); `swap!` stores `f`'s `Owned` result (E2, moved) and
releases the old vector, which the reader's `snapshot` still holds until
its `let` exits. Case 16 is the same with a struct. Atomic counts: the E4
walk and every store into an already-shared object set the flag (§8.8).

### 6.9 Async (§8)

**Decided** (§8, §3.4, §3.5; confirmed as D7): an `async` form is an
escaping closure whose captures must be sendable (§2.8, §5.2), because
the executor is multi-threaded: a task is resumed on whichever worker
thread picks it up, so its creation is a thread crossing (§8.8).
Mechanism: E3 for every capture
at task creation (this is "retained on entry"); the body runs in a
heap-allocated frame owned by the task; every local of the body lives in
that frame; so nothing in the body is `Borrowed` of an enclosing
binding, only of the task's own captures, which the task owns, and there
is no rule to check at `await` itself. Every capture is retained at
creation and released by the task's drop, whether or not its uses
precede the first `await`: evaluating the `async` form does not run the
body (syntax §3.14), so a use before the first `await` may still run
after the creating scope has released the captured binding; the earlier
idea of skipping such retains is withdrawn (**Decided**;
proposed case 24 in `spec/drafts/PROPOSED_CASES.md`). A `loop` body (syntax §3.18) inside
the `async` is part of the task's state machine, so a loop may `await`
on every iteration.

The body's value is the task's result. The E1 row of §6.3 applies at the
end of the body with the completion of §8.8 in place of a return: the
value is consumed, share-marked and stored into `result`,
`state := done` is published and the waiters are woken, all after the
body's last step has returned. So a call in tail position of an `async`
body is an ordinary call, never a tail call (§6.10 rule (f); proposed
cases 70, 71). An `async` body has no parameters, and it never releases
its own task: the task is no closure's `env`, and its counts belong to
its holders (§8.8).

**Decided** (§8): `&` parameters are not allowed in async functions.
Check, before typing, on a `defun` with an `&` parameter `v`: if any
`async` form in its body (including inside nested `fn`s) mentions `v`
(as `@v`, `&v` or the target of `set!`, anywhere inside the
`async` body), or an `async` form is in tail position of the body (the
function's value is a task it creates), the function is an **async
function** (syntax §3.1) and the error is `& parameter in async
function: buf in fill` (case 14: `fill`'s `async` both mentions `&buf`
and is its value). It runs before the escaping-capture check of §6.5,
so case 14 reports this text and not case 18's. An `async` that
mentions no `&` parameter and is not the function's value, such as a
task driven to completion by a `block-on` inside the call (`(defun
bump (&v) (let ((n (block-on (async (do (await (yield)) 1))))) (push!
&v n)))`, proposed case 38), does not make the function async: the
task cannot reach the private cell, because no expression has the cell
as its value (§2.14), and a closure that captures `v` and is captured
by the `async` is escaping, rejected by §6.5 and, being `local`, by
`Send` (§5.4). The earlier definition, any `defun` containing an
`async`, rejected that safe shape for no reason (**Decided**). `await`
outside `async`, or inside a `fn` nested in one,
is `await outside async`; inside a `loop` of the `async` it is fine.

Case 11: `measure` creates a task capturing `s` (retained); `main`'s
inner `let` releases `s` (1, held by the task); `block-on` drives the
task; `(length s)` reads the task's capture; the task is freed after
`block-on` returns its value, releasing `s`; clean.

### 6.10 Tail calls (Decided, D5)

A call is in **tail position** of a body when it is the last step of
the body, a branch of a tail `if`/`match`, the body of a tail `let`,
`do` or `loop`, or a `recur`, transitively; never inside a `fn` or
`async` literal nested in the body. A call in tail position is a **tail
call** — the caller's frame is discarded and the callee's result is the
caller's — unless one of the following holds, in which case it is an
ordinary call followed by the releases of §6.3:

- (b) it has an `&` argument that does not forward an `&` parameter of
  the enclosing `defun`: that write-back must run after the call
  returns (§6.6). An argument `&v`, `v` an `&` parameter of the
  enclosing `defun`, at any `&` position of any callee, forwards the
  private cell — no copy-in, no write-back — so a call whose every `&`
  argument forwards stays a tail call (mutual recursion threading an
  in-out accumulator runs in constant stack); every other `&` argument
  (a `let` cell, a cell reached another way) makes the call ordinary;
- (e) an argument at a **borrowed** position of a callee outside the
  current SCC (an imported `defun`, a protocol method, a `defun` of an
  earlier SCC; from a `fn` body, which belongs to no SCC, every `defun`;
  an `async` body belongs to none either, and has no tail calls, rule
  (f)) is frame-owned (§6.4): the caller's frame must survive the call
  to release it, and the callee's kinds cannot be changed from here.
  Inside the SCC the fixpoint makes the position owned instead (§6.4
  rule 3), so a self or mutual tail call between `defun` bodies is
  always a tail call; a callee that is a closure value owns every
  parameter, so this never applies to it. A capture of a heap closure is
  frame-owned (§6.4), so in the heap closure `(fn () (count v))` the
  call `(count v)` is ordinary: as a tail call it would release `env`,
  and with it `v`, before `count` ran (proposed case 57);
- (f) it is in tail position of an `async` body, through a `loop` there
  too (a `recur` is not concerned: it keeps the frame): the body's
  value is the task's result, which the task's completion stores,
  share-marks and publishes after the call has returned (§6.9, §8.8),
  so something of the task runs after the call and its frame, the task
  object, cannot be left. An `async` body therefore has no tail calls
  and, like a `fn` body, belongs to no SCC (§6.4 rule 3). As a tail
  call, the `(k)` of `(async (k))` jumped out of `resume` with the task
  unfinished: its result was never stored, `state` never became
  `done`, and a holder driving it waited forever or, read as a closure
  body releasing its `env` before the jump, read a freed task (proposed
  cases 70, 71);
- it is a call to an `extern` (§6.13): a foreign convention, and its
  `raw` borrows are valid for the enclosing scope.

**Tail sites: the order of the decisions.** Rules (b) and (f) and the
extern rule are syntactic. Rule (e) is not: whether an argument is
frame-owned depends on the kinds of the caller's parameters and on
which closures are heap (a capture of a heap closure is frame-owned,
§6.4), and both of those depend on the calls in tail position — the
head or an argument of a tail call makes a closure heap (E6, §6.5), and
an argument at an owned position makes a parameter owned (§6.4 rule 2).
Decided together with the admission of calls, they feed each other the
wrong way round: in `(defun f (p) (hold (fn () (str-len p)) p 3))`,
with `hold`'s first position owned and its second borrowed, admitting
the call makes the literal heap, its capture makes `p` owned (§6.4 rule
1), `p` frame-owned at `hold`'s borrowed position makes the call
ordinary, and an ordinary call leaves the literal stack and `p`
borrowed, which admits the call again (proposed case 74). So the pass
decides in this order, first to last:

- **tail sites**, before any kind: a call in tail position is a *tail
  site* iff this section admits it when every inferred kind is taken as
  `borrowed` and every capture as a stack closure's — rules (b), (f) and
  the extern rule do not apply to it, and rule (e) does not apply
  through an argument that is frame-owned whatever the fixpoint decides:
  an `Owned` object that is not immortal, or `Borrowed(x)` or
  `Derived(x)` of an owning `let` binding, a loop variable, a `match` or
  implicit temporary, or an owned parameter whose kind is not inferred
  (every parameter of a `fn` body and its `env`, an `:owned` method
  parameter, every parameter of an all-owned body, §8.4). So case 08's
  `(for-each @v (fn ..))`, whose `@v` is a fresh argument at a borrowed
  position, is no tail site, and its closure stays on the stack;
- **the fixpoint** of §6.4 — kinds, escape summaries, heap closures —
  in which the head and the arguments of every tail site are E6 for
  the heap rule of §6.5 and for rule 2 of §6.4, whether or not the
  site is admitted afterwards, so that nothing in the fixpoint depends
  on its own outcome;
- **admission**: a tail site is a tail call unless rule (e) applies
  with the finished kinds and heap closures, which it can now only
  through an owned parameter, a capture of a heap closure or a part of
  one; a call in tail position that is not a tail site is ordinary;
- **scope-local bindings** (§6.11), for which E6 is the admitted tail
  calls.

In `f`, then, the call is a tail site: the literal is heap, `p` is owned
by rule 1, and the call is ordinary by rule (e) through `p`. That is
sound: the literal retains `p`, `hold` frees the literal with its
parameter, and `f`, whose frame the ordinary call kept, releases `p`
after `hold` returns. A tail site that calls a member of the caller's
SCC is always a tail call, since rule (e) concerns only callees outside
it.

At a tail call `(g a₁ .. aₙ)`:

1. the head (when it is not a global name) and the arguments are
   evaluated left to right, as in any call (syntax §2);
2. every argument is `consume`d for its position: at an owned position
   an `Owned` argument is moved and a `Borrowed`/`Derived` one retained,
   except that `Borrowed(x)` for an owning binding `x` of the frame (a
   `let` binding, a loop variable, an owned parameter, a temporary) is
   **moved**: `x` is not released at the scope exits that follow and its
   count travels with the argument (a second occurrence of the same `x`
   among the arguments retains). At a borrowed position nothing is
   emitted, and by (e) the argument is frame-independent. Through a
   closure value every position is owned and the closure value itself
   is consumed the same way (§8.4). A forwarded `&v` emits nothing;
3. the scope exits of every scope enclosing the call inside the body
   run (their owning bindings released in reverse order, except those
   moved in step 2), then every remaining `Owned` temporary of the step
   is released, then every owned parameter of the frame that step 2
   did not move — in a closure body `env` among them, unless a self
   tail call of a named `fn` hands it on (§8.4) — exactly as at the
   body's exit (§6.3, the E1 row; case 07's `acc`). Rule (c): no
   `Owned` temporary or owned parameter of the caller survives the
   jump, since each was consumed into an argument or released here,
   and none could be released later because nothing of the caller runs
   later;
4. the frame is discarded and control passes to `g` (lIR `tailcall` or
   `indirect-tailcall`, §8.9); `g`'s result is the caller's result. The
   callee releases its owned parameters at its own exits, or hands them
   on.

The head and the arguments of a tail call are escape position **E6**
(§6.3): a binding whose value is passed or called there is never
scope-local (§6.11), since the frame that would hold it is gone while
the callee runs; and a closure literal, a `let`-bound closure or a
named `fn`'s self-name that is the head or an argument of a tail site,
admitted or not (above), is a heap closure that owns its captures
(§6.5), whatever the callee's summary says, the self-name as the head
of its own self tail call excepted (§6.5, use (a)). A `Derived(x)`
argument is retained at an owned position; at a borrowed position it
needs nothing, and by (e) `x` is then frame-independent.

Self tail calls, mutual recursion inside an SCC, calls through closure
values and calls to `:owned` positions of methods therefore run in
constant stack, outside `async` bodies (rule (f)). The interpreter does
the same (§6.12): it decides tail calls by this section, discards the
frame before entering the callee and lets the callee's parameter
bindings own their values, so the same objects are freed at the same
points as in compiled code.

**`loop` and `recur`** (syntax §3.18) are this rule applied to a local
function whose body is part of the enclosing function: a `loop` binds
its variables as owning bindings (each initialiser `consume`d, like an
argument at an owned position; what a loop variable receives is followed
through it by rule 1 and the escape summary, §6.4, and never comes from
a scope-local binding, §6.11) and runs its body; `recur` is a tail call
to the loop that leaves the enclosing function's frame in place, so it
moves only what it exits:

- its arguments are evaluated and `consume`d as in step 2, except that
  `Borrowed(x)` is **moved** only when `x` is a binding the `recur`
  exits — a `let` or `match` binding or a temporary inside the loop
  body, or a loop variable — and is **retained** when `x` is any other
  binding (of a scope enclosing the loop, a parameter of the enclosing
  function, a capture), which stays in scope and is released at its
  own scope exit; a second occurrence of the same `x` retains, as in
  step 2, and an `Owned` argument is moved;
- the scope exits inside the loop body run, the bindings just moved
  excepted, and the step's other temporaries are released; the
  enclosing function's owned parameters are not, since its frame
  continues;
- the old value of every loop variable is released, except a loop
  variable that was moved as an argument, whose count now belongs to
  the variable it was passed to; the new values are stored and the
  body restarts.

A closure among the arguments of a `recur` is at E6. When the body
finishes without `recur`, the value follows the scope-exit rule of
§6.3 with the loop variables as the scope's owning bindings (a
`Borrowed(slot)` result is moved out, a `Derived(slot)` one retained,
then the variables are released). A call in tail position of the loop
body that is not a `recur` is in tail position of the enclosing
function, with the loop variables among the bindings that step 3
releases. Moving a binding from outside the loop would leave it in
scope with no count: in `(let ((y (Box ..))) (loop ((b (Box ..)) (i
0)) (if (< i 2) (recur y (+ i 1)) (str-len (unbox b)))))` the second
`recur` would free `y`'s box, `unbox` would read it, and the loop's
exit would release it again (proposed case 63).

Case 07: `build`'s `acc` is owned (§6.4 rule 1: the base case returns
it; rule 3: the self tail call passes a fresh `(conj acc n)` at its
position). `(conj acc n)` is an `Owned` temporary moved into the call;
`acc` reaches the call only through `conj`, so step 3 releases it
before the jump, and the previous version dies as soon as the new one,
which shares its nodes, exists. On the base path the `if` joins
`Borrowed(acc)` with the tail-call branch, a call and so `Owned`
(§6.2): the join retains `acc` (§6.3), E1 moves that count to the
caller, and the parameter is released at the exit — a retain and a
release, not a move; depth is constant. `main`'s `(count (build 100000
[]))` is an ordinary call (rule (e): a fresh vector at `count`'s
borrowed position), and `(build 100000 [])` is an ordinary call whose
`[]` is moved into `build`'s owned `acc`. `(defun fill (&v n) (if (= n
0) () (do (append &v n) (fill &v (- n 1)))))`: `(append &v n)`
acquires and copies on its first update; the tail call forwards the
private cell; the caller's single write-back stores the final vector
(proposed case 25). A tail call through a closure value `(k x)` hands
`k` and `x` over owned and the closure body releases both at its exit
(§8.4), so continuation-passing code runs in constant stack, with a
named function as `k` too, since its closure runs its all-owned body
(§8.4; proposed case 52).

### 6.11 Allocation: stack when scope-local, heap otherwise (Decided, D6)

A binding `b` — an owning `let` binding, a `match` temporary or an
implicit temporary of a step — is **scope-local** iff its initialiser
**allocates its object in this frame** — a struct or variant
constructor call, `(cell e)` or `(atom e)`; closures are decided by
§6.5 and private `&` cells by §6.6 — and **no occurrence** of
`Borrowed(b)`, on any path of its scope, is:

- at an escape position E1–E6: returned, stored, captured by a heap
  closure, passed to `spawn`, or passed or called at a tail call
  (§6.10; an admitted one: a tail site that rule (e) makes ordinary
  keeps the frame);
- passed to an `escapes` parameter of a known callee, or to any
  parameter of a closure value;
- moved out or retained by a scope exit or a join (§6.3);
- the initialiser of a `loop` variable (a `recur` argument is at E6
  already): the variable is an owning binding that the scope does not
  bound — it may be stored, returned, captured or moved out of the loop
  as its value (§6.10) — and retaining a `STACK` object into it is a
  no-op, so it would go on pointing into a scope that has ended
  (proposed case 76);
- the operand of `weak` (§6.7: a stack object cannot be observed dead by
  a weak box) or of `raw-retained` (§6.13).

An occurrence of `Derived(b)` never disqualifies `b`: a sub-object is a
heap object stored at E2 and outlives `b` on its own count. Being passed
to a `noescape` parameter of an ordinary call, borrowed or owned, does
not disqualify `b` either: the callee's retain and release are no-ops on
a `STACK` object, its summary says it keeps nothing, and the frame is
alive for the whole call. The condition is on the **binding over all
paths**, not on a path: an object returned on one branch and dropped on
another is a heap object on both, allocated once at its initialiser,
which closes the mixed-branch hole (an object stack-allocated on one
branch and escaping on another) by construction.

Any other initialiser makes an ordinary owning binding, counted and
released at its scope exit, because the frame did not allocate its
object and others may hold it: a call result (the callee allocated the
object, or returned one that others still hold: case 01's `h` is the
box inside `l`'s list, retained by `first`); a literal collection,
whose rewrite is a chain of prelude calls (syntax §1.4; a fused
allocation would have to be specified before it could qualify); the
result of a scope exit or a join; a cell, atom or weak read; `swap!`,
`join`, `await`; an immortal. A parameter's object arrived from
elsewhere, and a loop variable is rebound by every `recur` to an object
made elsewhere (§6.10), so neither is ever scope-local in v1. An
inline drop on an object the frame did not allocate would release
children that others still hold and leave its own count unreleased
(proposed cases 61, 62, 64).

A scope-local object is allocated in the creating frame with the `STACK`
flag and count 0 (§8.2), receives no count operation, and its `drop`
runs inline at the exit of its binding's **scope** (its counted children
are released; nothing is freed), in the binding's place in the reverse
order in which the scope releases its owning bindings (§6.3): at the
`let` or `match` exit for a `let` binding or a `match` temporary, at the
end of its step for an implicit temporary, and among the scope exits
and releases that a tail call or a `recur` runs before its jump
(§6.10). Its sub-objects are always heap objects, because they were
stored at E2. A `(some e)` of a non-`Option` object allocates nothing
(§8.1): an `Option` value is never itself a stack candidate, and its
payload was consumed at E2, so it is a heap object. Stack closures
(§6.5) and private `&` cells (§6.6) are the same mechanism.

A stack object lives as long as its scope, not as long as the
function: one made in an inner `let` is dropped when that `let` exits,
and one made in a loop body when a `recur` or the loop's exit leaves
the body, which is also what lets the next execution of the site reuse
its slot (§8.2). An `async` body's scopes belong to its task, not to
one call of `resume`: a scope open at an `await` stays open across it,
its stack objects living in the task object (§8.2), and it ends where
the body leaves it, after a later resumption.

This is in force from the first implementation, not an optimisation
added later: the compiler allocates on the stack wherever the condition
holds, and so does the interpreter, exactly there (§6.12), so the two
agree on every allocation and every free. `fibc --explain` prints
`scope-local` on the binding (§9). Obligations on `fibref`, beside
`write-unique` (§6.6, §6.12):

- the audited heap gains **stack scopes**: `alloc_in_scope(scope, ..)`
  allocates a `STACK` object that belongs to the scope in which the
  compiled code ends it (above): the `let` or `match` of a `let`
  binding or a `match` temporary, the step of an implicit temporary or
  of a stack closure written as an argument, and the call of a private
  `&` cell, which ends at its write-back (§6.6); an `async` body's
  scopes are the task's (above). The scope's exit ends the object,
  running its drop where the compiled code runs it inline, and is
  itself a trace event. The scope is never the function's activation:
  ended at the function's exit, the scope-local `h` of proposed case 79
  kept its count on an array past the inner `let` that made it, so a
  unique write that followed copied in `fibref` and wrote in place in
  the compiled program, and in a loop every iteration's object stayed
  live until the function returned;
- a reference to a stack object that is stored into a heap object,
  escapes its scope (returned, captured by a heap closure, passed to
  another thread, passed or called at a tail call), or is read after
  its scope has ended is an audit error, distinct from every other
  failure, so that a checker that misclassifies a binding is caught by
  the case that exercises it, including a read after the scope has
  ended but within the same activation, where the compiled program
  reads a slot that its site may already have reused (§8.2).

Both choices give the same frees (**Decided**, §2: "scope-local objects
are not counted"): what a scope-local binding would have released at
its scope exit under counting is exactly what the inline drop releases
there.

### 6.12 Same results, same frees: what the interpreter does

The reference interpreter implements the plain counting semantics of §2:
every binding holds a count (it retains on every `let`, every capture,
every parameter), every temporary is released at its step's end, and it
optimises nothing — with five deliberate exceptions that are part of the
*semantics* and are implemented by both:

1. tail calls (§6.10): the interpreter decides which calls in tail
   position are tail calls by exactly the rules of §6.10 (it runs the
   checker, so it has the kinds and summaries of §6.4), releases the
   frame's bindings and owned parameters before entering the callee
   (§6.10 step 3) and lets the callee's parameter bindings own the
   arguments; so 100000 accumulator versions are not held until a
   recursion unwinds, and a forwarded `&` parameter keeps its private
   cell;
2. the copy-in of an `&` argument always acquires (§6.6), so an `&`
   function copies on its first update in both;
3. the unique-write test of `array-set!`/`set-field!` (§6.6);
4. stack allocation (§6.11): the interpreter allocates a scope-local
   object in its binding's scope (`alloc_in_scope`) exactly where the
   checker says so and ends it at that scope's exit, running its drop
   where the compiler runs the inline drop — the `let` or `match` exit,
   the step's end, the scope exits of a tail call or a `recur` — never
   deferred to the function's exit, so an allocation the compiler puts on
   the stack is never a heap event in the interpreter's trace, and the
   children it holds are released at the same points in both (proposed
   case 79);
5. the parameter convention (§6.4): at every call, ordinary or tail,
   the interpreter follows the callee's count kinds as the checker
   computed them — through a function value every object parameter and
   the closure are owned, and a named function reached that way runs
   its all-owned body (§8.4). An `Owned` argument at an owned position
   is moved into the parameter binding (it is no temporary of the call
   step, syntax §2, and is not released at the step's end); a
   `Borrowed` or `Derived` one is retained by the binding; the callee
   releases the parameter at its exit or before a tail call's jump, or
   hands its count on exactly where the compiler does (§6.4, §6.10). A
   borrowed position keeps plain counting: the binding retains and
   releases around the call, which the compiler elides. Without this
   the two would disagree on free points, and so on the count that a
   unique write running between the two points sees: `(defun set0 (a)
   (let ((c (cell a))) (do (array-set! &c 0 9) (array-get @c 0))))`,
   given a fresh array, frees it at `set0`'s exit in compiled code,
   where the owned parameter is released, while plain counting at the
   call frees it at the end of the caller's step (proposed case 58; the
   write copies in both, since the cell's store retains the parameter,
   §6.4).

The compiler elides: alias and derived bindings (no count), borrowed
parameters (no count), stack-closure captures (no count) and
scope-local objects (no count). It does not elide cell reads (§6.3) or
the captures of an `async` (§6.9). Each elision removes a
retain/release pair around an interval during which another count on
the same object is provably held by a binding that nothing can write
during the interval; the object's count at every remaining release is
the same, so it reaches zero at the same program point: the multiset of
objects freed at each scope exit, step end, callee exit and tail-call
jump is identical between the two. The audit compares exactly that
(method.md rule 6), and `fibc --explain` (§9) prints the compiler's
side of the comparison.

**`fibref` obligations** that these rules put on the audited heap, each
with a test that shows it firing (CLAUDE.md): `write-unique` (§6.6: a
write to an immutable object legal iff it is neither `SHARED`,
`IMMORTAL` nor `STACK` and its count is exactly 1); stack scopes,
`alloc_in_scope` and the stack-reference errors (§6.11); and the
discarded frame of a tail call (§6.10: a binding released at the jump
must not be read by the callee, which the ordinary use-after-free check
already catches). The `read of taken field` event of the earlier draft
is gone with D1: nothing is ever taken.

### 6.13 Unsafe (§9)

`unsafe` changes nothing above. Externs take scalars, `ptr` and `unit`
only (syntax §3.15), so no fibber object is ever an extern argument.
`(raw e)` is the address of `e`'s object with no count operation, valid
while `e`'s binding is: the borrow of §9. `(raw-retained e)` is a
consume position (E2): `e` is retained (an `Owned` temporary is moved)
and the count belongs to the foreign side until `(release-raw p)` or
the runtime's exported `fib_release` releases it; this is §9's "unless
the foreign interface says the reference is retained" (**Decided**).
`ptr` is a scalar: no count, not `Send`. The programmer's
obligation inside `unsafe` is that every `raw-retained` count is
released exactly once; the audit reports an unreleased one as a leak
and a double release as a count going negative.

### 6.14 Error catalogue

| Text (canonical part in bold) | Rule |
|---|---|
| **variable x passed to more than one & parameter** in call to f | §6.5, case 12 |
| **& parameter captured by escaping closure**: v in f | §6.5, case 18 |
| **& parameter in async function**: v in f | §6.9, case 14 |
| **cell cannot be shared between threads**: `<path>` has type (Cell T) | §5.3, case 13 |
| value of type T cannot be shared between threads: `<path>` | §5.3 |
| parameter p of f is declared :borrow but escapes | §6.4 |
| implementation of P/m for T makes parameter p escape; the protocol declares it :borrow | §6.4 |
| & argument must be a cell variable | §2.14 |
| **& parameter v used as a value** in f | §2.14, syntax §3.13 |
| parameter v of g is &; pass &x | §2.2 |
| function with & parameters is not a value | §2.1, §2.13 |
| T has no field f | §2.5, §2.13, §3.3 |
| cannot infer the struct type of e for field f; annotate it | §3.4, §2.13 |
| cannot infer whether x is a cell, an atom or a weak reference | §3.4 |
| no implementation of P for T | §3.3 |
| ambiguous constraint P a in f; add an annotation | §3.3 |
| cannot construct the infinite type | §3.2 |
| cannot unify T₁ with T₂ | §3.2 |
| non-exhaustive match: missing V / redundant match clause | §2.6 |
| await outside async | §6.9 |
| weak requires an object type | §2.11 |
| V is a constant, not a function; write V | §2.2 |
| recur outside loop / recur not in tail position | §2.4, syntax §3.18 |
| no implementation of P for a; add (P a) to the :where of the impl | §2.7 |
| def g has an unresolved type; annotate it | §2.16 |
| def g: initialiser is not a constant expression | §2.16, syntax §3.19 |
| def g and defun f depend on each other | §2.16, §3.5, syntax §3.19 |

---

## 7. Case table

For each case: the rule of ownership.md that decides it, the mechanism
of this document that produces the verdict, and for reject cases the
exact error text (the case header's `error:` string is the canonical
fragment; the rest is the position and witness the checker appends).

| Case | Rule | Mechanism | Verdict / error |
|---|---|---|---|
| 01 return-part-of-argument | §4 | `first` returns `Derived(self)` → E1 retain (§6.3); `head` returns `Owned`; `h` owns one count, the list the other; released at scope exits | accept, 1, clean |
| 02 structural-sharing | §5 | `conj` builds a new version whose nodes retain the shared subtrees (E2 inside the library); `make`'s `v` released at scope exit; nodes stay alive via `w` | accept, 7, clean |
| 03 store-borrowed-value | §3.2 | `conj`'s element is stored (E2 inside `conj`) → `remember`'s `item` escapes (§6.4); `s` retained once per vector; inner `let`: body `Borrowed(a)` → move out, `b` and `s` released (§6.3); `keep` released at `main`'s exit | accept, 5, clean |
| 04 branch-dependent-owner | §4 | join of `Borrowed(x)` and `Owned` → retain on the `x` branch; result `Owned` on both paths (§6.3) | accept, 5, clean |
| 05 closures-share-state | §6 | both `fn`s are E2 arguments of `cons` → escaping (§6.5) → E3 retains `n` twice; `set!` on a captured cell; `let` releases its count; the list's drop frees the closures, then the cell | accept, 2, clean |
| 06 capture-borrowed-param | §3.3 | the `fn` is at E1 → escaping → E3 retains `prefix`; `matcher`'s `prefix` escapes; `p`'s `let` releases; the closure owns the string until `m` dies | accept, 1, clean |
| 07 recursive-accumulator | §4, §5 | the self call is a real tail call (§6.10): `acc` is inferred owned (§6.4: returned, and passed a fresh vector at a tail call); `(conj acc n)` is `Owned`, moved into the call; the frame's `acc` released before the jump (§6.10 step 3); on the base path the join with the tail-call branch retains `acc` and the exit releases the parameter (§6.3), a retain and a release rather than a move; constant stack | accept, 100000, clean |
| 08 mutate-while-iterating | §5 | the copy-in acquires (§6.6, D1); `@v` acquires for the `for-each` call, an ordinary call (§6.10 rule (e)); the closure is a stack closure (`for-each`'s `f` is `:borrow`) so capturing `&v` is legal; each `append` acquires again and `push!` sees a count above one and copies (§6.6, traced). Since D1, `main`'s cell keeps `V0` alive for the whole call, so the case also passes with the `@v` acquire elided: it pins copy-in/copy-out and the stack-closure capture of `&v`, not the acquire, which proposed cases 66 and 78 pin; its header's "the iteration holds a count" predates D1 (`append` copies because its own copy-in acquires) and is to be corrected in the case file, verdict unchanged | accept, 6, clean |
| 09 iterator-outlives-source | §3.1 | `iter` stores `v` into the iterator struct (E2, retain); `evens` returns `Owned`; `v`'s `let` releases; the iterator keeps the vector | accept, 2, clean |
| 10 atom-old-value | §7 | `plet` → `spawn`; `Send (Atom (Vec i64))` holds; the `pmap` closure captures `a` (`Send`) → colour `send`; `@a` acquires under the lock; `swap!` releases the old vector after the store; `snapshot` keeps it | accept, 1000, clean |
| 11 borrow-across-await | §8 | `async` is E3 for `s` (retained at creation) and requires `Send str` ✓; the task's frame owns it; `s`'s `let` releases | accept, 5, clean |
| 12 reject-same-binding-twice-inout | §5 | syntactic distinct-places check on `(bar &x &x)` (§6.5) | reject: `variable x passed to more than one & parameter in call to bar` |
| 13 reject-cell-crosses-thread | §7 | the closure's colour is `local` (capture `n : (Cell i64)`); it flows to `pmap`'s `(fn :send (a) b)`; `local ⊑ send` fails (§5.4); witness `n` | reject: `cell cannot be shared between threads: closure capture n has type (Cell i64)` |
| 14 reject-inout-in-async | §8 | `fill` has `&buf`, and its `async` mentions `&buf` and is the body's value, so `fill` is an async function (§6.9; syntax §3.1), checked first | reject: `& parameter in async function: buf in fill` |
| 15 cycle-through-cell-leaks | §6 | `[k]` retains `k` (E2); `set!` stores the vector into `k`'s cell; the `let` releases one count; the audit finds the SCC through the cell (§6.7) | accept, 1, leak-cycle |
| 16 coordinated-update-single-atom | §7 | `Send Accounts` holds (scalar fields); one `swap!` replaces the whole struct: `f`'s `Owned` result stored, the old released; `snap`/`final` acquire and release | accept, 200, clean |
| 17 inout-and-borrow-same-call | §5 | the copy-in acquires (D1); `@v` acquires again; `push!` copies; the write-backs release the old vector's counts; the temporary released after the call (§6.6, traced) | accept, 4, clean |
| 18 reject-inout-captured-by-escaping-closure | §5 | the `fn` is at E1 → escaping; its capture set contains `&` parameter `v` (§6.5) | reject: `& parameter captured by escaping closure: v in make-pusher` |
| 19 weak-parent-pointer | §6 | `(weak parent)` is not a count operation; `conj` and `set!` on the children cell are E2; the strong graph is a tree; `let`s release in reverse (§6.7) | accept, 2, clean |
| 20 weak-ref-to-dead-object | §6 | the inner `let` releases `v` (freed; box cleared); `@w` finds the box dead → `nil` (§6.7) | accept, 1, clean |

Cases 12, 14 and 18 are decided by syntactic checks, case 13 by a type
constraint; none needs the mode analysis. Every other case is decided
by the tables of §6.2–§6.3 plus summaries, and its audit verdict follows
from applying those tables, which the interpreter confirms by running
the counting semantics with the deliberate exceptions of §6.12.

Programs that the confirmed findings on this document turned into
candidate cases (an `&` parameter used as a value, a whole-object
pattern variable, a cell read beside a sibling write, a pre-`await` use
of a capture, an `&` argument through a self tail call, a discarded
`Task`, `(some nil)`, `weak` of a literal, an update inside `dotimes`, a
user `count`, an `await` inside a loop, and the others), those of the
second round (a push into a `Form` literal that must copy, an `&`
function that blocks on a task, `derive` on a generic struct, a declared
`:borrow` that escapes, a spliced top-level macro, a `def` table read
from two threads, one task joined by two threads and awaited by two
tasks, a bounded generic instantiated at two `ptr`-class types, a `weak`
taken on a shared object, an `:as` pattern in `let`), those of the third
(`derive` on an enum, a closure passed at a self tail call, a `def`
naming a function, a macro that emits `nil`), those D5 and D6 call for
(a mutual tail recursion of depth 10⁶, a tail call passing a
stack-eligible object, a tail call with an `&` argument that is not a
self-forward, a stack object captured by an escaping closure, a returned
closure over a would-be stack object: entries 53 to 57), and those of
the fourth round, on the sections D1–D7 touched (continuation-passing
through a named function 10⁶ hops deep, a closure body ending in a call
that borrows a capture, an owned parameter at an ordinary call, a named
`fn` storing its own name, a call result bound by `let`, a `recur` of a
binding outside the loop, a loop variable rebound by `recur`, an `&`
call inside a loop of 10⁶ iterations, a cell read beside an iteration,
two names for one cell in one call, an owned parameter dropped at a tail
call, a returned parameter), and those of the fifth round (an `async`
body ending in a call, directly and through a closure value; an owned
parameter stored and read afterwards, into a struct and into a cell; a
closure and the parameter it captures passed at a tail site; a parameter
returned through a loop variable; a loop initialised from a would-be
stack object; a `dyn` in a private cell; a read of an `&` parameter that
outlives a `set!` of it: entries 70 to 78), and those of the sixth (a
scope-local struct dropped at its own `let`'s exit before an update of
the array it shares, with its forms in a loop, in an `async` body and
before a weak read; a unique write that ties a struct to the cell that
holds it, with the field operand of `set-field!`: entries 79 and 80)
are listed with their expected headers in
`spec/drafts/PROPOSED_CASES.md`; they become cases only when added to
`cases/` with the owner's sign-off. Its entries 25,
29, 30 and 35 predate D1 and D5 (field places, exclusivity, taken
fields) and are not updated there; D1 makes 30 and 35 ill-formed and
turns 25's and 29's `--explain` expectations into `acquire`. Entry 49 is
restated under D5, and entries 21, 37, 58, 59 and 60, which relied on a
field place, on the exclusive move and on the move of a stored
parameter, are corrected there.

---

## 8. Mapping to lIR

lIR is the S-expression assembler for LLVM IR (liar's `doc/lIR.md`),
used as it is: opaque `ptr`, `defstruct` with positional field types,
`define`/`declare`, `call`/`tailcall`, `indirect-call` and
`indirect-tailcall` in their present form `(indirect-call fnptr R
args..)`, `getelementptr`, `load`/`store`,
`alloca`, `br`/`phi`/`select`, `icmp`, and the atomic operations
`atomicrmw`, `cmpxchg`, `atomic-load`, `atomic-store` and `fence`, which
lir-core parses but `doc/lIR.md` does not yet document (§8.11). lIR has
no `switch`, no array type in its type grammar, no typed indirect call,
and no struct-typed `alloca`, `global` or constant: `alloca` takes a
scalar or `ptr` element type and an optional count, and a `global` is a
`ptr` or a scalar initialised with a literal, null or a string (a
function address is not accepted as an initialiser; `lair` reports
`complex global initializers` as not implemented). This mapping uses
none of the missing forms (`match` is an `icmp`/`br` chain, §8.3;
variable-length payloads are addressed with `getelementptr` on the
element type, §8.3; §8.4 writes the indirect call as lIR has it; a
stack object is a word-sized `alloca` addressed through
`getelementptr` on its struct type, §8.2; every static object and
table is built by the module initialiser of §8.2 and reached through a
`ptr` global) and §8.11 lists what hardening must add (`musttail`) and
what it may add. Everything
fibber-specific is a naming and layout convention on top; no fibber
vocabulary enters lIR, and none of liar ADR 021's safe-lIR features
(`own`, `rc`, `closure`) is used. Runtime support functions are ordinary lIR `define`s in a
`fib.rt` module (or C, linked). All of §8 is **Decided** as the v1
mapping; a later lIR extension (§8.11) may change a shape without
changing a rule.

### 8.1 Representation of every type

| fibber type | lIR type | Layout class |
|---|---|---|
| `bool` | `i1` | i1 |
| `i8 i16 i32 i64` | same | that width |
| `f32`, `f64` | `float`, `double` | float, double |
| `char` | `i32` | i32 |
| `keyword` | `i64` (interned id) | i64 |
| `unit` | no value: `void` result, no argument; `()` in a value position is dropped | none |
| field-less `defenum` | `i32` variant index | i32 |
| `ptr` (unsafe) | `ptr`, uncounted | i64-like scalar |
| every object type (`str`, `Form`, `Array`, struct, enum with fields, `Cell`, `Atom`, `Weak`, `Task`, closure) | `ptr` to a block starting with the header (§8.2) | ptr |
| `(Option T)`, `T` an object type that is not itself an `Option` | `ptr`, null = `nil`; no allocation for `some` | opt |
| `(Option T)`, `T` a scalar, a `dyn`, or itself an `(Option ..)` | `ptr` to a heap enum object: tag and payload (§8.3; v1) | ptr |
| `(fn κ (Ā) R)` | `ptr` to a closure object (§8.4) | ptr |
| `(dyn P)` | `{ ptr ptr }` by value: object, vtable | dyn |
| `(& T)` parameter | `ptr` to a private cell (§8.6) | — |

The null representation is used exactly when `T`'s own representation
has no null value. `(Option (Option str))` is therefore a heap enum
whose `some` variant carries a nullable payload: `(some nil)` is a
non-null object with tag `some` and a null payload, distinct from the
outer `nil`, as §1.5 requires; with a bare nullable pointer the two
would be the same bit pattern and a compiled `(match (some nil) ((some
_) 1) (nil 0))` would answer 0 where the interpreter, which carries
real tags (§4.5), answers 1 (method.md rule 6). The layout class `opt`
(§4.3) keeps the rule intact under monomorphisation.

Scalars inside objects are stored unboxed at their lIR type. A struct
`(defstruct P (x: i64 s: str))` is `(defstruct P.obj (i64 i32 i32 i64
ptr))`: the header fields first, then the fields in declaration order;
one lIR `defstruct` per monomorphised object type.

### 8.2 The count header, runtime primitives, type table

Every object begins with a 16-byte header:

```
(defstruct fib.hdr (i64 i32 i32))       ; count, type-id, flags
flags: bit 0 SHARED    counts are atomic from now on (§7)
       bit 1 HAS-WEAK  a weak box exists (§8.7)
       bit 2 STACK     a scope-local object: retain/release are no-ops (§6.11)
       bit 3 IMMORTAL  static data: literals and everything reachable from them,
                       def values (syntax §3.19), named-function closures, vtables
```

`count` is the number of counted references (§2) of a counted object.
An `IMMORTAL` or `STACK` object has no count: its `count` field is
**0**, a value no test accepts (`fib.unique?` needs 1, a weak upgrade
needs > 0), and nothing ever changes it; a static object built by the
module initialiser (or emitted as a constant once lIR holds one,
§8.11), a literal allocated by the interpreter, a stack object's
`alloca` and `fib.immortalise` all initialise it so (**Decided**).
`type-id` indexes the table `fib.types` of per-type records
`(defstruct fib.typerec (ptr ptr ptr i64))` — `drop`, `trace`, `name`,
`size` — one per monomorphised object type: `drop` releases the
object's counted children; `trace` calls a callback on each child
pointer (used by share-marking, §8.8, by `fib.immortalise` and by the
interpreter's audit). lIR has no struct-typed `global`, so the table is
a block of `n` records that the **module initialiser** allocates and
fills: `(global fib.types ptr (ptr null))` holds its address, and
record `tid`'s field `k` is `(getelementptr %struct.fib.typerec (load
ptr @fib.types) (i32 tid) (i32 k))`; `fib.types[tid].drop` below is
that load. The initialiser `fib.init.<module>` is an lIR `define` the
compiler emits per module; the entry point it emits calls the
initialisers of every module in dependency order and only then the
program's `main`, and each initialiser, in this order: allocates the type records and stores the code pointers
(`(store @drop.T slot)` is an ordinary store of a function address,
which lIR accepts where a `global` initialiser does not); builds every
**static object** of the module — string and `Form` literals with
their headers (§8.3), the constant closures of named functions and
constructors (§8.4), vtables (§8.5) — with `fib.alloc` followed by
`count := 0, flags := IMMORTAL`, storing each address into its own
`ptr` global, from which code loads it; then evaluates the module's
`def`s (§8.10). Once lIR has struct-typed constants (§8.11) the static
objects become static data with the same headers and nothing else
changes: an `IMMORTAL` object is never freed either way and the audit
does not track it (§6.7) (**Decided**).

**The flags word is accessed atomically** (**Decided**).
`SHARED`, `STACK` and `IMMORTAL` are fixed before any second thread can
see the object, but `HAS-WEAK` is set by `(weak x)` on an object that
may already be shared while another thread reads the same word in
`fib.retain`; under the LLVM memory model, which lIR follows exactly, a
racing non-atomic load is `undef`, so that retain could take the
non-atomic count path on a shared object, or skip. Hence every read of
`flags` below is `atomic-load monotonic` and every write after
allocation is `atomicrmw or monotonic` (`fib.share` setting `SHARED`,
`weak` setting `HAS-WEAK`); on the targets lIR supports these cost what
a plain load and a locked `or` cost. The count's two paths are
unchanged: the `SHARED` bit read this way selects them, and an object's
bit is set before the object is handed over (§8.8), so the handoff's
own synchronisation orders that write before any other thread's read.
No other header field changes after allocation.

```
fib.alloc   (i64 size, i32 tid) -> ptr   ; malloc; count = 1, flags = 0
flags(p)    = atomic-load monotonic p.flags
fib.retain  (ptr p) -> void
    if p == null: return                         ; Option nil
    if flags(p) & (STACK|IMMORTAL): return
    if flags(p) & SHARED: atomicrmw add count 1 (monotonic)  else: count += 1
fib.release (ptr p) -> void
    if p == null or flags(p) & (STACK|IMMORTAL): return
    if flags(p) & SHARED: old = atomicrmw sub count 1 (acq_rel); if old == 1: fib.drop p
    else:                 if count == 1: fib.drop p  else: count -= 1
fib.drop    (ptr p) -> void
    if flags(p) & HAS-WEAK: fib.weak-clear p     ; §8.7
    call fib.types[tid].drop p                   ; releases children
    free p
fib.unique? (ptr p) -> i1
    if flags(p) & (SHARED|IMMORTAL|STACK): return 0   ; shared: never written in place; static and
                                                      ; stack data have no count to test (§6.6)
    return count == 1
fib.share   (ptr p) -> void                      ; §8.8
fib.immortalise (ptr p) -> void                  ; def initialisation (syntax §3.19): walk p through trace,
                                                 ; stopping at IMMORTAL objects; on each: count := 0,
                                                 ; flags |= IMMORTAL. Runs before main, single-threaded.
```

A `STACK` object has the same layout in an `alloca` of the frame,
never passed to `fib.release`; the compiler emits `fib.types[tid].drop`
inline at scope exit. lIR's `alloca` takes a scalar or `ptr` element
type, not a named struct, so the object is `(alloca i64 (i32 k))` with
`k` the layout's size in 8-byte words — every field of these layouts
is at most 8-byte aligned, so the words give the alignment the fields
need — and its fields are addressed through `(getelementptr
%struct.T.obj p (i32 0) (i32 i))` exactly as a heap object's are; the
header is stored by the code that allocates it. Each `STACK`
allocation site — a scope-local object, a stack closure, a private `&`
cell — has exactly one such `alloca`, in the function's entry block
(in an `async` body, whose locals live in the task object so as to
survive an `await`, §6.9, one slot of the task object instead), and
every execution of the site reuses it, storing the header and the
fields afresh. lIR emits an `alloca` where it is written, and one
written in a loop block is a dynamic allocation reclaimed only at
return: an `&` call inside a loop of 10⁶ iterations would take 24 MB of
stack and overflow where the interpreter, which frees each private
cell at its write-back, does not (method.md rule 6; proposed case 65).
Reuse is sound because the object a site made last time is dead before
the site runs again: within one activation only a `recur` re-runs a
site, a `recur` first runs every scope exit inside the loop body
(§6.10), a private cell dies at its write-back, and no loop variable
ever holds a `STACK` object: none is scope-local, and a binding that
initialises one or is passed by a `recur` is not scope-local either
(§6.11, E6). Immortal
objects are never `drop`ped or freed, and the audit does not count
them among the allocations live at exit (§6.7). The
interpreter uses the same header, type table and `drop`/`trace`
functions (interpreted) and allocates its literals and `def` values
with the same `IMMORTAL` header, so its trace of `alloc`, `retain`,
`release`, `free`, `read`, `write`, `weak`, `upgrade` and `shared`
events is comparable one-to-one with an instrumented build of the
compiled program.

### 8.3 Structs, enums, strings, arrays, `Option`

- Struct: `(i64 i32 i32 field-types..)`; object fields `ptr`, scalars
  inline. The constructor allocates, writes the header, stores each field
  after `consume` (E2). Its `drop` releases every object field.
- Enum with fields: `(i64 i32 i32 i32 payload..)`: tag, then the largest
  variant's fields; one lIR `defstruct` per variant with the same prefix;
  `match` loads the tag and compares it with each clause's tag in clause
  order (`icmp eq` and `br`; lIR has no `switch`, §8.11), then
  `getelementptr`s through the variant struct; `drop` does the same on
  the tag.
- `(Option T)`, `T` a non-`Option` object type: the bare nullable
  pointer; `nil` is `(ptr null)`; `(some p)` in a pattern is a null
  test. `fib.retain`/`release` are null-tolerant, so codegen needs no
  special case. Every other `Option` (§8.1) is an ordinary heap enum
  with tag `0` = `nil`, `1` = `some` and the payload at its own lIR type;
  `(some p)` in a pattern reads the tag.
- `str`: the header struct `(i64 i32 i32 i64)` (count, type id, flags,
  byte length) followed in the same allocation by the UTF-8 bytes and a
  NUL for FFI, addressed with `getelementptr i8` from the end of the
  header (lIR's type grammar has no array type); no children. Literals
  are `IMMORTAL` objects with count 0, built by the module initialiser
  and reached through a `ptr` global each, since lIR has no struct-typed
  constant; its `(string ..)` constant holds the bytes the initialiser
  copies (§8.2).
- `(Array T)`: the header struct `(i64 i32 i32 i64)` (length last)
  followed by `n` elements of `T`'s lIR type, addressed with
  `getelementptr T` from the end of the header; `array-set!` does a
  unique write when `fib.unique?` holds, else allocates, copies, writes, stores
  into the place and releases the old array. `set-field!` likewise on a
  struct.
- `Vec`, `Map`, `Set`, `List`: library structs and enums over `Array`, by
  the struct rule.
- `Form`: an ordinary enum; exists at run time only where a program
  quotes. A quoted literal, with every `Form`, `Vec`, `Array` and `str`
  object reachable from it, is an `IMMORTAL` graph (count 0) that the
  module initialiser builds once and a `ptr` global names (§8.2), so a
  part of it pulled out by a `match` is never written in place (§6.6).

### 8.4 Closures and function values

A `fn` literal `L` with captures `c₁ .. cₖ` compiles to

```
(defstruct fib.closure.L (i64 i32 i32  ptr  slot₁ .. slotₖ))          ; header, code, captures
(define (L.code R) ((ptr env) (A₁ p₁) .. (Aₙ pₙ)) ..)                  ; loads captures from env
```

A call through a function value `f` is

```
(indirect-call (load ptr (getelementptr fib.closure.L f (i32 0) (i32 3))) R f a₁ .. aₙ)
```

in lIR's present form `(indirect-call fnptr ret-type args..)`, whose
arguments are all typed `ptr` today (`lir-audit/`): until the typed
form of §8.11 exists, every closure entry point takes `ptr`-sized words
and scalar arguments travel through them by a width-preserving
conversion, which is one of the reasons the typed form is on the
hardening list. Named functions used as
values are `IMMORTAL` closures with no captures whose code ignores
`env`, built by the module initialiser and named by a `ptr` global
each (§8.2); constructors likewise. A heap closure (§6.5: escaping, or
at a tail site) is `fib.alloc`ed with each object capture consumed
(E3); its `drop` releases the captures. A stack closure is a `STACK`
object of the creating frame: `(alloca i64 (i32 k))` addressed through
`(getelementptr %struct.fib.closure.L ..)` (§8.2), storing uncounted
pointers. Colours have no representation. A named `fn`'s
self-reference is the closure's own `env`, and each occurrence of it is
a use of the literal (§6.5), so a closure that stores, returns or
passes on its own name is a heap closure.

**The closure convention** (**Decided**, D5): `env` and every object
argument are owned by the callee. The caller `consume`s the closure
value and each argument (§6.3, §6.4); `L.code` releases `env` — its own
closure object — and its object parameters at every exit, a tail
call's jump included (§6.10 step 3), unless it hands them on: a self
tail call of a named `fn` passes `env` on unchanged, and a tail call
through another value consumes as §8.9 says. Named functions used as
values are `IMMORTAL` closures whose code is the function's
**all-owned body** `f.owned`: `f`'s body compiled a second time under
the closure convention, every object parameter owned and `env`
ignored. In it a call to a member of `f`'s SCC, `f` included, goes to
that member's all-owned body, whose positions are all owned, so a tail
call among them stays a tail call; every other call is compiled with
the callee's kinds as in any body, and the ownership pass decides the
body's tail calls, scope-local bindings and heap closures afresh, since
its parameters are owned (§3.5 step 5f). A call through the value is
thus an `indirect-tailcall` into `f.owned` wherever it is a tail call,
with no frame between, and `f.owned` releases each parameter at its
own exit or jump, where the interpreter releases it too (§6.12,
exception 5). An adapter that called `f` and released the borrowed
positions after `f` returned would keep one frame per hop and hold
every such argument until the whole chain returned: continuation-passing
code through a named function would overflow at depth 10⁶ and free in
another order than the interpreter (proposed case 52). When every
object parameter of every member of `f`'s SCC is owned, the all-owned
bodies are the functions themselves. The monomorphiser, which sees the
whole program (§4.3), emits an all-owned body for every function whose
value is taken anywhere in it and for the members of its SCC, at the
specialisation the value's type selects; a protocol method's
implementation used as a value likewise. A constructor stores every
argument (E2), so its code has the closure convention already. Retain
and release on an `IMMORTAL` or `STACK` closure are no-ops, so a stack
closure called directly, or a named function passed as a value, pays
nothing for the convention beyond the consume of its object arguments.

### 8.5 Protocol dispatch

Static: the monomorphiser rewrites `(count v)` at `v : (Vec i64)` to
`(call @count.Vec.i64 v)`. No tables.

Dynamic: for every `(P, K)` instance reachable through a `(dyn P e)` the
compiler emits a vtable: a block of one `ptr` slot per method in
protocol order, holding the code pointers. lIR has no struct-typed
`global` and takes no function address in a `global` initialiser, so
the module initialiser allocates the block, `store`s each code pointer
into its slot and stores the block's address into the `ptr` global
`P.vt.K` (§8.2); a struct-valued constant, if lIR gains one, would make
it static data (§8.11). `(dyn P e)` builds `{ e, (load ptr @P.vt.K) }`;
a method call loads slot `i` and `indirect-call`s with `obj` as `self`.
Retain and release of a `(dyn P)` value act on `obj`.

### 8.6 Cells, private `&` cells, atoms

```
(defstruct fib.cell (i64 i32 i32  T))             ; T = lIR type of the content (ptr for objects)
(defstruct fib.atom (i64 i32 i32  i32  ptr))      ; header, spinlock, value
```

`fib.cell` is one `defstruct` per content layout. A `(dyn P)` content,
two words by value (§8.1), is laid out as two `ptr` fields, object then
vtable: `(i64 i32 i32 ptr ptr)`, which lIR's `defstruct` takes as
documented (lir-core also parses a `{ ptr, ptr }` field type, which
`doc/lIR.md` does not document).

- `@c` on a cell: `load`, then `fib.retain` if the content is an object
  (never elided, §6.3).
- `(set! c v)`: `old = load`; `store v` (after `consume`); `fib.release
  old`. If the cell is `SHARED` (only an atom can be), the value is
  share-marked before the store.
- A private `&` cell: a `STACK` object in the caller's frame, one
  `(alloca i64 (i32 k))` per call site in the entry block, reused by
  every execution of the call (§8.2), sized like any stack object: `k`
  is the header's two words plus the content's, one word for a scalar, a
  `ptr` or an `opt` and two for a `(dyn P)`, so 3 or 4 (a fixed three
  words let the copy-in and every `set!` of a `dyn` write its vtable
  word past the slot, into a neighbouring slot of the entry block:
  proposed case 77) — addressed through
  `(getelementptr %struct.fib.cell t (i32 0) (i32 3))` (§8.2), since
  lIR's `alloca` takes no struct type; its header is stored (count 0,
  `STACK`) and it is initialised by the copy-in (`store` the acquired
  content, §6.6) or, when forwarded at a self tail call, not created at
  all (the caller's pointer is passed on); passed as `ptr`; write-back
  as §6.6; the `alloca` needs no drop. Case 17's `(push-count &v @v)`
  emits exactly this: three words, the header, the acquired vector
  stored through the field-3 `getelementptr`, `push-count` called with
  the `alloca`'s address, the content stored back into `v` afterwards.
- `@a` on an atom: `fib.lock a` (`cmpxchg` spinlock, acquire), `v = load`,
  `fib.retain v`, `fib.unlock a` (release store) — the single atomic step
  §7 requires.
- `(swap! a f)`: loop { `old = @a` (retained: the snapshot);
  `fib.retain old` and `fib.retain f` (the call's counts: `f` is called
  through a function value, which consumes the closure and each object
  argument, §8.4, and a heap closure's code releases its `env` at its
  exit); `new = f(old)`; lock; if
  `load == old` { `fib.share new` if the atom is `SHARED`; `store new`;
  `fib.retain new` (the caller's result); unlock; `fib.release old` (the
  atom's count); `fib.release old` (the snapshot); return `new` } else {
  unlock; `fib.release old` (the snapshot); `fib.release new`; retry } }.
  `load == old` compares the word (the object's address, a scalar's
  bits). `f` is `:borrow` at `swap!`'s own call (§2.10), so the caller
  passes it with no count and the two retains are `swap!`'s.
- `(reset! a v)`: `consume v`; `fib.share v` if `SHARED`; lock; `old =
  load`; `store v`; unlock; `fib.release old`.

A scalar-typed cell or atom holds the scalar inline and counts nothing.
Publication order — share, then store — is what keeps a concurrent
reader's non-atomic path from ever touching an object that is about to
become shared.

### 8.7 Weak references

```
(defstruct fib.weakbox (i64 i32 i32  i32  ptr))   ; header (its count = number of Weak values), lock, target
```

`(weak x)`: under the mutex of the global table `fib.weak-table`
(address → box): if `x` has `HAS-WEAK` (read with the atomic load of
§8.2), its box is found in the table (only flagged objects are ever
looked up); else a box is allocated, registered, and the flag set with
`atomicrmw or` (§8.2: `x` may be shared and another thread may be
reading its flags in `fib.retain` at that moment; doing the test and
the set under the mutex is what makes two threads taking a `weak` of
one shared object agree on one box); the box is retained and returned:
a `(Weak T)` value is the box. If `x` is `IMMORTAL` (a literal, a `def`
value, a named function used as a value, a vtable), a fresh box with
target `x` is allocated and not registered, and `x`'s header is not
written (static data may be read-only); such a box is never cleared,
`@w` on it always succeeds, and it is freed like any other box when its
own count reaches zero (§6.7). `@w`: lock the box; `t = load
target`; if null → unlock, `nil`; else if `t` is `IMMORTAL` → unlock,
`(some t)` with no count operation (its count is 0 and it never dies,
§8.2); else "retain if count > 0" on `t` (a plain increment when not
`SHARED`; a `cmpxchg` loop refusing zero when `SHARED`); unlock; return
`t` or `nil`.
`fib.weak-clear obj`: lock the box, store null, unregister, unlock;
called from `fib.drop` before the children are released. The box is
freed when its own count reaches zero and its target is null or
`IMMORTAL`. Only
objects that had a `weak` taken pay: one flag test in `fib.drop`, and
the table is touched only by `weak` and `weak-clear`. `weak`
force-allocates its argument on the heap (§6.11).

### 8.8 Threads, share marking, tasks

`(spawn f)`: `fib.share f` walks `f` and everything reachable through
`fib.types[tid].trace`, setting `SHARED` on each object with the
`atomicrmw or` of §8.2, stopping at objects already `SHARED` and at
`IMMORTAL` ones; an atom is locked, its content walked, unlocked; a
weak box is locked and its live target walked; a task is walked through
its captures and result only (below). The task object is allocated with count 2 (§6.8): the caller's handle
and the thread's. Then the runtime hands `f` and the task to a new OS
thread (or a pool); the thread calls `f`, share-marks the result, stores
it, sets the state to done, signals, and finally `fib.release`s the
task; the caller's `join` waits, retains the result, returns it.
Whichever release is last frees the task, so a discarded handle never
leaves the thread writing into freed memory. Process exit: `main`'s
return joins every thread still running before the result is returned
to the OS (§6.8). The interpreter's `mark_shared`
follows the same edges (`fibref` `heap/shared.rs`).

`(Task T)`: `(i64 i32 i32  i32 state  i32 driver  i32 lock  ptr resume
ptr result  ptr waiters  ..captures ..locals)`. `async` lowers to a
state machine, not to LLVM coroutine intrinsics: `resume(task)`
switches on `state`, runs to the next `await`, stores the live locals
into the task object and returns; when the body's last step has
returned — a call in tail position included, which is an ordinary
`call` there (§6.10 rule (f)) — it completes the task with that value
(below) and returns. The executor is multi-threaded
(**Decided**, D7): a pool of worker threads takes tasks from a shared
run queue, so any worker may resume any task, which is why a task's
captures must be `Send` and are share-marked at creation (§5.5, §6.9).

**One driver at a time, any number of waiters** (**Decided**).
`Send (Task T) = Send T` (§1.6), so one task can be joined from
two threads (`(plet ((a (join t)) (b (join t))) (+ a b))`) or awaited
by two tasks, and the checker accepts both (proposed cases 43, 44). A
count on the task is what keeps it allocated; it is not mutual
exclusion, and two threads calling `resume` on one task would both
rewrite its `state`, locals and `result`. Exclusion is the runtime's:

- `resume` is entered only after `cmpxchg driver 0 → 1` (acquire)
  succeeds, and `driver` is stored back to 0 (release) when the resume
  returns. A `join`/`block-on` or an executor worker that loses the
  race never touches the frame: a worker leaves the task to its current
  driver, and a `join` blocks on the task's completion signal (a futex
  or condition variable in the runtime's per-task record, broadcast to
  every joiner) instead of resuming. So the live locals of a task are
  only ever read or written by its current driver, and a task that
  migrates between workers is handed over through the run queue's own
  synchronisation.
- `state` and `result` are written once, by the driver that completes
  the task: `result` first, then `state := done` with `atomic-store
  release`; readers use `atomic-load acquire`. The result is
  share-marked before the store (§5.5).
- `await e` takes `e`'s `lock` (a spinlock like an atom's, §8.6),
  re-checks `state` under it so that a completion cannot slip between
  the check and the registration, and appends the awaiting task's
  waker to `e`'s **waiter list**; a single waker slot would let a
  second registration overwrite the first, leaving one awaiter never
  woken and the count its registration took never released. Each
  registration retains the awaiting task; completion takes the list
  under the lock, wakes every entry and releases it; the awaited
  task's drop releases whatever is still registered. `e` itself is an
  `Owned` temporary of the `await` step, held in the task's frame until
  the await completes, so the awaited task is never freed while
  something awaits it. An `await` on a task already `done` returns the
  retained result at once.
- A task's `trace` covers its captures and its result, never its live
  locals, so `fib.share` (above) never reads a running frame: the
  captures were share-marked at creation (§5.5), the result before
  completion, and the locals belong to the current driver alone.

`join`/`block-on` drive the executor until `state == done`, claiming
the task's `driver` whenever they resume it themselves, then retain and
return `result`. The task's `drop` releases captures, live locals, the
result and any waker still registered. Every holder that may resume or
complete a task — a handle, the run queue, a spawned thread, a
registered waker — holds a count on it; nothing touches a task it does
not hold, and nothing resumes a task it has not claimed. The
interpreter may run tasks as coroutines; frees must match.

### 8.9 Calls, returns, tail calls

- A `defun` is `(define (R) ((A₁ p₁) .. (Aₙ pₙ)) ..)`; `&` parameters
  are `ptr`; `unit` results are `void`. An owned parameter (§6.4) is an
  owning binding of the frame: `fib.release` on every exit path unless
  it was moved out, which only its being the function's value or an
  argument of a tail call does; stored, captured or passed to `spawn`,
  it is retained there (E2–E4) and still released at the exit; a
  borrowed parameter emits nothing.
- An ordinary call: `call`; for each argument at an owned position of
  the callee, `consume` before the call (`fib.retain` a
  `Borrowed`/`Derived` argument; an `Owned` temporary is moved);
  nothing for a borrowed position. The caller releases its remaining
  owned temporaries after the call returns and after the write-backs
  (§6.3); the callee retains at its own escape positions. No other
  ownership passes through a calling convention except `spawn`'s
  argument (consumed).
- A tail call (§6.10): evaluate the head and the arguments; `consume` each
  argument for its position, moving the frame's owning bindings that are
  arguments; run the scope-exit releases of the enclosing scopes,
  release the step's other temporaries and the frame's owned parameters
  not moved (`env` among them in a closure body); then
  `(tailcall @g args..)` for a known callee, or
  `(indirect-tailcall code R env args..)` through a closure value with
  the consumed closure object as `env` (§8.4); nothing follows in the
  function. An `async` body has no tail call: a call in its tail
  position is a `call` whose value `resume` stores as the task's result
  (§6.10 rule (f), §8.8). A `loop`'s `recur` is the sequence §6.10 gives
  for `recur` — moving only what it exits and releasing no owned
  parameter, since the frame stays — ending in `(br loop)` on
  entry-block `alloca` slots for the loop variables, since it cannot
  leave the function. lIR's `tailcall` must lower to `musttail` (§8.11):
  a frame that stays would make a 100000-deep recursion overflow where
  the interpreter does not, and method.md rule 6 requires the two to
  agree.
- A call in tail position that §6.10 makes ordinary — rules (b), (e)
  and (f), or an extern — is `call` followed by the write-backs and
  releases, then `ret`, or in an `async` body the task's completion
  (§8.8).

### 8.10 What the compiler must emit, summarised

| Event | Emission |
|---|---|
| object literal, constructor, `cell`, `atom`, closure creation | `fib.alloc`, header init, `consume` each stored field/capture (E2/E3) |
| E1 return of `Borrowed`/`Derived` | `fib.retain` before `ret`; an owned parameter that is the result is moved out instead (no operation) |
| E2 store of `Borrowed`/`Derived` | `fib.retain` before the store |
| E4 `spawn` | `consume` the closure, `fib.share`, allocate the task with count 2, hand both to the runtime |
| join retain (`if`/`match`) | `fib.retain` at the tail of each non-`Owned` branch |
| scope exit | `fib.release` each owning binding not moved out, in reverse order, on every exit path |
| `Derived(x)` result leaving `x`'s scope | `fib.retain` the result, then the releases |
| argument at an owned position of a known callee, or any argument of a call through a closure value | `consume` before the call: `fib.retain` a `Borrowed`/`Derived` argument, move an `Owned` one (§8.9) |
| `Owned` temporary at a borrowed position | `fib.release` after the call returns and its write-backs |
| named function used as a value | its `IMMORTAL` closure, whose code is the all-owned body `f.owned`, emitted by the monomorphiser for every function whose value is taken and for its SCC (§8.4) |
| owned parameter | `fib.release` on every exit path, a tail call's jump included, unless it is the function's value or moved into the tail call (§8.9); a store, capture or `spawn` of it retains (E2–E4) and does not spare it the release |
| non-final `do` step with an `Owned` value | `fib.release` at the step's end |
| `@c`, `@a`, `@w` | as §8.6/§8.7 (always a `fib.retain`; atom under lock; weak "retain if alive") |
| `set!`, `reset!`, `swap!` | as §8.6: `consume` new, share if `SHARED`, store, `fib.release` old |
| `&` copy-in | acquire: `fib.retain` the variable's content and store it into the private cell (§6.6); nothing for a forwarded `&v` in a self tail call, nor for the `&` operand of `array-set!` or `set-field!`, which update the variable's own cell (§2.13) |
| `&` write-back | store the private cell's content into the variable; `fib.release` the variable's old content |
| `array-set!`, `set-field!` | `fib.unique?` test (flags first, then the count, §8.2); in-place write, or copy + store + `fib.release` old |
| module initialiser `fib.init.<module>` | called by the entry point before the program's `main`, modules in dependency order (§8.2): allocate and fill the type table, build every static object (literals, named-function closures, vtables) with `fib.alloc` and an `IMMORTAL` header, store each address into its `ptr` global |
| `def` initialisation | in the module initialiser after its static objects, `def`s in source order: evaluate the constant expression and `fib.immortalise` its value (syntax §3.19); as static data once lIR holds struct-typed constants (§8.11) |
| stack object (`STACK`: private `&` cell, stack closure, scope-local object, §6.11) | `(alloca i64 (i32 k))`, `k` the layout in 8-byte words, one per allocation site in the function's entry block, reused by every execution of the site; header stored, fields through `getelementptr` on the struct type (§8.2, §8.4, §8.6) |
| `(weak x)` | box lookup or allocation under the table mutex; `HAS-WEAK` set with `atomicrmw or` (§8.7) |
| tail call, `loop`/`recur` | §8.9: consume the arguments, run the releases, `tailcall`/`indirect-tailcall` or `br` |
| `raw-retained` | `consume` the operand; the count is the foreign side's until `release-raw`/`fib_release` |
| `drop` per type | `fib.release` each object field; `fib.weak-clear` if flagged; `free` |
| stack object scope end | inline `drop` body, no `free` (§6.11) |

### 8.11 lIR hardening this mapping relies on

From `lir-audit/`: a whole-module type checker run by default (method.md
rule 7); `indirect-call` carrying the full function type (today every
argument is typed `ptr`, so §8.4's calls pass scalars through
`ptr`-sized words until then); string globals that load correctly; a
`fence` with real cross-thread semantics; `atomicrmw`/`cmpxchg`/
`atomic-load`/`atomic-store` documented in `doc/lIR.md` (they are parsed
and lowered but undocumented); `tailcall` and `indirect-tailcall`
lowered to `musttail` — today they are `tail`, a hint LLVM may ignore
(`lir-audit/README.md`) — under a calling convention that guarantees a
tail call between differing prototypes (LLVM's `tailcc`; under the C
convention `musttail` requires matching prototypes, which mutual
recursion and calls through closure values do not have), which §8.9
relies on: without it a compiled tail recursion of depth 10⁶ (proposed
cases 52 and 53) overflows the stack where the interpreter does not,
failing method.md rule 6. Conveniences this mapping
does *not* depend on but would use if added: `switch` (§8.3 uses
`icmp`/`br` chains for `match`), an array type in the type grammar
(§8.3 addresses trailing elements with `getelementptr`), a named
struct as the element type of `alloca` (§8.2 sizes stack objects in
words and addresses them with `getelementptr` on the struct type), and
struct-typed `global`s and constants with function addresses and
nested aggregates as initialisers (§8.2 builds the type table, literal
objects, constant closures and vtables in the module initialiser and
reaches them through `ptr` globals; with them, all of it becomes
static data). `lair` today parses `(alloca %struct.T)` and `(global g
%struct.T ..)` as `UnknownType` and rejects `(global g ptr @f)` as an
unimplemented initialiser, which is what fixed the v1 shapes. Nothing
here needs ADR 021's safe lIR.

---

## 9. What the checker prints

`fibc --explain file.fib` prints, per `defun`, `fn` and `impl` method,
the complete output of §3, §5 and §6 so that a verdict can be checked by
eye and diffed against the interpreter's trace (**Decided**):

```
defun dup-all : (fn ((& (Vec i64))) unit)
  params:    v  &param  type=(Cell (Vec i64))  escapes=no
  bindings:  (none)
  closure @8:15  escaping=no  heap=no  reason=arg-to-borrow(for-each.f)  captures: v (alias)
  calls:     @8:3  for-each(@v, <closure>)   call (e: fresh argument at borrowed position for-each.coll)   @v: acquire
             @8:24 append(&v, x)             call (b: & argument)   &v: acquire
  ops:       L8 retain [@v]; L8 call for-each; L8 release [@v]

defun build : (fn (i64 (Vec i64)) (Vec i64))
  params:    n  scalar;  acc  owned (returned; tail call passes fresh)  escapes=yes
  calls:     @3:18 build(n-1, (conj acc n))   tail-call   arg 2: moved;  acc: released before the jump
  ops:       L3 base: retain [acc] (join), release [acc] (exit);  L3 else: call conj, release [acc], tailcall build
```

Every line is one of: a parameter (type, count kind `owned` or
`borrowed` with the rule of §6.4 that decided it, summary), a binding
(`owns` / `alias-of b` / `derived-of b`, and `scope-local` when §6.11
holds), a closure literal (`escaping` with the clause of §6.5 that
decided it, `heap` with its reason — `arg-of-tail-call`,
`head-of-tail-call`, `arg-of-recur`, `arg-of-tail-site` or
`head-of-tail-site` (at a tail site that §6.10 rule (e) then makes
ordinary), or the escaping use — and its
captures with their kinds), a call (`tail-call`, or `call` with the
rule of §6.10 that made it ordinary; the copy-in of every `&` argument,
`acquire` or `forward`), a colour solution (`ς₁ = local, forced by
capture n`), and the emitted operations with source lines; a function
whose value is taken has a second entry, `defun f.owned`, for its
all-owned body (§8.4). Reject cases print the error and the rule
number of this document. The interpreter's trace (`fibref`
`heap/event.rs`) lists every retain, release, free, end of a stack
object's scope (§6.11), frame exit and tail-call jump with the same
line numbers, so the compiler's `ops` and the interpreter's trace must
free the same objects at the same lines (method.md rule 6).

## 10. Decision record and open items (types and checker)

On 2026-09-27 the owner signed off the 33 items of the three review
rounds, each as recommended except where the table says otherwise,
together with seven amendments:

- **D1** copy-in always acquires: no copy-in move, no exclusive places,
  no takeable structs, no taken fields, no `&(. x f)` places, no
  `fib.takeable?`, no taken-field audit event (§6.6, §8.2);
- **D2** `&` parameters are cells read with `@v`, never values (§2.14);
- **D3** HM inference generalising only at top-level `defun` SCCs; `let`
  and `fn` never generalise; no coercions; no literal polymorphism
  (§1.1, §1.7, §2.4, §3);
- **D4** closure colours as the two-point lattice in function types
  (§5.4);
- **D5** full tail calls with Lean-4-style owned parameters, replacing
  "only self tail calls become loops" (§6.3 E6, §6.4, §6.5, §6.10,
  §6.12, §8.4, §8.9, §8.11; ownership.md §2–§4);
- **D6** stack allocation of scope-local objects in v1, decided on the
  binding over all paths (§6.11, §6.12, §8.2, §8.10);
- **D7** `async` captures must be `Send`; the executor is
  multi-threaded (§2.8, §5.2, §6.9, §8.8).

Each item is now **Decided** in the section that states its rule; the
old numbering is kept here because the drafts and
`spec/drafts/PROPOSED_CASES.md` cite it.

| Item | Rule | Now in |
|---|---|---|
| 1 | HM with generalisation at `defun` SCCs only; `let`/`fn` never generalise (D3) | §3, §2.4 |
| 2 | two-point colour lattice with `⊑` at flow sites (D4) | §5.4 |
| 3 | copy-in moves when exclusive — **decided the other way (D1): always acquire** | §6.6 |
| 4 | cell reads owned and never elided | §6.3, §6.7 |
| 5 | escape kinds declared on methods; closure types carry none | §6.4 |
| 6 | only self tail calls become loops — **replaced by D5: every admissible call in tail position is a tail call; kinds inferred** | §6.10, §6.4 |
| 7 | monomorphisation by layout class / full type; whole program | §4.3 |
| 8 | heap everything in v1 — **replaced by D6: stack allocation from the first implementation** | §6.11 |
| 9 | `async` captures `Send` (D7) | §2.8, §5.2, §6.9 |
| 10 | atoms: per-atom spinlock, `swap!` retries | §8.6 |
| 11 | `(dyn P)` not `Send`; `Send ptr` false | §4.4, §5.1 |
| 12 | no coercions anywhere (D3) | §1.7, §3 |
| 13 | field access and `deref` fixed by the end of the SCC | §3.4 |
| 14 | `Option` as a nullable pointer only for non-`Option` object payloads; class `opt` | §8.1, §4.3 |
| 15 | instances keyed by `(P, head)`; no overlap, defaults or supertraits | §4.1 |
| 16 | `write-unique` obligation on fibref | §6.6, §6.12 |
| 17 | leak-cycle classification is fibref's | §6.7 |
| 18 | whole-scrutinee pattern variable takes the scrutinee's mode | §6.1, §6.3, §6.4 |
| 19 | an `&` parameter is not a value (D2) | §2.14 |
| 20 | instance contexts declared with `:where` | §2.7, §3.5 |
| 21 | `fib.takeable?` and taken fields — **dropped with D1: nothing is ever taken** | §6.6 |
| 22 | task created with count 2; `main` joins running threads | §6.8, §8.8 |
| 23 | `async` retains every capture at creation | §6.9 |
| 24 | `weak` of an `IMMORTAL` object | §6.7, §8.7 |
| 25 | `match` as `icmp`/`br`; untyped `indirect-call`; no new lIR types required | §8, §8.11 |
| 26 | `raw-retained`/`release-raw` | §6.13 |
| 27 | one driver, any number of waiters; `Task` stays `Send` | §8.8, §6.8 |
| 28 | specialisation keys | §4.3 |
| 29 | flags word read and written atomically | §8.2, §8.7, §8.8 |
| 30 | `fib.unique?` tests flags first; immortal and stack objects carry count 0 | §8.2, §6.6 |
| 31 | `def` values typed closed, in dependency order, immortalised before `main` | §2.16, §3.5, §8.2 |
| 32 | a closure passed at a self tail call is escaping — **generalised by D5: heap at any tail call (E6), escaping only by its uses**; read since the fifth review round as heap at any tail site, admitted or not | §6.5, §6.10 |
| 33 | static objects built by the module initialiser; word-sized stack `alloca`s | §8.2, §8.10 |

**Fourth review round.** A review of the sections that D1–D7 touched
found places where their application broke a decided rule or a
property the text itself states. Each was corrected in the section
that states it, as a correction of how an amendment was applied, not a
change to a decided rule, and is listed here for the owner's review: a
capture of a heap closure is frame-owned and a `fn` body belongs to no
SCC (§6.4, §6.10 rule (e)); a named function's closure runs its
all-owned body instead of an adapter with a frame (§8.4; the other fix
offered, making owned every parameter of a function whose value is
taken, fails across modules, since an importer cannot change the kinds
a module exports); the interpreter follows the parameter convention at
every call (§6.12 exception 5; syntax §2); a named `fn`'s self-name is
a use of the literal (§6.1, §6.5); only an allocation in the frame can
be scope-local, never a call result or a loop variable (§6.11); a
`recur` moves only the bindings it exits (§6.10); each stack
allocation site has one `alloca` in the entry block (§8.2; the
alternative was `stacksave`/`stackrestore` in lIR); a tail call
releases the frame's owned parameters (§6.10 step 3), and case 07's
base path is a retain and a release, not a move (§6.10, §7); case 08
no longer pins the `@v` acquire (§7). They come with proposed case 49,
restated, and cases 52–69. Two findings of the round needed a decision;
the owner's decisions are recorded at the end of this section.

**Fifth review round.** A review of the same sections after those
corrections found more places where the application of D5–D7 broke a
decided rule or a property the text states. Each was corrected in the
section that states it and is listed here for the owner's review in the
same way: a call in tail position of an `async` body is never a tail
call, since the task's completion follows it, and an `async` body
belongs to no SCC (§6.10 rule (f); §6.3, §6.4, §6.9, §8.8, §8.9; syntax
§2, §3.1, §3.2, §3.14); an owned parameter is moved out only as the
function's value or at a tail call, and one that is stored, captured or
spawned is retained there and still released at the exit (§6.4, §6.12,
§8.9, §8.10); tail sites are fixed before the kinds, the summaries and
the heap closures, which are one fixpoint, and the admission of tail
calls follows it, since decided together the rules had no consistent
solution for some programs (§6.10, §6.4 rule 2, §6.5, §3.5, §9; syntax
§3.2; the E3 row of §6.3 now names heap closures, as rule 1 and §6.5
already did); a value that a loop variable receives is followed through
it by rule 1 and the escape summary (§6.4); a loop initialiser keeps a
binding off the stack (§6.11, §8.2); a private `&` cell is sized by its
content, four words for a `(dyn P)` (§8.6); and the case 08 trace uses
the case's own names (§6.6). They come with proposed cases 70–78 and
corrections to entries 21, 37, 58, 59 and 60. None changes the verdict
of a case in `cases/`. The one that chose between readings, tail sites,
extends to every closure and to rule 2 what the text already did for a
named `fn`'s self-name, whose argument uses in tail position counted
before admission, and narrows it to the calls that can be admitted at
all, so that a closure passed beside a fresh argument at a borrowed
position, as in case 08, stays on the stack.

**Sixth review round.** A review of the sections that D1–D7 touched,
after those corrections, found two more places where the text
contradicted a decided rule or itself. Each was corrected in the
section that states it and is listed here for the owner's review in the
same way. First, the interpreter ends a scope-local object at the exit
of its binding's scope — the `let` or `match` exit, the step's end, the
scope exits that a tail call or a `recur` runs — and not at the
function's exit, which kept a count alive past the point where the
compiled program drops it and so changed what a later unique write
sees; an `async` body's scopes stay open across `await` in the task;
and the stack-reference audit error is a read after the scope has
ended (§6.11, §6.12 exception 4, §9; the rule applied is ownership.md
§2's "freed when the scope ends"). Second, the `&` primitives are
written as the signatures they are, not as types; `set-field!`, whose
field operand is a name, is a primitive form with a rule of its own,
like the conversions and `dyn`; the `&` clause of §2.2 names the
primitives; the target of `set!` is an expression of cell type or the
name of an `&` parameter, which is not an expression (D2); the `&`
operand of a primitive takes no copy-in, as §6.6's unique write
already assumed; and a unique write closes a cycle only through the
cell that holds the object it writes, which is all that the
`ImmutableCycle` argument needs (§1, §1.4, §2.2, §2.9, §2.13, §6.6,
§6.7, §6.14, §8.10; syntax §2, §3.11, §3.13, §4.3). They come with proposed
cases 79 and 80. None changes the verdict of a case in `cases/` or a
decided rule. Proposed case 80 passes a cell beside an `&` argument
naming it; under the owner's decision on two names for one cell (option
(A), below) it stays `accept`.

### Decided on the last open items

The owner decided the three items the fourth review round left open:

1. **`&` forwarding at any tail call** (recommended option): rule (b) of
   §6.10 admits a tail call whose every `&` argument forwards an `&`
   parameter of the enclosing `defun`, at any position and to any
   callee (§6.6, §6.10).
2. **Two names for one cell in one call: option (A).** The
   distinct-variables check stays on names. ownership.md §5, syntax §2
   and syntax §3.13 now say that a cell reaching a call under another
   name can be written through it during the call and that the later
   write-back wins; such a call is memory-safe. Proposed case 67 is
   `accept` with the lost update.
3. **ownership.md §5 and §8 reworded** as recommended: §5 describes the
   private cell read with `@v`; §8 cites case 14 for the `&` rule and
   case 11 for the retain on entry.

Nothing is open.
