# fibber syntax (draft: inference-first)

Status: draft for sign-off. Every decision below is marked **Decided**
(a direct consequence of [spec/ownership.md](../../ownership.md), which
is authoritative) or **Proposed** (everything else). Verdicts of the
cases in `cases/ownership/` are fixed; the surface syntax in this file
is what those cases will be rewritten into (Appendix A) once the
Proposed items are signed off (§9).

The companion [types.md](types.md) gives the typing rule and the
ownership rule for each form here. This file gives the reader, the
grammar and the evaluation rule.

Notation: `⟨x⟩*` zero or more, `⟨x⟩+` one or more, `⟨x⟩?` optional.
`e` is an expression, `p` a pattern, `T` a type (type syntax is in
types.md §1), `x` a symbol.

## 1. The reader

**Proposed** unless marked otherwise. The reader turns text into
*syntax values*: the built-in sum type `Syntax` (§6.4). Macros are
functions from `Syntax` to `Syntax`; nothing in the reader knows about
special forms.

| Text | Syntax value | Notes |
|------|--------------|-------|
| `42`, `-7`, `0x1F`, `0b101`, `1_000` | `(Int i)` | 64-bit. Overflow of the literal is a read error. Type `i64` (types.md §1.1). |
| `3.14`, `1e-9`, `-0.5` | `(Float f)` | IEEE double. A literal with `.` or exponent. Type `f64`. |
| `(i32 42)`, `(f32 1.5)`, `(i8 -1)` | call syntax | Not a reader form: a sized-literal *core form* (§4.1). |
| `"hello\n"` | `(Str s)` | UTF-8. Escapes: `\n \t \r \\ \" \0 \u{1F600}`. Type `str`. |
| `\a`, `\newline`, `\space`, `\tab`, `\u{41}` | `(Char c)` | One Unicode scalar value. Type `char`. |
| `foo`, `set!`, `nil?`, `+`, `->`, `m/foo` | `(Sym name)` | Symbol chars: anything but whitespace, `()[]{}"';` `` ` `` `,` `@` `\` `:`-prefix. `/` separates a module qualifier once. |
| `:foo`, `:my/key` | `(Kw name)` | Interned. Type `keyword`. |
| `nil` | `(Sym "nil")` | Resolves to the core constant `nil : (? a)` (types.md §1.6). |
| `true`, `false` | `(Sym ...)` | Resolve to the core constants of type `bool`. |
| `(a b c)` | `(List [a b c])` | Application or special form after resolution. |
| `[a b c]` | `(Vec [a b c])` | Vector literal: evaluates to a persistent vector (§4.1). |
| `{k v k v}` | `(Map [k v k v])` | Map literal; odd count is a read error. |
| `'e` | `(List [(Sym "quote") e])` | |
| `` `e `` | `(List [(Sym "quasiquote") e])` | |
| `,e` | `(List [(Sym "unquote") e])` | |
| `,@e` | `(List [(Sym "unquote-splicing") e])` | |
| `@e` | `(List [(Sym "deref") e])` | Reader macro. `deref` is a library protocol method (§6). |
| `&x` | `(List [(Sym "&") (Sym x)])` | Reader macro. Only meaningful as a `defun` parameter or a call argument (§4.7). `&` must be followed by a symbol without whitespace. |
| `_` | `(Sym "_")` | Wildcard in patterns; an error as an expression. |
| `; ...` to end of line | dropped | Comment. |
| `#_ e` | dropped | Discards the next form. |
| `#| ... |#` | dropped | Block comment, nests. |

Rules:

- Whitespace and `,`-free commas: `,` is unquote, never whitespace
  (unlike Clojure). **Proposed.**
- Case-sensitive. `Node` and `node` differ. Type and constructor names
  are conventionally capitalised but the reader does not care.
- A symbol containing `:` other than as its first character is a read
  error, except the annotation form `x:` (a symbol ending in `:`) which
  the reader keeps as `(Sym "x:")`; `defun`, `fn`, `let`, `defstruct`
  and `defenum` interpret `x: T` as "x annotated with T" (§4.2).
- `->` is an ordinary symbol; `defun` and protocol signatures interpret
  it as the return-type marker.
- Every source file is a sequence of top-level forms; the first must
  be `(module ...)` (§7) unless the file is a script (then it is
  implicitly `(module main)`).

## 2. Evaluation model

**Decided** (ownership.md §1, §2): scalars are copied; objects are
heap (or stack, when the compiler proves it safe) and counted when
they escape. The reference interpreter counts every reference; the
compiler counts fewer. Neither is visible to the programmer except
through the four compile errors of §3–§8 of ownership.md and the audit.

**Proposed** evaluation order:

1. Within a list form, subforms are evaluated left to right, then the
   call is made. This includes arguments to constructors, `set!`, and
   operands of `if`'s test before either branch.
2. `let` bindings are evaluated top to bottom; each sees the earlier
   ones. `do` bodies left to right. `match` clauses are tried top to
   bottom; the first matching clause's body runs.
3. `plet` initialisers run in parallel with each other; their relative
   order is unspecified; the body runs after all have finished.
4. `pmap` applies its function to elements in an unspecified order and
   returns results in element order.
5. `if` evaluates exactly one branch. `and`/`or` (macros) short-circuit.
6. Temporaries (owned results not bound to a name) live until the end
   of the enclosing *statement*: the enclosing `let` binding form, `do`
   step, `set!`, or function body expression, whichever is innermost.
   This is when the compiler releases them (types.md §6.2).
7. A `&` argument's write-back happens after the callee returns and
   before the call expression's value is used (ownership.md §5).

There is no truthiness: `if` takes a `bool`. `nil` is not false and
`0` is not false.

## 3. Definitions: top-level forms

A module is a sequence of top-level forms. Only these are allowed at
top level: `module`, `import`, `defun`, `defstruct`, `defenum`,
`defprotocol`, `extend-protocol`, `defmacro`, `extern`, `defconst`.
Order matters only for macros (a macro must be defined before use,
§4.13) and for `defstruct`/`defenum` referenced by macros; `defun`s in a
module may refer to each other in any order (types.md §3.5).

### 3.1 `defun` — **Proposed** (form), **Decided** (parameter and result ownership)

```
(defun name (param*) ⟨-> T⟩? body+)
param ::= x | x: T | &x | &x: T
```

Defines a global, named function. Names are global and capture
nothing (ownership.md §6), so a `defun` is never a closure.

- Every parameter is a borrow (ownership.md §4). Every result is owned.
- `&x` declares a copy-in, copy-out parameter (ownership.md §5; §4.7
  below).
- `body+` is an implicit `do`; the value of the last form is returned.
- Recursion, including mutual recursion within a module, needs no
  forward declaration.
- `-> T` is optional; `main` must be declared `-> i64`.
- A `defun` with a `&` parameter is not a first-class value: it can be
  called but not passed or stored (types.md §2.1).

### 3.2 `defstruct` — **Proposed**

```
(defstruct Name (field*))
(defstruct (Name a b ...) (field*))      ; generic
field ::= x: T | x
```

Defines a nominal product type, its constructor `(Name e1 ... en)`
(positional, in declaration order), and field access `(. e x)`.

- A field without a type annotation is given a fresh type parameter,
  appended to the struct's parameter list in field order. So
  `(defstruct Pair (a b))` means `(defstruct (Pair a b) (a: a b: b))`.
- A struct that refers to itself (directly or through another type)
  must annotate the recursive field; unannotated recursion is rejected
  at definition time ("recursive struct field must be annotated").
- Struct values are immutable objects (ownership.md §1). A field of
  type `(Cell T)` holds a cell; the cell is mutable, the struct is not.
- `(. e x)` is core syntax (§4.9), not a function.

### 3.3 `defenum` — **Proposed**

```
(defenum Name variant+)
(defenum (Name a b ...) variant+)
variant ::= (V T*)  |  V
```

Defines a nominal sum type with positional variant payloads. `(V e*)`
constructs; `V` alone (no payload) is a constant. Variants are
matched with `match` (§4.5). Variant names live in the same namespace
as functions; a variant may not share a name with a struct or a
`defun` in the same module.

`(? T)` (option) is the built-in `(defenum (? a) (some a) nil)`; `nil`
is its payload-less variant and `some` its constructor (types.md §1.6).

### 3.4 `defprotocol` and `extend-protocol` — **Proposed**

```
(defprotocol (P self det*) sig+)
(defprotocol P sig+)                      ; ≡ (defprotocol (P self) sig+)
sig ::= (m [T+] -> T)  |  (m [T+] -> T (default body+))
```

- `self` is the *dispatch parameter*: the type the implementation is
  chosen by. `det*` are *determined parameters*: types fixed by `self`
  (one implementation per `self` head, and it fixes them). Example:
  `(defprotocol (Seq s e) (first [s] -> e) (rest [s] -> (? s)))`.
- Every signature is fully typed. Type variables in a signature other
  than the protocol's parameters are universally quantified per method.
  The first parameter type of every method must mention `self`.
- Method parameters are borrows and results are owned, like `defun`.
  A method may declare that an implementation is allowed to retain a
  parameter with `^retain` before the type: `(conj [self ^retain a] -> self)`.
  Without it, no implementation may make that parameter escape
  (types.md §6.3). **Proposed.**
- `(default body+)` gives a default implementation, used by an
  `extend-protocol` that omits the method.

```
(extend-protocol P Type impl*)
(extend-protocol (P Type Det*) impl*)     ; protocols with determined params
impl ::= (m (x+) body+)
```

`Type` is a type constructor applied to distinct type variables (e.g.
`(Vec a)`), or a scalar, or a struct/enum name. One `extend-protocol`
per (protocol, head constructor) in the whole program; overlap is an
error. Implementations may be given for types defined in other
modules. A protocol may require others: `(defprotocol (Ord a) :requires (Eq a) ...)`.

### 3.5 `defmacro` — **Proposed**

```
(defmacro name (param*) body+)
param ::= x | ... x           ; `... rest` binds remaining forms as (Vec Syntax)
```

See §4.13.

### 3.6 `extern` — **Proposed**

```
(extern c-name (T*) -> T)
(extern c-name (T* ...) -> T)             ; varargs
```

Declares a C function. Types are lIR-level types (`i8 i16 i32 i64 f32
f64 ptr` and `unit` for void). Calling it requires an enclosing
`unsafe` (§4.12).

### 3.7 `defconst` — **Proposed**

```
(defconst name e)
```

`e` must be a literal, a constructor of literals, or a vector/map of
those. Evaluated once at program start; the value is immortal (never
freed, never counted). Objects in it are marked shared (they are
reachable from every thread).

## 4. Core special forms

A *core form* is one the type checker or the ownership checker has a
rule for that could not be derived from a definition written in
fibber. Everything else is a macro or a library function (§6).

### 4.1 Literals and sized literals

Literals (§1) are expressions. `[e*]` builds a persistent vector,
`{k v ...}` a persistent map; both are core because their element
types are unified (types.md §2.1) and because the compiler builds them
directly rather than through `conj`.

```
(i8 e) (i16 e) (i32 e) (i64 e) (f32 e) (f64 e)
```

`e` must be a numeric literal; the form is that literal at that width.
It is not a conversion. Conversions are library functions (`trunc`,
`sext`, `sitofp`, ... on the lIR names).

### 4.2 `fn` — **Proposed** (form), **Decided** (capture rule)

```
(fn (param*) body+)
(fn name (param*) body+)              ; self-recursive closure
param ::= x | x: T
```

Anonymous function value. Free variables of the body are *captured*.
What is captured is decided by the checker, not the programmer:

- An immutable variable is captured by value (the reference is copied;
  it is retained iff the closure escapes, ownership.md §3.3).
- A variable holding a cell is captured as that cell; two closures
  capturing it share it (ownership.md §6, case 05). There is no other
  "captured mutable variable": the only mutable thing a closure can
  see is a cell, an atom, or an `&` parameter of the enclosing `defun`.
- An `&` parameter may be captured only by a closure that does not
  escape the call (ownership.md §5, case 18).

With `name`, the body may call `name` to call the closure itself; this
is a self-reference through the closure's own code and environment,
not a capture, so it forms no cycle (ownership.md §6). `name` is bound
only inside the body. No `&` parameters on `fn`.

### 4.3 `let` — **Proposed**

```
(let (binding*) body+)
binding ::= (x e) | (x: T e) | (p e)
```

Sequential: each binding sees the ones above it. `p` is a pattern
(§4.5); a `let` pattern must be irrefutable (a variable, `_`, a struct
pattern, a single-variant enum pattern, or a vector pattern of fixed
length is *not* irrefutable and is rejected). Shadowing an enclosing
binding is allowed (the ADR 006 rule is dropped: **Proposed**); a
name may not be bound twice in the same `let`.

The scope of `x` is the following bindings and the body. An object
bound by `let` and not escaping is freed when the `let` ends
(ownership.md §2).

### 4.4 `if`, `do`

```
(if test then else)
(do e*)
```

`test : bool`. Both branches have the same type. `(do)` is `unit`.

**Proposed** narrowing rule: in `(if (nil? x) e1 e2)` where `x` is a
variable of type `(? T)`, `x` has type `T` inside `e2`; in
`(if (some? x) e1 e2)`, `x : T` inside `e1`. This is a desugaring:
`e2` becomes `(let ((x (unwrap-unchecked x))) e2)`. Only a bare
variable narrows; nothing else is flow-sensitive.

### 4.5 `match` — **Proposed**

```
(match e clause+)
clause ::= (p body+) | (p :when test body+)
p ::= _ | x | literal | nil | (some p) | (V p*) | (Name p*) | [p*] | (x :as p)
```

- `(V p*)`: enum variant, one sub-pattern per payload field.
- `(Name p*)`: struct, positional in field order.
- `[p*]`: vector of exactly that length. `[p* ... rest]` binds the
  remainder.
- Clauses are tried in order. The set of clauses must be exhaustive
  for enum and option scrutinees; literal and vector patterns need a
  final `_`. Non-exhaustive `match` is a compile error
  ("non-exhaustive match: missing V"). A clause that can never match
  is a compile error.
- Variables bound by a pattern are borrows of the scrutinee's
  components (types.md §6.2); the scrutinee is kept alive for the
  whole `match`.

### 4.6 Cells: `cell`, `deref`/`@`, `set!` — **Decided** (semantics), **Proposed** (spelling)

```
(cell e)            ; library constructor, type (Cell T)
@e  ≡ (deref e)     ; read: T for (Cell T) and (Atom T); (? T) for (Weak T)
(set! place e)
place ::= x            ; x is an & parameter of the enclosing defun
        | e            ; any expression of type (Cell T)
```

`(set! c v)` replaces the cell's content: the old content is released,
`v` is retained if it was borrowed. Returns `unit`. `set!` is core
because of the `&`-parameter place; `cell` and `deref` are library.
Reading a cell yields an *owned* reference (types.md §6.2): the value
you read stays valid even if the cell is overwritten while you hold it.

### 4.7 `&` parameters — **Decided** (ownership.md §5)

Declaration `(defun f (&v ...) body)`: inside `body`, `v` is a local
mutable binding of the parameter's type, initialised from the caller's
variable. It is read by naming it (`v`), updated by passing it on
(`(g &v)`) or by `(set! v e)`.

Call `(f &x e2 ...)`: `x` must be a *place*:

- a variable of type `(Cell T)`: `f`'s parameter is initialised from
  `@x`, and after the call `(set! x v')` is performed; or
- an `&` parameter of the enclosing `defun`: `f`'s parameter is
  initialised from it, and after the call it is assigned `v'`.

Nothing else is a place. A plain `let` variable is immutable and
cannot be passed with `&`.

Rules (ownership.md §5, all **Decided**):

1. The `&` arguments of one call must name distinct variables;
   `(bar &x &x)` is the compile error `passed to more than one & parameter`.
   The check is syntactic.
2. The same variable may be passed both as `&x` and read as `@x` or
   `x` in one call (case 17): the read sees the pre-call value.
3. An `&` parameter may not be captured by a closure that escapes the
   call: `& parameter captured by escaping closure`.
4. An `&` parameter may not be used inside an `async` form:
   `& parameter in async function` (ownership.md §8).
5. Updates through `&` are copy-on-write on the count: unique →
   in place; shared → copy.

### 4.8 Threads: `plet`, `pmap`, `spawn`, `atom` — **Decided** (rules), **Proposed** (which are core)

```
(plet ((x e)*) body+)                    ; macro over spawn/join
(spawn (fn () e))  : (Thread T)           ; core: runs e on another thread
(join t)           : T                    ; library
(pmap f coll)                             ; library
(atom e) (swap! a f) (reset! a e) @a     ; library, on the built-in (Atom T)
```

`plet` expands to `spawn` of each initialiser as a nullary closure,
then `join` of each in order, then the body. `spawn` is the one core
form: it is the thread boundary (ownership.md §3.4). Its argument
closure must be *sendable* (types.md §5.4): everything it captures is
immutable or an atom; a cell that is not an atom makes the error
`cell cannot be shared between threads`. `pmap`'s function argument
and elements are checked the same way through `pmap`'s type.

Atoms: `(atom e)` requires `e`'s type to be sendable (an atom's content
is read from every thread). `@a` returns a retained reference to the
current value; `swap!`/`reset!` release the old one (ownership.md §7).
`swap!` applies `f` atomically with respect to other `swap!`s on the
same atom, retrying on contention; `f` must be pure enough to re-run.
There is no `dosync` (ownership.md §7, **Decided**).

### 4.9 Field access `.`

```
(. e x)
```

`e : (Name ...)` a struct with field `x`. The result is a borrow of
the field, valid while `e`'s object is. Core (it needs the struct's
declaration). `(. e x)` on a `(? Name)` is an error; match or narrow
first.

### 4.10 `async`, `await`, `block-on` — **Decided** (ownership.md §8), **Proposed** (spelling)

```
(async body+)   : (Task T)      ; creates a task; body runs when polled
(await e)       : T             ; e : (Task T); only inside async
(block-on e)    : T             ; library; runs a task to completion
(yield)         : (Task unit)   ; library; a task that completes next poll
```

`async` is a closure that escapes by construction (its frame outlives
the call): every free variable it uses is retained on entry (case 11).
It is also a thread boundary: the task may run on any executor thread,
so its captures must be sendable (same check as `spawn`). `&`
parameters may not appear inside it. `await` outside `async` is a
compile error. A `(Task T)` is an ordinary object; dropping it before
completion cancels it.

### 4.11 `defun`-local recursion, `loop`

There is no `loop`/`recur`. Tail calls to named functions and to a
named `fn` are guaranteed to run in constant stack (types.md §8.6):
`(fn go (i acc) (if (= i 0) acc (go (- i 1) (+ acc i))))`.

### 4.12 `unsafe` — **Decided** (ownership.md §9), **Proposed** (spelling)

```
(unsafe body+)
```

Inside: `extern` functions may be called; the raw types `ptr` and the
library functions `ptr+`, `load-i64`, `store-i64`, `load-byte`,
`store-byte`, `malloc`, `free`, `str-bytes` (borrow a string's bytes
as `ptr`) may be used. Counting and type checking are unchanged. A
`ptr` passed to foreign code is valid only for the call unless the
`extern` is declared `^retains` (then the compiler retains the object
before the call and never releases it: the foreign side owns one count
and must call `fib_release`).

### 4.13 `defmacro`, `quote`, `quasiquote` — **Proposed**

```
(quote e)          'e         : Syntax
(quasiquote e)     `e         : Syntax, with ,e (unquote) and ,@e (splice) inside
(gensym "prefix")             : Syntax  (a fresh symbol)
```

A macro is a fibber function of type `(-> (Syntax ...) Syntax)` (or
`(Vec Syntax)` for a `...` rest parameter), compiled and run by the
reference interpreter at expansion time. Its body may use any function
from modules the current module imports, provided those modules are
already compiled (module order, §7). Expansion is outermost-first,
repeated until no macro call remains, before name resolution and
typing. Macros are unhygienic; use `gensym` for introduced bindings.
A macro call at top level may expand to any sequence of top-level
forms. Reflection: `(struct-fields 'Name)` returns the field names of a
struct defined earlier in the module or imported (ADR 023 kept,
**Proposed**).

`quasiquote` builds `Syntax` values: `` `(a ,x) `` ≡
`(List [(Sym "a") x])` where `x : Syntax`, and `,@xs` splices a
`(Vec Syntax)`.

## 5. Patterns of use fixed by the cases

These are not new forms; they pin how the forms above combine and
they are what Appendix A relies on.

- A mutable local is a `let`-bound cell: `(let ((v (cell [1 2 3]))) ...)`,
  passed with `&v`, read with `@v`.
- A vector is built with `conj`, read with `nth`, sized with `count`;
  these are protocol methods (`Collection`, `Indexable`, `Countable`)
  from the library, statically dispatched when the type is known.
- `(list e*)` builds a `(List T)`; `(box e)`/`(unbox b)` wrap a scalar
  in a `(Box T)` object.
- `(weak e)` : `(Weak T)`, `(deref w)` : `(? T)` (ownership.md §6).
- `(range n)` : `(Vec i64)`. `(iter v)`, `(filter-iter it pred)`,
  `(collect it)` are the library iterator functions used in case 09.

## 6. Core versus macro versus library

**Proposed** rule for deciding: a form is **core** iff the checker
needs a rule for it that cannot be expressed as the type scheme plus
escape summary of a function definition (types.md §3.4, §6.3). A form
is a **macro** iff it is a purely syntactic rewrite into other forms
with no new typing or ownership behaviour. Everything else is
**library**: an ordinary function or protocol method whose contract is
its type and its escape summary.

| Core (needs a rule) | Macro (rewrite) | Library (a type) |
|---|---|---|
| `defun fn let if do match` | `plet` → `spawn`/`join` | `cell atom swap! reset! deref` |
| `defstruct defenum` | `cond when unless and or` | `weak join` |
| `defprotocol extend-protocol` | `let*`-style destructuring sugar | `pmap map filter reduce for-each` |
| `set!` (for `&` places) | `->` `->>` threading | `conj nth count first rest` |
| `& ` params and args | `defrecord`-style derive macros | `+ - * / = < ...` (protocol methods `Num`, `Eq`, `Ord`) |
| `spawn async await` | `assert`, `dotimes`, `while` (over named `fn`) | `block-on yield` |
| `.` field access | `some->`, `if-let` | `list box unbox range iter collect` |
| `unsafe extern defconst` | | `print println str-len` ... |
| `defmacro quote quasiquote gensym` | | |
| `[...]` `{...}` literals, sized literals | | |
| `module import` | | |

Why each core item is core: `defun`/`fn` (parameter borrow, result
owned, capture analysis); `let` (scope = lifetime); `if`/`do`
(branches, narrowing); `match` (exhaustiveness, borrows of components);
`defstruct`/`defenum`/`defprotocol` (nominal types and instances);
`set!` (writes to an `&` place); `&` (copy-in/out and the distinctness
check); `spawn`/`async`/`await` (thread boundary, escape by
construction, coroutine transform); `.` (needs the declaration);
`unsafe`/`extern` (permission and FFI types); macros and quoting
(compile-time evaluation); literals (built directly).

Why `cell`, `atom`, `weak` are library: their behaviour is fully
captured by their types (`(-> (a) (Cell a))`, `(Send a) => (-> (a) (Atom a))`,
`(Object a) => (-> (a) (Weak a))`) plus the `Deref` instances; the
runtime implements them, but the checker treats them as functions.
`pmap` is library for the same reason: its type carries the `Send`
constraints and its function parameter is typed `(->send (a) b)`.

## 7. Modules — **Proposed**, minimal

```
(module name)                          ; first form of a file; name is dotted
(module name (export x y Z))           ; explicit export list
(import name)                          ; all exports, unqualified
(import name (x y))                    ; only these
(import name :as alias)                ; qualified as alias/x
```

- One module per file. `name` maps to a path (`a.b.c` → `a/b/c.fib`)
  under the roots the compiler is given.
- Without an export list, every top-level definition is exported,
  including macros, structs, enums, protocols and protocol
  implementations. Implementations are always exported (they are
  global facts).
- Module dependencies must be acyclic. Modules are compiled in
  dependency order; a module's exported interface is the set of type
  schemes, escape summaries, struct/enum layouts, protocol
  declarations and instances, and macro definitions (types.md §3.7).
- Importing two modules that export the same name is an error at the
  first *use* of that name unqualified.
- The prelude (`fibber.core`) is imported implicitly; `(module name
  :no-prelude)` turns that off.
- There is no separate interface file; the interface is derived from
  the source.

## 8. What is deliberately not in the language

- No `loop`/`recur`, no `set!` on `let` variables, no mutable struct
  fields (use a cell in the field), no `dosync` (ownership.md §7).
- No nil-punning, no truthiness, no implicit numeric promotion
  (ADR 017 dropped: **Proposed**).
- No SIMD literals (ADR 016 dropped for the core; can return as a
  library over lIR vector types).
- No conventional `<[...]>` collections (ADR 018 dropped; the `&` +
  copy-on-write rule gives in-place updates on unique persistent
  values instead, ownership.md §5).
- No `dosync`, no `ref`.

## Appendix A. The twenty ownership cases in the proposed syntax

Headers are unchanged from `cases/ownership/`. Bodies differ from the
current files only where the proposed syntax requires it; each
difference is noted. The `deref`/`@` spelling, `cell`, `set!`, `&`,
`plet`, `pmap`, `atom`, `async`, `await`, `weak` are as in §4.
Unannotated struct fields are generic (§3.2), so case 19 gains field
types (its `Node` is recursive).

```lisp
;; 01-return-part-of-argument.fib
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
Unchanged. `first : (Seq s e) => (-> (s) e)`; on `(List (Box i64))`
it returns `(Box i64)`. (`first` on a possibly-empty list returns the
element, not an option, and panics on empty; see types.md §9 Open 7.)

```lisp
;; 02-structural-sharing.fib
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

```lisp
;; 03-store-borrowed-value.fib
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
    (str-len (nth keep 0))))
```
`length` → `str-len` (the string length function; `count` is for
collections).

```lisp
;; 04-branch-dependent-owner.fib
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
      (str-len a))))
```
`length` → `str-len`.

```lisp
;; 05-closures-share-state.fib
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; Two escaping closures share one captured mutable variable.
(defun make-counter ()
  (let ((n (cell 0)))
    (list (fn () (set! n (+ @n 1)))
          (fn () @n))))

(defun main () -> i64
  (let ((c (make-counter)))
    (let ((inc (nth c 0))
          (get (nth c 1)))
      (inc)
      (inc)
      (get))))
```
Does not type: the two closures have types `(-> () unit)` and
`(-> () i64)` and a `(List T)` is homogeneous (types.md §2.1).
Proposed rewrite keeping the verdict and the shape (one shared cell,
two escaping closures):

```lisp
(defun make-counter ()
  (let ((n (cell 0)))
    (list (fn () (set! n (+ @n 1)) @n)
          (fn () @n))))
```
Both closures are now `(-> () i64)`. `main` is unchanged; `(inc)` is
called for effect and its value is dropped. Result is still 2.

```lisp
;; 06-capture-borrowed-param.fib
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

```lisp
;; 07-recursive-accumulator.fib
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
Unchanged.

```lisp
;; 08-mutate-while-iterating.fib
;; spec:   §5
;; expect: accept
;; result: 6
;; audit:  clean
;; The loop is still reading v when append replaces it. The iteration
;; holds a count, so append copies instead of updating in place, and the
;; loop sees the original three elements.
(defun dup-all (&v)
  (for-each v (fn (x) (append &v x))))

(defun main () -> i64
  (let ((v (cell [1 2 3])))
    (dup-all &v)
    (count @v)))
```
Unchanged. `append : (-> (&(Vec a) a) unit)` is the library's
in-place-when-unique push (types.md §6.5).

```lisp
;; 09-iterator-outlives-source.fib
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
Unchanged.

```lisp
;; 10-atom-old-value.fib
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
Unchanged. `pmap`'s result `(Vec i64)` is dropped.

```lisp
;; 11-borrow-across-await.fib
;; spec:   §8
;; expect: accept
;; result: 5
;; audit:  clean
;; The parameter is used after the await, so it is retained on entry
;; and outlives the caller's scope.
(defun measure (s) (async (await (yield)) (str-len s)))

(defun main () -> i64
  (let ((task (let ((s "hello")) (measure s))))
    (block-on task)))
```
`length` → `str-len`.

```lisp
;; 12-reject-same-binding-twice-inout.fib
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

```lisp
;; 13-reject-cell-crosses-thread.fib
;; spec:   §7
;; expect: reject
;; error:  cell cannot be shared between threads
(defun main () -> i64
  (let ((n (cell 0)))
    (pmap (fn (i) (set! n (+ @n i))) (range 10))
    @n))
```
Unchanged.

```lisp
;; 14-reject-inout-in-async.fib
;; spec:   §8
;; expect: reject
;; error:  & parameter in async function
(defun fill (&buf) (async (await (yield)) (append &buf 1)))

(defun main () -> i64
  (let ((b (cell [])))
    (block-on (fill &b))
    (count @b)))
```
Unchanged.

```lisp
;; 15-cycle-through-cell-leaks.fib
;; spec:   §6 (proposed)
;; expect: accept
;; result: 1
;; audit:  leak-cycle
;; A cell that ends up holding a vector containing itself. Under the
;; proposed rule this leaks, and the audit must report it as a cycle
;; leak, not as any other failure.
(defun main () -> i64
  (let ((c (cell [])))
    (set! c [c])
    (count @c)))
```
Unchanged. `c : (Cell (Vec (Cell ...)))` is a recursive type; it is
accepted because `(cell [])` gives `c : (Cell (Vec a))` and `[c]`
then unifies `a` with `(Cell (Vec a))`. The occurs check would reject
that. **Proposed:** the checker allows this program through the
library's `(defenum (Rec) ...)`-free path only if recursive types are
admitted through `Cell`; see types.md §9 Open 3 for the two options
(equirecursive unification through `Cell`, or rewriting the case with
an explicit `(defstruct Self (v: (Cell (Vec Self))))`). The verdict is
the same either way; the audit output is what the case tests.

```lisp
;; 16-coordinated-update-single-atom.fib
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
Unchanged.

```lisp
;; 17-inout-and-borrow-same-call.fib
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

```lisp
;; 18-reject-inout-captured-by-escaping-closure.fib
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

```lisp
;; 19-weak-parent-pointer.fib
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; A tree whose nodes point at their parent through a weak reference.
;; The strong edges form no cycle, so the whole tree is freed when main
;; returns.
(defstruct Node (parent: (? (Weak Node)) children: (Cell (Vec Node))))

(defun add-child (parent)
  (let ((c (Node (weak parent) (cell []))))
    (set! (. parent children) (conj @(. parent children) c))
    c))

(defun depth (n)
  (let ((p (deref (. n parent))))
    (if (nil? p) 0 (+ 1 (depth p)))))

(defun main () -> i64
  (let ((root (Node nil (cell []))))
    (let ((a (add-child root)))
      (let ((b (add-child a)))
        (depth b)))))
```
Changes: `Node`'s fields are annotated (recursive struct, §3.2).
`(Node (weak parent) ...)` passes a `(Weak Node)` where `(? (Weak Node))`
is expected: accepted by option promotion (types.md §3.6, **Proposed**);
without it the line reads `(Node (some (weak parent)) (cell []))`.
`(deref (. n parent))` on `(? (Weak Node))` needs `deref` on an
option of a weak: the library provides `(Deref (? (Weak a)) (? a))`
so this line is unchanged; `(if (nil? p) ...)` narrows `p` to `Node`
in the else branch (§4.4).

```lisp
;; 20-weak-ref-to-dead-object.fib
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
Unchanged.

## 9. Open decisions

Each needs the owner's sign-off. Recommendation first, alternative
second.

1. **Binding syntax.** Recommend Scheme-style `(let ((x e)) ...)`
   (keeps the cases nearly verbatim; `[...]` stays data).
   Alternative: Clojure-style `(let [x e] ...)`.
2. **`deref`/`@` on weak references.** Recommend one `Deref` protocol
   with determined result type, so `@c`, `@a` and `(deref w)` are one
   mechanism and `@w` also works (returning `(? T)`). Alternative:
   `@` only for cells and atoms; weak has its own `upgrade`.
3. **Option promotion** (`T` accepted where `(? T)` is expected, at
   sites where both are known). Recommend on: it removes `some` from
   most constructor calls (case 19). Alternative: explicit `some`
   everywhere; strictly principal types.
4. **Struct fields without annotations become type parameters.**
   Recommend on. Alternative: annotations required on every field
   (simpler error messages, more typing).
5. **`plet` as a macro over `spawn`/`join`.** Recommend: one thread
   boundary form, and the Send check lives in one type. Alternative:
   `plet` core with its own rule.
6. **`if`-narrowing of `nil?`/`some?` on a bare variable.** Recommend
   on (case 19 reads naturally). Alternative: `match` or `if-let`
   only.
7. **Shadowing allowed** (drops ADR 006). Recommend allowed: macros
   and nested `let`s become much easier to write. Alternative: keep
   the ban.
8. **Case 05's heterogeneous list.** Recommend the rewrite shown
   (both closures return `i64`). Alternative: a `(Tuple a b)` type and
   `(tuple f g)` in the case; or `(dyn Fn)` — rejected because it
   erases the closure's argument types.
9. **`&` only on `defun`, and `&`-taking functions not first-class.**
   Recommend. Alternative: allow `&` on `fn` (function types then
   carry `&`, closures with places).
10. **No numeric literal polymorphism**: `1 : i64`, `1.0 : f64`.
    Recommend (no defaulting, no ambiguity). Alternative: Haskell-style
    `Num a => a` literals with defaulting to `i64`/`f64`.
11. **Macros are typed fibber functions over `Syntax` run by the
    reference interpreter.** Recommend. Alternative: an untyped
    macro-time evaluator (liar's `eval.rs`), which is a second language.
