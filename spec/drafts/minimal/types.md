# fibber types and the ownership checker — minimal-core draft

Status: draft, angle "minimal core". Authority: [ownership.md](../../ownership.md)
(decided). Marks: **Decided** only where the point follows directly from
ownership.md; **Proposed** otherwise. Companion: [syntax.md](syntax.md).

Contents: §1 type grammar · §2 typing rules per core form · §3 inference
algorithm · §4 protocol dispatch · §5 the ownership checker, rule by rule ·
§6 case table · §7 mapping to lIR · Open decisions.

---

## 1. Type grammar

```
type ::= scalar | object | (cell type) | (atom type) | (weak type) | (task type)
       | (fn (type*) type)                ; function or closure
       | (any Proto)                      ; dynamic protocol value
       | tvar                             ; type parameter in scope
       | Self                             ; inside defprotocol / impl

scalar ::= bool | i8 | i16 | i32 | i64 | f32 | f64 | char | keyword | unit
         | Enum                           ; a defenum whose variants have no fields

object ::= str | Form
         | (vec type) | (map type type) | (set type)      ; library nominal types
         | Struct | (Struct type+)                        ; nominal, defstruct
         | Enum   | (Enum type+)                          ; nominal, defenum with fields
         | (option type)                                  ; = prelude defenum

param-mode ::= (& type)                   ; only in parameter position of a signature
```

**Scalars** are copied and have no identity (Decided, §1). **Objects**
are counted heap values (or stack values when proven non-escaping,
§5.1). `cell`, `atom`, `weak`, `task` and closures are objects.

Kinds of type and their ownership behaviour:

| Type | Object? | Mutable? | May cross a thread (`Send`) | Notes |
|---|---|---|---|---|
| scalar | no | no | yes | `unit` has one value `()` |
| `str`, `(vec T)`, `(map K V)`, `(set T)`, `Form` | yes | no | if elements are | persistent; structural sharing |
| struct, enum with fields | yes | no | if fields are | nominal |
| `(cell T)` | yes | **yes** | **no** | the only mutable container (§6) |
| `(atom T)` | yes | yes, atomically | yes, if `T` is | requires `T: Send` at construction |
| `(weak T)` | yes | no | if `T` is | `T` must be an object type |
| `(task T)` | yes | internal | if `T` is | created by `async` or `spawn` |
| `(fn (A..) R)` | yes | no | if every capture is | see 1.2 |
| `(any P)` | yes | no | no (v1) | pointer + vtable |

### 1.1 Nominal types and generics

`defstruct`/`defenum` names are nominal: two structurally identical structs
are different types. Type parameters are explicit (`[T]`, `[K V]`), appear
positionally in uses (`(Pair i64 str)`), and are quantified only at
`defun`, `defstruct`, `defenum`, `defprotocol`, `impl`. There is no
higher-rank polymorphism, no let-polymorphism inside bodies, and no
inference of type parameters for definitions: a generic definition is
written generic. Recursive nominal types are allowed; recursive *structural*
types are not (occurs check, §3.2) — this is why case 15 ties its knot
through a struct.

Bounds: `[(T Send)]`, `[(T Show)]`, `[(I (Iter T)) T]`. `Send` is a
built-in predicate (§5.7), not a protocol.

### 1.2 Function and closure types

`(fn (A1 .. An) R)` is the type of named functions used as values and of
closures alike. Two attributes of a function type are not written in
source but are part of the type and are solved by inference:

- **send attribute** `κ ∈ {send, local}`: `send` iff every captured
  variable has a `Send` type. Named functions are `send`. It is the
  "closure colour becomes a check on what a closure captures" of §7.
  Printed in interfaces as `(fn :send (A) R)` / `(fn :local (A) R)`.
- **parameter escape kinds** are *not* part of closure types (Open
  decision 1). A call through a closure value therefore treats every
  argument as escaping (§5.2), which is always safe.

`(& T)` in a signature marks an in-out parameter; a call site supplies
`&x` with `x : (cell T)`. `&` never appears in closure types other than as
the mode of the closure's own parameters.

### 1.3 `option`, `nil`

`(option T)` is `(defenum option [T] (nil) (some v: T))` from the prelude.
`nil : (option T)` for any `T`; `(some e) : (option T)` when `e : T`.
There is no implicit conversion from `T` to `(option T)` and no `nil?`
in the core: `match` eliminates it (§2.6). For an object `T`, the
representation is a nullable pointer (§7.1), so the abstraction costs
nothing.

### 1.4 Protocol types

A protocol `P` is not a type. `(any P)` is: the type of a value of some
unknown type implementing `P`, carrying its dispatch table (§4.2). A
value of concrete type `T` implementing `P` becomes `(any P)` only where
the expected type is known to be `(any P)` (§3.4); this is an
elaboration inserted by the checker, not subtyping.

### 1.5 Builtin signatures (normative)

```
cell    [T] (v: T) -> (cell T)                          ; v escapes (stored)
set!    [T] (c: (cell T) v: T) -> unit                  ; v escapes; old value released
atom    [(T Send)] (v: T) -> (atom T)                   ; v escapes
swap!   [T] (a: (atom T) (f: (fn (T) T) :borrow)) -> T  ; result owned (+1)
reset!  [T] (a: (atom T) v: T) -> unit
weak    [T] (x: T) -> (weak T)                          ; T an object type; forces x onto the heap, no retain
deref   protocol (Deref T): (cell T) -> T | (atom T) -> T | (weak T) -> (option T)   ; result owned (+1)
spawn   [(T Send)] ((f: (fn :send () T) :escapes)) -> (task T)
join    [T] (t: (task T)) -> T                          ; result owned
block-on[T] (t: (task T)) -> T
yield   () -> (task unit)
panic   [T] (msg: str) -> T                             ; diverges
vec-empty [T] () -> (vec T)
conj    [T] (v: (vec T) (x: T :escapes)) -> (vec T)     ; new version
nth     [T] (v: (vec T) i: i64) -> T                    ; borrow of element; traps out of range
count   protocol (Countable): (self) -> i64
push!   [T] (&v: (vec T) (x: T :escapes)) -> unit       ; in place if unique
vec-set! [T] (&v: (vec T) i: i64 (x: T :escapes)) -> unit
pop!    [T] (&v: (vec T)) -> T
map-empty [K V] () -> (map K V)
assoc   [K V] (m: (map K V) (k: K :escapes) (v: V :escapes)) -> (map K V)
get     [K V] (m: (map K V) k: K) -> (option V)
map-put! [K V] (&m: (map K V) (k: K :escapes) (v: V :escapes)) -> unit
map-del! [K V] (&m: (map K V) k: K) -> unit
```

`:escapes`/`:borrow` are escape kinds (§5.4). Everything not listed
(`range`, `pmap`, `for-each`, `iter`, `filter-iter`, `collect`,
`starts-with?`, ...) is fibber code in `lib/` and gets its signature by the
ordinary rules; `for-each`'s and `pmap`'s function parameters are declared
`:borrow`.

---

## 2. Typing rules for the core forms

Judgement: `Γ ⊢ e : τ` under a substitution `S` (§3). `Γ` maps variables
to types and modes (`plain`, `cell-param`), and holds the definitions in
scope. Rules are written as "premises ⟹ conclusion" and read as
constraint generation (§3.1).

### 2.1 Literals and variables

- Integer literal: fresh `α` with the constraint `α ∈ {i8,i16,i32,i64}`;
  defaults to `i64` if unconstrained at the end. Float literal: `α ∈
  {f32,f64}`, default `f64`. `"s" : str`, `\c : char`, `true/false :
  bool`, `:k : keyword`, `() : unit`, `nil : (option α)`.
- Variable `x` with `Γ(x) = τ` ⟹ `x : τ`. A `&` parameter `v` has
  `Γ(v) = (cell τ)` with mode `cell-param`.
- A named function `f` with signature `[T..] (A..) -> R` used as a value:
  instantiate `T..` fresh ⟹ `f : (fn :send (A..) R)`. Bounds on `T..`
  become obligations (§3.3).

### 2.2 Calls

```
f : (fn (A1..An) R)     ai : Ai  (plain params)
&x for a param (& Ai):  Γ(x) = (cell Ai)
⟹ (f a1 .. an) : R
```

Head position may be any expression of function type. For a protocol
method `m` of `P` with `self` of type `Self`: the receiver's type must be
resolvable to a type with an `impl` (or a bound) or to `(any P)` (§4).
Constructors are functions: `(Name e..)` for a struct, `(Variant e..)`
for an enum variant, with the type's parameters instantiated fresh.

### 2.3 `defun`, `fn`

```
(defun f [T..] (x1: A1 .. xn: An) -> R body)
   Γ, x1:A1 .. xn:An ⊢ body : R          ; T.. are rigid (skolem) in the body
(fn (x1: A1 .. xn: An) body)   in context Γ
   Γ, x1:A1.. ⊢ body : R      ⟹  (fn ..) : (fn :κ (A1..An) R)
   κ = send iff for every free variable y of the body captured from Γ, Send(Γ(y))
```

Unannotated parameters and results get fresh variables. A `&` parameter
`&v: A` enters `Γ` as `v : (cell A)`. `body` is a `do`.

### 2.4 `let`, `do`, `if`

```
(let ((x e)) body):  e : τ ;  Γ, x:τ ⊢ body : ρ  ⟹ ρ
(do e1 .. en):       ei : unit (i<n, Open decision 5 relaxes) ; en : τ ⟹ τ ;  (do) : unit
(if c a b):          c : bool ; a : τ ; b : τ  ⟹ τ
```

### 2.5 `.`

`e : (S θ..)` for a struct `S` with field `f : F` ⟹ `(. e f) : F[θ..]`.
The struct type must be known (head constructor resolved) when the rule
fires; the checker defers the constraint until it is, and reports
`field access on value of unknown type` if it never is.

### 2.6 `match`

```
e : σ ;  for each clause (p body): p matches σ binding Γp ;  Γ, Γp ⊢ body : τ  ⟹ τ
```

Pattern typing: `_`, `x` match any `σ` (`x : σ`); a literal matches its
scalar type or `str`; `(V p1..pk)` requires `σ = (E θ..)` where `V` is a
variant of `E` with field types `F1..Fk`, and `pi` matches `Fi[θ..]`;
`(S p1..pk)` likewise for a struct; `nil`/`(some p)` for `option`.
Exhaustiveness: for enums and `option`, every variant covered (recursively
through nested patterns, standard usefulness check); for other types, a
final `_`/variable clause is required.

### 2.7 `defstruct`, `defenum`, `defprotocol`, `impl`

Well-formedness only: field types are closed under the declared type
parameters; every named type exists; a struct or enum may mention itself.
`impl P T` must provide every method of `P` with a type that instantiates
the protocol's method type at `Self = T`; each method body is checked as a
`defun` with `self : T` (or `self : (cell T)` for `&self`). The protocol's
own type parameters must be determined by `Self` (§4.1).

### 2.8 `async`, `await`, `unsafe`, `quote`

```
(async body):  Γ ⊢ body : τ  ⟹ (task τ)       ; body is checked as a closure body (§2.3), κ irrelevant
(await e):     e : (task τ)  ⟹ τ               ; only inside async
(unsafe body): body : τ      ⟹ τ               ; raw-pointer builtins and externs typable only here
(quote f):     Form
```

`(extern g (P..) -> R)` gives `g : (fn (P..) R)` where each `P`, `R` is a
scalar or `ptr`; `ptr` is a scalar type available only inside `unsafe`.

### 2.9 `defmacro`

`(defmacro m (p1 .. pn) body)`: `pi : Form` (`... rest : (vec Form)`),
`body : Form`. The macro is type-checked as a defun in its module at
definition time and evaluated at expansion time.

---

## 3. Inference

**Approach (Proposed):** *unification-based local inference with explicit
polymorphism* — Hindley–Milner constraint solving with **no
generalisation** (rank-1, monomorphic unification variables) inside
definitions, explicit type parameters for anything generic, and a
module-wide solve so that unannotated non-generic `defun`s get their
types from their bodies and their uses. This is the discipline of Rust's
function bodies applied one level up: signatures are the boundary, and
an unannotated signature is just one with variables in it that the
module must pin down.

### 3.1 Data

- Types as in §1, with **unification variables** `α` and **rigid
  variables** `T` (a `defun`'s own type parameters inside its body).
- Function types carry `κ` as a small unification variable over
  `{send, local}`.
- A **substitution** `S` (union-find over variables).
- A worklist of **deferred obligations**:
  - `(P τ θ..)`: `τ` implements protocol `P` at parameters `θ..`;
  - `Send(τ)`;
  - `field(τ, f) = ρ`: `τ` is a struct with field `f` of type `ρ`;
  - `κ = send-iff(τ1 .. τn)`: closure attribute from capture types;
  - `lit-int(α)`, `lit-float(α)`.

### 3.2 Unification

Robinson unification with occurs check over the type grammar; nominal
constructors unify only with themselves and unify their arguments
pairwise; `(fn κ1 (A..) R)` with `(fn κ2 (B..) Q)` unifies arities,
parameters, results and `κ1 = κ2`; `(& A)` with `(& B)` only (a `&`
parameter never unifies with a plain one); rigid `T` unifies only with
itself or a variable. Occurs-check failure is the error
`infinite type` (case 15's original spelling would hit it; the case is
written nominally).

### 3.3 Obligations

An obligation is retried whenever a variable it mentions is bound; it is
resolved when its head types are known:

- `(P τ θ..)`: if `τ` is rigid `T`, it must be entailed by a bound on `T`;
  if `τ` is a nominal type or scalar, the unique `impl` for `(P, head τ)`
  is looked up and its parameters unified with `θ..` (functional
  dependency, §4.1); if `τ` is `(any P)`, satisfied; otherwise error
  `no implementation of P for τ`.
- `Send(τ)`: decided by the predicate in §5.7 once `τ`'s head is known;
  for a rigid `T` requires the bound `Send`.
- `field`: needs `τ`'s head; then unifies.
- `κ = send-iff(...)`: when every capture type's `Send` is decidable,
  `κ` is bound. If `κ` was already forced to `send` by a use (e.g. passed
  to `spawn`) and a capture is not `Send`, the error is reported at the
  closure: `cell cannot be shared between threads` when the offending
  capture's type is `(cell _)` (case 13), otherwise
  `value of type τ cannot be shared between threads`.
- `lit-int(α)`: if `α` bound to a non-integer type → error; at the end
  unbound → `i64`.

### 3.4 Expected types (check mode)

Inference is bottom-up except at four places where an expected type is
pushed down: `let` initialisers with an annotated binding (a prelude
macro adds `(x: T e)` support), function arguments against parameter
types, `if`/`match` branches against each other, and constructor
arguments. Pushing the expected type is what triggers the only
elaboration: an argument of concrete type `T` at a parameter of type
`(any P)` is wrapped in `(any-of P e)` (§4.2) if `(P T)` holds. No other
coercions exist.

### 3.5 Per-module algorithm

```
1. Expand macros (syntax.md §3.17).
2. Collect declarations: types, protocols, impls (check well-formedness),
   externs, and a signature skeleton for every defun: annotated positions
   as written, unannotated positions as fresh variables; generic defuns
   must be fully annotated (error otherwise).
3. For every defun and impl method body: generate constraints (§2),
   unify eagerly into one shared substitution S, queue obligations.
   Order does not matter; the substitution is global to the module.
4. Retry obligations until no progress.
5. Apply literal defaults; retry obligations; any obligation still
   unresolved is an error at its origin (ambiguous type).
6. Any signature variable still unbound is an error:
   cannot infer type of parameter x of f; add an annotation.
7. Zonk: substitute S everywhere. Every expression now has a ground type
   (or a rigid type parameter of its defun). Emit the interface
   (§3.6) and pass the typed program to the ownership checker (§5).
```

Errors are reported at the form that generated the failing constraint.
Because the substitution is module-wide, a mistake in one body can
surface as a mismatch in another; the message names both locations.

### 3.6 Across modules

A module imports **interfaces**, never re-infers a dependency. An
interface holds, per exported definition, the zonked signature with `κ`
attributes and escape kinds (§5.4), and for generic definitions and
macros the bodies (forms). A generic function is instantiated where it
is used: type parameters are replaced by fresh variables at each call
(§2.1) for checking, and the compiler monomorphises the body per
distinct ground instantiation across the whole program (§4.1, §7). A
non-generic exported function is checked once, in its own module; its
callers see a ground signature.

### 3.7 How each feature is handled

- **Closures:** `fn` introduces fresh parameter variables unless
  annotated; the body is checked in the extended `Γ`; the free variables
  of the body that are bound in enclosing scopes are the **capture set**,
  recorded on the closure node for §5; `κ` is an obligation over their
  types. A closure's result type is inferred from its body; passing the
  closure to a parameter of known function type pushes the parameter
  types in (§3.4), which is how `(fn (c) (conj c i))` in case 10 gets `c :
  (vec i64)` without an annotation.
- **Cells:** `(cell e) : (cell τ)`; reads go through the `Deref`
  protocol, whose parameter is functionally determined by `Self`, so
  `@c` has type `τ` as soon as `c`'s head is known. `set!` unifies the
  stored type with the cell's.
- **Atoms:** as cells plus the `Send` obligation on construction.
  `swap!`'s function argument is checked against `(fn (T) T)`; its `κ`
  is unconstrained (it runs on the calling thread).
- **Protocols:** method calls generate `(P τ θ..)` with `τ` the
  receiver's type; the call's result type is the method's result with
  `Self := τ` and `θ..` from the resolved impl. Static resolution
  happens in step 4; what remains after zonking is a direct call
  (§4.1) or, for an `(any P)` receiver, a vtable call (§4.2).
- **Generics:** rigid inside their definition (only the bounds are
  known about them), fresh variables at each use; bounds become
  obligations at the use site. No inference of type parameters for
  definitions; no polymorphic recursion problems because every
  definition's signature is written.
- **`&` parameters:** `Γ(v) = (cell A)`; the mode is checked
  syntactically at call sites (§5.5) and the unifier keeps `(& A)`
  distinct from `A` so a plain argument cannot be passed to an `&`
  position or vice versa.
- **Async:** `async` bodies are closures whose result is wrapped in
  `task`; `await` unwraps.

---

## 4. Protocol dispatch

Both static and dynamic dispatch exist; the type decides which.

### 4.1 Static (the default)

A method call whose receiver has a concrete type after inference is a
direct call to that type's implementation (a lIR `call` to a mangled
function name). Generic functions are **monomorphised** per ground
instantiation, so a bounded type parameter `(T Show)` also resolves
statically inside each instance. Consequences: no dispatch cost, no
boxing of scalars in generic code, and retain/release inside generic
code is emitted knowing which parameters are objects.

Protocol type parameters are **output** positions determined by `Self`:
for a given protocol head and implementing type head there is exactly one
`impl`, so `(Deref T)` for `Self = (cell i64)` yields `T = i64` without
annotation. A protocol whose parameters are not determined by `Self`
(i.e. two impls for the same `Self` head with different parameters) is
rejected at the second `impl`.

Protocol parameter escape kinds (§5.4) are declared on the protocol;
every `impl` is checked against them; callers rely on them without
seeing the implementation, which is what makes dispatch (static or
dynamic) sound for the escape analysis.

### 4.2 Dynamic: `(any P)`

`(any P)` is a two-word value `{ptr obj, ptr vtable}` (§7.5). It is
created by the elaboration `(any-of P e)` where `e : T`, `(P T)` holds,
and the expected type is `(any P)` (§3.4); the vtable is the global
constant for `(P, T)`. A method call on an `(any P)` receiver loads the
method's slot and calls indirectly with `obj` as `self`. `(any P)` is
chosen only when the program writes it (a field or parameter of that
type) — typically heterogeneous collections. It is not `Send` in v1
(the vtable would need a `Send` bit; Open decision 7).

`(any P)` does not support `&self` methods (the value is not a cell) and
does not support protocols with type parameters in v1.

### 4.3 The reference interpreter

The interpreter carries a type id in every object header (§7.2) and
dispatches every protocol call dynamically by (protocol, type id). That
is observably identical to the compiler's static resolution because an
impl is unique per (protocol head, type head).

---

## 5. The ownership checker

Runs after inference on a typed program, one function at a time, plus a
module-wide fixed point for parameter escape kinds (§5.4). Its outputs
are (a) accept/reject with the messages below, (b) per allocation site:
stack or heap, (c) per escape point: a retain, (d) per scope and step:
releases, (e) per parameter: escape kind, (f) per closure: escaping or
not. The reference interpreter needs only (a): it counts every reference
(ownership.md §2) and so never has to decide (b)–(f). The compiler must
produce the same frees; §5.9 states what "same" means.

### 5.1 Value classes

Every expression in a body is classified, after typing, as one of:

| Class | Expressions | Holds a count? |
|---|---|---|
| **scalar** | any expression of scalar type | n/a |
| **owned** | call result (incl. constructors, `conj`, `deref`, `join`, `swap!`), literal collection, `fn`, `async`, `cell`, `atom`, `weak`, string literal (immortal) | yes, +1 belongs to the holder |
| **borrow** | a variable whose binding is a borrow or a param; `(. e f)`; a pattern variable of `match` (`@v` is not a borrow: `deref` is owned) | no |
| **let-owned** | a variable bound to an owned expression | the binding holds the +1 |

A `let` binding of a **borrow** expression is itself a borrow (an alias);
a `let` binding of an owned expression owns it. `if`/`match`/`do` are
owned if every result branch is owned, borrow if every branch is a
borrow, and **mixed** otherwise; a mixed result is made owned by
retaining on the borrow branches (that is case 04 inside `pick`, but
seen from the caller's side; inside `pick` the return rule §5.4 does
the retain).

**Roots.** Each borrow has a set of *roots*: the parameters or let-owned
variables it may alias, computed syntactically:
`roots(x) = {x}` for a param or let-owned `x`; `roots(y) = roots(e)` for
`y` bound to borrow `e`; `roots((. e f)) = sub(roots(e))` (a sub-object
of a root — distinct from the root itself); `roots(if c a b) = roots(a)
∪ roots(b)`; `match` arms likewise; `do` its last step; a pattern
variable of `(match e ...)` is `sub(roots(e))`.

### 5.2 Escape classification (ownership.md §3), per form

An expression `e` is at an **escape point** when it appears in one of
these positions. If `e` is a borrow, a retain of the object is emitted
there (ownership.md §3: "+1 at the point of escape"); if `e` is owned,
its count transfers and nothing is emitted; scalars are copied.

| §3 clause | Position of `e` | Notes |
|---|---|---|
| 3.1 returned | result position of a `defun` or `fn` body (last step of `do`, both branches of `if`, all `match` arms, transitively) | §5.4 |
| 3.2 stored | argument to a constructor (`Struct`, `Variant`, `some`), to `cell`, `set!`, `atom`, `reset!`, or to any parameter of kind `:escapes` (`conj`, `assoc`, `push!` value, …) | the callee retains inside; from the caller's side the argument is treated as escaping for allocation decisions (§5.8) |
| 3.3 captured by an escaping closure | free variable of a `fn` or `async` that escapes (§5.3) | retained at closure creation |
| 3.4 passed to another thread | argument to `spawn` (`:escapes`) — `plet`/`pmap` reduce to it | plus the `Send` check (§5.7) and runtime share-marking (§7.7) |
| 3.5 held across an `await` | any capture of an `async` body | subsumed by 3.3: an `async` body is an escaping closure by definition |
| — passed to a closure value | argument to a call whose head is not a named function or resolved method | treated as `:escapes` (unknown callee); the callee retains if it stores |
| — `(weak e)` | | no retain, but forces `e`'s roots onto the heap (§5.8) |
| — `&x` copy-in | | `@x` is owned (deref), no extra retain |

Not escapes: passing a value to a `:borrow` parameter; reading a field;
binding with `let`; matching; passing to a `&` position (the private
cell is local, §5.5); `count`, `nth`, comparisons.

### 5.3 Closures: capture and escape

For each `fn`/`async` node the checker has its **capture set** `C`
(free variables bound in enclosing scopes, from §3.7). The closure
**escapes** iff it appears at any escape point of §5.2 (returned,
stored, captured by an escaping closure, passed to `spawn`, passed to
a `:escapes` parameter, passed to a closure value, or is an `async`),
or is bound by `let` to a variable that appears at one. This is decided
at the creation site, syntactically, in one pass with a fixed point over
`let`-bound closure variables.

Consequences:

- **Escaping closure:** every object in `C` is retained at creation and
  released when the closure is freed; the closure object is heap
  allocated and counted. Cells in `C` are captured by reference (the
  cell object itself is retained), which is what makes two closures share
  one cell (case 05) and why a closure over a cell keeps mutating the
  same cell after the creating scope is gone.
- **Non-escaping closure:** its environment holds uncounted borrows and
  lives on the stack of the creating frame; valid because the closure
  cannot outlive the call it is passed to (the callee's parameter is
  `:borrow`, so the callee does not store or return it) — this is what
  `:borrow` on `for-each`'s and `swap!`'s function parameter buys.
- **`&` parameter in `C` and the closure escapes:** error
  `& parameter captured by escaping closure` (case 18). Decided,
  ownership.md §5. Note that passing such a closure to a closure value or
  to a `:escapes` parameter counts as escaping (conservative).
- **`async` capturing a `&` parameter:** the same error, unless the
  enclosing defun is an async function, where §5.8's earlier check gives
  `& parameter in async function` (case 14).

### 5.4 Borrowed parameters, owned results (ownership.md §4)

Decided: parameters are borrows for the call; results are owned.

- **Return rule:** the result expression of a body is an escape point
  (§5.2, 3.1). If it is a borrow, the object is retained before
  returning; if owned, returned as is; if mixed (case 04: one branch a
  parameter, the other a fresh string) each borrow branch retains, so
  the caller always receives exactly one count. Returning a sub-object
  of a parameter (`(nth xs 0)`, case 01; `(. x f)`) retains that
  sub-object — always a heap object, because it was stored into its
  container (3.2).
- **Escape kind of a parameter** `p`: `:escapes` iff some escape point's
  expression has `p` itself (not `sub(p)`) among its roots, or `p` is
  captured by an escaping closure, or `p` is passed to `spawn`, or `p`
  is passed in a `:escapes` position of another call (including calls
  through closure values), or `(weak p)` appears. Otherwise `:borrow`.
  Computed as a module-wide monotone fixed point (start all `:borrow`;
  imported and protocol signatures are fixed inputs). An explicit
  `:borrow` on a defun parameter is checked and violated with
  `parameter p is declared :borrow but escapes`. Protocol methods: as
  declared (default `:escapes`); an `impl` whose body makes a `:borrow`
  parameter escape is rejected with `implementation of P/m makes
  parameter p escape; protocol declares it :borrow`.
- Why callers need the kind: a `:borrow` parameter lets the caller keep
  its argument on the stack and uncounted (§5.8) and lets a non-escaping
  closure be passed (§5.3). A `:escapes` parameter forces the caller's
  argument to be a counted heap object. Correctness of the count itself
  never depends on the caller: the callee retains at its escape points.

### 5.5 `&` parameters (ownership.md §5)

At each call form with `&` arguments, before desugaring:

1. Every `&` argument must be a plain variable of type `(cell T)` whose
   `T` unifies with the parameter's; else `& argument must be a variable
   holding a cell`.
2. **Distinct variables** (Decided): if two `&` arguments name the same
   variable → `variable x passed to more than one & parameter in call to
   f` (contains the canonical text; case 12). Purely syntactic, no alias
   analysis, exactly as §5 says; two different cells holding the same
   object are fine because updates copy when the count exceeds one.
3. The desugaring of syntax.md §3.13 is applied: the private cell is a
   local allocation whose only references are the callee's parameter and
   closures that capture it, which by §5.3 do not escape; so it is a
   **stack** cell (§7.4), needs no count, and its content is written back
   and released at return.
4. **Unique update** (Decided): the in-place builtins test the count of
   the object in the cell at run time: one → update in place, more →
   copy then update. Soundness under elided counts: every reference the
   compiler leaves uncounted is either a scope-local object that never
   escapes (then nothing else can hold it in a cell), a `:borrow`
   parameter whose caller holds a count or an unescaped stack object
   (the callee can't put it in a cell without it being `:escapes`), or a
   non-escaping closure environment (which borrows the same things its
   creator holds). Reads of cells (`@v`) are owned, so a value read from
   a `&` cell and still in use always contributes a count — case 08:
   `@v` passed to `for-each` is a retained temporary alive for the whole
   iteration, so `push!` copies.

### 5.6 Cells and weak references (ownership.md §6)

- `(cell e)`: `e` escapes into the cell. `@c`: owned result (+1). `(set!
  c e)`: `e` escapes; the previous content is released after the store
  (order matters for `(set! c (f @c))`: the temporary `@c` keeps the old
  value alive through `f`).
- Cycles: only cells can close them (Decided). The checker does not
  reject them. The audit at exit (method.md rule 2) classifies every
  still-live object: build the reference graph over live objects; an
  object is a **cycle leak** iff it is in, or reachable from, a strongly
  connected component that contains a cell and has at least one edge;
  such objects are reported as `leak-cycle`; any other live object is an
  audit failure (`leak`). Case 15 has SCC {Knot, cell, vec}.
- `(weak x)`: no retain; forces `x`'s allocation onto the heap (a stack
  object cannot be observed dead by a weak box); the first `weak` of an
  object allocates its weak box and sets the header flag (§7.6). `deref`
  on a weak: atomically "retain if count > 0" (§7.6); result `(option T)`
  owned. Case 20: the inner `let` releases the only count → the box is
  cleared in the object's destructor → `deref` gives `nil`.

### 5.7 Threads (ownership.md §7): the `Send` predicate

Decided: only immutable objects or atoms may cross. As a type-level
predicate, computed coinductively (a recursive type is assumed `Send`
while its own definition is being examined):

```
Send(scalar)            = true
Send(str) = Send(Form)  = true
Send((vec T))           = Send(T);   (map K V): Send(K) ∧ Send(V);   (set T): Send(T)
Send((S θ..))           = ∧ Send(field types with θ substituted)     ; struct
Send((E θ..))           = ∧ Send(variant field types)                ; enum
Send((option T))        = Send(T)
Send((cell T))          = false                                       ; "a cell that is not an atom may not cross"
Send((atom T))          = Send(T)    ; @a hands the value to the other thread
Send((weak T))          = Send(T)
Send((task T))          = Send(T)
Send((fn κ (A..) R))    = (κ = send)  ; captures decide; A.., R are passed at call time
Send((any P))           = false      ; v1
Send(T rigid)           = T has bound Send
```

Where it is checked: every `Send(τ)` obligation from a bound (`spawn`,
`atom`, `pmap`, `plet` via `spawn`). The error names the innermost
offending type: if the path to the failure ends in `(cell _)` the text is
`cell cannot be shared between threads` (case 13: the closure given to
`pmap` captures `n : (cell i64)`, so its `κ` cannot be `send`); otherwise
`value of type τ cannot be shared between threads`. `pmap` is a library
function whose signature is `[(A Send) (B Send)] ((f: (fn (A) B) :borrow)
xs: (vec A)) -> (vec B)`; its body calls `spawn` with closures that capture
`f`, so inference forces `f`'s `κ` to `send` — the check on user code
is ordinary unification, not a special case.

Runtime: `spawn` marks the closure and everything reachable from it
**shared** (§7.7) before the other thread can see it; shared objects use
atomic count operations; atoms mark each new value on `swap!`/`reset!`
when the atom is shared; `@a` is a single atomic retain-under-lock
(§7.4), which is the "implementation obligation" of §7.

### 5.8 Async (ownership.md §8) and other per-function checks, in order

For each `defun`:

1. **Async function with `&` parameter** → `& parameter in async
   function` (case 14; Decided). Checked first so it wins over the
   escaping-capture message.
2. `await` outside `async` → error.
3. `async` bodies are escaping closures (§5.3): all captures retained on
   creation, which is "retained on entry" of §8; the body's own locals
   live in the task frame with ordinary scoping. Case 11: `s` is captured
   → retained → outlives `main`'s inner `let`.
4. Closure escape decisions (§5.3), `&` call checks (§5.5), escape kinds
   (§5.4).
5. **Allocation sites**: a let-owned or temporary object is **heap and
   counted from birth** iff any expression with it among its roots
   reaches an escape point (§5.2) or a `weak`; else **stack, uncounted**,
   with its counted children released at scope end (Decided,
   ownership.md §2 "scope-local objects are not counted").
6. **Release placement**: owned temporaries at the end of their step
   (syntax.md §2); let-owned at scope end in reverse binding order;
   `&` private cells at call return after write-back; parameters never;
   closure environments when the closure is freed; task frames when the
   task is freed.

### 5.9 Same results, same frees

The interpreter counts every binding; the compiler elides. They agree
because: elision only removes counts on references that cannot be the
last (borrows are covered by their owner's count for their whole
lifetime; stack objects never escape), and the release points in §5.8
step 6 are the same scope ends and step ends at which the interpreter's
bindings die. The audit compares, per `do` step and scope exit, the
multiset of objects freed; a difference is a compiler bug (method.md
rule 6).

### 5.10 Unsafe (ownership.md §9)

`unsafe` changes nothing above. Externs are calls whose parameters are
`:borrow` unless listed in `:retains`, which makes them `:escapes` with
the retain emitted by the caller. Raw pointers (`ptr`) are scalars and
carry no count.

---

## 6. Case table

Column "mechanism" names the rule of this document that produces the
verdict; "audit" is what the interpreter's heap audit must report.

| Case | ownership.md | Mechanism (this doc) | Verdict / result | Error text (reject) |
|---|---|---|---|---|
| 01 return-part-of-argument | §4 | return rule §5.4: `(nth xs 0)` is `sub(xs)`, retained before return; caller's `h` owns it; `l` releases its vec at scope end | accept, 1, clean | — |
| 02 structural-sharing | §5 | `conj` returns a new version sharing nodes (library); `v` is `:borrow` in `add4`; `make` returns an owned vec; nodes held by both versions have count 2 until `v` dies | accept, 7, clean | — |
| 03 store-borrowed-value | §3.2 | `conj`'s `item` is `:escapes` (§1.5) → `remember`'s `item` is `:escapes` (§5.4 fixed point) → `s` heap and counted (§5.8.5); `conj` retains it twice | accept, 5, clean | — |
| 04 branch-dependent-owner | §4 | mixed result in `pick` (§5.1): the `x` branch retains, the literal branch is owned; `pick` is `:escapes` on `x` | accept, 5, clean | — |
| 05 closures-share-state | §6 | the two `fn`s are stored into `Counter` → escaping (§5.3) → each retains the cell `n`; `set!` writes the shared cell | accept, 2, clean | — |
| 06 capture-borrowed-param | §3.3 | the `fn` is returned → escaping → `prefix` retained at creation; `matcher`'s `prefix` is `:escapes` | accept, 1, clean | — |
| 07 recursive-accumulator | §4, §5 | self tail call → loop (§7.8); `acc` becomes an owned loop slot; old versions released as the loop advances | accept, 100000, clean | — |
| 08 mutate-while-iterating | §5 | `@v` is an owned temporary (§5.6) alive through `for-each`; `push!` sees count ≥ 2 and copies (§5.5.4); the closure captures the `&` cell but `for-each`'s `f` is `:borrow`, so it does not escape | accept, 6, clean | — |
| 09 iterator-outlives-source | §3.1 | `iter` stores `v` into the iterator struct (`:escapes`), retaining it; `evens` returns an owned iterator; `main`'s `v` dies, the iterator keeps the vec | accept, 2, clean | — |
| 10 atom-old-value | §7 | `plet` → `spawn` → `Send((atom (vec i64)))` holds; `@a` is an atomic retain (§7.4); `swap!` releases the old value; `snapshot` keeps it | accept, 1000, clean | — |
| 11 borrow-across-await | §8 | `async` is an escaping closure (§5.8.3): `s` retained at task creation; `measure`'s `s` is `:escapes` | accept, 5, clean | — |
| 12 reject-same-binding-twice-inout | §5 | syntactic distinct-variables check §5.5.2 on `(bar &x &x)` | reject | `passed to more than one & parameter` |
| 13 reject-cell-crosses-thread | §7 | `pmap` forces the closure's `κ = send`; capture `n : (cell i64)` fails `Send` (§5.7, §3.3) | reject | `cell cannot be shared between threads` |
| 14 reject-inout-in-async | §8 | `fill` is an async function with a `&` parameter (§5.8.1) | reject | `& parameter in async function` |
| 15 cycle-through-cell-leaks | §6 | `k` is stored into a vec stored into `k`'s own cell (both §3.2 escapes); at exit the SCC {Knot, cell, vec} contains a cell → classified `leak-cycle` (§5.6), not a failure | accept, 1, leak-cycle | — |
| 16 coordinated-update-single-atom | §7 | one `swap!` replaces the whole `Accounts`; readers see old or new, never in between; `Send(Accounts)` holds (two scalars) | accept, 200, clean | — |
| 17 inout-and-borrow-same-call | §5 | `&v` desugars to a private cell initialised from `@v` (+1); `@v` as second argument is a retained temporary; write-back at return; `push!` copies (count ≥ 2) | accept, 4, clean | — |
| 18 reject-inout-captured-by-escaping-closure | §5 | the `fn` is returned → escaping (§5.3) and captures `&` cell `v` | reject | `& parameter captured by escaping closure` |
| 19 weak-parent-pointer | §6 | `(weak parent)` no retain, box allocated; children cell holds strong edges downward only; `deref` retains-if-alive; at `main` exit root → a → b freed in cascade, boxes freed with their last weak holder | accept, 2, clean | — |
| 20 weak-ref-to-dead-object | §6 | `v` heap (forced by `weak`), single count released at inner `let` end → destructor clears the box; `deref` → `nil` | accept, 1, clean | — |

---

## 7. Mapping to lIR

lIR is the S-expression assembler for LLVM IR (liar's `doc/lIR.md`),
used exactly as it is: opaque `ptr`, `defstruct` with positional field
types, `define`/`declare`, `call`, `getelementptr`, `load`/`store`,
`phi`/`br`, `atomicrmw`/`cmpxchg`/`fence`. Everything fibber-specific is
a naming and layout convention on top; no liar or fibber vocabulary
enters lIR. Runtime support functions are ordinary lIR `define`s in a
`fib.rt` module (or C, linked). Hardening needs from the lIR audit that
this mapping relies on: a real module type checker, `tailcall` as
`musttail`, and a typed `indirect-call` (the audit found every argument
typed as `ptr`).

### 7.1 Representation of every type

| fibber type | lIR type | Notes |
|---|---|---|
| `bool` | `i1` | |
| `i8 i16 i32 i64` | same | |
| `f32`, `f64` | `float`, `double` | |
| `char` | `i32` | Unicode scalar value |
| `keyword` | `i64` | interned id |
| `unit` | no value; functions returning `unit` are `void` | `()` in a value position is `(i1 0)` and dropped |
| field-less `defenum` | `i32` | variant index |
| every object type (`str`, `vec`, `map`, `set`, `Form`, struct, enum with fields, `cell`, `atom`, `weak`, `task`, closure) | `ptr` to an object with the header of §7.2 | |
| `(option T)`, `T` an object | `ptr`, null = `nil` | no allocation for `some` |
| `(option T)`, `T` a scalar | `ptr` to a heap enum object | v1; unboxing is Open decision 8 |
| `(fn (A..) R)` | `ptr` to a closure object (§7.3) | named functions used as values are static closure objects with no captures |
| `(any P)` | `{ ptr, ptr }` by value: object, vtable | §7.5 |
| `(& T)` parameter | `ptr` to a cell (§7.4) | stack cell at the call site |
| `ptr` (unsafe) | `ptr` | uncounted |

Scalars inside objects are stored unboxed at their lIR type. A struct
`(defstruct P (x: i64 s: str))` is `(defstruct P.obj (i64 i32 i32 i64 ptr))`
— header fields first, then fields in declaration order; the compiler
emits one lIR `defstruct` per monomorphised object type.

### 7.2 The count header

Every object begins with a 16-byte header:

```
(defstruct fib.hdr (i64 i32 i32))      ; count, type-id, flags
flags: bit 0 SHARED   (counts are atomic from now on)
       bit 1 HAS-WEAK (a weak box exists; see 7.6)
       bit 2 IMMORTAL (static data: literals, named-function closures; retain/release are no-ops)
```

`count` is the number of counted references (ownership.md §2). Type-id
indexes a global table `fib.types` of per-type records
`{ ptr drop, ptr trace, ptr name, i64 size }`: `drop` releases the
object's counted children and frees it; `trace` calls a callback on each
child pointer (used by share-marking §7.7 and the interpreter's audit).
Both are generated per object type by the compiler.

Runtime primitives (lIR `define`s, inlinable):

```
fib.retain  (ptr) -> void   ; if IMMORTAL: return. if SHARED: atomicrmw add count 1 (monotonic)
                            ; else: count += 1
fib.release (ptr) -> void   ; if IMMORTAL: return. n = (SHARED ? atomicrmw sub count 1 (acq_rel) : --count)
                            ; if n == 0: (HAS-WEAK ? fib.weak-clear(obj)) ; call fib.types[tid].drop(obj)
fib.alloc   (i64 size, i32 tid) -> ptr    ; malloc; count = 1, flags = 0
fib.unique? (ptr) -> i1     ; count == 1 (atomic load if SHARED)
```

Stack objects (§5.8.5) are `alloca`s with the same layout, `count` unused,
and no `fib.release` call; their `drop` body is emitted inline at scope
exit (children released; no free).

What the compiler must emit: `fib.retain` at every escape point of a
borrow (§5.2), at closure creation for each captured object of an escaping
closure, at `@c`/`@a`/`(deref w)`; `fib.release` at every release point
of §5.8.6, in `set!`/`swap!`/`reset!` for the old value, in the write-back
of a `&` call for the caller's old value, and in each generated `drop`.
Nothing else touches counts.

### 7.3 Closures

```
(defstruct fib.closure (i64 i32 i32  ptr  ...captures))   ; header, code, captures in capture order
```

`code` is the lifted function `@name.lambda_N` with signature
`(define (R) ((ptr env) (A1 p1) .. (An pn)) ...)`; captured variables are
loaded from `env` by `getelementptr`. A call through a closure value `f`:

```
(indirect-call (fn R (ptr A1 .. An)) (load ptr (getelementptr fib.closure f (i32 0) (i32 3))) f a1 .. an)
```

(typed indirect call — the hardened form). Named functions used as
values are `(constant name.clo fib.closure {(i64 0) (i32 tid) (i32 IMMORTAL) @name.lifted})`
where `@name.lifted` ignores `env`. Non-escaping closures (§5.3) are
`alloca`'d with captures stored as uncounted borrows; escaping ones are
`fib.alloc`'d with each object capture retained. The closure type's
`drop` releases the captures.

### 7.4 Cells, `&` cells, atoms

```
(defstruct fib.cell (i64 i32 i32  T))            ; T = lIR type of the content (ptr for objects)
(defstruct fib.atom (i64 i32 i32  ptr  i32))     ; header, value, spinlock
```

- `@c` on a cell: `load`, then `fib.retain` if `T` is an object.
- `(set! c v)`: `old = load`, `store v`, `fib.release old`.
- A `&` argument: `(alloca fib.cell)` in the caller, initialised with the
  retained `@x`; passed as `ptr`; at return: `new = load t`, `old = load
  x.value`, `store new x.value`, `fib.release old` (the private cell's
  count on `new` moves to `x`; nothing else to release).
- In-place builtins (`push!` etc.) read the cell, call `fib.unique?`, and
  either mutate the object's storage or build the new version and
  `store`+`fib.release old`.
- Atom `@a`: `fib.lock a` (cmpxchg spinlock, acquire), `v = load`,
  `fib.retain v`, `fib.unlock` — the single atomic step §7 requires.
  `swap!`: `old = @a` (retained); `new = f(old)`; lock; if `load == old`
  then `store new; unlock; fib.release old` (the atom's count) and
  `fib.release old` (our snapshot) else `unlock; fib.release old;
  fib.release new; retry`. `reset!`: lock, exchange, unlock, release old.
  Values stored into a SHARED atom are share-marked first (§7.7).

### 7.5 Sum types and dynamic dispatch

```
(defstruct E.obj (i64 i32 i32  i32  [payload]))   ; header, variant tag, then the largest variant's fields
```

Each variant has its own lIR `defstruct E.V.obj` with the same prefix;
`match` loads the tag, `switch`es, and `getelementptr`s through the
variant struct. `(option T)` with object `T` is the bare pointer (7.1).
Field-less enums are `i32` and need no object. The `drop` of an enum
switches on the tag to release the right fields.

`(any P)`: value `{ptr obj, ptr vt}`; `vt` is
`(constant P.for.T (ptr ptr ...) {@P.m1.T @P.m2.T ...})`, one slot per
method in protocol order, each a `ptr` to that type's implementation
with `self` as `ptr`. A method call is `(indirect-call (fn R (ptr ..))
(load ptr (getelementptr [k x ptr] vt (i32 0) (i32 i))) obj args..)`.
Retain/release of an `(any P)` value act on `obj`. Static dispatch is a
plain `(call @P.m.T ...)`.

### 7.6 Weak references

```
(defstruct fib.weakbox (i64 i32 i32  ptr  i32))   ; header (its own count = number of weak refs), target, lock
```

`(weak x)`: if `x` has HAS-WEAK, its box is found in a global table
keyed by address (only objects with the flag are ever looked up); else a
box is allocated, registered, and the flag set; the box is retained and
returned. `(deref w)`: lock the box; `t = load target`; if null →
unlock, `nil`; else attempt "retain if count > 0" on `t` (non-atomic
`count += 1` when not SHARED; `cmpxchg` loop refusing zero when SHARED);
unlock; return `t` (owned) or `nil`. `fib.weak-clear(obj)`: lock, store
null, unregister, unlock; called from `fib.release` before `drop` (7.2).
The box is freed when its own count reaches zero (the last `weak` value
released). Only objects that had a `weak` taken pay: one flag test in
`release`, and the table lookup happens only in `weak`/`weak-clear`.

### 7.7 Threads and share-marking

`spawn f`: `fib.share f` walks `f` and everything reachable through
`fib.types[tid].trace`, setting SHARED on each object (idempotent; stops
at already-SHARED objects; atoms mark their current value), then hands
`f` to a new OS thread (or pool). From then on counts on those objects
use atomic operations, and every value later stored into a SHARED atom
is share-marked before the store. Objects that never cross keep the
non-atomic path (ownership.md §7).

`task`: `(defstruct fib.task (i64 i32 i32  i32 state  ptr resume  ptr result  ptr waker  ...captures/locals))`.
`async` lowers to a state machine: `resume(task)` switches on `state`,
runs to the next `await`, stores live locals into the task object, and
returns. `await e` registers `waker` with `e` and yields. `join`/`block-on`
drive the executor until `state == done`, then move `result` out
(owned). The task's `drop` releases captures, live locals and `result`.
The reference interpreter may instead run tasks as coroutines; frees must
match.

### 7.8 Calls, returns, tail calls

- Function signature `(define (R) ((A1 p1) .. (An pn)))`; `&` parameters
  are `ptr`; `unit` results are `void`.
- The caller releases owned temporaries after the call returns (§5.8.6);
  the callee retains what it makes escape (§5.2). No calling-convention
  ownership transfer exists except for `spawn`'s argument (moved).
- A **self tail call** `(f e1 .. en)` in result position is compiled as
  a loop: parameters become `alloca` slots; on entry every object
  parameter is retained (the slot owns it); each iteration evaluates the
  new arguments into temporaries, releases the old slot values, stores
  the new; the loop exit releases the slots after the result is
  computed (retaining the result first if it is one of them). Case 07's
  `(conj acc n)` temporary thus becomes the next owned `acc` with no
  extra count. Tail calls to other functions are ordinary calls (Open
  decision 6).

### 7.9 What the interpreter shares with this

The interpreter uses the same header, type table, `drop`/`trace`
functions (interpreted), weak boxes and share flags, so that its audit
log (every `alloc`, `retain`, `release`, `free`, `access`) is
comparable one-to-one with an instrumented build of the compiled program.

---

## Open decisions (types and checker)

Recommendation first, alternative second. Items 1–3 affect what the
checker must implement; the rest are representation.

1. **Closure types do not carry parameter escape kinds; a call through a
   closure value treats every argument as escaping.** Recommend: yes —
   keeps function types simple and unification-only; the cost is a
   retain/release pair per object argument to a closure call, and that
   a `&`-capturing closure cannot be passed to a closure value.
   Alternative: kinds in closure types `(fn ((A :borrow) B) R)`, which
   makes every higher-order signature heavier and needs variance rules.

2. **Protocol method parameters default to `:escapes`; `:borrow` is an
   explicit promise checked on every `impl`.** Recommend: yes (safe
   default; the library marks `for-each`, `swap!`, `pmap`, `reduce`
   `:borrow`). Alternative: infer kinds from the impls visible in the
   module, which is unsound across modules (open world).

3. **`&` parameters are cells read with `@v`, so every read is a retained
   temporary.** Recommend: yes; it is what makes §5.5.4 sound without a
   separate liveness rule, and the retain is elidable later by an
   optimisation ("no `&v` or `&v`-capturing closure is passed in the
   temporary's live range"). Alternative: auto-deref with that rule as a
   soundness requirement from day one.

4. **Module-wide monomorphic inference for unannotated non-generic
   defuns; generics fully annotated.** Recommend: yes — one unifier,
   one substitution, no generalisation, and the cases stay light.
   Alternative: mandatory signatures on every `defun` (Rust); simpler
   error locality, more annotation in every case file.

5. **Monomorphisation of generics, static dispatch by default, `(any P)`
   for dynamic.** Recommend: yes; retain/release in generic code then
   knows which parameters are objects. Alternative: uniform boxed
   representation with dictionary passing — one copy of each generic
   body, but scalars box and every count operation needs a runtime
   type test.

6. **Self tail calls only become loops.** Recommend: yes for v1.
   Alternative: an owned-argument calling convention for tail calls to
   other functions (callee releases), needed for mutual recursion
   without stack growth; a second convention doubles the calling rules
   the checker must get right.

7. **`(any P)` is not `Send` and has no `&self` methods in v1.**
   Recommend: yes. Alternative: a `Send` bit in the vtable and a
   `(any P :send)` type.

8. **`(option T)` of a scalar is a heap object.** Recommend: accept for
   v1 (rare in practice; `(option i64)` mostly appears in library
   iterators, where it is short-lived). Alternative: an unboxed
   `{i1, T}` by-value representation, which introduces a second value
   class (non-scalar, non-pointer) into every rule of §5.

9. **Atoms use a per-atom spinlock and `swap!` retries with `f` outside
   the lock.** Recommend: yes; it satisfies the §7 obligation directly
   and `f` may be re-run, as in Clojure. Alternative: run `f` under the
   lock (no re-run, but `f` may not touch the same atom) or hazard
   pointers/deferred reclamation (no lock, more machinery).

10. **Leak-cycle classification = SCC containing a cell, or reachable
    from one.** Recommend: yes; it is exactly "unreachable except
    through a cycle of cells". Alternative: report all leaks as
    failures and require a cycle collector before any cell-cycle
    program is accepted, which contradicts §6's decision.

11. **Weak boxes are separate allocations found through a global table.**
    Recommend: yes — only objects that ever had a weak reference pay,
    and the header stays 16 bytes. Alternative: a weak-count field in
    every header (simpler, 8 bytes on every object).

12. **Stack allocation is decided per allocation site by the escape
    analysis of §5.8.5.** Recommend: yes. Alternative: heap everything
    in v1 and let the interpreter/compiler agreement on frees be checked
    first; stack allocation then becomes a pure optimisation pass
    verified by the same audit. This is the lowest-risk order of
    implementation and is compatible with the recommendation.
