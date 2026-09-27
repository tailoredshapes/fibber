# fibber types and the ownership checker (draft: ownership-first angle)

Status: draft. **Decided** marks a direct consequence of
`spec/ownership.md`; **Proposed** marks everything else.
`spec/ownership.md` is authoritative. Surface syntax is in `syntax.md`.

Organising principle: the type system exists to make every rule of
`spec/ownership.md` a property the checker computes from types and
syntax alone, with no alias analysis, no lifetimes and no
flow-sensitive reasoning beyond scope nesting. The checker's outputs
(§9) can be printed and compared against the reference interpreter.

Sections: 1 type grammar; 2 typing rules per form; 3 inference
algorithm; 4 protocol dispatch; 5 the `Send` predicate; 6 how the
ownership rules are decided; 7 case table; 8 mapping to lIR; 9 what
the checker prints; 10 open decisions.

---

## 1. Type grammar

```
type   := scalar | object | tvar | (Fn (type...) type [flag])
scalar := bool | i8 | i16 | i32 | i64 | f32 | f64 | char | unit
object := str
        | (Vec type) | (Map type type) | (Set type) | (List type)   ; library, nominal
        | (Array type)                                             ; primitive immutable array
        | Name | (Name type...)                                    ; struct or sum type
        | (Cell type) | (Atom type) | (Weak type)
        | (Task type) | (Future type)
        | (dyn Proto) | (dyn (Proto type...))
        | ptr                                                      ; only inside unsafe
tvar   := a | b | ... (lower-case symbols in signatures)
flag   := :send | :local                                           ; closure sendability, §5.5
```

### 1.1 Scalars

**Decided** (§1): scalars are copied, have no identity and no count.
`bool` is lIR `i1`; `i8..i64` are the lIR integers; `f32`/`f64` are
`float`/`double`; `char` is a Unicode scalar value in an `i32`; `unit`
has the one value `()` and is erased in codegen (functions returning
`unit` return `void`). No implicit conversion between any two scalar
types (**Decided** by lIR's no-promotion rule). Literals: an unsuffixed
integer literal is `i64`, an unsuffixed float literal is `f64`
(**Proposed**; the alternative, literals polymorphic over the integer
types with defaulting, is in §10).

### 1.2 Objects

**Decided** (§1): every object type denotes a heap-or-stack value with
a count header (§8.1), immutable except `Cell` and `Atom`. An object
value is always represented as one pointer. The library collections
are ordinary `defstruct`s in `fib.core` built on `(Array T)`; they are
listed here because the checker knows their names for literal syntax
(`[...]` builds a `Vec`, `{...}` a `Map`).

### 1.3 Nominal types

`defstruct` and `defsum` introduce nominal types, possibly parametric.
Two nominal types are equal iff they are the same definition applied to
equal arguments. Structural types (`Fn`, `Cell`, ...) are equal iff
constructor and arguments are equal.

### 1.4 Functions and closures

`(Fn (A...) R)` is the type of every callable: `defun`s, closures,
variant and struct constructors, primitives. A `defun` used as a value
is a closure with no captures. `Fn` types carry a sendability flag
(§5.5) written `:send` or `:local` in printed types; in source
annotations the flag may be omitted, meaning `:local` in a struct field
or parameter annotation (the permissive choice for storage) — see §10.

`&` parameters are part of a `defun`'s *signature*, not of its `Fn`
type: `(defun f (&v x) ...)` has signature `(f (& (Vec i64)) i64) -> unit`
and cannot be used as a first-class value (**Proposed**; §10).

### 1.5 `Cell`, `Atom`, `Weak`

- `(Cell T)`: **Decided** (§6) the one mutable container; not
  sendable.
- `(Atom T)`: **Decided** (§7) a cell with atomic operations; well
  formed only if `T` is sendable (§5.7), so an atom can always cross.
- `(Weak T)`: **Decided** (§6) a non-owning reference; `(upgrade w)`
  has type `(Option T)`. `Weak` is an object type for the ownership
  rules (it is retained and released, on its own weak count; §8.5).

### 1.6 Option and nil

`(Option a)` is the library sum type `(defsum (Option a) nil (some a))`.
`nil` has type `(Option a)` for a fresh `a`; there is no null of any
other type and no implicit lifting `T -> (Option T)`. Where an object
type appears inside `Option`, codegen uses the null pointer for `nil`
(§8.4), so the representation matches liar's `nil` at no type-system
cost.

### 1.7 Recursive types

Recursion is allowed only through a nominal type: a struct or sum type
may mention itself. Unification performs the occurs check, so an
inferred cyclic type is an error (`cannot construct the infinite type`).
This is why case 15 is written with a struct.

### 1.8 Generics

Type variables are introduced by a `defstruct`/`defsum`/`defprotocol`
head, or by generalisation at a `defun` boundary (§3.2). A generic
`defun`'s signature may carry *bounds*: `(Countable a)` (protocol
constraints) and `(Send a)` (§5). There is no subtyping anywhere
(**Proposed**); `(dyn P)` is reached only by the explicit `(dyn P e)`
primitive.

### 1.9 Protocol types

`(dyn P)` is the existential "some type implementing `P`". Its values
are fat pointers (§8.6). `(dyn P)` itself implements `P` (dynamic
dispatch) and nothing else. It is sendable iff `P` is declared
`:send` (§5.7).

---

## 2. Typing rules for core forms and primitives

Judgement: `Γ ⊢ e : T`, with `Γ` mapping variables to types and
marking each as `param`, `&param`, `let`, or `captured`. Rules are
given as "requirements ⇒ result". `fresh` means a fresh type variable.
Unification `~` is §3.1. Every rule is also an inference rule: it
generates equations, it does not require types to be known in advance,
except where stated ("must be resolved").

### 2.1 Literals and variables

| Form | Result |
|---|---|
| integer literal (suffix `S` or none) | `S`, default `i64` |
| float literal | `f32`/`f64`, default `f64` |
| `"..."` | `str` |
| `\c` | `char` |
| `true`/`false` | `bool` |
| `()` | `unit` |
| `nil` | `(Option fresh)` |
| `sym` bound in Γ | `Γ(sym)`; if `sym` is a `&`-param it is used as its value type |
| `sym` a `defun` | instance of its generalised signature (§3.2); error if it has `&` params |
| `sym` a variant constant | `Name` (instantiated) |
| `[e1 ... en]` | all `ei ~ a` ⇒ `(Vec a)` |
| `{k1 v1 ...}` | all `ki ~ k`, `vi ~ v` ⇒ `(Map k v)` |

### 2.2 Calls

```
(f a1 ... an)      f : (Fn (T1 ... Tn) R),  ai : Ti          ⇒ R
(g ... &x ...)     g a defun/method with & at that position of value type T:
                   x : (Cell T)  or  x is a &-param of type T  ⇒ ok
```

Arity must match exactly. `&x` at a position whose parameter is not
`&` is an error; a non-`&` argument at an `&` position is an error
(`parameter v of g is &; pass &x`). A call to a `defun` instantiates
its signature with fresh variables and records its bounds as
constraints (§3.3).

### 2.3 `defun`

```
(defun f (p1 ... pn) [-> R] body)
```

Each `pi` gets its annotation or `fresh`; `body : R'`; `R' ~ R` if
annotated. After the whole strongly-connected component of mutually
recursive `defun`s is inferred, each is generalised (§3.2). Inside the
SCC the function is monomorphic. `main` must have type `(Fn () i64)`.

### 2.4 `fn`

```
(fn (p1 ... pn) [-> R] body)     ⇒ (Fn (T1 ... Tn) R  φ)
```

`pi : Ti` (annotation or fresh), `body : R`. `φ` is a fresh
sendability flag, constrained by the captures (§5.5). Closures are
never generalised (monomorphic; **Proposed**, §3.2).

### 2.5 `let`, `do`, `if`

```
(let ((pat1 e1) ... ) body)   ei : Ti, pat_i : Ti binds Γ_i, body under all ⇒ T_body
(do e1 ... en)                each ei typed; ⇒ Tn   ((do) ⇒ unit)
(if c t e)                    c : bool, t : T, e : T ⇒ T
```

A non-final `do` form whose type is not `unit` is allowed (its value
is dropped); **Proposed**: a warning if it is an object type, since
that is usually a mistake.

### 2.6 `match`

```
(match e (pat1 b1) ... (patn bn))   e : T, each pati : T binds Γ_i, bi : R ⇒ R
```

Pattern typing: `_` and `sym` match any `T`; a literal must have
`T`; `nil` and `(some p)` require `T ~ (Option a)`; `(ctor p...)`
requires `T ~ (Name args)` for the sum type declaring `ctor`, with the
payload types instantiated; `(Struct p...)` likewise for structs;
`[p...]` requires `T ~ (Vec a)`. Exhaustiveness is checked after
inference on the resolved type of `e`; if `T` is still a variable at
that point, only `_`/`sym` clauses are exhaustive.

### 2.7 `defstruct`, constructor, field access

```
(defstruct (N a...) (f1 T1) ... (fk Tk))
N : (Fn (T1 ... Tk) (N a...))                      constructor, generalised over a...
(. e fi)                                            e : (N args) resolved ⇒ Ti[args/a]
```

`(. e f)` is a *deferred* constraint: it is recorded as
`HasField(τ, f, ρ)` on the current type `τ` of `e` and solved as soon
as `τ` is unified with a struct type (§3.4). If `τ` is still a variable
when the enclosing `defun` is generalised: error
`cannot infer the struct type of e for field f; annotate it`.

### 2.8 `defsum`, variants

```
(defsum (N a...) v1 (v2 T...) ...)
v1 : (N a...)                     constant
v2 : (Fn (T...) (N a...))         constructor
```

### 2.9 `cell`, `deref`, `set!`

```
(cell e)                e : T ⇒ (Cell T)
(deref e)  @e           e : (Cell T) ⇒ T        e : (Atom T) ⇒ T
(set! x e)              x a &-param of type T, e : T ⇒ unit
(set! c e)              c : (Cell T) (a variable or (. e' f)), e : T ⇒ unit
```

`deref` on a variable of unresolved type is deferred like field access
(`HasDeref(τ, ρ)`), resolved when `τ` becomes `Cell` or `Atom`.

### 2.10 `atom`, `swap!`, `reset!`

```
(atom e)      e : T, Send T ⇒ (Atom T)
(swap! a f)   a : (Atom T), f : (Fn (T) T φ) ⇒ T
(reset! a e)  a : (Atom T), e : T ⇒ T
```

`f`'s flag `φ` is unconstrained: `swap!` runs `f` on the calling
thread.

### 2.11 `spawn`, `join`

```
(spawn f)     f : (Fn () T :send), Send T ⇒ (Task T)
(join t)      t : (Task T) ⇒ T
```

### 2.12 `async`, `await`

```
(async body)  body : T ⇒ (Future T)        (no Send requirement; syntax.md §2.15)
(await e)     e : (Future T) ⇒ T            only lexically inside an async body
```

An `await` inside a `fn` inside an `async` is an error (the `fn` is a
different frame).

### 2.13 `weak`, `upgrade`

```
(weak e)      e : T, T an object type ⇒ (Weak T)
(upgrade w)   w : (Weak T) ⇒ (Option T)
```

### 2.14 Protocols

```
(defprotocol (P a...) (m (self x1 ... xk) -> R) ...)
m : (Fn (S T1 ... Tk) R)  with bound (P a... S), generalised over S and a...
(impl (P A...) S (m (self x...) body) ...)   registers instance (P A... S); bodies checked
                                             against the signatures with self : S
(dyn P e)     e : S, instance (P S) must be resolved ⇒ (dyn P)
```

Instances are keyed by the head constructor of `S` (§3.5).

### 2.15 `unsafe`, `extern`

```
(unsafe body)                       body : T ⇒ T; enables ptr ops and extern calls inside
(extern name (T1...) -> R)          name : (Fn (T1...) R), callable only inside unsafe
(raw e)                             e : object ⇒ ptr    (address, valid to end of enclosing scope)
```

`ptr` may not be stored in a struct field, captured, or returned from
a function that is not itself entirely an `unsafe` body
(**Proposed**); it is a scalar for the ownership rules (no count).

### 2.16 `defmacro`, `quote`

Macros are expanded before typing and have no typing rule. A `quote`
form that survives expansion has type `Form` (the `fib.form` sum type)
and is only legal inside a macro body.

### 2.17 Primitive arithmetic

`+ - * / rem` : `(Fn (I I) I)` for each integer type `I`, resolved by
the operand types; `= != < <= > >=` : `(Fn (I I) bool)`; `f+` etc. for
floats; `not : (Fn (bool) bool)`. An integer operator whose operands
are unresolved defaults them to `i64` at generalisation (**Proposed**).
`=` on `str`, `char`, `bool` is also primitive; on other object types
it is the `Eq` protocol (library).

---

## 3. Inference algorithm

**Proposed**: Hindley–Milner inference with union-find unification,
let-polymorphism *only at `defun` boundaries*, single-parameter
protocol constraints with the `self` type as the dispatch key, deferred
field/deref constraints, and a separate post-pass for sendability
flags. Named here as "HM(defun) + deferred structural constraints".
This is the smallest algorithm that types the case files and the
liar standard library without annotations on most parameters, while
keeping every decision local to one `defun` (so the ownership pass can
run per `defun` on fully resolved types).

### 3.1 Unification

Types are terms over constructors (`Fn`, `Vec`, `Cell`, nominal names,
scalars) and variables. Union-find with path compression; unifying two
constructors requires the same head and arity and unifies arguments
pairwise; unifying a variable with a term binds it after the occurs
check (§1.7). Flags (§5.5) are a separate sort of variable with values
`send`/`local`, unified by the same machinery but solved by §5.6.

### 3.2 Generalisation and instantiation

- Program order: the compiler builds the call graph of `defun`s in a
  module (names referenced in bodies), computes strongly connected
  components (Tarjan), and infers components in reverse topological
  order: callees before callers.
- Within a component every `defun` gets a monomorphic type; the bodies
  are inferred; then each function's type is generalised over the
  variables not free in the (empty, for top level) environment. Bounds
  collected on those variables (§3.3) become the signature's bounds.
  Remaining flag variables become flag parameters.
- A call to a generalised `defun` instantiates all its quantified
  variables (types and flags) freshly and adds its bounds, instantiated,
  to the current constraint set.
- `let` bindings and `fn` literals are not generalised. A local
  polymorphic helper must be a `defun`. (Alternative in §10.)
- Annotations are unified in, never trusted alone: a body that
  disagrees with its annotation is an error at the disagreement.

### 3.3 Constraints

Constraints are collected in a worklist during inference of a
component and solved incrementally whenever unification binds a
variable:

| Constraint | Solved when | Failure |
|---|---|---|
| `(P A... τ)` protocol bound | head of `τ` known: instance lookup (§3.5) unifies `A...` with the instance's parameters and discharges; if `τ` is quantified at generalisation it becomes a bound of the signature | `no impl of P for τ` |
| `HasField(τ, f, ρ)` | `τ` becomes a struct: `ρ ~` field type | `τ has no field f`; unresolved at generalisation: `cannot infer the struct type` |
| `HasDeref(τ, ρ)` | `τ` becomes `Cell`/`Atom` | unresolved: `cannot infer whether x is a cell or an atom` |
| `Send τ` | structural (§5.7), re-run as `τ` resolves; quantified ⇒ bound | `cell cannot be shared between threads: ...` |
| `IntLit(τ)` | `τ` resolves to an integer type; otherwise defaults to `i64` at generalisation | `τ` resolves to a non-integer: `integer literal used as τ` |
| `Flag(φ ≤ ...)` | §5.6 | `closure captures X of type (Cell T) and is passed where a :send closure is required` |

At generalisation, every remaining `HasField`/`HasDeref` on a variable
is an error (they cannot be bounds: dispatch needs the struct name).

### 3.4 Field access without annotations

`(. x f)` on an unresolved `x` is legal as long as *some* later use in
the same `defun` fixes `x`'s type: a constructor argument, a call to a
function with a known parameter type, or an annotation. Case 16's
`transfer` is resolved by the `Accounts` constructor in the closure
body; case 19's `depth` has only field accesses and a recursive call,
so it is annotated. The rule "unresolved at generalisation is an
error" is what keeps the checker simple: there is no search for "the
unique struct with a field `f`" (alternative in §10).

### 3.5 Protocol instances

An instance `(impl (P A...) S ...)` is keyed by `(P, head(S))`. Only
one instance per key (**Proposed**: no overlapping instances, no
instance for a bare type variable). Instance lookup for `(P A... τ)`
with `head(τ) = H`: find `(P, H)`, instantiate its type variables,
unify the instance's `S` with `τ` and its `A...` with the constraint's
`A...` (the protocol parameters are determined by `S`: a functional
dependency `S → A...`). The instance may carry its own bounds (e.g.
`(impl (Countable) (Vec a))` has none; `(impl Eq (Vec a))` requires
`(Eq a)`), which are added to the worklist.

### 3.6 Closures

A `fn` literal's parameter and result types are fresh variables unified
by its body and by its uses. Its captures are the free variables of its
body that are bound in the enclosing `defun`. The capture list is
computed syntactically after macro expansion and before inference; it
is needed by the flag pass (§5.5) and by the ownership pass (§6.5). The
closure's type is monomorphic in the enclosing function; when the
enclosing `defun` is generalised, type variables appearing in the
closure's type are quantified with the function.

### 3.7 Cells and atoms

`(cell e)` and `(atom e)` are ordinary constructors for the type
system. `set!`'s first operand must resolve to `Cell` (not `Atom`:
atoms are written only by `swap!`/`reset!`). `Send` on `(atom e)` is a
constraint solved structurally; if `e`'s type is still a variable at
generalisation, the bound `(Send a)` is added to the signature, so a
generic function that makes an atom of its argument requires sendable
arguments.

### 3.8 At `defun` boundaries and across modules

A `defun` boundary is the only place types are generalised and the
only place ownership summaries (§6.4) are attached. Inside a body,
everything is monomorphic. Exported signatures are complete (all
bounds, all flag parameters, all `&` positions, all conventions); an
importing module instantiates them exactly as a local call would. No
inference state crosses a module boundary, so modules can be checked
independently once their dependencies are checked.

### 3.9 Error positions

Each unification failure names the form whose rule generated the
failing equation; because rules are applied in evaluation order, the
reported position is the *first* form that cannot be typed.

---

## 4. Protocol dispatch

**Proposed**: both static and dynamic, chosen by the type at the call
site after inference.

### 4.1 Static (default)

Every generic `defun` (one with quantified variables or bounds) is
*monomorphised*: each distinct instantiation reachable from `main`
becomes its own lIR function with concrete types. Inside a
monomorphised body every protocol constraint is resolved to a concrete
instance, and a method call `(m e ...)` compiles to a direct call of
that instance's method. There is no runtime type tag on ordinary
objects. Choose static whenever the receiver's type at the call site is
concrete, which is always the case except for `(dyn P)`.

Monomorphisation is also what makes the ownership rules type-directed:
in a monomorphised body the compiler knows for every value whether it
is a scalar (no count) or an object (counted), so retain/release is
never emitted for scalars and never omitted for objects.

### 4.2 Dynamic

`(dyn P e)` packs `e` with the vtable of `(P, head(type(e)))`. A method
call on a `(dyn P)` receiver loads the method from the vtable. This is
chosen only when the programmer wrote `dyn`. Heterogeneous collections
are `(Vec (dyn P))`.

### 4.3 Cost and scope

Monomorphisation needs the bodies of generic functions from other
modules, so the compiler is whole-program (`syntax.md` §6). The
reference interpreter needs no monomorphisation: it dispatches on the
runtime type of `self` (it keeps the type tag beside every object,
`fibref` `heap/value.rs`), which must give the same results.

---

## 5. Sendability (§7)

### 5.1 The predicate

**Decided** (§7): only immutable objects and atoms may cross a thread;
a non-atom cell may not. Everything reachable from a crossing value
crosses with it. Hence the type-level predicate

```
Send(T)  =  "no value of type T can reach a Cell that is not an Atom"
```

defined structurally:

| T | Send(T) |
|---|---|
| any scalar, `str`, `ptr` | yes |
| `(Cell T)` | **no** |
| `(Atom T)` | yes (well-formedness already required `Send T`) |
| `(Weak T)` | `Send T` (an upgrade yields a `T`) |
| `(Vec T)`, `(Array T)`, `(List T)`, `(Set T)` | `Send T` |
| `(Map K V)` | `Send K ∧ Send V` |
| struct `(N A...)` | `Send` of every field type after substitution |
| sum `(N A...)` | `Send` of every payload type |
| `(Fn (A...) R φ)` | `φ = send` (§5.5) |
| `(Task T)`, `(Future T)` | `Send T` (a Future's frame may hold non-sendable locals; **Proposed**: `Future` is *not* sendable, `Task` is) |
| `(dyn P)` | yes iff `P` is declared `(defprotocol P :send ...)`, which requires every `impl` type to be sendable |
| type variable `a` | the bound `(Send a)` (quantified) |

Recursive nominal types: `Send` is computed as a greatest fixed point
over the definitions (assume yes, refute on finding a `Cell`), once per
definition, so a struct with a `(Cell ...)` field anywhere in its
transitive structure is not sendable.

### 5.2 Where it is required

`(spawn f)`: `Send` of `f`'s type (i.e. `φ = send`) and of the result;
`(atom e)`: `Send` of `e`'s type; `plet`/`pmap` inherit these through
`spawn`. Nothing else. `async` does not require it (`syntax.md` §2.15).

### 5.3 The witness

When `Send(T)` fails, the solver records the path to the offending
`Cell`: the first field/element/capture chain that reaches it. The
error text is

```
cell cannot be shared between threads: <path> has type (Cell T)
```

where `<path>` is e.g. `closure capture n` (case 13) or
`field children of Node`. The first clause is the canonical text of
case 13.

### 5.4 Marking shared

**Decided** (§7): at `spawn`, the runtime marks `f` and everything
reachable from it shared (§8.1) before the thread starts; at `join`,
the result is already shared (it was produced on the other thread by a
value that was marked, or freshly allocated there; **Proposed**: the
spawned thread marks its result before completing). The mark is
monotone and the walk stops at objects already marked, since everything
reachable from a shared object is shared (invariant maintained by every
store: storing an unshared object into a shared one marks the stored
object; §8.3).

### 5.5 Closure flags

A closure type `(Fn (A...) R φ)` has a flag `φ ∈ {send, local}` or a
flag variable. The rule at a `fn` literal with captures `c1..cn`:

```
φ ≤ Send(type(c1)) ∧ ... ∧ Send(type(cn))     (send > local)
```

i.e. the closure is `send` iff every capture is sendable. A closure
with no captures is `send`. A `defun` used as a value is `send`.

### 5.6 Solving flags

After a component's types are resolved, flags are solved by
propagation: every flag starts at `send`; for each closure literal
whose capture set contains a type whose `Send` is `no`, or contains a
closure type whose flag is `local`, set the flag to `local`; iterate to
a fixed point (monotone, terminates). Flags that depend on a quantified
type variable's `Send` bound or on a flag variable stay symbolic and
are quantified: e.g. `(defun twice (f) (fn (x) (f (f x))))` gets
`(Fn (a) a φ) -> (Fn (a) a φ)`. A `spawn` on a symbolic flag turns it
into the bound `φ = send` on the signature. A `spawn` on a `local` flag
is the error of §5.3, whose witness is the capture that made it local.

Annotations: `(Fn (A) R)` in a struct field or parameter annotation
without a flag means `:local` (accepts any closure; the containing
struct is then not sendable); write `(Fn (A) R :send)` to require
sendability. (**Proposed**; §10.)

### 5.7 Atoms of cells

`(atom (cell 0))` is a type error (`Send (Cell i64)` fails at `atom`),
which is how "a cell that is not an atom may not cross" is enforced
without examining atom contents at spawn time. A struct with a `Cell`
field cannot be put in an atom either; use nested atoms or an immutable
struct replaced wholesale (case 16).

---

## 6. How the checker decides each ownership rule

The ownership pass runs per `defun`, after inference and
monomorphisation, on a body in which every expression has a concrete
type. It uses no information beyond: the types, the syntax tree, the
capture lists of closures, and the *summaries* of callees (§6.4). It
computes, and can print (§9), the *mode* of every object-typed
expression, the *escape* status of every binding and closure literal,
and the retain/release operations to emit.

Scalars are ignored throughout: a rule that says "retain" is a no-op
on a scalar-typed expression. This is the one place the type system is
load-bearing for correctness of the counting: an object is never
mistaken for a scalar because the type says which it is.

### 6.1 Modes

Every object-typed expression has one of three modes:

| Mode | Meaning |
|---|---|
| `Owned` | the expression's value carries a count that belongs to this expression; whoever consumes it must release it or hand it on |
| `Borrowed(b)` | the value *is* the value of binding `b` (a parameter, `let` binding, pattern variable or capture), with no count of its own; valid while `b` is |
| `Derived(b)` | the value is a part of `b`'s value (reached through fields, elements or pattern variables); valid while `b` is |

`b` is a *binding*, never an object: the checker reasons about which
binding keeps the value alive, not about aliases. Two bindings holding
the same object are two independent counts (interpreter) or two
independent reasons the object is alive (compiler); no rule ever needs
to know they are the same object.

### 6.2 Mode of each form

| Form | Mode |
|---|---|
| object literal, `[...]`, `{...}`, struct/variant constructor | `Owned` |
| call of a `defun`, method, primitive returning an object | `Owned` (**Decided** §4: results are owned) |
| call of a closure value | `Owned` |
| `@c` (cell or atom) | `Owned` (§6.7) |
| `(cell e)`, `(atom e)`, `(weak e)`, `(fn ...)`, `(async ...)`, `(spawn ...)`, `(join ...)` | `Owned` |
| variable `x` | `Borrowed(x)` |
| `(. e f)` | `Derived(b)` if `e` is `Borrowed(b)`/`Derived(b)`; `Derived(tmp)` if `e` is `Owned`, where `tmp` is an implicit binding for the temporary (§6.3) |
| pattern variable `p` bound by `match`/`let` on scrutinee of mode `m` | binding `p` is a *derived binding* of the scrutinee's binding (or of the scrutinee temporary); reading `p` is `Derived(that)` |
| `(let ... body)` | mode of `body`, adjusted at scope exit (§6.3) |
| `(do ... e)` | mode of `e` |
| `(if c t e)`, `(match ...)` | *join*: if every branch is `Borrowed(b)` for the same `b`, `Borrowed(b)`; if every branch is `Derived`/`Borrowed` of the same `b`, `Derived(b)`; otherwise `Owned`, and each branch that is not `Owned` gets a retain at its tail (**Decided** by case 04) |
| `(unsafe body)` | mode of `body` |
| `(set! ...)`, `(reset! ...)`: `unit` | — |
| `(swap! a f)`, `(reset! a e)` | `Owned` (a retained reference to the new value) |
| `(upgrade w)` | `Owned` |

### 6.3 Where counts change: the escape positions (§3)

**Decided** (§3): a reference escapes when it is (1) returned, (2)
stored into an object, (3) captured by an escaping closure, (4) passed
to another thread, (5) held across an `await`. Each is a *position* in
the syntax. The rule at every escape position is the same:

```
consume(e):   Owned          -> move   (no operation; the count travels with the value)
              Borrowed(b)    -> retain
              Derived(b)     -> retain
```

and the positions are:

| # | Position | Forms |
|---|---|---|
| E1 | return | the body of a `defun`; the body of a `fn`; the body of an `async` |
| E2 | store | arguments of struct and variant constructors; elements of `[...]`, `{...}`; `(cell e)`, `(atom e)`, `(set! c e)`, `(reset! a e)`; the result of `f` in `(swap! a f)`; `(array-set a i e)` and every library function that stores, *through its own body* (§6.4: stores inside `conj` are `conj`'s business) |
| E3 | capture | each free variable of a `fn` literal that is *escaping* (§6.5) and of every `async` form; the retain happens when the closure/future object is created |
| E4 | thread | the argument of `spawn` (a closure; E3 already retained its captures; `spawn` retains the closure object itself and marks shared) |
| E5 | await | subsumed by E3: the `async` frame owns everything it references, so nothing is borrowed across an `await` (§6.9) |

Everything else is *not* an escape and gets no count operation:

| Position | Rule |
|---|---|
| argument to a `borrow`-convention parameter | none (**Decided** §4: parameters are borrows). If the argument is `Owned` (a temporary), it is released after the call returns |
| argument to an `own`-convention parameter (§6.4) | `consume` |
| `let` binding `(x e)` | `e` `Owned`: `x` owns it, released at scope exit unless moved out. `e` `Borrowed(b)`/`Derived(b)`: `x` is an *alias binding* of `b`, no count; reading `x` is `Borrowed(b)`/`Derived(b)` |
| scope exit of `(let ((x e)) body)` | if `body` is `Borrowed(x)` and `x` owns: *move out* — `x` is not released, result becomes `Owned` (no operation). If `body` is `Derived(x)` and `x` owns: retain result, release `x`. Otherwise: release every owning binding of the scope; result mode unchanged |
| scrutinee temporary of `match`/`if` | if `Owned`: bound to an implicit binding `tmp` for the duration of the form; same exit rule as `let` |
| non-final `do` form | if `Owned`: release immediately |
| `defun` body result (E1) | `consume`; then release every owning local in reverse scope order; `own` parameters are released (unless moved out) |
| `&` copy-in at a call `(g &x)` | `x` a cell: `@x` (an `Owned` acquire, §6.7) is the callee's initial value. `x` an `&`-param: *move* if `x` occurs in no other argument of the same call and in no closure literal of the function; else retain (§6.6) |
| `&` write-back after the call | `x` a cell: `(set! x v)` with `v` `Owned` (moved), releasing the old value. `x` an `&`-param: assign the slot; if the copy-in was a retain, release the old slot value |
| `(set! v e)` on an `&`-param slot | `consume(e)`, then release the old slot value |

This table *is* the escape classification: the checker walks the tree
once, labels each object-typed expression with its position, and emits
the operation from the two tables. Nothing else emits a count
operation.

### 6.4 Borrowed parameters, owned results, and summaries (§4)

**Decided** (§4): every parameter is a borrow; every result is owned;
returning a parameter or something reachable from one is an escape
(E1) so the callee retains.

The pass attaches to every `defun` a summary, part of its interface:

- **Escape summary** per parameter `p`: `escapes` if `p` (as
  `Borrowed(p)` or `Derived(p)`) reaches any escape position E1–E4 in
  the body, or is passed to a parameter of a callee whose summary is
  `escapes`, or is passed to a closure value (unknown callee: assumed
  `escapes`), or is captured by an escaping closure. Otherwise
  `noescape`. Computed as a least fixed point over the module's call
  graph starting from `noescape` (**Proposed**; monotone, so the
  fixpoint exists and is the least solution of the equations, which is
  exactly "may escape").
- **Convention** per parameter: `own` if some `defun` in the same
  call-graph component (including the function itself) makes a *tail
  call* to this function with a non-variable expression at that
  position; otherwise `borrow` (**Proposed**; §10). An `own` parameter
  is released by the callee at exit unless moved out; callers apply
  `consume` to the argument. This is what keeps case 07 a tail call:
  `(build (- n 1) (conj acc n))` passes the `Owned` temporary by move,
  and `build` releases its old `acc` before the tail call.
- Closure parameters are always `borrow` (a caller cannot know the
  callee), so a closure that stores a parameter retains it (E2) like a
  `defun` would.

What the summaries buy (**Decided** §2, "guaranteed minimum"): a
binding whose value is `Owned` at creation and whose every use is a
non-escape position with a `noescape` callee is *scope-local*: it is
freed unconditionally at scope exit, and the compiler may allocate it
on the stack. A binding with any other use is heap-allocated with
count 1 and *released* (decrement, free at zero) at scope exit. In both
cases the binding itself performs no retain; the difference is only
whether the object needs a count word that a callee may have
incremented. This is "borrow first, count second" made concrete:
counting exists for objects that some callee may have kept.

Case 01: `nth`'s body returns `Derived(xs)` — E1, retain. `head`
returns `Owned` — nothing. `main`'s `h` owns one count, `l` owns the
vector which owns the other; both released at scope exit; clean.
Case 04: `pick`'s `if` joins `Borrowed(x)` with `Owned`: retain `x`
in that branch; the result is `Owned` on both paths.

### 6.5 Closure capture and the `&` rules (§5)

**Escaping closure**: a `fn` literal is *escaping* unless every use of
it is one of: (a) called directly at the literal; (b) passed as an
argument to a `noescape` parameter of a `defun`, method or primitive
(`swap!`, `for-each`, `map`, `reduce`...); (c) bound by `let` to a
variable whose every use is (a) or (b), and which is not itself
captured by any other closure. A closure passed to a closure-typed
parameter, returned, stored, spawned, or captured is escaping. This is
a syntactic check using only summaries.

- Escaping closure: E3 — every captured object binding is retained at
  creation; the closure object owns those counts and releases them
  when freed. Captured cells are retained like any object (case 05:
  both closures retain `n`, the `let` releases its own count, the cell
  lives while either closure does).
- Non-escaping closure: captures are `Borrowed(b)` of the enclosing
  bindings; no count. Valid because the closure cannot outlive the
  scope that created it: uses (a)–(c) all complete before the scope
  exits.

**Decided** (§5, case 18): an `&` parameter may not be captured by an
escaping closure. Check: for every escaping `fn` literal, if its
capture list contains an `&` parameter of the enclosing `defun`, error
`& parameter captured by escaping closure: v in make-pusher`. A
non-escaping closure may capture an `&` parameter and use `&v` or
`(set! v ...)` inside it (case 08).

**Decided** (§5, case 12): the `&` arguments of one call name
distinct variables. Check: for every call, the list of `&x` argument
names has no duplicates; error
`variable x passed to more than one & parameter in call to bar`.
Purely syntactic; aliasing of *objects* is fine because both callee
parameters start from a copy-in that leaves the count above one, so
each update copies (§5).

### 6.6 In-out parameters in detail (§5)

The callee's `&v` is a *slot* owning one count of whatever it holds.
`(set! v e)` consumes `e` into the slot and releases the old value.
`(g &v)` from inside the callee is copy-in/copy-out again on the slot.

Copy-in at the caller (**Proposed** refinement of §5): the caller's
count on `x`'s value must be either *retained* (interpreter: always)
or *moved* into the callee's slot (compiler, when provably safe). The
move is safe iff nothing can read `x` during the call: `x` does not
occur in another argument of the same call (case 17 reads `@x` in the
same call, but a cell copy-in is always `@x`, an acquire, so cells are
never moved: the cell keeps its own count), and `x` is not captured by
any closure literal in the function (a non-escaping closure could be
passed to the callee and read the slot). Both conditions are syntactic.
Under a move, the write-back is a plain store with no release (the old
value's count travelled into the callee and came back as the new
value, or was released there). Results and audits are identical to the
retain version; only in-place update of unique values (§5) becomes
possible.

Borrows during a call: an argument `Borrowed(x)` where `x` is an
`&`-param is safe without a retain unless the slot can be reassigned
during the call — which can only happen through a closure that
captures `x` and is called by the callee (case 08's
`(for-each v (fn (x) (append &v x)))`). Rule: if `x` is an `&`-param
that appears as `&x` or as a `set!` target inside any closure literal
of the function, then every plain `x` argument in the function is
retained for the duration of the call and released after. Otherwise no
operation. (The interpreter counts every parameter binding, so it
needs no rule; this is the compiler's proof obligation, discharged
syntactically.)

Write-back and audit for case 08 (compiler, with the rules above): the
caller's cell holds `V0` (count 1); copy-in `@v` retains (2); inside
`dup-all`, `v` is captured and written in a closure, so `for-each`'s
borrow of `v` is retained for the call (3); iteration 1: the closure's
`(append &v x)` retains `V0` on copy-in (4; `v` is captured, so never
moved), `append` does `(set! v (conj v x))`: `conj` allocates `V1`,
`set!` releases `V0` (3); write-back into `dup-all`'s slot stores `V1`
and releases `V0` (2: the caller's cell and the for-each borrow);
iterations 2 and 3 free `V1` and `V2` the same way; `for-each` returns
and releases its borrow (1); `dup-all` returns; write-back
`(set! cell V3)` releases `V0` (0, freed). `(count @v)` = 6. Nothing
live at exit.

### 6.7 Cells and weak references (§6)

- `(cell e)`: E2 store of `e`; the cell owns one count of its value.
- `@c`: **Proposed** uniform rule: an *acquire* — the result is
  `Owned`, i.e. the reader takes +1 (for atoms this is **Decided** §7;
  for cells it is the same rule so that `(let ((x @c)) (set! c y) x)`
  can never read freed memory). The compiler may elide the retain and
  the matching release when the acquired value's uses are all
  arguments of calls with `noescape` parameters, none of which is
  passed a closure or an `&` argument, and no `set!`/`reset!`/`swap!`
  occurs between the acquire and the last use (a straight-line
  syntactic condition). Otherwise the pair stays.
- `(set! c e)`: E2 store of `e`, then release the old value. On an
  `(Atom T)` this is `reset!`; `set!` on an atom is a type error.
- Cycles: **Decided** (§6) a cycle through cells is garbage the
  implementation need not reclaim. The checker does nothing about
  cycles; the audit classifies them (`fibref` `heap/audit.rs`). Case
  15: `k` (count 1 from `let`) → cell → `[k]` (E2: `k` retained, count
  2) → `k`; `let` exit releases `k` (1); nothing reaches 0; the audit
  finds the strongly connected component through the cell and reports
  `leak-cycle`, and nothing else.
- `(weak e)`: not an escape; produces an `Owned` `(Weak T)` value
  which holds a *weak* count on the control block (§8.5), never on the
  object. `(upgrade w)` is `Owned` `(Option T)`: a strong count if the
  object is alive, `nil` otherwise. Case 20: `v` is released at the
  inner `let` (count 0, freed, control block marked dead); `upgrade`
  returns `nil`. Case 19: the only strong edges are root → cell → `[a]`
  → cell → `[b]`; parents are weak; `main`'s `let`s release `b`, `a`,
  `root` in reverse order, each freeing what only it holds; clean.

### 6.8 Threads (§7)

Decided by `Send` (§5) at the type level and by E4 at the ownership
level:

- `(spawn f)`: type rule requires `Send` of `f`'s captures (through the
  flag) and of the result; ownership: `consume(f)` (retain if borrowed)
  and mark shared; the thread owns that count and releases it when the
  thunk returns. `join` transfers the result's count to the caller.
- Case 13: the closure captures `n : (Cell i64)`, so its flag is
  `local` with witness `n`; `pmap`'s parameter requires `:send`;
  error `cell cannot be shared between threads: closure capture n has
  type (Cell i64)`.
- Case 10: `a : (Atom (Vec i64))` is sendable; each `@a` acquires;
  `swap!` stores the closure result (E2, moved) and releases the old
  vector, which the reader's `snapshot` still holds until its `let`
  exits. Case 16 is the same with a struct.
- Atomic counts: objects marked shared use atomic increments and
  decrements (§8.1); the mark is set by the E4 walk and by stores into
  already-shared objects (§8.3).

### 6.9 Async (§8)

**Decided** (§8): borrows cannot be held across an `await`; anything
used after an `await` is retained on entry. Mechanism: an `async` form
is an escaping closure (E3 for every free variable) whose body runs
inside a heap-allocated frame owned by the `Future`. All locals of the
body live in that frame; nothing in the body is `Borrowed` of an
enclosing binding, only of the frame's own captures, which the frame
owns. There is therefore no rule to check at `await` itself. The
compiler may skip retaining a capture whose every use precedes the
first `await` on every path (**Proposed** optimisation; the
interpreter retains all).

**Decided** (§8): `&` parameters are not allowed in async functions.
Check: a `defun` that has an `&` parameter and contains an `async`
form anywhere in its body (including inside nested `fn`s) is rejected:
`& parameter in async function: buf in fill` (case 14). This check
runs before the closure-capture check of §6.5, so case 14 reports
this text and not case 18's.

Case 11: `measure` captures `s` (retain), the caller's `let` releases
`s` (count 1, held by the frame); `block-on` drives the future;
`(length s)` reads the frame's capture; the future is freed after
`block-on` returns its value, releasing `s`; clean.

### 6.10 Unsafe (§9)

`unsafe` changes nothing above. `(raw e)` is `Derived(b)` of `e`'s
binding for the purposes of validity but has scalar type `ptr`, so it
is never counted; the programmer must not let it outlive `b`
(**Decided** §9: foreign code gets borrows for the duration of the
call). `:retains` on an `extern` makes the argument position an E2
store.

---

## 7. Case table

For each case: the rule that decides it, the mechanism in this
document that produces the verdict, and for reject cases the exact
error text. The quoted fragment in each error is the case header's
`error:` text, adopted as the canonical message; the rest of the line
is the position/witness the checker appends.

| Case | Rule | Mechanism | Verdict / error text |
|---|---|---|---|
| 01 return-part-of-argument | §4 | `nth` body is `Derived(xs)` at E1 → retain; `head` result `Owned`; `h` and `l` released at scope exit | accept, 1, clean |
| 02 structural-sharing | §5 | `conj` retains the shared nodes it stores (E2 inside its body); `make`'s `let` releases `v`, nodes stay alive via `w` | accept, 7, clean |
| 03 store-borrowed-value | §3.2 | `item` reaches E2 inside `conj` → `remember`'s summary `escapes`; `s` is retained twice; `b` released at scope exit, `keep` at `main`'s | accept, 5, clean |
| 04 branch-dependent-owner | §4 | `if` join: `Borrowed(x)` vs `Owned` → retain in the `x` branch; result `Owned` on both paths | accept, 5, clean |
| 05 closures-share-state | §6 | both `fn`s are escaping (stored in `[...]`, E2) → E3 retains `n` twice; `set!` on captured cell allowed; `let` releases its count | accept, 2, clean |
| 06 capture-borrowed-param | §3.3 | `fn` is escaping (E1) → E3 retains `prefix`; `p`'s `let` releases; closure owns the string | accept, 1, clean |
| 07 recursive-accumulator | §4, §5 | `acc` gets convention `own` (self tail call with a temporary); `(conj acc n)` is `Owned`, moved; old `acc` released before the tail call; `if` branch `acc` moved out at E1 | accept, 100000, clean |
| 08 mutate-while-iterating | §5 | closure is non-escaping (`for-each`'s `f` is `noescape`) so capturing `&v` is allowed; `v` written in a closure → `for-each`'s borrow of `v` retained for the call; each `append` copies (count > 1) | accept, 6, clean |
| 09 iterator-outlives-source | §3.1 | `iter` stores `v` (E2, retain); `evens` returns `Owned`; `v`'s `let` releases | accept, 2, clean |
| 10 atom-old-value | §7 | `Send (Atom (Vec i64))` holds; `@a` acquires (`Owned`); `swap!` releases the old value after the store; readers' counts keep old vectors | accept, 1000, clean |
| 11 borrow-across-await | §8 | `async` is E3 for `s` (retained at creation); frame owns it; `s`'s `let` releases | accept, 5, clean |
| 12 reject-same-binding-twice-inout | §5 | §6.5 syntactic distinct-names check on `(bar &x &x)` | reject: `variable x passed to more than one & parameter in call to bar` |
| 13 reject-cell-crosses-thread | §7 | closure flag `local` (capture `n : (Cell i64)`); `pmap` requires `:send`; §5.3 witness | reject: `cell cannot be shared between threads: closure capture n has type (Cell i64)` |
| 14 reject-inout-in-async | §8 | §6.9 syntactic check: `&buf` and an `async` form in `fill` | reject: `& parameter in async function: buf in fill` |
| 15 cycle-through-cell-leaks | §6 | E2 retains `k` into `[k]`; `let` releases one count; SCC through the cell → audit `leak-cycle` | accept, 1, leak-cycle |
| 16 coordinated-update-single-atom | §7 | `Send Accounts` (scalar fields) holds; `swap!` stores the closure's `Owned` struct and releases the old; `snap`/`final` acquire and release | accept, 200, clean |
| 17 inout-and-borrow-same-call | §5 | cell copy-in is an acquire (+1), `@v` argument is another acquire; both released after the call; write-back stores the new vector and releases the old | accept, 4, clean |
| 18 reject-inout-captured-by-escaping-closure | §5 | `fn` is escaping (E1); its capture list contains `&`-param `v` → §6.5 | reject: `& parameter captured by escaping closure: v in make-pusher` |
| 19 weak-parent-pointer | §6 | `weak` is not an escape; `conj` and `set!` on the children cell are E2; strong graph is a tree; `let`s release in reverse | accept, 2, clean |
| 20 weak-ref-to-dead-object | §6 | inner `let` releases `v` (freed); `upgrade` finds the control block dead → `nil` | accept, 1, clean |

Cases 12–14 and 18 are decided by *syntactic* checks (§6.5, §6.9) or
by a *type* constraint (`Send`, §5); none needs the mode analysis.
Cases 01–11, 15–17, 19–20 are decided by the mode tables of §6.2–6.3
plus summaries; the audit verdict follows from applying those tables,
which the reference interpreter checks by running the plain counting
semantics.

---

## 8. Mapping to lIR

lIR is an S-expression assembler for LLVM IR, 1:1, with no knowledge
of fibber (`/home/user/liar/doc/lIR.md`). Everything below is emitted
by the fibber compiler as ordinary lIR: structs, calls, loads, stores,
atomics. **Proposed** throughout except where a layout is forced by a
Decided rule.

### 8.1 Object header and the count

Every object begins with the same header:

```
(defstruct fib.hdr (i64 ptr))        ; word 0: count word, word 1: type descriptor
```

Count word layout: bit 0 = `shared` (§7), bit 1 = `has-weak` (§8.5),
bits 2–63 = the count. An object is created with count 1 (the creating
expression's `Owned` reference). Static objects (string literals,
`defun`s as closure values, payload-free variants) have count word
`-1`: retain/release test for it and do nothing.

```
retain(p):  if p == null return                     ; Option nil
            c = load i64 p
            if c == -1 return
            if c & 1: atomicrmw add p 4 (seq_cst)   ; shared: atomic
            else:     store (c + 4) p               ; unshared: plain

release(p): if p == null or load == -1 return
            if shared: old = atomicrmw sub p 4 (acq_rel); if old >> 2 == 1 then drop(p)
            else:      c = load; if c >> 2 == 1 then drop(p) else store (c - 4) p

drop(p):    desc = load ptr (p + 8); call desc.drop p   ; releases each object field per the descriptor
            if has-weak: weak-table-kill(p)              ; §8.5
            call @free p
```

The compiler emits `retain`/`release` exactly at the points §6.3
prescribes, as calls to these runtime functions (or inlined). A
per-type `drop` is generated from the type's layout: for each
object-typed field, `release`. Scalars in fields are ignored. This is
where the type system pays off: `drop` needs no runtime tags because
the descriptor is per monomorphised type.

Mark shared (E4): `share(p)`: if null, static, or already shared,
return; set bit 0; for each object field (via descriptor) `share`. For
an `Atom`, lock it, `share` its content, unlock.

### 8.2 Scalars

`bool` → `i1`; `i8..i64` → the same; `f32`/`f64` → `float`/`double`;
`char` → `i32`; `unit` → erased (`void` return, no parameter). No
count, no header.

### 8.3 Structs, sum types, strings, arrays, collections

- Struct `(N ...)`: `(defstruct N.<inst> (fib.hdr field-types...))`,
  object fields as `ptr`, scalars inline. The constructor allocates,
  writes the header (count 1, descriptor), stores `consume`d fields.
- Sum type: `(fib.hdr i32 payload...)` with the largest variant's
  payload; tag in the `i32`. Payload-free-only sums are an `i64` tag
  with no allocation. `(Option T)` with `T` an object type is `ptr`
  with `nil` = null (§1.6); with `T` a scalar it is the boxed form.
- `str`: `(fib.hdr i64 [N x i8])`, byte length then UTF-8 bytes; no
  object fields, so `drop` is just `free`.
- `(Array T)`: `(fib.hdr i64 [N x T])`. `array-set` copies unless the
  count is exactly 1 and unshared, in which case it writes in place
  (the "unique updates happen in place" rule, §2, applied by the
  primitive itself — the caller passes the array with `consume`).
- `Vec`, `Map`, `Set`, `List`: library structs over `Array` (32-way
  trie, HAMT, cons cells), as in liar's `lib/`, but typed and without
  `heap-array`/`aget`.
- Stores into a shared object (`set!` on a shared cell, `reset!`/
  `swap!` on an atom): after `consume`, `share` the stored value so the
  invariant of §5.4 holds.

### 8.4 Cells and atoms

```
(defstruct fib.cell (fib.hdr ptr))            ; value slot (ptr or inline scalar)
(defstruct fib.atom (fib.hdr i32 ptr))        ; lock word, value slot
```

`@cell`: load the slot, `retain`, return. `set!`: `consume` new,
store, `release` old. `@atom`: lock (spin on the `i32` with
`cmpxchg`), load, `retain`, unlock — one atomic step relative to a
writer's release (**Decided** obligation, §7). `swap! a f`: loop
{ lock; old = load; retain old; unlock; new = f(old);
lock; if load == old { store new; unlock; release old (the atom's);
release old (ours); return new (retained for the caller) }
else { unlock; release old } }. `reset!`: lock; old = load; store
`consume`d new; unlock; release old. A scalar-typed cell/atom holds
the scalar inline and does no counting.

### 8.5 Weak references

Only objects that ever had a weak reference pay (**Decided** §6):
`(weak e)` sets `has-weak` in `e`'s header and inserts (or finds) a
control block in a global table keyed by address, guarded by a mutex:

```
(defstruct fib.weak (i32 i64 ptr))            ; lock, weak count, object or null
```

A `(Weak T)` value is a pointer to the block; retain/release of a
`Weak` value adjust the block's weak count (the block is freed when it
is zero and the object pointer is null). `drop` of an object with
`has-weak` locks its block, nulls the object pointer, and removes the
table entry before `free`. `upgrade`: lock block; if object null →
`nil`; else `retain` object; unlock; return. Because `drop` nulls the
block under the same lock before freeing, `upgrade` never retains a
freed object.

### 8.6 Closures and protocol dispatch

```
(defstruct fib.closure (fib.hdr ptr captures...))    ; code pointer then captures
```

A closure is called as `(call code env args...)` with `env` the
closure object itself; the body loads captures from `env`. A `defun`
used as a value is a static closure with no captures whose code is a
trampoline to the `defun`. Escaping closures' captures are stored
`consume`d (E3) and released by the closure's `drop`. Non-escaping
closures may be stack-allocated (`alloca`) with borrowed captures,
since §6.5 proves they do not outlive the scope.

`(dyn P)`: a two-word value `{ptr obj, ptr vtable}` passed and stored
as an lIR struct; the vtable is a constant global of code pointers in
the protocol's method order. Retain/release act on `obj`. Static
dispatch is a direct `call` to the monomorphised method.

### 8.7 Futures and tasks

`(Future T)`: `(fib.hdr ptr i32 ptr captures... locals...)` — poll
function, state, result slot, then the frame. `async` lowers to a
state machine over `await` points (one `switch` on the state at entry,
as liar's runtime expects a `poll` function). `(Task T)`: a runtime
handle `{fib.hdr, thread, result, done}`; `spawn` calls the runtime
with the closure, `join` blocks and moves the result out.

### 8.8 Calling convention

`defun` parameters: `borrow` → passed as `ptr`, no count operation
either side; `own` → caller `consume`s, callee releases at exit unless
moved out. `&` parameters: passed as `ptr` to a stack slot holding the
value; the callee reads and writes the slot; write-back is the caller
reading the slot after the call. Results: returned as `ptr` with the
count the callee arranged (E1). `unit` results are `void`.

### 8.9 Tail calls

A call in tail position whose arguments carry no `Owned` temporaries
into `borrow` parameters and after which no release is pending is
emitted as lIR `tailcall` (which must be `musttail` in the hardened
lIR; `lir-audit` notes it is currently `tail`). The `own` convention
(§6.4) is what makes accumulator recursion satisfy this. A tail call
that does not satisfy it is an ordinary call followed by the pending
releases; the reference interpreter runs with an explicit stack so
depth is never a correctness question there.

### 8.10 What the compiler must emit, summarised

| Event | Emission |
|---|---|
| object literal / constructor | `malloc`, header init (count 1, descriptor), `consume` each field |
| E1 return of `Borrowed`/`Derived` | `retain` before `ret` |
| E2 store of `Borrowed`/`Derived` | `retain` before the store |
| E3 capture | `retain` each object capture at closure/future creation |
| E4 spawn | `consume` closure, `share` walk, hand to runtime |
| `own` argument | `consume` (retain if not `Owned`) |
| `Owned` temporary as `borrow` argument | `release` after the call returns |
| scope exit | `release` each owning binding not moved out, in reverse order |
| `Derived(x)` result at scope exit | `retain` result, then `release` bindings |
| `set!`/`reset!` | `consume` new, store, `release` old |
| `@` acquire | `retain` (atom: under lock) |
| `swap!` | as §8.4 |
| `&` cell copy-in / write-back | `@x` acquire; after call `set!` with the slot's value (moved) |
| `&`-param borrow during a closure-writing call (§6.6) | `retain` before, `release` after |
| `drop` per type | `release` each object field; weak-table kill if flagged; `free` |

---

## 9. What the checker prints

**Proposed**: `fibc --explain file.fib` prints, per `defun`, the
complete output of §3 and §6 so that a verdict can be checked by eye
and diffed against the interpreter's trace:

```
defun dup-all : (& (Vec i64)) -> unit
  params:   v  &param  convention=slot  escapes=no  written-in-closure=yes
  closure @8:15  escaping=no  reason=arg-to-noescape(for-each.f)
            captures: v (&param, by reference)
  calls:    for-each(v, <closure>)  v: retain-for-call (§6.6)
  ops:      L8 retain v; L8 call for-each; L8 release v
```

Every line is one of: a parameter summary (`convention`, `escapes`),
a closure literal (`escaping` with the clause of §6.5 that decided it,
captures with their mode), a binding (`owns`/`alias-of`/`scope-local`),
and the emitted operations with source lines. Reject cases print the
error and the rule number. The reference interpreter's trace
(`fibref` `heap/event.rs`) lists every retain/release/free with the
same line numbers, so the compiler's `ops` list and the interpreter's
trace must free the same objects (`spec/method.md`, rule 6).

---

## 10. Open decisions

Each needs the owner's sign-off. Recommendation first, alternative
second. Items that also appear in `syntax.md` §8 are cross-referenced.

1. **Inference: HM at `defun` boundaries only, no local
   generalisation (§3.2).** Recommend: yes; the ownership pass then
   sees one concrete type per local. Alternative: generalise `let`
   bindings (more programs type-check; closures become polymorphic
   and the escape pass must handle instantiation).
2. **Integer literals are `i64` unless suffixed (§1.1, §2.17).**
   Recommend: yes, one rule, no defaulting search. Alternative:
   literals polymorphic over integer types with `i64` defaulting
   (`IntLit` constraint) — friendlier for `i32` FFI code.
3. **Field access must resolve by the end of the `defun`; no
   unique-field-name lookup (§3.4).** Recommend: yes; annotate
   (case 19 `depth`). Alternative: resolve `(. x f)` to the unique
   struct in scope declaring `f`.
4. **Parameter conventions `borrow`/`own` inferred from tail calls
   (§6.4).** Recommend: yes; keeps §4's "parameters are borrowed" as
   the default and case 07 a real tail call. Alternatives: (a) all
   parameters owned (Perceus/Lean style: every call site retains, more
   in-place reuse, more count traffic); (b) no `own`: accumulator
   recursion is not a tail call and relies on stack depth.
5. **Escape summaries are a least fixed point over the call graph,
   assumed `escapes` for closure-typed callees (§6.4).** Recommend:
   yes. Alternative: per-parameter `noescape` annotations on closure
   types, so higher-order library functions can take closures without
   forcing their arguments to be counted.
6. **`&` copy-in moves when the variable is not otherwise mentioned in
   the call and not captured (§6.6).** Recommend: yes, it is the only
   route to in-place unique updates through `&`; the interpreter
   implements the same rule. Alternative: always retain (simplest;
   `&` updates always copy).
7. **`@cell` acquires, with syntactic elision (§6.7).** See
   `syntax.md` §8 item 2.
8. **Closure flags: default `:local` in annotations (§5.6).**
   Recommend: yes; a field without a flag accepts any closure.
   Alternative: default `:send`, so unannotated structs stay sendable
   and stateful callbacks need `:local` written out.
9. **`Future` is not sendable; `Task` is (§5.1).** Recommend: yes
   under the single-threaded executor. Alternative: a work-stealing
   executor with `async` requiring `Send` (then `Future` is sendable
   and `Future`'s locals must be too).
10. **Monomorphisation and whole-program compilation (§4).**
    Recommend: yes; every ownership decision is on concrete types.
    Alternative: dictionary passing with uniform boxed representation
    (separate compilation, runtime dispatch on every bound call,
    every generic value an object).
11. **Overlapping and variable-headed instances are rejected
    (§3.5).** Recommend: yes. Alternative: liar's
    `extend-protocol-default` as blanket instances with a
    specificity order.
12. **Header layout: 16 bytes, weak references through a side table
    (§8.1, §8.5).** Recommend: yes; objects without weak references
    pay one bit. Alternative: a third header word holding the control
    block pointer (faster `upgrade`, 8 bytes per object).
13. **`Option` of an object type is a nullable pointer (§1.6, §8.3).**
    Recommend: yes. Alternative: uniform boxed sums (simpler codegen,
    one allocation per `some`).
14. **Cycle collector.** §6 leaves it open; recommend: none until the
    audit's leak reports from real programs argue for it. Alternative:
    trial deletion over cells, as §6 sketches.
15. **`dosync`/`ref`.** Reserved, deferred (§7). Recommend: keep the
    word reserved and do nothing. Alternative: design `ref` now so the
    snapshot semantics can be tested in the interpreter.
16. **Static dispatch as the default and `dyn` as explicit (§4).**
    Recommend: yes. Alternative: implicit coercion to `(dyn P)` where a
    `(dyn P)` is expected (needs subtyping in unification).
