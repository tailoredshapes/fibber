# Proposed cases

Programs that the confirmed findings on `spec/syntax.md` and
`spec/types.md` turned into candidate test cases. Each entry gives the
header the case would carry (the format of `cases/ownership/README.md`),
the program in the syntax of `spec/syntax.md`, and the rule it pins.
None of these is a case yet: a program moves into `cases/` only with
the owner's sign-off on its verdict (method.md rule 3). Numbers
continue from the twenty existing cases; the spec sections cited are
the ones that decide the verdict after the findings were applied.

A verdict marked `accept` also asserts a clean audit (method.md rule 2),
and every `accept` program below was traced by hand under both the
interpreter's plain counting and the compiler's rules of types §6; the
two must agree (method.md rule 6). Where a finding's program was
ill-typed as written, the corrected spelling is given and the change is
noted.

---

## 21-reject-inout-param-used-as-value

```lisp
;; spec:   §5 (private cell lives only for the call)
;; expect: reject
;; error:  & parameter v used as a value
(defun leak (&v) v)

(defun main () -> i64
  (let ((x (cell [1 2 3])))
    (let ((c (leak &x)))
      (count @c))))
```

Pins syntax §3.13 rule 5 / types §2.14: an `&` parameter occurs only as
`@v`, `&v`, `&(. v f)` or the target of `set!`. Before the rule, `leak`
type-checked with result `(Cell (Vec i64))` and returned a pointer to
the caller's stack-allocated private cell, freed at the write-back.
The same error must be reported for the variants
`(defun leak2 (&v) (cell v))` (store), `(defun leak3 (&v) (map @v (fn
(i) v)))` (a non-escaping closure returning `v`) and `(defun leak4 (&v)
(weak v))`.

## 22-whole-object-pattern-variable-escapes

```lisp
;; spec:   §3.3, §4
;; expect: accept
;; result: 5
;; audit:  clean
;; A function returns its parameter through a symbol pattern. The
;; parameter escapes, so the closure passed to it is escaping and
;; retains its capture; the string outlives the scope that made it.
(defun same (p) (match p (w w)))

(defun main () -> i64
  (let ((g (let ((s (str-concat "he" "llo")))
             (same (fn () (str-len s))))))
    (g)))
```

Pins types §6.1/§6.3/§6.4: a pattern variable that binds the whole
scrutinee is `Borrowed(p)`, so `same`'s summary is `p: escapes`, the
`fn` literal is escaping (§6.5 clause (b) does not apply) and E3
retains `s`. Under the old reading (`w` as `Derived(p)`) the summary
was `noescape`, the closure was non-escaping and `(g)` called through a
dead frame whose capture pointed at freed memory. `fibc --explain` must
show `closure ... escaping=yes reason=arg-to-escapes(same.p)`.
`str-concat` is used so that the string is not an `IMMORTAL` literal.
Two more programs pin the same rule and should be added beside it:

```lisp
(defenum T (Leaf) (Node inner: T))
(defun unwrap1 (p) (match p ((Node inner) inner) ((Leaf) p)))   ; join of Derived(p) and Borrowed(p) → Owned, retain on the Leaf branch → p escapes
(defun w (p) (match p (x (weak x))))                             ; x is Borrowed(p) → operand of weak → p escapes
```

## 23-cell-read-beside-sibling-write

```lisp
;; spec:   §6, §2 (same frees)
;; expect: accept
;; result: 0
;; audit:  clean
;; The struct read from the cell is replaced by a sibling argument of
;; the same call. The read is counted, so the old struct and its array
;; live until the call has returned.
(defstruct S (arr: (Array i64)))

(defun main () -> i64
  (let ((s (cell (S (array 1 0)))))
    (array-get (. @s arr)
               (do (set! s (S (array 1 7))) 0))))
```

Pins types §6.3 (`@c` read: never elided). With the withdrawn elision
the compiled program read the freed array under `array-get` while the
interpreter answered 0 with a clean audit. Companion programs for the
same rule: `(defun g (&s) (array-get (. @s arr) (do (set-field! &s arr
(array 1 7)) 0)))` (the elided read also falsified `unique?`, so the
in-place write freed the array under the call) and a nested-cell
`set!` target `(set! (. @d inner) (do (set! d (S2 (cell []))) [1]))`.

## 24-async-capture-used-before-await

```lisp
;; spec:   §8
;; expect: accept
;; result: 5
;; audit:  clean
;; The parameter is used only before the first await, but the body
;; does not run until the task is driven, after the caller's scope has
;; released the string. Every capture is retained at creation.
(defun measure (s) (async (str-len s) (await (yield)) 5))

(defun main () -> i64
  (let ((task (let ((s (str-concat "hel" "lo"))) (measure s))))
    (block-on task)))
```

Pins types §6.9 (no pre-`await` elision) and syntax §3.14. Sits next
to case 11, whose use of `s` after the `await` forced the retain
anyway. `str-concat` keeps the string off the `IMMORTAL` path.

## 25-inout-through-self-tail-call

```lisp
;; spec:   §5
;; expect: accept
;; result: 3
;; audit:  clean
;; An & accumulator threaded through a self tail call. The tail call
;; forwards the private cell; the single write-back happens when the
;; loop returns to main.
(defun fill (&v n)
  (if (= n 0) () (do (append &v n) (fill &v (- n 1)))))

(defun main () -> i64
  (let ((x (cell [])))
    (fill &x 3)
    (count @x)))
```

Pins types §6.10 step 2 and syntax §3.13 (self tail call with `&v` at
its own position: no copy-in, no write-back). Under the literal loop
rewrite each iteration moved the vector into a fresh private cell that
was never written back: `x` ended up empty and the vector leaked.
`fibc --explain` must show `&v: forward (self tail call)` at the tail
call and `&v: move (v exclusive)` at `append`, so every append is in
place.

## 26-fire-and-forget-spawn

```lisp
;; spec:   §7
;; expect: accept
;; result: 0
;; audit:  clean
;; The task handle is discarded while the thread is still running. The
;; thread holds its own count on the task; main's return waits for it.
(defun main () -> i64
  (do (spawn (fn () (count (range 100000))))
      0))
```

Pins types §6.8/§8.8 (task created with count 2; `main` joins running
threads at exit; syntax §3.12). Under the old text the discarded
`Owned` temporary of the `do` step released the only count and the
thread wrote its result into freed memory. Variant with a conditional
`join`: `(let ((t (spawn (fn () (count (range 100000)))))) (if (> (count
[1]) 5) (join t) 0))`, result 0.

## 27-nested-option-some-nil

```lisp
;; spec:   §1 (values), method.md rule 6
;; expect: accept
;; result: 1
;; audit:  clean
;; (some nil) and nil are different values of (Option (Option str)).
(defun wrap (x: (Option str)) -> (Option (Option str)) (some x))

(defun main () -> i64
  (match (wrap nil) ((some _) 1) (nil 0)))
```

Pins types §8.1/§8.3 (the nullable-pointer representation only for a
non-`Option` object type; `(Option (Option T))` is a heap enum) and
§4.3 (layout class `opt`, so the generic `wrap` keeps the distinction
after monomorphisation). Under the old row both values were the null
pointer and the compiled program answered 0 against the interpreter's
1. Variant without a function boundary: `(let ((o: (Option (Option
str)) (some nil))) (match o ((some _) 1) (nil 0)))`.

## 28-weak-of-literal

```lisp
;; spec:   §6
;; expect: accept
;; result: 1
;; audit:  clean
;; A weak reference to a string literal. The literal is immortal, so
;; the reference always upgrades; its box is freed with the binding.
(defun main () -> i64
  (let ((w (weak "abc")))
    (if (nil? @w) 0 1)))
```

Pins types §8.7 (`weak` on an `IMMORTAL` object: unregistered box,
never cleared, the object's header untouched, box freed when its count
reaches zero). Likewise `(weak count)` on a named function used as a
value.

## 29-inout-update-inside-dotimes

```lisp
;; spec:   §5 (unique updates happen in place), §2
;; expect: accept
;; result: 5
;; audit:  clean
;; A vector built by pushing inside a loop. The loop body is part of
;; the function, so v stays exclusive and every push! moves and writes
;; in place.
(defun iota (n: i64) -> (Vec i64)
  (let ((v (cell [])))
    (dotimes (i n) (push! &v i))
    @v))

(defun main () -> i64
  (count (iota 5)))
```

Pins syntax §3.18 and §4.4 (`dotimes` expands to `loop`/`recur`, not to
a named `fn`) with §3.13 exclusivity. The verdict alone does not
distinguish in-place from copying, so the case must also assert the
checker's output: `fibc --explain` shows `&v: move (v exclusive)` at
the `push!`, and the interpreter's trace shows one array allocation
per growth step of the persistent vector and no copy of the vector
object. The same expectation holds for `while` and `loop`/`recur`
spelled by hand. (A `for-each` over a collection with a closure body
still copies on its first update, by design: syntax §4.5.)

## 30-taken-field-reached-through-weak

```lisp
;; spec:   §5, §6
;; expect: accept
;; result: 1
;; audit:  clean
;; The struct has a weak reference to itself, so the field place is
;; acquired rather than moved, and the weak upgrade inside the callee
;; sees an intact struct.
(defstruct S (w: (Cell (Option (Weak S))) n: i64))

(defun peek (&c: (Cell (Option (Weak S)))) -> i64
  (match @(deref c)
    ((some wk) (match @wk
                 ((some s) (match @(. s w) ((some _) 1) (nil 0)))
                 (nil 0)))
    (nil 0)))

(defun go (&x: S) -> i64 (peek &(. x w)))

(defun main () -> i64
  (let ((x (cell (S (cell nil) 1))))
    (set! (. @x w) (some (weak @x)))
    (go &x)))
```

Pins syntax §3.13 / types §6.6 (`fib.takeable?`: a field is moved out
only when the struct has no weak box) and the interpreter's `read of
taken field` audit failure, which this program must *not* trigger.
Under `unique?` alone the field was taken, `@wk` upgraded and
`@(. s w)` loaded through null in accepted code.

## 31-user-count-writes-a-cell

```lisp
;; spec:   §6, §2 (same frees)
;; expect: accept
;; result: 1
;; audit:  clean
;; A user implementation of count writes a cell that holds the only
;; other reference to self. The read of @c is counted, so self survives
;; the write; the cycle is broken by the set!, so nothing leaks.
(defstruct Bag (back: (Cell (Option (Cell Bag)))))

(impl Countable Bag
  (count (self)
    (match @(. self back)
      ((some c) (do (set! c (Bag (cell nil)))
                    (match @(. self back) ((some _) 1) (nil 0))))
      (nil 0))))

(defun main () -> i64
  (let ((c (cell (Bag (cell nil)))))
    (set! (. @c back) (some c))
    (count @c)))
```

Pins types §6.3 (no `@c` elision: `count` is a protocol method, so
"writes no cell" was not decidable by name). Companion program needing
no user impl, inside `(defun f (&p) ...)`: `(str-concat (. @p name)
(do (set-field! &p name "z") "!"))`, where the counted read makes the
sibling `set-field!` copy instead of freeing the old name under
`str-concat`.

## 32-await-inside-loop

```lisp
;; spec:   §8
;; expect: accept
;; result: 3
;; audit:  clean
;; A loop that awaits on every iteration inside an async body.
(defun main () -> i64
  (block-on (async (dotimes (i 3) (await (yield))) 3)))
```

Pins syntax §3.18 / §3.14 and types §2.8, §6.9: a `loop` body inside an
`async` is inside it, so `await` is legal there. Under the named-`fn`
expansion of `dotimes` this was rejected with `await outside async`.

## 33-list-of-two

```lisp
;; spec:   §1
;; expect: accept
;; result: 2
;; audit:  clean
;; The normative list expansion ends in the bare constant empty.
(defun main () -> i64 (count (list 1 2)))
```

Pins syntax §3.9 / §4.4 and types §2.2: `(list 1 2)` expands to `(cons
1 (cons 2 empty))`; a field-less variant is used bare, and `(empty)`
is the error `empty is a constant, not a function`. Cases 01 and 05
depend on the same expansion.

## 34-reject-missing-instance-through-impl-context

```lisp
;; spec:   types §2.7, §3.5 (declared instance contexts)
;; expect: reject
;; error:  no implementation of Describe for (Cell i64)
(defstruct (Wrap a) (v: a))
(defprotocol Describe (describe (self) -> str))
(impl Describe i64 (describe (self) "n"))
(impl Describe (Wrap a) :where ((Describe a))
  (describe (self) (describe (. self v))))

(defun f (b) (describe b))

(defun main () -> i64
  (str-len (f (Wrap (cell 0)))))
```

Pins types §2.7/§3.5: `f` is generalised with the bound `(Describe a)`
taken from the declared context of `(impl Describe (Wrap a))`, so the
call in `main` fails at type checking rather than at monomorphisation.
Variant: the same program with `:where` omitted from the generic impl
is rejected earlier with `no implementation of Describe for a; add
(Describe a) to the :where of the impl`.

## 35-field-place-of-let-cell

```lisp
;; spec:   §5
;; expect: accept
;; result: 1
;; audit:  clean
;; An in-place update through a field place of a let-bound cell.
(defstruct P (items: (Vec i64) tag: str))

(defun main () -> i64
  (let ((s (cell (P [] "b"))))
    (push! &(. s items) 1)
    (count (. @s items))))
```

Pins syntax §3.13's place grammar (`(. sym field)` for any variable of
cell type, not only an `&` parameter) and types §2.14. The finding's
spelling `(set-field! &(. s name) "z")` was ill-typed (`set-field!`
takes the struct's place and a field name: `(set-field! &s name "z")`,
which the old grammar already accepted), so the case uses the field
place with a library in-place update instead. A `:where` on a `defun`
(syntax §3.1) can be pinned beside it:

```lisp
(defun first-len (x: (Vec a)) :where ((Countable a)) -> i64 (count (nth x 0)))
(defun main () -> i64 (first-len [[1 2 3]]))     ; accept, 3, clean
```

## 36-raw-retained-balanced

```lisp
;; spec:   §9
;; expect: accept
;; result: 2
;; audit:  clean
;; A count handed to foreign code and released again on the fibber side.
(defun main () -> i64
  (let ((s (str-concat "a" "b")))
    (unsafe (let ((p (raw-retained s))) (release-raw p)))
    (str-len s)))
```

Pins syntax §3.15 / types §6.13 (`raw-retained` is a consume position;
`release-raw` releases that count). The unbalanced program (no
`release-raw`) must fail the audit with a leak; the header format has
no verdict for that, so it belongs in the audit's own tests, not in
`cases/`. A real extern callback (finding: `g_signal_connect`) needs a
C stub and is out of scope for the case directory.

---

Cases 37 onward came out of the second round of confirmed findings on
`spec/syntax.md` and `spec/types.md`.

## 37-push-into-form-literal-copies

```lisp
;; spec:   §1 (objects are immutable), §5
;; expect: accept
;; result: 2
;; audit:  clean
;; A vector pulled out of a Form literal is pushed to through a cell.
;; The literal is immortal, so the push copies and the literal is not
;; changed: a second look at it still counts two items.
(defun items (f: Form) -> (Vec Form)
  (match f ((List xs) xs) (_ [])))

(defun main () -> i64
  (let ((q '(a b)))
    (let ((c (cell (items q))))
      (push! &c 'z)
      (count (items q)))))
```

Pins types §8.2 / §6.6 (`fib.unique?` is false on `IMMORTAL`, `STACK`
and `SHARED` objects; immortal objects carry count 0) and syntax §3.13,
§3.16. Trace: `(items q)` returns the literal's own `(Vec Form)` (join
of `Derived(f)` and `Owned`, the retain a no-op); `c` is exclusive, so
the vector is moved into `push!`'s private cell; `push!`'s
`set-field!`/`array-set!` see the `IMMORTAL` flag and copy; the copy
is written back into `c` and freed at `main`'s exit. Under the old
`unique?` (count == 1 and not `SHARED`) an immortal whose count field
read 1 was written in place: the compiled program mutated read-only
data or answered 3, and the interpreter, whose literals were ordinary
allocations, answered 2. The interpreter must allocate the literal with
the `IMMORTAL` header so that its `write-unique` event refuses the
write for the same reason the compiler's does.

## 38-inout-function-blocks-on-task

```lisp
;; spec:   §8, §5
;; expect: accept
;; result: 1
;; audit:  clean
;; An & function that drives a task to completion inside the call and
;; only then updates its parameter. No frame outlives the call and the
;; task never mentions the & parameter, so this is not an async
;; function.
(defun bump (&v)
  (let ((n (block-on (async (do (await (yield)) 1)))))
    (push! &v n)))

(defun main () -> i64
  (let ((x (cell [])))
    (bump &x)
    (count @x)))
```

Pins syntax §3.1 / §3.13 rule 3 and types §6.9: a `defun` is an async
function iff an `async` in its body mentions one of its `&` parameters
or is the body's value. Under the earlier definition (any `defun`
containing an `async`) this was rejected with `& parameter in async
function: v in bump` for no safety reason. Case 14 keeps its verdict:
`fill`'s `async` mentions `&buf` and is the function's value. The
companion reject programs that the narrowed rule must still catch:
`(defun fill2 (&buf) (async (push! &buf 1)))` (mentions the parameter),
`(defun fill3 (&buf) (do (push! &buf 1) (async 5)))` (the value is a
task; error text of case 14), and `(defun fill4 (&v) (let ((g (fn ()
(push! &v 1)))) (block-on (async (g) 1))))`, rejected by rule 2 (`&
parameter captured by escaping closure: v in fill4`), since `g` is
captured by the `async` and so escapes.

## 39-derive-eq-on-generic-struct

```lisp
;; spec:   §1; types §2.7, §3.3; syntax §3.16
;; expect: accept
;; result: 1
;; audit:  clean
;; derive on a struct whose fields are unannotated, hence generic. The
;; macro reads the parameters and field types by reflection and emits
;; the head and the :where context a programmer would write.
(defstruct Pair (a b))
(derive Eq Pair)

(defun main () -> i64 (if (= (Pair 1 "x") (Pair 1 "x")) 1 0))
```

Pins syntax §3.16 (`struct-params`, `struct-field-types`; the normative
expansion `(impl Eq (Pair a b) :where ((Eq a) (Eq b)) ..)`) and types
§2.7 / §3.3 (the call resolves through the declared context to the
`i64` and `str` instances). Before the reflection extension the macro
could not produce the head (the arity and parameter names of `Pair`
were not available) and, even with the head, its body was rejected with
`no implementation of Eq for a; add (Eq a) to the :where of the impl`.
The `fibc --explain` output should show the expanded `impl` head and
context. Variant: `(defstruct (Wrap a) (xs: (Vec a)))` with `(derive Eq
Wrap)` and `(= (Wrap [1]) (Wrap [1]))` → 1: the field's `(Eq (Vec a))`
reduces through the `Vec` instance to the listed `(Eq a)`.

## 40-reject-declared-borrow-escapes

```lisp
;; spec:   §4 (types §6.4)
;; expect: reject
;; error:  declared :borrow but escapes
(defun keep (xs: (Vec i64) :borrow) -> (Vec i64) xs)

(defun main () -> i64 (count (keep [1 2])))
```

Pins syntax §3.1's `param` grammar (`sym :borrow`, `sym: type
:borrow`) and types §6.4: `xs` is `Borrowed(xs)` at E1, so its summary
is `escapes`, contradicting the declaration; the full text is
`parameter xs of keep is declared :borrow but escapes`. Before the
grammar admitted the annotation, the error in the catalogue could not
be produced. The accept companion, which pins that the annotation
changes no verdict and makes the closure literal non-escaping by
contract (types §6.5 clause (b)):

```lisp
(defun each-twice (xs f :borrow) (for-each xs f) (for-each xs f))
(defun main () -> i64
  (let ((n (cell 0)))
    (each-twice [1 2 3] (fn (x) (set! n (+ @n x))))
    @n))                                        ; accept, 12, clean
```

`fibc --explain` must show `closure ... escaping=no
reason=arg-to-borrow(each-twice.f)` and `f  param ... escapes=no
(declared)`.

## 41-top-level-macro-splices-forms

```lisp
;; spec:   syntax §2, §3.16 (a top-level do is spliced)
;; expect: accept
;; result: 1
;; audit:  clean
;; A macro whose expansion is two definitions returns them as one
;; (do ..) form; the struct is registered before the derive that
;; follows it is expanded.
(defmacro defrecord (name fields)
  `(do (defstruct ,name ,fields)
       (derive Eq ,name)))

(defrecord Point (x: i64 y: i64))

(defun main () -> i64 (if (= (Point 1 2) (Point 1 2)) 1 0))
```

Pins syntax §2 and §3.16: a top-level `(do f₁ .. fₙ)` stands for its
forms, expanded and registered in order, so the spliced `(derive Eq
Point)` sees the spliced `defstruct`. Before the convention a macro
could return only one form and `do` had no top-level meaning, so the
normative `derive`/`defrecord` shape had no representation. Variant:
`(do)` at top level defines nothing and is not an error.

## 42-def-table-read-from-two-threads

```lisp
;; spec:   §1, §7 (an immortal immutable object is safe from any thread)
;; expect: accept
;; result: 8
;; audit:  clean
;; A global constant table built once before main and read from two
;; threads without an atom: its objects are immortal and immutable, so
;; they need no count, no Send check and no share-marking walk.
(def primes [2 3 5 7 11 13])

(defun main () -> i64
  (plet ((a (count primes))
         (b (nth primes 0)))
    (+ a b)))
```

Pins syntax §3.19 and types §2.16, §6.1 (a `def` name is a global, not
a capture: the `plet` thunks mention `primes` without capturing it),
§8.2 (`fib.immortalise`: count 0, `IMMORTAL`), §6.7 (immortal objects
are not audited allocations, so the table is not a leak at exit).
Reject companions for the same rule: `(def e [])` → `def e has an
unresolved type; annotate it`; `(def c (cell 0))` → `def c: initialiser
is not a constant expression`.

## 43-task-joined-from-two-threads

```lisp
;; spec:   §7, §8
;; expect: accept
;; result: 10
;; audit:  clean
;; One task, joined from two threads. The task is resumed by whichever
;; joiner claims it; the other waits for its completion and reads the
;; same result.
(defun main () -> i64
  (let ((t (async (do (await (yield)) 5))))
    (plet ((a (join t))
           (b (join t)))
      (+ a b))))
```

Pins types §8.8 (one driver at a time: `cmpxchg` on `driver`; a losing
`join` waits on the broadcast completion signal; `state`/`result`
written once with release ordering) and §1.6 (`Send (Task i64)`). The
verdict alone does not detect a double resume; an instrumented or
thread-sanitised build of the compiled program does, and the
interpreter's coroutine scheduler must refuse to resume a task that is
already running. Count trace: `t` (1, the `let`) is captured by both
thunks (3); each thread releases its closure (1); the `let` releases
(0); the task's `drop` releases nothing counted (a scalar result).

## 44-task-awaited-by-two-tasks

```lisp
;; spec:   §7, §8
;; expect: accept
;; result: 10
;; audit:  clean
;; One task awaited by two tasks. Each registration goes into the
;; awaited task's waiter list; completion wakes both.
(defun main () -> i64
  (let ((t (async (do (await (yield)) 5))))
    (let ((u (async (await t)))
          (w (async (await t))))
      (+ (block-on u) (block-on w)))))
```

Pins types §8.8 (the waiter list; registration under the task's lock
after re-checking `state`; each registration retains the awaiting task
and completion releases it) and syntax §3.14. Under a single waker
slot the second registration overwrote the first, one awaiter was never
woken and the count its registration took was never released, a leak
the audit reports. `(await t)` on the already-completed `t` inside `w`
returns the retained result at once.

## 45-bounded-generic-at-two-ptr-class-types

```lisp
;; spec:   types §4.2, §4.3 (method.md rule 6)
;; expect: accept
;; result: 6
;; audit:  clean
;; A generic function with a protocol bound, instantiated at two types
;; of the same layout class. Each instantiation is its own
;; specialisation, so the method call inside is a direct call.
(defstruct (Box a) (v: a))
(defstruct Tag (name: str))
(impl Show Tag (show (self) (. self name)))
(impl Show (Box a) (show (self) "box"))

(defun describe (x) (str-len (show x)))

(defun main () -> i64
  (+ (describe (Tag "abc")) (describe (Box 1))))
```

Pins types §4.3 (a protocol-bounded type variable is keyed by its full
type argument, so `describe<Tag>` and `describe<(Box i64)>` are
distinct specialisations, while an unconstrained variable still shares
by layout class) and §4.2. Under the class-only key both calls fell
into one `describe<ptr>` in which `(show x)` named no implementation,
so the compiler as specified could not be built or answered differently
from the interpreter, which dispatches on the header's type id. The
finding's program used `(impl Show str ..)` for the first receiver;
this spelling uses a user struct so that the case does not depend on
whether the prelude already provides `Show` for `str` (one instance
per `(P, K)`, types §4.1). `fibc --explain` must list both
specialisations.

## 46-weak-taken-on-shared-object

```lisp
;; spec:   §6, §7
;; expect: accept
;; result: 1
;; audit:  clean
;; A weak reference is taken on one thread while another thread
;; retains the same shared object. The HAS-WEAK bit is set with an
;; atomic or and the retain reads the flags atomically.
(defstruct B (v: i64))

(defun main () -> i64
  (let ((b (B 1)))
    (plet ((w (weak b))
           (o (some b)))
      (match o ((some x) (. x v)) (nil 0)))))
```

Pins types §8.2 / §8.7 (the flags word is read with `atomic-load
monotonic`, `HAS-WEAK` set with `atomicrmw or` under the weak table's
mutex). The verdict alone does not detect the race (a plain load of
`flags` racing with a plain store is `undef` under the LLVM model, not
a wrong answer on every run); an instrumented or thread-sanitised build
of the compiled program is what pins the rule. Count trace: `b` (1) is
captured twice (3); `(some b)` retains (4); the closures release (2);
`o` and then `b`'s own `let` release (0). The box: created at 1 as the
first thunk's result, held by its task, retained by `join` for `w` (2);
`w`'s release and the task's drop bring it to 0, but its target is
alive, so it stays registered until `b`'s drop runs `weak-clear`, which
nulls the target and frees it; clean.

## 47-as-pattern-in-let

```lisp
;; spec:   syntax §3.3, §3.6
;; expect: accept
;; result: 3
;; audit:  clean
;; The :as pattern in a let: the parts and the whole are bound at once.
(defstruct Pair (a b))

(defun main () -> i64
  (let ((((Pair a b) :as p) (Pair 1 2)))
    (+ a (. p b))))
```

Pins the single spelling `(pat :as sym)` of syntax §3.3 and §3.6 (types
§2.6): §3.3 had the mirror image `(x :as pat)`, so exactly one of the
two spellings parsed depending on which section an implementer read.
Ownership: the initialiser is `Owned`, so the `let` owns an implicit
binding `t` for it; `p` is an alias of `t`, `a` and `b` are scalar
copies; `t` is released at the `let`'s exit.

---

Cases 48 onward came out of the third round of confirmed findings on
`spec/syntax.md` and `spec/types.md`.

## 48-derive-eq-on-enum

```lisp
;; spec:   §1; syntax §3.9, §3.16, §4.4; types §2.12
;; expect: accept
;; result: 1
;; audit:  clean
;; derive on a sum type. The macro reads the variants by reflection and
;; emits the nested match on both operands that had to be written by
;; hand before.
(defenum Shape (Circle r: f64) (Rect w: f64 h: f64))
(derive Eq Shape)

(defun same? (a: Shape b: Shape) -> bool (= a b))

(defun main () -> i64 (if (same? (Circle 1.0) (Circle 1.0)) 1 0))
```

Pins syntax §3.16 (`enum?`, `enum-params`, `enum-variants`; the
normative expansion of `(derive Eq Shape)` given there) and types
§2.12. Before the enum reflection existed `(derive Eq Shape)` was the
expansion error `struct-fields 'Shape — Shape is not a struct` and `(=
a b)` on any enum was `no implementation of Eq for Shape`, so `Eq`,
`Ord`, `Hash` and `Show` for every sum type, `Option` and `List`
included, were hand-written nested matches growing with the square of
the variant count. `fibc --explain` must show the expanded `impl`.
Count trace: the two `Circle` objects are temporaries of the call
step, borrowed by `same?` and released after it returns; clean.
Companions for the same rule, each `accept` with a clean audit:
`(if (= (Circle 1.0) (Rect 1.0 1.0)) 0 1)` → 1 (different variants);
`(if (= (some 1) (some 1)) 1 0)` → 1 and `(if (= (list 1 2) (list 1 2))
1 0)` → 1 with no `derive` in the program (the prelude's own, syntax
§4.4); `(if (< nil (some 1)) 1 0)` → 1 (`Ord` by variant order);
`(defenum Colour Red Green)` with `(if (= Red Red) 1 0)` → 1 and
`(derive Eq Colour)` expanding to `(do)` (built-in instances for a
field-less enum).

## 49-closure-through-self-tail-call

```lisp
;; spec:   §3.3, §2 (an object that never escapes is freed at scope end)
;; expect: accept
;; result: 1
;; audit:  clean
;; A closure passed at a self tail call outlives the scope that made
;; it: the jump releases x before the next iteration calls g. The
;; closure is escaping whatever spin's summary for g says, so it
;; retains x and frees it with itself.
(defun make () (conj [] 1))

(defun spin (g n)
  (let ((x (make)))
    (if (= n 0)
        (g)
        (spin (fn () (count x)) (- n 1)))))

(defun main () -> i64 (spin (fn () 0) 1))
```

Pins types §6.5 (a closure literal, or a `let`-bound closure of clause
(c), that is an argument of a self tail call or a `recur` is escaping
regardless of the callee's summary) and §6.10. `spin`'s summary for `g`
is `noescape` (its only occurrence is the call `(g)`), so under clause
(b) alone the literal was non-escaping and its capture of `x` an
uncounted alias; the tail call is a slot loop, whose jump runs the
`let`'s scope exit (freeing `x`) and then stores the closure into slot
`g`, and the next iteration's `(g)` read freed memory in the compiled
program while the interpreter, which retains every capture, answered
1. `fibc --explain` must show `closure ... escaping=yes
reason=arg-of-self-tail-call captures: x (owns)`. Count trace under
the rule: `main`'s `(fn () 0)` captures nothing; `spin`'s slot `g`
retains it. Iteration 1: `x` = `V` (1); the literal `C` retains `V`
(2) at E3; the `let` exit releases `x` (1); the old slot value is
released; `g := C`, `n := 0`. Iteration 2: `x` = `V'` (1); `(g)` reads
`C`'s own count on `V` → 1; the `let` exit frees `V'`; the exit path
releases the slots: `C` is freed and releases `V` (0, freed). Clean.
Companions with the same verdict and trace: the `let`-bound spelling
`(let ((f (fn () (count x)))) (spin f (- n 1)))`, and the `loop`
spelling `(loop ((g (fn () 0)) (n 1)) (let ((x (make))) (if (= n 0)
(g) (recur (fn () (count x)) (- n 1)))))`.

## 50-def-names-a-function

```lisp
;; spec:   syntax §3.19; types §2.16, §3.5
;; expect: accept
;; result: 42
;; audit:  clean
;; A def whose value is a named function. The def is typed after the
;; function's SCC, so the function's scheme exists; its value is the
;; function's immortal constant closure, called through the global.
(defun double (x: i64) -> i64 (+ x x))
(def twice-fn double)

(defun main () -> i64 (twice-fn 21))
```

Pins types §2.16 / §3.5 (`def`s are nodes of the dependency graph,
typed after the functions they name and before the functions that read
them) and syntax §3.19. Before the change a `def` was typed before any
`defun` had a scheme, so the grammar's "a named function as a value"
could not be typed; `double` here is annotated so that the `def`'s type
is closed without an annotation of its own. Count trace: `twice-fn`'s
value is `IMMORTAL` (§8.2), reading it is count-free, the call goes
through the closure's code pointer, the result is a scalar; clean.
Reject companions for the same rule: the generic `(defun double (x) (+
x x))` with `(def twice-fn double)` → `def twice-fn has an unresolved
type; annotate it` (and `(def twice-fn: (fn (i64) i64) double)` with
it → accept, 42, clean); `(def t [f])` with `(defun f () -> i64 (count
t))` → `def t and defun f depend on each other`. The literal-collection
side of the same finding is already pinned by case 42: `(def primes [2
3 5 7 11 13])` is typed through the prelude's `vec-empty`/`conj`
schemes, the only calls a constant expression contains.

## 51-macro-emits-nil

```lisp
;; spec:   syntax §1.1, §3.9, §3.16
;; expect: accept
;; result: 3
;; audit:  clean
;; A macro whose expansion contains nil in a pattern. The reader gives
;; the macro the form (Nil), and the expander accepts that form as the
;; constant wherever nil may stand.
(defmacro or-zero (e)
  `(match ,e ((some x) x) (nil 0)))

(defun pick (flag) (if flag (some 3) nil))

(defun main () -> i64 (+ (or-zero (pick true)) (or-zero (pick false))))
```

Pins syntax §1.1 (`nil` is a literal, not a symbol), §3.9 (`Option` is
built in; the form `(Nil)` and a macro-built symbol `nil` are the
constant in expressions and the empty-variant pattern in patterns;
`(nil)` is the `(Variant)` spelling) and §3.16. Before the rule the
prelude's own `(defenum (Option a) (nil) (some v: a))` could not be
read, since `(nil)` reads as `(List [(Nil)])`, and nothing said what a
macro's `(Nil)` meant to the expander. Count trace: each `(pick ..)`
result is the `match`'s implicit owning binding, released after the
whole form (a heap enum, since the payload is a scalar, types §8.1);
`x` is a scalar copy; clean. Companions: `(defmacro none () 'nil)`
used as an expression of type `(Option i64)`; and a macro that builds
the empty-variant pattern from `(enum-variants 'Option)`, which yields
`(List [(Sym "nil")])` and so emits the pattern `(nil)`.
