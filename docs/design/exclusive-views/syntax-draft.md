# Draft rows for `spec/syntax.md` (exclusive views)

**Draft, not spec.** Proposed by package D2; to be merged into `spec/syntax.md` only when the owner approves
`docs/design/exclusive-views.md`. Section numbers are those the rows would take in the live file (3.20 `var` is the last
section of 3 today). Everything here is **Proposed**.

## Row for 3.7 (`defstruct`), grammar

```
(defstruct Name :scoped? private? (field+))
(defstruct (Name param+) :scoped? private? (field+))
```

`:scoped` marks the struct as a **scoped type** (types 1.9): its values may be borrowed but never consumed. It may follow the
name or parameter list, before `private`. A struct with a scoped field is scoped without the flag. The flag only restricts: a
`:scoped` struct is otherwise an ordinary struct. Constructing one that holds a raw pointer needs `unsafe` as any such struct does.

## Row for 3.1 (`defun`)

Add after the paragraph on `&`: a **lender** is a `defun` with at least one `&` parameter whose declared result type is scoped. It
may be called only as the initialiser of a `with-view` binding (3.21); anywhere else is `lender call outside with-view: f`. A
lender's body that produces its scoped result does so in `unsafe`.

## 3.21 `with-view`

```
(with-view [binding+] body+)
binding ::= pat (lender &place arg*)
place   ::= sym
```

`with-view` lends the cell `place` exclusively for the evaluation of `body`, and binds the window the lender returns. It is a core
form: the checker must see on the surface which variable is lent, what the body mentions and what the binders are used for
(§4.1 criterion 1).

Evaluation (**Proposed**). Bindings are processed left to right as `let*` does. For one binding:

1. the arguments `arg*` are evaluated left to right, then the call `(lender &place arg*)` runs as an ordinary `&` call in which
   the `&place` argument is **taken** (types 6.6, "Taking the content"): the content moves into the lender's private cell and
   is written back at its return, with count 1 after the lender's unique test;
2. the result is an implicit owning temporary of the form; `pat` is matched against it as `let` matches (3.3), a symbol or a
   struct pattern, and each variable it binds is a **view cell** of the scoped type of the part it names;
3. the next binding, then the body, are evaluated; the value of the form is the value of the last body expression;
4. when the body has been evaluated, the temporaries of the bindings are released in reverse order (types 6.3 scope exit) and the
   lent places are usable again.

The body is not in tail position and no call in it is a tail call (types 6.15 rule L5).

**Place** (**Proposed**). `place` is a variable that is a *private cell* in the sense of types 6.3 (cell peeks, P1): an `&`
parameter of the enclosing `defun`, a `let` cell initialised directly by `(cell e)`, or a view cell of an enclosing `with-view`; every
occurrence of it in the enclosing body is the place of `@`, `set!`, an `&` argument or `set-field!`, and no `fn` or `async` literal
captures it. Otherwise the error is `lent place must be a private cell: c` followed by the reason.

**Freeze** (**Proposed**). While a binding's lend is active, the place may not occur anywhere in the remaining operands of that
lender call, in the later bindings, or in the body, inside `fn` and `async` literals included: `t is lent to v and cannot be used
here`. The same place named by two bindings of one form is this error.

**View cells** (**Proposed**). A variable bound by `with-view` is a view cell. It is not a value (as an `&` parameter is not, §3.13).
It may occur only as `&v`, an `&` argument at a call, or as `@v` consumed at once by a borrowed position of a non-tail call or by a
scalar field read, with no other operand of the call mentioning `v`. It may not be assigned and may not occur in a `fn` or `async`
literal. Errors:

| Situation | Text |
|---|---|
| `v` as a value, `(let [w @v] ..)`, an owned or stored position | `view cell v can only be read as a call argument` |
| `(set! v e)` | `view cell v cannot be assigned` |
| `v` in a `fn` or `async` literal | `view cell v cannot be captured by a closure` |
| `(f &v @v)`, `(f &v v2 &v)` | `view cell v is passed in-out and used again in the same call` |
| `await` in the body | `await inside with-view` |
| value of the form of scoped type | `with-view value has scoped type T` |
| operand not a lender call with a `&place` | `with-view needs a call of a function with an & parameter and a scoped result` |

## 4.2 Core forms

`with-view` is added: twenty-four core forms. 4.3 (builtins) gains nothing; the window operations are library functions
(`fib.tensor`, `fib.array`), the sigil-free `vget vset! fill! copy!` are prelude macros (4.4) over `win-get @v`, `win-set! &v`,
`win-fill! &v`, `win-copy! &v @src`.

## 3.13 `&`

Add: an `&` argument may name a view cell (3.21) as it names a `let` cell or an `&` parameter; a view cell passed as `&v` is lent
to the callee as a private cell of scoped content type.

## Appendix A cases to add (rejects, with the accepting twin each)

`with-view` rows V1 to V9 of `exclusive-views.md` 7.1, each as `NNN-reject-*.fib` with its text in the header and a `NNNb` twin.
