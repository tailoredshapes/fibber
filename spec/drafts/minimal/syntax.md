# fibber syntax — minimal-core draft

Status: draft, angle "minimal core". Authority: [ownership.md](../../ownership.md)
is decided; everything here is **Proposed** unless marked **Decided**, and a
Decided mark is used only where the point is a direct consequence of
ownership.md. Case verdicts in `cases/ownership/` are fixed; the appendix
rewrites their bodies into this syntax with headers unchanged.

Companion: [types.md](types.md) (type grammar, inference, the checker that
decides every ownership rule, and the lIR mapping).

Design rule for this draft: a form is **core** only if the checker must see it
on the surface to decide a rule, or it binds names or introduces a type.
Everything else is a macro over core forms or a library function. §4 states
the rule precisely and lists what falls on each side.

---

## 1. Reader

The reader turns text into **forms**. A form is a value of the built-in sum
type `Form` (§3.17); the same type is what macros receive and return, which
is what makes fibber homoiconic.

### 1.1 Lexical structure

| Class | Syntax | Form produced |
|---|---|---|
| whitespace | space, tab, newline, comma (`,` outside quasiquote is whitespace) | — |
| line comment | `;` to end of line | — |
| form comment | `#_` followed by one form | the next form is read and discarded |
| integer | `42`, `-7`, `0x1F`, `0b1010`, `1_000_000` | `(Int v)` |
| float | `3.14`, `-0.5`, `1e9`, `2.5e-3` (a `.` or an exponent makes it a float) | `(Flt v)` |
| string | `"..."` with escapes `\n \t \r \0 \\ \" \xNN \u{HHHH}`; UTF-8 | `(Str s)` |
| character | `\a`, `\\`, `\newline`, `\space`, `\tab`, `\u{1F600}` | `(Chr c)` |
| boolean | `true`, `false` | `(Bool b)` |
| nil | `nil` | `(Nil)` |
| keyword | `:name`, `:ns/name` | `(Kw s)` |
| symbol | anything else not starting with a digit, `:`, `\`, `#`, `"`; may contain `/` once to qualify (`m/name`); `.`, `&`, `@` are ordinary symbol characters except as below | `(Sym s)` |
| list | `( f1 f2 ... )` | `(List [f1 f2 ...])` |
| vector | `[ f1 f2 ... ]` | `(Vec [f1 f2 ...])` |
| map | `{ k1 v1 k2 v2 ... }` (even count required) | `(Map [k1 v1 k2 v2 ...])` |

Symbols are case-sensitive. `-` followed by a digit starts a number, not a
symbol. Numbers carry no type in the reader; the type of a literal is decided
by inference (types.md §2.1).

### 1.2 Prefix reader macros

Each rewrites to a list form. There are exactly five.

| Text | Reads as | Meaning |
|---|---|---|
| `'x` | `(quote x)` | the form `x` as a `Form` value |
| `` `x `` | `(quasiquote x)` | template (§3.17) |
| `,x` (inside quasiquote) | `(unquote x)` | splice one value |
| `,@x` (inside quasiquote) | `(unquote-splicing x)` | splice a sequence |
| `@x` | `(deref x)` | read a cell, atom or weak (§3.11) |
| `&x` | `(& x)` | in-out argument or parameter (§3.13); `x` must be a symbol |

`@` and `&` bind tightly: `@(. p children)` is `(deref (. p children))`;
`&x` is only meaningful in a parameter list or argument position and is an
error anywhere else.

### 1.3 Literal collections are library calls

`[e ...]` and `{k v ...}` in expression position are not core forms. After
macro expansion the compiler rewrites them:

```
[]              ⟹ (vec-empty)
[e1 e2 ... en]  ⟹ (conj (conj ... (conj (vec-empty) e1) ...) en)
{}              ⟹ (map-empty)
{k1 v1 ...}     ⟹ (assoc (assoc (map-empty) k1 v1) ...)
```

`conj`, `assoc`, `vec-empty`, `map-empty` are library functions (§4.3).
The compiler may fuse the chain into one allocation; that is an
optimisation, not a semantic. Inside `quote`/`quasiquote` the brackets stay
`Vec`/`Map` forms and are not rewritten.

---

## 2. Programs, expressions, evaluation order

A **program** is a sequence of top-level forms in one or more modules (§5).
Top-level forms are the definitions `defun`, `defstruct`, `defenum`,
`defprotocol`, `impl`, `defmacro`, `extern`, `ns`. Every other form is an
**expression**.

Expression evaluation is strict and **left to right, inner before outer**
(**Proposed**; liar ADR 008 kept):

- in a call `(f a1 ... an)`: `f`, then `a1` ... `an`, then the call;
- in `let`: each binding in order, each initialiser seeing the earlier ones;
- in `do`: each step in order, the value of the last is the value of the form;
- in `if`/`match`: the scrutinee, then exactly one branch;
- constructor and literal-collection arguments: left to right.

Only `plet` (a macro, §4.2) and `pmap`/`spawn` (library) evaluate anything
concurrently, and only closures given to them.

**Temporaries.** A value produced by an expression and not bound is owned by
the enclosing *step*: an argument temporary lives until the call it was
passed to returns; a `do` step's value dies when the step ends; a `let`
initialiser's value is owned by the binding. (Decided in consequence of
ownership.md §2 and §4: results are owned by the caller; the caller must
release them somewhere, and this fixes where.)

---

## 3. Core special forms

Grammar notation: `sym` symbol, `type` a type expression (types.md §1),
`expr` expression, `pat` pattern, `*` zero or more, `+` one or more,
`?` optional. Each entry gives grammar, static requirements, and the
evaluation rule.

### 3.1 `defun`

```
(defun name tparams? (param*) ret? body+)
tparams ::= [ tparam+ ]
tparam  ::= sym | (sym bound+)          ; bound: ProtocolName | (ProtocolName type+) | Send
param   ::= sym | sym: type | &sym | &sym: type
         | (sym type? :borrow) | (sym type? :escapes)   ; explicit escape kind, checked
ret     ::= -> type
```

Static: `name` is bound in the module's namespace before any body is
checked, so functions may be mutually recursive without forward declaration.
Parameter names must be distinct. A parameter or return type left
unannotated is inferred (types.md §3); a generic function (`tparams`
present) must be fully annotated. `&` parameters are the in-out mode of
§3.13. A function whose body is exactly one `async` form is an **async
function** (§3.14).

Evaluation: a named function is a global, captures nothing (Decided,
ownership.md §6), and is a value of function type when used in
non-head position (it is then an escaping-safe closure with no
environment).

Ownership: every parameter is a borrow for the duration of the call; the
result is owned by the caller (Decided, §4).

### 3.2 `fn`

```
(fn name? (param*) ret? body+)
```

An anonymous function that closes over the variables it uses from the
enclosing scopes. If `name` is given it is bound inside the body to the
closure itself, resolved through the closure's own code pointer and
environment, not by capture (Decided, §6: a local recursive closure does
not create a cycle).

Static: a `fn` may use `&` parameters only as a *callee*; it may
capture an enclosing `&` parameter only if the closure does not escape
the call (Decided, §5; error text in §3.13). The captured variable set is
computed by the checker (types.md §5.3) and is what decides whether the
closure may cross a thread (types.md §5.7).

Evaluation: creates a closure value; captures are read at creation time
(variables are immutable, cells are captured by reference, which is what
makes two closures share one cell — case 05).

### 3.3 `let`

```
(let ((sym expr)+) body+)
```

Sequential bindings; each initialiser sees the earlier ones. A name may not
rebind a name already visible in the same function (**Proposed**: liar ADR
006 kept, it removes a class of `set!` confusion; alternative: allow
shadowing). Destructuring is not core: `(let (((Point x y) p)) ...)` is a
prelude macro over `match`.

Evaluation: evaluate each initialiser in order, bind, evaluate body, return
the last body value. Bindings die in reverse order when the body finishes.

Ownership: a binding **owns** what it is bound to (a fresh object, a call
result, a deref) or **borrows** it (another variable, a field path). The
checker decides which (types.md §5.1); the program cannot tell the
difference.

### 3.4 `if`

```
(if expr expr expr)
```

The test must be `bool`; both branches required and of one type.
`when`, `unless`, `cond`, `and`, `or` are prelude macros.

### 3.5 `do`

```
(do expr*)
```

Evaluates steps in order; value of the last, `()` (unit) if empty. Each step
except the last must be of type `unit` (**Proposed**: it catches discarded
results; alternative: allow any type and discard).

### 3.6 `match`

```
(match expr clause+)
clause ::= (pat body+)
pat    ::= _                       ; wildcard
         | sym                     ; binds
         | literal                 ; integer, float, string, char, bool, keyword
         | nil                     ; the None variant of (option T)
         | (Variant pat*)          ; sum-type variant, positional
         | (Struct pat*)           ; struct, positional
         | (some pat)              ; the Some variant of (option T)
```

Static: clauses are tried top to bottom; the set must be exhaustive for
sum types and `option`, and must end with `_` or a variable for scalars
and strings. Non-exhaustive is a compile error. `match` is the only
eliminator for sum types and `option`; there is no `nil?` in the core
(it is a prelude function written with `match`).

Evaluation: evaluate the scrutinee once; select the first clause whose
pattern matches; bind pattern variables; evaluate the body.

Ownership: pattern variables **borrow** sub-objects of the scrutinee (they
are field paths, types.md §5.1). A pattern variable bound to a scalar is a
copy.

### 3.7 `defstruct`

```
(defstruct Name tparams? (field+))
field ::= sym: type
```

Introduces a nominal product type, a constructor function `Name` taking
the fields positionally, and enables `(. e field)`. Structs are immutable
objects (Decided, §1); a field of type `(cell T)` is the only way to make
a struct mutable in place. Recursive types are allowed by name
(`(defstruct Node (parent: (option (weak Node)) ...))`).

### 3.8 `.` (field access)

```
(. expr field)
```

`expr` must have a struct type known after inference; `field` is a symbol.
Result: the field value. Ownership: a borrow of the sub-object (or a copy
for scalars). To write a field the field must be a cell: `(set! (. p f) v)`.

### 3.9 `defenum`

```
(defenum Name tparams? variant+)
variant ::= (Variant) | (Variant field+)
```

Introduces a nominal sum type and one constructor function per variant.
`(option T)` is defined in the prelude as `(defenum option [T] (nil) (some v: T))`
and its variants get the reader spellings `nil` and `(some x)`; there is
nothing else special about it. A `defenum` whose variants all have no fields
is a scalar (types.md §7.1).

### 3.10 `defprotocol`, `impl`

```
(defprotocol Name tparams? method+)
method ::= (mname (self-param param*) ret)
self-param ::= self | &self

(impl tparams? proto type method-impl+)
proto  ::= Name | (Name type+)
method-impl ::= (mname (self-param param*) ret? body+)
```

`defprotocol` introduces a nominal protocol; `impl` declares that `type`
implements it and provides every method (all required; no defaults in the
core — default methods are a prelude macro that expands to per-type
`impl`s). One `impl` per (protocol head, type head) pair per program. A
protocol's type parameters are determined by `Self` (types.md §4).

Method parameters other than `self` default to the **escaping** kind; mark
`(x type :borrow)` to promise that no implementation makes `x` escape
(types.md §5.4). The checker rejects an `impl` that breaks the promise.

Dispatch: static when the receiver type is known, dynamic through
`(any Proto)` (types.md §4).

### 3.11 Cells, atoms, weak references: functions, not forms

These are **builtin functions** with ordinary call syntax; they are listed
here because the reader gives `@` to them and because the checker treats
their types specially. None is a special form.

| Call | Type (types.md §1) | Meaning |
|---|---|---|
| `(cell v)` | `T → (cell T)` | new cell holding `v` |
| `(deref c)`, `@c` | protocol `Deref` | cell → value (owned +1); atom → value (owned +1); weak → `(option T)` (owned +1 if alive) |
| `(set! c v)` | `(cell T) T → unit` | store `v`, release the old value |
| `(atom v)` | `T → (atom T)`, `T: Send` | new atom |
| `(swap! a f)` | `(atom T) (fn (T) T) → T` | replace atomically with `(f old)`; `f` may run more than once; returns the new value |
| `(reset! a v)` | `(atom T) T → unit` | replace |
| `(weak x)` | `T → (weak T)`, `T` an object type | weak reference; does not keep `x` alive |

`set!` on a plain variable is an error: variables are immutable; only cells
change (Decided, §6). Captured mutable state is written as an explicit
`(cell ...)`; there is no implicit boxing of assigned variables.

### 3.12 Threads: functions and a macro

| Call | Type | Meaning |
|---|---|---|
| `(spawn f)` | `(fn () T) → (task T)`, `T: Send`, `f` send-safe | run `f` on another thread |
| `(join t)` | `(task T) → T` | wait, take the result (owned) |
| `(pmap f xs)` | library, `f` send-safe | map in parallel; results in order |

`plet` is a prelude macro (§4.2). The rule "only immutable objects or atoms
may cross" (Decided, §7) is the `Send` predicate on the type of `f`
(types.md §5.7); the reader-visible consequence is that a closure that
captures a `(cell T)` cannot be given to `spawn`, `pmap` or `plet`.

### 3.13 `&` — in-out parameters and arguments

```
param:    &sym | &sym: type
argument: &sym                 ; sym must be a variable of type (cell T)
```

Meaning (Decided, §5, copy-in copy-out; the desugaring is the Proposed
representation of that meaning):

```
(f &x a)            ; x : (cell T), f's first parameter is &v: T
⟹  (let ((t (cell @x)))      ; copy-in: read x (+1), private cell
      (let ((r (f t a)))      ; callee's v is the private cell t
        (set! x @t)           ; write-back: x takes t's final value
        r))
```

Inside the callee, `v` is a `(cell T)`: read it with `@v`, pass it on with
`&v`, update it with the in-place builtins (`push!`, `vec-set!`, ...) or
`(set! v ...)`. The caller's `x` is untouched until the write-back, so a
plain `@x` in the same call stays valid (case 17).

Static requirements (all checked on the surface form, before desugaring):

1. **Distinct variables** (Decided, §5): the `&` arguments of one call must
   name distinct variables. Error text: `passed to more than one & parameter`
   (case 12). Passing `&x` and `@x` in one call is fine (case 17).
2. **No escaping capture** (Decided, §5): a closure that captures a `&`
   parameter must not escape the call that owns it. Error text:
   `& parameter captured by escaping closure` (case 18). "Escapes" is decided
   by types.md §5.3; in particular such a closure may be passed only to a
   parameter of kind `:borrow`.
3. **Not in async functions** (Decided, §8): a `&` parameter on an async
   function is an error: `& parameter in async function` (case 14).
4. The argument variable must have type `(cell T)` where `T` is the
   parameter's type. `&` on a non-variable expression is an error (the
   distinct-variables check is syntactic, so the argument must be a name).

In-place update (Decided, §5): the in-place builtins on `&v` update the
object in `v` in place when its count is one, and copy first otherwise.
Because copy-in retains, the first update in a call always sees a count
of at least two and copies; later updates in the same call are in place.
This is the cost of keeping the caller's `x` readable during the call
(§5, case 17) and is discussed in Open decision 3.

### 3.14 `async`, `await`

```
(async body+)          ; expression of type (task T) where body : T
(await expr)           ; only inside async; expr : (task T), result T
```

`async` creates a task: a heap object holding the body's captured variables
and its suspended state. Evaluating the form does not run the body; the
executor does (`block-on`, `join`, `spawn` are library). `await` suspends
the task until the awaited task completes and yields its result (owned).

Ownership (Decided, §8): an `async` body is treated exactly as an
**escaping closure**: every variable it captures is retained when the task
is created. That is what "any parameter or local used after an `await` is
retained on entry" means in this design; locals created inside the body
live in the task's own frame and follow the ordinary scope rules there.
`&` parameters are not allowed on async functions (§3.13 rule 3).

Static: `await` outside `async` is an error. An async function's declared
return type, if given, is `(task T)`.

### 3.15 `unsafe`, `extern`

```
(unsafe body+)
(extern name (type*) -> type opts?)     ; opts: :varargs, (:retains sym+)
```

`extern` declares a foreign function with C calling convention; its
parameter and result types are scalars or `ptr`. Calls to externs and the
raw-pointer builtins (`ptr+`, `load-byte`, `store-byte`, `malloc`, `free`,
`addr-of`, `from-addr`) are permitted only lexically inside `unsafe`.
Nothing else changes inside `unsafe`: counting and type checking still
apply to fibber objects (Decided, §9). A fibber object passed to an extern
is a borrow for the duration of the call unless the parameter is named in
`:retains`, in which case the compiler retains it before the call and the
foreign side owns that count.

### 3.16 `quote`

```
(quote form)      ; 'form
```

Value: the `Form` (§3.17) for `form`, unevaluated. Type `Form`.

### 3.17 `defmacro`, quasiquote, `Form`

```
(defmacro name (param*) body+)
param ::= sym | ... sym          ; "... rest" binds the remaining forms as (vec Form)
```

A macro is a function from forms to a form, run at expansion time. Its
parameters have type `Form` (or `(vec Form)` for the rest parameter) and
its body must produce a `Form`. The body is ordinary fibber and is
evaluated by the reference interpreter's evaluator over the macro's module
(**Proposed**: this makes macro-time evaluation the same executable spec as
run time; the compiler may JIT it instead but must agree).

`Form` is the built-in sum type

```
(defenum Form
  (Sym name: str) (Kw name: str)
  (Int v: i64) (Flt v: f64) (Str v: str) (Chr v: char) (Bool v: bool) (Nil)
  (List items: (vec Form)) (Vec items: (vec Form)) (Map items: (vec Form)))
```

Quasiquote is rewritten by the expander, not evaluated:

```
`atom                 ⟹ (quote atom)
`(a ,b ,@cs d)        ⟹ (List (concat [ 'a ] [ b ] cs [ 'd ]))     ; b : Form, cs : (vec Form)
`[ ... ]  `{ ... }    ⟹ same with Vec / Map
```

`(gensym "prefix")` returns a fresh `Sym`. There is no automatic hygiene
(**Proposed**; alternative in Open decision 7). Expansion is
outermost-first, repeated until no macro call remains; a macro must be
defined earlier in the module (or in a required module) than its first use.

### 3.18 `ns`

See §5.

---

## 4. Core versus macro versus library

### 4.1 The rule

A form is **core** if and only if at least one of:

1. the checker must see it on the surface to decide a rule of ownership.md
   (`&`: distinct variables; `fn`/`async`: capture and escape; `match`:
   exhaustiveness and sub-object borrows; `.`: field paths);
2. it binds names or introduces a type or an implementation (`defun`,
   `fn`, `let`, `defstruct`, `defenum`, `defprotocol`, `impl`, `defmacro`,
   `extern`, `ns`);
3. it changes control flow or evaluation regime in a way no function can
   (`if`, `do`, `match`, `async`, `await`, `unsafe`, `quote`).

Everything with an ordinary call shape whose meaning is "evaluate the
arguments, then do something" is a **library function**, even when the
compiler implements it directly (a *builtin*). Everything that is a pure
rewrite of forms is a **macro** in the prelude.

Core forms (18): `defun fn let if do match defstruct . defenum defprotocol
impl & async await unsafe extern quote defmacro ns` — plus the reader-level
`quasiquote`/`unquote`/`unquote-splicing`, which exist only until expansion.

### 4.2 Prelude macros (normative list for the reference implementation)

| Macro | Expands to |
|---|---|
| `when`, `unless`, `cond`, `and`, `or`, `not`-free `if` chains | `if` |
| `let` with patterns, `if-let`, `when-let` | `match` |
| `loop`/`recur`-style `while`, `dotimes`, `for-each` over a range | named `fn` + self tail call |
| `->`, `->>`, `doto` | call rewriting |
| `plet` | `(plet ((a e1) (b e2)) body)` ⟹ `(let ((ta (spawn (fn () e1))) (tb (spawn (fn () e2)))) (let ((a (join ta)) (b (join tb))) body))`; bindings may not refer to each other |
| `defstruct ... :impl`, protocol default methods | `impl` per type |
| `assert`, `dbg` | `if` + `panic` |

### 4.3 Builtins and library (types in types.md §1.5)

Builtins the compiler knows by name: arithmetic and comparison on scalars
(`+ - * / rem = != < <= > >=`, float and bitwise variants, conversions
`i8 ... i64 f32 f64 char`), `cell set! deref atom swap! reset! weak spawn
join panic`, `vec-empty map-empty conj assoc nth count push! vec-set! pop!
map-put! map-del!`, string primitives, `Form` constructors, `gensym`, the
raw-pointer set, `block-on yield`.

Library (written in fibber, in `lib/`): `option` helpers, `list`,
persistent `vec`/`map`/`set` beyond the builtin core operations, `Seq`,
`Iter` (`iter`, `next`, `filter-iter`, `collect`), `range`, `pmap`,
`for-each`, `starts-with?`, `box`/`unbox`, `even?`, `length`, I/O.

---

## 5. Modules

```
(ns name clause*)
clause ::= (:require [name :as alias]+) | (:use name+)
```

One `ns` form, first in the file, names the module (dotted; `a.b` lives at
`a/b.fib`). `:require` makes `alias/x` refer to `x` in that module;
`:use` brings all of a module's top-level names in unqualified. Every
top-level definition is exported (no private names in the minimal core;
Open decision 8). A name defined locally shadows a `:use`d one; two `:use`d
modules exporting the same name make that name an error when referenced
unqualified. Requires may not be cyclic. `lib.prelude` is implicitly
`:use`d.

A module compiles against the **interfaces** of its requires: every
exported function's solved signature including escape kinds and closure
send attributes, every type, protocol and impl, every macro (as forms), and
the bodies of generic functions (needed for instantiation; types.md §3.6).

---

## Appendix A — the 20 ownership cases in this syntax

Headers (the `;;` lines) are the originals, unchanged. Bodies use the syntax
above; library names used are listed in §4.3. Where the rewrite is more
than a spelling change, a note follows the case.

### 01-return-part-of-argument.fib

```lisp
;; spec:   §4
;; expect: accept
;; result: 1
;; audit:  clean
;; A function returns memory owned by its argument. The callee retains
;; it on return, so the caller's binding and the list each hold a count.
(defstruct Box (v: i64))

(defun head (xs: (vec Box)) -> Box (nth xs 0))

(defun main () -> i64
  (let ((l [(Box 1) (Box 2)]))
    (let ((h (head l)))
      (. h v))))
```
Note: `list`/`box`/`unbox` replaced by a vector and a one-field struct;
`nth` returns a borrow of an element, and returning it retains it (§4).

### 02-structural-sharing.fib

```lisp
;; spec:   §5
;; expect: accept
;; result: 7
;; audit:  clean
;; The new vector shares nodes with v, and outlives v.
(defun add4 (v: (vec i64)) -> (vec i64) (conj v 4))

(defun make () -> (vec i64)
  (let ((v [1 2 3]))
    (add4 v)))

(defun main () -> i64
  (let ((w (make)))
    (+ (count w) (nth w 2))))
```

### 03-store-borrowed-value.fib

```lisp
;; spec:   §3.2
;; expect: accept
;; result: 5
;; audit:  clean
;; A borrowed value is stored into a collection that outlives it, and
;; the same value goes into two collections.
(defun remember (coll: (vec str) item: str) -> (vec str) (conj coll item))

(defun main () -> i64
  (let ((keep (let ((s "hello"))
                (let ((a (remember [] s))
                      (b (remember [] s)))
                  a))))
    (count (nth keep 0))))
```

### 04-branch-dependent-owner.fib

```lisp
;; spec:   §4
;; expect: accept
;; result: 5
;; audit:  clean
;; One branch returns the borrowed argument, the other a fresh object.
;; Both are returned owned (+1), so the caller treats them the same.
(defun pick (flag: bool x: str) -> str (if flag x "fresh"))

(defun main () -> i64
  (let ((s "hello"))
    (let ((a (pick true s))
          (b (pick false s)))
      (count a))))
```

### 05-closures-share-state.fib

```lisp
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; Two escaping closures share one captured mutable variable.
(defstruct Counter (inc: (fn () unit) get: (fn () i64)))

(defun make-counter () -> Counter
  (let ((n (cell 0)))
    (Counter (fn () (set! n (+ @n 1)))
             (fn () @n))))

(defun main () -> i64
  (let ((c (make-counter)))
    (let ((inc (. c inc))
          (get (. c get)))
      (inc)
      (inc)
      (get))))
```
Note: the two closures have different types (`set!` returns `unit`), so a
struct replaces the heterogeneous list.

### 06-capture-borrowed-param.fib

```lisp
;; spec:   §3.3
;; expect: accept
;; result: 1
;; audit:  clean
;; An escaping closure captures a borrowed parameter; the object it was
;; borrowed from dies before the closure is called.
(defun matcher (prefix: str) -> (fn (str) bool)
  (fn (s: str) (starts-with? s prefix)))

(defun main () -> i64
  (let ((m (let ((p "ab")) (matcher p))))
    (if (m "abc") 1 0)))
```

### 07-recursive-accumulator.fib

```lisp
;; spec:   §4, §5
;; expect: accept
;; result: 100000
;; audit:  clean
;; Each call's accumulator is a new version sharing structure with the
;; previous one, passed through a tail call.
(defun build (n: i64 acc: (vec i64)) -> (vec i64)
  (if (= n 0) acc (build (- n 1) (conj acc n))))

(defun main () -> i64
  (count (build 100000 [])))
```
Note: the self tail call is compiled and interpreted as a loop
(types.md §7.8); the owned temporary `(conj acc n)` becomes the loop's
owned accumulator.

### 08-mutate-while-iterating.fib

```lisp
;; spec:   §5
;; expect: accept
;; result: 6
;; audit:  clean
;; The loop is still reading v when append replaces it. The iteration
;; holds a count, so append copies instead of updating in place, and the
;; loop sees the original three elements.
(defun dup-all (&v: (vec i64)) -> unit
  (for-each @v (fn (x: i64) (push! &v x))))

(defun main () -> i64
  (let ((v (cell [1 2 3])))
    (dup-all &v)
    (count @v)))
```
Note: `append` is `push!`; inside the callee the `&` parameter is a cell
and is read with `@v`, which is exactly the retained read that makes the
iteration hold a count.

### 09-iterator-outlives-source.fib

```lisp
;; spec:   §3.1
;; expect: accept
;; result: 2
;; audit:  clean
;; The returned iterator refers to a vector that the caller's scope
;; has already let go of.
(defun evens (v: (vec i64)) (filter-iter (iter v) even?))

(defun main () -> i64
  (let ((it (let ((v [1 2 3 4])) (evens v))))
    (count (collect it))))
```
Note: the return type of `evens` is inferred (an iterator struct type);
`iter` stores `v` into the iterator, which is the escape that retains it.

### 10-atom-old-value.fib

```lisp
;; spec:   §7
;; expect: accept
;; result: 1000
;; audit:  clean
;; Readers hold old values while writers replace them.
(defun main () -> i64
  (plet ((a (atom [])))
    (pmap (fn (i: i64)
            (let ((snapshot @a))
              (swap! a (fn (c: (vec i64)) (conj c i)))
              (count snapshot)))
          (range 1000))
    (count @a)))
```

### 11-borrow-across-await.fib

```lisp
;; spec:   §8
;; expect: accept
;; result: 5
;; audit:  clean
;; The parameter is used after the await, so it is retained on entry
;; and outlives the caller's scope.
(defun measure (s: str) -> (task i64) (async (await (yield)) (count s)))

(defun main () -> i64
  (let ((task (let ((s "hello")) (measure s))))
    (block-on task)))
```

### 12-reject-same-binding-twice-inout.fib

```lisp
;; spec:   §5 (proposed)
;; expect: reject
;; error:  passed to more than one & parameter
(defun bar (&a: (vec i64) &b: (vec i64)) -> unit (push! &a 1) (push! &b 2))

(defun main () -> i64
  (let ((x (cell [3 4])))
    (bar &x &x)
    (count @x)))
```

### 13-reject-cell-crosses-thread.fib

```lisp
;; spec:   §7
;; expect: reject
;; error:  cell cannot be shared between threads
(defun main () -> i64
  (let ((n (cell 0)))
    (pmap (fn (i: i64) (set! n (+ @n i))) (range 10))
    @n))
```

### 14-reject-inout-in-async.fib

```lisp
;; spec:   §8
;; expect: reject
;; error:  & parameter in async function
(defun fill (&buf: (vec i64)) -> (task unit) (async (await (yield)) (push! &buf 1)))

(defun main () -> i64
  (let ((b (cell [])))
    (block-on (fill &b))
    (count @b)))
```

### 15-cycle-through-cell-leaks.fib

```lisp
;; spec:   §6 (proposed)
;; expect: accept
;; result: 1
;; audit:  leak-cycle
;; A cell that ends up holding a vector containing itself. Under the
;; proposed rule this leaks, and the audit must report it as a cycle
;; leak, not as any other failure.
(defstruct Knot (items: (cell (vec Knot))))

(defun main () -> i64
  (let ((k (Knot (cell []))))
    (set! (. k items) [k])
    (count @(. k items))))
```
Note: the original `(set! c [c])` needs the type `(cell (vec X))` with
`X` equal to itself, which the type system rejects (occurs check). The
knot is tied nominally through a struct instead; the cycle is still
through a cell, so the verdict is unchanged.

### 16-coordinated-update-single-atom.fib

```lisp
;; spec:   §7
;; expect: accept
;; result: 200
;; audit:  clean
;; Two balances that must change together live in one atom and change
;; with one swap!, so no reader can see the money in flight.
(defstruct Accounts (a: i64 b: i64))

(defun transfer (accts: (atom Accounts) amount: i64) -> Accounts
  (swap! accts (fn (x: Accounts) (Accounts (- (. x a) amount) (+ (. x b) amount)))))

(defun main () -> i64
  (plet ((accts (atom (Accounts 100 100))))
    (pmap (fn (i: i64)
            (transfer accts 1)
            (let ((snap @accts)) (+ (. snap a) (. snap b))))
          (range 1000))
    (let ((final @accts))
      (+ (. final a) (. final b)))))
```
Note: `(transfer accts 1)` as a non-final `do` step must be `unit`; the
prelude's `pmap` closure body is a `do`, so this case relies on Open
decision 5 (allow discarding non-unit steps) or on writing
`(let ((_ (transfer accts 1))) ...)`. The draft takes the former.

### 17-inout-and-borrow-same-call.fib

```lisp
;; spec:   §5
;; expect: accept
;; result: 4
;; audit:  clean
;; The same variable passed in-out and as a plain borrow in one call.
;; The caller's variable is untouched until the write-back, so the
;; borrow stays valid for the whole call.
(defun push-count (&v: (vec i64) x: (vec i64)) -> unit (push! &v (count x)))

(defun main () -> i64
  (let ((v (cell [1 2 3])))
    (push-count &v @v)
    (count @v)))
```

### 18-reject-inout-captured-by-escaping-closure.fib

```lisp
;; spec:   §5
;; expect: reject
;; error:  & parameter captured by escaping closure
(defun make-pusher (&v: (vec i64)) -> (fn (i64) unit) (fn (x: i64) (push! &v x)))

(defun main () -> i64
  (let ((v (cell [])))
    (let ((push (make-pusher &v)))
      (push 1)
      (count @v))))
```

### 19-weak-parent-pointer.fib

```lisp
;; spec:   §6
;; expect: accept
;; result: 2
;; audit:  clean
;; A tree whose nodes point at their parent through a weak reference.
;; The strong edges form no cycle, so the whole tree is freed when main
;; returns.
(defstruct Node (parent: (option (weak Node)) children: (cell (vec Node))))

(defun add-child (parent: Node) -> Node
  (let ((c (Node (some (weak parent)) (cell []))))
    (set! (. parent children) (conj @(. parent children) c))
    c))

(defun depth (n: Node) -> i64
  (match (. n parent)
    (nil 0)
    ((some w) (match (deref w)
                (nil 0)
                ((some p) (+ 1 (depth p)))))))

(defun main () -> i64
  (let ((root (Node nil (cell []))))
    (let ((a (add-child root)))
      (let ((b (add-child a)))
        (depth b)))))
```
Note: `nil?` plus a use of the possibly-nil value is replaced by `match`
on `(option ...)`, the only way the core lets a nil be eliminated.

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
    (match (deref w)
      (nil 1)
      ((some _) 0))))
```

---

## Open decisions (syntax)

Each needs the owner's sign-off. Recommendation first, alternative second.

1. **`&` parameters are cells inside the callee (`@v` to read).**
   Recommend: yes — one mechanism (cells) for all mutation, the desugaring
   in §3.13 is the whole semantics, and the retained read is what makes
   case 08's "iteration holds a count" true by construction. Alternative:
   auto-deref (`v` reads the value, as the original cases wrote it), which
   needs a separate rule that retains reads of `v` across any call that
   could write `v` (types.md Open decision 3 states it) and a second kind
   of variable in the checker.

2. **Explicit `(cell ...)` for captured mutable state; `set!` only on
   cells.** Recommend: yes — no implicit boxing, the escape analysis never
   has to guess which variables are mutable. Alternative: `set!` on a
   captured `let` variable implicitly makes it a cell (liar), which hides
   an allocation and a count behind an assignment.

3. **Copy-in retains, so the first in-place update per call copies.**
   Recommend: accept — it is what §5's "caller's `x` is untouched" costs,
   and with persistent collections the copy is a path copy. Alternative:
   move-in (empty the caller's cell for the call), which breaks
   `(f &x @x)` (case 17) and was rejected by §5's text.

4. **No shadowing within a function (liar ADR 006 kept).** Recommend:
   keep for now; it simplifies the alias analysis (one name, one root).
   Alternative: allow nested shadowing; the checker would key on binding
   sites rather than names.

5. **Non-final `do` steps must be `unit`.** Recommend: relax to "any type,
   value discarded" — case 16 and most `pmap` bodies discard results;
   the `unit` rule would force `let ((_ ...))` noise. Alternative: keep
   strict and add a `(discard e)` builtin.

6. **`swap!` returns the new value; `set!`/`reset!` return `unit`.**
   Recommend: as stated (Clojure's convention; case 16 reads naturally).
   Alternative: all mutators return `unit`.

7. **Unhygienic macros with `gensym`.** Recommend: yes for the core; it
   keeps the expander a pure `Form → Form` function. Alternative: renaming
   hygiene (syntax objects with scopes), a much larger expander.

8. **All top-level names exported; no `private`.** Recommend: yes for
   now. Alternative: `(defun- ...)`/`:private` metadata; cheap to add
   later, no ownership consequence.

9. **`option` is an ordinary `defenum` with reader spellings `nil` and
   `(some x)`; no implicit lifting of `T` to `(option T)`.** Recommend:
   yes — it keeps inference pure unification (no subsumption) and makes
   every nil check a `match`. Alternative: nullable object types with
   `T <: (option T)` at argument positions, which needs bidirectional
   checking and a subtyping rule.

10. **Literal vectors/maps desugar to `conj`/`assoc` chains.** Recommend:
    yes (no variadic machinery in the core). Alternative: a builtin
    variadic `vector` with a special typing rule.

11. **Self tail calls only are guaranteed loops.** Recommend: yes — the
    owned-temporary argument problem (case 07 note) is solved by turning
    parameters into owned loop slots; general `musttail` with owned
    arguments needs an owned calling convention. Alternative: guarantee
    all tail calls via an "owned argument" convention chosen per call
    site (types.md Open decision 6).
