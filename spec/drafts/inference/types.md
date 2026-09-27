# fibber types (draft: inference-first)

Status: draft for sign-off. **Decided** marks a direct consequence of
[spec/ownership.md](../../ownership.md); **Proposed** marks everything
else. Surface forms are those of [syntax.md](syntax.md). Section
references of the form "ownership §n" are to ownership.md.

Design in one paragraph. Types are inferred by Hindley–Milner with
let-polymorphism at top-level definitions, extended with (a) protocol
constraints in the style of type classes, with one dispatch parameter
and functionally determined extra parameters; (b) a *colour*
qualifier on function types, `send` or `local`, solved as a two-point
lattice, which is how "what a closure captures" reaches thread
boundaries through higher-order code; (c) a small set of *site
coercions* (`T` to `(? T)`, `T` to `(dyn P)`) resolved after
unification. Ownership (ownership §3–§8) is not part of the type
system: it is a separate elaboration pass over typed, closure-converted
code that decides escapes per function using per-parameter *escape
summaries* exported alongside type schemes. Generic code is
monomorphised; protocol calls are static unless the receiver's type is
`(dyn P)`.

## 1. Type grammar

```
T ::= scalar | object | α                     ; α: type variable
scalar ::= i1 | i8 | i16 | i32 | i64 | f32 | f64 | bool | char | keyword | unit | ptr
object ::= str
         | (Name T*)                          ; nominal struct or enum, applied
         | (Vec T) | (Map T T) | (List T) | (Box T) | (Iter T)   ; library structs
         | (Cell T) | (Atom T) | (Weak T) | (Task T) | (Thread T) ; built-in
         | (? T)                              ; option (an enum; see §1.6)
         | (->κ (T*) T)                       ; function, colour κ
         | (-> (&T ...) T)                    ; defun with & params; not first-class
         | (dyn P) | (dyn (P T*))             ; protocol object
         | Syntax                             ; macro-time syntax (an enum)
κ ::= send | local | ς                        ; colour: constant or colour variable ς
```

Type schemes: `σ ::= ∀ᾱ ς̄. C ⇒ T` where `C` is a set of constraints
(§1.8). Type annotations in source never mention κ; `(-> (a) b)` in an
annotation means `(->ς (a) b)` with a fresh ς, and `(->send (a) b)`
may be written where the colour must be pinned (protocol signatures,
`extern` callbacks).

### 1.1 Scalars

Copied, never counted (ownership §1). `i64` is the type of integer
literals, `f64` of float literals; `bool` of `true`/`false`; `char` a
Unicode scalar value; `keyword` an interned id; `unit` the type of
`(do)`, `set!`, and functions called for effect (one value, `()`);
`ptr` a raw pointer, usable only inside `unsafe`. No implicit
conversion between any two scalars.

### 1.2 Objects

Everything else. `str` is immutable UTF-8. Nominal types come from
`defstruct` (product) and `defenum` (sum). Library collections are
ordinary nominal structs defined in `fibber.core` (their names are
not keywords of the type system). Objects are immutable unless the
type is `(Cell T)` or `(Atom T)` (ownership §1, §6, §7).

Predicates on types used by the rules (all structural, decidable by
one pass over the type after substitution; a type variable makes the
predicate a *constraint* on that variable, §1.8):

- `Object T`: `T` is not a scalar. `(weak x)` requires it.
- `Send T` (ownership §7): `T` may cross a thread. Defined in §5.4.
- `Immutable T`: `T` contains no `Cell` outside an `Atom`. Used only
  to define `Send`.

### 1.3 Nominal structs and enums

`(defstruct (Name ā) (x₁: T₁ ... xₙ: Tₙ))` introduces the type
constructor `Name` of arity |ā|, the constructor function
`Name : ∀ā. (->send (T₁ ... Tₙ) (Name ā))`, and field selectors used by
`(. e xᵢ)`. Unannotated fields are fresh parameters appended to ā
(syntax.md §3.2).

`(defenum (Name ā) (V₁ T₁*) ... (Vₖ Tₖ*))` introduces `Name` and
constructors `Vᵢ : ∀ā. (->send (Tᵢ*) (Name ā))`, or constants
`Vᵢ : ∀ā. (Name ā)` for payload-less variants.

Two nominal types are equal iff same constructor and equal arguments.
There is no subtyping between nominal types.

### 1.4 Generics and parametric polymorphism

Type parameters are implicit: any type variable free in a `defun`'s
inferred type after solving is generalised (§3.5). There is no syntax
to declare a defun's type parameters; annotations may mention type
variables by name (`x: (Vec a)`) and any lowercase symbol not naming
a type is a variable, scoped to that `defun`. Polymorphism is
rank-1 (prenex) only.

### 1.5 Function and closure types

`(->κ (T₁ ... Tₙ) R)`. Arity is fixed; no variadics (macros with
`...` are compile-time only). The colour κ (§5.4) says whether the
value may cross a thread: `send` if every capture is `Send`, else
`local`. Named `defun`s have colour `send` (they capture nothing).
The colour is the only difference between "function" and "closure"
types; there is one type constructor. Closure types do not record
what they capture.

A `defun` with `&` parameters has a *signature*, not a first-class
type: `(-> (&T₁ T₂ ...) R)`. It may be called with `&x` at those
positions and may not be referenced as a value.

### 1.6 `nil` and option

`(? T)` is the enum `(defenum (? a) (some a) nil)`. `nil : ∀a. (? a)`,
`some : ∀a. (->send (a) (? a))`. `nil? : ∀a. (->send ((? a)) bool)`,
`some?` likewise. There is no null pointer type; a field or variable
that may be absent has type `(? T)`. `(? (? T))` is a legal, distinct
type. Site coercion `T ↝ (? T)` (§3.6) is **Proposed**.

### 1.7 `cell`, `atom`, `weak`, `task`

Built-in type constructors with library-typed operations:

```
cell   : ∀a.            (->send (a) (Cell a))
atom   : ∀a. Send a ⇒   (->send (a) (Atom a))
weak   : ∀a. Object a ⇒ (->send (a) (Weak a))
deref  : ∀c t. Deref c t ⇒ (->send (c) t)        ; instances below
set!   : core form; place : (Cell a), value : a, result unit
swap!  : ∀a. (->send ((Atom a) (->send (a) a)) a)      ; returns the new value
reset! : ∀a. (->send ((Atom a) a) a)
spawn  : ∀a. Send a ⇒ (->send ((->send () a)) (Thread a))
join   : ∀a. (->send ((Thread a)) a)
async  : core form; body : a, result (Task a)
await  : core form; operand (Task a), result a, only inside async
block-on : ∀a. (->send ((Task a)) a)
pmap   : ∀a b. (Send a, Send b) ⇒ (->send ((->send (a) b) (Vec a)) (Vec b))
```

`Deref` is a protocol with one dispatch and one determined parameter
(§4.1): `(defprotocol (Deref c t) (deref [c] -> t))` with the built-in
instances `(Deref (Cell a) a)`, `(Deref (Atom a) a)`,
`(Deref (Weak a) (? a))`, and the library instance
`(Deref (? (Weak a)) (? a))`. `@e` reads `(deref e)`.

### 1.8 Constraints

```
C ::= P T₁ ... Tₙ             ; protocol constraint; T₁ is the dispatch position
    | Send T | Object T       ; built-in structural predicates (also written as protocols)
    | κ₁ ⊑ κ₂                 ; colour order: send ⊑ local
    | κ ⊒ Caps{T₁..Tₖ}        ; κ is at least as local as the captures demand (§5.4)
    | T₁ ↝ T₂                 ; site coercion (§3.6), never in a scheme
```

Schemes contain only the first four kinds. `Send` and `Object` are
ordinary protocols with built-in structural instances; they may be
written in annotations.

### 1.9 Protocol types

`(dyn P)` is an object type whose values are a pair (object pointer,
implementation table) for a single-parameter protocol `P`, or `(dyn (P
T*))` for a protocol whose determined parameters are fixed. A value of
type `(dyn P)` supports exactly `P`'s methods (and those of protocols
`P` `:requires`), dispatched at run time. `(dyn P)` is `Send` only if
declared `(dyn P :send)`, in which case only `Send` types may be
coerced into it. **Proposed.** Producing a `(dyn P)`: the site coercion
`T ↝ (dyn P)` when `T` implements `P` (§3.6), or `(as-dyn P e)`.

## 2. Typing rules

Judgement: `Γ ⊢ e : T | C` — under environment Γ (variables to types
or schemes, and the global tables of structs, enums, protocols,
instances), `e` has type `T` producing constraints `C`. The algorithm
(§3) generates `T` and `C` with fresh variables and solves; the rules
here are what it must be equivalent to. `inst(σ)` instantiates a
scheme with fresh variables, adding its constraints to `C`. `fresh`
is a fresh type variable; `ς` a fresh colour variable.

### 2.1 Expressions

| Form | Rule |
|---|---|
| integer literal | `i64`; `(i32 n)` etc.: the named width; float literal `f64` |
| `"s"` | `str`; `\c` : `char`; `:k` : `keyword`; `true false` : `bool` |
| `x` (local) | `Γ(x)` — monomorphic |
| `x` (global defun / constructor / variant / method) | `inst(Γ(x))` |
| `x` (global defun with `&` params) | error unless in call position: "function with & parameters is not a value" |
| `[e₁ … eₙ]` | each `eᵢ : a` (one shared `a`), result `(Vec a)`. `[]` : `(Vec fresh)` |
| `{k₁ v₁ …}` | all `kᵢ : k`, all `vᵢ : v`, `(Eq k, Hash k)`, result `(Map k v)` |
| `(f e₁ … eₙ)` | `f : (->κ (T₁ … Tₙ) R)`, `eᵢ : Tᵢ` (via `↝`, §3.6), result `R`. Arity must match exactly. If `f` is a `&`-signature defun: each `&` position must receive `&x` with `x` a place of type `Tᵢ` (§2.3), non-`&` positions must not receive `&x` |
| `(fn (x₁ … xₙ) b)` | Γ, xᵢ: aᵢ (fresh, or the annotation) ⊢ b : R; result `(->ς (a₁ … aₙ) R)` with `ς ⊒ Caps{Γ(y) for y free in b, y local}` (§5.4) |
| `(fn g (x…) b)` | as above with `g : (->ς (a…) R)` in Γ while typing `b` (monomorphic); `g` is not a capture |
| `(let ((x e) …) b)` | `e : T`, then Γ, x: T (monomorphic, no generalisation) ⊢ rest. With annotation `x: T'`: unify `T ~ T'`. Pattern bindings: §2.2 |
| `(if c t e)` | `c : bool`, `t : T`, `e : T`, result `T`; with narrowing (§2.4) |
| `(do e₁ … eₙ)` | `eᵢ` any type for `i < n` (a non-`unit` object result is released at the step); result type of `eₙ`; `(do)` : `unit` |
| `(match s cl…)` | `s : S`; each clause `p` checked against `S` binding pattern variables (§2.2); each body : `T`; result `T`; exhaustiveness checked after solving (§2.2) |
| `(Name e₁ … eₙ)` | constructor: `inst` of the constructor's scheme, then the application rule |
| `(. e x)` | `e : (Name T̄)` must be *resolved* to a struct with field `x` when the constraint is discharged (deferred head constraint, §3.3); result the field's type under `T̄ / ā`. If `e`'s type is still a variable at generalisation: error "field access on a value of unknown type; annotate" |
| `(cell e)`, `@e`, `(deref e)` | library types (§1.7); `@e` ≡ `(deref e)`: `e : c`, result `t`, constraint `Deref c t` |
| `(set! e₁ e₂)` | `e₁ : (Cell T)` (an expression) or `e₁` is an `&` parameter name of type `T`; `e₂ : T`; result `unit` |
| `(spawn e)` | library: `e : (->send () a)`, `Send a`, result `(Thread a)` |
| `(async b)` | Γ' ⊢ b : T where Γ' = Γ with `await` enabled; result `(Task T)`; constraint `send ⊒ Caps{captures of b}` (the task may run on any thread); no `&` parameter may be free in `b` (§6.8) |
| `(await e)` | `e : (Task T)`, result `T`; error outside `async` |
| `(unsafe b)` | `b : T` with `ptr` operations and `extern` calls enabled; result `T` |
| `(extern-name e…)` | as application, only inside `unsafe`; params and result are lIR scalars or `ptr` |
| `'e`, `` `e `` | `Syntax`; inside `` ` ``, `,e` requires `e : Syntax` and `,@e` requires `e : (Vec Syntax)` |
| `(gensym s)` | `s : str`, result `Syntax` |
| `nil` | `inst(∀a. (? a))` |
| `x: T` annotation anywhere | unify the inferred type with `T` (with named variables scoped to the enclosing `defun`) |

### 2.2 Patterns

`Γ ⊢ p : S ⇝ Γ'` — checking pattern `p` against scrutinee type `S`
yields bindings Γ'.

| Pattern | Rule |
|---|---|
| `_` | any `S`; no bindings |
| `x` | binds `x : S` |
| literal | `S` is the literal's type; requires `Eq S` |
| `nil`, `(some p)` | `S ~ (? a)`; `p : a` |
| `(V p₁ … pₙ)` | `V` a variant of enum `(Name ā)` with `n` payload types; `S ~ (Name b̄)` fresh `b̄`; `pᵢ : Tᵢ[b̄/ā]` |
| `(Name p₁ … pₙ)` | struct with `n` fields, likewise |
| `[p₁ … pₙ]`, `[p₁ … ... r]` | `S ~ (Vec a)`; `pᵢ : a`; `r : (Vec a)` |
| `(x :as p)` | binds `x : S`, then `p : S` |

A variable may occur once per pattern. Clause bodies are typed with
Γ ∪ Γ'. Exhaustiveness and redundancy are decided after solving, on
the resolved scrutinee type, by the standard matrix algorithm
(Maranget); for `(Vec a)` scrutinees a `_` or `[… ... r]`-only
clause is required; for scalars a `_` is required unless the type is
`bool` or `unit`.

`let` accepts only irrefutable patterns: `_`, `x`, `(x :as p)`,
`(Name p…)` (struct), and `(V p…)` when `V` is the only variant.

### 2.3 Places and `&`

`(f &x …)`: `x` must be (a) a local of type `(Cell T)` — then the `&`
position has type `T` — or (b) an `&` parameter of the enclosing
`defun` of type `T`. Any other `x` is the error "`&` argument must be a
cell variable or an & parameter". In `defun f (&v …)`, `v : T` where
`T` is fresh or the annotation; `T` is unified by uses. `&` positions
appear in the defun's signature and nowhere else: a `fn` cannot declare
them and a function type cannot mention them.

### 2.4 `if` narrowing — **Proposed**

`(if (nil? x) e₁ e₂)` with `x` any local variable in scope (a `let`
binding, a parameter, or a captured variable) of type `(? T)` types
`e₂` under Γ, x: T. Symmetric for `(some? x)` and `e₁`.
Implemented as the desugaring of syntax.md §4.4. The unwrapped `x` is
a borrow of the option's payload (§6.2).

### 2.5 Top-level forms

| Form | Rule |
|---|---|
| `(defun f (p…) -> R b)` | Params get fresh types (or annotations); `b : R'`; `R' ~ R` if annotated. `f` is bound monomorphically inside its SCC (§3.5); generalised after the SCC is solved. Every `Object`-typed parameter and the result get an escape summary (§6.3) |
| `(defstruct …)`, `(defenum …)` | Registered before any `defun` is typed. Field types are checked to be well-formed (arity, known constructors). Recursive occurrences are allowed only through annotated fields |
| `(defprotocol (P s d̄) sigs)` | Each method `m : ∀ s d̄ b̄. P s d̄ ⇒ (->send (T…) R)` where `b̄` are the other variables in its signature. `s` must occur in the first parameter type. The functional dependency `s → d̄` is recorded |
| `(extend-protocol (P (K ā) D̄) impls)` | For each method, the impl body is typed against the signature with `s := (K ā)`, `d̄ := D̄`, and must not be more specific. Missing methods take the `default` or are an error. The instance `∀ā. P (K ā) D̄` is added; overlap on `(P, K)` is an error. Each impl parameter's escape summary must be ⊑ the declared one (§6.3) |
| `(defmacro m (p…) b)` | `m : (->send (Syntax… ) Syntax)` (rest param `(Vec Syntax)`); typed and compiled like a `defun` in a separate macro-time module, before expansion of the rest of the module |
| `(extern c (T…) -> R)` | `c : (->send (T…) R)` usable only under `unsafe`; T, R are lIR scalars, `ptr`, or `unit` |
| `(defconst x e)` | `e : T` closed and constant; `x : T` global, immortal |

## 3. The inference algorithm — **Proposed**

Name: **constraint-based Hindley–Milner with level-based
generalisation (Rémy levels), qualified types with functional
dependencies (Jones-style improvement), a two-point colour lattice
solved by fixpoint, and deferred site coercions.** Every piece is
standard and decidable; the combination is described here in the
order an implementation runs it. Per module:

```
1. read, expand macros (macro-time modules compiled first)
2. collect top-level declarations: structs, enums, protocols, instances, externs, defun names
3. build the call graph of defuns; compute SCCs (Tarjan); order them topologically
4. for each SCC in order:
   a. bind each defun in the SCC to a fresh monomorphic function type at level ℓ+1
   b. generate constraints for each body (§3.2), unifying eagerly (§3.1)
   c. solve deferred constraints (§3.3): heads, protocol constraints with improvement, coercions
   d. solve colour constraints (§5.4)
   e. generalise (§3.5): quantify variables at level > ℓ; remaining constraints become the context; check ambiguity
   f. run the ownership elaboration on the SCC (§6); compute escape summaries (fixpoint within the SCC)
5. type-check protocol implementations against their signatures (they are their own SCCs; they may call any defun)
6. check `main : (->send () i64)`
7. export: schemes, summaries, layouts, instances, macros (§3.7)
8. monomorphise from `main` and exported instantiations (§4.3, §8)
```

### 3.1 Unification

Types are terms over constructors with arities; variables are
union-find nodes carrying a *level* (the `let`-depth at which they
were created; here only top-level SCC depth matters since local
`let` does not generalise) and a *kind* tag (`type` or `colour`).
`unify(T₁, T₂)`:

- var–any: occurs check; bind; propagate the smaller level to all
  variables in the bound type (Rémy).
- constructor–constructor: same constructor and arity, unify
  arguments pairwise; else error "cannot unify T₁ with T₂".
- function–function: unify parameter lists (same arity) and results.
  Colours are **never bound by unification**: unifying `(->κ₁ …)` with
  `(->κ₂ …)` emits the colour constraints `κ₁ ⊑ κ₂` and `κ₂ ⊑ κ₁`,
  except at a *flow site* (an application argument, a branch join, an
  annotated `let`, a `set!` value, a constructor argument), where only
  `κ_from ⊑ κ_to` is emitted. Colours are solved separately (§5.4).
  This is what lets a `send` closure be passed where a `local` one is
  accepted without subtyping on types.
- `(dyn P)` unifies only with itself.

The occurs check makes types finite; recursive data goes through
nominal `defstruct`/`defenum` (Open 3 for case 15).

### 3.2 Constraint generation

A single recursive function `infer(Γ, e) → (T, effects)` that unifies
as it goes and pushes non-equality constraints onto the SCC's
*deferred list*:

- protocol constraints `P T…` from `inst` of method and function
  schemes;
- `Send T`, `Object T` from library schemes;
- deferred head constraints: `Field(T, x, R)` from `(. e x)`;
  `Deref` is just the protocol;
- coercions `T_arg ↝ T_param` at every application argument, every
  constructor argument, every `set!` value, every `let` with an
  annotation, and every `if`/`match` branch join (branch types are
  coerced towards the first branch's type, then unified);
- colour constraints from `fn` literals and thread boundaries (§5.4).

Applications: `infer(f)` gives `(->κ (T₁…Tₙ) R)` after unifying with a
fresh function type of the call's arity; for each argument, `infer`
then emit `Tᵢ' ↝ Tᵢ`. A coercion whose two sides are already
structurally unifiable is discharged by unification immediately
(the common case, so most code never touches the deferred list).

### 3.3 Solving deferred constraints

Repeat until no progress:

1. **Heads.** `Field(T, x, R)`: if `T` resolves to `(Name T̄)`, look up
   the field, unify `R` with its type; if `T` is a variable, wait; if
   `T` is any other constructor, error.
2. **Protocol constraints with improvement.** For `P T₁ … Tₙ`: if
   `T₁`'s head is known, find the unique instance for `(P, head)`;
   instantiate it; unify its determined arguments with `T₂ … Tₙ`
   (this is the functional dependency `T₁ → T₂…`: it *improves* the
   other arguments); replace the constraint with the instance's own
   context. If no instance: error "no implementation of P for T₁".
   If `T₁` is a variable: keep the constraint; it will be generalised
   or reported ambiguous.
3. **Coercions.** `A ↝ B`: if `A ~ B` unifies, discharge. Otherwise if
   `B` resolves to `(? B')` and `A` resolves to a non-option type:
   record an inserted `some` at the site and unify `A ~ B'`. Otherwise
   if `B` resolves to `(dyn P)` and `A` is resolved and `P A` is
   satisfiable: record an inserted `as-dyn` and discharge. Otherwise
   if either side is still a variable: wait. Otherwise: the
   unification error for `A ~ B`.
4. **Structural predicates** (`Send`, `Object`): evaluate on the
   resolved type (§5.4); on a variable: keep.

Termination: each step either binds a variable (finitely many),
removes a constraint, or replaces one constraint by an instance
context (instance contexts are required to be *smaller*: every type
in the context must be a proper subterm of the instance head, the
usual Paterson condition), so the process terminates.

When no progress is possible, any remaining coercion is resolved by
plain unification (which may then fail with the ordinary message).

### 3.4 Closures, cells, atoms in inference

- **Closures** are function types; capture is invisible to the type
  except through the colour κ (§5.4). A closure bound by `let` is
  monomorphic in its scope: `(let ((id (fn (x) x))) (id 1) (id "a"))`
  is rejected. Top-level `defun`s are the unit of polymorphism. This
  is the deliberate limit that avoids the value restriction: because
  `let` never generalises, a cell created in a `let` cannot acquire a
  polymorphic type, so `(let ((c (cell nil))) (set! c (some 1)) (set! c (some "a")))`
  is a plain type error, as it should be.
- **Cells** are `(Cell T)`; `(cell nil)` is `(Cell (? a))` with `a`
  resolved by later uses in the same defun or defaulted (§3.5).
- **Atoms** are `(Atom T)` with `Send T` deferred until `T` is known.
- **Recursion**: SCCs are typed monomorphically together, then
  generalised, so `(defun even (n) …(odd …))` and `odd` get one
  scheme each. Polymorphic recursion (`f` used at two different
  instantiations inside its own SCC) requires `f` to be fully
  annotated; then the annotation is the scheme used for the recursive
  occurrences (standard). A self-recursive `fn g` is monomorphic
  inside its body.
- **Sum types** need no special treatment: constructors are functions;
  `match` types clauses against the instantiated variant payloads.

### 3.5 Generalisation at `defun` boundaries

After solving an SCC at level ℓ: every type or colour variable whose
level is > ℓ and that does not occur in Γ is quantified. Constraints
mentioning only quantified variables go into the scheme's context.
Then:

- **Ambiguity.** A protocol constraint `P T₁…` whose `T₁` is a
  quantified variable that does not occur in the defun's type (and is
  not determined from one that does) is an error: "ambiguous
  constraint P a in f; add an annotation".
- **Defaulting.** A quantified variable with no constraints that does
  not occur in the type is dropped (its instantiation is irrelevant;
  monomorphisation picks `i64`).
- **Colour** variables are quantified with their `⊑` and `Caps`
  constraints (§5.4), so `(defun twice (f) (fn (x) (f (f x))))` gets
  `∀ a ς₁ ς₂. ς₁ ⊑ ς₂ ⇒ (->send ((->ς₁ (a) a)) (->ς₂ (a) a))`.
- Local `let` bindings and `fn` parameters are never generalised.

Effect on error messages: a defun is checked in isolation from its
callers; a caller's misuse is reported at the call site against the
callee's scheme.

### 3.6 Site coercions — **Proposed**

Only two: `T ↝ (? T)` (option promotion) and `T ↝ (dyn P)`. They
apply only where §3.2 lists them, only when both sides are resolved
enough (§3.3), and never inside a type constructor (`(Vec T)` is not
coerced to `(Vec (? T))`). They are recorded as explicit `some` /
`as-dyn` nodes in the elaborated tree, so later passes and the
reference interpreter see no coercion. Principal types are preserved
for programs that contain no coercion site; a program whose coercion
depends on solving order is impossible by construction because
coercions are attempted only after all equalities in the SCC are
exhausted and then in source order.

### 3.7 Across modules

A module exports, per definition: the closed type scheme, the escape
summary (§6.3), and for structs/enums the layout (§8). Protocol
instances and macros are exported unconditionally. Importing
instantiates schemes; nothing is re-inferred. Because inference is
per SCC within a module and modules are acyclic, cross-module
recursion is impossible without an annotation-free workaround, and
that is intended: a recursive pair across modules must live in one
module. A module's interface is deterministic: two compilations of
the same source produce identical schemes (the solver is
deterministic: source order everywhere ties are possible).

### 3.8 What must be annotated

Inferred without annotation: all local types, `defun` parameter and
result types, closure types and colours, protocol constraints,
instantiations at every call, `Send`-ness, escapes, stack-vs-heap.

Annotation required (the checker says which):

| Situation | Why | Annotation |
|---|---|---|
| Recursive struct/enum fields | occurs check; nominal recursion needs a declared type | field types |
| Protocol method signatures | instances are checked against a contract that must exist before any instance | full signature |
| `extern` | no body | full signature |
| Polymorphic recursion | undecidable in general (Henglein) | the defun's full type |
| Ambiguous constraint (`(count [])`-style: `Countable a` with no `a` in the type) | no principal instance | any use that fixes `a`, or `[]: (Vec i64)` |
| `(. x f)` where `x`'s type is unknown at the end of the SCC | field names do not determine the struct | `x: Name` |
| Heterogeneous positions (`(list f g)` with different closure types) | HM has no subtyping | `(dyn P)` on the element type or a struct/tuple |
| `main` | the entry contract | `-> i64` |
| Higher-rank arguments (a parameter used at two types) | rank-1 only | not expressible; restructure |
| Pinning a colour in a signature (`(->send …)`) | protocol/extern callbacks must be sendable by contract | `->send` |

## 4. Protocols and dispatch

### 4.1 Declarations and instances

`(defprotocol (P s d₁ … dₖ) sigs)`: `s` dispatches, `d̄` are determined
by `s` (one instance per head constructor of `s`; the instance fixes
`d̄`). Coherence: at most one `extend-protocol` per `(P, K)` in the
whole program, checked at link (monomorphisation) time across modules
as well as within a module. Instances are `∀ā. (P (K ā) D̄) ⇐ C` where
`C` is the instance context (constraints on `ā` that the impl bodies
needed, inferred from the bodies and reported in the interface; e.g.
`(extend-protocol Eq (Vec a) …)` gets context `Eq a`). Instance
contexts must satisfy the Paterson condition (§3.3).

Scalars, `str`, `(Vec a)`, `(Map k v)`, `(List a)`, `(? a)`, tuples of
structs implement `Eq`, `Ord`, `Hash`, `Show` in the core library.
Structs and enums get them by `(derive Eq Name)` (a macro over
`struct-fields`).

Arithmetic: `+ - * /` are methods of `(Num a)`, comparisons of
`(Ord a)`, `=` of `(Eq a)`; instances for every integer and float
scalar. `(+ x 1)` therefore fixes `x : i64` (the literal is `i64`),
and `(defun add (a b) (+ a b))` is `∀a. Num a ⇒ (->send (a a) a)`.

### 4.2 Static dispatch (the default)

After monomorphisation (§4.3) every protocol constraint in a compiled
function has a resolved dispatch type, so every method call is a
direct call to the instance's function (`count$Vec` etc.). No
run-time type tag is consulted, no table is passed. This is why
`(Seq s e)`'s `first` can return an unboxed `i64` for `(List i64)`
and a pointer for `(List str)`: the two are different functions.

### 4.3 Monomorphisation

Starting from `main` (and exported non-generic functions, and every
`extend-protocol` method reachable through a `(dyn P)`), each call to
a polymorphic defun at instantiation `T̄` creates the specialisation
`f<T̄>` if absent, recursively. Type variables still free when a
specialisation is requested are instantiated to `i64`
(unconstrained) or are an error (constrained: this cannot happen
after §3.5's ambiguity check). Specialisations are keyed by the
*layout class* of each type argument (§8.1), not the type, so
`(Vec (Box i64))` and `(Vec str)` share one specialisation of
`count`; scalar widths and object-vs-scalar are what differ. Code
size is bounded by the number of distinct layout-class tuples used.
Recursive instantiation at ever-growing types (`(f (list x))` inside
`f`) is polymorphic recursion and was already rejected.

### 4.4 Dynamic dispatch: `(dyn P)`

Used when the static type is `(dyn P)`: heterogeneous collections
(`(Vec (dyn Show))`), plugin-style interfaces, and closures over
values whose type must be hidden. Representation §8.5. Each method
call on a `(dyn P)` receiver loads the function pointer from the
table. Methods whose signature mentions `s` anywhere other than the
receiver position (`(conj [s a] -> s)`) are *not callable* through
`(dyn P)` (object safety, as in Rust); a protocol with any such method
is still usable for `dyn` with those methods excluded. Protocols with
determined parameters are usable as `(dyn (P D̄))` with `D̄` fixed.

When each is chosen: static whenever the receiver's type is not
`(dyn …)`, which after inference is every case except those where the
programmer wrote `(dyn P)` in an annotation or a coercion site
produced one. There is no implicit fallback to dynamic dispatch on
inference failure; inference failure is an error.

## 5. Threads: the `Send` predicate and closure colours

### 5.1 What may cross — **Decided** (ownership §7)

Only objects that are immutable, or atoms, may cross a thread
boundary. A cell that is not an atom may not. The type-level
predicate that decides it:

```
Send i1 | i8 | … | f64 | bool | char | keyword | unit      = true
Send ptr                                                  = false   (unsafe only; Open 9)
Send str                                                  = true
Send (Cell T)                                             = false   ← "contains a non-atom cell"
Send (Atom T)                                             = Send T
Send (Weak T)                                             = Send T
Send (Task T)                                             = Send T
Send (Thread T)                                           = Send T
Send (? T)                                                = Send T
Send (->κ (T…) R)                                         = (κ = send)
Send (dyn P)                                              = false unless (dyn P :send)
Send (Name T̄)   (struct/enum, incl. Vec, Map, List, Box)  = ∧ Send Fᵢ[T̄/ā] over all field/payload types Fᵢ
Send Syntax                                               = true
Send α                                                    = the constraint (Send α), kept in the scheme
```

For recursive nominal types the definition is the greatest fixpoint
(assume `Send` for the type being defined while checking its fields);
this is computed once per struct/enum definition as a function of its
parameters, e.g. `Send (Vec a) = Send a`, `Send Node = false` if any
field is a cell. "Contains a non-atom cell" is exactly `¬Send` for
types without closures or `dyn`.

### 5.2 Where it is checked — **Decided** (ownership §3.4)

At every thread boundary, which after macro expansion is exactly: the
argument of `spawn` (so every `plet` initialiser), the function and
element types of `pmap` and the other parallel library functions, and
the body of `async`. All of these are expressed as `Send` constraints
in library types plus the colour rule for closures; the checker has
no other thread rule.

### 5.3 The error — **Decided** (case 13)

When a `Send T` constraint fails because `T` is, or contains, a
`(Cell …)` outside an `Atom`, the message is
`cell cannot be shared between threads` (followed by the type and the
capture or argument that carried it). When it fails for another
reason (`ptr`, a `local` closure captured by a `send` closure, a
non-send `dyn`) the message is
`value of type T cannot be shared between threads`.

### 5.4 Closure colours — **Proposed**

Every function type carries a colour κ ∈ {`send`, `local`} or a
colour variable ς. Constraints:

```
ς ⊒ Caps{T₁ … Tₖ}     at each (fn …): T̄ are the types of its free local variables
κ₁ ⊑ κ₂               at flow sites (§3.1) from the flowing function type to the receiving one
κ ⊑ send              at thread boundaries: the receiving type is (->send …), so this is the flow rule
```

Meaning: `send ⊑ local`; `ς ⊒ Caps{T̄}` means ς = `local` if any `Tᵢ`
is not `Send`, and unconstrained otherwise. `Send (->κ …)` is `κ =
send`, so a captured closure contributes `κ_captured ⊑ ς`, and a
captured `(Cell …)` contributes `local ⊑ ς`.

Solving (after type constraints of the SCC are solved):

1. Rewrite each `Caps` constraint: for each `Tᵢ`, evaluate `Send Tᵢ`
   by §5.1. If false → replace by `local ⊑ ς` and remember which
   capture forced it. If true → drop. If undecided (contains `Send α`
   or a colour variable) → keep as a symbolic constraint `Send Tᵢ ⇒ …`
   in the form `ς ⊒ send-of(Tᵢ)`.
2. Least fixpoint: set every colour variable to `send`; repeatedly,
   for every `κ₁ ⊑ κ₂` with κ₁ = `local` set κ₂ := `local`, until
   stable. Finite lattice, monotone, terminates.
3. Check: for every `κ ⊑ send` with κ = `local`, report §5.3 using the
   remembered capture (walk the ⊑ chain back to the `Caps` that forced
   `local`).
4. Generalise: colour variables still `send`-by-default that are
   quantified become scheme variables together with their remaining
   symbolic constraints; instantiation at a call site re-solves them
   with the caller's types. Colour variables not quantified (they
   appear in Γ — impossible at top level) do not arise.

Ergonomic consequences: a user never writes a colour; a `defun` that
takes a function and returns a closure that calls it (`comp`,
`partial`, `twice`) is transparently usable in `pmap` iff its
arguments are; a closure that captures only scalars and immutable
data is `send`; `(fn (i) (set! n …))` capturing `n : (Cell i64)` is
`local` and is rejected only when it reaches a boundary.

## 6. Ownership: how each rule is decided

Ownership is decided by an **elaboration pass** run per SCC after
types are final (§3 step 4f). It works on the closure-converted tree
(every `fn` is a struct literal plus a code pointer; captures are
explicit) and annotates every object-typed expression with a *mode*,
`owned` or `borrowed`, and every function with an *escape summary*.
The output is the tree the reference interpreter runs and the
compiler lowers (§8.6). Scalars have no mode.

Two facts fix the whole design: parameters are borrows and results are
owned (ownership §4, **Decided**); an object created in a scope and
never escaping it is not counted (ownership §2, **Decided**). So the
pass has to answer, per value, "does it escape?" and, per function
parameter, "may the callee make it escape?".

### 6.1 Escape classification, per form — **Decided** (ownership §3)

A *reference* is an occurrence of an object-typed expression. It
**escapes** its function iff it is in one of these positions:

| ownership §3 | Position in the tree | How the checker sees it |
|---|---|---|
| 3.1 returned | the tail position of a `defun` or `fn` body: the last form of the body, and recursively the branches of a tail `if`/`match`, the body of a tail `let`/`do` | `tail(e)` predicate over the tree |
| 3.2 stored into an object | argument to a constructor, to `set!`, to `cell`, `atom`, `reset!`, `some`, `[…]`/`{…}` literal, or to any callee whose summary says the parameter is retained (§6.3) | `retains(callee, i)` from summaries; constructors and the listed primitives retain all object arguments |
| 3.3 captured by an escaping closure | free variable of a `fn` whose own closure object escapes by this table | computed after the closure's escape is known (fixpoint over nested closures, innermost last) |
| 3.4 passed to another thread | free variable of a `fn` passed to `spawn`/`pmap`/…, i.e. captured by a closure that flows to a `->send` parameter; `pmap`'s element type also crosses | the closure is treated as escaping (its summary says `retains`) |
| 3.5 held across `await` | any free variable of an `async` body; any local of the async body live after an `await` | the async body is a closure that escapes by construction; locals live across `await` become coroutine-frame slots, which are owned (§6.8) |

A **call is never an escape for the caller** (ownership §4): if the
callee returns or stores a parameter, the callee retains it. The
caller consults summaries only to decide (a) whether a closure
literal it passes needs to own its captures, (b) whether an `&`
parameter captured by that closure is illegal (§6.5), and (c) whether
an object may live on the stack (§6.4).

`match` and pattern bindings, `(. e x)`, `(nth v i)`, `first`, etc.
produce references *derived* from another reference; a derived
reference escaping is an escape of the derived value (it is retained
at the escape point), not of the base.

### 6.2 Modes: what is emitted — **Decided** (semantics), **Proposed** (rules)

Every object-typed expression gets a mode:

| Expression | Mode | Reason |
|---|---|---|
| literal (`"s"`, `[…]`, `{…}`), constructor call, function/method call result, `@e`/`deref`, `swap!`/`reset!` result, `join`, `await`, `block-on`, `collect` | **owned** (+1 held by the consumer) | results are owned (§4); reads of cells and atoms return retained references (§7 for atoms; cells by the same rule, so a value read stays valid if the cell is overwritten) |
| local variable, parameter | **borrowed** | the binding holds the count |
| `&` parameter read (`v`) | **owned** (+1) | the local may be replaced by a nested `&` call while the read is in use (case 8); the compiler elides the retain when no `&`-call or `set!` on `v` occurs during the read's live range |
| `(. e x)`, pattern-bound variable, narrowed `x` | **borrowed** from `e`/the scrutinee | immutable objects never change; the base is kept alive for the borrow's live range (the enclosing statement, or the `match`/`let` scope) |
| `if`/`match` result | owned if any branch is owned (borrowed branches are retained at the join) else borrowed | branch-dependent owner (case 04) |
| `(do … e)` | mode of `e` | |

Emission rules (the reference interpreter does the same with every
mode treated as owned; ownership §2):

1. **Statement end.** An owned temporary not bound or consumed is
   released at the end of its statement (syntax.md §2 rule 6).
2. **`let`.** `(let ((x e)) b)`: if `e` is owned, `x` *owns* it and it
   is released when the `let` ends unless *moved*; if `e` is
   borrowed, `x` is an alias with no count operation.
3. **Move.** If `x` owns its value and `x` appears exactly once in
   `b`, in tail position or as an argument to a retaining parameter,
   the count is transferred (no retain, no release). Otherwise the
   escape point retains and the scope end releases.
4. **Tail/return.** The function's result expression: owned → returned
   as is; borrowed → retained then returned (ownership §4: "the callee
   retains it before returning").
5. **Retaining argument.** Argument to a constructor/primitive/callee
   parameter marked `retains`: owned temp → moved; borrowed → the
   callee retains (constructors and primitives retain inside; for a
   `defun` the retain is emitted inside the callee where the escape
   happens, so the caller emits nothing). Non-retaining argument:
   passed as a borrow; an owned temp argument is released after the
   call returns.
6. **Closure creation.** A `fn` that escapes (by §6.1, or because it
   flows to a callee/parameter that retains it, or to a `->send`
   parameter, or its summary is unknown) *owns* its captures: each
   object capture is retained at creation and released when the
   closure is freed. A `fn` that does not escape *borrows* its
   captures: no count operations; the closure object itself is
   scope-local.
7. **`set!`.** Release the old content after storing the new; the new
   value is retained if borrowed, moved if owned.
8. **`&` call.** Copy-in: the callee's local is initialised with a
   retained reference to the place's current content. Write-back:
   `(set! place v')` by rule 7. Nothing is released before the call
   returns, so `(f &x @x)` is sound (case 17).
9. **Copy-on-write** (ownership §5): a primitive that updates
   through `&v` checks the count: 1 → in place; >1 → copy, update the
   copy, release the old, store the copy.
10. **Scope end.** Every owned local not moved is released in reverse
    binding order.

Under rules 2–4 and 6, "scope-local objects are not counted"
(ownership §2) holds: an owned temp bound to a `let` and never
escaping gets exactly one release at scope end; the compiler removes
even that if the object is stack-allocated (§6.4). "Parameters are
borrowed" holds by the variable rule. "Unique updates happen in
place" holds by rule 9.

### 6.3 Escape summaries — **Proposed**

For every `defun`, `fn` and protocol method: for each parameter `i`
of object type, `retains(f, i) ∈ {no, yes}`. Computed per SCC as a
least fixpoint starting from `no`: `retains(f, i)` is `yes` iff the
parameter, or a reference derived from it (`.`, pattern binding,
`nth`, `first`, narrowed variable), occurs in an escaping position
(§6.1) in `f`, including "argument to `g` at position `j` with
`retains(g, j)`". Calls through a function *value* (a parameter or
local of function type) are `retains = yes` for every argument
(unknown callee). `extern` functions: `no` unless declared
`^retains`.

For protocol methods the summary is **declared** in the protocol
(`^retain` on a parameter, syntax.md §3.4; default `no`) and every
implementation is checked: an implementation whose computed summary
is `yes` for a parameter declared `no` is the error
`implementation of P.m for T retains parameter x, which the protocol
declares borrowed`. Calls to protocol methods use the declared
summary, whichever implementation runs. This keeps summaries modular
with dynamic dispatch and separate compilation.

Summaries are exported with type schemes (§3.7). The summary of a
generic function is the same for all instantiations (it depends on
positions, not types).

### 6.4 Stack allocation — **Proposed**

An object created by a literal or constructor in function `f` may be
allocated in `f`'s frame iff it never escapes in `f` (§6.1 says no),
every callee it is passed to has `retains = no` for that position,
and it is not read through a cell or atom (it never is; a stored
object escaped). Then no count operations are emitted for it at all,
and its `drop` releases its children at scope end. Otherwise it is
heap-allocated with a count header (§8.2). Both choices give the same
frees.

### 6.5 `&` parameters — **Decided** (ownership §5)

- **Distinct variables.** At each call, the set of `&` arguments is
  checked for a repeated variable name (after macro expansion, before
  typing): error `passed to more than one & parameter` (case 12).
  Purely syntactic: `(bar &x &x)`. `(bar &x &y)` with `x` and `y`
  holding the same object is fine (the count is 2, so the first
  update copies).
- **Escaping closure capture.** After §6.1 has decided which closures
  escape: a `fn` whose free variables include an `&` parameter of the
  enclosing `defun` and which escapes (tail, retained argument,
  unknown callee, thread boundary, or inside `async`) is the error
  `& parameter captured by escaping closure` (case 18). Case 8's
  closure is passed to `for-each`, whose protocol declaration has `f`
  borrowed (`retains = no`), so it does not escape and the capture
  is allowed. The closure borrows `v`'s slot; the slot's address is
  the callee's frame, valid for the call.
- **Copy-on-write** is implemented by the primitives (`append`,
  `assoc!`-style functions in the library) using the count header
  (§8.2); the compiler only guarantees the count is accurate, which
  rules 2–8 of §6.2 do.

### 6.6 Cells and weak references — **Decided** (ownership §6)

- `(Cell T)` is an object with one mutable slot; `set!` follows §6.2
  rule 7. `@c` is owned. Captured cells are shared by identity (the
  closure captures the cell reference, not its content), which is
  what case 05 needs and how "captured mutable variables are cells"
  is realised: there is no other mutable variable to capture.
- Cycles: only through cells (ownership §6). The checker does not try
  to detect them; `(set! c [c])` type-checks (Open 3) and runs. The
  reference interpreter reports objects still live at exit that are
  reachable only from cells as `leak-cycle`, distinct from every other
  audit failure (case 15). The compiler leaks them too; a trial
  deletion collector over cells may be added later without changing
  any rule.
- `(weak x)`: `x` must be `Object`; the object gets a control block on
  first `weak` (§8.7). `(deref w)` returns `(? T)`, owned when `some`.
  Weak references never keep the object alive, and never keep its
  memory mapped either: the control block is separate (§8.7). Case
  19's tree is freed when `main` returns because the only strong
  edges go downwards; case 20's `deref` finds the control block's
  strong pointer null.

### 6.7 Thread crossing — **Decided** (ownership §7)

Decided by types (§5). At run time, every object reachable from a
value crossing `spawn`/`pmap`/`async` is marked *shared* (§8.2 flag)
by a traversal at the boundary; shared objects use atomic count
operations from then on. Atoms are shared from creation. Because
`Send` guarantees no non-atom cell is reachable, the traversal only
meets immutable objects and atoms; it stops at atoms (their contents
are marked when stored, since `reset!`/`swap!` on a shared atom mark
the new value).

### 6.8 Async — **Decided** (ownership §8)

- `(async b)` is closure-converted to a coroutine object: captures are
  every free variable of `b`; the object escapes by construction, so
  §6.2 rule 6 retains every object capture on creation ("retained on
  entry"). Case 11's `s` is retained when `(measure s)` creates the
  task and released when the task is freed after `block-on`.
- Locals of `b` that are live across an `await` are stored in the
  coroutine frame; every frame slot is owned (borrowed values are
  retained when stored), so no borrow is ever held across `await`.
- If any free variable of `b` is an `&` parameter: error
  `& parameter in async function` (case 14). This is checked before
  the capture rule of §6.5 so that the more specific message wins.
- `await` outside `async`: "await outside async".

## 7. Case table

| Case | Rule | Mechanism | Verdict / error |
|---|---|---|---|
| 01 return part of argument | §4 | `head`: `xs` param borrowed; `(first xs)` result owned (protocol result); tail → returned. `first`'s impl for `List` returns a borrow of the node's head → retained before return (§6.2 rule 4). `retains(head,0)=yes` (derived ref escapes) | accept, 1, clean |
| 02 structural sharing | §5 | `(conj v 4)` owned result, `v` borrowed; `make`'s `v` owned local released at scope end while the new vector holds counts on shared nodes (library `conj` retains shared subtrees) | accept, 7, clean |
| 03 store borrowed value | §3.2 | `remember`'s `item` passed to `conj`, whose protocol declares `^retain` on the element → `retains(remember,1)=yes`; `s` retained twice (once per vector); inner `let` releases `s` and `b`; `a` moved out (rule 3) | accept, 5, clean |
| 04 branch-dependent owner | §4 | `if` join: `x` borrowed, `"fresh"` owned → result owned, `x` retained on its branch (rule 4 applied per branch) | accept, 5, clean |
| 05 closures share state | §6 | `n : (Cell i64)` captured by two `fn`s in tail position (inside `list`, a retaining constructor) → both escape → each retains the cell (count 3); `let` releases one; list release frees closures then cell | accept, 2, clean |
| 06 capture borrowed param | §3.3 | `(fn (s) …)` in tail position escapes → owns `prefix` (retained at creation); `p` released when its `let` ends; closure freed after `main`'s `let` | accept, 1, clean |
| 07 recursive accumulator | §4, §5 | `(conj acc n)` owned temp moved into the tail call's argument; `acc` borrowed; tail call (§8.6) frees the previous version when its count drops (only the new version references shared nodes) | accept, 100000, clean |
| 08 mutate while iterating | §5 | `v` read (owned, +1) passed to `for-each`; closure captures `&v` but `for-each`'s `f` is declared borrowed → not escaping → allowed; `append` sees count>1 on the first push → copies; later pushes in place; write-back at return | accept, 6, clean |
| 09 iterator outlives source | §3.1 | `(iter v)` is a library constructor with `^retain` on `v` → `retains(evens,0)=yes`; `v` released at inner `let` end, iterator still holds it | accept, 2, clean |
| 10 atom old value | §7 | `plet` → `spawn`; `(atom [])` needs `Send (Vec i64)` ✓; closure captures `a : (Atom …)` (Send) → colour `send` ✓ for `pmap`; `@a` returns retained snapshot; `swap!` releases old value; snapshot released at closure `let` end | accept, 1000, clean |
| 11 borrow across await | §8 | `async` captures `s` → retained on task creation; `s`'s `let` ends; task holds it until `block-on` completes and `task` is released | accept, 5, clean |
| 12 same binding twice | §5 | syntactic check on `(bar &x &x)` | reject: `passed to more than one & parameter` |
| 13 cell crosses thread | §7 | `(fn (i) (set! n …))` has `ς ⊒ Caps{(Cell i64)}` → `local`; flows to `pmap`'s `(->send (a) b)` → `local ⊑ send` fails; §5.3 message names the cell | reject: `cell cannot be shared between threads` |
| 14 inout in async | §8 | free variables of the `async` body include `&buf` → §6.8 check | reject: `& parameter in async function` |
| 15 cycle through cell | §6 | `(set! c [c])`: vector retains `c`; `c`'s `let` releases one count; cell and vector each keep the other at 1 → not freed; interpreter's exit audit finds them reachable only from a cell | accept, 1, leak-cycle |
| 16 single atom | §7 | `Send Accounts` ✓ (two `i64` fields); `swap!` replaces the struct atomically; readers hold retained snapshots | accept, 200, clean |
| 17 inout and borrow same call | §5 | args left to right: `@v` owned temp (+1), then copy-in (+1); `append` copies (count 3), callee local holds copy; write-back releases the original once; temp released after the call | accept, 4, clean |
| 18 inout captured by escaping closure | §5 | `(fn (x) (append &v x))` is in tail position → escapes → captures `&v` → §6.5 | reject: `& parameter captured by escaping closure` |
| 19 weak parent pointer | §6 | `Node` annotated; `(weak parent)` promoted to `(? (Weak Node))` (§3.6); `children` cell retains each child; parents are only weakly referenced upward; `depth` narrows `p`; `main`'s `root` release cascades down | accept, 2, clean |
| 20 weak ref to dead object | §6 | `v` owned by inner `let`, released at its end → freed, control block's strong pointer cleared; `(deref w)` → `nil` | accept, 1, clean |

## 8. Mapping to lIR — **Proposed**

lIR is an S-expression assembler for LLVM IR with no knowledge of
fibber (liar's `doc/lIR.md`; ADR 019: nothing bypasses it). Every
abstraction below is emitted *by the fibber compiler* as ordinary lIR
structs, functions and calls into a small runtime (`fib_*`, written in
lIR or C). lIR must type-check whole modules and run the LLVM
verifier (method.md rule 7); the audit findings in `lir-audit/` are
the acceptance list for that.

### 8.1 Representation of every type

| fibber type | lIR type | Layout class (for §4.3) |
|---|---|---|
| `i1 i8 i16 i32 i64` | same | scalar-N |
| `f32`, `f64` | `float`, `double` | scalar-f32 / f64 |
| `bool` | `i1` | scalar-1 |
| `char` | `i32` | scalar-32 |
| `keyword` | `i64` (interned id) | scalar-64 |
| `unit` | no value; `void` as a result, omitted as an argument | none |
| `ptr` | `ptr` | scalar-ptr (uncounted) |
| every object type below | `ptr` to a heap or stack block starting with the header (§8.2) | object |
| `(? T)`, T object | `ptr`, `nil` = null | object |
| `(? T)`, T scalar | lIR struct `{ i1 tag, T payload }` by value | scalar-opt-N |
| `(->κ (T…) R)` | `ptr` to a closure object (§8.4) | object |
| `(dyn P)` | lIR struct `{ ptr obj, ptr table }` by value (fat pointer) | dyn |
| `Syntax` | object (an ordinary enum); exists only in macro-time modules | object |

Inside containers every slot is 8 bytes: scalars narrower than 64 bits
are widened (`zext`/`sext` on load/store by the container's
specialisation), `(? scalar)` is stored as two slots, `dyn` as two
slots. This is what makes one specialisation of `(Vec a)` serve every
object element type and one per scalar width.

### 8.2 The count header

Every counted object begins with:

```
(defstruct fib_header (i64 i32 i32))
;  field 0: count        — strong count; 1 on creation
;  field 1: flags        — bit0 SHARED (atomic ops), bit1 HAS_WEAK, bit2 STACK (never counted), bit3 IMMORTAL (defconst)
;  field 2: type_id      — index into the runtime's type table: size, drop function, protocol tables
```

Retain and release are runtime functions the compiler calls:

```
(declare fib_retain  void (ptr))   ; if STACK or IMMORTAL: no-op; if SHARED: atomic add; else add
(declare fib_release void (ptr))   ; as above; on reaching 0: (type_table[type_id].drop obj) then free
(declare fib_share   void (ptr))   ; mark obj and everything reachable SHARED (stops at atoms' contents already shared)
```

`drop` for each struct/enum/closure/container is generated by the
compiler: it calls `fib_release` on every object-typed field (or
payload slot, per variant tag), clears the weak control block if
`HAS_WEAK`, then returns; `fib_release` frees the block. Stack objects
have `STACK` set and are never passed to `fib_release`; the compiler
emits a direct call to `drop` at scope end instead. A stack object may
be passed to a callee only if the callee's summary says `retains = no`
for that parameter (§6.4), so no retain ever reaches it.

The reference interpreter's audit instruments exactly `fib_retain`,
`fib_release`, allocation and free, plus every field load, and checks:
count never negative, no access after free, no double free, and at
exit every live object either is `IMMORTAL` or is reachable only
through a `Cell` (reported as `leak-cycle`).

### 8.3 Structs, enums, strings, collections

```
struct (Name …)  :  { fib_header, field₁, …, fieldₙ }          ; fields in declaration order, 8-byte slots
enum   (Name …)  :  { fib_header, i32 tag, [payload slots of the largest variant] }
enum with no payloads at all : i64 tag, no allocation, scalar layout class
str              :  { fib_header, i64 len, [len x i8] } ; NUL-terminated for FFI convenience
(Cell T)         :  { fib_header, slot }
(Atom T)         :  { fib_header, i32 lock, ptr value }   ; value's object is SHARED
(Weak T)         :  ptr to a control block (§8.7); the Weak itself is a counted object { fib_header, ptr block }
(Task T)         :  { fib_header, ptr code, i32 state, [frame slots] }  ; the coroutine (§8.8)
(Thread T)       :  { fib_header, ptr handle, slot result }
(Vec T), (Map k v), (List T), (Box T), (Iter T) : library structs, defined in fibber, so by the struct rule
```

Field access `(. e x)` is `(getelementptr %struct.Name p (i32 0) (i32 k))`
then `load`; construction is a runtime allocation of the type's size,
header initialisation, and stores; for a stack object, `alloca`.

### 8.4 Closures and function values

A `fn` literal `L` with captures `c₁…cₖ` compiles to:

```
(defstruct closure_L (fib_header ptr slot₁ … slotₖ))   ; field 1: code pointer
(define (L_code R) ((ptr env) (T₁ p₁) … (Tₙ pₙ)) …)    ; loads captures from env
```

A function value of type `(->κ (T…) R)` is a `ptr` to such an object.
Calling one: load field 1, `indirect-call` with `(ptr env, args…)`.
lIR's `indirect-call` must carry the full function type (lir-audit
finding: it currently types every argument as `ptr`); that is a lIR
hardening item. A named `defun` used as a value is a static
`constant` closure object with no captures and `IMMORTAL` set.
Escaping closures (§6.2 rule 6) emit `fib_retain` per object capture
before the stores; `drop` for `closure_L` releases those captures.
Non-escaping closures are `alloca`ed with `STACK` set and store raw
borrowed pointers. Colours have no run-time representation.

### 8.5 Protocol dispatch

Static (the default): the monomorphiser rewrites `(count v)` at
`v : (Vec i64)` to `(call @count$Vec$s64 v)`. No tables.

Dynamic: for every `(P, K)` instance reachable through a `(dyn P)`
coercion the compiler emits `(constant P$K$table ptr-array …)`: one
code pointer per method of `P` (and of its required protocols) in
declaration order, preceded by the `type_id`. `(as-dyn P e)` builds
the fat pointer `{ e, @P$K$table }` and retains `e` if the `dyn` value
escapes (it is an object for ownership purposes; `drop` of a `dyn`
releases `obj`). A method call loads `table[i]` and `indirect-call`s
with `obj` as the receiver.

### 8.6 What the compiler must emit for counting

From the elaborated tree (§6.2), in the order the interpreter does it:

| Elaboration | lIR emitted |
|---|---|
| retain at escape point / closure capture / `set!` of a borrow | `(call @fib_retain p)` |
| release at statement end / scope end / `set!` old value / `swap!` old value | `(call @fib_release p)` (in reverse binding order at scope end; along every exit path of the scope, including early `match` returns) |
| move | nothing |
| stack object scope end | `(call @drop_T p)` |
| tail call | `(tailcall @f args…)` with every release the frame owes emitted *before* the call. This is sound when each owned local is either moved into the call (no release owed) or not passed to the call at all (released before it, and the callee never sees it). If an owned local is passed *borrowed* to the tail call, releasing it first would hand the callee freed memory, so the compiler falls back to `call` + release + `ret` for that call (case 07 moves its accumulator, so it stays a `tailcall`) |
| `&` copy-in | `(call @fib_retain content)` then store to the callee's local slot (an `alloca` in the callee, passed by pointer) |
| `&` write-back | `set!` sequence on the place |
| thread boundary | `(call @fib_share p)` on the closure and on each argument before `spawn` / per element before `pmap`'s worker call |
| atom read | `fib_atom_load`: lock, load, `fib_retain`, unlock (the §7 obligation; a per-atom spinlock is the proposed implementation; Open 8) |
| atom write | `fib_atom_store`: lock, swap pointers, unlock, `fib_share` new, `fib_release` old |

lIR needs `tailcall` to mean `musttail` (lir-audit finding) for the
constant-stack guarantee of syntax.md §4.11.

### 8.7 Weak references

The first `(weak x)` on an object allocates a control block
`{ i64 weak_count, ptr strong }`, sets `HAS_WEAK` on `x`, and records
`x → block` in a runtime side table keyed by address (so objects that
never had a weak reference pay nothing, ownership §6). Each `(Weak T)`
value holds the block and bumps `weak_count`. `fib_release` reaching
zero on an object with `HAS_WEAK` sets `block.strong := null` and
removes the table entry before freeing. `(deref w)`: if
`block.strong` is non-null, `fib_retain` it and return `some`; else
`nil`. For `SHARED` objects the block's `strong` is read and retained
under the same lock discipline as atoms. The block is freed when
`weak_count` reaches zero and `strong` is null.

### 8.8 Async

An `async` body is compiled to a state machine (LLVM coroutine
intrinsics are *not* used, to keep lIR 1:1 and inspectable): a
`(Task T)` object holds a `state` index, every capture and every
local live across an `await`, and a `poll` code pointer. `await e`
polls `e`; if pending, stores the state and returns `pending`. The
executor (`liar-runtime`'s reactor, brought over) calls `poll`. All
frame slots are owned (§6.8): `drop` of the task releases them.

### 8.9 Unsafe and FFI

`(extern c (T…) -> R)` is `(declare c R (T…))`. `str-bytes` returns
the pointer to the string's bytes (a borrow); `malloc`/`free` are the
C functions; `ptr+`, `load-*`, `store-*` are `getelementptr`, `load`,
`store`. None of these touch headers. `^retains` externs receive a
`fib_retain`ed pointer and the foreign side must call `fib_release`.

## 9. Open decisions

1. **Colours in function types** (§5.4). Recommend: yes, as
   specified; it makes closure sendability flow through higher-order
   library code and generalises cleanly. Alternative: no colour in
   types; require the closure at a thread boundary to be a `fn`
   literal or a `defun` name (syntactic check), rejecting
   `(pmap f xs)` for a parameter `f` — simpler, much less composable.
2. **Escape summaries declared on protocol methods** (`^retain`,
   §6.3). Recommend: yes; it is the only modular option with dynamic
   dispatch. Alternative: infer the summary as the join over all
   implementations visible at link time (whole-program; breaks
   separate compilation but needs no annotation).
3. **Recursive types through `Cell`** (case 15). Recommend: rewrite
   the case with `(defstruct Self (v: (Cell (Vec Self))))` — the
   verdict (`accept`, `leak-cycle`) is unchanged and the type system
   stays finite. Alternative: equirecursive unification when the
   cycle passes through `(Cell …)`, which is what makes `(set! c [c])`
   type as written; decidable, but complicates every consumer of
   types (layouts, `Send`, monomorphisation keys).
4. **`let` never generalises** (§3.4). Recommend: yes; avoids the
   value restriction and keeps cells simple. Alternative: generalise
   `let`-bound syntactic values (value restriction), giving local
   polymorphic helpers at the cost of a subtler rule.
5. **Monomorphisation keyed by layout class** (§4.3). Recommend: yes
   (bounded code size, unboxed scalars). Alternative: dictionary
   passing (no specialisation, boxed generics, run-time cost on every
   protocol call in generic code).
6. **Reads of cells and `&` locals are owned (+1)** (§6.2). Recommend:
   yes; it is the only rule under which case 08 and case 17 are sound
   without alias analysis, and the compiler elides the retain when it
   can see no intervening write. Alternative: borrowed reads with a
   flow-sensitive "no write while borrowed" check (a small borrow
   checker), which the ownership spec deliberately avoids.
7. **`first`/`nth` on an empty collection**: recommend a run-time
   panic (a trap, audited as a failure) with `(? T)`-returning
   variants `first?`/`nth?` in the library; keeps case 01 as written.
   Alternative: `first : (? e)` always, and case 01 gains a `match`.
8. **Atom read/retain atomicity**: recommend a per-atom spinlock
   (simple; the §7 obligation is met trivially). Alternative:
   hazard-pointer or epoch deferred reclamation (lock-free reads,
   much more runtime).
9. **`Send ptr`**: recommend false (a raw pointer's target has no
   header to mark shared). Alternative: true with the burden on
   `unsafe` code.
10. **`(dyn P)` sendability** declared per use (`(dyn P :send)`).
    Recommend as specified. Alternative: `dyn` never `Send`.
11. **Option promotion and `dyn` promotion as the only coercions**
    (§3.6). Recommend both on. Alternative: neither (explicit `some`
    and `as-dyn`).
12. **Unknown-callee conservatism**: calling through a function value
    makes every argument `retains = yes`, so a closure literal passed
    to such a call owns its captures and may not capture an `&`
    parameter. Recommend: accept the conservatism (it costs retains,
    never correctness) and revisit with summaries in function types
    if real programs hit it. Alternative: add a retain-set to function
    types (a second effect on `->`), which is what a fully precise
    version needs.
