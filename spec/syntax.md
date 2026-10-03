# fibber syntax

Status: signed off. This file grew out of the three drafts under
`spec/drafts/` (see `spec/drafts/SYNTHESIS.md`); on 2026-09-27 the owner
accepted every open decision of the three review rounds as recommended,
with the amendments D1–D7 recorded in the decision table at the end of
this file. Every rule here is now **Decided** unless marked
**Proposed**; a "D*n*" beside a rule names the amendment it comes from.
Authority: [ownership.md](ownership.md) is Decided and wins where the
two meet. The case verdicts in `cases/ownership/` are fixed; Appendix A
rewrites the case bodies into this syntax with their headers unchanged.

Companion: [types.md](types.md) gives the type grammar, the typing rule
of every form here, the inference algorithm, the ownership checker that
decides every rule of ownership.md, the case table and the lIR mapping.
Section references of the form "types §n" are to that file; "§n" alone
is ownership.md.

Design rule (**Decided**): a form is **core** only if the checker must see it
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
| whitespace | space, tab, newline, and the comma (`[1,2]` is `[1 2]`) | none |
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
| ratio | `7/2`, `-7/2`: decimal digits, `/`, decimal digits, the sign only on the first and no suffix, no `_`, no radix (`7/-2`, `7/2i32`, `0x7/2` are read errors, as every number-like token that is not a number) | `(List [(Sym "/") (Int 7 i64) (Int 2 i64)])`, which is the call `(/ 7 2)` |
| list | `( f1 f2 ... )` | `(List [f1 f2 ...])` |
| vector | `[ f1 f2 ... ]` | `(Vec [f1 f2 ...])` |
| map | `{ k1 v1 k2 v2 ... }` (odd count is a read error) | `(Map [k1 v1 k2 v2 ...])` |
| set | `#{ f1 f2 ... }` | `(List [(Sym "hash-set") f1 f2 ...])`, which is the call `(hash-set f1 f2 ...)`; any count, no pairing |
| function | `#( f1 f2 ... )` (§1.2) | `(fn (%1 ... %n) (f1 f2 ...))` |

A literal that does not fit its width (`300i8`, `9223372036854775808`) is a
read error, never a wrap. A suffix that names no width (`2.5f16`, `1i128`) is a
read error too, and so is the same literal built by a macro: a `(Flt v
w)` form whose `w` is not `:f32` or `:f64`, or an `(Int v w)` whose `w`
is not one of the four integer widths, is an error where the macro's
expansion is turned back into code (**Decided**, owner, 2026-09-28;
case 105, as case 93 is for a value out of its width). A `Form` value
built at run time is data and is not checked until it is compiled. Numbers carry their width in the reader; there
is no literal polymorphism (types §1.1; **Decided**, D3).

**Symbols.** A symbol is a maximal run of characters that are not
whitespace, `( ) [ ] { } " ; ' `` ` `` `,` `@` `~` `\`, not starting with a
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
| `~x` | `(unquote x)` | inside a quasiquote: splice one form |
| `~@x` | `(unquote-splicing x)` | inside a quasiquote: splice a `(Vec Form)` |
| `@x` | `(deref x)` | read a cell, atom, weak reference or task (§3.11) |
| `&x` | `(& x)` | in-out argument or parameter (§3.13); `x` must be a symbol |

The comma is whitespace everywhere, as in Clojure: `[1,2]` and `{:a 1, :b 2}`
read as `[1 2]` and `{:a 1 :b 2}`, and `,x` is the symbol `x` (before E14 a
comma touching a form was `unquote`; the unquote is now `~`). `~` is not a
symbol constituent. `@` and `&` bind tightly to the following form:
`@(. p children)` is `(deref (. p children))`. `&` is only meaningful in
a parameter list or in argument position; anywhere else `(& x)` is an
error at expansion.

### 1.3 Source positions

Every form carries a position (file, line, column). Positions survive
macro expansion: a form produced by a macro carries the position of the
macro call, unless the macro built it from an input form, which keeps
its own. Every compile error names a position. (**Decided**.)

### 1.4 Literal collections are library calls

`[e ...]` and `{k v ...}` in expression position are not core forms. After
macro expansion the compiler rewrites them:

```
[]              ⟹ (fib.prelude/vec-empty)
[e1 e2 ... en]  ⟹ (fib.prelude/vec-conj (fib.prelude/vec-conj ... (fib.prelude/vec-conj (fib.prelude/vec-empty) e1) ...) en)
{}              ⟹ (fib.prelude/map-empty)
{k1 v1 ...}     ⟹ (fib.prelude/map-assoc (fib.prelude/map-assoc (fib.prelude/map-empty) k1 v1) ...)
```

`vec-empty`, `vec-conj`, `map-empty` and `map-assoc` are prelude functions
(§4.5), not the collection protocols' `conj` and `assoc`: the rewrite writes
them `fib.prelude/NAME` (§4.4, hygiene), so a user binding of `conj` or
`vec-conj` does not change what a literal means, and a `def` initialiser
that writes `(conj ..)` or `(assoc ..)` by hand is not a constant expression
while the literal is (stdlib design §6.3, tranche 1 R2). The
compiler may fuse the chain into one allocation; that is an
optimisation, not a semantic. Inside `quote`/`quasiquote` the brackets
stay `Vec`/`Map` forms and are not rewritten, so macros see `[x y]` as
data. In a pattern (§3.6) brackets are a **vector pattern**, matching
a `(Vec T)` by its length and elements; braces are not allowed in a
pattern (**Decided** (owner, 2026-09-28), replacing the v1 rule "no
brackets in patterns"; types §10, "Decided on vector patterns and
guards"). A bracket in a pattern is therefore never a `Form`: a `Vec`
form inside a quoted or quasiquoted template stays data until the
template's result is expanded, and only then, if it stands in pattern
position, is it a vector pattern.

### 1.5 Annotations

`x: T` in a binding position annotates `x` with the type `T` (grammar in
types §1). `-> T` after a parameter list annotates the result. Both are
optional everywhere except where types §3.8 says they are required.

### 1.6 What the reader decides where §1 is silent (**Proposed**)

The reference reader (`crates/fibref/src/syntax/`, each choice pinned by
a test there) and the self-hosted one (`compiler/syntax/`, which reads
the same text the same way, `spec/bootstrap.md`) decide these points,
which the rules above leave open:

- A carriage return is whitespace, and `\r\n` ends one line; a lone
  `\r` does not end a line or a `;` comment. A byte order mark is
  skipped at offset 0 and is an error anywhere else.
- A control character, and any Unicode white space other than space,
  tab, line feed and carriage return, is an error outside a string and a
  comment (it would otherwise be an invisible symbol constituent).
- `'` and `` ` `` may be separated from their form by whitespace and
  comments; `@`, `&`, `~` and `~@` must touch theirs. `&` not followed
  by a form is the symbol `&`; `&` applied to anything but a symbol is
  a read error. A run of tildes is that many nested unquotes (`~~x`).
- `#(` .. `)` (E8, E14) reads the forms up to the `)` as the body of an
  anonymous function: `#(f % 2)` is `(fn (%1) (f %1 2))`. Every symbol
  `%N` met between the `#(` and its `)`, at any depth and also in a
  discarded form, is a parameter; a bare `%` is `%1` and is read as the
  symbol `%1`; the parameter list is `%1` to `%n`, `n` the largest index
  used, and an index never used is still a parameter, so `#(f %3)` takes
  three. The body is one list of the forms, so `#(f 1)` is `(fn () (f 1))`
  and `#()` is `(fn () ())`. `N` is written without a leading zero and is
  at most 255: `%0`, `%01` and `%256` are the read error `BadFnParam`.
  `#(` inside a `#(` is the read error `NestedFn` (at the inner `#(`), and
  `%&`, which Clojure binds to the rest, is the read error
  `UnsupportedRest` until a later item gives it a meaning. Outside a `#(`
  `%`, `%1` and `%&` are ordinary symbols.
- `#{` .. `}` (E8) reads as `(hash-set ..)`, so `#{1 2}` is `(hash-set 1 2)`;
  an unclosed one is the error `Unclosed` for `{`.
- `UnknownDispatch`'s message is `unknown reader syntax #X: only #_, #(, #{ are defined`;
  `#` at the end of the input says so.
- `7/2` reads as `(/ 7 2)`: the three forms are the symbol `/` and the
  `Int`s of the digits, each with the position of its own text (§1.3).
- Hexadecimal and binary literals are values, so `0xFFi8` is out of
  range; `_` may only stand between two digits; `1f32` is invalid (a
  float needs a `.` or an exponent); a float that overflows its width
  is an error and one that underflows rounds.
- `\xNN` is at most `7F` (a string is UTF-8); `\u{..}` takes one to six
  hex digits naming a Unicode scalar value; after `\` in a character
  literal a character that cannot continue a symbol is the character
  itself, otherwise the whole run must be one character or one of
  `newline`, `space`, `tab`, `return`.
- Forms nest at most 1000 deep, counting every open delimiter, every
  pending prefix and every pending `#_`; deeper is a read error.

---

## 2. Programs, expressions, evaluation order

A **program** is a set of modules (§5), one of which defines `main` with
type `(fn () i64)`. A module is a sequence of top-level forms: `ns`,
`defun`, `def`, `defstruct`, `defenum`, `defprotocol`, `impl`,
`defmacro`, `extern`, a macro call expanding to one of these, or a
top-level `(do form*)`, which is **spliced**: it stands for its forms in
order, each of them a top-level form expanded in turn (**Decided**: a
macro returns one `Form`, and `do` had no top-level meaning to lose).
The splice is how a macro whose expansion is several
definitions returns them as one `Form` (§3.16); `do` keeps its
expression meaning everywhere else (§3.5). Every other form is an
**expression**, and an expression at top level is an error.

Evaluation is strict and **left to right, inner before outer**
(**Decided**; liar ADR 008 kept):

- a call `(f a1 ... an)`: `f` (when it is not a global name), then `a1`
  ... `an`, then the call; the copy-ins of the `&x` arguments happen at
  call entry, after `an`, in parameter order (**Decided**, owner,
  2026-09-28; §3.13), so an argument that writes `x` is seen by the
  callee whatever its position, and the write-backs happen after the
  call returns, in parameter order (the `&` arguments name distinct
  variables; when two names reach one cell, the later write-back wins,
  ownership.md §5); the two in-place primitives of §3.13 have
  neither, and update the variable's own cell;
- a **tail call** (a call in tail position of a `defun` or `fn` body
  that types §6.10 admits as one; **Decided**, D5; a call in tail
  position of an `async` body never is one, since the task stores the
  body's value after the call returns, §3.14): its head and arguments
  as above, then the scope exits of every scope enclosing the call in
  the body (their owning bindings released, except those moved into the
  call), the release of the step's other temporaries and of the
  function's owned parameters not moved into the call (a closure's own
  object among them), then the jump. The callee's result is the
  caller's result and nothing of the caller runs after it; in
  particular a call with a write-back is never a tail call, except the
  forwarding case of §3.13, which excludes an `&v` that another
  argument of the call captures;
- `let`: initialisers in order, each seeing the earlier bindings;
- `do`: steps in order; the value of the last is the value of the form;
- `loop`: initialisers in order, like `let`; `recur`: a tail call to the
  loop: its arguments left to right, then the scope exits inside the
  loop body, then the rebinding of every loop variable at once; the
  function's frame stays, so a `recur` moves only a binding it exits (a
  binding of the loop body, a loop variable) and retains any other
  (§3.18; types §6.10);
- `if`/`match`: the test or scrutinee, then exactly one branch; a
  `match` first tries its clauses in order, each clause's pattern
  test, then its bindings (rest vectors built left to right), then its
  guard, a false guard releasing the clause's rest vectors before the
  next clause is tried (§3.6);
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
| the argument list of a call | after the call returns and its write-backs are done; an `Owned` argument at an **owned** position of the callee (types §6.4; every object argument of a call through a function value) is not a temporary: it is moved into the parameter at the call, and the callee releases it at its exit or hands it on |
| a `do` step that is not last | immediately after the step |
| the test of an `if` | after the test is evaluated, before either branch |
| the scrutinee of a `match` | after the whole `match` (the scrutinee is an implicit binding for the form; types §6.3) |
| the guard of a `match` clause | after the guard is evaluated, before the body or the next clause (a guard is a step of its own; its value is a `bool`) |
| a `let` initialiser | never as a temporary: the binding owns the value |
| the argument list of a tail call, `recur` included | never as temporaries: each is consumed into the callee's parameter (the loop's variable) at the jump; every other temporary of the step is released before it (types §6.10) |
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
(defun name private? (param*) where? ret? body)          ; private ::= :private (§5)
param ::= sym | sym: type | sym :borrow | sym: type :borrow | &sym | &sym: type
       | (pat :as sym: type)            ; a pattern parameter (stdlib §7 L7)
where ::= :where (constraint+)          ; constraint ::= (Proto type+) | (Send type)
ret   ::= -> type
```

A pattern parameter `(pat :as p: T)` is sugar the expander builds:
`(defun f (a (pat :as p: T)) body)` is `(defun f (a p: T) (let ((pat p)) body))`,
the `let` of §3.3 (so `[k v]` takes a prefix of a vector and the fields of a
`Pair`). A `defun` states every type, so the type is required (`defun: a
pattern parameter is (pattern :as name: type)`); a `fn` literal takes
`(pat :as p)` as well, and a bare vector `[k v]` item (a fresh name stands
for it), with the type inferred. `loop` takes a pattern in a binding in
the same way (§3.18).

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
one (**Decided**: the flat spelling is what `x: T` already is). `&sym`
is an in-out parameter (§3.13). A function with an `&` parameter is not
a value: it may be called but not passed, stored or returned
(**Decided**; types §1.4). A `defun` may not be redefined in its
namespace.

A `defun` is an **async function** for §3.13 rule 3 iff, after
expansion, (i) an `async` form anywhere in its body (including inside
nested `fn`s) mentions one of its `&` parameters, as `@v`, `&v` or the
target of a `set!`, anywhere inside the `async` body; or
(ii) an `async` form is in tail position of its body (the function's
value is a task it creates: the last step of a `do`, a branch of a tail
`if`/`match`, the body of a tail `let`, transitively, but not inside a
`fn`). A function whose body merely contains an `async` that it drives
to completion itself (`(block-on (async ..))`) and that mentions no `&`
parameter is an ordinary function (**Decided**: the broader definition,
any `defun` containing an `async`, rejected an in-out buffer filled
from a task's result for no safety reason; proposed case 38). Both
conditions are syntactic.

Evaluation: a named function is global, captures nothing (**Decided**, §6),
and when used in non-head position is a function value with an empty
environment (types §8.4). A call to a named function in tail position
of a `defun` or `fn` body is a tail call unless types §6.10 makes it an
ordinary call (§2); in tail position of an `async` body it is always an
ordinary call (§3.14).

Ownership (**Decided**, §4 as amended by D5): a parameter is
**borrowed** — valid for the whole call, never freed by the callee,
passed with no count operation — unless the checker infers it
**owned**, which it does when the body returns it (on any branch),
stores it, captures it in a heap closure, spawns it, or passes it on in
a tail call that must carry its count (types §6.4 has the full rule);
an owned parameter arrives with one count that the callee releases or
hands on. The kind is inferred and exported with the function, never
written on a `defun`. It changes no result and no verdict, but it is a
calling convention: an argument handed to an owned parameter dies at
the callee's exit rather than after the caller's call, so the reference
interpreter follows the kinds too (types §6.12) and the two free the
same objects at the same points. The result is owned by the caller.

### 3.2 `fn`

```
(fn name? (param*) ret? body)
param ::= sym | sym: type
```

The parameter list may be written in brackets, `(fn [x y] body)` and `(fn name [x: i64] -> i64 body)`:
the reader reads a vector, and the expander respells it as the list (`(fn (x y) body)`; the
parameters of a bracket list are the items of the parenthesised one, patterns included), before
anything else looks at the form (**Decided**, owner, stdlib E3). Both spellings are core.

An anonymous function that closes over the variables it uses from
enclosing scopes. With `name`, the body may call `name`; the call goes
through the closure's own code pointer and environment, not through a
capture, so it creates no cycle (**Decided**, §6). `name` is bound only in
the body, and every occurrence of it there is a use of the closure for
the escape rules (types §6.5): a call through it is a direct call, but
storing, returning or otherwise passing `name` on makes the closure
escaping, and so a heap object (proposed cases 59, 60). A `fn` has no
`&` parameters (**Decided**: closures keep one calling convention and
the escaping-capture rule stays a check on captures).

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
borrows its captures and lives in the creating frame, unless it is
passed or called at a tail site — a call in tail position that may be a
tail call, whether or not it turns out to be one (types §6.10) — which
puts it on the heap with retained captures while keeping it non-escaping
(types §6.5, E6; **Decided**, D5). The parameters of a closure, and the
closure object itself for the duration of a call through it, are always
owned: a call through a function value hands every object argument and
the closure over with one count, and the body releases them at its exit
or hands them on (types §6.4, the closure convention). A call to a
closure in tail position of a `defun` or `fn` body is a tail call; in an
`async` body it is an ordinary call (§3.14).

### 3.3 `let`

```
(let (binding+) body)
binding ::= (pat expr) | (sym: type expr)
(let [binding-item*] body)     ; the bracket spelling: flat pairs
```

The bracket spelling takes the bindings as a flat vector, `(let [a 1 b 2] body)`: a pattern and its
expression, again and again; a name that ends in `:` takes the next form as its type and the one
after as its value (`[a: i64 1 b 2]`, three items for the first binding). The expander regroups
the items into the parenthesised pairs, `(let ((a: i64 1) (b 2)) body)`, so the rest of this
section, and the checker, see only the parenthesised form. A binding with no expression
(`[a 1 b]`, `[a: i64]`) is `malformed let: a binding is a pattern and an expression, or
name: a type and an expression`, at the first form of the incomplete binding (**Decided**, owner,
stdlib E3). The parenthesised spelling keeps working.

Sequential bindings: each initialiser sees the earlier ones. `(sym:
type expr)` annotates the variable `sym` (§1.5); the initialiser's type
must fit the annotation as an argument's must fit an annotated
parameter (types §2.4). `pat` is any pattern of §3.6.
An irrefutable one (a symbol, `_`, `(pat :as sym)`, a struct pattern, a
pattern of the only variant of an enum, `[& r]` with `r` a symbol or `_`,
each with irrefutable sub-patterns) is a plain binding. A refutable one
(stdlib §7 L8) is lowered by the checker to
`(match expr (pat <the rest of the let>) (_ (trap "let: pattern does not match")))`,
so it traps when the value does not fit, and that last clause is never
reported as redundant. **A vector pattern in a binding position takes a
prefix**: `[a b]` is `[a b & _]` at every depth, binding the first two
elements and ignoring the rest (a vector of fewer traps), as Clojure's
destructuring does; a `match` keeps `[a b]` exact. Against a `Pair` or
`Triple` (§3.6) a vector pattern names the fields in order and must name
them all. `[& r]` binds `r` to a new vector holding every
element (§3.6: a rest variable owns a fresh copy), which the `let`
releases at its exit like any owning binding. Shadowing an enclosing binding
is allowed; a name may not be bound twice in one `let` (**Decided**:
liar ADR 006 is dropped; nested `let`s and macro-introduced bindings
need shadowing, and the checker keys every rule on binding sites, not
names, so it costs nothing).

Evaluation: evaluate each initialiser in order and bind; evaluate the
body; its last value is the value of the form. Bindings die in reverse
order when the body finishes.

Ownership: a binding **owns** the value of an initialiser that is a fresh
object, a call result or a read of a cell, and **borrows** it when the
initialiser is another variable or a field path (types §6.3). A value
that leaves the scope as its result is moved out or retained (the
scope-exit rule, types §6.3). An object that the initialiser itself
allocates in this frame (a constructor call, `cell`, `atom`; a `fn`
literal by types §6.5) and that never escapes is freed at the scope end
with no count operations (**Decided**, §2: "an object created in a
scope") and lives in the creating frame rather than on the heap, from
the first implementation (types §6.11; **Decided**, D6). A call result
is not such an object — the callee made it, or returned one that others
still hold — so its binding is counted and released at scope exit like
any owning binding; so is a loop variable (§3.18).

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
clause ::= (pat body+)
         | (pat :when guard body+)  ; guarded clause
pat    ::= _                    ; wildcard
         | sym                  ; binds the whole value
         | literal              ; integer, float, string, char, bool, keyword
         | nil                  ; the empty variant of Option: the reader's (Nil) form; (nil) is its (Variant) spelling (§3.9)
         | (some pat)           ; the full variant of Option
         | (Variant pat*)       ; enum variant, positional; (Variant) for a field-less variant
         | (Struct pat*)        ; struct, positional in field order
         | [pat*]               ; a (Vec T) of exactly that many elements, or a Pair/Triple
                                ;   field by field, all of them (stdlib §7 L3b); a prefix in a let (§3.3)
         | [pat* & rest]        ; a (Vec T) of at least that many elements
         | (pat :as sym)        ; bind the whole while matching inside
rest   ::= sym | _
```

Static: the scrutinee's type must be resolved when exhaustiveness is
checked (types §2.6). Clauses are tried top to bottom; the set must be
exhaustive: every variant of an enum or `Option`, both values of `bool`,
every length of a `Vec`, and a final `_` or symbol clause for every
other type. A guarded clause counts for nothing towards exhaustiveness:
the unguarded clauses alone must be exhaustive. A clause that can
never match is an error. A variable may occur once per pattern. A bare symbol other than `nil`
is always a binding, never a variant constant; a field-less variant is
matched as `(Variant)` (**Decided**; §3.9). `match` is the only
eliminator of enums and `Option` in the core; `nil?`,
`some?`, `if-let`, `when-let` are prelude definitions over it.

Evaluation: evaluate the scrutinee once; take the first clause whose
pattern matches and whose guard, if it has one, is true; bind its
variables; evaluate its body. A clause is tried in three stages: its
pattern is **tested** (tags, lengths, literals; this runs no user code
and allocates nothing), then its variables are **bound** (the rest
vectors of its vector patterns are built at this point, and only once
the whole pattern has matched), then its **guard** is evaluated with
those variables in scope. A false guard ends the clause: its rest
vectors are released and the next clause is tried against the same
scrutinee value.

**Vector patterns** (**Decided** (owner, 2026-09-28)). `[p₁ .. pₖ]`
matches a `(Vec T)` of length exactly `k` whose element `i` matches
`pᵢ`; `[p₁ .. pₖ & r]` matches one of length at least `k` whose first
`k` elements match `p₁ .. pₖ`, and binds `r`, unless it is `_`, to a
new `(Vec T)` of the remaining elements in order. `&` stands alone,
followed by whitespace (the text `&r` reads as `(& r)`, §1.2, which is
not a pattern), at most once per vector pattern, and is followed by
exactly one symbol or `_`. The sub-patterns are any patterns, vector
patterns included, so vector, struct, variant, `Option` and literal
patterns nest in each other in any order. `Vec` means the prelude's
`Vec` (§4.5), whatever the current namespace binds; a vector pattern
against any other type, `Form` included, is a type error. Vector
patterns and patterns of `Vec`'s own variants may not be mixed on one
scrutinee (types §2.6). How the length and elements are read, and what
each costs, is in types §8.3: the length in constant time, element `i`
by one walk of the trie, the rest by building a new vector of `n - k`
elements.

**Guards** (**Decided** (owner, 2026-09-28)). The keyword `:when` as
the second item of a clause makes the third item the clause's guard,
an expression of type `bool` evaluated with the clause's pattern
variables in scope. A guard may have side effects (call functions,
write cells, `await` inside an `async` body): it is evaluated at most
once each time the clause is tried, after the pattern has matched and
before anything else, and never when an earlier clause was taken, so
its effects happen in a fixed order (**Decided**: there is no effect
system in which a pure guard could be checked; types §6.3 gives the
same reason for never eliding a cell read). A guard is never in tail
position; the body of the clause is, when the `match` is. A guarded
clause after an unguarded clause that already covers its pattern is
never taken and is the redundant-clause error; a guarded clause makes
no later clause redundant.

Ownership: pattern variables are borrows of parts of the scrutinee
(*derived* bindings, types §6.2); the scrutinee is kept alive for the
whole form. A pattern variable bound to a scalar is a copy. An element
of a vector pattern is a part of the vector exactly as a field is a
part of a struct, and is bound the same way. A rest variable is the
exception: it **owns** its new vector (an owning binding of its
clause, types §6.3), which holds a count on each of its elements, so
it may be returned, stored or captured like any owned value; it is
released at the end of its clause's body unless it leaves as the
clause's value, and when the clause's guard is false.

**Matching forms.** A `Form` is not a `Vec`, but a `List`, `Vec` or
`Map` form holds its items as a `(Vec Form)` (§3.16), so a vector
pattern inside a variant pattern matches a form by shape with no
syntax of its own: `(List [(Sym "if") c t e])` matches an `if` form and
binds its three operands, and `(List [(Sym "do") & steps])` binds the
steps of a `do` as a new `(Vec Form)`. This is how macros and the
self-hosted compiler take forms apart. A quoted form in a pattern
(`'(if c t e)`) is not a pattern.

### 3.7 `defstruct`

```
(defstruct Name private? (field+))
(defstruct (Name param+) private? (field+))
param ::= tvar | tvar :colour           ; a colour parameter (types §1.3)
field ::= sym: type | sym
```

A nominal product type, a constructor function `Name` taking the fields
positionally, and field access via `.`. A field without a type
annotation becomes a fresh type parameter appended to the struct's
parameter list in field order, so `(defstruct Pair (a b))` means
`(defstruct (Pair a b) (a: a b: b))` (**Decided**). A struct that mentions
itself, directly or through other types, must annotate the recursive
fields (types §1.3). A parameter followed by `:colour` is a **colour
parameter** (**Decided**, owner, 2026-09-28; types §1.3): it is the
colour of function types in the fields (`(defstruct (Handler k :colour)
(f: (fn k (i64) i64)))`), each instance's argument is `:send`, `:local`
or inferred at construction (`(Handler (fn (x) x))` is a `(Handler
:send)`, `(Handler (fn (x) (+ x @c)))` with `c` a cell a `(Handler
:local)`), and the struct is sendable exactly when its argument is
`:send`. Structs are immutable objects (**Decided**, §1); a
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
argument of `set!`. There is no field place: `&` takes a variable, never
a `(. x field)` form (§3.13; **Decided**, D1).

### 3.9 `defenum`

```
(defenum Name private? variant+)
(defenum (Name param+) private? variant+)       ; param as for defstruct
variant ::= (Variant) | (Variant field+) | Variant
field   ::= sym: type | type
```

A nominal sum type and one constructor per variant; a variant without
fields is a constant, written bare in expressions (`Empty`, `nil`) and
as `(Variant)` in patterns (§3.6); `(Empty)` as an expression is the
error `Empty is a constant, not a function; write Empty` (types §2.2;
**Decided**: `nil` already works this way, and `(Empty)` as a call to a
value of non-function type broke the normative `list` expansion of cases
01 and 05). The prelude's `List` names its variants `Empty` and `Cons`
(§4.5), capitalised so that they do not collide with the library's method
`empty` and function `cons` (stdlib design C-3). Variant names live in the namespace beside functions and must be
unique there (**Decided**). An enum
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
`Empty`. A macro that writes `nil` inside a quasiquote emits the
reader's `(Nil)` and needs no special case (proposed case 51). Its
representation is in types §8.1; the prelude derives `Eq`, `Ord` and
`Hash` for it and writes its `Show` by hand (§4.4, §4.5) (**Decided**: the declaration `(defenum
(Option a) (nil) (some v: a))` cannot be read, since `nil` reads as
`(Nil)`, and accepting the reader's form in the expander is what lets
a macro emit `nil` through a quasiquote without a special case). There
is no implicit lifting of `T` to `(Option T)` and no null of any other
type (**Decided**, D3: every coercion the inference draft proposed
made acceptance order-dependent, and `if-let` covers the idiom).

### 3.10 `defprotocol`, `impl`

```
(defprotocol Name private? requires? method+)
(defprotocol (Name self det*) private? requires? method+)   ; self dispatches; det* are determined by self
requires    ::= :requires (super+)              ; super ::= Proto | (Proto type+), the first type self
method      ::= (mname (self qual* mparam*) -> type body?)   ; a body is the default
mparam      ::= sym: type qual*
qual        ::= :borrow | :owned

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
implements the protocol, and gives every method that has no default
(a method with a default may be given or omitted). A generic `impl` declares the bounds
its bodies need on its type variables in `:where` (`(impl Eq (Vec a)
:where ((Eq a)) ...)`); an `impl` without `:where` has an empty context,
and a body that needs a bound not listed is an error (types §2.7; Open
decision 19). Contexts are declared, not inferred, because a `defun`
that calls a method is type-checked and generalised before any `impl`
body is (types §3.5). One `impl` per (protocol, head constructor) in the
whole program.

A colour parameter of the head's type (§3.7, §3.9) is given a colour
variable or a colour (**Decided**, owner, 2026-09-28; types §1.3).
`(impl P (Hook k) ..)` implements `P` for every colour, and `k` is
**rigid** in the bodies, like Rust's `impl<K> P for Hook<K>`: `self`'s
fields are read and written at colour `k`, a sendable closure (a named
function, a closure over sendable values) may be stored where colour
`k` is expected and a closure over a cell may not, `self` may not be
passed where a `(Hook :local)` or a `(Hook :send)` is expected, and a
`k` closure may not be spawned. `(impl P (Hook :local) ..)` implements
`P` for local hooks only, and its bodies see `self` as a `(Hook
:local)`; `(impl P (Hook :send) ..)` likewise for sendable ones.
Either excludes any other `impl` of `P` for `Hook`, and using a
`:local` instance on a `(Hook :send)` is `no implementation of P for
(Hook :send)`.

**Supertraits and defaults** (**Decided**, owner, 2026-09-28; types
§4.1). `(defprotocol Ord :requires (Eq) ..)` makes `Eq` a supertrait
of `Ord`: an `impl Ord` for a type needs an `impl Eq` for the same
type, whose context the `Ord` impl's context entails; a bound `(Ord t)`
entails `(Eq t)`, so a generic `impl` or `defun` bounded by `Ord` may
use `=`; and a `(dyn Ord)` value may call `Eq`'s methods (subject to
object safety) and be upcast with `(dyn Eq d)`. A method whose
signature is followed by body forms has a default: an `impl` that
omits the method gets that body, specialised to its type and checked
there as if the `impl` had written it, with its names resolved where
the protocol is defined. The built-in `Eq` defaults `!=` to `(not (=
self y))`, and the built-in `Ord` requires `Eq` and defaults `<=`, `>`
and `>=` from `<` (types §2.12), so `(impl Ord T (< (self y) ..))` is a
complete instance once `Eq` has one.

Escape and count kinds (**Decided**; types §2.7, §6.4): every method
parameter, `self` included, defaults to the **escaping** kind and to the
**borrowed** count convention. (`self` must default to escaping: the
prelude's `conj` on a `List` stores `self` in the new cell.) `x: type :borrow` promises that no implementation makes `x`
escape; the checker rejects an `impl` whose body breaks the promise.
The same keyword after a `defun` parameter is the same promise on a
plain function (§3.1). `:owned` declares that the caller hands the
parameter over with one count and every implementation releases it or
hands it on (types §6.4, D5): the convention an implementation needs to
pass the parameter on in a tail call without a frame. A `defun`'s kinds
are inferred (§3.1); a method's are declared, because callers are
compiled against the protocol, not the implementation, and an
implementation is compiled with the declared kind whatever its body
would have inferred. Only a `:borrow` parameter may receive a
non-escaping closure or a closure that captures an `&` parameter
(cases 08, 18). A method signature that mentions a closure type without
a colour (`(fn (a) unit)`) accepts closures of any colour; `(fn :send
(a) unit)` requires a sendable one (types §1.4).

Dispatch: static when the receiver's type is concrete after inference,
which is always except through the explicit `(dyn P)` type (types §4). `(dyn P e)` needs `e` of an object type: a scalar, a field-less enum
or `unit` is `dyn requires an object type` (**Decided**, owner,
2026-09-27; types §2.15), since a `(dyn P)` is the object's pointer and
a vtable. `(dyn P :send e)` makes the distinct type `(dyn P :send)`,
which may cross a thread: `e`'s type must be `Send` as well
(**Decided**, owner, 2026-09-28; types §2.15, §5.1). `(dyn P d)` of a
`(dyn P :send)` value `d` is the explicit conversion to `(dyn P)`; there
is none the other way. The type is written `(dyn P :send)` or `(dyn (P
D..) :send)`.

### 3.11 Cells, atoms, weak references: builtins, not forms

These are **builtin functions** with ordinary call syntax. They are listed
here because the reader gives `@` to `deref` and because the checker has
a rule for each (types §2.9–§2.11). None is a special form.

| Call | Type | Meaning |
|---|---|---|
| `(cell v)` | `a -> (Cell a)` | a new cell holding `v` |
| `(deref c)`, `@c` | protocol `Deref` | cell → its value; atom → its value; weak → `(Option T)`; task → its result, as `(join c)` (**Proposed**, owner's rule 2026-10-01: Clojure's `@f` of a future; types §2.9). Every object result is owned (+1). `c` is an expression, or the name of an `&` parameter (§3.13) |
| `(set! c v)` | `(Cell a) a -> unit` | store `v`, release the old value; the target `c` is an expression of cell type (a field path among them, §3.8) or the name of an `&` parameter, which is not an expression but may stand here and as the operand of `@` (§3.13; types §2.9) |
| `(atom v)` | `a -> (Atom a)`, `Send a` | a new atom |
| `(swap! a f)` | `(Atom a) (fn (a) a) -> a` | replace atomically with `(f old)`; `f` may run more than once, and `swap!` may never finish under contention or when `f` itself changes the atom each time (**Decided**, ownership.md §7); returns the new value (owned); `(swap! a f x ..)` with extra arguments is the prelude macro of §4.4 |
| `(reset! a v)` | `(Atom a) a -> unit` | replace, release the old value |
| `(weak x)` | `a -> (Weak a)`, `a` an object type other than an `Option` (types §2.11) | a weak reference; does not keep `x` alive |

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
| `(plet ((sym expr)+) body)`; a binding may be `(sym: type expr)` (§1.5), annotating `s1` | macro | `(let ((t1 (spawn (fn () e1))) ...) (let ((s1 (join t1)) ...) body))` |
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
running (**Decided**: a discarded task then finishes and the audit sees
a quiescent heap), so a fire-and-forget `spawn` is an ordinary program.

A `Task` is `Send` when its result is (types §1.6), so one task may be
held, joined or awaited by several threads or tasks at once: `(plet ((a
(join t)) (b (join t))) (+ a b))` is accepted, and so are two `async`
bodies awaiting one task. The runtime, not the checker, keeps that
safe: a task is resumed by one driver at a time, every other holder
waits for its completion, and a task keeps a list of waiters rather
than one waker slot (types §8.8; **Decided**). The executor is
multi-threaded: a task may be resumed on any worker thread, which is
why what a task captures must be sendable (§3.14; **Decided**, D7). A
count on a task is what keeps it allocated, never what serialises it.

### 3.13 `&`: in-out parameters and arguments

```
parameter: &sym | &sym: type
argument:  &sym                     ; sym a variable of type (Cell T): a let binding or an & parameter
```

Meaning (**Decided**, §5: copy-in, copy-out). Inside the callee an `&`
parameter `v` **is a cell** of type `(Cell T)` (**Decided**, D2): read it
with `@v`, pass it on with `&v`, replace its content with `(set! v e)`,
update it in place with the primitives below. The private cell is
created by the caller at the call and lives exactly for the call:

```
(f &x a)                    ; x : (Cell T), f's first parameter is &v: T
   arguments:  a            ; every argument of the call, left to right (§2)
   copy-in:    t := a new private cell holding @x            (an acquire: +1)
   call:       (f t a)      ; the callee's v is t
   write-back: x's content := t's content (moved), x's old content released; t is freed
   result:     the call's value
```

**The copy-in happens at call entry** (**Decided**, owner, 2026-09-28):
after every argument of the call has been evaluated, the private cells
are made and filled in parameter order, each from its variable as it
is then. An argument written after `&x` that writes `x` (a `set!`, a
call that takes `&x`, a closure that captures `x`) is seen by the
callee (cases 151 to 153). This makes forwarding (below) agree with
copy-in/copy-out: a forwarded callee uses the private cell as the
arguments left it, and so does a copied-in one (cases 150, 151).

The call does not write `x` before the write-back, so a plain `@x` in
the same call stays valid (case 17): it is an acquire of its own at its
argument position, which holds the value `x` had then; nothing between
it and the copy-in writes `x` in case 17, so the two are the same
object. A cell that also reaches the call
under another name (an alias, a capture, a field) can be written through
that name during the call; the write-backs still run in parameter order
and the later one wins (ownership.md §5). A read `@v` inside the callee is an
owned reference (+1), so a value read from `v` and still in use always
holds a count, even after the callee replaces `v`'s content (types
§6.3, §6.7): proposed case 78 pins that inside an `&` function, and
proposed case 66 on a `let` cell. Case 08 does not: since D1 the
caller's cell keeps the vector alive for the whole call, so its loop
sees the original elements with or without the acquire (Appendix A).

**An `&` parameter is not a value** (**Decided**, D2). Inside the callee,
`v` may occur only as `@v` (that is, `(deref v)`), `&v`, or the first
argument of `(set! v e)`; this holds in the body and in every closure
literal inside it. Any other occurrence (`v` returned, stored, bound by
`let`, passed as a plain argument, `(cell v)`, `(weak v)`, ...) is the
compile error `& parameter v used as a value in f`. The private cell is
a stack object of the caller's frame that lives exactly for the call
(types §8.6); a value that referred to it would outlive it at the
write-back.

**Copy-in always acquires** (**Decided**, D1). The private cell is
initialised with `@x` (+1), and the caller's `x` keeps its own count
until the write-back. There is no move, no "exclusive" place and no
field place: `&` names a variable, never a field, and nothing is ever
taken out of a struct. An `&` argument hands a *cell* over, not the
object in it, which the two cells share for the duration of the call.
Consequence: the first in-place update inside a user-written `&`
function sees a count of at least two and copies (types §6.6); the copy
is then held by the private cell alone, so every later update in the
same call is in place, and after the write-back the caller's variable
holds it with count one. The library collections update in place
through the two unique-write primitives below, on objects they hold
alone.

**Write-back** stores the private cell's content into the variable,
releasing the variable's old content, then frees the private cell.

**Forwarding at a tail call** (**Decided**, D5 as amended). At a call in
tail position (types §6.10), an argument `&v` where `v` is an `&`
parameter of the enclosing `defun`, at any `&` position of any callee,
forwards the private cell: no copy-in, no write-back, the cell is kept;
the one write-back is that of the frame below that made the cell, after
the whole chain returns. The exception (**Decided**, owner,
2026-09-28): `&v` is not forwarded when another argument of the same
call mentions `v` — writes or reads it, or is a closure that captures
it, at any depth — or names a `let`-bound closure that does (types §6.6
gives the exact, syntactic test); it is then copied in and written
back as at any call. Forwarding is therefore indistinguishable from
copy-in/copy-out: since the copy-in happens at call entry, a forwarded
call and the same call with a copy-in and a write-back see the same
writes by the arguments, and the only way the callee could write the
cell through another name during the call, a closure over `v` passed to
it, makes the call copy in. In `(defun f (&v: i64) -> i64 (g &v (fn ()
(set! v 7))))`, with `g` writing 1 into its `&w`, calling the closure
and returning `@w`, `g` returns 1 and its write-back stores 1 into `v`,
in tail position or not (types §6.6; cases 162, 163). A call whose
every `&` argument forwards stays a tail call, so an in-out accumulator
can thread through mutual recursion in constant stack. Any other `&`
argument — a `let` cell, a cell reached another way, a captured `&v` —
makes the call an ordinary call with copy-in and write-back, never a
tail call, because the write-back must run after it returns (types
§6.10 rule (b)).

**Unique update** (**Decided**, §5): the in-place primitives update the
object in a place when it is **unique**: its count is one and it is
neither shared, nor immortal (a literal, a `def` value, a named function
used as a value, and everything reachable from one: §3.16, §3.19), nor
stack-allocated; they copy first otherwise (types §6.6, §8.2). An
immortal object has no count to test, so an update through a place that
holds part of a literal always copies and the literal is never written
(proposed case 37):

| Primitive | Signature or rule | Effect |
|---|---|---|
| `(array-set! &a i x)` | `&(Array T) i64 T -> unit`: a signature, not a type (types §1.4) | element `i` := `x` |
| `(set-field! &s field x)` | a primitive form (§4.3; types §2.13): `s : (Cell S)` for a struct `S` with a field `field: F`, `field` that field's name, `x : F`; result `unit` | the field `field` := `x` |

Neither is a value: each occurs only as the head of a call (types
§2.13). Neither takes a copy-in or a write-back: `&a` and `&s` hand the
primitive the variable's own cell, whose content it tests and updates
(types §6.6). `field` is a name, never an expression: it is not
evaluated and not resolved as a variable, so `(set-field! &c x (+ x
2))` sets the field `x` from the local `x` (proposed case 80).

Everything else that updates in place (`push!`, `pop!`, `map-put!`,
`append`, ...) is library code over these two, `&` and `set!`. Because
the copy-in leaves the count above one, the first update through an `&`
parameter copies; the copy is unique in the private cell, so every
further update in the same call is in place. A function that holds an
object in a `let` cell it created and updates it through the primitives
directly updates in place from the start.

Static requirements, all checked on the surface form before desugaring:

1. **Distinct variables** (**Decided**, §5): the `&` arguments of one
   call must name distinct variables. Error: `variable x passed to more
   than one & parameter in call to f` (case 12). Passing `&x` and `@x`
   in one call is fine (case 17).
2. **No escaping capture** (**Decided**, §5): a closure that captures an
   `&` parameter must not escape the call. Error: `& parameter captured
   by escaping closure: v in f` (case 18). "Escapes" is decided by types
   §6.5; such a closure may only be called directly or passed to a
   `:borrow` parameter, in a tail call or not.
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
4. The variable must have a cell type whose content type unifies with
   the parameter's, else `& argument must be a cell variable`. A plain
   argument at an `&` position, or `&x` at a plain position, is a type
   error.
5. **Not a value** (**Decided**, D2, above): an `&` parameter occurs only
   as `@v`, `&v` or the target of `set!`. Error: `& parameter v used as
   a value in f`.

### 3.14 `async`, `await`

```
(async body)             ; type (Task T) where body : T
(await expr)             ; only inside async; expr : (Task T); result T
```

`async` creates a task: an object holding the body's captured variables
and its suspended state. Evaluating the form does not run the body; the
executor does, on whichever of its worker threads picks the task up
(it is multi-threaded; types §8.8), when the task is joined, awaited or
spawned. `await`
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
there; nothing is ever borrowed across an `await` (types §6.9). The
body's value is the task's result, stored and published when the body
finishes, so a call in tail position of an `async` body is an ordinary
call, never a tail call (types §6.10 rule (f), §8.8). Every
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
(extern name private? (type*) -> type opt*)      ; opt: :varargs
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
runtime's exported `fib_release` called from C, releases it (**Decided**:
extern positions are scalars, so liar's `:retains` named parameters that
cannot exist; types §6.13). No extern parameter is ever typed as an
object, so nothing else transfers a count across the boundary. `ptr` is
a scalar for the ownership rules and is not sendable (types §5). A call
to an extern is never a tail call (types §6.10). At a `:varargs`
extern, an argument past the fixed parameters of type `bool`, `i8`,
`i16` or `f32` is widened to `i32` or `f64` before the call, as C
promotes it; lIR rejects the unpromoted argument (lir.md §7.1,
**Decided**, owner, 2026-09-28).

**`alloc` is zeroed** (**Proposed**, implemented at the owner's request, 2026-10-01). `(alloc n)` is `n`
bytes of raw memory, every byte of them zero, in the reference interpreter
and in compiled code alike: the compiled `alloc` is `calloc`. Before, it
was `malloc`, whose block is zero only when the system has just given it,
so a program that read memory it had not written saw zeroes interpreted
and, compiled, whatever a freed block had held. A program may therefore
read a block it has not stored to. It must still stay inside the `n`
bytes and not touch a block after `(free p)`: the interpreter traps on
both, compiled code does not look. Case 190 stores a non-zero byte in
every position of blocks of 1 to 65536 bytes, frees them, allocates the
same sizes again and counts the bytes that read zero. A block that cannot
be had (a size that is negative or more than the system will give) is the
trap `out of memory`, in both, as a failed allocation of the runtime is;
the compiled `alloc` checks what `calloc` returns (case 195).

The reference interpreter has no C library, so it provides three externs
and stops at any other with `unsupported: extern NAME is not available in
the reference interpreter`: the prelude's `write` (descriptors 1 and 2),
and `strtod` and `strtof`, which must be declared `(ptr ptr) -> f64` and
`(ptr ptr) -> f32`. They convert the start of the NUL-terminated string
in raw memory as C does in the C locale: white space, a sign, then `inf`,
`infinity`, `nan` or `nan(chars)` in any case, or decimal digits with an
optional `.` and exponent; the value is correctly rounded at the result's
own width and overflows to infinity; with no number the result is 0 and
the end is the start; the end address is stored through the second
argument unless that is null (`(load-ptr c)` after `(store-i64 c 0)`).
Decimal only: a hexadecimal float (`0x1p3`) stops the run with
`unsupported: hex float`, so that the interpreter never converts it as
the `0` before the `x` and disagrees with the compiled program silently
(**Proposed**; case 189 pins the rest).

**`write`** (**Proposed**, implemented at the owner's request, 2026-10-01), declared `(i32 ptr i64) ->
i64`, is one write(2) of the `n` bytes at `p` to descriptor 1 or 2: it
returns the number of bytes written, which may be fewer than `n`, or -1
when the call failed (a full device, for one), as the C function does;
the interpreter writes to a duplicate of the descriptor, unbuffered, and
gives the same answers, except for a descriptor that was closed when the
program started: a compiled program gets -1 (EBADF), while the Rust
runtime that the interpreter is opens `/dev/null` in the place of a
closed standard descriptor, and so succeeds. The prelude's `println` and
`eprintln` rest on it (§4.5); the tests make it fail on `/dev/full`, cut
it short and then fail under a file-size limit, and take seven bytes at a
time through a preloaded library (`crates/fibref/tests/run_io.rs`,
`crates/fibc/tests/cli.rs`).

### 3.16 `quote`, `quasiquote`, `defmacro`, `Form`

```
(quote form)          ; 'form : Form
(quasiquote form)     ; `form, with (unquote e) ~e and (unquote-splicing e) ~@e inside
(defmacro name private? (param*) body)
param ::= sym | ... sym            ; "... rest" binds the remaining forms as (Vec Form)
```

A macro is a fibber function from forms to a form, run at expansion
time. Its parameters have type `Form` (`(Vec Form)` for the rest
parameter) and its body has type `Form`. The body is ordinary fibber,
type-checked as a `defun` in a macro-time module and evaluated by the
reference interpreter's evaluator over that module (**Decided**: macro-time
evaluation is then the same executable spec as run time). A compiler,
including the self-hosted one, compiles the macro-time module through
lIR and runs it with lIR's JIT, as Clojure compiles and loads every form
(**Decided**, owner, 2026-09-27); its results must agree with the
reference interpreter's. There is no separate macro interpreter.

**Phase separation** (**Decided**, same decision). Only the macro-time
module runs at expansion time: the `defmacro` bodies, and the functions
and `def`s they reach, which must come from modules the current module
requires and which are already compiled. No top-level form of the module
being compiled is evaluated at expansion time, so compiling a module has
no side effects of its own (unlike Clojure's AOT, which loads and runs
the namespace it compiles). A macro that calls a function of the module
it is defined in is an expansion error: `macro m calls f, which is not
available at expansion time; move f to a required module`.

**After the flip of stdlib tranche 1** the macro-time module is still the prelude and
the macro, with the modules the module requires: the implicit library (`fib.core`, `fib.seq`,
`fib.coll`) is not in it, so a macro body that calls `count`, `conj`, `first` or `map` is
rejected at expansion time under both tools (`fibref` says `macro m calls count, which is not available at expansion time`, `fibc` `macro m failed: .. unbound name count`), and writes the prelude's raw operations
(`vec-count`, `vec-conj`, `vec-nth`) instead (cases `cases/ownership` 83, 91, 92). Whether the
implicit modules should join the macro-time module is an open decision of the owner (reported
by the flip; it makes every macro run compile the library).

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
`(a ~b ~@cs d)       ⟹ (List (concat ['a] [b] cs ['d]))      ; b : Form, cs : (Vec Form)
`[ ... ]  `{ ... }   ⟹ the same with Vec / Map
```

The rewrite's heads are the prelude's, written `fib.prelude/List`,
`fib.prelude/Vec`, `fib.prelude/Map` and `fib.prelude/concat` as the
literals of §1.4 write theirs, so that a program's or a library's own
`concat`, or an enum with a variant named `List`, is not what a template
is built with (**Proposed**, stdlib design §7 B2; case 012 of
`cases/modules`). `` `() `` is `(List (concat))`, the empty list, which
both tools expand.

`(gensym "prefix")` returns a fresh `Sym` that cannot collide with any
source symbol. There is no automatic hygiene (**Decided**: renaming
hygiene is a much larger expander, and `gensym` covers the cases).
Expansion is outermost-first, repeated until no macro call remains,
before name resolution and typing. A macro must be defined earlier in
the module, or in a required module, than its first use. A macro call
at top level may expand to several top-level forms by returning them as
`(do f₁ .. fₙ)`: a top-level `do` is spliced into the module as its
forms (§2; **Decided**). Top-level forms are expanded
and registered in order, a spliced sequence included: a `defstruct` is
registered with the expander before the next form is expanded, so a
spliced `(derive ..)` that follows a spliced `(defstruct ..)` sees the
struct. `(do)` splices nothing. So

```lisp
(defmacro defrecord (name fields)
  `(do (defstruct ~name ~fields) (derive Eq ~name)))
```

defines the struct and its `Eq` instance (proposed case 41).

**Implementation limits** (not part of the language; the reference
implementation's values, which a compiler may raise but must enforce
in some form, so that expansion terminates on every input): the
expansion of one top-level form, a spliced `do` counting as one form,
fails with a compile error at the macro call when it performs more
than 100 000 macro expansions (user macros, prelude macros and
quasiquote rewrites each count one: `more than 100000 macro expansions
in one top-level form`), when it nests forms deeper than 2 000 levels
(`expansion nested deeper than 2000 levels`), or when the results of
its expansions, each counted in full as the number of forms in its
tree, add up to more than 4 000 000 forms (`macro expansions of one
top-level form produced more than 4000000 forms`). The last limit is
what stops a macro whose expansion grows at each step, such as
`` (defmacro g (x) `(g (do ~x ~x))) ``, which would otherwise do
exponential (or, growing by one form a step, quadratic) work long
before it reached the first limit.

Reflection at expansion time (**Decided**; liar ADR 023 kept and
extended: without `struct-params` and `struct-field-types` no `derive`
could be written for a generic struct, which is every struct with an
unannotated field), for a struct `Name` defined earlier in the module
or imported:

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
struct or an enum, the context lists `(Ord t)` alone: the body compares
with `=` as well as `<`, and `(Ord t)` entails `(Eq t)` through `Ord`'s
supertrait (types §4.1; **Decided**, owner, 2026-09-28, replacing the
`(Eq t)` listed beside it while there were no supertraits). An `Ord`
instance needs an `Eq` instance of the same type, so `(derive Ord
Name)` goes with `(derive Eq Name)` or an `impl Eq`.

The same reflection exists for enums (**Decided**), for
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
  as the struct derive combines its fields' hashes: each step is the
  prelude's `(hash-combine h x)`, called as `fib.prelude/hash-combine`
  so that a binding of the name in the program cannot capture it, from
  the seed `h` of the variant's index (0 for a struct); see types
  §2.12.
- `Show`: the variant name followed by the shown fields, as a call form.
- `Debug` and `ToStr` (stdlib design §2.7, L17; **Proposed**, the shapes
  were decided in the tranche 1 plan, R8): the text of a record,
  `#m.Done{:v 3}` for a variant with fields and `#m.P{:x 1, :y "x"}` for a
  struct, each field's text the `debug` of the field (`fib.core/debug`).
  `m` is the module the `derive` form is expanded in, because the
  expander's type table does not record the module a type was defined in,
  so a `derive` in another module than the type's prints the deriving
  module. A variant field written without a name is called by its
  position, `#m.Wrap{:0 3, :1 "q"}`, and a variant with no field prints as
  its bare name. `ToStr` is the same text built from the fields' `Debug`,
  so its `:where` asks for `Debug` and the type needs no `Debug` instance
  of its own (a type that derives only `ToStr` has `to-str` and no
  `debug`; a recursive type that derives only `ToStr` fails with `no
  implementation of Debug for ..` at the recursive field). The protocols
  are named `fib.core/Debug` and `fib.core/ToStr`, so the module needs
  `fib.core` in scope: a `:use fib.core` until the implicit list is
  filled (§5), else the error is `unknown protocol fib.core/Debug`.

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
produce; `Debug` and `ToStr` are not built in, so an enum with no field
in any variant still gets those two. The prelude derives `Eq`, `Ord` and
`Hash` for `Option` and for `List` (§4.4; their `Show` is written by hand,
§4.5), which is why `(= (some 1) (some 1))` and `(= (list 1 2) (list 1
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
(loop ((sym expr)*) body)     ; a variable may be annotated: (sym: type expr), §1.5
(loop [sym expr ..] body)     ; the bracket spelling of the bindings, as for `let` (§3.3)
(recur expr*)                 ; only in tail position of the innermost enclosing loop body
```

A binding `(pat expr)` whose `pat` is not a symbol (stdlib §7 L7) binds one
hidden loop variable to `expr`, which `recur` rebinds whole, and matches `pat`
against it at the start of the body as a `let` binding does (§3.3): `(loop
(([a b] [1 2])) .. (recur [b (+ a b)]))`. The pattern's names are not in scope
in the initialisers of the bindings after it.

`loop` binds its variables like a sequential `let` and evaluates `body`;
`recur` evaluates its arguments left to right, rebinds every loop
variable at once (the arity must match) and continues at the start of
`body`. The value of the form is the value of `body` when it finishes
without `recur`. `recur` must be in tail position of the loop body (the
last step of the body, the branches of a tail `if`/`match`, the body of
a tail `let`/`do`, transitively); anywhere else, outside any `loop`, or
inside a `fn` or `async` literal nested in the loop, it is the error
`recur outside loop` / `recur not in tail position` (**Decided**).

Semantics (**Decided**, D5): a `loop` has no evaluation rule of its
own. It is the local function `(fn g (sym*) body)` called once with the
initialisers, and `recur` is a tail call to `g` (types §6.10): the loop
variables are owning bindings, a `recur` consumes its arguments into
them and releases the old values before continuing, a closure among its
arguments is on the heap (E6), and a value that leaves the loop as its
result follows the scope-exit rule. Unlike a tail call out of the
function, a `recur` leaves the function's frame in place, so it moves
only what it exits — a binding or temporary of the loop body, or a
loop variable, whose count passes to its new variable instead of being
released as an old value — and retains an argument that names any
other binding (one outside the loop, a parameter), which stays in scope
and is released at its own scope exit, as a capture of `g` would be.
Loop variables are never stack objects, since every `recur` rebinds
them (types §6.10, §6.11). `while`, `dotimes` and `for-each` over a
`range` are prelude macros over it (§4.4).

Why it stays a core form. D5 asked for `loop`/`recur` to become a macro
over a named local `fn` called in tail position unless a concrete
reason forbids it; two rules cannot survive the rewrite `(let ((g (fn g
(sym*) body'))) (g init*))`, because the loop body would then be a
closure body rather than part of the enclosing function:

1. **`&` forwarding.** A self tail call of the enclosing `defun` from
   inside a loop body, forwarding the function's own `&` parameter —
   `(defun run (&s n) (loop ((i 0)) (if (< i k) (do (step &s) (recur (+
   i 1))) (run &s (- n 1)))))` — is a tail call under §3.13 only because
   `&s` forwards an `&` parameter of the enclosing `defun`. In
   the rewritten body the same call sits inside `g`, where `s` is a
   capture, not a parameter of `g` (a `fn` has no `&` parameters, §3.2);
   the forwarding exception of types §6.10 rule (b) cannot apply, and
   the call becomes an ordinary call with copy-in and write-back:
   correct, but one frame per call. A macro cannot preserve the
   forwarding rule for such calls.
2. **`await`.** An `await` in a loop body inside an `async` is a
   suspension point of that task's state machine (§3.14; proposed case
   32); inside a `fn` it is the error `await outside async`, and no
   checker rule can make it otherwise, since a closure body is compiled
   as its own function (types §8.4), not as part of the task.

Both hold because the loop body belongs to the enclosing function: its
free variables are that function's own bindings, it has no capture set,
and it is inside whatever `defun` or `async` encloses it. A named local
`fn` called in tail position gives the same constant-stack iteration
(types §6.10) and is the right spelling whenever neither rule is needed.

### 3.19 `def`

```
(def name private? expr)
(def name: type private? expr)
```

A top-level binding: `name` is bound in the module's namespace, beside
the functions, to the value of `expr`, which is any expression (**Decided**,
L15), made once before `main`. A **constant expression** is made at
compile time as static data (**Decided**: liar spelled every constant as
a nullary function, which for a table allocates on every call, while the
immortal mechanism the runtime has for literals covers it exactly):

```
const ::= literal | 'form | nil | Variant             ; a field-less variant, bare (§3.9)
        | (Ctor const*) | (some const)                ; a struct or variant constructor
        | [const*] | {const const ...}                ; literal collections (§1.4)
        | name                                        ; an earlier def, or a named function (of this module or a required one) as a value
```

No call other than a constructor and the prelude calls that the
literal-collection rewrite introduces (`vec-empty`, `vec-conj`,
`map-empty`, `map-assoc`, §1.4; a hand-written `(conj ..)` or `(assoc ..)`
is a protocol call, so it is not one), and no `cell`, `atom`, `weak`, `fn`, `async`, `unsafe`
or `@`: a constant expression has no effect and builds only immutable
objects. Its type is inferred as for a `let` binding (types §2.16):
monomorphic, never generalised, and closed; `(def e [])` is the error
`def e has an unresolved type; annotate it`, and a form outside the
grammar is not an error (L15): it is made at run time, below. A `def`
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
(**Decided**). A `def` naming a generic function must
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
The compiler evaluates the initialiser at compile time and emits the
graph as static data (types §8.2, §8.10; **Decided**, owner,
2026-09-28); a constant expression has no effect, so this is
indistinguishable from evaluating it before `main`, as the interpreter
does. `name` in an expression is a read of the global
with no count operation (types §6.1: a global binding). Because its
objects are immutable and immortal it may be used from any thread and
inside any `fn` or `async` body without a `Send` check or a
share-marking walk (types §2.16, §5.5): like a named function it is a
global, not a capture. That is what makes a lookup table or a keyword
map usable from a `plet` without an atom (proposed case 42). Mutable
global state is a `def` of an `Atom` (`(def counter: (Atom i64) (atom 0))`,
L15), never of a `Cell` or a `Weak`: see below.

**Run-time `def`s (L15)**: an initialiser outside the constant grammar
(a call, `atom`, `set`, a closure) is evaluated once before `main`, after
the arguments are known, by an init function: the `def`s in the order the
checker typed them (a `def` after the `def`s its initialiser names, and
those that the functions it calls read; modules in dependency order, a
module's `def`s in source order). The value and everything it reaches become
immortal and shared (types §8.2), so it is read as a global like a constant
and an `Atom` among it locks. **The type of a `def` may not contain a `Cell`
or a `Weak`** (types §2.16, case 1702); a trap in an initialiser reads
`def NAME: MESSAGE` (case 1706). The interpreter evaluates every `def` so;
`fibc` makes the constants as static data and the others by one init
function per module (`crates/fibc/src/inits.rs`).

Why a core form: it binds a name (§4.1, criterion 2) and introduces the
one kind of binding that is never released. liar had no `def`, so its
libraries spelled every constant as a nullary function (`(defun
O_RDONLY () 0)`); for a scalar that costs nothing, for a table it
allocates on every call.

---

### 3.20 `var`

```
(var name)          ; name a symbol, qualified (m/x) or not
```

**Decided** (owner, 2026-09-28; §5). `(var name)` stands for the
top-level definition `name` resolves to (a function, a constructor, a
variant constant, a protocol method, a `def`, an `extern`) exactly as the
symbol `name` would in expression position, with two differences: a
`:private` definition of another module is visible to it (§5), and a
local binding of the same name is not (it names a top-level definition
only). It evaluates to what the symbol would; in the head of a call it
is a direct call, as the symbol is (types §2.2), so `((var m/helper) x)`
is the call `(m/helper x)` would be if `helper` were exported. It names
values only: a type or a protocol has no `var`, since a macro template
that must name a private type or protocol spells a constructor, a
constant or a method through `var` instead, or the module exports the
type. It is core (§4.1 rule 2: it decides what a name refers to, which
no macro or function can), and it is the form a macro's expansion uses
to reach a private helper of its own module. `(var x)` where no
definition is named `x` is `var: no definition named x`.

## 4. Core versus macro versus library

### 4.1 The rule (Decided)

A form is **core** iff at least one of:

1. the checker must see it on the surface to decide a rule of ownership.md
   (`&`: distinct variables, the escaping-capture rule; `fn`/`async`: capture
   and escape; `match`/`let` patterns: derived borrows and
   exhaustiveness; `.`: field paths);
2. it binds names or introduces a type, a protocol or an implementation,
   or decides what a name refers to (`defun def fn let defstruct defenum
   defprotocol impl defmacro extern ns var`);
3. it changes control flow or the evaluation regime in a way no function
   can (`if do match loop recur async await unsafe quote`).

Everything with an ordinary call shape whose meaning is "evaluate the
arguments, then do something" is a **library function**, even when the
compiler implements it directly (a **builtin**) or it has its own typing
rule (a **primitive**). Everything that is a pure rewrite of forms
preserving verdicts and error positions is a **macro**.

### 4.2 Core forms

Twenty-three: `defun def fn let if do match loop recur defstruct . defenum
defprotocol impl & async await unsafe extern quote defmacro ns var`
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
| arithmetic (protocol methods with built-in scalar instances, types §2.12) | `Num`: `+ - * / quot rem neg` (`quot` is the division `/` is, and `/` leaves `Num` for the library's `Div`, stdlib §7 L30); `Float` (`f32` and `f64`): `fdiv`; `Eq`: `= !=`; `Ord`: `< <= > >=`; `Bits` (integers): `bit-and bit-or bit-xor bit-not shl shr sar popcount`; `not` |
| conversions (target type first) | `trunc zext sext fptrunc fpext fptosi fptoui sitofp uitofp char->i32 i32->char` |
| float bit casts (ordinary builtin functions, types §2.12) | `(f64->bits x: f64) -> i64`, `(bits->f64 n: i64) -> f64`, `(f32->bits x: f32) -> i32`, `(bits->f32 n: i32) -> f32`: the IEEE 754 binary64 or binary32 pattern of a float and the float of a pattern, with every bit kept: a NaN's payload and its quiet bit, the sign of a zero. They never trap |
| arrays (types §2.13) | `array array-len array-get array-with array-copy array-set!` |
| structs | `set-field!` |
| the program's surroundings (M5) | `(args) -> (Vec str)`, the command line after the program (`fibc run FILE -- a b`, `fibref run FILE -- a b`, or a built executable's own), each word a `str` whatever its bytes: an invalid UTF-8 sequence in it becomes U+FFFD exactly as Rust's `String::from_utf8_lossy` makes it, one replacement for each maximal prefix of a sequence (a stray continuation byte; a lead cut short by the next byte or by the end of the word; `FE` and `FF`), so an overlong encoding, an encoded surrogate and a value above 10FFFF are replaced byte by byte, and `(args)` never traps (**Decided**, owner, 2026-10-01; a built executable did not convert, and its `str` was not UTF-8, and `fibc run` and `fibref run` panicked on such a word; the tools' own words before `--` must be UTF-8 and are refused otherwise, exit 2; each way is compared with `from_utf8_lossy` over the same 792 byte strings (the empty one, each of the 255 non-zero bytes alone, 59 hand-written kinds, valid text and every shape of invalid sequence, in four positions each, and 300 from a fixed seed): a built executable and `fibc run` by `crates/fibc/tests/cli/args.rs`, `fibref run` by `crates/fibref/tests/run_io.rs`); `(read-file path: str) -> (Option str)`, the whole file, `nil` when it cannot be read (a directory, a missing or unreadable file, a path with a NUL in it) or is not UTF-8, `(some "")` only for a file that is empty; `(write-file path: str text: str) -> bool`, whether the whole text was written (false for a path with a NUL in it) |
| strings | `str-len str-bytes str-from-bytes str-concat str-slice str-byte-at str-find str-eq starts-with?` and `Countable`/`Eq`/`Ord`/`Hash` instances; `(str-from-bytes a: (Array i8)) -> str` is a fresh string of the bytes and traps `str-from-bytes: invalid UTF-8` unless they are the shortest UTF-8 encodings of scalar values (M5); `(str-byte-at s: str i: i64) -> i8` is byte `i` of the UTF-8 text (bytes above 127 are negative) and traps `str-byte-at: index {i} out of range 0..{n}` when `i` is not in `0..n`, `n` being `(str-len s)`; `(str-find s: str pat: str from: i64) -> (Option i64)` is the byte offset of the first occurrence of `pat` at or after byte `from`, `nil` when there is none, an empty `pat` being found at `from`, and it traps `str-find: from {from} out of range 0..{n}` unless `0 <= from <= n` and `str-find: from {from} splits a character` when `from` is inside a multi-byte character; neither copies the string (`str-byte-at` allocates nothing; `str-find` allocates its result, one `Option` object, as any `Option` of a scalar is a heap enum, types §8.1) (stdlib design 7, L18) |
| forms | `Form` constructors, `gensym`, `struct?`, `struct-fields`, `struct-params`, `struct-field-types`, `enum?`, `enum-params`, `enum-variants` (§3.16) |
| `Option` (built in, §3.9) | `some` (constructor), `nil` (a literal, §1.1); `nil?`, `some?`, `if-let` are prelude definitions (§4.4, §4.5) |
| unsafe | `ptr+ load-i8 load-i16 load-i32 load-i64 load-ptr store-i8 ... store-ptr alloc free raw raw-retained release-raw`; `(alloc n)` is `n` zero bytes (§3.15) |
| dynamic dispatch | `(dyn P e)`, `(dyn P :send e)` |

`set-field!`, `dyn` and the conversions that name a target type
(`trunc` to `uitofp`) are **primitive forms**: each takes one operand
that is not an expression, a field name second for `set-field!`, a
protocol first for `dyn` (followed by the keyword `:send` in `(dyn P
:send e)`, which is not an operand either) and the target type first for a conversion
(types §2.12, §2.13, §2.15). They keep the call shape of §4.1 and
are read and expanded as calls, except that the expander leaves that
operand alone and name resolution and typing read it by the form's own
rule, as a field name, a type or a protocol, never as a variable. Every
other operand of a primitive form, and every argument of the other
names above, is an expression or, at an `&` position, an `&x` (§3.13),
evaluated left to right as in any call (§2), except that the name of an
`&` parameter may also stand as the operand of `deref` and the target
of `set!` (§3.11, §3.13). `array-set!` and `set-field!` are not values
(types §2.13).

### 4.4 Prelude macros (normative list for the reference implementation)

| Macro | Expands to |
|---|---|
| `when`, `unless`, `and`, `or` | `if` |
| `cond` | `(cond t1 e1 t2 e2 ..)`, Clojure's flat pairs (stdlib E4): nested `if`s, `(cond a 1 b 2 :else 3)` ⟹ `(if a 1 (if b 2 3))`. A test that is a keyword (`:else`, or any other) is always true and must be the last test (`malformed cond: a keyword test must be the last`); the symbol `else` is not special, it is a variable. Without a keyword test falling off the end is `(fib.prelude/trap "cond: no clause matched at POS")`, which has every type, so `(cond)` is that trap (X5 changes it to `nil`). An odd number of forms is `malformed cond: a test with no expression`, at the last form. The paired `(cond (t e) ..)` of the first tranche is gone; a clause of several bodies is `t (do b1 b2)` |
| `if-let`, `when-let`, `nil?`-free option tests | `match` (the pair `(p e)` may be written `[p e]`, as `dotimes`'s `(i n)` may be `[i n]`, stdlib E3): `(if-let (p e) a b)` ⟹ `(match e ((some p) a) (_ b))`, `(when-let (p e) body ..)` ⟹ `(match e ((some p) body) (_ ()))`. `p` may be any pattern (§3.6): a name, a vector pattern, a variant pattern, a literal; the else is taken on `nil` and on a mismatch of `p`, so a refutable `p` is exhaustive (an else of `(nil b)` made it `missing (some [])`). `(if-let (p e) a)` has `()` as its else (arity 2 to 3; stdlib design §2.4, §7 E13) |
| `list` | `(list a b)` ⟹ `(fib.prelude/Cons a (fib.prelude/Cons b fib.prelude/Empty))`, `(list)` ⟹ `fib.prelude/Empty` (`Empty` is `List`'s field-less variant, used bare: §3.9; the variants are qualified so that the library's function `cons` and method `empty` cannot capture them) |
| `plet` | §3.12 |
| `while`, `dotimes`, `for-each` over a `range` | `loop`/`recur` (§3.18): `(while c body)` ⟹ `(loop () (if c (do body (recur)) ()))`; `(dotimes (i n) body)` ⟹ `(let ((m n)) (loop ((i 0)) (if (< i m) (do body (recur (+ i 1))) ())))` with `m` a gensym; `(for-each (range a b) (fn (i) body))`, with a literal `(range a b)` or `(range n)` (read as `(range 0 n)`) and a literal `fn` with no name, one unannotated parameter and no result annotation, ⟹ `(let ((s a) (m b)) (loop ((i s)) (if (< i m) (do body (recur (+ i 1))) ())))` with `s` and `m` gensyms, the body spliced in with no closure. A loop variable takes no annotation, so a `fn` whose parameter is annotated (`(fn (i: i64) ..)`) is left to the library function, which checks it (case 86). The bounds are evaluated once, left to right, before the loop variable exists, so a bound that mentions a variable named like `i` sees the outer one (**Decided**, owner, 2026-09-27; the earlier `(loop ((i 0) (m n)) ..)` bound `n` inside the loop's own `i`); any other `(for-each c f)` is `(fib.seq/run! f c)`, the library's walk with the function first (the prelude's `for-each` function and its `Traversable` protocol are gone, stdlib §8.3 C), and a call of any other number of arguments is the macro's own arity error |
| `range` | `(range a b)` ⟹ `(fib.seq/range-by a b 1)`, a `Range`; `(range n)` is not rewritten: it is the library function `range`, which is also what `range` names as a value (§4.5). A call of three arguments, Clojure's `(range a b step)`, is declined as well and stays a call of the library function, whose arity error says `use range-by` (stdlib §7 D1); `(range)` and four or more arguments are the macro's own arity error. There is no arity overloading, so the two-argument form exists as this rewrite (**Decided**, owner, 2026-09-27: both arities) |
| `->`, `->>`, `doto` | call rewriting |
| `assert` | `(assert c)` / `(assert c msg)` ⟹ `(if c () (trap msg))`, the default message naming the position and the test |
| `dbg` | `(dbg e)` ⟹ `(let ((t e)) (eprintln (str-concat "dbg FILE:LINE:COL: E = " (show t))) t)` (every head written `fib.prelude/NAME`) with `t` a gensym and the literal text naming the call's position and `e` as written: evaluates `e` once, prints it with `Show` to stderr, returns it (**Decided**, owner, 2026-09-27) |
| `derive` | `(derive P Name)`, for `P` one of `Eq`, `Ord`, `Hash`, `Show`, `Debug`, `ToStr` (any other is `cannot derive X: only Eq, Ord, Hash, Show, Debug and ToStr`; the last two, §3.16): one `impl` whose head comes from `struct-params` or `enum-params`, whose `:where` lists `(P t)` for each parameter used by a field (`struct-field-types`, `enum-variants`; for `Ord` too, whose supertrait `Eq` the context then entails, types §4.1) and whose methods go field by field over `struct-fields` for a struct, or variant by variant over `enum-variants` for an enum, with a nested `match` on both operands (§3.16); `(do)` for a field-less enum, whose instances are built in; several protocols are several `derive` forms, which a macro may return in one top-level `do`. The prelude itself contains `(derive Eq Option)`, `(derive Ord Option)`, `(derive Hash Option)` and the same three for `List`; their `Show` is written by hand in `lib/prelude.fib` (§4.5) |
| `defn`, `defn-` | `(defn name doc? [x: T ..] -> R body ..)` ⟹ `(defun name (x: T ..) -> R body ..)`; `defn-` puts `:private` after the name; the docstring is dropped; everything after the vector (`-> R`, `:where`, the body) is passed on. A `&` in the vector is the error `rest parameters need L2`, a list of clauses `([x] ..) ..` is `several arities need L1` (stdlib §7). The built `defun` takes the call's position and the parameter list the vector's |
| `str`, `println`, `print`, `prn`, `pr` | `(str)` ⟹ `""`; `(str a b ..)` ⟹ the right fold of `fib.prelude/str-concat` over the pieces, a string-literal piece as it is, a literal `nil` as `""`, any other piece `(fib.core/to-str x)`. `(println a b ..)` ⟹ `(fib.prelude/println S)` and `(print a b ..)` ⟹ `(fib.prelude/print-str S)`, `S` the pieces joined by one space, a literal `nil` piece as `"nil"` (a bare `nil` has no type, so it would otherwise be an ambiguous-constraint error) and any other piece, a string literal included, `(fib.prelude/show x)`; `(println)` and `(print)` write `""`. `prn` and `pr` are `println` and `print` over `(fib.core/debug x)`, with the same literal-`nil` rule. A name in value position (`(map str xs)`) is not a call and is left alone: the function twins of `fib.print` serve it (stdlib §2.7, §4.14). A `fib.core/` head resolves in every module that sees `fib.core`, which is every module but the library's own since the flip (§5), and in those by a `:use` |
| `+ - * < > <= >= = max min bit-and bit-or bit-xor` | the variadic folds, each answering only the calls the binary builtin or function cannot and **declining** the rest: `(+)` ⟹ `0` and `(*)` ⟹ `1` (`i64`s); `(+ a)`, `(* a)` ⟹ `a`; from three arguments `(+ a b c)` ⟹ `(fib.prelude/+ (fib.prelude/+ a b) c)`, a left fold, so the first sum that overflows traps; `(-)` is the arity error `macro - takes at least 1 argument(s), got 0`, `(- a)` ⟹ `(fib.prelude/neg a)`; `(< a b c)` ⟹ `(and (< a b) (< b c))` with every operand evaluated once, in order, before the first test (an operand that is not a symbol, a literal or a field path `(. x f)` is first bound to a gensym), and `(< a)` ⟹ `(let ((t a)) true)`; `max`, `min` and the three `bit-` operations fold from three arguments (`fib.core/max`, `fib.prelude/bit-and` ..) and decline below that |
| `conj`, `assoc`, `dissoc`, `merge` | `(conj c)`, `(dissoc m)` ⟹ the collection itself; `(conj c x y ..)`, `(dissoc m k1 k2 ..)` ⟹ the left fold of `fib.coll/conj`, `fib.coll/dissoc`; `(assoc m k v k2 v2 ..)` ⟹ the left fold of `fib.coll/assoc` by pairs, and a key without a value is the error `malformed assoc: a key without a value`; `(merge)` is the error `malformed merge: needs at least one argument`, a literal `nil` operand is skipped (`(merge a nil b)` is `(merge a b)`, and `nil` when every operand is one); the two- and three-argument calls (and `(merge a b)` with no literal `nil`) are declined: the method or function serves them. Clojure's rules for a literal `nil` first argument, `(conj nil x)` as `(list x)` and `(assoc nil k v)` as a map, are not implemented (stdlib §2.4) |
| `swap!` | `(swap! a f x ..)` ⟹ `(fib.prelude/swap! a (fn (v) (f v x ..)))`, `v` a gensym, the extra arguments evaluated inside the closure each time the builtin calls it (which may be more than once, §3.11); the two-argument call is the builtin's and is declined |
| `update` | `(update m k f x ..)` ⟹ `(fib.coll/update m k (fn (v) (f v x ..)))`, `v` a gensym; a literal `(fnil g d)` for `f` ⟹ `(fib.coll/update-or m k g d)` (with extra arguments, `g` is called with them inside the closure); a call of fewer than three arguments, and a three-argument call whose `f` is not a literal `fnil`, is declined and stays the library function `update` |
| `reduce` | `(reduce f c)` ⟹ `(fib.seq/reduce-nonempty f c)`; for the literal heads `+ * conj merge` ⟹ `(fib.seq/reduce f init c)` from `0 1 [] {}`; a literal `str` ⟹ `(fib.seq/reduce (fn (a x) (fib.prelude/str a x)) "" c)` (gensym parameters `#r.N`: the value `str` is the library's one-argument function, not the accumulator's step, so the step is the macro, and each element takes its `to-str` as `(str a b)` does; Clojure's `(reduce str c)`), and a literal `concat` ⟹ `(fib.seq/reduce concat (fib.seq/lazy-node (fn () fib.seq/LNil)) c)` (`concat` is the two-argument function and returns an `LSeq`, so the start is the empty lazy seq, not `[]`); `(reduce str init c)` is the same `fn` over the given `init`; `(reduce (fn (a x) body) init c)` whose body has `(reduced e)` in a tail position (found through `if let do match cond when unless if-let when-let loop`, with an explicit stack) ⟹ `(fib.seq/reduce-while f' init c)`, each tail `(reduced e)` rewritten to `(fib.core/Done e)`, every other value tail `v` to `(fib.core/More v)` and a `(recur ..)` left alone; any other call, a literal `fn` with a `->` or `:where` included, is declined and stays the library function. A literal head that the module defines itself or sees exported by a program module it `:use`s is that function, not the prelude's, and has no identity: `(reduce str c)` there is `reduce-nonempty` over the module's `str`. `(reduce + [1.5 2.5])` is `cannot unify f64 with i64` until L19 (cases 780-796) |

**Hygiene** (stdlib design §6.3, tranche 1 R2, R14). Every head or constant a
Rust prelude macro emits that is not a core form is written with the name
of the module that defines it, `fib.prelude/NAME` (`fib.core/`, `fib.coll/`
and `fib.seq/` for the library's names), so that a binding of the
program's own `cons`, `trap`, `show`, `+` or `update-or` is not what the
expansion calls; the bindings a macro introduces are gensyms. The core
forms (`if let match loop recur fn do .`) are plain. The macros an
expansion names, `and` and `or` (`(and a b c)`, `(< a b c)` and the derived
`Eq` and `Ord` write them), are written `fib.prelude/and` and
`fib.prelude/or`, and **a head `fib.prelude/NAME` is the prelude's macro
NAME in every module** whatever the module defines or a user `defmacro`
named like it says (R14, the owner's rule of 2026-10-01): a user
`defmacro and` does not change what `(< a b c)` means, and a module that
defines a function `when` reaches the macro as `(fib.prelude/when ..)`.
The exceptions are the heads the prelude's own expansions call as
functions, which the macros of the same name would answer again for ever:
`fib.prelude/println` is the function; a call written `fib.prelude/NAME`
that the macro declines (`(fib.prelude/+ a b)`, the binary builtin) is
that call, and counts no expansion step. The rule is checked by a test
that runs one sample per registry row and requires every head of the output
to be core or qualified (`expand/tests/hygiene.rs`).

**Declining, and a module's own definitions** (R14, the owner's rule of
2026-10-01: Clojure's resolution, a name is the module's own, then what its
`:use`s export, then the implicit library, then the prelude). A row that
says *declined* leaves the call unchanged, to be served by the builtin or
the library function of that name. The expander knows macros by name, so
it asks the module: **a prelude macro does not apply, and the call is an
ordinary call of the module's function, in a module that defines the name
itself** (a top-level `defun`, `defn`, `defn-`, `def`, `extern`,
`defstruct` constructor, `defenum` variant or `defprotocol` method, written
before or after the call, a top-level `do` included, or produced by a user
macro, from the form it appears in on) **or sees it exported by a program
module it `:use`s** (a public name; the library's modules, `fib.`, do not
count, since their `update`, `reduce`, `str` and the rest are the twins the
macros are designed to sit beside). So a user `defun` named `str`, `print`,
`println`, `prn`, `pr`, `when`, `list`, `max` or `update` is called, with
whatever arity and types it has, and a macro name in value position
(`(map str xs)`, `(run! println xs)`) is not a call and is left alone, as
ever. A user `defmacro` of the name still wins in the user's own module.
A local binding (a `let` or a parameter named `when`) is not a definition
of the module and does not hide the macro. The module reaches the macro
with the qualified head (`(fib.prelude/str a b)`, Hygiene above).

**Fusion** (stdlib design §2.1 rule 2, §7 E16) is not a macro: it is a
second pass over each top-level form after the macros have run, which
rewrites a chain of the library's sequence functions consumed by a
terminal into recipe structs (`(->> v (map f) (filter p) (reduce g 0))`
becomes `(reduce g 0 (fib.seq/Filtered (fib.seq/Mapped v f) p))`). Its rule
is stated in stdlib §2.1; the expansion dump of a program that sees
`fib.seq` shows the recipes and gensyms named `#fuse.N`, and the dump
format is unchanged (bootstrap §2).

### 4.5 Library (written in fibber, in `lib/`)

`Option` helpers (`nil?`, `some?`, `unwrap-or`; the type itself is built
in, §3.9); `List` (`(defenum (List a)
(Empty) (Cons head: a tail: (List a)))`, declared in the expander's
prelude, with `Eq`, `Ord` and `Hash` derived for it and for `Option` and
their `Show` written by hand in `lib/prelude.fib`: a present `Option` prints
as its payload, `nil` as `nil`, a `List` as `(1 2)` or `()`, which a derived
instance cannot say; the variants are capitalised because the library's
method `empty` and function `cons` would collide with lower-case ones,
stdlib design C-3); `(Pair a b)` with fields `fst` and `snd`, `(Triple a b
c)` with `fst`, `snd`, `thd`, and `(Result a b)` with `(Ok v)` and `(Err e)`
(`Eq`, `Ord`, `Hash` derived and no `Show` in the prelude: `fib.print` supplies one for `Pair` and `Triple`, and none exists for `Result` yet); a
public `print-str` writing a string without the newline; `Vec`, `Map`, `Set` as
persistent structures over `(Array T)` with the raw operations `vec-empty vec-conj
vec-nth vec-count map-empty map-assoc map-get map-count dissoc contains? map-put!
map-del! set-empty set-count disj set-contains?` (`Map` and `Set` are an HAMT over
the keys' `hash`, keys needing `Hash` and `Eq`; `map-each` and
`set-each` walk them and can stop early); the protocols `Hash` and `Show`. Since the
flip (stdlib §8.3) the prelude has no `Seq Countable Indexable Collection Associative
Traversable Iter`, no `iter collect filter-iter for-each map range`: `first rest count nth
conj map filter reduce range` and the rest of the sequence functions are the library's
(`fib.seq`, `fib.coll`, implicit in every module, stdlib §4). `(range n)` is `(range 0 n)`
(**Decided**, owner, 2026-09-27), and `(range a b)` is the prelude macro's rewrite to
`(fib.seq/range-by a b 1)`, a `Range` of the `i64`s from `a` up to but not including `b`,
empty when `b ≤ a` (§4.4); `pmap` (over a `Vec`), `append` (=
`push!`, over a `Vec`), `even?`, `length` (string length), `starts-with?`, `box`/`unbox`
over `(defstruct (Box a) (v: a))`, `yield`, `block-on`, I/O (`(println s: str) -> unit` and `(eprintln s: str) -> unit`, which write `s` and then a newline to standard output and to standard error, each as write(2) calls (§3.15) repeated until every byte has been written, a call being free to take fewer; a call that fails (-1) or takes no byte of a rest that is not empty is the trap `println: write failed` (`eprintln: write failed`), naming the function the program called, so a full device is a trap and never a success; the prelude does not retry an interrupted call, as it cannot read `errno`; the string may have gone out when the newline fails (**Decided**, owner, 2026-10-01; case 192, `crates/fibc/tests/cli.rs`, `crates/fibref/tests/run_io.rs`); `dbg` uses the latter; `read-file`, `write-file` and `args` are builtins, §4.3).

`for-each`, `map`, `filter`, `reduce`, `swap!` take their function
parameter `:borrow`; `pmap`'s function parameter escapes (its body
spawns closures that capture it), which is what makes case 13's closure
a thread crossing. An `&` update through a library function (`push!`,
`append`, ...) copies on the first update of every call, because the
copy-in acquires (§3.13, D1); the library's own in-place paths are the
unique writes it performs on objects it holds alone, and a build that
must not copy keeps the object in one function and uses the primitives,
or uses `conj`, which shares structure with the previous version (§5).

---

## 5. Modules

```
(ns name clause*)
clause ::= (:require [name :as alias]+) | (:use name+) | (:export-from name+)
```

One `ns` form, first in the file, names the module (dotted; `a.b` lives
at `a/b.fib` under a root given to the compiler). `:require` makes
`alias/x` refer to `x` in that module; `:use` brings all of a module's
exported top-level names in unqualified. A name defined locally shadows a
`:use`d one; two `:use`d modules exporting the same name make that name
an error when referenced unqualified. The prelude is not counted as a
second `:use`: a `:use`d module's name shadows the prelude's, as a local
definition does.

**Roots and re-exports** (**Proposed**, stdlib design §6.2, §7 E7). A
*root* is a directory under which a module `a.b` is the file `a/b.fib`.
A module is looked for in the directory of the main file, then in each
library root in order: first each `-I DIR` of the command line
(compiler.md §1) in the order given, then each directory of the
environment variable `FIB_LIB`, then the library that the executable
carries (the modules of the repository's `lib/` other than the prelude,
which every executable has). The first place that has the file wins and
the others are not read: a program's module shadows a library module of
the same name, and a module found in two roots is the first one's. A
module found in no place is an error that names the file it was expected
at and the others it was looked for in.
`(:export-from m ..)` is a `:use` that is also an export: the module
exports what the modules named export, as if it had defined them (their
`:private` definitions are not exported), so that one name, `fib.coll`, can
stand for several files. Two modules re-exported under one name make that
name an error where it is referenced unqualified, as two `:use`s do, and the
same definition re-exported by two modules is one name. The modules a
`:export-from` names are required, so they may not require the module back.

**Private names** (**Decided**, owner, 2026-09-28; replaces "every
top-level definition is exported, no private names in v1"). The keyword
`:private` right after a definition's name keeps it out of the module's
interface:

```
(defun helper :private (x) ..)        (def table :private [..])       (def t: (Vec i64) :private [..])
(defstruct Node :private (..))        (defstruct (Node a) :private (..))
(defenum (Tree a) :private ..)        (defprotocol Walk :private ..)
(defmacro m :private (x) ..)          (extern write :private (i32 ptr i64) -> i64)
```

It is written where the other qualifiers of a binding are, after the
thing it qualifies, as `:borrow` follows a parameter (§3.1) and `:send`
a protocol (§3.10): a second head per definition (Clojure's `defn-`)
would double the core forms, and fibber has no metadata maps. A private
definition's names are visible in its own module and in no other: a
function's, a `def`'s or an `extern`'s name; a struct's name as a type
and as its constructor; an enum's name and every variant, as a
constructor, a constant and a pattern; a protocol's name and every
method; a macro's name. From another module, a `:use` does not bring
them in, a qualified `m/x` does not reach them, and `struct?`, `enum?`
and the other reflection calls of §3.16 (and so `derive`) do not see a
private type: each is the error `x is private to m; it is not
exported`, where the same text without the definition would be `unbound
name x`, `unknown type x` or `unknown protocol x`. Since a private name
is not brought in, it never conflicts: the using module may define the
same name, and two `:use`d modules may both have a private `x`.
Privacy is of names, not of values: a public function may return a
value of a private type, whose fields `.` reads and whose methods
dispatch as usual, and an `impl` of any protocol is a global fact
(below) whether its protocol or type is private or not. The interface
(types §3.9) still carries what the compiler needs of a private
definition that exported code reaches (the layout of a private type, a
private function a generic exported body calls), marked as not
nameable.

**Macros and private helpers.** A macro's expansion is resolved in the
module that uses the macro, like every expansion (there is no automatic
hygiene, §3.16), so a template that names a private helper of the
macro's own module by a bare or qualified symbol would reach nothing,
or the user's own name. It writes `(var m/helper)` instead (§3.20): the
one reference that sees a private definition, which is Clojure's
answer as well (its syntax-quote qualifies `helper` to `m/helper`, which
compiles only if `helper` is public, and `#'m/helper`, the var, is how a
macro reaches a private one). Like Clojure's, it is a deliberate door,
not a security boundary: privacy keeps a module's interface small and
its helpers free to change, and `var` is visible in the text wherever
it is used. A macro defined in the module that uses it needs neither.

The reference implementation loads a program's modules from the main
file's directory and then the library roots above, `a.b` at `a/b.fib`,
once each in dependency order (M5; `cases/modules`, whose README says how
a case names its roots), and checks every rule above between them and
with the prelude: a reference to a private definition, unqualified or
qualified, reflection on a private type, and `(var m/x)` (cases 113 to
116 for the prelude, cases/modules for other modules), macros
included: a module's macros reach the modules that `:use` it
unqualified and the ones that `:require` it through the alias, and a
`:private` macro its own module only. Requires may not be cyclic.
`fib.prelude` and the implicit modules (below) are implicitly `:use`d. Protocol implementations are global
facts and are always visible once their module is required.

**Implicit modules** (stdlib design §6.2, §4.4; in force since the flip of
tranche 1). A program
may have modules that every one of its modules sees without naming them: the
reference implementation lists them in `modules::IMPLICIT_LIB`, which names the
facades `fib.core`, `fib.seq`, `fib.coll` and `fib.print`. They are loaded before the program's
other modules, in the order listed, and seen as the prelude is, between a
module's `:use`s and the prelude: a module's own definition and a `:use`d
module's name shadow an implicit module's, with no clash (an implicit module
is not a second `:use`), and the full name of an implicit module
(`fib.seq/x`) names its exports from any module that sees it, without a
`:require`, so that a macro's template can name the library function it
needs by the facade's name and be expanded in a module that has never
mentioned it (a `:require` alias spelled the same shadows it). Two implicit
modules that export one name differently make it an error where it is
referenced bare, as two `:use`s do (the library keeps its four facades
disjoint, `crates/fibref/tests/lib_disjoint.rs`, and its parts one facade's
each). A module whose name starts `fib.` (the library's own), `Long` and
`Math` see no implicit module, and neither does an implicit module: they
write their `:use` lines by hand. `fibref expand` leaves the implicit
modules out of its dump as it does the prelude (spec/bootstrap.md §5.1).

A protocol belongs to its module like any other definition, and so do its
methods: two modules may each define a protocol of one name with methods of
one name (case 008), and a program's own protocol `Collection` with a method
`conj`, implemented for `(Vec a)`, shadows the library's for a bare `conj`
while the literal `[1 2 3]` stays the prelude's `vec-conj` and `fib.coll/conj` the library's
(case 007, stdlib design §7 B3). A compiler names a method's
code and a protocol's vtables with the defining module, so that two such
protocols cannot be taken for one (compiler.md §2).

A module compiles against the **interfaces** of its requires and never
re-infers a dependency: every exported function's generalised type
scheme with colour constraints, escape summaries and count kinds (types
§3.9, §6.4),
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
(defun head (xs)
  (match (first xs)
    ((some x) x)
    (nil (trap "head: empty list"))))

(defun main () -> i64
  (let ((l (list (box 1) (box 2))))
    (let ((h (head l)))
      (unbox h))))
```
The body of `head` changed with the flip of tranche 1 (stdlib §8.3, the owner's Q28): `first` is the
library's, and it returns `(Option e)`, so `head` matches it and traps on nil, and answers the element
as it did. `list` is the prelude macro (`(fib.prelude/Cons (box 1)
(fib.prelude/Cons (box 2) fib.prelude/Empty))`, §4.4), `box`/`unbox` the prelude `Box` struct. `head`'s
parameter is inferred as `∀c e. (Reducible c e) ⇒ (fn (c) e)`; the element it returns is memory the
list owns, which the callee retains on return.

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
`unit`; **Decided**). The shape is unchanged: one cell, two escaping
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
Unchanged. The self tail call is a real tail call (types §6.10;
**Decided**, D5): `acc` is inferred owned, `(conj acc n)` is moved into
the call and the previous version is released before the jump, so the
recursion runs in constant stack and each version dies as soon as the
next exists.

### 08-mutate-while-iterating.fib

```lisp
;; spec:   §5
;; expect: accept
;; result: 6
;; audit:  clean
;; The loop is still reading v when append replaces it. append's own
;; copy-in holds a count and the caller's cell keeps the original alive,
;; so append copies instead of updating in place, and the loop sees the
;; original three elements.
(defun dup-all (&v)
  (for-each @v (fn (x) (append &v x))))

(defun main () -> i64
  (let ((v (cell [1 2 3])))
    (dup-all &v)
    (count @v)))
```
Change: inside `dup-all` the `&` parameter is a cell, so the vector it
holds is read with `@v` (§3.13). That read is a retained reference
(types §6.3), but since D1 the caller's cell also keeps the vector
alive for the whole call, so the case passes whether or not the read
is counted: it pins copy-in/copy-out and the stack-closure capture of
`&v`, not the acquire, which proposed cases 66 and 78 pin. `append`
copies because its own copy-in acquires (§3.13, D1; traced in types §6.6),
not because the iteration holds a count; the header comment was
corrected to say so (the verdict is unchanged). `append` is the library
in-place push.

### 09-iterator-outlives-source.fib

```lisp
;; spec:   §3.1
;; expect: accept
;; result: 2
;; audit:  clean
;; The returned iterator refers to a vector that the caller's scope
;; has already let go of.
(defun evens (v) (filter even? v))

(defun main () -> i64
  (let ((it (let ((v [1 2 3 4])) (evens v))))
    (count (vec it))))
```
The body changed with the flip (stdlib §8.3): the prelude's `Iter` protocol, `iter`, `filter-iter` and
`collect` are gone; the library's `filter` is the lazy sequence, `vec` the collector. `filter` stores `v`
into the recipe it returns; that store is the escape that retains it.

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
          (vec (range 1000)))
    (count @a)))
```
The argument of `pmap` changed with the flip (stdlib §8.3): `(range 1000)` is a `Range`, and `pmap`
takes a `Vec`, so the case writes `(vec (range 1000))`. The result of `pmap` is a discarded `do` step.

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
    (pmap (fn (i) (set! n (+ @n i))) (vec (range 10)))
    @n))
```
The argument of `pmap` changed with the flip, as in 10.

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
unchanged (**Decided**: the rewrites of 05, 08, 15 and 19 each still
exercise their rule).

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
          (vec (range 1000)))
    (let ((final @accts))
      (+ (. final a) (. final b)))))
```
The argument of `pmap` changed with the flip, as in 10. `(transfer accts 1)` is a discarded `do` step whose owned
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

None remain. On 2026-09-27 the owner signed off the 29 items of the
three review rounds, each as recommended except where the table says
otherwise, together with seven amendments:

- **D1** copy-in always acquires; no copy-in move, no exclusive places,
  no takeable structs, no taken fields, no field places (§3.13);
- **D2** `&` parameters are cells read with `@v`, never values (§3.13);
- **D3** HM inference generalising only at top-level `defun` SCCs, no
  coercions, no literal polymorphism (§1.1, §3.9; types §3);
- **D4** closure colours as the two-point lattice (§3.12; types §5.4);
- **D5** full tail calls with inferred owned/borrowed parameters,
  replacing "only self tail calls become loops" (§2, §3.1, §3.13,
  §3.18; types §6.10);
- **D6** stack allocation of scope-local objects in v1 (§3.3; types
  §6.11);
- **D7** `async` captures must be `Send`; the executor is
  multi-threaded (§3.12, §3.14).

Each item is now **Decided** in the section that states its rule; the
old numbering is kept here because the drafts and
`spec/drafts/PROPOSED_CASES.md` cite it. The one question that
applying D5 raised (forwarding an `&` parameter beyond the self call at
its own position) is recorded in types §10, with the two that the
fourth review round raised (two names for one cell in one call;
ownership.md §5 and §8 against D2) and the corrections that round made
to §2, §3.1, §3.2, §3.3, §3.18 and Appendix A (case 08) here. A fifth
review round corrected §2, §3.1, §3.2 and §3.14 here (a call in tail
position of an `async` body is never a tail call; in §3.2 also the
tail sites at which a closure is put on the heap) and §3.13 (what the
`@v` acquire is pinned by); types §10 lists them with that round's
corrections to types.md. A sixth review round corrected §3.11 here (the
target of `set!` is an expression of cell type or the name of an `&`
parameter, not "any expression", since an `&` parameter is none), §3.13
and §4.3 (`set-field!` is a primitive form whose field operand is a
name, neither `&` primitive is a value, and neither takes a copy-in or
a write-back) and §2 (the same for the order of evaluation); types §10
lists them with that round's corrections to types.md, among them the
scope at whose exit the interpreter ends a stack object. On 2026-09-28
the owner moved the copy-in of an `&` argument from the argument's
position to call entry (§2, §3.13), and then closed the one question
that applying it left open (a forwarded cell written through another
name during the call) by not forwarding an `&` parameter that another
argument of the call captures (§3.13); types §10 records both.
On 2026-09-28 the owner lifted item 11: patterns may be vector patterns and clauses
may have guards (§1.4, §3.3, §3.6); types §10 records the decision.

| Item | Rule | Now in |
|---|---|---|
| 1 | `&` only on `defun` parameters; `fn` has none | §3.2 |
| 2 | `&` parameters are cells, read with `@v`, not values (D2) | §3.13 |
| 3 | copy-in moves when exclusive — **decided the other way (D1): always acquire** | §3.13 |
| 4 | `&(. x f)` field places — **decided the other way (D1): variables only** | §3.13, §3.8 |
| 5 | `set!`/`reset!` return `unit`; `swap!` the new value | §3.11, Appendix A case 05 |
| 6 | shadowing allowed; liar ADR 006 dropped | §3.3 |
| 7 | unhygienic macros with `gensym`, run by the interpreter | §3.16 |
| 8 | literal collections desugar to prelude calls | §1.4 |
| 9 | `Option` an ordinary enum; no lifting, no narrowing (D3) | §3.9 |
| 10 | no literal polymorphism (D3) | §1.1 |
| 11 | no vector patterns, no guards in v1 — **replaced by the owner's decision of 2026-09-28: vector patterns `[p* & r]` and guarded clauses `(pat :when g body+)`** (types §10, "Decided on vector patterns and guards") | §1.4, §3.3, §3.6 |
| 12 | variant names unqualified, unique per namespace | §3.9 |
| 13 | `first`/`nth` trap; `first?`/`nth?` return `Option` | Appendix A case 01 |
| 14 | cases 05, 08, 15, 19 rewritten as shown | Appendix A |
| 15 | non-final `do` steps may have any type | §3.5 |
| 16 | signed overflow wraps — **replaced by the owner's decision of 2026-09-27: Rust's semantics (overflow and division by zero trap, shift amounts masked, float-to-integer conversion saturating)** | types §2.12, §8.12 |
| 17 | `loop`/`recur` core — **amended by D5: kept core for the two reasons §3.18 gives; semantics now the tail-call rule** | §3.18 |
| 18 | field-less variant bare in expressions, `(V)` in patterns | §3.6, §3.9 |
| 19 | instance contexts declared with `:where` | §3.10 |
| 20 | `raw-retained`/`release-raw` replace `:retains` | §3.15 |
| 21 | `main` waits for spawned threads | §3.12 |
| 22 | async-function definition (mentions an `&` parameter, or is the value) | §3.1 |
| 23 | `:borrow` on `defun` and method parameters | §3.1, §3.10 |
| 24 | top-level `do` splices | §2, §3.16 |
| 25 | `def` constants, immortal | §3.19 |
| 26 | struct reflection: `struct-params`, `struct-field-types` | §3.16 |
| 27 | enum reflection and `derive` on enums | §3.16, §4.4 |
| 28 | `Option` built in; `nil` a reader literal | §1.1, §3.9 |
| 29 | `def`s are nodes of the dependency order | §3.19 |
