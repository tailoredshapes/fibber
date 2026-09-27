# fibber syntax (draft: ownership-first angle)

Status: draft. Every decision below is marked **Decided** (a direct
consequence of `spec/ownership.md`) or **Proposed** (everything else).
`spec/ownership.md` is authoritative; where this draft and it disagree,
it wins. The companion document is `types.md` (type grammar, inference,
and how every ownership rule is decided by the checker).

Design rule for this draft: every surface form maps to a small set of
core forms, each of which has one typing rule and one ownership rule in
`types.md`. Nothing is left for the ownership pass to guess at: a form
either has a rule or is a macro over forms that do.

---

## 1. Reader

**Proposed** throughout unless noted. The reader turns text into
*forms*: atoms and lists. It knows nothing about special forms.

### 1.1 Lexical structure

| Class | Syntax | Form produced |
|---|---|---|
| line comment | `;` to end of line | none |
| discard | `#_` followed by a form | none (the form is read and dropped) |
| integer | `42`, `-7`, `0x1F`, `0b1010`, `1_000_000`; optional suffix `i8` `i16` `i32` `i64` | integer literal, type from suffix, default `i64` |
| float | `3.14`, `1e9`, `-2.5e-3`; optional suffix `f32` `f64` | float literal, default `f64` |
| string | `"..."` with escapes `\n \t \r \\ \" \0 \u{XXXX}` | string literal (UTF-8) |
| character | `\a`, `\space`, `\newline`, `\tab`, `\return`, `\u{41}` | char literal (a Unicode scalar) |
| symbol | see 1.2 | symbol |
| keyword | `:name`, `:ns/name` | keyword |
| constants | `nil` `true` `false` | reserved symbols (not rebindable) |
| list | `( forms... )` | list |
| vector | `[ forms... ]` | `(vector forms...)` |
| map | `{ k v k v ... }` (even count, else reader error) | `(hash-map k v ...)` |
| quote | `'x` | `(quote x)` |
| quasiquote | `` `x `` | `(quasiquote x)` |
| unquote | `,x` | `(unquote x)` |
| splice | `,@x` | `(unquote-splicing x)` |
| deref | `@x` | `(deref x)` |
| in-out | `&x` (x a symbol) | `(& x)` |

Whitespace and commas are equivalent separators (`,` is unquote only
when directly attached to a following form; `,` followed by whitespace
is a separator). `#_` and `'`-family prefixes stack in reading order.

Integer literals must fit their type; `9223372036854775808` is a reader
error, not a wrap. No literal is ever promoted (see `types.md` §1.1).

### 1.2 Symbols

A symbol is a sequence of characters from letters, digits, and
`+ - * / < > = ! ? _ . $ % & ~ ^ '`, not starting with a digit, not
starting with `:`, `'`, `` ` ``, `,`, `@`, `#`, and not entirely
consisting of `&` (so `&` alone is reserved for the in-out marker).
A symbol may contain one `/` separating a namespace from a name:
`seq/first`. The symbol `/` alone is a name. A symbol ending in `:`
(as in `x:`) is an ordinary symbol; the grammar of binding forms
recognises it as an annotation marker (§2.1). `.` alone is the field
access operator (§2.8). Symbols are case-sensitive.

### 1.3 Ordering guarantees

The reader preserves source order and attaches a source position
(file, line, column) to every form. Positions survive macro expansion:
a form produced by a macro carries the position of the macro call
unless the macro constructed it from an input form, which keeps its
own. Every compile error names a position.

---

## 2. Core special forms

**Decided:** the language has a fixed set of *core forms* (this
section), a fixed set of *primitives* (§3: ordinary call syntax, but
with a typing and ownership rule of their own), *macros* (§4) that
expand to those, and a *library* written in fibber. The ownership
checker sees only core forms and primitives.

Notation: `sym` a symbol; `e` an expression; `body` one or more
expressions evaluated in order, value of the last; `type` a type form
(grammar in `types.md` §1); `pat` a pattern (§2.6). `[...]` in grammar
means optional, `...` repetition.

Evaluation rule conventions: "value" means the result of evaluating an
expression; the ownership mode of that value (owned or borrowed) is
defined in `types.md` §6 and is not repeated here.

### 2.1 Binding syntax shared by several forms

```
param   := sym | sym: type | (& sym) | (& sym) : type
```

- `sym` — a parameter, type inferred.
- `sym: type` — an annotated parameter. The reader yields the two forms
  `sym:` and `type`; the grammar pairs them. `(defun f (x: i64) ...)`.
- `(& sym)` — an in-out parameter (`&sym` in source). **Decided** (§5):
  copy-in, copy-out; see §2.14.

```
ret     := -> type          (optional return annotation, after params)
```

### 2.2 `defun`

```
(defun name (param...) [-> type] [docstring] body)
```

Defines a global function `name` in the current namespace. Names are
global and capture nothing (**Decided**, §6: "Named functions are
global and capture nothing"). A `defun` may appear only at top level.
Parameters are in scope in `body`. `body` is evaluated as `(do body)`.

Every parameter is a borrow valid for the whole call; the result is
owned by the caller (**Decided**, §4). The checker infers, per
parameter, a *convention* (`borrow` or `own`) and an *escape summary*
(`noescape` or `escapes`); see `types.md` §6.4. Neither is written by
the programmer.

A function may not be redefined in the same namespace (**Proposed**).
Mutual recursion between `defun`s in one namespace is allowed.

### 2.3 `fn`

```
(fn (param...) [-> type] body)
```

An anonymous function (closure). Its free variables are *captured* at
the point the `fn` form is evaluated. **Decided** (§3.3, §6): a
closure that escapes retains every object it captures at creation and
releases them when it is freed; a closure that does not escape captures
by borrow. Whether a closure escapes is decided by the checker
(`types.md` §6.5), never by the programmer.

Captured variables are immutable inside the closure with two
exceptions: a captured `(Cell T)` may be written with `set!` (that is
what cells are for; case 05), and a captured `&` parameter may be
written or passed as `&v` **only if the closure does not escape**
(**Decided**, §5, case 18).

A `fn` may not use `&` parameters of its own (**Proposed**: in-out is a
`defun` feature; closures are called through a uniform calling
convention that has no write-back slot). The alternative is in the
open decisions.

### 2.4 `let`

```
(let ((pat e) ...) body)
```

Sequential bindings: each `e` is evaluated in order, in a scope where
the previous bindings are visible; `pat` is matched against its value
(irrefutable patterns only: a symbol, `_`, a struct pattern, a vector
pattern of fixed length is *not* irrefutable and is rejected). `body`
is then evaluated. A `let` binding may not shadow a name bound in the
same function, including parameters (**Proposed**, from liar ADR 006;
shadowing across `fn` boundaries inside the same `defun` is also
rejected). A binding ends when the `let` ends.

**Decided** (§2): an object bound by `let` that never escapes is freed
when the `let` ends, with no count operations. One that escapes is
counted at the point of escape.

Local `let` bindings are monomorphic (no let-polymorphism;
**Proposed**, `types.md` §3.2).

### 2.5 `if`, `do`

```
(if cond then else)
(do body)
```

`if`: `cond` must have type `bool` (no truthiness; **Proposed**). Both
branches are required and must have the same type. Exactly one branch
is evaluated. `do`: evaluates each form in order, returns the last;
`(do)` returns `()` of type `unit`.

Ownership (**Decided** by §3.1 and case 04): if one branch's value is
owned and the other's is borrowed, the borrowed branch retains its
value so the `if` as a whole is owned. Values of non-final `do` forms
that are owned temporaries are released immediately after the form.

### 2.6 `match`

```
(match e (pat body)...)

pat := _                          wildcard
     | sym                        bind (irrefutable)
     | literal                    integer, float, string, char, bool
     | nil                        the Option None constructor
     | (ctor pat...)              sum-type variant, positional
     | (Struct pat...)            struct, positional in field order
     | (Struct :field pat ...)    struct, by field name
     | [pat...]                   vector of exactly that length
     | (pat :as sym)              bind the whole while matching inside
```

`e` is evaluated once. Clauses are tried in order; the first whose
pattern matches evaluates its body, with pattern variables bound to the
*parts* of the value. All bodies must have the same type. The clauses
must be exhaustive over the scrutinee's type (**Proposed**: a
non-exhaustive `match` is a compile error; `_` makes any match
exhaustive).

Ownership: pattern variables are borrows *derived from* the scrutinee
(`types.md` §6.2); a clause body that returns one of them returns a
derived borrow, which the enclosing rule retains if it must outlive the
scrutinee.

### 2.7 `defstruct`

```
(defstruct Name (field type)...)
(defstruct (Name tvar...) (field type)...)
```

A nominal product type with named, typed fields. Field types are
required (**Proposed**: no inference for field types; the struct is a
declared interface). Type variables in the head make the struct
generic. `Name` is also the constructor function, positional in field
order: `(Name e...)`. Structs are immutable objects (**Decided**, §1);
mutability is only through a field of type `(Cell T)` or `(Atom T)`.

A struct may refer to itself (`(defstruct Node (next (Option Node)))`).
This is the only way to write a recursive type (`types.md` §1.7).

### 2.8 Field access `.`

```
(. e field)
```

Reads `field` of struct value `e`. The type of `e` must be a struct
type known at this point of inference, or become known before the end
of the enclosing `defun` (`types.md` §3.4). The result is a borrow
derived from `e`. `(. e field)` is a valid first operand of `set!` when
the field has type `(Cell T)` (§2.10).

### 2.9 `defsum` (sum types)

```
(defsum Name variant...)
(defsum (Name tvar...) variant...)
variant := sym | (sym type...)
```

A nominal sum type. A payload-free variant `sym` is a constant of type
`Name`; a variant `(sym type...)` is a constructor function. Variants
are matched by `match` (§2.6). Variant names live in the namespace of
the `defsum` alongside functions; a name may be a variant of only one
sum type (**Proposed**; the alternative, qualified variants
`Name/variant`, is in the open decisions).

The library predefines

```
(defsum (Option a) nil (some a))
```

and the reader constant `nil` is the `Option` None constructor.

### 2.10 `cell`, `deref` (`@`), `set!`

```
(cell e)          primitive: a new cell holding the value of e
(deref e)  @e     primitive: the current value of a cell or atom
(set! target e)   core form: target := (sym | (. e field)) of type (Cell T)
```

**Decided** (§6): a cell is the one mutable container. `(cell e)`
creates it holding an owned reference to `e`'s value (a store: §3.2).
`@c` returns an *owned* reference to the current value (+1) for both
cells and atoms (`types.md` §6.7; for atoms this is §7 verbatim, for
cells it is **Proposed** as the uniform rule, with the elision
optimisation described there). `(set! c v)` stores `v` (a store: +1 if
borrowed, moved if owned) and releases the previous value; it returns
`()`.

`set!` is also how an `&` parameter is assigned inside its function:
`(set! v e)` where `v` is an `&` parameter of the enclosing `defun`
(§2.14). Nothing else may be the target of `set!`: not a `let`
binding, not a plain parameter.

### 2.11 `defprotocol`, `impl`

```
(defprotocol Name
  (method (self param...) -> type)...)
(defprotocol (Name tvar...) ...)

(impl Name type
  (method (self param...) body)...)
(impl (Name type...) type ...)
```

A protocol is a set of method signatures over a distinguished `self`
type. `impl` provides them for one type (a type constructor such as
`(Vec a)` with its own variables in scope). Every method of the
protocol must be implemented; extra methods are an error. A method
call `(method e args...)` is an ordinary call that dispatches on the
type of `e` (`types.md` §4: static when the type is concrete, through a
vtable when `e` has type `(dyn Name)`). Protocol type parameters are
determined by the `self` type (`types.md` §3.5).

Default methods: a protocol may give a method a body, used when an
`impl` omits it (**Proposed**). liar's `extend-protocol-default`
(implementing protocol A for anything implementing protocol B) is not
in the core; write it as a generic `defun` with a bound.

### 2.12 `atom`, `swap!`, `reset!`

```
(atom e)           primitive: a new atom holding the value of e
(swap! a f)        primitive: a := (f @a), atomically; returns the new value (owned)
(reset! a e)       primitive: a := e; returns the new value (owned)
```

**Decided** (§7): an atom holds a counted reference; `@a` returns a
retained reference; `swap!` and `reset!` replace the value and release
the old one. `f` may be called more than once if another writer
intervenes (compare-and-swap loop; **Proposed** implementation,
`types.md` §8.6), so `f` must be pure with respect to `a`. Atoms never
take part in transactions (**Decided**); there is no `dosync` in this
draft (reserved word; `types.md` open decisions).

`(atom e)` requires the type of `e` to be sendable (`types.md` §5.7);
this is what makes every atom safe to cross a thread.

### 2.13 `spawn`, `join`; `plet`, `pmap`

```
(spawn f)     primitive: run the thunk f on another thread; returns (Task T)
(join t)      primitive: wait for t; returns its value (owned)
```

**Decided** (§3.4, §7): passing a value to another thread is an
escape; only sendable values may cross. `spawn` is the single
thread-crossing primitive: the thunk `f` (its captures) and the result
type must be sendable, which the checker decides by the `Send`
predicate (`types.md` §5.7). At `spawn`, `f` is retained and marked
shared together with everything reachable from it (§7); at `join`, the
result is owned by the caller and is already marked shared.

`plet` and `pmap` are **macro** and **library** respectively
(**Proposed**), built on `spawn`/`join`:

```
(plet ((sym e)...) body)
  ==> (let ((t1 (spawn (fn () e1))) ...)
        (let ((sym1 (join t1)) ...) body))

(defun pmap (f coll) ...)    ;; spawns (f x) per element, joins in order
```

`plet` bindings therefore cannot see each other, and every free
variable of an init expression must be sendable. `pmap` preserves
result order. The error a non-sendable capture produces is the same
wherever it arises (case 13), because it is produced by `spawn`'s
typing rule, with the position of the `plet`/`pmap` call.

### 2.14 In-out parameters `&`

```
definition:  (defun f (... (& v) ...) body)         written (defun f (... &v ...) body)
call:        (f ... (& x) ...)                      written (f ... &x ...)
```

**Decided** (§5): copy-in, copy-out. At the call, `x` must be a
variable that is either (a) an `&` parameter of the enclosing `defun`,
or (b) a variable whose type is `(Cell T)`. The callee's parameter `v`
has type `T`, is a local mutable binding initialised from `x`'s current
value (`@x` for a cell), and may be assigned with `(set! v e)` or
passed on as `&v`. When the call returns, `x` is assigned `v`'s final
value (`(set! x v)` for a cell). During the call the caller's `x` is
untouched (case 17).

**Decided** rules the checker enforces (`types.md` §6.6):

1. The `&` arguments of one call name distinct variables (case 12):
   error `variable X passed to more than one & parameter`.
2. An `&` parameter may not be captured by a closure that escapes the
   call (case 18): error `& parameter captured by escaping closure`.
3. A `defun` with an `&` parameter may not contain an `async` form
   (case 14): error `& parameter in async function`.

Only `defun`s have `&` parameters (§2.3). A protocol method may declare
`&` parameters after `self` (**Proposed**).

### 2.15 `async`, `await`

```
(async body)     core form: a Future computing body; returns (Future T)
(await e)        core form, only inside async: suspends until e's Future completes; returns T
```

**Decided** (§8): the frame of an async computation outlives the call
that created it, so an `async` form is treated exactly as an escaping
closure: every free variable is captured and retained when the `async`
form is evaluated, and released when the Future is freed. Locals
created inside `body` live in the Future's frame. There is therefore no
borrow that could be held across an `await` (`types.md` §6.9). An
"async function" is a `defun` whose body is an `async` form; rule 3 of
§2.14 applies to any `defun` containing `async`.

`(await e)` outside `async` is an error. `block-on`, `yield`, `spawn`
of Futures, and combinators are library (`fib.async`). **Proposed:** a
Future runs on the thread that `block-on`s it (single-threaded
executor per `block-on`); hence `async` imposes no `Send` requirement.
The alternative (work-stealing executor, `async` captures must be
sendable) is in the open decisions.

### 2.16 `unsafe`, `extern`

```
(unsafe body)
(extern name (type...) -> type)                  C ABI, borrowed pointer args
(extern name (type...) -> type :retains (i...))   args at indices i are retained by the callee
(extern name (type...) -> type :varargs)
```

**Decided** (§9): `unsafe` enables raw pointers (`ptr` type; `ptr+`,
`load-*`, `store-*`, `alloc`, `free`, `(raw e)` giving the address of
an object for the duration of the enclosing scope) and calls to
`extern` functions. It does not change type checking or counting of
fibber objects. A foreign callee receives borrows valid for the call
unless the declaration says `:retains`, in which case the compiler
emits a retain before the call and the foreign side owns that count.
An `extern` may be called only inside `unsafe`.

### 2.17 `defmacro`, `quote`, `quasiquote`

```
(defmacro name (param... [& rest]) body)
(quote form)          'form
(quasiquote form)     `form   with (unquote e) ,e and (unquote-splicing e) ,@e
```

A macro is a function from forms to a form, run at expansion time in a
compile-time evaluator (`fibref` for the reference implementation; the
compiler JITs or interprets — **Proposed**: interprets). Macro bodies
are ordinary fibber restricted to the `fib.form` library (lists,
symbols, `gensym`, `symbol?`, `list?`, `first`, `rest`, `cons`,
`concat`). Macros are expanded outermost-first, repeatedly, before
name resolution; the result must consist of core forms, primitives and
calls. `& rest` in a macro's parameter list binds the remaining forms
as a list. `(gensym "base")` returns a fresh symbol that cannot collide
with any source symbol.

Macros cannot inspect types, ownership or struct definitions (no
compile-time reflection in this draft; liar ADR 023 is out).

### 2.18 `ns`

```
(ns name (:require [other :as alias] ...))
```

First form of a file. See §6.

### 2.19 Calls

```
(f arg...)
```

Any list whose head is not a core form or macro is a call. The head is
evaluated first, then arguments left to right (§5). The head must have
a function type: a `defun`, a protocol method, a primitive, a variant
constructor, a struct constructor, or a closure value. `&arg` is legal
only in a call to a `defun` or protocol method whose corresponding
parameter is `&`.

---

## 3. Primitives

Primitives use call syntax but each has its own typing and ownership
rule in `types.md`. They are the complete list of operations the
checker knows about that are not core forms.

| Group | Primitives |
|---|---|
| integers | `+ - * / rem neg` (per width, no promotion), `bit-and bit-or bit-xor bit-not shl shr sar popcount`, `= != < <= > >=` |
| floats | `f+ f- f* f/ frem fneg`, `f= f!= f< f<= f> f>=` |
| bool | `not` (short-circuit `and`/`or` are macros over `if`) |
| conversions | `trunc zext sext fptrunc fpext fptosi fptoui sitofp uitofp` (target type first: `(zext i64 x)`), `char->i32`, `i32->char` (checked) |
| cells/atoms | `cell deref atom swap! reset!` |
| threads | `spawn join` |
| weak | `weak upgrade` (§6: "provided by the standard library" — they are primitives exposed through `fib.core`) |
| arrays | `array (n init)`, `array-len`, `array-get`, `array-set` (returns a new array; in place when unique), `array-copy` — the immutable fixed array on which `Vec`, `Map` and `str` operations are built |
| strings | `str-len`, `str-bytes`, `str-concat`, `str-eq`, `str-slice` (byte-indexed, validated) |
| unsafe | `ptr+ load-i8 load-i64 load-ptr store-i8 store-i64 store-ptr alloc free raw` |
| dyn | `(dyn P e)` — coerce a value whose type implements `P` to `(dyn P)` |
| traps | `(trap "message")` — aborts the program; type `a` |

Arithmetic is width-specific and never promotes (**Decided** by lIR's
1:1 rule; liar ADR 017 is out). `(+ (i32 1) 2)` is a type error;
write `(+ (i32 1) (i32 2))` or `(+ 1i32 2i32)`. Signed overflow traps
(**Proposed**); `wrapping-*` variants exist for the other behaviour.

---

## 4. Core versus macro versus library

**Proposed rule for deciding:**

1. A construct is **core** if the type checker or the ownership checker
   needs a rule specific to it: a binding structure, an escape
   position, a thread crossing, a mutable container, a suspension
   point, or a typing rule that cannot be expressed as a function type.
2. A construct is a **macro** if it can be rewritten into core forms
   such that the rewritten program has the same verdicts and reports
   the same errors at the same positions.
3. Everything else is **library**: a `defun`, `defstruct`, `defsum`,
   `defprotocol` or `impl` written in fibber.

Consequences:

| Core forms | `defun fn let if do match defstruct defsum defprotocol impl set! async await unsafe extern defmacro quote quasiquote ns` and the `&` call marker |
|---|---|
| Primitives | §3 |
| Macros (prelude) | `plet` `cond` `when` `unless` `and` `or` `->` `->>` `let*`(alias of `let`) `doto` `loop`/`recur` (rewritten to a local `defun`-style tail-recursive helper) `assert` |
| Library | `Vec Map Set List` and their operations (`conj assoc nth get count first rest`), `Seq`/`Countable`/`Indexable` protocols, `map filter reduce for-each range iter collect filter-iter`, `pmap`, `Box`, `nil?` `some?`, `length starts-with?`, `block-on yield`, `println` |

`pmap` being library (not core) is a consequence of `spawn` being the
only crossing primitive: the `Send` requirement is in `pmap`'s
inferred type, not in a special rule.

`loop`/`recur` (**Proposed**) expands to a `letfn`-style local named
function called in tail position; tail calls to it are guaranteed
(`types.md` §8.9). There is no `letfn` in the core; the macro expands
to `let` of a `fn` and the self-call goes through the closure's own
code pointer without capturing the closure (§6, "a local recursive
closure calls itself through its own code pointer").

---

## 5. Evaluation order

**Decided** (liar ADR 008 carried over; needed to give `&` one
meaning): evaluation order is source order, left to right, depth
first.

- Call: head, then arguments left to right; then the call. For an
  `&x` argument, the copy-in read of `x` happens in argument order, so
  `(f &x @x)` reads `x` twice, both before the call (case 17).
  Write-backs happen after the call returns, in parameter order (they
  target distinct variables, so order is unobservable).
- `let`: bindings top to bottom; `do`: forms in order; `if`/`match`:
  condition/scrutinee, then exactly one branch.
- Struct constructors, vector and map literals: elements left to right.
- Owned temporaries are released as early as the ownership rules allow
  (`types.md` §6.3), which is after the enclosing call returns or after
  the enclosing `do` form completes; the reference interpreter releases
  at exactly those points.
- `plet` init expressions run concurrently in unspecified order; `pmap`
  applies its function in unspecified order and returns results in
  input order.

---

## 6. Modules

**Proposed**, minimal:

```
(ns fib.example
  (:require [fib.seq :as seq]
            [fib.vec]))
```

- One namespace per file; the `ns` form is the first form. The file
  path is the namespace with `.` as the directory separator, under a
  root given to the compiler.
- Every top-level definition is exported. Reference as `alias/name`,
  or `name` when the require has no `:as` and the name is unambiguous;
  ambiguity is an error naming both candidates.
- `fib.core` (primitives, `Option`, `Vec`, `Map`, `str` operations,
  `Send`) is required implicitly.
- Requires form a DAG; a cycle is an error.
- Type inference does not cross module boundaries: every exported
  `defun` has a fully generalised signature when its module is
  compiled, and ownership summaries (parameter convention and escape,
  `types.md` §6.4) are part of that interface. Modules are compiled in
  dependency order. Generic functions are monomorphised at use sites,
  so a module's compiled interface includes the bodies of its generic
  `defun`s (whole-program compilation; `types.md` §4.3).
- The program's entry point is `main` in the root namespace, with type
  `(-> i64)`.

---

## 7. Appendix: the twenty ownership cases in the proposed syntax

Headers are unchanged. Changes from the liar-syntax originals are
noted after each case and are syntactic only; each verdict is the one
in the header. `types.md` §7 has the mechanism that produces each
verdict.

### 01-return-part-of-argument.fib

```
;; spec:   §4
;; expect: accept
;; result: 1
;; audit:  clean
;; A function returns memory owned by its argument. The callee retains
;; it on return, so the caller's binding and the list each hold a count.
(defstruct (Box a) (v a))

(defun head (xs) (nth xs 0))

(defun main () -> i64
  (let ((l [(Box 1) (Box 2)]))
    (let ((h (head l)))
      (. h v))))
```

Changes: `box`/`unbox` become a library-style generic struct; `list`
becomes a vector literal; `first` (which returns an `Option`) becomes
`nth`, which traps out of range and returns the element itself, so the
case keeps a single object-returning call.

### 02-structural-sharing.fib

```
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

Changes: none.

### 03-store-borrowed-value.fib

```
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

Changes: none.

### 04-branch-dependent-owner.fib

```
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

Changes: none.

### 05-closures-share-state.fib

```
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; Two escaping closures share one captured mutable variable.
(defun make-counter ()
  (let ((n (cell 0)))
    [(fn () (set! n (+ @n 1)) @n)
     (fn () @n)]))

(defun main () -> i64
  (let ((c (make-counter)))
    (let ((inc (nth c 0))
          (get (nth c 1)))
      (inc)
      (inc)
      (get))))
```

Changes: `list` becomes a vector literal; both closures return `i64`
so the vector has one element type (`set!` returns `unit`).

### 06-capture-borrowed-param.fib

```
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

Changes: none.

### 07-recursive-accumulator.fib

```
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

Changes: none. The checker gives `acc` the `own` convention
(`types.md` §6.4) so the self tail call stays a tail call.

### 08-mutate-while-iterating.fib

```
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

Changes: none. `append` is the library function
`(defun append (&v x) (set! v (conj v x)))`.

### 09-iterator-outlives-source.fib

```
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

Changes: none.

### 10-atom-old-value.fib

```
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

Changes: none.

### 11-borrow-across-await.fib

```
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

Changes: none.

### 12-reject-same-binding-twice-inout.fib

```
;; spec:   §5 (proposed)
;; expect: reject
;; error:  passed to more than one & parameter
(defun bar (&a &b) (append &a 1) (append &b 2))

(defun main () -> i64
  (let ((x (cell [3 4])))
    (bar &x &x)
    (count @x)))
```

Changes: none.

### 13-reject-cell-crosses-thread.fib

```
;; spec:   §7
;; expect: reject
;; error:  cell cannot be shared between threads
(defun main () -> i64
  (let ((n (cell 0)))
    (pmap (fn (i) (set! n (+ @n i))) (range 10))
    @n))
```

Changes: none.

### 14-reject-inout-in-async.fib

```
;; spec:   §8
;; expect: reject
;; error:  & parameter in async function
(defun fill (&buf) (async (await (yield)) (append &buf 1)))

(defun main () -> i64
  (let ((b (cell [])))
    (block-on (fill &b))
    (count @b)))
```

Changes: none.

### 15-cycle-through-cell-leaks.fib

```
;; spec:   §6 (proposed)
;; expect: accept
;; result: 1
;; audit:  leak-cycle
;; A cell that ends up holding a vector containing itself. Under the
;; proposed rule this leaks, and the audit must report it as a cycle
;; leak, not as any other failure.
(defstruct Knot (inner (Cell (Vec Knot))))

(defun main () -> i64
  (let ((k (Knot (cell []))))
    (set! (. k inner) [k])
    (count @(. k inner))))
```

Changes: the original `(set! c [c])` needs a type `a = (Cell (Vec a))`,
which the occurs check rejects (`types.md` §1.7: recursion only
through nominal types). The cycle is now `k -> cell -> vector -> k`;
it still passes through exactly one cell, so the audit classifies it
as `leak-cycle`.

### 16-coordinated-update-single-atom.fib

```
;; spec:   §7
;; expect: accept
;; result: 200
;; audit:  clean
;; Two balances that must change together live in one atom and change
;; with one swap!, so no reader can see the money in flight.
(defstruct Accounts (a i64) (b i64))

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

Changes: field syntax `(a i64)` instead of `a: i64`.

### 17-inout-and-borrow-same-call.fib

```
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

Changes: none.

### 18-reject-inout-captured-by-escaping-closure.fib

```
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

Changes: none.

### 19-weak-parent-pointer.fib

```
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; A tree whose nodes point at their parent through a weak reference.
;; The strong edges form no cycle, so the whole tree is freed when main
;; returns.
(defstruct Node (parent (Option (Weak Node))) (children (Cell (Vec Node))))

(defun add-child (parent: Node) -> Node
  (let ((c (Node (some (weak parent)) (cell []))))
    (set! (. parent children) (conj @(. parent children) c))
    c))

(defun depth (n: Node) -> i64
  (match (. n parent)
    (nil 0)
    ((some w) (match (upgrade w)
                (nil 0)
                ((some p) (+ 1 (depth p)))))))

(defun main () -> i64
  (let ((root (Node nil (cell []))))
    (let ((a (add-child root)))
      (let ((b (add-child a)))
        (depth b)))))
```

Changes: fields are typed (`defstruct` requires it); a missing parent
is `(Option (Weak Node))` so `nil` and `(some (weak parent))` have one
type; `deref` on a weak reference is `upgrade`, returning
`(Option Node)`, consumed by `match`; `depth` annotates `n` because its
only clue to `n`'s type is a field access (`types.md` §3.4).

### 20-weak-ref-to-dead-object.fib

```
;; spec:   §6
;; expect: accept
;; result: 1
;; audit:  clean
;; The only strong reference dies with the inner let; deref then gives
;; nil rather than the freed object.
(defun main () -> i64
  (let ((w (let ((v (conj [] 1))) (weak v))))
    (if (nil? (upgrade w)) 1 0)))
```

Changes: `deref` becomes `upgrade`.

---

## 8. Open decisions

Each needs the owner's sign-off. Recommendation first, alternative
second.

1. **`&` only on `defun` parameters (§2.3).** Recommend: yes; closures
   use one calling convention and rule 2 of §2.14 stays a check on
   captures. Alternative: allow `&` on `fn` parameters, adding a
   write-back slot to the closure convention and a second capture rule.
2. **`@` on a cell returns an owned (+1) reference (§2.10).**
   Recommend: yes, one rule for cells and atoms, elided by the
   compiler when no write can intervene (`types.md` §6.7). Alternative:
   `@cell` is a borrow, with a rule forbidding `set!` on that cell
   while the borrow is live (a flow-sensitive check the checker would
   have to get right).
3. **`if` requires `bool`; no truthiness.** Recommend: yes.
   Alternative: `Option` and `bool` are both testable.
4. **No shadowing within a `defun` (§2.4).** Recommend: keep liar's
   rule; the ownership pass then keys everything by name. Alternative:
   ordinary lexical shadowing, keyed by binding identity.
5. **Variant names are unqualified and unique per namespace (§2.9).**
   Recommend: yes for now. Alternative: `Shape/circle`.
6. **`plet` is a macro and `pmap` a library function over `spawn`
   (§2.13).** Recommend: yes; one crossing rule. Alternative: make
   them core so their error messages can be worded per form.
7. **Single-threaded executor; `async` needs no `Send` (§2.15).**
   Recommend: yes for the first implementation. Alternative:
   work-stealing executor, `async` treated like `spawn` for `Send`.
8. **Signed overflow traps (§3).** Recommend: trap, with explicit
   `wrapping-*`. Alternative: wrap silently (lIR default).
9. **Macros are interpreted at compile time, no reflection (§2.17).**
   Recommend: yes. Alternative: liar's JIT-based expansion and ADR 023
   struct reflection.
10. **Non-exhaustive `match` is an error (§2.6).** Recommend: yes.
    Alternative: warn and trap at runtime.
11. **Case 15 and 19 rewrites.** The verdicts are unchanged, but 15 now
    goes through a nominal struct and 19 uses `Option`/`match`. Sign
    off that these still exercise the intended rule (a cell holding a
    reference back to itself; a weak back-pointer).
12. **`unit` and `()`.** Recommend: `()` is the unit value and `unit`
    its type; `nil` is only `Option`'s None. Alternative: `nil` doubles
    as unit (liar's convention).
