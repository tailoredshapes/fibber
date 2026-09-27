# fibber syntax

Status: synthesis of the three drafts under `spec/drafts/` (see
`spec/drafts/SYNTHESIS.md`). Authority: [ownership.md](ownership.md) is
Decided. Every decision here is marked **Decided** only where it is a
direct consequence of ownership.md, and **Proposed** otherwise. The case
verdicts in `cases/ownership/` are fixed; Appendix A rewrites the case
bodies into this syntax with their headers unchanged.

Companion: [types.md](types.md) gives the type grammar, the typing rule
of every form here, the inference algorithm, the ownership checker that
decides every rule of ownership.md, the case table and the lIR mapping.
Section references of the form "types §n" are to that file; "§n" alone
is ownership.md.

Design rule (Proposed): a form is **core** only if the checker must see it
on the surface to decide a rule, it binds names or introduces a type or an
implementation, or it changes control flow or the evaluation regime in a
way no function can. Everything else is a macro over core forms or a
library function. §4 states the rule and lists what falls on each side.

---

## 1. Reader

The reader turns text into **forms**. A form is a value of the built-in
sum type `Form` (§3.16); the same type is what macros receive and return,
which is what makes fibber homoiconic. The reader knows nothing about
special forms.

### 1.1 Lexical structure

| Class | Syntax | Form produced |
|---|---|---|
| whitespace | space, tab, newline; a comma not immediately followed by a non-separator character (§1.2) | none |
| line comment | `;` to end of line | none |
| form comment | `#_` followed by one form | the form is read and discarded |
| integer | `42`, `-7`, `0x1F`, `0b1010`, `1_000_000`, with optional width suffix `i8` `i16` `i32` `i64` (`1i32`, `0xFFu8` is not valid: no unsigned types) | `(Int v width)`; width is `i64` when no suffix |
| float | `3.14`, `-0.5`, `1e9`, `2.5e-3` (a `.` or an exponent makes it a float), optional suffix `f32` `f64` | `(Flt v width)`; `f64` when no suffix |
| string | `"..."`, UTF-8, escapes `\n \t \r \0 \\ \" \xNN \u{HHHH}` | `(Str s)` |
| character | `\a`, `\\`, `\newline`, `\space`, `\tab`, `\return`, `\u{1F600}` | `(Chr c)` (one Unicode scalar) |
| boolean | `true`, `false` | `(Bool b)` |
| nil | `nil` | `(Nil)` |
| unit | `()` | `(List [])`; as an expression it is the unit value |
| keyword | `:name`, `:ns/name` | `(Kw s)` |
| symbol | see below | `(Sym s)` |
| list | `( f1 f2 ... )` | `(List [f1 f2 ...])` |
| vector | `[ f1 f2 ... ]` | `(Vec [f1 f2 ...])` |
| map | `{ k1 v1 k2 v2 ... }` (odd count is a read error) | `(Map [k1 v1 k2 v2 ...])` |

A literal that does not fit its width (`300i8`, `9223372036854775808`) is a
read error, never a wrap. Numbers carry their width in the reader; there
is no literal polymorphism (types §1.1; Open decision 10).

**Symbols.** A symbol is a maximal run of characters that are not
whitespace, `( ) [ ] { } " ; ' `` ` `` `,` `@` `\`, not starting with a
digit (or `-` followed by a digit), `:` or `#`. Symbols are
case-sensitive. A symbol may contain `/` once, separating a namespace
alias from a name (`seq/first`); `/` alone is a name. A symbol ending in
`:` (`x:`) is an ordinary symbol that the binding grammars of §3
recognise as an **annotation marker**: `x: T` reads as the two forms
`x:` and `T`. `.`, `&` and `->` are ordinary symbols; `defun`, `fn`,
`let`, `defstruct`, `defenum`, `defprotocol` and `impl` give them their
meaning (§3). The symbol `&` alone is reserved (§1.2). `_` is the
wildcard in patterns and an error as an expression. `true`, `false` and
`nil` are literals, not symbols: the reader never produces `(Sym
"nil")`, and the form `(Nil)` is the constant `nil` of `Option`
wherever a form is read or built by a macro (§3.9, §3.16).

### 1.2 Prefix reader macros

Each rewrites to a list form. There are exactly six.

| Text | Reads as | Meaning |
|---|---|---|
| `'x` | `(quote x)` | the form `x` as a `Form` value (§3.16) |
| `` `x `` | `(quasiquote x)` | template (§3.16) |
| `,x` | `(unquote x)` | inside a quasiquote: splice one form |
| `,@x` | `(unquote-splicing x)` | inside a quasiquote: splice a `(Vec Form)` |
| `@x` | `(deref x)` | read a cell, atom or weak reference (§3.11) |
| `&x` | `(& x)` | in-out argument, parameter or place (§3.13); `x` must be a symbol or a `(. sym field)` form |

A comma immediately followed by a character that can start a form is
`unquote`; any other comma is whitespace, so `[1, 2]` and `` `(a ,b) ``
both read as expected. `@` and `&` bind tightly to the following form:
`@(. p children)` is `(deref (. p children))`. `&` is only meaningful in
a parameter list or in argument position; anywhere else `(& x)` is an
error at expansion.

### 1.3 Source positions

Every form carries a position (file, line, column). Positions survive
macro expansion: a form produced by a macro carries the position of the
macro call, unless the macro built it from an input form, which keeps
its own. Every compile error names a position. (Proposed.)

### 1.4 Literal collections are library calls

`[e ...]` and `{k v ...}` in expression position are not core forms. After
macro expansion the compiler rewrites them:

```
[]              ⟹ (vec-empty)
[e1 e2 ... en]  ⟹ (conj (conj ... (conj (vec-empty) e1) ...) en)
{}              ⟹ (map-empty)
{k1 v1 ...}     ⟹ (assoc (assoc (map-empty) k1 v1) ...)
```

`conj`, `assoc`, `vec-empty` and `map-empty` are prelude names (§4.4);
the rewrite resolves them in the prelude, not in the current namespace,
so a user binding of `conj` does not change what a literal means. The
compiler may fuse the chain into one allocation; that is an
optimisation, not a semantic. Inside `quote`/`quasiquote` the brackets
stay `Vec`/`Map` forms and are not rewritten, so macros see `[x y]` as
data. In a pattern (§3.6) brackets are not allowed in v1 (Open decision
11).

### 1.5 Annotations

`x: T` in a binding position annotates `x` with the type `T` (grammar in
types §1). `-> T` after a parameter list annotates the result. Both are
optional everywhere except where types §3.8 says they are required.

---

## 2. Programs, expressions, evaluation order

A **program** is a set of modules (§5), one of which defines `main` with
type `(fn () i64)`. A module is a sequence of top-level forms: `ns`,
`defun`, `def`, `defstruct`, `defenum`, `defprotocol`, `impl`,
`defmacro`, `extern`, a macro call expanding to one of these, or a
top-level `(do form*)`, which is **spliced**: it stands for its forms in
order, each of them a top-level form expanded in turn (Proposed; Open
decision 24). The splice is how a macro whose expansion is several
definitions returns them as one `Form` (§3.16); `do` keeps its
expression meaning everywhere else (§3.5). Every other form is an
**expression**, and an expression at top level is an error.

Evaluation is strict and **left to right, inner before outer**
(Proposed; liar ADR 008 kept):

- a call `(f a1 ... an)`: `f` (when it is not a global name), then `a1`
  ... `an`, then the call; for an `&x` argument the copy-in happens at
  its argument position (§3.13), and the write-backs happen after the
  call returns, in parameter order (they target distinct places, so the
  order is unobservable);
- `let`: initialisers in order, each seeing the earlier bindings;
- `do`: steps in order; the value of the last is the value of the form;
- `loop`: initialisers in order, like `let`; `recur`: its arguments left
  to right, then the rebinding of every loop variable at once (§3.18);
- `if`/`match`: the test or scrutinee, then exactly one branch;
- constructor arguments and literal-collection elements: left to right;
- `plet` initialisers run concurrently in unspecified order; `pmap`
  applies its function in unspecified order and returns results in input
  order.

**Steps and temporaries** (Decided in consequence of §2 and §4: results
are owned by the caller, so the caller must release them somewhere, and
this fixes where). A value produced by an expression and not bound is an
*owned temporary* of the innermost enclosing **step**:

| Step | Its temporaries die |
|---|---|
| the argument list of a call | after the call returns and its write-backs are done |
| a `do` step that is not last | immediately after the step |
| the test of an `if` | after the test is evaluated, before either branch |
| the scrutinee of a `match` | after the whole `match` (the scrutinee is an implicit binding for the form; types §6.3) |
| a `let` initialiser | never as a temporary: the binding owns the value |
| the argument list of a `recur` | never as temporaries: the values are moved into the loop's slots at the rebinding (types §6.10) |
| the body of a function | its value is the result; nothing else survives |

The reference interpreter releases at exactly these points; the compiler
elides count operations but frees the same objects (types §6.12).

No truthiness: `if` takes a `bool`. `nil` is `Option`'s empty variant
and nothing else (§3.9).

---

## 3. Core special forms

Grammar notation: `sym` symbol, `type` a type form (types §1), `expr`
expression, `pat` pattern (§3.6), `body` one or more expressions
evaluated as a `do`, `*` zero or more, `+` one or more, `?` optional.
Each entry gives grammar, static requirements and the evaluation rule.
Ownership consequences are stated briefly and decided in types §6.

### 3.1 `defun`

```
(defun name (param*) where? ret? body)
param ::= sym | sym: type | sym :borrow | sym: type :borrow | &sym | &sym: type
where ::= :where (constraint+)          ; constraint ::= (Proto type+) | (Send type)
ret   ::= -> type
```

Static: `name` is bound in the module's namespace before any body is
checked, so functions may be mutually recursive without forward
declaration. Parameter names must be distinct. Unannotated parameter and
result types are inferred and generalised at the `defun` boundary (types
§3); `:where` lists protocol or `Send` bounds on the type variables of
the annotations (types §1.3), rarely needed since bounds are inferred.
`sym :borrow` (with or without a type; the keyword follows the
parameter it qualifies, as `:borrow` does in `defprotocol`, §3.10) is a
checked promise that the function does not make that parameter escape:
its escape summary (types §6.4) is then part of the interface rather
than an inference result, and a body that breaks it is the error
`parameter p of f is declared :borrow but escapes`. Summaries are
inferred, so the annotation changes no verdict; it documents and pins
one (Proposed; Open decision 23). `&sym` is an in-out parameter
(§3.13). A function with an `&` parameter is not a value: it may be
called but not passed, stored or returned (Proposed; types §1.4). A
`defun` may not be redefined in its namespace.

A `defun` is an **async function** for §3.13 rule 3 iff, after
expansion, (i) an `async` form anywhere in its body (including inside
nested `fn`s) mentions one of its `&` parameters, as `@v`, `&v`, `&(. v
f)` or the target of a `set!`, anywhere inside the `async` body; or
(ii) an `async` form is in tail position of its body (the function's
value is a task it creates: the last step of a `do`, a branch of a tail
`if`/`match`, the body of a tail `let`, transitively, but not inside a
`fn`). A function whose body merely contains an `async` that it drives
to completion itself (`(block-on (async ..))`) and that mentions no `&`
parameter is an ordinary function (Proposed; Open decision 22). Both
conditions are syntactic.

Evaluation: a named function is global, captures nothing (**Decided**, §6),
and when used in non-head position is a function value with an empty
environment (types §8.4).

Ownership (**Decided**, §4): every parameter is a borrow for the whole call
and is never freed by the callee; the result is owned by the caller.

### 3.2 `fn`

```
(fn name? (param*) ret? body)
param ::= sym | sym: type
```

An anonymous function that closes over the variables it uses from
enclosing scopes. With `name`, the body may call `name`; the call goes
through the closure's own code pointer and environment, not through a
capture, so it creates no cycle (**Decided**, §6). `name` is bound only in
the body. A `fn` has no `&` parameters (Proposed; Open decision 1).

Static: the **capture set** of a `fn` is the set of free variables of its
body bound in enclosing scopes, computed after expansion (types §3.7).
Whether the closure **escapes** is decided by the checker, never written
by the programmer (types §6.5). A closure may capture an `&` parameter of
the enclosing `defun` only if it does not escape the call (**Decided**, §5;
error text in §3.13).

Evaluation: creates a closure value. Captured variables are read at
creation. Variables are immutable, so a capture is a copy of the
reference; a captured cell is the cell object itself, which is why two
closures capturing the same cell share it (**Decided**, §6, case 05).

Ownership: an escaping closure retains every object it captures at
creation and releases them when it is freed; a non-escaping closure
borrows its captures (types §6.5).

### 3.3 `let`

```
(let ((pat expr)+) body)
```

Sequential bindings: each initialiser sees the earlier ones. `pat` must
be irrefutable: a symbol, `_`, `(pat :as sym)` (§3.6), a struct pattern, or a
pattern of the only variant of an enum. Shadowing an enclosing binding
is allowed; a name may not be bound twice in one `let` (Proposed: liar
ADR 006 is dropped; the checker keys every rule on binding sites, not
names, so shadowing costs nothing).

Evaluation: evaluate each initialiser in order and bind; evaluate the
body; its last value is the value of the form. Bindings die in reverse
order when the body finishes.

Ownership: a binding **owns** the value of an initialiser that is a fresh
object, a call result or a read of a cell, and **borrows** it when the
initialiser is another variable or a field path (types §6.3). A value
that leaves the scope as its result is moved out or retained (the
scope-exit rule, types §6.3); an object that never escapes is freed at
the scope end with no count operations (**Decided**, §2).

### 3.4 `if`

```
(if expr expr expr)
```

The test has type `bool`; both branches are required and have one type.
`when`, `unless`, `cond`, `and`, `or` are prelude macros over `if`.

Ownership (**Decided** by §3.1 and case 04): if one branch's value is a
borrow and the other's is owned, the borrowed branch retains, so the
`if` as a whole is owned (types §6.3, the join rule).

### 3.5 `do`

```
(do expr*)
```

Evaluates the steps in order; the value of the last is the value of the
form; `(do)` is `()` of type `unit`. A non-final step may have any type;
its value is discarded (an owned object is released at the step's end).

### 3.6 `match`

```
(match expr clause+)
clause ::= (pat body)
pat    ::= _                    ; wildcard
         | sym                  ; binds the whole value
         | literal              ; integer, float, string, char, bool, keyword
         | nil                  ; the empty variant of Option: the reader's (Nil) form; (nil) is its (Variant) spelling (§3.9)
         | (some pat)           ; the full variant of Option
         | (Variant pat*)       ; enum variant, positional; (Variant) for a field-less variant
         | (Struct pat*)        ; struct, positional in field order
         | (pat :as sym)        ; bind the whole while matching inside
```

Static: the scrutinee's type must be resolved when exhaustiveness is
checked (types §2.6). Clauses are tried top to bottom; the set must be
exhaustive: every variant of an enum or `Option`, both values of `bool`,
and a final `_` or symbol clause for every other type. A clause that can
never match is an error. A variable may occur once per pattern. A bare symbol other than `nil`
is always a binding, never a variant constant; a field-less variant is
matched as `(Variant)` (Open decision 18). `match` is the only
eliminator of enums and `Option` in the core; `nil?`,
`some?`, `if-let`, `when-let` are prelude definitions over it.

Evaluation: evaluate the scrutinee once; take the first clause whose
pattern matches; bind its variables; evaluate its body.

Ownership: pattern variables are borrows of parts of the scrutinee
(*derived* bindings, types §6.2); the scrutinee is kept alive for the
whole form. A pattern variable bound to a scalar is a copy.

### 3.7 `defstruct`

```
(defstruct Name (field+))
(defstruct (Name tvar+) (field+))
field ::= sym: type | sym
```

A nominal product type, a constructor function `Name` taking the fields
positionally, and field access via `.`. A field without a type
annotation becomes a fresh type parameter appended to the struct's
parameter list in field order, so `(defstruct Pair (a b))` means
`(defstruct (Pair a b) (a: a b: b))` (Proposed). A struct that mentions
itself, directly or through other types, must annotate the recursive
fields (types §1.3). Structs are immutable objects (**Decided**, §1); a
field of type `(Cell T)` or `(Atom T)` is how a struct holds mutable
state, and the unique-update primitives of §3.13 are how a struct held
in a place is updated in place.

### 3.8 `.` (field access)

```
(. expr field)
```

`expr` has a struct type known by the end of the enclosing `defun`'s
inference (types §3.4); `field` is a symbol. The result is the field's
value: a borrow derived from `expr` for objects, a copy for scalars. A
`(. expr field)` whose field has type `(Cell T)` may be the first
argument of `set!`. The place form `&(. x field)` of §3.13, with `x` a
variable of cell type holding a struct, is not a field access on the
cell: it names the field of the struct *in* `x`.

### 3.9 `defenum`

```
(defenum Name variant+)
(defenum (Name tvar+) variant+)
variant ::= (Variant) | (Variant field+) | Variant
field   ::= sym: type | type
```

A nominal sum type and one constructor per variant; a variant without
fields is a constant, written bare in expressions (`empty`, `nil`) and
as `(Variant)` in patterns (§3.6); `(empty)` as an expression is the
error `empty is a constant, not a function` (types §2.2; Open decision
18). Variant names live in the namespace beside
functions and must be unique there (Proposed; Open decision 12). An enum
whose variants all have no fields is a scalar (types §8.1).

`Option` is **built in**, like `Form` (§3.16), not declared in the
prelude: its variants are the field-less `nil` and `(some v: a)`, but
`nil` is a reader literal (§1.1) that reads as the form `(Nil)`, never
as a symbol, so no `defenum` can spell it (the text `(defenum (Option
a) (nil) (some v: a))` reads its first variant as `(List [(Nil)])`,
which is not a variant). `nil` and `(some x)` are its constructors,
`nil` and `(some pat)` its patterns, and `match` is its eliminator like
any other enum's. The expander treats the form `(Nil)` and the symbol
`nil`, which only a macro can build (as `(Sym "nil")`, or through
`enum-variants`, §3.16), alike: the constant `nil` in an expression and
the empty-variant pattern in a pattern; `(nil)` in a pattern is the
`(Variant)` spelling of the same, and `(nil)` in an expression is the
error `nil is a constant, not a function; write nil`, exactly as for
`empty`. A macro that writes `nil` inside a quasiquote emits the
reader's `(Nil)` and needs no special case (proposed case 51). Its
representation is in types §8.1; the prelude derives `Eq`, `Ord`,
`Hash` and `Show` for it (§4.4) (Proposed; Open decision 28). There is
no implicit lifting of `T` to `(Option T)` and no null of any other
type (Proposed; Open decision 9).

### 3.10 `defprotocol`, `impl`

```
(defprotocol Name method+)
(defprotocol (Name self det*) method+)          ; self dispatches; det* are determined by self
method      ::= (mname (self mparam*) -> type)
mparam      ::= sym: type | sym: type :borrow

(impl Name type where? method-impl+)
(impl (Name type*) type where? method-impl+)    ; protocols with determined parameters
where       ::= :where (constraint+)            ; the instance context (types §2.7)
method-impl ::= (mname (self sym*) ret? body)
```

`defprotocol` introduces a nominal protocol with fully typed method
signatures. The first parameter of every method is `self`, of the
dispatch type; the determined parameters `det*` are fixed by the
implementing type (types §4.1). `impl` declares that a type constructor
applied to distinct type variables (`(Vec a)`), a scalar, a built-in
object type (`str`, `Form`, `(Array a)`), or a struct/enum name
implements the protocol, and gives every method (all required; no
defaults in the core). A generic `impl` declares the bounds
its bodies need on its type variables in `:where` (`(impl Eq (Vec a)
:where ((Eq a)) ...)`); an `impl` without `:where` has an empty context,
and a body that needs a bound not listed is an error (types §2.7; Open
decision 19). Contexts are declared, not inferred, because a `defun`
that calls a method is type-checked and generalised before any `impl`
body is (types §3.5). One `impl` per (protocol, head constructor) in the
whole program.

Escape kinds (Proposed; types §6.4): a method parameter other than `self`
defaults to the **escaping** kind. `x: type :borrow` promises that no
implementation makes `x` escape; the checker rejects an `impl` whose body
breaks the promise. The same keyword after a `defun` parameter is the
same promise on a plain function (§3.1). Only a `:borrow` parameter may receive a
non-escaping closure or a closure that captures an `&` parameter
(cases 08, 18). A method signature that mentions a closure type without
a colour (`(fn (a) unit)`) accepts closures of any colour; `(fn :send
(a) unit)` requires a sendable one (types §1.4).

Dispatch: static when the receiver's type is concrete after inference,
which is always except through the explicit `(dyn P)` type (types §4).

### 3.11 Cells, atoms, weak references: builtins, not forms

These are **builtin functions** with ordinary call syntax. They are listed
here because the reader gives `@` to `deref` and because the checker has
a rule for each (types §2.9–§2.11). None is a special form.

| Call | Type | Meaning |
|---|---|---|
| `(cell v)` | `a -> (Cell a)` | a new cell holding `v` |
| `(deref c)`, `@c` | protocol `Deref` | cell → its value; atom → its value; weak → `(Option T)`. Every object result is owned (+1) |
| `(set! c v)` | `(Cell a) a -> unit` | store `v`, release the old value; `c` is any expression of cell type, including a field path and an `&` parameter |
| `(atom v)` | `a -> (Atom a)`, `Send a` | a new atom |
| `(swap! a f)` | `(Atom a) (fn (a) a) -> a` | replace atomically with `(f old)`; `f` may run more than once; returns the new value (owned) |
| `(reset! a v)` | `(Atom a) a -> unit` | replace, release the old value |
| `(weak x)` | `a -> (Weak a)`, `a` an object type | a weak reference; does not keep `x` alive |

Variables are immutable; `set!` on anything but a cell is an error. The
only mutable things a program can name are cells, atoms and `&`
parameters, and an `&` parameter is a cell (§3.13). Captured mutable
state is an explicit `(cell ...)` (**Decided**, §6: "captured mutable
variables are cells"); there is no implicit boxing of assigned
variables.

### 3.12 Threads: two builtins, a macro and a library function

| Call | Type | Meaning |
|---|---|---|
| `(spawn f)` | `(fn :send () a) -> (Task a)`, `Send a` | run `f` on another thread; the task is held by the caller and, until the result is stored, by the thread (types §6.8) |
| `(join t)` | `(Task a) -> a` | wait; the result, owned. Any number of holders may `join` one task: the runtime resumes a task on one thread at a time and the others wait for its completion (types §8.8) |
| `(plet ((sym expr)+) body)` | macro | `(let ((t1 (spawn (fn () e1))) ...) (let ((s1 (join t1)) ...) body))` |
| `(pmap f xs)` | library | `(fn :send (a) b) (Vec a) -> (Vec b)`, `Send a`, `Send b`; results in order |

`spawn` is the only thread-crossing builtin; `plet` bindings cannot see
each other. Passing a value to another thread is an escape (**Decided**,
§3.4) and only immutable objects or atoms may cross (**Decided**, §7);
both are decided by the `Send` predicate on the closure's type (types
§5), and a closure's sendability is decided by what it captures (its
**colour**, types §5.4). The reader-visible consequence: a closure that
captures a `(Cell T)` cannot be given to `spawn`, `plet` or `pmap` (case
13). `block-on` is the prelude's name for `join` used on an `async` task
(§3.14).

A task whose handle is discarded still runs to completion: the task
object is created with one count for the caller and one for the thread
(types §6.8), and `main` returning waits for every spawned thread still
running (Proposed; Open decision 21), so a fire-and-forget `spawn` is
an ordinary program and the audit sees a quiescent heap.

A `Task` is `Send` when its result is (types §1.6), so one task may be
held, joined or awaited by several threads or tasks at once: `(plet ((a
(join t)) (b (join t))) (+ a b))` is accepted, and so are two `async`
bodies awaiting one task. The runtime, not the checker, keeps that
safe: a task is resumed by one driver at a time, every other holder
waits for its completion, and a task keeps a list of waiters rather
than one waker slot (types §8.8; types Open decision 27). A count on a
task is what keeps it allocated, never what serialises it.

### 3.13 `&`: in-out parameters, arguments and places

```
parameter: &sym | &sym: type
argument:  &place
place ::= sym                       ; a variable of type (Cell T)
        | (. sym field)             ; sym a variable of type (Cell S), S a struct; field of object type
```

Meaning (**Decided**, §5: copy-in, copy-out). Inside the callee an `&`
parameter `v` **is a cell** of type `(Cell T)` (Proposed representation;
Open decision 2): read it with `@v`, pass it on with `&v`, replace its
content with `(set! v e)`, update it in place with the primitives below.
The private cell is created by the caller at the call and lives exactly
for the call:

```
(f &x a)                    ; x : (Cell T), f's first parameter is &v: T
   copy-in:    t := a new private cell holding x's content        (move or +1, below)
   call:       (f t a)      ; the callee's v is t
   write-back: x's content := t's content (moved); t is freed
   result:     the call's value
```

The caller's `x` is untouched until the write-back, so a plain `@x` in
the same call stays valid (case 17). A read `@v` inside the callee is an
owned reference (+1), so a value read from `v` and still in use always
holds a count; that is what makes the loop in case 08 see the original
elements (types §6.6).

**An `&` parameter is not a value** (Proposed; Open decision 2). Inside
the callee, `v` may occur only as `@v` (that is, `(deref v)`), `&v`,
`&(. v f)`, or the first argument of `(set! v e)`; this holds in the
body and in every closure literal inside it. Any other occurrence (`v`
returned, stored, bound by `let`, passed as a plain argument, `(cell
v)`, `(weak v)`, ...) is the compile error `& parameter v used as a
value in f`. The private cell is a stack object of the caller's frame
that lives exactly for the call (types §8.6); a value that referred to
it would outlive it at the write-back.

**Copy-in: move or retain** (Proposed; types §6.6). The copy-in *moves*
the place's content into the private cell iff the place is **exclusive**
for this call; otherwise it *retains* (+1). A place `x` is exclusive iff
`x` is an `&` parameter of the enclosing `defun` or a `let` binding whose
initialiser is syntactically `(cell e)`; every occurrence of `x` in the
function is `&x`, `&(. x f)`, `@x` or the target of a `(set! x e)` (for
an `&` parameter this is already required), none of them inside a `fn`
or `async` literal (a `loop` body, §3.18, is part of the function, so
occurrences in it are ordinary); and `x` occurs in no other argument of
this call (a `set!` on `x` inside another argument counts). A place
`(. x f)` is exclusive iff `x` is and, at run time, the struct in `x` is
**takeable**: unique (count 1 and neither shared, immortal nor
stack-allocated, types §8.2) and no weak reference to it exists
(`HAS-WEAK` clear); a unique struct that has a weak
reference is acquired instead, because an upgrade during the call could
otherwise reach the taken field (types §6.6). Under these conditions no
expression can observe the place during the call, so the move is
unobservable; the interpreter and the compiler apply the same rule so
their allocations agree. A moved-out struct field is *taken* until the
write-back (types §6.6).

A self tail call of the enclosing `defun` (types §6.10) whose argument
for an `&` parameter `v` is `&v` itself, at `v`'s own position, forwards
the private cell: no copy-in, no write-back, and the call is a loop
iteration. A self call with any other `&` argument at an `&` position
(`&(. v f)`, a `let` cell, or `&v` at a different position) is an
ordinary call with copy-in and write-back.

**Unique update** (**Decided**, §5): the in-place primitives update the
object in a place when it is **unique**: its count is one and it is
neither shared, nor immortal (a literal, a `def` value, a named function
used as a value, and everything reachable from one: §3.16, §3.19), nor
stack-allocated; they copy first otherwise (types §6.6, §8.2). An
immortal object has no count to test, so an update through a place that
holds part of a literal always copies and the literal is never written
(proposed case 37):

| Primitive | Type | Effect |
|---|---|---|
| `(array-set! &a i x)` | `&(Array T) i64 T -> unit` | element `i` := `x` |
| `(set-field! &s field x)` | `&S F -> unit` for a struct `S` with field `field: F` | field := `x` |

Everything else that updates in place (`push!`, `pop!`, `map-put!`,
`append`, ...) is library code over these two, `&` and `set!`. Because a
retained copy-in leaves the count above one, the first update through a
non-exclusive place copies; a moved copy-in and every update after the
first copy are in place.

Static requirements, all checked on the surface form before desugaring:

1. **Distinct variables** (**Decided**, §5): the `&` arguments of one
   call must name distinct places (distinct variables, or distinct
   fields of distinct variables). Error: `variable x passed to more than
   one & parameter in call to f` (case 12). Passing `&x` and `@x` in one
   call is fine (case 17).
2. **No escaping capture** (**Decided**, §5): a closure that captures an
   `&` parameter must not escape the call. Error: `& parameter captured
   by escaping closure: v in f` (case 18). "Escapes" is decided by types
   §6.5; such a closure may only be called directly or passed to a
   `:borrow` parameter.
3. **Not in async functions** (**Decided**, §8): an `&` parameter on an
   async function, as §3.1 defines one (an `async` in the body mentions
   the parameter, or the body's value is an `async`), is an error: `&
   parameter in async function: v in f` (case 14: `fill`'s `async`
   mentions `&buf` and is the function's value). Checked before rule 2,
   so case 14 reports this text. An `async` that mentions no `&`
   parameter and is not the function's value does not make the function
   async, and the private cell cannot reach such a task any other way:
   no expression has the cell as its value (rule 5), and a closure that
   captures the parameter and is itself captured by the `async` is
   escaping (rule 2) and `local` (types §5).
4. The place must have a cell type whose content type unifies with the
   parameter's, else `& argument must be a cell variable or a field of a
   cell variable`. A plain argument at an `&` position, or `&x` at a
   plain position, is a type error.
5. **Not a value** (Proposed, above): an `&` parameter occurs only as
   `@v`, `&v`, `&(. v f)` or the target of `set!`. Error: `& parameter v
   used as a value in f`.

### 3.14 `async`, `await`

```
(async body)             ; type (Task T) where body : T
(await expr)             ; only inside async; expr : (Task T); result T
```

`async` creates a task: an object holding the body's captured variables
and its suspended state. Evaluating the form does not run the body; the
executor does, when the task is joined, awaited or spawned. `await`
suspends the task until the awaited task completes and yields its result
(owned). A task may be awaited by any number of tasks and joined by any
number of threads: the runtime resumes it on one thread at a time and
wakes every waiter when it completes (§3.12; types §8.8). `(yield)` is
a prelude task that completes on its next poll; `block-on` is `join`.

Ownership (**Decided**, §8, §3.4, §3.5): an `async` body is an **escaping
closure** whose captures must be **sendable**: every variable it captures
is retained when the task is created (this is "retained on entry" of
§8), and a task is a thread crossing (§3.4 lists "a task"), so a
capture of type `(Cell T)` is the error of case 13. Locals created inside
the body live in the task's frame and follow the ordinary scope rules
there; nothing is ever borrowed across an `await` (types §6.9). Every
capture is retained when the task is created and released when the task
is freed, whether or not it is used before the first `await`: evaluating
the form does not run the body, so no use in the body is safe to leave
uncounted (types §6.9). `&` parameters are not allowed on async
functions, as §3.1 defines them (§3.13 rule 3). `await` outside `async`, or inside a `fn`
nested in an `async`, is an error: `await outside async`; a `loop` body
(§3.18) inside the `async` is part of the task, so a loop may `await`
on every iteration.

### 3.15 `unsafe`, `extern`

```
(unsafe body)
(extern name (type*) -> type opt*)      ; opt: :varargs
```

`extern` declares a foreign function with the C calling convention; its
parameter and result types are scalars, `ptr` or `unit`. Calls to
externs and the raw-pointer builtins (`ptr+`, `load-i8` ... `load-ptr`,
`store-i8` ... `store-ptr`, `alloc`, `free`, `raw`, `raw-retained`,
`release-raw`) are permitted only lexically inside `unsafe`. Nothing
else changes inside `unsafe`: counting and type checking still apply to
fibber objects (**Decided**, §9).

A fibber object reaches foreign code only as an address. `(raw e)` is
the address of `e`'s object, valid for the duration of the enclosing
scope: a borrow, which is what §9 means by "borrows valid only for the
duration of the call" (**Decided**, §9). When the foreign interface keeps
the reference (a callback, a buffer the library owns), `(raw-retained
e)` retains `e` and returns its address; that count belongs to the
foreign side until `(release-raw p)` on the fibber side, or the
runtime's exported `fib_release` called from C, releases it (Proposed;
Open decision 20; types §6.13). No extern parameter is ever typed as an
object, so nothing else transfers a count across the boundary. `ptr` is
a scalar for the ownership rules and is not sendable (types §5).

### 3.16 `quote`, `quasiquote`, `defmacro`, `Form`

```
(quote form)          ; 'form : Form
(quasiquote form)     ; `form, with (unquote e) ,e and (unquote-splicing e) ,@e inside
(defmacro name (param*) body)
param ::= sym | ... sym            ; "... rest" binds the remaining forms as (Vec Form)
```

A macro is a fibber function from forms to a form, run at expansion
time. Its parameters have type `Form` (`(Vec Form)` for the rest
parameter) and its body has type `Form`. The body is ordinary fibber,
type-checked as a `defun` in a macro-time module and evaluated by the
reference interpreter's evaluator over that module (Proposed: macro-time
evaluation is then the same executable spec as run time; the compiler
may JIT it but must agree). A macro may use any function of a module the
current module requires, provided that module is already compiled.

`Form` is the built-in enum

```
(defenum Form
  (Sym name: str) (Kw name: str)
  (Int v: i64 width: keyword) (Flt v: f64 width: keyword)
  (Str v: str) (Chr v: char) (Bool v: bool) (Nil)
  (List items: (Vec Form)) (Vec items: (Vec Form)) (Map items: (Vec Form)))
```

Quasiquote is rewritten by the expander, not evaluated:

```
`atom                ⟹ (quote atom)
`(a ,b ,@cs d)       ⟹ (List (concat ['a] [b] cs ['d]))      ; b : Form, cs : (Vec Form)
`[ ... ]  `{ ... }   ⟹ the same with Vec / Map
```

`(gensym "prefix")` returns a fresh `Sym` that cannot collide with any
source symbol. There is no automatic hygiene (Proposed; Open decision 7).
Expansion is outermost-first, repeated until no macro call remains,
before name resolution and typing. A macro must be defined earlier in
the module, or in a required module, than its first use. A macro call
at top level may expand to several top-level forms by returning them as
`(do f₁ .. fₙ)`: a top-level `do` is spliced into the module as its
forms (§2; Proposed, Open decision 24). Top-level forms are expanded
and registered in order, a spliced sequence included: a `defstruct` is
registered with the expander before the next form is expanded, so a
spliced `(derive ..)` that follows a spliced `(defstruct ..)` sees the
struct. `(do)` splices nothing. So

```lisp
(defmacro defrecord (name fields)
  `(do (defstruct ,name ,fields) (derive Eq ,name)))
```

defines the struct and its `Eq` instance (proposed case 41).

Reflection at expansion time (Proposed; liar ADR 023 kept and extended,
Open decision 26), for a struct `Name` defined earlier in the module or
imported:

| Call | Returns |
|---|---|
| `(struct? 'Name)` | `bool`: whether `Name` names a struct |
| `(struct-fields 'Name)` | `(Vec Form)`: the field names as `Sym`s, in order |
| `(struct-params 'Name)` | `(Vec Form)`: the type parameters as `Sym`s, in order, including those synthesised from unannotated fields (§3.7); `[]` for a monomorphic struct |
| `(struct-field-types 'Name)` | `(Vec Form)`: the type of each field as a type form (`(Sym "i64")`, `(List [(Sym "Vec") (Sym "a")])`, ...), in field order; an unannotated field's type is its synthesised parameter |

Each of the last three is an error at expansion when `Name` is not a
struct. This is what `derive` uses (§4.4): `(derive P Name)` expands to
one `impl` whose head is `Name` applied to `(struct-params 'Name)`,
whose `:where` lists `(P t)` for every type parameter `t` that occurs
in a field type, and whose methods work field by field over
`(struct-fields 'Name)`. A field whose type is a compound over a
parameter (`(Vec a)`) is served by that type's own instance, whose
context reduces to the listed `(P a)` (types §3.3); a field whose type
is ground needs a ground instance. So for `(defstruct Pair (a b))`,
`(derive Eq Pair)` is

```lisp
(impl Eq (Pair a b) :where ((Eq a) (Eq b))
  (= (self y) (and (= (. self a) (. y a)) (= (. self b) (. y b))))
  (!= (self y) (not (= self y))))
```

which is the instance a programmer would write by hand, and `(= (Pair
1 "x") (Pair 1 "x"))` resolves through it to the `i64` and `str`
instances (proposed case 39). Without the two new calls no `derive`
could be written for a generic struct, which is every struct with an
unannotated field: an `impl` head must apply the constructor to its
parameters and its context must be declared (§3.10). For `Ord`, on a
struct or an enum, the context lists `(Eq t)` beside `(Ord t)`: the
body compares with `=` as well as `<`, and there are no supertraits in
v1 (types §4.1).

The same reflection exists for enums (Proposed; Open decision 27), for
an enum `Name` defined earlier in the module or imported, and for the
built-in `Option` and `Form`:

| Call | Returns |
|---|---|
| `(enum? 'Name)` | `bool`: whether `Name` names an enum |
| `(enum-params 'Name)` | `(Vec Form)`: the type parameters as `Sym`s, in order; `[]` for a monomorphic enum |
| `(enum-variants 'Name)` | `(Vec Form)`: one `(List [(Sym "Variant") type*])` per variant in declaration order, each field's type as a type form in field order (field names are not returned: variant patterns are positional, §3.6); a field-less variant is `(List [(Sym "Variant")])`, and `Option`'s empty variant is `(List [(Sym "nil")])` (§3.9) |

The last two are an error at expansion when `Name` is not an enum;
`struct?` and `enum?` are how a macro tells the two apart, since the
struct calls are errors on an enum and these on a struct. `(derive P
Name)` on an enum expands to one `impl` whose head is `Name` applied to
`(enum-params 'Name)`, whose `:where` lists `(P t)` for every parameter
that occurs in a variant field type, and whose methods match on the
variants, with gensyms for the pattern variables:

- `Eq`: `=` matches `self`, and in each clause matches `y`: the same
  variant → the fields compared pairwise with `=` and `and`ed (`true`
  for a field-less variant); any other variant → `false`, through a
  final `_` clause of the inner match, omitted when the enum has one
  variant (it would be redundant, §3.6). `!=` is `(not (= self y))`.
- `Ord`: `<` is true when `self`'s variant precedes `y`'s in
  declaration order, or both are the same variant and `self`'s fields
  precede `y`'s lexicographically (`<`, then `=`, field by field); the
  inner match enumerates the variants of `y` in order, so no `_` clause
  is needed; `<=`, `>`, `>=` follow from `<` and `=`.
- `Hash`: combines the variant's index with the hashes of its fields,
  as the struct derive combines its fields' hashes.
- `Show`: the variant name followed by the shown fields, as a call form.

The inner matches enumerate the variants, so the expansion is
quadratic in the variant count; that is the macro's cost, not the
programmer's. So for

```lisp
(defenum Shape (Circle r: f64) (Rect w: f64 h: f64))
(derive Eq Shape)
```

the expansion is (with `r`, `r2`, `w`, `w2`, `h`, `h2` standing for
gensyms)

```lisp
(impl Eq Shape
  (= (self y)
    (match self
      ((Circle r) (match y ((Circle r2) (= r r2)) (_ false)))
      ((Rect w h) (match y ((Rect w2 h2) (and (= w w2) (= h h2))) (_ false)))))
  (!= (self y) (not (= self y))))
```

which is what had to be written by hand for every sum type before
this, growing with the square of the variant count (proposed case 48).
A field-less enum is a scalar with built-in `Eq`, `Ord` (declaration
order), `Hash` and `Show` (types §2.12), so `derive` of any of the four
on it expands to `(do)`: the built-in instance is the one it would
produce. The prelude derives all four for `Option` and for `List`
(§4.4), which is why `(= (some 1) (some 1))` and `(= (list 1 2) (list 1
2))` need nothing from the programmer.

A macro that needs the constant `nil`, in a pattern or an expression,
writes `nil` in its template: the reader gives it the form `(Nil)`,
which the expander accepts as the constant wherever `nil` may stand
(§3.9); a macro that builds the pattern for `Option`'s empty variant
from `enum-variants` produces `(nil)`, the `(Variant)` spelling, which
is accepted too.

`(quote f)` and `` ` `` may appear in run-time code; the value has type
`Form` and is an **immortal** immutable object (§3.19; types §8.2): it
and everything reachable from it is static data with no count, so an
in-place update through a place holding part of it always copies
(§3.13).

### 3.17 `ns`

See §5.

### 3.18 `loop`, `recur`

```
(loop ((sym expr)*) body)
(recur expr*)                 ; only in tail position of the innermost enclosing loop body
```

`loop` binds its variables like a sequential `let` and evaluates `body`;
`recur` evaluates its arguments left to right, rebinds every loop
variable at once (the arity must match) and continues at the start of
`body`. The value of the form is the value of `body` when it finishes
without `recur`. `recur` must be in tail position of the loop body (the
last step of the body, the branches of a tail `if`/`match`, the body of
a tail `let`/`do`, transitively); anywhere else, outside any `loop`, or
inside a `fn` or `async` literal nested in the loop, it is the error
`recur outside loop` / `recur not in tail position` (Proposed; Open
decision 17).

Why a core form: the loop body is part of the enclosing function, not a
closure. Occurrences of an `&` parameter or a `let` cell in it are
ordinary occurrences for the exclusivity rule of §3.13, so
`(dotimes (i n) (push! &v i))` updates in place; and an `await` in a
loop body inside an `async` is inside that `async` (§3.14). Neither is
true of a loop spelled as a named `fn` and a self tail call, which is
why `while`, `dotimes` and `for-each` over a `range` are prelude macros
over `loop` (§4.4).

Ownership (types §6.10): the loop variables are owning slots, exactly
like the parameter slots of a self tail call; `recur` moves or retains
its arguments into them and releases the old values; a value that leaves
the loop as its result follows the scope-exit rule. A closure among the
arguments of a `recur` or of a self tail call, or a `let`-bound closure
passed there, is escaping whatever the callee's summary says: the jump
releases the scope that created it while the slot keeps it (types
§6.5; proposed case 49).

### 3.19 `def`

```
(def name expr)
(def name: type expr)
```

A top-level **constant**: `name` is bound in the module's namespace,
beside the functions, to the value of `expr`, which must be a
**constant expression** (Proposed; Open decision 25):

```
const ::= literal | 'form | nil | Variant             ; a field-less variant, bare (§3.9)
        | (Ctor const*) | (some const)                ; a struct or variant constructor
        | [const*] | {const const ...}                ; literal collections (§1.4)
        | name                                        ; an earlier def, or a named function (of this module or a required one) as a value
```

No call other than a constructor and the prelude calls that the
literal-collection rewrite introduces (`vec-empty`, `conj`, `map-empty`,
`assoc`, §1.4), and no `cell`, `atom`, `weak`, `fn`, `async`, `unsafe`
or `@`: a constant expression has no effect and builds only immutable
objects. Its type is inferred as for a `let` binding (types §2.16):
monomorphic, never generalised, and closed; `(def e [])` is the error
`def e has an unresolved type; annotate it`, and a form outside the
grammar is `def e: initialiser is not a constant expression`. A `def`
may name `def`s earlier in its module or in a required module and named
functions of its module (before or after it: names are bound before
any body is checked, §3.1) or of a required module; it may not be
redefined, and shares its namespace with `defun` (§5). A function with
`&` parameters is not a value (§3.1) and cannot be named. A `def` is
typed after the functions it names and before the functions that read
it: `def`s are nodes of the dependency order of types §3.5, so `(def
twice-fn double)` sees `double`'s scheme (proposed case 50), and a
`def` and a `defun` that depend on each other (`(def t [f])` with `f`
reading `t`) are the error `def t and defun f depend on each other`
(Proposed; Open decision 29). A `def` naming a generic function must
fix the instantiation with its annotation, since its type is closed:
`(def twice-fn double)` with `double : ∀a. (Num a) ⇒ (fn (a) a)` is
`def twice-fn has an unresolved type; annotate it`, and `(def twice-fn:
(fn (i64) i64) double)` is accepted; an omitted colour in a `def`
annotation means `send` (types §1.4).

Evaluation: before `main` runs, the `def`s of each module are evaluated
in source order, modules in dependency order (§5); the value and every
object reachable from it become **immortal** (types §8.2), exactly like
a literal: no count, never freed, never written in place (§3.13). A
named function as a value is its immortal constant closure (types
§8.4), the specialisation the `def`'s type selects for a generic one.
The compiler may evaluate the initialiser at compile time and emit the
graph as static data once lIR can hold it (types §8.11; in v1 the
module initialiser builds it, types §8.2); a constant expression has no
effect, so the two are indistinguishable. `name` in an expression is a read of the global
with no count operation (types §6.1: a global binding). Because its
objects are immutable and immortal it may be used from any thread and
inside any `fn` or `async` body without a `Send` check or a
share-marking walk (types §2.16, §5.5): like a named function it is a
global, not a capture. That is what makes a lookup table or a keyword
map usable from a `plet` without an atom (proposed case 42). Mutable
global state is not a `def`: it is an `(atom ..)` created in `main`, or
in a function it calls, and passed explicitly.

Why a core form: it binds a name (§4.1, criterion 2) and introduces the
one kind of binding that is never released. liar had no `def`, so its
libraries spelled every constant as a nullary function (`(defun
O_RDONLY () 0)`); for a scalar that costs nothing, for a table it
allocates on every call.

---

## 4. Core versus macro versus library

### 4.1 The rule (Proposed)

A form is **core** iff at least one of:

1. the checker must see it on the surface to decide a rule of ownership.md
   (`&`: distinct places, the escaping-capture rule; `fn`/`async`: capture
   and escape; `match`/`let` patterns: derived borrows and
   exhaustiveness; `.`: field paths);
2. it binds names or introduces a type, a protocol or an implementation
   (`defun def fn let defstruct defenum defprotocol impl defmacro extern
   ns`);
3. it changes control flow or the evaluation regime in a way no function
   can (`if do match loop recur async await unsafe quote`).

Everything with an ordinary call shape whose meaning is "evaluate the
arguments, then do something" is a **library function**, even when the
compiler implements it directly (a **builtin**) or it has its own typing
rule (a **primitive**). Everything that is a pure rewrite of forms
preserving verdicts and error positions is a **macro**.

### 4.2 Core forms

Twenty-two: `defun def fn let if do match loop recur defstruct . defenum
defprotocol impl & async await unsafe extern quote defmacro ns`
(`quasiquote`,
`unquote`, `unquote-splicing` exist only until expansion). `set!`,
`cell`, `deref`, `atom`, `swap!`, `reset!`, `weak`, `spawn`, `join` are
builtins: the checker knows them by name and type, but the reader and
expander treat them as calls.

### 4.3 Builtins and primitives (known to the checker by name)

| Group | Names |
|---|---|
| cells, atoms, weak | `cell deref set! atom swap! reset! weak` |
| threads, tasks | `spawn join trap` |
| arithmetic (protocol methods with built-in scalar instances, types §2.12) | `Num`: `+ - * / rem neg`; `Eq`: `= !=`; `Ord`: `< <= > >=`; `Bits` (integers): `bit-and bit-or bit-xor bit-not shl shr sar popcount`; `not` |
| conversions (target type first) | `trunc zext sext fptrunc fpext fptosi fptoui sitofp uitofp char->i32 i32->char` |
| arrays (types §2.13) | `array array-len array-get array-with array-copy array-set!` |
| structs | `set-field!` |
| strings | `str-len str-bytes str-concat str-slice str-eq starts-with?` and `Countable`/`Eq`/`Ord`/`Hash` instances |
| forms | `Form` constructors, `gensym`, `struct?`, `struct-fields`, `struct-params`, `struct-field-types`, `enum?`, `enum-params`, `enum-variants` (§3.16) |
| `Option` (built in, §3.9) | `some` (constructor), `nil` (a literal, §1.1); `nil?`, `some?`, `if-let` are prelude definitions (§4.4, §4.5) |
| unsafe | `ptr+ load-i8 load-i16 load-i32 load-i64 load-ptr store-i8 ... store-ptr alloc free raw raw-retained release-raw` |
| dynamic dispatch | `(dyn P e)` |

### 4.4 Prelude macros (normative list for the reference implementation)

| Macro | Expands to |
|---|---|
| `when`, `unless`, `cond`, `and`, `or` | `if` |
| `if-let`, `when-let`, `nil?`-free option tests | `match`: `(if-let (x e) a b)` ⟹ `(match e ((some x) a) (nil b))` |
| `list` | `(list a b)` ⟹ `(cons a (cons b empty))` (`empty` is `List`'s field-less variant, used bare: §3.9) |
| `plet` | §3.12 |
| `while`, `dotimes`, `for-each` over a `range` | `loop`/`recur` (§3.18): `(while c body)` ⟹ `(loop () (if c (do body (recur)) ()))`; `(dotimes (i n) body)` ⟹ `(loop ((i 0) (m n)) (if (< i m) (do body (recur (+ i 1) m)) ()))` with `m` a gensym; `(for-each (range a b) (fn (i) body))`, with a literal `(range ..)` and a literal `fn`, ⟹ `(loop ((i a) (m b)) (if (< i m) (do body (recur (+ i 1) m)) ()))`, the body spliced in with no closure; any other `for-each` is the library function (§4.5) |
| `->`, `->>`, `doto` | call rewriting |
| `assert`, `dbg` | `if` and `trap` |
| `derive` | `(derive P Name)`, for `P` one of `Eq`, `Ord`, `Hash`, `Show`: one `impl` whose head comes from `struct-params` or `enum-params`, whose `:where` lists `(P t)` for each parameter used by a field (`struct-field-types`, `enum-variants`; `(Eq t)` as well for `Ord`) and whose methods go field by field over `struct-fields` for a struct, or variant by variant over `enum-variants` for an enum, with a nested `match` on both operands (§3.16); `(do)` for a field-less enum, whose instances are built in; several protocols are several `derive` forms, which a macro may return in one top-level `do`. The prelude itself contains `(derive Eq Option)`, `(derive Ord Option)`, `(derive Hash Option)`, `(derive Show Option)` and the same four for `List` |

### 4.5 Library (written in fibber, in `lib/`)

`Option` helpers (`nil?`, `some?`, `unwrap-or`; the type itself is built
in, §3.9); `List` (`(defenum (List a)
(empty) (cons head: a tail: (List a)))`), with `Eq`, `Ord`, `Hash` and
`Show` derived for both (§4.4); `Vec`, `Map`, `Set` as
persistent structures over `(Array T)` with `vec-empty conj nth count
push! pop! vec-set! map-empty assoc get map-put! map-del!`; the
protocols `Seq Countable Indexable Collection Associative Traversable
Iter Hash Show` with `first rest count nth conj for-each map filter
reduce iter next collect filter-iter`; `range`, `pmap`, `append` (=
`push!`), `even?`, `length` (string length), `starts-with?`, `box`/`unbox`
over `(defstruct (Box a) (v: a))`, `yield`, `block-on`, I/O.

`for-each`, `map`, `filter`, `reduce`, `swap!` take their function
parameter `:borrow`; `pmap`'s function parameter escapes (its body
spawns closures that capture it), which is what makes case 13's closure
a thread crossing. An `&` update inside a closure passed to one of them
copies on its first update, because a place used inside a `fn` literal
is never exclusive (§3.13); in-place accumulation over a loop is written
with `loop`, `dotimes` or `while` (§3.18), whose bodies belong to the
enclosing function.

---

## 5. Modules

```
(ns name clause*)
clause ::= (:require [name :as alias]+) | (:use name+)
```

One `ns` form, first in the file, names the module (dotted; `a.b` lives
at `a/b.fib` under a root given to the compiler). `:require` makes
`alias/x` refer to `x` in that module; `:use` brings all of a module's
top-level names in unqualified. Every top-level definition is exported
(Proposed; no private names in v1). A name defined locally shadows a
`:use`d one; two `:use`d modules exporting the same name make that name
an error when referenced unqualified. Requires may not be cyclic.
`fib.prelude` is implicitly `:use`d. Protocol implementations are global
facts and are always visible once their module is required.

A module compiles against the **interfaces** of its requires and never
re-infers a dependency: every exported function's generalised type
scheme with colour constraints and escape summaries (types §3.9, §6.4),
every `def` with its closed type (its value is built by the defining
module's initialisation, §3.19), every type with its layout, every
protocol, every impl, every macro (as forms), and the bodies of generic
functions (needed for instantiation; types §4.3). Modules are compiled in dependency order; a recursive pair
of functions must live in one module.

---

## Appendix A — the 20 ownership cases in this syntax

Headers (the `;;` lines) are the originals, unchanged. Bodies are the
originals wherever this syntax admits them; where the rewrite is more
than a spelling change, a note follows the case and types §7 names the
rule that produces the verdict. Library names used are listed in §4.5.

### 01-return-part-of-argument.fib

```lisp
;; spec:   §4
;; expect: accept
;; result: 1
;; audit:  clean
;; A function returns memory owned by its argument. The callee retains
;; it on return, so the caller's binding and the list each hold a count.
(defun head (xs) (first xs))

(defun main () -> i64
  (let ((l (list (box 1) (box 2))))
    (let ((h (head l)))
      (unbox h))))
```
Unchanged. `list` is the prelude macro (`(cons (box 1) (cons (box 2)
empty))`, §4.4), `box`/`unbox` the prelude `Box` struct, `first` the `Seq` method (traps on an empty list; Open decision
13). `head` is inferred as `∀s e. (Seq s e) ⇒ (fn (s) e)`.

### 02-structural-sharing.fib

```lisp
;; spec:   §5
;; expect: accept
;; result: 7
;; audit:  clean
;; The new vector shares nodes with v, and outlives v.
(defun add4 (v) (conj v 4))

(defun make ()
  (let ((v [1 2 3]))
    (add4 v)))

(defun main () -> i64
  (let ((w (make)))
    (+ (count w) (nth w 2))))
```
Unchanged.

### 03-store-borrowed-value.fib

```lisp
;; spec:   §3.2
;; expect: accept
;; result: 5
;; audit:  clean
;; A borrowed value is stored into a collection that outlives it, and
;; the same value goes into two collections.
(defun remember (coll item) (conj coll item))

(defun main () -> i64
  (let ((keep (let ((s "hello"))
                (let ((a (remember [] s))
                      (b (remember [] s)))
                  a))))
    (length (nth keep 0))))
```
Unchanged. `length` is the prelude string length.

### 04-branch-dependent-owner.fib

```lisp
;; spec:   §4
;; expect: accept
;; result: 5
;; audit:  clean
;; One branch returns the borrowed argument, the other a fresh object.
;; Both are returned owned (+1), so the caller treats them the same.
(defun pick (flag x) (if flag x "fresh"))

(defun main () -> i64
  (let ((s "hello"))
    (let ((a (pick true s))
          (b (pick false s)))
      (length a))))
```
Unchanged.

### 05-closures-share-state.fib

```lisp
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; Two escaping closures share one captured mutable variable.
(defun make-counter ()
  (let ((n (cell 0)))
    (list (fn () (set! n (+ @n 1)) @n)
          (fn () @n))))

(defun main () -> i64
  (let ((c (make-counter)))
    (let ((inc (nth c 0))
          (get (nth c 1)))
      (inc)
      (inc)
      (get))))
```
Change: the first closure returns `@n` after the `set!`, so both closures
have the type `(fn () i64)` and the list is homogeneous (`set!` returns
`unit`; Open decision 5). The shape is unchanged: one cell, two escaping
closures sharing it.

### 06-capture-borrowed-param.fib

```lisp
;; spec:   §3.3
;; expect: accept
;; result: 1
;; audit:  clean
;; An escaping closure captures a borrowed parameter; the object it was
;; borrowed from dies before the closure is called.
(defun matcher (prefix) (fn (s) (starts-with? s prefix)))

(defun main () -> i64
  (let ((m (let ((p "ab")) (matcher p))))
    (if (m "abc") 1 0)))
```
Unchanged.

### 07-recursive-accumulator.fib

```lisp
;; spec:   §4, §5
;; expect: accept
;; result: 100000
;; audit:  clean
;; Each call's accumulator is a new version sharing structure with the
;; previous one, passed through a tail call.
(defun build (n acc)
  (if (= n 0) acc (build (- n 1) (conj acc n))))

(defun main () -> i64
  (count (build 100000 [])))
```
Unchanged. The self tail call is interpreted and compiled as a loop
whose parameter slots own their values (types §6.10); each iteration
releases the previous version.

### 08-mutate-while-iterating.fib

```lisp
;; spec:   §5
;; expect: accept
;; result: 6
;; audit:  clean
;; The loop is still reading v when append replaces it. The iteration
;; holds a count, so append copies instead of updating in place, and the
;; loop sees the original three elements.
(defun dup-all (&v)
  (for-each @v (fn (x) (append &v x))))

(defun main () -> i64
  (let ((v (cell [1 2 3])))
    (dup-all &v)
    (count @v)))
```
Change: inside `dup-all` the `&` parameter is a cell, so the vector it
holds is read with `@v` (§3.13). That read is the retained reference
that makes the iteration hold a count. `append` is the library
in-place push.

### 09-iterator-outlives-source.fib

```lisp
;; spec:   §3.1
;; expect: accept
;; result: 2
;; audit:  clean
;; The returned iterator refers to a vector that the caller's scope
;; has already let go of.
(defun evens (v) (filter-iter (iter v) even?))

(defun main () -> i64
  (let ((it (let ((v [1 2 3 4])) (evens v))))
    (count (collect it))))
```
Unchanged. `iter` stores `v` into the iterator struct; that store is the
escape that retains it.

### 10-atom-old-value.fib

```lisp
;; spec:   §7
;; expect: accept
;; result: 1000
;; audit:  clean
;; Readers hold old values while writers replace them.
(defun main () -> i64
  (plet ((a (atom [])))
    (pmap (fn (i)
            (let ((snapshot @a))
              (swap! a (fn (c) (conj c i)))
              (count snapshot)))
          (range 1000))
    (count @a)))
```
Unchanged. The result of `pmap` is a discarded `do` step.

### 11-borrow-across-await.fib

```lisp
;; spec:   §8
;; expect: accept
;; result: 5
;; audit:  clean
;; The parameter is used after the await, so it is retained on entry
;; and outlives the caller's scope.
(defun measure (s) (async (await (yield)) (length s)))

(defun main () -> i64
  (let ((task (let ((s "hello")) (measure s))))
    (block-on task)))
```
Unchanged. Every capture of an `async` is retained at creation whether
or not it is used before the first `await` (§3.14); the proposed case
that uses `s` only before the `await` is in
`spec/drafts/PROPOSED_CASES.md`.

### 12-reject-same-binding-twice-inout.fib

```lisp
;; spec:   §5 (proposed)
;; expect: reject
;; error:  passed to more than one & parameter
(defun bar (&a &b) (append &a 1) (append &b 2))

(defun main () -> i64
  (let ((x (cell [3 4])))
    (bar &x &x)
    (count @x)))
```
Unchanged.

### 13-reject-cell-crosses-thread.fib

```lisp
;; spec:   §7
;; expect: reject
;; error:  cell cannot be shared between threads
(defun main () -> i64
  (let ((n (cell 0)))
    (pmap (fn (i) (set! n (+ @n i))) (range 10))
    @n))
```
Unchanged.

### 14-reject-inout-in-async.fib

```lisp
;; spec:   §8
;; expect: reject
;; error:  & parameter in async function
(defun fill (&buf) (async (await (yield)) (append &buf 1)))

(defun main () -> i64
  (let ((b (cell [])))
    (block-on (fill &b))
    (count @b)))
```
Unchanged. `fill` is an async function under §3.1 on both counts: its
`async` mentions `&buf`, and it is the function's value; rule 3 of
§3.13 fires before anything else. A function that only drives a task to
completion inside the call and mentions no `&` parameter in it is not
async (proposed case 38).

### 15-cycle-through-cell-leaks.fib

```lisp
;; spec:   §6 (proposed)
;; expect: accept
;; result: 1
;; audit:  leak-cycle
;; A cell that ends up holding a vector containing itself. Under the
;; proposed rule this leaks, and the audit must report it as a cycle
;; leak, not as any other failure.
(defstruct Knot (items: (Cell (Vec Knot))))

(defun main () -> i64
  (let ((k (Knot (cell []))))
    (set! (. k items) [k])
    (count @(. k items))))
```
Change: the original `(set! c [c])` needs the type `(Cell (Vec X))` with
`X` equal to itself, which the occurs check rejects (types §3.2). The
knot is tied through a nominal struct instead. The cycle `k → cell →
vector → k` still passes through exactly one cell, so the verdict is
unchanged (Open decision 14).

### 16-coordinated-update-single-atom.fib

```lisp
;; spec:   §7
;; expect: accept
;; result: 200
;; audit:  clean
;; Two balances that must change together live in one atom and change
;; with one swap!, so no reader can see the money in flight.
(defstruct Accounts (a: i64 b: i64))

(defun transfer (accts amount)
  (swap! accts (fn (x) (Accounts (- (. x a) amount) (+ (. x b) amount)))))

(defun main () -> i64
  (plet ((accts (atom (Accounts 100 100))))
    (pmap (fn (i)
            (transfer accts 1)
            (let ((snap @accts)) (+ (. snap a) (. snap b))))
          (range 1000))
    (let ((final @accts))
      (+ (. final a) (. final b)))))
```
Unchanged. `(transfer accts 1)` is a discarded `do` step whose owned
result is released at the step's end.

### 17-inout-and-borrow-same-call.fib

```lisp
;; spec:   §5
;; expect: accept
;; result: 4
;; audit:  clean
;; The same variable passed in-out and as a plain borrow in one call.
;; The caller's variable is untouched until the write-back, so the
;; borrow stays valid for the whole call.
(defun push-count (&v x) (append &v (count x)))

(defun main () -> i64
  (let ((v (cell [1 2 3])))
    (push-count &v @v)
    (count @v)))
```
Unchanged.

### 18-reject-inout-captured-by-escaping-closure.fib

```lisp
;; spec:   §5
;; expect: reject
;; error:  & parameter captured by escaping closure
(defun make-pusher (&v) (fn (x) (append &v x)))

(defun main () -> i64
  (let ((v (cell [])))
    (let ((push (make-pusher &v)))
      (push 1)
      (count @v))))
```
Unchanged.

### 19-weak-parent-pointer.fib

```lisp
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; A tree whose nodes point at their parent through a weak reference.
;; The strong edges form no cycle, so the whole tree is freed when main
;; returns.
(defstruct Node (parent: (Option (Weak Node)) children: (Cell (Vec Node))))

(defun add-child (parent)
  (let ((c (Node (some (weak parent)) (cell []))))
    (set! (. parent children) (conj @(. parent children) c))
    c))

(defun depth (n: Node) -> i64
  (match (. n parent)
    (nil 0)
    ((some w) (if-let (p @w) (+ 1 (depth p)) 0))))

(defun main () -> i64
  (let ((root (Node nil (cell []))))
    (let ((a (add-child root)))
      (let ((b (add-child a)))
        (depth b)))))
```
Changes: `Node` is recursive, so its fields are annotated (§3.7); a
missing parent is `(Option (Weak Node))`, so the root's `nil` and a
child's `(some (weak parent))` have one type; `@w` on a weak reference
yields `(Option Node)`, eliminated with `if-let` (a `match`) in place of
`nil?` plus a use of the possibly-nil value; `depth` annotates `n`
because a field access is its only clue to `n`'s type (types §3.4).
`add-child` needs no annotation: `(weak parent)` unifies with `(Weak
Node)`.

### 20-weak-ref-to-dead-object.fib

```lisp
;; spec:   §6
;; expect: accept
;; result: 1
;; audit:  clean
;; The only strong reference dies with the inner let; deref then gives
;; nil rather than the freed object.
(defun main () -> i64
  (let ((w (let ((v (conj [] 1))) (weak v))))
    (if (nil? (deref w)) 1 0)))
```
Unchanged. `nil?` is the prelude `(defun nil? (o) (match o (nil true)
((some _) false)))`.

---

## Open decisions (syntax)

Each needs the owner's sign-off. Recommendation first, alternative
second. Decisions that also affect the checker are cross-referenced to
types.md.

1. **`&` only on `defun` parameters; `fn` has none.** Recommend: yes;
   closures keep one calling convention and the escaping-capture rule
   stays a check on captures. Alternative: `&` on `fn` parameters, which
   puts write-back slots into closure types.
2. **`&` parameters are cells inside the callee, read with `@v`, and
   are not values.** Recommend: yes; cells are then the single mutable
   thing the checker knows, the copy-in/copy-out of §5 is the whole
   semantics, and the retained read is what makes case 08 sound by
   construction (types §6.6). `v` occurs only as `@v`, `&v`, `&(. v f)`
   or the target of `set!` (§3.13): the private cell is a stack object
   of the caller's frame, so a `v` that could be returned, stored or
   captured as a value (`(defun leak (&v) v)`) would dangle after the
   write-back. Cost: case 08 writes `(for-each @v ...)`. Alternative:
   auto-deref (`v` reads the value, as the original case is written),
   which is the same ownership rule under a second kind of variable in
   every rule.
3. **Copy-in moves when the place is exclusive, else retains (§3.13).**
   Recommend: yes; it is the only route to in-place updates through
   user-defined `&` functions, the condition is syntactic (plus a
   count test for fields), and the interpreter implements the same rule
   so allocations match. Alternative: always retain, so the first update
   in every `&` call copies (types Open decision 3).
4. **`&(. x f)` places (a mutable borrow of one field of a unique
   struct).** Recommend: yes; it is what lets `Vec`'s tail be updated in
   place from library code. Alternative: cell variables only, with
   `Vec`/`Map` as runtime builtins whose in-place paths are not written in
   fibber.
5. **`set!` and `reset!` return `unit`; `swap!` returns the new value.**
   Recommend: as stated (one extra `@n` in case 05). Alternative: `set!`
   returns the stored value (case 05 verbatim), at the cost of a
   retain/release pair per discarded `set!` in the interpreter.
6. **Shadowing allowed; liar ADR 006 dropped.** Recommend: yes; nested
   `let`s and macro-introduced bindings need it and the checker keys on
   binding sites. Alternative: keep the ban.
7. **Unhygienic macros with `gensym`, run by the reference interpreter,
   with struct reflection (item 26) and multi-form top-level expansion
   through a spliced `do` (item 24).** Recommend: yes. Alternative:
   renaming hygiene (a much larger expander) or no reflection (no
   `derive`).
8. **Literal vectors and maps desugar to `conj`/`assoc` chains after
   expansion; inside quotes they stay `Vec`/`Map` forms.** Recommend: yes.
   Alternative: variadic `vector`/`hash-map` core forms.
9. **`Option` behaves as an ordinary enum (it is built in only because
   `nil` is a literal, item 28); no implicit lifting, no `nil?`
   narrowing in `if`.** Recommend: yes; every coercion the inference
   draft proposed was shown to make acceptance order-dependent, and
   `if-let` covers the idiom. Alternative: `T ↝ (Option T)` at argument
   positions plus narrowing (types Open decision 12).
10. **No literal polymorphism: `1 : i64`, `1.0 : f64`, widths by suffix.**
    Recommend: yes; no defaulting search, and `(+ x 1)` pins `x` to
    `i64`, which is what liar's prelude assumed anyway. Alternative:
    `Num a ⇒ a` literals with defaulting.
11. **No vector patterns and no `:when` guards in `match` (v1).**
    Recommend: yes; `Vec` is a library type and guards need fallthrough.
    Alternative: add both once `Vec` has a core view.
12. **Variant names unqualified and unique per namespace.** Recommend:
    yes. Alternative: `Name/variant`.
13. **`first`/`nth` trap on an empty or out-of-range collection.**
    Recommend: yes (keeps case 01 as written); `first?`/`nth?` return
    `(Option T)`. Alternative: `first : (Option e)` always, and case 01
    gains a `match`.
14. **Cases 05, 08, 15 and 19 are rewritten as shown.** Sign off that
    each still exercises its rule: two escaping closures sharing a cell;
    an iteration holding a count while `&` updates copy; a cycle through
    exactly one cell; a weak back-pointer that is `nil` for the root and
    `nil` after the target dies.
15. **Non-final `do` steps may have any type.** Recommend: yes (case 16
    and every `pmap` body discard a value). Alternative: `unit` only,
    with a `(discard e)` builtin.
16. **Signed integer overflow wraps (two's complement).** Recommend: yes
    for v1: it is what lIR emits and what the interpreter can match
    exactly. Alternative: trap on overflow (needs lIR to expose the
    overflow intrinsics) with `wrapping-*` variants.
17. **`loop`/`recur` are core forms (§3.18); `while`, `dotimes` and
    `for-each` over a range expand to them.** Recommend: yes; a loop
    body is then part of its function, so an `&` place updated in a loop
    stays exclusive and updates in place, and an `await` in a loop inside
    an `async` is legal. Alternative: keep loops as named `fn`s, let the
    exclusivity rule see through non-escaping closures with the extra
    condition that no other argument of the call can reach a closure
    capturing the place (a re-entrancy check), and let the `await` check
    see through a named `fn` called only at its literal.
18. **A field-less variant is written bare in expressions (`empty`,
    `nil`) and as `(Variant)` in patterns (§3.6, §3.9).** Recommend: yes;
    `nil` already works this way, and `(empty)` was a call to a value of
    non-function type that the typing rules reject, which broke the
    normative `list` expansion used by cases 01 and 05. Alternative:
    accept `(V)` as the constant in expressions too.
19. **Instance contexts are declared with `:where` on `impl` (§3.10).**
    Recommend: yes; a `defun` that calls a method is generalised before
    any impl body is checked (types §3.5), so the context must exist
    without inferring the bodies, exactly as method signatures must.
    Alternative: infer contexts by putting impl bodies into the call
    graph (types §10 item 20).
20. **`raw-retained`/`release-raw` replace `extern`'s `:retains`
    (§3.15).** Recommend: yes; extern positions are scalars, so
    `:retains` named parameters that cannot exist. Alternative: object
    types allowed at `:retains` positions of an extern signature, with
    the foreign side calling `fib_release`.
21. **`main` returning waits for every spawned thread (§3.12).**
    Recommend: yes; a discarded task then finishes and the audit sees a
    quiescent heap. Alternative: detach on exit and exclude the running
    threads' objects from the audit.
22. **A `defun` is an async function iff an `async` in its body
    mentions one of its `&` parameters or is the body's value (§3.1,
    §3.13 rule 3).** Recommend: yes; the earlier definition (any
    `defun` containing an `async`) rejected `(defun bump (&v) (let ((n
    (block-on (async ..)))) (push! &v n)))`, an in-out buffer filled
    from a task's result, although no frame outlives the call and the
    task cannot reach the private cell: no expression has the cell as
    its value (rule 5), and a closure that captures it and is captured
    by the `async` is escaping (rule 2) and `local` (types §5). Case 14
    keeps its verdict on both counts. Alternative: keep the broad
    definition and write such functions with a plain parameter and a
    returned value.
23. **`:borrow` on `defun` parameters, spelled `p :borrow` or `p: T
    :borrow`, with the same flat spelling for `defprotocol` (§3.1,
    §3.10).** Recommend: yes; types §6.4 already checked the annotation
    and the grammar had no place for it, and the flat spelling is what
    `x: T` already is (two forms; here three). Alternative: delete the
    annotation from types §6.4 and let summaries be inference results
    only, which leaves no way to make a summary part of an interface.
24. **A top-level `(do f₁ .. fₙ)`, written or produced by a macro,
    splices into the module as n top-level forms expanded in order (§2,
    §3.16).** Recommend: yes; a macro returns one `Form`, and
    `defrecord`-style macros and any macro that emits a definition plus
    its instances need to return several; `do` had no top-level meaning
    to lose. Alternative: a dedicated head such as `splice`, which keeps
    `do` uniform at the cost of a name whose only meaning is at top
    level.
25. **`(def name expr)` for constant expressions, evaluated once before
    `main`, immortal (§3.19).** Recommend: yes; it reverses the
    synthesis's rejection of `defconst` ("not needed by any case"):
    liar's libraries spell every constant as a nullary function, which
    for a table or a keyword map allocates on every call and cannot be
    shared across threads without a `plet`, while the immortal
    mechanism the runtime already has for literals covers it exactly.
    Alternative: no `def` (constants stay nullary functions), or a
    `defconst` restricted to scalars.
26. **Expansion-time reflection also returns type parameters and field
    types (`struct-params`, `struct-field-types`), and `derive` emits
    the head and the `:where` context from them (§3.16, §4.4).**
    Recommend: yes; without them `derive` cannot be written for any
    generic struct, which includes every struct with an unannotated
    field (§3.7), because an `impl` head must apply the constructor to
    its parameters and its context must be declared (item 19).
    Alternative: `(derive Eq (Pair a b) :where ((Eq a) (Eq b)))` with
    the head and context written by the programmer, or inferred
    contexts for derived impls only (types §10 item 20's alternative,
    for one macro).
27. **Expansion-time reflection for enums (`enum?`, `enum-params`,
    `enum-variants`), `derive` extended to enums with a nested `match`
    on both operands, built-in `Eq`/`Ord`/`Hash`/`Show` for field-less
    enums, and the prelude deriving all four for `Option` and `List`
    (§3.16, §4.4; types §2.12).** Recommend: yes; without it `Eq`,
    `Ord`, `Hash` and `Show` for every sum type, the library's `Option`
    and `List` included, had to be written by hand as nested matches
    growing with the square of the variant count, while structs got
    them from one `derive`. Alternative: `derive` for structs only,
    with the library instances written by hand.
28. **`Option` is built in, not declared in the prelude; `nil` is a
    reader literal whose form `(Nil)` the expander accepts as the
    constant in expressions and patterns, and a macro-built symbol
    `nil` or pattern `(nil)` means the same (§1.1, §3.9, §3.16).**
    Recommend: yes; the declaration `(defenum (Option a) (nil) (some
    v: a))` cannot be read, since `nil` reads as `(Nil)`, and making the
    expander accept the reader's form is what lets a macro emit `nil`
    through a quasiquote without a special case. Alternative: read
    `nil` as the symbol `(Sym "nil")` and drop `Nil` from `Form`, so
    that `Option` becomes a prelude enum with a variant named `nil`; a
    larger change to the reader tables for the same programs.
29. **`def`s are nodes of the dependency order: a `def` is typed after
    the functions it names and before the functions that read it, a
    `def`–`defun` cycle is an error, and a `def` naming a generic
    function is annotated (§3.19; types §2.16, §3.5).** Recommend:
    yes; a `def` was typed before any `defun` had a scheme, so the
    grammar's "a named function as a value" could not be typed at all,
    an unannotated generic function has no closed type to give a
    `def`, and "contains no other call" was false for every literal
    collection, which §1.4 rewrites into prelude calls. Alternative:
    type a `def`–`defun` cycle as one SCC (the `def` then makes its
    functions monomorphic in it), or forbid naming functions in a `def`
    and spell function tables as nullary functions.
