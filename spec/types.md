# fibber types and the ownership checker

Status: synthesis of the three drafts under `spec/drafts/` (see
`spec/drafts/SYNTHESIS.md`). Authority: [ownership.md](ownership.md) is
Decided. **Decided** marks a direct consequence of ownership.md;
**Proposed** marks everything else. Surface syntax is in
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
· §8 mapping to lIR · §9 what the checker prints · §10 open decisions.

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

Signatures of functions with `&` parameters are written `(fn ((& T) ...) R)`
and are not types (§1.4).

### 1.1 Scalars and literals

Scalars are copied and have no identity, no count and no mode (**Decided**,
§1). `bool` is lIR `i1`; the integers are the lIR integers; `f32`/`f64` are
`float`/`double`; `char` is a Unicode scalar value in an `i32`; `keyword`
an interned id in an `i64`; `unit` has the one value `()` and is erased
in codegen; `ptr` is a raw pointer, usable only inside `unsafe`. No
implicit conversion exists between any two scalar types (Proposed;
consistent with lIR's no-promotion rule, liar ADR 017 is out).

An integer literal has the width of its suffix, `i64` without one; a
float literal `f64` unless suffixed `f32`. There is no literal
polymorphism (syntax Open decision 10): `(+ x 1)` pins `x : i64`;
`(+ x 1i32)` pins `i32`.

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
(Proposed; rarely needed on a `defun`, since bounds are inferred;
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

Closure types carry no capture list and no escape summary (Proposed;
§10 item 5): a call through a function value treats every argument as
escaping (§6.4), which is always safe.

A `defun` with `&` parameters has a **signature** `(fn ((& T₁) T₂ ..) R)`
rather than a type: it can be called with `&place` at those positions
and may not be referenced as a value (`function with & parameters is
not a value`). Inside its body an `&` parameter `v` has type `(Cell T)` and is not a
value: it occurs only as `@v`, `&v`, `&(. v f)` or the target of `set!`
(syntax §3.13, §2.14).

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
produced only by the explicit primitive `(dyn P e)` (Proposed: no
subtyping, no coercion anywhere in the type system; §10 item 12).

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
| `x` bound locally | `Γ(x)`, monomorphic; an `&` parameter `v` has `Γ(v) = (Cell T)` but is not an expression: it occurs only as `@v`, `&v`, `&(. v f)` or the target of `set!` (§2.14), else `& parameter v used as a value in f` |
| `f` a global defun, constructor, variant constant or protocol method | `inst(σ_f)`; error if `f` has `&` parameters (`function with & parameters is not a value`) |
| `g` a `def` name (syntax §3.19) | `Γ(g)`, its closed monomorphic type (§2.16); a global like `f`, never a capture |
| `[e₁ .. eₙ]`, `{k v ..}` | rewritten to prelude calls before typing (syntax §1.4) |
| `'form`, `` `form `` | `Form`; inside a quasiquote `,e` needs `e : Form` and `,@e` needs `e : (Vec Form)` |

### 2.2 Calls

```
(f a₁ .. aₙ)     f : (fn κ (T₁ .. Tₙ) R),  aᵢ : Tᵢ          ⇒ R
(g .. &p ..)     g a defun or protocol method with & at that position of value type T;
                 p a place (syntax §3.13) of type (Cell T)      ⇒ ok for that position
```

Arity must match exactly. Head position may be any expression of function
type; a call to a `defun` or method instantiates its scheme. `&p` at a
non-`&` position, or a plain argument at an `&` position, is a type
error (`parameter v of g is &; pass &x`). A place `(. x f)` requires `x`
to be a variable of type `(Cell S)` and `f` a field of object type of
the struct `S` (§2.14). A field-less variant is a constant, not a
function: `(V)` with zero arguments is the error `V is a constant, not a
function; write V` (syntax §3.9).

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

`let` never generalises (Proposed; §10 item 1): `(let ((id (fn (x) x)))
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
| `(defprotocol (P s d̄) (m (self x₁: T₁ ..) -> R) ..)` | `m : ∀ s d̄ b̄. (P s d̄) ⇒ (fn :send (s T₁ ..) R)` where `b̄` are the signature's other variables; the functional dependency `s → d̄` is recorded; each parameter's escape kind (`:borrow` or the default, escaping) is recorded (§6.4) |
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
set!    : ∀a. (fn :send ((Cell a) a) unit)         ; first argument any expression of cell type
```

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
division and remainder trap on zero; signed overflow wraps (Proposed;
syntax Open decision 16). Conversions are primitive forms whose first
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
and counted (Proposed):

```
array      : ∀a. (fn :send (i64 a) (Array a))                     ; n copies of init (each stored: E2)
array-len  : ∀a. (fn :send ((Array a)) i64)
array-get  : ∀a. (fn :send ((Array a) i64) a)                      ; traps out of range; result is a borrow of the element inside the body, owned to the caller (§4)
array-with : ∀a. (fn :send ((Array a) i64 a) (Array a))            ; a new array with one slot changed (E2 for the element)
array-copy : ∀a. (fn :send ((Array a) i64 i64) (Array a))          ; slice [i, j)
array-set! : ∀a. (fn :send ((& (Array a)) i64 a) unit)             ; in place iff unique, else copy (§6.6)
set-field! : (fn ((& S) F) unit) for a struct S with field f : F   ; (set-field! &s f e); in place iff unique, else copy
```

`array-set!` and `set-field!` are the only primitives that perform a
**unique write** (§6.6); everything else that updates in place is
library code over them.

### 2.14 `&` parameters and places

In `(defun f (&v ..) b)`, `Γ(v) = (Cell T)` with `T` the annotation or
fresh; `T` is unified by the uses of `@v`, `&v`, `(set! v e)` and the
primitives. `v` itself is not an expression (Proposed; syntax §3.13; §10
item 19): the only forms in which it may occur, in `b` and in every
closure literal inside `b`, are `@v` (`(deref v)`), `&v`, `&(. v f)` and
`(set! v e)`; any other occurrence is `& parameter v used as a value in
f`. So no expression ever has the private cell as its value, and the
checker assigns no mode to `v` (§6.6). At a call, `&x` requires `x :
(Cell T)` for the parameter's `T`; `&(. x f)` requires `x : (Cell S)`
with `HasField(S, f, T)`, i.e. the struct `S` in `x` to have the field
`f : T` of object type; `x` may be an `&` parameter or any other
variable of cell type (syntax §3.13). Any other `&` argument is `&
argument must be a cell variable or a field of a cell variable`.

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
f depend on each other` (Proposed; syntax Open decision 29). `T` is
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

**Name (Proposed):** constraint-based Hindley–Milner with generalisation
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
(Proposed; §10 item 13).

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
   f. run the ownership pass on the SCC (§6): summaries as a fixpoint within the SCC
6. type-check impl method bodies against their signatures under their declared contexts (§2.7);
   each impl is its own SCC and may call any defun, whose schemes are complete by now because
   instance contexts are declared, not inferred from these bodies (§10 item 20)
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
bounds, instantiations at every call, escape summaries, escaping
closures, allocation sites.

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
summary of every parameter (§6.4), for a `def` its closed type (§2.16),
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
overlapping instances are rejected (Proposed; §10 item 15). Protocol
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
scheme (Proposed; §10 items 7 and 28):

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

### 5.4 Closure colours (Proposed)

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
object-typed expression, the kind of every binding, whether each closure
literal escapes, the copy-in decision at every `&` argument, and the
retain/release operations to emit. Scalars are ignored throughout: a
rule that says "retain" is a no-op on a scalar-typed expression.

### 6.1 Modes and bindings

Every object-typed expression has one of three modes (Proposed; the
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
| **owning**: a `let` binding whose initialiser is `Owned`; a capture of an escaping closure; a parameter slot of a self-tail-call loop (§6.10); an implicit temporary | `Borrowed(b)` | yes, unless moved out |
| **parameter** (a plain parameter of a `defun`, `fn` or method) | `Borrowed(b)` | no (**Decided**, §4) |
| **`&` parameter** (its value is its private cell, which is never an expression: syntax §3.13) | never read: `@v` is `Owned`, `&v` forwards the cell, `(set! v e)` writes it (§6.6) | no (the cell is the caller's) |
| **alias of `b'`**: a `let` binding whose initialiser is `Borrowed(b')`; a capture of a non-escaping closure; a pattern variable that binds the *whole* scrutinee (a top-level symbol pattern or a top-level `:as`) of a scrutinee whose mode is `Borrowed(b')` | `Borrowed(b')` | no count |
| **derived of `b'`**: a `let` binding whose initialiser is `Derived(b')`; a pattern variable bound *inside* a variant or struct pattern (a payload or a field) of a scrutinee whose binding is `b'`; a whole-scrutinee pattern variable of a scrutinee whose mode is `Derived(b')` | `Derived(b')` | no count |
| **global**: a `def` name (syntax §3.19, §2.16) | `Borrowed(g)` | never: its value is immortal (§8.2), so every count operation `consume` would emit on it is a no-op the compiler may omit |

`Derived(b)` always names a *strict* sub-object of `b`'s value, reached
through a field, an element or a variant payload; a pattern variable
that binds the whole scrutinee is the scrutinee's value and takes the
scrutinee's mode (Proposed; §10 item 18). The escape summary of §6.4
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
| `(loop ..)` | the mode of its body, adjusted by the scope-exit rule with the loop's slots as its owning bindings (§6.10) |
| `(recur ..)` | no value; its arguments are consumed into the slots (§6.10) |
| `(do .. e)` | the mode of `e` |
| `(if c a b)`, `(match ..)` | the **join** (§6.3) of the branch modes |
| `(unsafe b)` | the mode of `b` |
| `(set! ..)`, `(reset! ..)`, `(array-set! ..)`, `(set-field! ..)` | `unit` |

### 6.3 Where counts change: escape positions, consume, scopes, joins (§3)

**Decided** (§3): a reference escapes when it is (1) returned, (2) stored
into an object, (3) captured by a closure that escapes, (4) passed to
another thread, (5) held across an `await`. Each is a position in the
syntax, and one rule applies at every one of them:

```
consume(e):   Owned        → move: no operation; the count travels with the value
              Borrowed(b)  → retain
              Derived(b)   → retain
```

| # | Position | Forms |
|---|---|---|
| E1 | return | the value of a `defun`, `fn`, method or `async` body (after the body's own scope exits) |
| E2 | store | the object arguments of struct and variant constructors; `(cell e)`; `(set! c e)`; `(atom e)`; `(reset! a e)`; `(array n e)` (n copies); `(array-with a i e)`; `(array-set! &a i e)`; `(set-field! &s f e)`; the result of `f` in `(swap! a f)`; the operand of `(raw-retained e)` (§6.13). Stores inside library functions are those functions' business: `conj`'s element is stored by `array-with` inside `conj` |
| E3 | capture | each object capture of a `fn` that is escaping (§6.5) and of every `async`; the retain happens when the closure or task object is created |
| E4 | thread | the argument of `spawn` (consumed; E3 already retained its captures) |
| E5 | await | subsumed by E3: the task's frame owns everything it references (§6.9) |

Everything else emits no count operation:

| Position | Rule |
|---|---|
| argument to a plain parameter of any callee, known or unknown | none (**Decided**, §4). An `Owned` argument is a temporary of the call step, released after the call returns and its write-backs are done |
| `let` binding `(x e)` | `e` `Owned`: `x` owns it. `e` `Borrowed(b)`/`Derived(b)` with `b` alive for `x`'s whole scope: `x` is an alias/derived binding of `b`, no count. `e` `Derived(t)` where `t` is a temporary of the initialiser step: the step is a scope (next row), so `x` owns a retained reference |
| **scope exit** of a `let` with body mode `m`, or of a step with temporaries | if `m = Borrowed(x)` and `x` is an owning binding of this scope: **move out**: `x` is not released and the result is `Owned`. If `m = Derived(x)` and `x` owns in this scope: retain the result, then release the scope's owning bindings; result `Owned`. Otherwise release every owning binding of the scope in reverse order; result mode unchanged |
| `match` scrutinee | `Owned`: an implicit owning binding `t` for the whole form; a pattern variable that binds the whole scrutinee is an alias of `t`, one bound inside a variant or struct pattern is derived of `t` (§6.1); the form's value is adjusted by the scope-exit rule with `t` as the scope. `Borrowed(b)`: whole-scrutinee variables are aliases of `b`, the rest derived of `b`. `Derived(b)`: every pattern variable is derived of `b` |
| `let` with a pattern | as a one-clause `match` whose scope is the `let` |
| **join** of `if`/`match` branches | all branches `Borrowed(b)` for one `b`: `Borrowed(b)`. All branches `Derived(b)` for one `b`: `Derived(b)`. Otherwise, including `Borrowed(b)` mixed with `Derived(b)`, `Owned`, and every branch that is not `Owned` gets a retain at its tail (**Decided** by case 04; the mixed case is Proposed, §10 item 18: read as `Derived(b)` it hid a `Borrowed(b)` occurrence from the escape summary of §6.4, and read as `Borrowed(b)` the scope-exit rule would move `b` out when the other branch had returned a sub-object of `b`, leaking `b`) |
| non-final `do` step | `Owned`: release at the step's end |
| `defun`/`fn` body (E1) | `consume` the body's value (every `let` and `match` inside it has already exited by the scope-exit rule, since they are expressions), then release the temporaries of the final step, then the loop slots of §6.10 if any |
| `@c` read | always an acquire (+1) with a matching release at the end of the value's scope or step; **never elided** in v1 (Proposed; §10 item 4). No elision of a cell read is sound on syntax alone: a callee can reach the same cell through any argument, field or capture and write it; a sibling sub-expression of the same step can write it (`(array-get (. @s arr) (do (set! s ..) 0))` freed the array under the call when the read was elided); and `count`, `nth` and `+` are protocol methods with user implementations, so "writes no cell" is not a property the checker can decide by name |
| `&` copy-in and write-back | §6.6 |
| `(weak e)` | no count operation; forces `e`'s binding onto the heap (§6.11) |
| `(raw e)` | no count operation; `e`'s binding must outlive every use of the pointer (**Decided**, §9; the programmer's obligation inside `unsafe`) |

These two tables *are* the escape classification. The checker walks the
tree once, labels each object-typed expression with its position, and
emits the operation from the tables; nothing else emits a count
operation. Case 01: `first`'s body returns `Derived(self)` → E1 retain;
`head` returns an `Owned` result → nothing; `main`'s `h` owns one count
and `l`'s list holds the other; both released at scope exits. Case 03:
the inner `let` binds `a` (`Owned`) and `b`; its body is `Borrowed(a)` →
move out, `b` released; `keep` owns `a`'s vector. Case 04: `pick`'s `if`
joins `Borrowed(x)` with `Owned` → retain on the `x` branch; the result
is `Owned` on both paths.

### 6.4 Borrowed parameters, owned results, summaries (§4)

**Decided** (§4): every parameter is a borrow, valid for the whole call
and never freed by the callee; every result is owned; returning a
parameter or something reachable from one is E1, so the callee retains.
Correctness of the count never depends on the caller: the callee retains
at its own escape positions, and a call is never an escape for the
caller.

The pass attaches to every `defun` an **escape summary**: per object
parameter `p`, `escapes` or `noescape`. `p` escapes iff an occurrence
of `Borrowed(p)` is at a `consume` position (E1–E4, a join retain), or
is passed to an `escapes` parameter of a known callee, or to any
parameter of a closure value (unknown callee), or is captured by an
escaping closure, or is the operand of `weak` or `raw-retained`. An
occurrence of `Derived(p)` never counts: a derived value is a strict
sub-object of `p`, reached through a field, an element or a variant
payload (§6.1), and is always a heap object because it was stored into
its container at E2. A pattern variable that binds the whole scrutinee
is `Borrowed(p)`, not `Derived(p)`, and a join that mixes the two modes
retains its `Borrowed(p)` branch, which is a consume position (§6.3);
so `(defun same (p) (match p (w w)))`, `(match p ((Node inner) inner)
((Leaf) p))` and `(match p (x (weak x)))` all make `p` escape
(Proposed; §10 item 18). Computed as a least fixpoint over the module's
call graph starting from `noescape`; imported and protocol summaries are
fixed inputs. An explicit `p :borrow` or `p: T :borrow` on a `defun`
parameter (syntax §3.1) fixes the summary at `noescape` and is checked:
`parameter p of f is declared :borrow but escapes`; the exported
summary is then the declared one, part of the interface, and a closure
literal passed to `p` is non-escaping by contract (§6.5; proposed case
40).

Protocol methods declare their kinds (syntax §3.10; default escaping);
every `impl` body is checked against the declaration: `implementation
of P/m for T makes parameter p escape; the protocol declares it
:borrow`. Callers use the declared kind whichever implementation runs,
which keeps summaries modular under dynamic dispatch and separate
compilation (Proposed; §10 item 5). Externs take scalars only: `(raw
e)` is not an escape of `e`'s binding, `(raw-retained e)` is (§6.13).

What summaries decide, and nothing else: (a) whether a closure literal
passed to that parameter is escaping (§6.5; at a self tail call it is
escaping whatever the summary says); (b) whether a closure that
captures an `&` parameter may be passed there (§6.5); (c) whether the
caller's argument object may live on the stack (§6.11); (d) the
copy-in decision for `&(. x f)` (§6.6). Summaries of `fn` literals are
computed for their own bodies but never exported: closure types carry
none, so a call through a function value assumes every argument escapes.

### 6.5 Closures: escaping, capture modes, the two `&` rules (§3.3, §5)

A `fn` literal is **escaping** unless every use of it is one of:

- (a) called directly at the literal: `((fn ..) args)`;
- (b) passed as an argument to a `noescape`/`:borrow` parameter of a known
  callee (`swap!`, `for-each`, `map`, `reduce`, a `defun` whose summary
  says so);
- (c) it is the *direct* initialiser of a `let` binding `f`, and every use
  of `f` is a call `(f args)` or a use of kind (b), and `f` is not
  captured by any other closure.

A closure at E1, E2, E3 or E4, passed to an `escapes` parameter, passed
to a closure value, or written as an `async` body, is escaping. So is,
**regardless of the callee's summary**, a closure literal that is an
argument of a self tail call of the enclosing `defun` or named `fn`, or
an argument or initialiser of a `loop`/`recur` (§6.10), and a
`let`-bound closure of clause (c) with such a use (Proposed; §10 item
32): the call is a slot loop, which never completes before the creating
scope exits — the jump runs those scope exits and then stores the
argument into a slot that the next iteration reads — so clause (b)'s
soundness argument below does not hold for it. `(defun spin (g n) (let
((x (make))) (if (= n 0) (g) (spin (fn () (count x)) (- n 1)))))` has
`g : noescape`, but the literal captures `x`, which the `let` releases
at the jump; as an escaping closure it retains `x` at E3 and frees it
with itself when the slot is released (proposed case 49). A closure
that captures an `&` parameter and is passed at a self tail call is
therefore rejected by the rule of case 18 below. The check is syntactic
and uses only summaries. A `loop` body (syntax §3.18) is not a closure:
its free variables are the enclosing function's own bindings and it has
no capture set.

- **Escaping closure:** E3 — every object capture is consumed (retained
  if `Borrowed`/`Derived`, moved if `Owned`, which cannot happen for a
  variable) when the closure object is created; the closure's captures
  are owning bindings of the closure, released by its `drop`. A captured
  cell is retained like any object: in case 05 both closures retain `n`,
  the `let` releases its own count, and the cell lives while either
  closure does.
- **Non-escaping closure:** its captures are alias bindings of the
  enclosing bindings; no count; the closure object may live on the stack
  of the creating frame. Sound because uses (a)–(c) all complete before
  the creating scope exits and a `:borrow` callee never stores or returns
  it; a self tail call or `recur` is the one call that does not
  complete before the scope exits, which is why an argument of one is
  escaping (above).

**Decided** (§5, case 18): an `&` parameter may not be captured by an
escaping closure. Check: for every escaping `fn` literal whose capture
set contains an `&` parameter `v` of the enclosing `defun`: `& parameter
captured by escaping closure: v in f`. A non-escaping closure may
capture `v` and use `&v`, `@v` or `(set! v ..)` (case 08).

**Decided** (§5, case 12): the `&` arguments of one call name distinct
places. Check, before typing: the list of `&` argument places of a call
has no duplicates (same variable, or same variable and field): `variable
x passed to more than one & parameter in call to bar`. Purely syntactic;
aliasing of *objects* is fine because both callees start from a copy-in
that leaves the count above one, so each update copies (§5).

### 6.6 `&` in detail: private cells, copy-in, unique updates (§5)

Inside the callee an `&` parameter `v` names its **private cell** and is
never itself an expression (syntax §3.13, §2.14): `@v` yields an `Owned`
reference to the cell's content (an acquire, +1), `(set! v e)` consumes
`e` into the cell and releases the old content, `&v` and `&(. v f)`
forward the cell or one field of the struct in it. The cell object
itself is a stack temporary of the *caller's* call step and is never
counted (§8.6); because `v` has no value, no expression, store, capture
or return can refer to the cell after the write-back frees it.

**Copy-in** at a call `(f .. &p ..)`, in argument order:

- the place `p` is **exclusive** for this call (syntax §3.13): the
  content is **moved** from the place into the private cell. For a
  variable place, the caller's cell holds nothing until the write-back;
  for a field place `(. x f)`, the struct in `x` must be **takeable**
  (`fib.takeable?`, §8.2: `fib.unique?`, that is count 1 and none of
  `SHARED`, `IMMORTAL`, `STACK`, and `HAS-WEAK` clear) and its
  field is **taken** (nulled) until the write-back; a unique struct that
  has a weak box is acquired instead. The conditions guarantee no
  expression can read the place meanwhile: no other argument mentions
  it, no closure captures it, no alias of the cell exists, and no weak
  reference can upgrade to the struct (`fib.unique?` counts strong
  references only; without the `HAS-WEAK` test an upgrade inside the
  call could read the taken field; Proposed, §10 item 21). The
  interpreter records a taken field and reports any read of one as the
  audit failure `read of taken field`; no accepted program reaches it,
  and the adversarial cases (method.md rule 4) are what check that.
- otherwise: **acquire** — the private cell is initialised with `@p`
  (+1); for a field place the struct in `x` is read through `@x`, its
  field retained, and the temporary released.

**Write-back** after the call returns, in parameter order: the private
cell's content is moved into the place: a variable place is stored
(`set!` semantics: the old content, if any, is released; there is none
after a move); a field place is written with `set-field!` semantics
(unique write when the struct is unique, else copy the struct, store the
copy, release the old struct). Then the private cell is freed (no
count). The interpreter implements exactly this decision procedure, so
the compiler's allocations match its allocations (§6.12).

**Self tail calls.** In a self tail call (§6.10) an argument `&v` for the
enclosing function's own `&` parameter `v`, at `v`'s position, performs
no copy-in and no write-back: the slot keeps the same private cell and
the loop continues with it. Any other `&` argument at an `&` position
makes the call an ordinary call. Without this rule the write-backs of a
loop's iterations, which syntax §2 places "after the call returns",
would never run, the caller's cell would be left empty and the final
value would sit in a dead private cell.

**Unique write** (**Decided**, §5): `array-set!` and `set-field!` read the
place's content without retaining, test `fib.unique?` (§8.2: the flags
have none of `SHARED`, `IMMORTAL`, `STACK`, and the count is exactly
1), and either write the object in place or build a copy with the
change, store it into the place and release the old object. The flag
test comes first and is what protects static data (Proposed; §10 item
30): an immortal object (a literal, a `def` value, and every object
reachable from one) has no count, so a value pulled out of a literal
(`(match f ((List xs) xs))` on a `Form` literal, §6.2) and moved into
a private cell is copied by the first `push!`, never written, and the
literal's tail array, immortal too, copies likewise. Without the flag
test an immortal count that happened to read 1 would let accepted code
write into a constant, which in the compiler may live in read-only
data once lIR holds static constants (§8.11), while the interpreter,
whose literals are ordinary allocations, would answer differently (method.md rule 6; proposed case 37). The audited
heap must permit a write to an immutable object exactly under that
test (`write-unique`, an obligation on fibref; §10 item 16), so the
interpreter allocates its literals with the `IMMORTAL` flag and count
0, like the compiler's constants (§8.2). A unique write can never close
a cycle: an object with count 1 held by the writer's place is reachable
from no other object.

Count trace for case 08, compiler and interpreter alike. `main`'s cell
holds `V0` (1). `(dup-all &v)`: `v` is exclusive in `main` (a `(cell ..)`
binding used only as `&v`/`@v`, not in another argument) → move: `main`'s
cell is empty, `dup-all`'s private cell `P` holds `V0` (1). `(for-each @v
..)`: `@v` acquires (2) for the call. The closure is non-escaping
(`for-each`'s `f` is `:borrow`), captures `P` as an alias. Iteration 1:
`(append &v x)`: `v` is captured by a closure → not exclusive → acquire
into `append`'s private cell `Q` (3); `append`'s `push!` sees count 3 →
copies: `Q` := `V1` (1), `V0` (2); write-back into `P`: `P`'s old `V0`
released (1), `P` := `V1`. Iteration 2: `Q` := `V1` (2), copy → `V2`,
`V1` (1); write-back releases `V1` (0, freed), `P` := `V2`. Iteration 3
likewise frees `V2`; `P` := `V3`. `for-each` returns; its temporary `V0`
released (0, freed). `dup-all` returns; write-back stores `V3` into
`main`'s empty cell. `(count @v)` = 6; nothing live at exit.

Case 17: `(push-count &v @v)`: `v` occurs in another argument → acquire
(2), then `@v` (3); `push!` copies; write-back releases `V0` (2); the
temporary is released after the call (1); result 4; clean.

### 6.7 Cells, weak references, cycles (§6)

- `(cell e)`: E2 for `e`; the cell owns one count of its content.
- `@c`: an acquire, `Owned` (+1) — the same rule as for atoms
  (**Decided** for atoms, §7; Proposed for cells), so that `(let ((x @c))
  (set! c y) x)` can never read freed memory. Never elided in v1 (§6.3; §10 item 4).
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
  in the implementation (immutable objects can only refer to older
  objects; a unique write never closes a cycle, §6.6). Case 15: `k` (1
  from its `let`) → cell → `[k]` (E2 retains `k`: 2) → `k`; the `let`
  exit releases one (1); the SCC {Knot, cell, Vec} contains a cell →
  `leak-cycle` and nothing else.
- `(weak e)`: no count operation; `e`'s binding is forced onto the heap
  (§6.11: a stack object cannot be observed dead by a weak box); the
  first `weak` of an object allocates its box and sets `HAS-WEAK` with
  an atomic or, since the object may be shared (§8.7, §8.2); on an
  `IMMORTAL` object (a literal, a `def` value, a named function used as
  a value) it allocates a box that is never cleared and leaves the
  object's header alone, so `@w` on it is always `(some ..)`: the
  upgrade tests the flag before the count, which is 0 on an immortal
  (§8.7; §10 items 24, 30). `@w`: atomically "retain if still alive" (§8.7); result
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
returns. The task object is created with count **2** (Proposed; §10 item
22): one is the caller's `Owned` result, one is held by the running
thread and released as its last action, after the result is stored and
the state set to done (§8.8). So a task whose handle is discarded (a
non-final `do` step, syntax Open decision 15) or released before `join`
is freed by whichever holder releases last, never while the thread still
writes into it. `join` retains the result for the caller. A task may be
joined by several threads and awaited by several tasks (`Send (Task T)
= Send T`, §1.6): the counts they hold keep it allocated, and §8.8
makes the runtime, not a count, the guarantee that only one of them
ever resumes it (Proposed; §10 item 27; proposed cases 43, 44). `main`
returning waits for every spawned thread still running, so the audit
runs on a quiescent heap (Proposed; §10 item 22). Case 13 is rejected
during typing (§5.4) before this pass runs. Case 10: `a : (Atom (Vec i64))` is `Send`;
each `@a` acquires under the atom's lock (§8.6, the implementation
obligation of §7); `swap!` stores `f`'s `Owned` result (E2, moved) and
releases the old vector, which the reader's `snapshot` still holds until
its `let` exits. Case 16 is the same with a struct. Atomic counts: the E4
walk and every store into an already-shared object set the flag (§8.8).

### 6.9 Async (§8)

**Decided** (§8, §3.4, §3.5): an `async` form is an escaping closure whose
captures must be sendable (§2.8, §5.2). Mechanism: E3 for every capture
at task creation (this is "retained on entry"); the body runs in a
heap-allocated frame owned by the task; every local of the body lives in
that frame; so nothing in the body is `Borrowed` of an enclosing
binding, only of the task's own captures, which the task owns, and there
is no rule to check at `await` itself. Every capture is retained at
creation and released by the task's drop, whether or not its uses
precede the first `await`: evaluating the `async` form does not run the
body (syntax §3.14), so a use before the first `await` may still run
after the creating scope has released the captured binding; the earlier
idea of skipping such retains is withdrawn (Proposed; §10 item 23;
proposed case 24 in `spec/drafts/PROPOSED_CASES.md`). A `loop` body (syntax §3.18) inside
the `async` is part of the task's state machine, so a loop may `await`
on every iteration.

**Decided** (§8): `&` parameters are not allowed in async functions.
Check, before typing, on a `defun` with an `&` parameter `v`: if any
`async` form in its body (including inside nested `fn`s) mentions `v`
(as `@v`, `&v`, `&(. v f)` or the target of `set!`, anywhere inside the
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
`async`, rejected that safe shape for no reason (Proposed; syntax Open
decision 22). `await` outside `async`, or inside a `fn` nested in one,
is `await outside async`; inside a `loop` of the `async` it is fine.

Case 11: `measure` creates a task capturing `s` (retained); `main`'s
inner `let` releases `s` (1, held by the task); `block-on` drives the
task; `(length s)` reads the task's capture; the task is freed after
`block-on` returns its value, releasing `s`; clean.

### 6.10 Loops: `loop`/`recur` and self tail calls (Proposed)

A **self tail call** is a call `(f args)` to the enclosing `defun` `f`, or
`(g args)` to the enclosing named `fn g`, in tail position of its body
(the last step of the body, the branches of a tail `if`/`match`, the
body of a tail `let`/`do`, transitively). A `recur` is in tail position
of its `loop` body in the same sense (syntax §3.18). The interpreter and
the compiler treat both as one **slot loop**:

1. on entry, each slot becomes an owning binding: a `loop` variable
   `consume`s its initialiser; a plain object parameter of a
   self-tail-calling function is retained into its slot; an `&`
   parameter's slot holds its private cell, which is never counted;
2. at the `recur` or the tail call: evaluate the argument expressions
   into temporaries with `consume` (an `Owned` argument is moved, a
   borrowed one retained; a closure literal or `let`-bound closure
   among the arguments is escaping, §6.5, so it owns its captures and
   survives the scope exits that follow); run the scope exits of every enclosing scope
   inside the body (release their owning bindings); release the old
   slot values; store the temporaries into the slots; continue at the
   start of the body. For an `&` parameter the argument at its position
   must be `&v` for that same parameter: the slot is left untouched,
   with no copy-in and no write-back (§6.6). A self call whose argument
   at an `&` position is anything else (`&(. v f)`, a `let` cell, `&v`
   at a different position), or that is not in tail position, is an
   ordinary call: copy-in, call, write-back;
3. at a normal exit: for a function, `consume` the result (E1), then
   release the slots; for a `loop`, apply the scope-exit rule of §6.3
   with the slots as the loop's owning bindings (a `Borrowed(slot)`
   result is moved out, a `Derived(slot)` result is retained, then the
   slots are released).

Case 07: `(conj acc n)` is an `Owned` temporary moved into the `acc`
slot; the previous version is released at step 2 and freed when its
count reaches zero (only the new version shares its nodes); depth is
constant; the base case returns `Borrowed(acc)` → retain, release slots
→ the caller receives one count. This achieves §4 ("never freed by the
callee": nothing but the loop's own slots is freed) without a second
calling convention. `(defun fill (&v n) (if (= n 0) () (do (append &v
n) (fill &v (- n 1)))))`: `v` is exclusive at `(append &v n)` (its only
occurrences are `&v`), so every append moves and updates in place; the
tail call forwards the private cell; the caller's single write-back
stores the final vector (proposed case 25 in
`spec/drafts/PROPOSED_CASES.md`). Tail calls to *other* functions are
ordinary calls in v1 (§10 item 6); the condition under which a general
tail call could release before jumping is: every owned local of the
caller is either moved into the call or not passed to it at all.

### 6.11 Allocation: heap in v1, stack when scope-local (Proposed)

v1 allocates every object on the heap with a count header (§10 item 8):
the interpreter/compiler agreement on frees is then checked first, and
stack allocation becomes a pure optimisation verified by the same audit.
When it is enabled, a `let` binding or temporary `b` is **scope-local**
iff its initialiser is `Owned` at creation and no occurrence of
`Borrowed(b)` is at a `consume` position, passed to an `escapes`
parameter or a closure value, captured by any escaping closure, moved
out or retained by a scope exit or a join, or the operand of `weak` or
`raw-retained`. A
scope-local object is allocated in the frame with the `STACK` flag
(§8.2), receives no count operation, and its `drop` runs inline at scope
exit (its counted children are released; nothing is freed). Its
sub-objects are always heap objects, because they were stored at E2. A
`(some e)` of a non-`Option` object allocates nothing (§8.1): an
`Option` value is never itself a stack candidate, and its payload was
consumed at E2, so it is a heap object.
Both choices give the same frees (**Decided**, §2: "scope-local objects
are not counted").

### 6.12 Same results, same frees: what the interpreter does

The reference interpreter implements the plain counting semantics of §2:
every binding holds a count (it retains on every `let`, every capture,
every parameter of a loop slot), every temporary is released at its
step's end, and it optimises nothing — with three deliberate exceptions
that are part of the *semantics* and are implemented by both:

1. the slot loop of `loop`/`recur` and self tail calls (§6.10): a
   rebinding releases the old binding, so 100000 accumulator versions
   are not held until the recursion unwinds, and a forwarded `&`
   parameter keeps its private cell;
2. the copy-in decision (§6.6): move when exclusive, acquire otherwise,
   so in-place updates happen in the same places;
3. the unique-write test of `array-set!`/`set-field!` (§6.6).

The compiler elides: alias and derived bindings (no count), plain
parameters (no count), non-escaping closure captures (no count) and
scope-local objects (no count, later). It does not elide cell reads
(§6.3) or the captures of an `async` (§6.9). Each elision removes a
retain/release pair around an interval during which another count on
the same object is provably held by a binding that nothing can write
during the interval; the object's count at every remaining
release is the same, so it reaches zero at the same program point: the
multiset of objects freed at each scope exit and step end is identical
between the two. The audit compares exactly that (method.md rule 6), and
`fibc --explain` (§9) prints the compiler's side of the comparison.

### 6.13 Unsafe (§9)

`unsafe` changes nothing above. Externs take scalars, `ptr` and `unit`
only (syntax §3.15), so no fibber object is ever an extern argument.
`(raw e)` is the address of `e`'s object with no count operation, valid
while `e`'s binding is: the borrow of §9. `(raw-retained e)` is a
consume position (E2): `e` is retained (an `Owned` temporary is moved)
and the count belongs to the foreign side until `(release-raw p)` or
the runtime's exported `fib_release` releases it; this is §9's "unless
the foreign interface says the reference is retained" (Proposed; §10
item 26). `ptr` is a scalar: no count, not `Send`. The programmer's
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
| & argument must be a cell variable or a field of a cell variable | §2.14 |
| **& parameter v used as a value** in f | §2.14, syntax §3.13 |
| parameter v of g is &; pass &x | §2.2 |
| function with & parameters is not a value | §2.1 |
| cannot infer the struct type of e for field f; annotate it | §3.4 |
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
| 07 recursive-accumulator | §4, §5 | self tail call → loop (§6.10): `(conj acc n)` is `Owned`, moved into the slot; the old version released each iteration; base case `Borrowed(acc)` → retain, slots released | accept, 100000, clean |
| 08 mutate-while-iterating | §5 | `@v` acquires for the `for-each` call; the closure is non-escaping (`for-each`'s `f` is `:borrow`) so capturing `&v` is legal; `v` is captured, so each `append` acquires and `push!` sees a count above one and copies (§6.6, traced) | accept, 6, clean |
| 09 iterator-outlives-source | §3.1 | `iter` stores `v` into the iterator struct (E2, retain); `evens` returns `Owned`; `v`'s `let` releases; the iterator keeps the vector | accept, 2, clean |
| 10 atom-old-value | §7 | `plet` → `spawn`; `Send (Atom (Vec i64))` holds; the `pmap` closure captures `a` (`Send`) → colour `send`; `@a` acquires under the lock; `swap!` releases the old vector after the store; `snapshot` keeps it | accept, 1000, clean |
| 11 borrow-across-await | §8 | `async` is E3 for `s` (retained at creation) and requires `Send str` ✓; the task's frame owns it; `s`'s `let` releases | accept, 5, clean |
| 12 reject-same-binding-twice-inout | §5 | syntactic distinct-places check on `(bar &x &x)` (§6.5) | reject: `variable x passed to more than one & parameter in call to bar` |
| 13 reject-cell-crosses-thread | §7 | the closure's colour is `local` (capture `n : (Cell i64)`); it flows to `pmap`'s `(fn :send (a) b)`; `local ⊑ send` fails (§5.4); witness `n` | reject: `cell cannot be shared between threads: closure capture n has type (Cell i64)` |
| 14 reject-inout-in-async | §8 | `fill` has `&buf`, and its `async` mentions `&buf` and is the body's value, so `fill` is an async function (§6.9; syntax §3.1), checked first | reject: `& parameter in async function: buf in fill` |
| 15 cycle-through-cell-leaks | §6 | `[k]` retains `k` (E2); `set!` stores the vector into `k`'s cell; the `let` releases one count; the audit finds the SCC through the cell (§6.7) | accept, 1, leak-cycle |
| 16 coordinated-update-single-atom | §7 | `Send Accounts` holds (scalar fields); one `swap!` replaces the whole struct: `f`'s `Owned` result stored, the old released; `snap`/`final` acquire and release | accept, 200, clean |
| 17 inout-and-borrow-same-call | §5 | `v` occurs in another argument → acquire copy-in; `@v` acquires again; `push!` copies; write-back releases the old vector; the temporary released after the call (§6.6) | accept, 4, clean |
| 18 reject-inout-captured-by-escaping-closure | §5 | the `fn` is at E1 → escaping; its capture set contains `&` parameter `v` (§6.5) | reject: `& parameter captured by escaping closure: v in make-pusher` |
| 19 weak-parent-pointer | §6 | `(weak parent)` is not a count operation; `conj` and `set!` on the children cell are E2; the strong graph is a tree; `let`s release in reverse (§6.7) | accept, 2, clean |
| 20 weak-ref-to-dead-object | §6 | the inner `let` releases `v` (freed; box cleared); `@w` finds the box dead → `nil` (§6.7) | accept, 1, clean |

Cases 12, 14 and 18 are decided by syntactic checks, case 13 by a type
constraint; none needs the mode analysis. Every other case is decided
by the tables of §6.2–§6.3 plus summaries, and its audit verdict follows
from applying those tables, which the interpreter confirms by running
the plain counting semantics.

Programs that the confirmed findings on this document turned into
candidate cases (an `&` parameter used as a value, a whole-object
pattern variable, a cell read beside a sibling write, a pre-`await`
use of a capture, an `&` argument through a self tail call, a discarded
`Task`, `(some nil)`, `weak` of a literal, an in-place update inside
`dotimes`, a taken field reached through `weak`, a user `count`, an
`await` inside a loop, and the others), and those of the second round
(a push into a `Form` literal that must copy, an `&` function that
blocks on a task, `derive` on a generic struct, a declared `:borrow`
that escapes, a spliced top-level macro, a `def` table read from two
threads, one task joined by two threads and awaited by two tasks, a
bounded generic instantiated at two `ptr`-class types, a `weak` taken
on a shared object, an `:as` pattern in `let`), and those of the third
(`derive` on an enum, a closure passed at a self tail call, a `def`
naming a function, a macro that emits `nil`), are listed with their
expected headers in `spec/drafts/PROPOSED_CASES.md`; they become cases
only when added to `cases/` with the owner's sign-off.

---

## 8. Mapping to lIR

lIR is the S-expression assembler for LLVM IR (liar's `doc/lIR.md`),
used as it is: opaque `ptr`, `defstruct` with positional field types,
`define`/`declare`, `call`/`tailcall`, `indirect-call` in its present
form `(indirect-call fnptr R args..)`, `getelementptr`, `load`/`store`,
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
`ptr` global) and §8.11 lists what hardening may add (§10 items 25,
33). Everything
fibber-specific is a naming and layout convention on top; no fibber
vocabulary enters lIR, and none of liar ADR 021's safe-lIR features
(`own`, `rc`, `closure`) is used. Runtime support functions are ordinary lIR `define`s in a
`fib.rt` module (or C, linked). All of §8 is Proposed except where a
Decided rule forces a layout.

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
| `(Option T)`, `T` a scalar, a `dyn`, or itself an `(Option ..)` | `ptr` to a heap enum object: tag and payload (§8.3; v1, §10 item 14) | ptr |
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
`alloca` and `fib.immortalise` all initialise it so (Proposed; §10 item
30). `type-id` indexes the table `fib.types` of per-type records
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
does not track it (§6.7) (Proposed; §10 item 33).

**The flags word is accessed atomically** (Proposed; §10 item 29).
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
fib.takeable? (ptr p) -> i1                      ; unique? and not HAS-WEAK: a field of p may be taken (§6.6)
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
header is stored by the code that allocates it. Immortal objects are never `drop`ped or freed, and the audit does
not count them among the allocations live at exit (§6.7). The
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
each (§8.2); constructors likewise. An escaping closure is
`fib.alloc`ed with each object capture consumed (E3); its `drop`
releases the captures. A non-escaping closure is a `STACK` object of
the creating frame: `(alloca i64 (i32 k))` addressed through
`(getelementptr %struct.fib.closure.L ..)` (§8.2), storing uncounted
pointers. Colours have no representation. A named `fn`'s
self-reference is the closure's own `env`.

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

- `@c` on a cell: `load`, then `fib.retain` if the content is an object
  (never elided, §6.3).
- `(set! c v)`: `old = load`; `store v` (after `consume`); `fib.release
  old`. If the cell is `SHARED` (only an atom can be), the value is
  share-marked before the store.
- A private `&` cell: a `STACK` object in the caller's frame, `(alloca
  i64 (i32 3))` — the header's two words and one for the content, whose
  lIR type is at most one word — addressed through `(getelementptr
  %struct.fib.cell t (i32 0) (i32 3))` (§8.2), since lIR's `alloca`
  takes no struct type; its header is stored (count 0, `STACK`) and it
  is initialised by the copy-in (move: `store` the place's content and
  null the place, or take the field when `fib.takeable?` holds;
  acquire: `store` the retained content); passed as `ptr`; write-back
  as §6.6; the `alloca` needs no drop. Case 17's `(push-count &v @v)`
  emits exactly this: three words, the header, the acquired vector
  stored through the field-3 `getelementptr`, `push-count` called with
  the `alloca`'s address, the content stored back into `v` afterwards.
- `@a` on an atom: `fib.lock a` (`cmpxchg` spinlock, acquire), `v = load`,
  `fib.retain v`, `fib.unlock a` (release store) — the single atomic step
  §7 requires.
- `(swap! a f)`: loop { `old = @a` (retained); `new = f(old)`; lock; if
  `load == old` { `fib.share new` if the atom is `SHARED`; `store new`;
  `fib.retain new` (the caller's result); unlock; `fib.release old` (the
  atom's count); `fib.release old` (the snapshot); return `new` } else {
  unlock; `fib.release old`; `fib.release new`; retry } }.
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
own count reaches zero (§10 item 24). `@w`: lock the box; `t = load
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
into the task object and returns.

**One driver at a time, any number of waiters** (Proposed; §10 item
27). `Send (Task T) = Send T` (§1.6), so one task can be joined from
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

### 8.9 Calls, returns, self tail loops

- A `defun` is `(define (R) ((A₁ p₁) .. (Aₙ pₙ)) ..)`; `&` parameters
  are `ptr`; `unit` results are `void`.
- The caller releases its owned temporaries after the call returns and
  after the write-backs (§6.3); the callee retains at its own escape
  positions. No ownership passes through a calling convention except
  `spawn`'s argument (consumed).
- A self tail call, and a `loop` (§6.10): parameters or loop variables
  become `alloca` slots; on entry each plain object parameter is
  `fib.retain`ed into its slot (a `loop` variable's initialiser is
  `consume`d into it; an `&` parameter's slot holds the private cell
  pointer and is never retained or released); the body starts at
  `(block loop)`; the tail call or `recur` evaluates its arguments into
  temporaries with `consume`, runs the pending scope-exit releases,
  `fib.release`s each old slot value, stores the temporaries, and `(br
  loop)`; a forwarded `&v` argument emits nothing for its slot. The exit
  path `consume`s the result, releases the slots and `ret`s (or falls
  through, for a `loop`). No `musttail` is needed.
- Calls to other functions in tail position are ordinary `call` +
  releases + `ret` in v1.

### 8.10 What the compiler must emit, summarised

| Event | Emission |
|---|---|
| object literal, constructor, `cell`, `atom`, closure creation | `fib.alloc`, header init, `consume` each stored field/capture (E2/E3) |
| E1 return of `Borrowed`/`Derived` | `fib.retain` before `ret` |
| E2 store of `Borrowed`/`Derived` | `fib.retain` before the store |
| E4 `spawn` | `consume` the closure, `fib.share`, allocate the task with count 2, hand both to the runtime |
| join retain (`if`/`match`) | `fib.retain` at the tail of each non-`Owned` branch |
| scope exit | `fib.release` each owning binding not moved out, in reverse order, on every exit path |
| `Derived(x)` result leaving `x`'s scope | `fib.retain` the result, then the releases |
| `Owned` temporary as a plain argument | `fib.release` after the call returns and its write-backs |
| non-final `do` step with an `Owned` value | `fib.release` at the step's end |
| `@c`, `@a`, `@w` | as §8.6/§8.7 (always a `fib.retain`; atom under lock; weak "retain if alive") |
| `set!`, `reset!`, `swap!` | as §8.6: `consume` new, share if `SHARED`, store, `fib.release` old |
| `&` copy-in | move (store, clear the place / take the field when `fib.takeable?`) or acquire (`fib.retain`) as §6.6; nothing for a forwarded `&v` in a self tail call |
| `&` write-back | store the private cell's content into the place; `fib.release` the place's old content (acquire case); `set-field!` semantics for a field place |
| `array-set!`, `set-field!` | `fib.unique?` test (flags first, then the count, §8.2); in-place write, or copy + store + `fib.release` old |
| module initialiser `fib.init.<module>` | called by the entry point before the program's `main`, modules in dependency order (§8.2): allocate and fill the type table, build every static object (literals, named-function closures, vtables) with `fib.alloc` and an `IMMORTAL` header, store each address into its `ptr` global |
| `def` initialisation | in the module initialiser after its static objects, `def`s in source order: evaluate the constant expression and `fib.immortalise` its value (syntax §3.19); as static data once lIR holds struct-typed constants (§8.11) |
| stack object (`STACK`: private `&` cell, non-escaping closure, later scope-local objects) | `(alloca i64 (i32 k))`, `k` the layout in 8-byte words, header stored, fields through `getelementptr` on the struct type (§8.2, §8.4, §8.6) |
| `(weak x)` | box lookup or allocation under the table mutex; `HAS-WEAK` set with `atomicrmw or` (§8.7) |
| self tail call, `loop`/`recur` | §8.9 |
| `raw-retained` | `consume` the operand; the count is the foreign side's until `release-raw`/`fib_release` |
| `drop` per type | `fib.release` each object field; `fib.weak-clear` if flagged; `free` |
| stack object scope end (later) | inline `drop` body, no `free` |

### 8.11 lIR hardening this mapping relies on

From `lir-audit/`: a whole-module type checker run by default (method.md
rule 7); `indirect-call` carrying the full function type (today every
argument is typed `ptr`, so §8.4's calls pass scalars through
`ptr`-sized words until then); string globals that load correctly; a
`fence` with real cross-thread semantics; `atomicrmw`/`cmpxchg`/
`atomic-load`/`atomic-store` documented in `doc/lIR.md` (they are parsed
and lowered but undocumented); `tailcall` as `musttail` only if general
tail calls are adopted later (§10 item 6). Conveniences this mapping
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
eye and diffed against the interpreter's trace (Proposed):

```
defun dup-all : (fn ((& (Vec i64))) unit)
  params:    v  &param  type=(Cell (Vec i64))  escapes=no  exclusive=no (captured by closure @8:15)
  bindings:  (none)
  closure @8:15  escaping=no  reason=arg-to-borrow(for-each.f)  captures: v (alias)
  calls:     @8:3  for-each(@v, <closure>)   @v: acquire
             @8:24 append(&v, x)             &v: acquire (v not exclusive)
  ops:       L8 retain [@v]; L8 call for-each; L8 release [@v]
```

Every line is one of: a parameter (type, summary, exclusivity with the
reason), a binding (`owns` / `alias-of b` / `derived-of b` /
`scope-local`), a closure literal (`escaping` with the clause of §6.5 that
decided it, `arg-of-self-tail-call` or `arg-of-recur` for the slot-loop
rule, its captures with their kinds), a call with the copy-in
decision of every `&` argument (`move`, `acquire`, or `forward` for the
`&v` of a self tail call, §6.10), a colour solution (`ς₁ = local, forced by
capture n`), and the emitted operations with source lines. Reject cases
print the error and the rule number of this document. The interpreter's
trace (`fibref` `heap/event.rs`) lists every retain, release and free
with the same line numbers, so the compiler's `ops` and the interpreter's
trace must free the same objects at the same lines (method.md rule 6).

---

## 10. Open decisions (types and checker)

Each needs the owner's sign-off. Recommendation first, alternative
second. Items 1–6 shape the checker; 7–11 the runtime; 12–17 are
smaller; 18–26 came out of the confirmed findings on the synthesis;
27–31 out of the second round of findings; 32–33 out of the third.

1. **Inference is HM with generalisation only at `defun` SCC boundaries;
   `let` and `fn` never generalise (§3).** Recommend: yes; liar's
   `lib/` ports without annotations, the ownership pass sees one
   concrete type per local, and cells need no value restriction.
   Alternative: generalise `let`-bound syntactic values.
2. **Closure colours are a two-point lattice with `⊑` constraints at
   flow sites, quantified in schemes (§5.4).** Recommend: yes; it is what
   lets `(pmap f xs)` take a parameter `f` and lets `comp`/`partial`
   compose across a boundary. Alternative: colours unified by equality
   (rejects composing a `send` with a `local` closure) or a syntactic
   rule (only literals and names at boundaries).
3. **Copy-in moves when the place is exclusive, else acquires (§6.6);
   the interpreter implements the same rule.** Recommend: yes; it is the
   only route to in-place updates through user-defined `&` functions and
   the condition is syntactic plus one count test. Alternative: always
   acquire; every `&` call copies on its first update.
4. **Reads of cells are owned (+1) and never elided in v1 (§6.3,
   §6.7).** Recommend: yes; owned reads are the only rule under which
   cases 08 and 17 are sound without alias analysis, and both elisions
   proposed so far were shown unsound: the ownership draft's under cell
   aliasing, and the synthesis's "cell-free primitives" list under a
   sibling write in the same step (`(array-get (. @s arr) (do (set! s
   ..) 0))`) and under user implementations of `count`/`nth`/`+`.
   Alternative: an elision restricted to a closed list of
   scalar-producing compiler builtins that dispatch to no user code
   (`array-len`, `str-len`, the scalar instances of `Num`/`Eq`/`Ord`),
   applied only when no other sub-expression of the same step contains
   a call, `set!`, `set-field!`, `array-set!`, `swap!` or `reset!`; or
   borrowed reads with a flow-sensitive "no write while borrowed"
   check, a small borrow checker.
5. **Escape kinds are declared on protocol methods (default escaping),
   checked on every `impl`; closure types carry none; calls through
   function values assume every argument escapes (§6.4).** Recommend:
   yes; the only modular option under dynamic dispatch and separate
   compilation, and the safe default. Alternative: kinds in closure
   types `(fn ((a :borrow)) r)`, which needs variance rules.
6. **Only self tail calls become loops; no owned-argument calling
   convention (§6.10).** Recommend: yes; case 07 gets constant stack
   without contradicting §4's "never freed by the callee" and without a
   second convention. Alternative: an inferred `own` convention for
   accumulator parameters (constant stack for mutual recursion too, at
   the price of a per-caller convention flip and a reading of §4 that
   the owner must bless). A self tail call forwards an `&` parameter's
   private cell when the argument at its position is `&v` itself and is
   an ordinary call for any other `&` argument; `loop`/`recur` (syntax
   §3.18) lower to the same slot loop, which is what lets a loop update
   an `&` place in place and `await` inside an `async`.
7. **Monomorphisation keyed by layout class for unconstrained type
   variables and by full type for protocol-bounded ones (item 28);
   whole-program compilation (§4.3).** Recommend: yes. Alternative:
   dictionary passing with a uniform boxed representation (separate
   compilation, boxed scalars, a runtime test in every count
   operation).
8. **Heap-allocate everything in v1; stack allocation is a later
   optimisation pass verified by the audit, under the scope-local
   condition of §6.11.** Recommend: yes; it is the lowest-risk order and
   neutralises every stack-allocation hole the drafts had until the
   audit can catch one. Alternative: stack allocation from day one.
9. **`async` captures must be `Send` (§2.8, §5.2).** This follows §3.4's
   listing of "a task" among thread crossings and is marked Decided. The
   owner should confirm the reading; the alternative (a single-threaded
   executor, `Task` not `Send`, no check) needs §3.4 amended first.
10. **Atoms use a per-atom spinlock; `swap!` runs `f` outside the lock
    and retries (§8.6).** Recommend: yes; it meets the §7 obligation
    directly. Alternative: hazard pointers or epoch reclamation.
11. **`(dyn P)` is not `Send` and has no `&self` methods in v1; `Send
    ptr` is false (§5.1).** Recommend: yes. Alternative: a `Send` bit in
    the vtable and `(dyn P :send)`.
12. **No coercions anywhere: no `T ↝ (Option T)`, no `T ↝ (dyn P)`
    (§1.7, §3).** Recommend: yes; the inference draft's coercions were
    shown to make `(f (some 1) 1)` and `(f 1 (some 1))` get different
    verdicts. Alternative: coercions attempted after all equalities, in
    source order, with the loss of principal types that implies.
13. **Field access and `deref` on an unresolved variable must be fixed
    by the end of the SCC; no unique-field-name lookup (§3.4).**
    Recommend: yes (case 19 annotates `depth`). Alternative: resolve
    `(. x f)` to the unique struct in scope declaring `f`.
14. **`(Option T)` is a nullable pointer only when `T` is a non-`Option`
    object type; of a scalar, a `dyn` or another `Option` it is a heap
    enum, and nullable `Option`s form their own layout class `opt`
    (§4.3, §8.1).** Recommend: accept for v1; `(some nil)` and `nil`
    must stay distinct (§1.5), and the class keeps generic code from
    collapsing them. Alternative: an unboxed `{i1, T}` by-value
    representation for every `Option`, a third value class in every
    rule of §6.
15. **Instances are keyed by `(P, head)`; no overlap, no instance for a
    bare variable, no default methods, no supertraits in v1 (§4.1).**
    Recommend: yes. Alternative: liar's `extend-protocol-default` as
    blanket instances with a specificity order.
16. **The audited heap gains `write-unique` (§6.6): a write to an
    immutable object legal iff it is neither `SHARED`, `IMMORTAL` nor
    `STACK` and its count is exactly 1 (item 30), used only by
    `array-set!`/`set-field!`.** Recommend: yes; it is what "unique
    updates happen in place" means as an audited event.
    Alternative: no unique writes; `&` updates always copy (then §2's
    third guaranteed minimum is vacuous).
17. **Leak-cycle classification is fibref's (§6.7): reachable from a
    cell or atom on a cycle, with count equal to live references.**
    Recommend: yes; it is exactly "unreachable except through a cycle
    of cells" and is already implemented and adversarially tested.
    Alternative: report every leak as a failure and require a cycle
    collector first, which contradicts §6.
18. **A pattern variable that binds the whole scrutinee takes the
    scrutinee's mode; `Derived(b)` is reserved for strict sub-objects;
    a join of `Borrowed(b)` with `Derived(b)` is `Owned` with a retain
    on the `Borrowed` branch (§6.1, §6.3, §6.4).** Recommend: yes; the
    escape summary's exemption for `Derived(p)` is sound only if a
    derived value is never `p` itself, and `(defun same (p) (match p (w
    w)))` was `noescape` under the old reading, which let a closure
    passed to it be treated as non-escaping and freed before it was
    called. Alternative: drop the exemption and count `Derived(p)` at a
    consume position as an escape (simpler; forces the argument of
    `first` onto the heap once stack allocation exists).
19. **An `&` parameter is not a value (§2.14; syntax §3.13).**
    Recommend: yes; the private cell is a stack object of the caller's
    frame, so `(defun leak (&v) v)` returned a pointer to memory freed
    at the write-back, and `(cell v)`, `(weak v)` and a non-escaping
    closure returning `v` did the same. Alternative: count private
    cells and heap-allocate them, at a cost on every `&` call.
20. **Instance contexts are declared with `:where` on `impl` and
    consulted when defuns are generalised (§2.7, §3.5).** Recommend:
    yes; a `defun` calling a method is generalised (step 5) before any
    impl body is typed (step 6), so an inferred context would not exist
    when it is needed, and `(defun f (b) (show b))` would lose the
    `(Show a)` bound of `(impl Show (Box a))`. Alternative: put impl
    bodies into the call-graph SCCs with an edge from every method call
    to every impl of that method, keeping a constraint whose instance
    is in the current SCC as a flexible bound of the scheme (larger
    SCCs, more monomorphic recursion, contexts still exported).
21. **A field is moved out of a struct only if the struct is
    `fib.takeable?`: count 1, not shared and `HAS-WEAK` clear (§6.6,
    §8.2); the interpreter reports a read of a taken field as an audit
    failure.** Recommend: yes; `fib.unique?` counts strong references
    only, and a weak reference upgraded during the call reached the
    taken (null) field. Alternative: never move a field out; `&(. x f)`
    always acquires, and the in-place path for a persistent vector's
    tail relies on copy-then-unique-write instead.
22. **`spawn` creates the task with count 2, one for the caller and one
    for the thread, released after the result is stored; a registered
    waker holds a count on the waiting task (the wakers form a list,
    item 27); `main` returning joins every running thread (§6.8,
    §8.8).** Recommend: yes; a discarded handle otherwise freed the
    task under the running thread.
    Alternative: detach threads at exit and exclude their objects from
    the audit; or forbid discarding a `Task` (a type error), which
    rejects fire-and-forget `spawn`, an ordinary program in liar's
    library.
23. **`async` retains every capture at creation; no pre-`await` elision
    (§6.9).** Recommend: yes; the body does not run at creation, so a
    use before the first `await` can run after the creating scope has
    released the binding. Alternative: the elision only for a task
    joined in the same step it is created, which no case needs.
24. **`(weak x)` on an `IMMORTAL` object allocates an unregistered,
    never-cleared box and leaves the object's header alone (§6.7,
    §8.7).** Recommend: yes; literals may live in read-only data and
    `fib.drop` never runs on them. Alternative: a compile error for a
    literal operand, which cannot catch a named function passed through
    a variable.
25. **`match` lowers to `icmp`/`br` chains; indirect calls use lIR's
    untyped form; no array type, struct-typed `alloca`/`global` or
    struct-valued constant is required (§8, §8.11; item 33).**
    Recommend: yes; it is what lIR has today, and it
    keeps method.md rule 7 (lIR verifies its input) independent of new
    instructions. Alternative: add `switch`, the typed `indirect-call`,
    an array type, struct-typed `alloca`/`global` and struct-valued
    constants to lIR first.
26. **`(raw-retained e)`/`(release-raw p)` are the only count transfer
    across the foreign boundary (§6.13; syntax §3.15).** Recommend:
    yes; extern positions are scalars, so `:retains` named parameters
    that cannot exist. Alternative: object types allowed at `:retains`
    positions of an extern signature, with the foreign side calling
    `fib_release`.
27. **A task is resumed by one driver at a time (a `cmpxchg` on its
    `driver` word), waited on by any number of joiners and awaiters
    (a waiter list, a broadcast completion signal), its `state` and
    `result` written once with release ordering, and its `trace`
    excludes live locals; `Task` stays `Send` (§8.8, §6.8).**
    Recommend: yes; nothing in the checker prevented `(plet ((a (join
    t)) (b (join t))) ..)` or two tasks awaiting one task, and a count
    is not mutual exclusion: two drivers would rewrite one frame, a
    `fib.share` walk could read a frame mid-resume, and one waker slot
    lost the second awaiter. Alternative (a): `Task` not `Send`, which
    rejects awaiting any task created outside the `async` body, i.e.
    task composition itself; (b) linear handles, joined or awaited
    once, which needs a linearity check the checker has no other use
    for, or a runtime trap that no case header can express.
28. **Specialisations key a protocol-bounded type variable by its full
    type argument and every other variable by layout class (§4.3).**
    Recommend: yes; with a class-only key `(show x)` at `x : a`, `(Show
    a)`, inside `describe<ptr>` names no implementation, so §4.2's
    direct call and layout-class sharing could not both hold; the
    interpreter's dispatch on the header's type id (§4.5) answers the
    same. Alternative: per-protocol dispatch tables indexed by the
    header's type id inside `ptr`-class specialisations, which cannot
    serve scalars (no header) and drops "always static".
29. **The header's flags word is read with `atomic-load monotonic` and
    written after allocation with `atomicrmw or` (§8.2, §8.7, §8.8).**
    Recommend: yes; `(weak x)` sets `HAS-WEAK` on an object that may
    be shared while another thread's `fib.retain` reads the word, and
    under the LLVM model a racing non-atomic load is `undef`, which
    could send the retain down the non-atomic count path; monotonic
    atomics cost a plain load and a locked `or`. Alternative:
    `HAS-WEAK` in the count word's high bits under the count's own
    atomics, which puts a mask into every count test.
30. **`fib.unique?` and `fib.takeable?` test the flags before the count
    and are false on `SHARED`, `IMMORTAL` and `STACK` objects; immortal
    and stack objects carry count 0 (§8.2, §6.6, §6.2).** Recommend:
    yes; without the flag test an immortal count that read 1 (a
    constant emitted like `fib.alloc` initialises it) let `push!` write
    into a `Form` literal pulled apart by `match`, mutating a constant
    or faulting on read-only data while the interpreter answered
    otherwise. Alternative: a maximum count value for immortals, which
    still needs the flag test in `unique?` and leaves a weak upgrade's
    `> 0` test accepting, so a missing flag test there would go
    unnoticed.
31. **`def` values are typed closed and monomorphic, in dependency
    order with the `defun`s (syntax Open decision 29), and immortalised
    before `main` (§2.16, §3.5, §8.2; syntax §3.19, syntax Open
    decision 25).** Recommend: yes; the constant grammar keeps
    initialisation effect-free and `Send` by construction, and the
    immortal header makes a global count-free and safe from every
    thread. Alternative: evaluate every `def` at compile time only (the
    interpreter as constant folder), so no runtime initialisation
    exists; equivalent for the programmer, heavier for the compiler.
32. **A closure literal, or a `let`-bound closure of §6.5 clause (c),
    that is an argument of a self tail call or of a `loop`/`recur` is
    escaping whatever the callee's summary says (§6.5, §6.10).**
    Recommend: yes; `spin`'s `g` is `noescape` because it is only
    called, but the tail call is a slot loop: the jump releases the
    creating scope's bindings and the closure survives in the slot, so
    a non-escaping closure's uncounted capture pointed at freed memory
    on the next iteration while the interpreter, which retains every
    capture, answered 1 (proposed case 49). Alternative: keep a slot-loop
    iteration's bindings alive across the jump (a frame per iteration,
    which forfeits constant stack), or forbid closures as
    self-tail-call arguments.
33. **v1 builds every static object and table (the type table, string
    and `Form` literals, named-function and constructor closures,
    vtables, `def` values) in a per-module initialiser run before the
    program's `main`, reached through `ptr` globals, and emits stack objects as
    word-sized `alloca`s addressed through `getelementptr` on the
    struct type (§8.2–§8.6, §8.10).** Recommend: yes; `lair` rejects a
    struct-typed `alloca` or `global` (`UnknownType`) and a function
    address as a global initialiser, so the shapes §8 gave for the
    private `&` cell, stack closures, vtables and the type table could
    not be emitted; the initialiser costs one allocation per static
    object at start-up and nothing after. Alternative: add struct-typed
    `alloca`, `global` and constants to lIR first (item 25's
    alternative), which turns the same objects into static data with
    no other change.
