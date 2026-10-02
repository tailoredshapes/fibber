# Proposed cases

**Promoted 2026-09-27.** Every entry below except 30 and 35 is now a
case in `cases/ownership/` under the same name, and all of them pass
under the reference interpreter. 30 and 35 are withdrawn: they use
`&(. x f)` field places, which D1 removed, and do not read. This file is
kept for the reasoning and the count traces behind each case; the case
files are authoritative.

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
interpreter's counting (plain counting with the deliberate exceptions
of types §6.12) and the compiler's rules of types §6; the two must
agree (method.md rule 6). Where a finding's program was
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
      (vec-count @c))))
```

Pins syntax §3.13 rule 5 / types §2.14: an `&` parameter occurs only as
`@v`, `&v` or the target of `set!` (the field place `&(. v f)` that this
entry once listed went with D1: `&` names a variable, never a field).
Before the rule, `leak` type-checked with result `(Cell (Vec i64))` and
returned a pointer to the caller's stack-allocated private cell, freed
at the write-back. The same error must be reported for the variants
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
    (vec-count @x)))
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
  (do (spawn (fn () (vec-count (range 100000))))
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
  (vec-count (iota 5)))
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

(defprotocol Countable (count (self) -> i64))

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
(defun main () -> i64 (vec-count (list 1 2)))
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
    (vec-count (. @s items))))
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
      (vec-count (items q)))))
```

Pins types §8.2 / §6.6 (`fib.unique?` is false on `IMMORTAL`, `STACK`
and `SHARED` objects; immortal objects carry count 0) and syntax §3.13,
§3.16. Trace, as corrected for D1, which removed the exclusive move:
`(items q)` returns the literal's own `(Vec Form)` (join of `Derived(f)`
and `Owned`, the retain a no-op), so `c` holds an immortal object and no
count; `push!`'s copy-in acquires it into its private cell, a no-op on
an immortal; `push!`'s `set-field!`/`array-set!` see the `IMMORTAL` flag
and copy; the write-back puts the copy into `c` (releasing the immortal,
a no-op), and `c`'s inline drop frees it when the inner `let` exits
(types §6.11: at its scope's exit, which here ends `main`). The literal
is untouched and the result is still 2. Under the old `unique?` (count ==
1 and not `SHARED`) an immortal whose count field read 1 was written
in place: the compiled program mutated read-only data or answered 3, and
the interpreter, whose literals were ordinary allocations, answered 2.
The interpreter must allocate the literal with the `IMMORTAL` header so
that its `write-unique` event refuses the write for the same reason the
compiler's does.

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
    (vec-count @x)))
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

(defun main () -> i64 (vec-count (keep [1 2])))
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
  (plet ((a (vec-count primes))
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
;; closure is on the heap whatever spin's summary for g says, so it
;; retains x and frees it with itself, after its own body has read it.
(defun make () (vec-conj [] 1))

(defun spin (g n)
  (let ((x (make)))
    (if (= n 0)
        (g)
        (spin (fn () (vec-count x)) (- n 1)))))

(defun main () -> i64 (spin (fn () 0) 1))
```

Restated under D5. Pins types §6.5 (a closure literal, or a
`let`-bound closure of clause (c), that is the head or an argument of
a tail call — E6 — is a heap closure that retains its captures,
whatever the callee's summary says; it is escaping only by its uses)
and §6.4 with §6.10 rule (e) (inside the closure's body its capture is
frame-owned, so a call that borrows it is not a tail call). `spin`'s
summary for `g` is `noescape` (its only occurrence is the call `(g)`),
and `g` is owned by rule 3 (the self tail call passes a fresh closure
at its position). Classified by clause (b) alone, the literal was a
stack closure whose capture of `x` was an uncounted alias: the jump
runs the `let`'s scope exit, freeing `x`, and the next iteration's
`(g)` read freed memory in the compiled program while the interpreter,
which retains every capture, answered 1. And while §6.4 called a
capture frame-independent, `(count x)` in the closure's body was a tail
call: the body released `env` before the jump, freeing the closure and
`x` under `count`. `fibc --explain` must show `closure ...
escaping=no heap=yes reason=arg-of-tail-call captures: x (owns)` and,
in the closure's body, `call (e: frame-owned argument at borrowed
position count.self)`. Count trace: `main`'s `(spin (fn () 0) 1)` is a
tail call, and `C0` (heap by E6; no captures) is moved into `g`.
Iteration 1: `x` = `X1` (1); `C1` retains `X1` (2) at E3 and is moved
into the call; the `let` exit releases `x` (1); the owned `g`, `C0`, is
released before the jump (freed). Iteration 2: `x` = `X2` (1), `n` =
0; `(g)` is a tail call through a closure value: `g` is moved as
`env`, the `let` exit frees `X2`, and control enters `C1`'s code with
`env` = `C1` (1). There `(count x)` is an ordinary call → 1; then `env`
is released: `C1` is freed and releases `X1` (freed). Clean.
Companions with the same verdict and trace: the `let`-bound spelling
`(let ((f (fn () (count x)))) (spin f (- n 1)))`, and the `loop`
spelling `(loop ((g (fn () 0)) (n 1)) (let ((x (make))) (if (= n 0)
(g) (recur (fn () (count x)) (- n 1)))))`, whose `(g)` is a tail call
of `main` and whose `recur` moves the new closure into `g` and releases
the old one (types §6.10).

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

---

Cases 52 onward came out of the fourth round of confirmed findings, on
the sections that the amendments D1–D7 touched, and include the
candidates that D5 and D6 call for (types §7): 53 to 57 are those, and
52 is the tail recursion of depth 10⁶ that types §8.11 cites.

## 52-cps-through-named-function-value

```lisp
;; spec:   §4 (types §6.10, §8.4)
;; expect: accept
;; result: 1
;; audit:  clean
;; Continuation-passing through a named function kept in a struct, a
;; million hops deep. Each hop is a tail call through a function value
;; into the function's all-owned body, so the stack stays flat and each
;; hop's vector dies at that hop's jump.
(defstruct K (f: (fn ((Vec i64) i64 K) i64)))

(defun mk () (vec-conj [] 1))

(defun hop (s: (Vec i64) n: i64 k: K) -> i64
  (if (= n 0)
      (vec-count s)
      ((. k f) (mk) (- n 1) k)))

(defun main () -> i64 (hop (mk) 1000000 (K hop)))
```

Pins types §8.4 (a named function's closure runs its all-owned body
`hop.owned`, not an adapter), §6.10 and §6.12 exception 5. `hop`'s `s`
is borrowed (its one use, `(count s)`, is a tail call whose argument
is a borrowed parameter) and `k` is owned (rule 2: an argument of a
tail call through a closure value, where every position is owned).
Under the adapter of the earlier §8.4, `hop.clo` called `hop` and
released `s` after it returned: each hop through `(. k f)` replaced
`hop`'s frame but kept the adapter's, so the compiled program held 10⁶
frames and 10⁶ vectors until the chain returned, where the interpreter
freed each vector at its own hop in constant stack (method.md rule 6).
`fibc --explain` must list `defun hop.owned` with `s owned, k owned`
and its call through `(. k f)` as `tail-call`. Count trace: `main`'s
call is ordinary (rule (e): a fresh vector at `s`'s borrowed
position), with `S0` a temporary of its step and `(K hop)` moved into
`k`. `hop` tail-calls through the value, moving `S1` = `(mk)` and `k`
and releasing nothing (`s` is borrowed). In each `hop.owned` the next
vector and `k` are moved into the next hop and `s` is released before
the jump (freed). At `n` = 0, `(count s)` is an ordinary call (rule
(e): `hop.owned`'s `s` is owned, so frame-owned, at `count`'s borrowed
position) → 1; then `s` and `k` are released (the `K` freed; its
field, the immortal closure, is not counted), and `main` releases `S0`
after the call. Clean, in constant stack in both. Companion with the
same verdict: the else branch `(if (= (rem n 2) 0) (hop s (- n 1) k)
((. k f) (mk) (- n 1) k))`, which mixes direct self tail calls with
hops through the value; once in `hop.owned`, a direct call to `hop`
goes to `hop.owned` (§8.4), so the stack stays flat there too.

## 53-mutual-tail-recursion-depth-million

```lisp
;; spec:   §4 (types §6.4, §6.10)
;; expect: accept
;; result: 1000000
;; audit:  clean
;; Two functions that call each other in tail position a million times,
;; passing a fresh box each time. Both run in constant stack, and each
;; box dies at the jump that replaces it.
(defun ping (b n) (if (= n 0) (unbox b) (pong (box (+ (unbox b) 1)) (- n 1))))
(defun pong (b n) (if (= n 0) (unbox b) (ping (box (+ (unbox b) 1)) (- n 1))))

(defun main () -> i64 (ping (box 0) 1000000))
```

Pins types §6.4 rule 3 (a tail call inside the SCC that passes a
frame-owned argument makes the position owned, so the mutual call
stays a tail call), §6.10 step 3 (the frame's owned `b`, not passed
on, is released before the jump) and §8.9 / §8.11 (`musttail`: without
it the compiled program overflows at this depth where the interpreter
does not). `fibc --explain` must show `b owned (rule 3)` in both
functions and `tail-call` at both mutual calls; each base case's
`(unbox b)` is an ordinary call (rule (e): the owned `b` is
frame-owned at `unbox`'s borrowed position). Count trace: `main`'s
tail call moves `(box 0)` into `ping`'s `b`; each hop reads the old
box, makes the next (1), moves it into the call and releases the old
one before the jump (freed); the last hop returns 1000000 and releases
its box. Clean.

## 54-tail-call-passes-stack-eligible-object

```lisp
;; spec:   §2 (scope-local objects), §4 (types §6.10, §6.11)
;; expect: accept
;; result: 2
;; audit:  clean
;; A box that would live on the stack is passed at a tail call. The
;; caller's frame is gone while the callee runs, so the box is a heap
;; object whose count moves into the call.
(defun walk (b n)
  (if (= n 0)
      (str-len (unbox b))
      (walk (Box (str-concat "x" "y")) (- n 1))))

(defun main () -> i64
  (let ((b (Box (str-concat "a" "b"))))
    (walk b 2)))
```

Pins types §6.11 (an occurrence at E6 keeps a binding off the stack)
and §6.10 step 2 (an owning binding passed at a tail call is moved).
`walk`'s `b` is owned by rule 3 (its self tail call passes a fresh
box) and `noescape`, so `main`'s `(walk b 2)`, in tail position with
`b` at an owned position, is a tail call. Were `b` scope-local, its box
would be a `STACK` object of `main`'s frame, discarded at the jump, and
`walk` would read it afterwards; `fibref` reports that as a stack
reference passed at a tail call (§6.11). `fibc --explain` must show `b
owns` without `scope-local` (reason `arg-of-tail-call`). Count trace:
`B0` (1) is moved into `walk`; each hop moves a new box into the next
and releases its own `b` before the jump (freed with its string); the
last hop reads its box, returns 2 and releases it. Clean.

## 55-inout-argument-in-tail-position-not-forwarded

```lisp
;; spec:   §5 (types §6.10 rule (b))
;; expect: accept
;; result: 2
;; audit:  clean
;; A call in tail position passes the function's own & parameter to
;; another function. That is not the forwarding case, so the call is
;; ordinary, with its own copy-in and write-back.
(defun add1 (&v) (push! &v 1))
(defun twice (&v) (do (push! &v 0) (add1 &v)))

(defun main () -> i64
  (let ((x (cell [])))
    (twice &x)
    (vec-count @x)))
```

Pins types §6.10 rule (b) and syntax §3.13: only a self tail call with
`&v` at `v`'s own position forwards the private cell. `(add1 &v)`
passes it to another function, so a new private cell acquires
`twice`'s content, `add1` runs, its write-back stores into `twice`'s
cell, and only then does `twice` return and `main`'s write-back run.
`fibc --explain` must show `call (b: & argument)` and `&v: acquire` at
`(add1 &v)`. Count trace: `x` holds `V0` (1); `twice`'s copy-in (2);
`(push! &v 0)` acquires (3), copies to `V1` = `[0]` and its write-back
releases `V0` (1); `(add1 &v)`'s copy-in acquires `V1` (2); `add1`'s
`push!` acquires (3), copies to `V2` = `[0 1]` (`V1` 2) and its
write-back releases `V1` (1); `add1`'s own write-back stores `V2` into
`twice`'s cell and frees `V1`; `main`'s stores `V2` into `x` and frees
`V0`; `(count @x)` = 2; `x`'s cell, scope-local, drops `V2` at the
exit. Clean. The mutual-recursion shape (`even-fill` and `odd-fill`
passing `&v` to each other) is accepted and, since the owner relaxed
types §6.10 rule (b), runs as tail calls in constant stack.

## 56-stack-eligible-object-captured-by-escaping-closure

```lisp
;; spec:   §2 (scope-local objects), §3.3 (types §6.5, §6.11)
;; expect: accept
;; result: 2
;; audit:  clean
;; A box that would live on the stack is captured by a closure that is
;; stored into a struct and called after the box's scope has ended. The
;; capture makes the box a heap object that the closure keeps alive.
(defstruct Thunk (f: (fn () i64)))

(defun main () -> i64
  (let ((t (let ((b (Box (str-concat "a" "b"))))
             (Thunk (fn () (str-len (unbox b)))))))
    ((. t f))))
```

Pins types §6.11 (a capture by a heap closure, E3, keeps a binding off
the stack) with §6.5 (a closure stored at E2 is escaping). Were `b`
scope-local, the closure would keep a pointer into a scope that has
ended; `fibref` reports that as a stack reference captured by a heap
closure. `fibc --explain` must show `closure escaping=yes heap=yes
captures: b (owns)` and `b owns` without `scope-local`. Count trace:
`B` (1) is retained by the closure at creation (2); the inner `let`
releases `b` (1); `t` owns the `Thunk`. `((. t f))` is a tail call
through a closure value: the head, `Derived(t)`, is retained, `t` is
released before the jump (the `Thunk` freed, its field released), and
the closure's code runs with `env` owned; it reads `b` through `unbox`
(an ordinary call) and returns 2, then releases `env`: the closure is
freed, and with it `B` and its string. Clean. `t` itself is not
scope-local: its initialiser is a `let`, not an allocation (§6.11).

## 57-closure-body-tail-call-passes-capture

```lisp
;; spec:   §3.3, §4 (types §6.4, §6.10)
;; expect: accept
;; result: 3
;; audit:  clean
;; A returned closure whose body ends in a call that borrows one of its
;; captures. The capture is held by the closure object, which the body
;; releases when it leaves, so that call must not be a tail call.
(defun mk () (let ((v [1 2 3])) (fn () (vec-count v))))

(defun main () -> i64 (+ ((mk)) 0))
```

Pins types §6.4 (a capture of a heap closure is frame-owned: in effect
`Derived(env)`) and §6.10 rule (e) (a `fn` body belongs to no SCC).
`count`'s `self` is borrowed. While §6.4 called a capture
frame-independent, `(count v)` was a tail call with nothing emitted at
the borrowed position; step 3 released `env`, the closure's count
reached zero, its drop released `v`, and `count` read a freed vector,
with no tail call in `main` at all. An interpreter that retains the
argument for `count`'s parameter before releasing the frame answers 3
(method.md rule 6). `fibc --explain` must show, in the closure's body,
`call (e: frame-owned argument at borrowed position count.self)`.
Count trace: in `mk`, `V` (1) is retained by the closure (2) and the
`let` releases `v` (1); the closure `C` (1) is returned. `((mk))`
consumes the `Owned` head as `env`; the body calls `count` → 3 and then
releases `env`: `C` is freed and releases `V` (freed). Clean.
Companions, each `accept` with a clean audit: the returned closure over
a would-be stack object that types §7 lists, `(defun mk2 () (let ((b
(Box 42))) (fn () (unbox b))))` with `(+ ((mk2)) 0)` → 42, where the
box is heap by E3 and `(unbox b)` is ordinary by the same rule; and a
`defun` callee, `(defun f (x n) (if (= n 0) (count x) (f x (- n 1))))`
with `(defun g () (let ((y (conj [] 1))) (let ((k (fn () (f y 3))))
(k))))` and `(defun main () -> i64 (g))` → 1, where `f`'s `x` stays
borrowed, the tail call `(k)` puts the closure on the heap, and `(f y
3)` in its body is ordinary.

## 58-owned-parameter-at-ordinary-call

```lisp
;; spec:   §4, §5 (types §6.4, §6.12)
;; expect: accept
;; result: 9
;; audit:  clean
;; A fresh array handed to a parameter that the callee stores. The count
;; moves into the callee, which frees the array at its own exit, in the
;; interpreter as in compiled code; the store takes a count of its own,
;; so the write through the cell copies in both.
(defun set0 (a)
  (let ((c (cell a)))
    (do (array-set! &c 0 9) (array-get @c 0))))

(defun main () -> i64 (+ (set0 (array 3 0)) 0))
```

Pins types §6.12 exception 5 (the interpreter follows the callee's count
kinds at every call) and syntax §2 (an `Owned` argument at an owned
position is not a temporary of the call step), with types §6.4 as
corrected in the fifth round (a stored owned parameter is retained, not
moved: proposed cases 72, 73). `set0`'s `a` is owned (rule 1: `(cell a)`
stores it), so `main` moves the array in. Count trace, the same in both
implementations: `A` (1) is moved into `a`; `(cell a)` retains it (2)
into the scope-local cell `c`; `array-set!` finds two counts and copies:
`A'` = `[9 0 0]` (1) goes into `c` and `A` is released (1); `(array-get
@c 0)` = 9; `c`'s inline drop frees `A'` at the `let`'s exit, and
`set0`'s exit releases `a`, freeing `A`. Under plain counting at the
call, `A` would be freed at the end of `main`'s step instead, as the
call's temporary. The verdict does not discriminate, so the case also
asserts the traces: two array allocations in both, `A` freed at `set0`'s
exit in both, and `a owned (stored)`, `arg 1: moved` and `(cell a)`:
`retain [a]` in `fibc --explain`. This entry's earlier trace, in which
`(cell a)` moved the parameter on and the write was in place, rested on
that unsound move; a write in place needs an object the cell holds
alone, as in `(let ((c (cell (array 3 0)))) (do (array-set! &c 0 9)
(array-get @c 0)))` → 9, one allocation, written in place. Companion,
the same agreement on a free point with no unique write: `(defun mk ()
(conj [] 1))`, `(defun sink (v n) (if (= n 0) 0 (sink (mk) (- n 1))))`
and `(defun main () -> i64 (+ (sink (mk) 1) 0))` → 0, where the first
vector is freed at `sink`'s jump in both, not at `main`'s step end.

## 59-named-fn-stores-its-self-name

```lisp
;; spec:   §3.3, §6 (types §6.5)
;; expect: accept
;; result: 2
;; audit:  clean
;; A named closure stores its own name into a struct that outlives the
;; frame that made it. The self-name is a use of the closure, so the
;; closure is on the heap and the struct holds a real count on it.
(defstruct Rec (k: (fn (i64) Rec) n: i64))

(defun mkrec ()
  (let ((f (fn go (n: i64) (Rec go n))))
    (let ((r (f 1))) r)))

(defun main () -> i64
  (let ((r (mkrec)))
    (. ((. r k) 2) n)))
```

Pins types §6.5 and §6.1: the self-name of a named `fn` is an alias of
its `env`, and each occurrence is a use of the literal; `(Rec go n)`
is an E2 use, kind (d). `f`'s only use is `(f 1)`, a call that is a
`let` initialiser and not a tail call, so classified by `f` alone the
literal was a stack closure: `(Rec go n)` stored a pointer into
`mkrec`'s frame into a heap `Rec`, and `main`'s `((. r k) 2)` called
through a dead closure; `fibref` reports the store of a stack
reference into a heap object on an accepted program. `fibc --explain`
must show `closure escaping=yes heap=yes reason=stored(Rec.k) via
self-name go`. Count trace: `G` (1) is `f`'s; `(f 1)` retains it for
the call (2); the body's `(Rec go n)` stores `env`, an owned parameter,
with a retain (3), and the body's exit releases `env` (2) (types §6.4:
a stored owned parameter is retained, not moved, as corrected in the
fifth round); `r` is moved out, and `mkrec`'s outer `let` releases `f`
(1, `R1`'s). In `main` the head `(. r k)` is retained (2), the body
stores `env` into `R2` with a retain (3) and releases it at its exit
(2), `(. R2 n)` is 2, and the step's temporary `R2` is released (1);
`main`'s `let` releases `r`, freeing `R1` and then `G`. Clean.

## 60-named-fn-stores-its-self-name-in-a-cell

```lisp
;; spec:   §3.3, §6 (types §6.5)
;; expect: accept
;; result: 2
;; audit:  clean
;; A named closure that captures a cell stores its own name into that
;; cell. The self-name makes the closure escaping, so its captures are
;; counted; replacing the cell's content at the end breaks the cycle.
(defun main () -> i64
  (let ((out (cell (fn (k: i64) k))))
    (do (let ((x (Box (str-concat "a" "b")))
              (g (fn self (n: i64) (do (set! out self) (+ n (str-len (unbox x)))))))
          (g 1))
        (let ((r (@out 0)))
          (do (set! out (fn (k: i64) k)) r)))))
```

Pins the rule of 59 through `set!` (E2) and captures. Classified by `g`
alone (its only use is `(g 1)`), the closure was a stack closure whose
captures of `out` and `x` were uncounted aliases: `(set! out self)` put
a pointer to it into the heap cell, the inner `let`'s exit freed the
string, and `(@out 0)` called through dead stack memory and read the
freed string through a dead capture. `fibc --explain` must show `closure
escaping=yes heap=yes reason=stored(set!) via self-name self captures:
out (owns), x (owns)`. Count trace: the cell `C` (1) holds `K0` (1); `G`
retains `C` (2) and `X` (2). `(g 1)` retains `G` for the call (2); the
body's `(set! out self)` stores `env` with a retain (3) and releases
`K0` (freed), and the body's exit releases `env` (2) (types §6.4: a
stored owned parameter is retained, not moved); the step's value 3 is
discarded; the inner `let` releases `g` (`G` 1, the cell's) and `x` (`X`
1, `G`'s). `(@out 0)` acquires `G` (2), moved in as `env`; the body
stores `env` into the cell with a retain (3), releases the old content,
`G` itself (2), and returns 2, releasing `env` at its exit (1). The last
`set!` stores `K1` and releases `G` (0): its drop releases `C` (1) and
`X` (freed, with its string). `main`'s `let` releases `out`, freeing `C`
and then `K1`. Clean. Without the last `set!` the verdict is the same
with `audit: leak-cycle`: `C` holds `G`, which captures `C` (§6).

## 61-call-result-bound-by-let-is-counted

```lisp
;; spec:   §2 (scope-local objects), §4 (types §6.11)
;; expect: accept
;; result: 1
;; audit:  clean
;; A let binds an element taken from a list. The element was not made
;; in this frame and the list still holds it, so the binding counts it
;; and releases it; only the list's own cell may live on the stack.
(defun head (xs) (first xs))

(defun main () -> i64
  (let ((l (list (box (vec-conj [] 1)))))
    (let ((h (head l)))
      (vec-count (unbox h)))))
```

Pins types §6.11 (only an initialiser that allocates in this frame — a
constructor, `cell`, `atom` — makes a binding scope-local) and syntax
§3.3. When a call result qualified, `h` was scope-local: no count
operation on the box that `first` had retained, and an inline drop at
the inner `let`'s exit that released the vector inside the box (freed)
while the list still held the box, which was then never released: a
leak holding a dangling pointer. Case 01 leaked its box under the same
reading. `fibc --explain` must show `h owns` without `scope-local`, and
`l owns scope-local` (its initialiser is the `cons` constructor, and
`head`'s `xs` is `noescape`). Count trace: `V` (1) in the box `B` (1)
in the `cons` cell `L` (on the stack); `first` retains `B` (2) for `h`;
`(unbox h)` retains `V` (2) for the call; `count` = 1; the temporary is
released (`V` 1); the inner `let` releases `h` (`B` 1); `l`'s inline
drop releases `B` (0), whose drop frees `V`. Clean.

## 62-call-result-shares-its-children

```lisp
;; spec:   §2 (scope-local objects), §4 (types §6.11)
;; expect: accept
;; result: 2
;; audit:  clean
;; A let binds a struct that a function returns out of another struct.
;; The outer struct still holds it, so the binding releases its own
;; count at scope exit and nothing more; the string is read again after.
(defstruct Name (s: str))
(defstruct Person (name: Name))

(defun name-of (p: Person) (. p name))

(defun main () -> i64
  (let ((p (Person (Name (str-concat "a" "b")))))
    (do (let ((n (name-of p))) (str-len (. n s)))
        (str-len (. (. p name) s)))))
```

Pins types §6.11 as 61 does, with a use-after-free where 61 had a leak.
`name-of` returns `Derived(p)`, retained at E1, so `n` holds the `Name`
that the `Person` also holds. When a call result qualified, `n` was
scope-local: its inline drop freed the string while the `Person` still
referred to it, the second step read it, and the `Name` leaked. `fibc
--explain` must show `n owns` and `p owns scope-local`. Count trace:
`S` (1) in `N` (1) in `P` (on the stack: `name-of`'s `p` is borrowed
and `noescape`); `name-of` retains `N` (2); the first step's value 2 is
discarded; the inner `let` releases `n` (`N` 1); the second step reads
`S` through `P` → 2; `P`'s inline drop releases `N` (0), whose drop
frees `S`. Clean. Companion, `accept`, 7, clean: `(defun main () ->
i64 (let ((y (let ((x (Box 7))) x))) (unbox y)))`, where `y`'s
initialiser is a scope exit that moves `x`'s heap box out, not an
allocation, so `y` is counted and released (when it qualified, the box
leaked).

## 63-recur-retains-binding-outside-loop

```lisp
;; spec:   §3.3, §4 (types §6.10)
;; expect: accept
;; result: 2
;; audit:  clean
;; A recur passes a binding from outside the loop as the new value of a
;; loop variable, twice. The binding stays in scope after each recur,
;; so each recur retains it rather than moving it.
(defun main () -> i64
  (let ((y (Box (str-concat "c" "d"))))
    (loop ((b (Box (str-concat "a" "b"))) (i 0))
      (if (< i 2) (recur y (+ i 1)) (str-len (unbox b))))))
```

Pins types §6.10 (a `recur` moves only a binding it exits, retains any
other, and does not release a loop variable it moved) and syntax §2,
§3.18. When `recur` consumed its arguments "as in step 2", which moves
any owning binding of the frame, the first `recur` moved `y` into `b`;
the second moved it again and released the old `b`, `y`'s own box,
freeing it; `(unbox b)` read the freed box and the loop's exit
released it a second time. `fibc --explain` must show `recur arg 1:
retain (y is outside the loop)`. Count trace: `Y` (1, heap: `y` is at
E6) and `B0` (1, heap: a loop variable). First `recur`: `Y` retained
(2), the old `b` released (`B0` freed with its string). Second: `Y`
retained (3), the old `b`, which is `Y`, released (2). `(str-len
(unbox b))` = 2; the loop's exit releases `b` (1); `y`'s scope exit
frees `Y` and its string. Clean. Companions, each `accept` with a clean
audit: in the same `let`, `(+ (loop ((b (Box (str-concat "a" "b"))) (i
0)) (if (< i 1) (recur y (+ i 1)) (str-len (unbox b)))) (str-len
(unbox y)))` → 4, which reads `y` after the loop; and `(loop ((a (Box
1)) (b (Box 2)) (i 0)) (if (< i 3) (recur b a (+ i 1)) (unbox a)))` →
2, a swap in which both loop variables are moved and neither old value
is released.

## 64-loop-variable-rebound-to-heap-objects

```lisp
;; spec:   §2 (scope-local objects) (types §6.11)
;; expect: accept
;; result: 5
;; audit:  clean
;; A loop variable starts as a fresh box and is rebound by every recur
;; to a box that a function made. Every value is counted: each recur
;; frees the previous box and the exit frees the last.
(defun grow (b) (Box (str-concat (unbox b) "x")))

(defun main () -> i64
  (loop ((b (Box (str-concat "a" "b"))) (i 0))
    (if (< i 3) (recur (grow b) (+ i 1)) (str-len (unbox b)))))
```

Pins types §6.11 (a loop variable is never scope-local). Judged by its
initialiser alone, `b` qualified (a constructor; `grow`'s `b` is
`noescape`), so the first box was a `STACK` object whose drop was due
only at the scope exit, and every later value, a heap box from `grow`,
received no count operation: the three boxes `grow` made leaked, as did
the first string, and the exit's inline drop, run on a heap box,
released its string and freed nothing. `fibc --explain` must show `b
owns` (a loop variable) without `scope-local`. Count trace: `B0` (1)
holds "ab"; each `recur` moves the new box in and releases the old one
(freed with its string); the exit reads "abxxx" → 5 and releases the
last box. Clean.

## 65-private-cells-inside-million-iteration-loop

```lisp
;; spec:   §5, method.md rule 6 (types §8.2)
;; expect: accept
;; result: 1000000
;; audit:  clean
;; An & call inside a loop of a million iterations. Each call's private
;; cell lives only for the call, so the compiled program reuses one
;; stack slot for it instead of growing the stack on every iteration.
(defun bump (&v) (set! v (+ @v 1)))

(defun main () -> i64
  (let ((c (cell 0)))
    (do (dotimes (i 1000000) (bump &c))
        @c)))
```

Pins types §8.2 (each `STACK` allocation site has one `alloca` in the
function's entry block, reused by every execution of the site), §8.6
and §8.10. `dotimes` is `loop`/`recur`, which §8.9 lowers to a branch
inside `main`; an `alloca` written in the loop block is a dynamic
allocation reclaimed only at return, so 10⁶ private cells of three
words took 24 MB of stack and overflowed where `fibref`, which frees
each private cell at its write-back, returned 1000000 with a clean
audit (method.md rule 6). The same holds for a scope-local object in a
loop body; companion, `accept`, 2000000, clean:

```lisp
(defun main () -> i64
  (let ((n (cell 0)))
    (do (dotimes (i 1000000)
          (set! n (+ @n (str-len (unbox (Box (str-concat "a" "b")))))))
        @n)))
```

where each iteration's `Box` is a scope-local temporary (its one
occurrence is `unbox`'s borrowed, `noescape` parameter) whose string,
a heap object, is released by its inline drop.

## 66-cell-read-is-an-acquire

```lisp
;; spec:   §5, §6 (types §6.3, §6.7)
;; expect: accept
;; result: 6
;; audit:  clean
;; A loop iterates the vector read from a cell while its body pushes
;; onto that cell. The read holds the only count beside the cell's, so
;; each push copies and the loop sees the original three elements.
(defun main () -> i64
  (let ((c (cell [1 2 3])))
    (for-each @c (fn (x) (push! &c x)))
    (vec-count @c)))
```

Pins types §6.3 and §6.7 (a cell read is an acquire, never elided),
which case 08 no longer pins: since D1, case 08's `main` keeps the
vector in its own cell for the whole call, so 08 passes with the
acquire elided (types §7). Here the iteration's count is the only one
beside the cell's. Count trace: `c` holds `V0` (1); `@c` acquires (2)
for the `for-each` call; `x` = 1: `push!`'s copy-in acquires (3),
`push!` copies to `V1` (`V0` 2), and the write-back into `c` releases
`V0` (1, the iteration's); `x` = 2 and 3 free `V1` and `V2` the same
way; `for-each` returns and the temporary `V0` is released (freed);
`(count @c)` = 6; `c`'s inline drop releases `V3`. Clean. With the
acquire elided, the first write-back frees `V0` while `for-each` is
still reading it.

## 67-two-names-for-one-cell

```lisp
;; spec:   §5
;; expect: accept
;; result: 2
;; audit:  clean
;; Two names for one cell passed to two & parameters of one call. The
;; names differ, so the distinct-variables check passes; every copy-in
;; acquires, so memory stays safe, but the second write-back overwrites
;; the first and the push of 1 is lost.
(defun bar (&a &b) (append &a 1) (append &b 2))

(defun main () -> i64
  (let ((x (cell [3 4])))
    (let ((y x))
      (bar &x &y)
      (nth @x 2))))
```

The header gives the verdict under the decided rules. The owner has
chosen option (A) for two names reaching one cell (types §10): the
check stays on names, this program is accepted, and the texts now say
the later write-back wins. Pins that the check of syntax §3.13
rule 1 is on names (types §6.5), that the write-backs run in parameter
order, and that the answer depends on that order: reversed, it is 1.
Count trace: `C` holds `V0` (1); the two copy-ins acquire (3); `(append
&a 1)` copies to `V1` = `[3 4 1]` and its write-back releases `V0`
(2); `(append &b 2)` copies to `V2` = `[3 4 2]` (`V0` 1); `a`'s
write-back stores `V1` into `C` and frees `V0`; `b`'s stores `V2` and
frees `V1`; `(nth @x 2)` = 2. Companions with the same property, each
`accept` with a clean audit under the decided rules, as `main`'s body:
with `(defun set0 (&a v) (push! &a v))`, `(let ((x (cell []))) (set0
&x (do (set0 &x 9) 1)) (count @x))` → 1, whose outer write-back
discards the inner push of 9; and with `(defun g (&v c) (do (set! c
[9]) (count @v)))`, `(let ((x (cell [1 2]))) (+ (g &x x) (count @x)))`
→ 4, where the callee writes the caller's cell during the call and
the write-back undoes it.

## 68-owned-parameter-dropped-at-tail-call

```lisp
;; spec:   §4 (types §6.10)
;; expect: accept
;; result: 1
;; audit:  clean
;; An owned accumulator that a self tail call replaces with a fresh
;; vector without passing it on. The frame releases it before the jump;
;; the base case returns it through the if.
(defun last-of (acc n)
  (if (= n 0) acc (last-of (vec-conj [] n) (- n 1))))

(defun main () -> i64 (vec-count (last-of (vec-conj [] 0) 1000)))
```

Pins types §6.10 step 3 (the frame's owned parameters that step 2 did
not move are released before the jump; syntax §2) and the base-path
accounting of case 07 (§6.3: the join with the tail-call branch
retains `acc`, and the parameter is released at the exit). `acc` is
owned by rule 1 (returned) and rule 3 (the self tail call passes a
fresh vector at its position); unlike case 07's, it does not reach the
call even through `conj`. `fibc --explain` must show `acc: released
before the jump` at the tail call and, on the base path, `retain [acc]
(join); release [acc] (exit)`. Count trace: each hop makes `(conj []
n)` (1), moves it into the call and releases the old `acc` (freed); at
`n` = 0 the join retains the last vector (2), E1 moves that count to
`main`, and the exit releases the parameter (1); `count` = 1 and
`main` releases the temporary. Clean. Without the release in step 3,
1000 vectors leak.

## 69-returned-parameter-is-owned

```lisp
;; spec:   §4 (types §6.4)
;; expect: accept
;; result: 4
;; audit:  clean
;; A function that returns its parameter. The parameter is owned: the
;; caller hands a count over and the callee moves it out as the result,
;; so the caller's two bindings hold one count each.
(defun id (x) x)

(defun main () -> i64
  (let ((s (str-concat "a" "b")))
    (let ((t (id s)))
      (+ (str-len t) (str-len s)))))
```

Pins ownership.md §2 and §4 as amended by D5, and types §6.4 rule 1: a
returned parameter is owned, not a borrow that the callee retains.
`fibc --explain` must show `x owned (returned) escapes=yes`, `arg 1:
retain` at `(id s)`, and no operation in `id`'s body (its value is the
owned parameter, moved out: §6.3, the E1 row). Count trace: `S` (1);
the call retains it for `x` (2); `id` moves it out; `t` owns it; 2 + 2
= 4; the inner `let` releases `t` (1) and the outer releases `s`
(freed). Clean. Case 04's `pick` is the join shape of the same rule:
its `x` is owned by the join retain, `main` retains `s` at each call,
and `pick` releases `x` at its exit.

---

Cases 70 onward came out of the fifth round of confirmed findings,
again on the sections that the amendments D1–D7 touched. 70 and 71 are
two spellings of one finding (an `async` body ending in a call), 72 and
73 two of another (an owned parameter that is stored), and 74 to 78 one
finding each. The round's finding on stale entries is answered by the
corrections to 21, 37 and 58 above; the traces of 59 and 60, whose
closures store their own `env`, were corrected with them, since a
stored owned parameter is now retained rather than moved (types §6.4).

## 70-async-body-ends-in-owned-call

```lisp
;; spec:   §8, §4 (types §6.9, §6.10)
;; expect: accept
;; result: 5
;; audit:  clean
;; An async body whose last step calls a function that returns its
;; owned argument. The task stores that value as its result and only
;; then completes, so the call is an ordinary call, not a tail call out
;; of the task.
(defun pass (b) b)

(defun main () -> i64
  (let ((b (Box 5)))
    (unbox (block-on (async (pass b))))))
```

Pins types §6.10 rule (f) with §6.9 and §8.8 (a call in tail position
of an `async` body is never a tail call: the task's completion —
share-mark, store `result`, publish `state := done`, wake the waiters —
runs after it) and syntax §2, §3.14. Read as a tail call, `(pass b)`
left `resume` by a jump: `pass`'s `b` is owned (§6.4 rule 1: returned),
so the jump moved a retained count of the capture into it, and `pass`
handed that count back to the executor instead of to the task's
`result`; `state` never became `done`, so `block-on` never returned,
or, with the body read as a closure body releasing its `env` before
the jump, the task was freed under its driver. `fibc --explain` must
show `call (f: async body)` at `(pass b)` in the `async` body, and `b
owned (returned)` for `pass`. Count trace: `B` (1) is `b`'s, a heap box
since the `async` captures it (E3); creating the task retains it (2);
`block-on` drives the task, in which `(pass b)` retains the capture into
`pass`'s owned `b` (3) and `pass` moves it out as its result; the task
stores that count in `result` and completes; `join` retains the result
for the caller (4). After the call the task, a temporary of the call
step, is released (freed): its drop releases the capture (3) and the
result (2). `unbox` reads 5 and the step's temporary is released (1);
`main`'s `let` releases `b` (freed). Clean. Companion with no object
involved, `accept`, 5, clean: `(defun len2 (s) (str-len s))` with
`(defun main () -> i64 (block-on (async (len2 "hello"))))`; the
literal is immortal, so rule (e) never applied, and without rule (f)
`resume` jumped into `len2`, whose `i64` became `resume`'s poll status.

## 71-async-body-ends-in-closure-call

```lisp
;; spec:   §8, §6 (types §6.9, §6.10)
;; expect: accept
;; result: 42
;; audit:  clean
;; An async body whose last step calls a captured closure. The call
;; does not replace the task: the task stores the closure's result when
;; the call returns, and frees the closure with itself.
(defun run-later (k) (async (k)))

(defun main () -> i64
  (block-on (run-later (fn () 42))))
```

Pins types §6.10 rule (f) through a closure value, where rule (e)
never applies and nothing else made the call ordinary. Read as a tail
call, `(k)` was an `indirect-tailcall` out of `resume` that returned 42
to the executor as its poll status, with the task's `state` still
pending and its `driver` claimed, so `block-on` spun or ran the body
again, retaining and releasing `k` on every pass, while the
interpreter's coroutine returned 42 as the task's result (method.md
rule 6). `fibc --explain` must show `call (f: async body)` at `(k)`.
Count trace: the literal `K` is an argument of `run-later`'s `k`,
which escapes (the task captures it, E3), so `K` is escaping and a heap
closure (1), moved into `run-later`'s owned `k`; the task retains it
(2); `run-later` releases `k` at its exit (1) and returns the task.
`block-on` drives it: `(k)` hands `K` over with a retain (2), its code
returns 42 and releases `env` (1), and the task stores 42 and
completes. After the call the task is released (freed), and its drop
releases `K` (freed). Clean. Companion, `accept`, 3, clean: `(defun f
(s) (str-len s))` with `(defun main () -> i64 (block-on (async (f
"abc"))))`: the literal is frame-independent at `f`'s borrowed
position, so only rule (f) keeps that call ordinary.

## 72-owned-parameter-stored-then-read

```lisp
;; spec:   §4 (types §6.3, §6.4)
;; expect: accept
;; result: 5
;; audit:  clean
;; An owned parameter is stored into a cell, the cell is overwritten,
;; and the parameter is read again. The store took a count of its own,
;; so the parameter is still alive, and the callee releases it at exit.
(defun stash (c b)
  (do (set! c b)
      (set! c (Box 0))
      (unbox b)))

(defun main () -> i64
  (let ((c (cell (Box 9))))
    (stash c (Box 5))))
```

Pins types §6.4 as corrected in the fifth round (an owned parameter is
moved out only as the function's value or at a tail call; stored,
captured or spawned, it is retained and still released at the exit),
§8.9 and §8.10, with §6.3 (E2 retains a `Borrowed` operand). `b` is
owned by rule 1 (E2). Under the text that let a store move an owned
parameter ("its count travels"), `(set! c b)` stored `b`'s only count,
the second `set!` freed the box and `(unbox b)` read freed memory; with
the store retaining and the exit releasing nothing, the other reading
of the earlier §8.9, the box leaked. `fibc --explain` must show `b
owned (stored)`, `retain [b]` at `(set! c b)` and `release [b] (exit)`.
Count trace: `main`'s call is ordinary (rule (e): its `c` is
frame-owned at `stash`'s borrowed `c`), so `c` is scope-local, its one
use being that `noescape` position; its cell holds `B9` (1). `B5` (1)
is moved into `b`. `(set! c b)` retains it (2), stores it and releases
`B9` (freed); `(set! c (Box 0))` stores `B0` and releases `B5` (1);
`(unbox b)` = 5, an ordinary call (rule (e): the owned `b` is
frame-owned at `unbox`'s borrowed position); `stash`'s exit releases `b`
(freed). `c`'s inline drop releases `B0` (freed). Clean. Companion, the
double store, `accept`, 6, clean:

```lisp
(defstruct Two (x: (Box i64) y: (Box i64)))
(defun dup (b) (let ((t (Two b b))) t))
(defun main () -> i64
  (let ((t (dup (Box 3))))
    (+ (unbox (. t x)) (unbox (. t y)))))
```

where each field store retains (`B` 3), `dup`'s exit releases `b` (2)
and the `Two`'s drop releases both fields (freed); with a moving store,
one count stood behind two fields and the drop took it below zero.

## 73-owned-parameter-in-cell-read-after

```lisp
;; spec:   §4, §5 (types §6.4, §6.6)
;; expect: accept
;; result: 3
;; audit:  clean
;; An owned array parameter is put into a cell whose content is then
;; replaced, and the parameter is read afterwards. The cell took a count
;; of its own, so the array stays alive until the callee's exit.
(defun f (a)
  (let ((c (cell a)))
    (do (set! c (array 1 7)) (array-len a))))

(defun main () -> i64 (f (array 3 0)))
```

Pins the same correction of types §6.4 as 72, on the shape that
proposed case 58's earlier trace relied on. Under the moving store
`(cell a)` handed the cell `a`'s only count, the `set!` freed the array
and `(array-len a)` read freed memory. Count trace: `main`'s `(f (array
3 0))` is a tail call (a fresh array at `f`'s owned `a`), moving `A`
(1) into `a`; `(cell a)` retains it (2) into the scope-local cell `c`;
`(set! c (array 1 7))` stores `A7` and releases `A` (1); `(array-len
a)` = 3, an ordinary call (rule (e): the owned `a` is frame-owned at
`array-len`'s borrowed position); `c`'s inline drop frees `A7`, and
`f`'s exit releases `a`, freeing `A`. Clean. Companion, `accept`, 0,
clean, on the count a unique write sees: `(defun peek0 (a) (let ((c
(cell a))) (do (array-set! &c 0 9) (array-get a 0))))` with `(defun
main () -> i64 (peek0 (array 3 0)))`: the cell's retain leaves two
counts, so `array-set!` copies and `a` still reads 0, and `peek0`
frees the copy at the `let`'s exit and `a` at its own; under the
moving store the write was in place and the immutable parameter read 9.

## 74-kinds-decided-before-tail-calls

```lisp
;; spec:   §3.3, §4 (types §6.4, §6.5, §6.10)
;; expect: accept
;; result: 2
;; audit:  clean
;; A closure and the parameter it captures go to a function in tail
;; position. The call is a tail site, so the closure is on the heap; its
;; capture makes the parameter owned; and an owned parameter at a
;; borrowed position makes the call an ordinary call.
(defun hold (k :borrow s n)
  (if (= n 0)
      (+ (k) (str-len s))
      (hold (fn () 0) s (- n 1))))

(defun f (p) (hold (fn () (str-len p)) p 3))

(defun main () -> i64 (f (str-concat "a" "b")))
```

Pins types §6.10 (tail sites: the order of the decisions), §6.4 (rule 2
and the fixpoint read tail sites; rule (e) is applied after the
fixpoint) and §6.5 (a closure at a tail site is heap, admitted or not).
`hold`'s `k` is owned (rule 3) and `:borrow`, its `s` borrowed. Decided
together with admission, the rules had no consistent answer for `f`:
admitted, the call made the literal heap, its capture made `p` owned
(rule 1), and `p`, frame-owned at the borrowed `s`, made the call
ordinary; ordinary, the call left the literal stack and `p` borrowed,
which admitted it. The order an implementation was likely to pick, the
literal stack and the call admitted, discarded `f`'s frame, which held
the literal, while `hold` still held it as `k`: `hold`'s release of `k`
before its first jump read the literal's header from memory that
`hold`'s own frame had replaced, and with `n` = 0 its `(k)` would have
called through it. `fibc --explain` must show, for
`f`, `p owned (rule 1: captured by a heap closure) escapes=yes`,
`closure escaping=no heap=yes reason=arg-of-tail-site captures: p
(owns)`, and `call (e: frame-owned argument p at borrowed position
hold.s)` at `(hold ..)`. Count trace: `main`'s call is a tail call (a
fresh string at `f`'s owned `p`), moving `S` (1) into `p`; the literal
`K` retains `p` (2) and is moved into `hold`'s owned `k`, with nothing
emitted for `s`; `hold`'s first self tail call moves a fresh `(fn ()
0)` into `k` and releases `K` before the jump (freed, and with it its
capture: `S` 1), and the next two replace their closures the same way;
at `n` = 0, `(k)` is 0 and `(str-len s)` 2, and `hold` releases its `k`
and returns 2 to `f`, whose exit releases `p` (freed). Clean.
Companion, `accept`, 2, clean, through rule 2 with no closure:

```lisp
(defun h (a b n) (if (= n (str-len b)) a (h a b (+ n 1))))
(defun g (p) (h p p 0))
(defun main () -> i64 (str-len (g (str-concat "a" "b"))))
```

where `h`'s `a` is owned (its base path returns it through a join) and
its `b` borrowed; `(h p p 0)` is a tail site, so `p`, at the owned `a`,
is owned by rule 2, and then frame-owned at the borrowed `b`, so the
call is ordinary. `fibc --explain` must show `p owned (rule 2: argument
of a tail site at h.a)` and `call (e: frame-owned argument p at
borrowed position h.b)`. Count trace: `S` (1) is moved into `g`'s `p`;
the call retains it into `h`'s `a` (2); `h`'s self tail calls move `a`
on; the base path's join retains `a` (3), moves that count out and
releases the parameter (2); `g`'s exit releases `p` (1); `main` releases
the result after `str-len` (freed).

## 75-loop-returns-its-parameter

```lisp
;; spec:   §3.1, §4 (types §6.4)
;; expect: accept
;; result: 2
;; audit:  clean
;; A function returns its parameter through a loop variable on the path
;; that never recurs. The parameter escapes and is owned as if it were
;; returned directly, so the caller's object is not a stack object.
(defstruct Named (s: str))

(defun last-or (default xs)
  (loop ((best default) (i 0))
    (if (< i (vec-count xs)) (recur (nth xs i) (+ i 1)) best)))

(defun main () -> i64
  (let ((r (last-or (Named (str-concat "a" "b")) [])))
    (str-len (. r s))))
```

Pins types §6.4 (loop variables: a value consumed into a loop variable
is followed through it by rule 1 and the escape summary) and, through
it, §6.11 and use (b) of §6.5. Without the rule, `best`'s initialiser
was a retain that no summary looked past and the loop's value left as
`Owned`, so `default` was `noescape` although `last-or` returns it: the
`Named` temporary, passed to a `noescape` parameter of an ordinary
call, was scope-local, `last-or` returned a pointer to it, the end of
the argument list ran its inline drop (freeing the string), and `(. r
s)` read the freed string. `fibc --explain` must show `default owned
(rule 1, through loop variable best) escapes=yes` and the `Named`
temporary without `scope-local`. Count trace: `N` (1, holding "ab") is
moved into `default`; `best` retains it (2); `(count xs)` is 0, so the
loop's value is `best`, moved out, and `last-or`'s exit releases
`default` (1); `r` owns `N`; `(str-len (. r s))` = 2; `main`'s `let`
releases `r` (freed, with its string). Clean. Companions, on use (b) of
§6.5 through the same rule:

```lisp
(defun keep (f) (loop ((g f) (i 0)) (if (< i 1) (recur f (+ i 1)) g)))

(defun mk (s) (let ((k (keep (fn () (str-len s))))) k))
(defun main () -> i64
  (let ((k (let ((s (str-concat "ab" "c"))) (mk s))))
    (k)))                                        ; accept, 3, clean

(defun leak (&v) (keep (fn () (set! v (vec-conj [] 9)))))
(defun main () -> i64
  (let ((c (cell (vec-conj [] 1))))
    (let ((k (leak &c)))
      (do (k) (vec-count @c)))))                     ; reject: & parameter captured by escaping closure
```

`keep`'s `f` escapes (the initialiser and the `recur` retain it into
`g`, and `g` is the loop's value), so a literal passed to it is
escaping (use (d)). In `mk` it is a heap closure that retains `s`,
where before it was a stack closure of `mk`'s frame that `keep` handed
back to `main`, which called it through a dead frame. In `leak` it
captures the `&` parameter `v` and escapes, which is case 18's error,
`& parameter captured by escaping closure: v in leak`. The finding
that produced this entry proposed `accept` for `leak`; under the rule
its closure outlives the call and would write a private cell that no
longer exists, which is what the error is for.

## 76-loop-initialised-from-scope-local-binding

```lisp
;; spec:   §2 (scope-local objects) (types §6.11)
;; expect: accept
;; result: 2
;; audit:  clean
;; A box made in an inner scope initialises a loop variable, which
;; stores it into a cell that outlives the scope. The loop variable is
;; an owning binding that the scope does not bound, so the box is a
;; heap object and the cell keeps it alive.
(defun main () -> i64
  (let ((out (cell (Box (str-concat "x" "y")))))
    (do (let ((b (Box (str-concat "a" "b"))))
          (loop ((x b)) (set! out x)))
        (str-len (unbox @out)))))
```

Pins types §6.11 (a `loop` initialiser disqualifies a binding) and
§8.2 (no loop variable ever holds a `STACK` object, which the reuse of
a site's `alloca` relies on). While the initialiser did not
disqualify it, `b` was scope-local: `x` held its `STACK` box with no
count, `(set! out x)` stored it into the heap cell (its retain a
no-op), the inner `let`'s inline drop freed "ab", and `(unbox @out)`
read it. `fibc --explain` must show `b owns` without `scope-local`
(reason: initialises loop variable `x`). Count trace: `out`'s cell,
scope-local (a `set!` target and a read), holds `X` (1, a heap box
stored at E2). `B` (1) holds "ab"; `x` retains it (2); `(set! out x)`
retains it (3), stores it and releases `X` (freed, with "xy"); the loop
ends without `recur` and releases `x` (2); the inner `let` releases `b`
(1). `@out` acquires `B` (2), `unbox` retains "ab" for its result,
`str-len` is 2, and the step's temporaries are released (`B` 1);
`out`'s inline drop releases `B` (freed, with "ab"). Clean. Companion,
`accept`, 2, clean: `(defun main () -> i64 (let ((r (let ((b (Box
(str-concat "a" "b")))) (loop ((x b)) x)))) (str-len (unbox r))))`,
where the loop moves `x` out as its value; with `b` scope-local, `r`
pointed into the inner `let`'s ended scope.

## 77-dyn-content-in-private-cell

```lisp
;; spec:   §5, method.md rule 6 (types §8.1, §8.6)
;; expect: accept
;; result: 9
;; audit:  clean
;; A cell holding a dyn value is passed in-out. The private cell holds
;; the two-word dyn, so its stack slot is four words, not three.
(defprotocol Area (area (self) -> i64))
(defstruct Sq (n: i64))
(impl Area Sq (area (self) (* (. self n) (. self n))))

(defun reset-shape (&s) (set! s (dyn Area (Sq 3))))

(defun main () -> i64
  (let ((c (cell (dyn Area (Sq 2)))))
    (do (reset-shape &c) (area @c))))
```

Pins types §8.6 (a private `&` cell is sized by its content: two words
of header plus one per content word, four for a `(dyn P)`) with §8.1
and §8.10. With the fixed three-word slot of the earlier §8.6, the
copy-in and the `set!` each wrote the vtable word eight bytes past the
`alloca`, into a neighbouring slot of `main`'s entry block (another
`STACK` object's header or field, or a loop variable's slot), while
`fibref`, which has no layout, answered 9 cleanly: only a compiled run
under the audit, or with a checked stack, catches it, and the case is
there so that one does. Count trace: `c`'s cell (scope-local, four
words) holds `{Sq2, vt}` (`Sq2` 1); the copy-in acquires `Sq2` (2) into
`reset-shape`'s private cell, also four words; the `set!` stores
`{Sq3, vt}` (`Sq3` 1) and releases `Sq2` (1); the write-back moves
`{Sq3, vt}` into `c` and releases `Sq2` (freed). `(area @c)`: the read
acquires `Sq3` (2), the call dispatches through the vtable and returns
9, and the temporary is released (1); `c`'s inline drop releases `Sq3`
(freed). Clean.

## 78-inout-read-outlives-set

```lisp
;; spec:   §5, §6 (types §6.3, §6.6, §6.7)
;; expect: accept
;; result: 4
;; audit:  clean
;; Inside an & function, a value read from the parameter is still in
;; use when the parameter's content is replaced. The read holds its own
;; count, so the vector it refers to outlives the replacement.
(defun swap-out (&v) -> i64
  (do (push! &v 0)
      (let ((snap @v))
        (do (set! v [])
            (vec-count snap)))))

(defun main () -> i64
  (let ((c (cell [1 2 3])))
    (+ (swap-out &c) (vec-count @c))))
```

Pins syntax §3.13's "a value read from `v` and still in use always holds
a count" (types §6.3, §6.7: a cell read is an acquire, never elided)
inside an `&` function, which case 08 cannot pin: since D1 the caller's
cell keeps the vector it passed alive for the whole call, and here the
vector read is one the callee made. Count trace: `c` holds `V0` (1); the
copy-in acquires it into the private cell `P` (2); `(push! &v 0)`
acquires again into its own private cell (3), copies to `V1` = `[1 2 3
0]` (`V0` 2), and its write-back into `P` releases `V0` (1), so `P`
holds `V1` alone (1); `snap` acquires `V1` (2); `(set! v [])` stores the
empty vector and releases `V1` (1, `snap`'s); `(count snap)` = 4; the
`let` releases `snap` (`V1` freed). `swap-out`'s write-back stores the
empty vector into `c` and releases `V0` (freed); `(count @c)` = 0; `c`'s
inline drop releases the empty vector; the result is 4 + 0 = 4. Clean.
With the `@v` acquire elided, `snap` held `V1` with no count, `(set! v
[])` freed it and `count` read freed memory, since nothing but `P` held
`V1`.

## 79-scope-local-object-dropped-at-its-scope-exit

```lisp
;; spec:   §2 (scope-local objects), §5 (types §6.11, §6.12)
;; expect: accept
;; result: 9
;; audit:  clean
;; A struct made in an inner let holds a second count on the array in a
;; cell. The struct is scope-local, so it is dropped when the inner let
;; exits, and the array is unique again when it is updated: one array
;; is allocated, written in place and freed once.
(defstruct Holder (items: (Array i64)))

(defun main () -> i64
  (let ((c (cell (array 3 0))))
    (do (let ((h (Holder @c))) (array-get (. h items) 0))
        (array-set! &c 0 9)
        (array-get @c 0))))
```

Pins types §6.11 (a scope-local object's drop runs at the exit of its
binding's scope, in its place among that scope's releases, in both
implementations) and §6.12 exception 4 (`fibref` ends a stack object at
that scope's exit, never at the function's), with the stack-reference
audit error for a read after the scope has ended. Under the earlier
text ("a frame's exit frees its objects"), `fibref` kept `h` until
`main` returned: the array was still at 2 when `array-set!` ran, so the
write copied — a second array stored into `c`, the first released to 1
— and `main`'s exit freed the first through `h`'s drop and the second
through `c`'s. Same result, 9, but two array allocations and a free at
another point, where the compiled program allocates one array, writes
it in place and frees it at the outer `let`'s exit (method.md rule 6).
`fibc --explain` must show `h owns scope-local` and `c owns
scope-local`, and both traces must show one array allocation, the end
of `h`'s scope at the inner `let`'s exit, a unique write at
`array-set!` and one free of the array, at the outer `let`'s exit.
Count trace: `(array 3 0)` makes `A` (1), stored into `c`'s cell (E2,
moved; the cell is scope-local, its uses being `@c` and `&c`); `@c`
acquires `A` (2) and the `Holder` constructor stores that count (E2,
moved) in the scope-local `h`; `(array-get (. h items) 0)` reads 0
through a `Derived(h)` argument at a borrowed position (no count
operation in compiled code; `fibref`'s parameter binding retains and
releases around the call, types §6.12 exception 5); the inner `let`
exits and `h`'s drop releases `A` (1). `(array-set! &c 0 9)` takes no
copy-in; `A` is unique (count 1, no flag), so element 0 := 9 in place.
`(array-get @c 0)`, in tail position, is an ordinary call (rule (e): a
fresh argument at a borrowed position): `@c` acquires `A` (2), the call
returns 9 and the temporary is released (1); the outer `let` exits and
`c`'s drop releases `A` (freed). Clean. Companions, each `accept` and
clean:

```lisp
(defstruct Holder (items: (Array i64)))

(defun main () -> i64                            ; result 1
  (let ((w (let ((a (array 3 0)))
             (let ((h (Holder a)))
               (do (array-len (. h items)) (weak a))))))
    (if (nil? @w) 1 0)))
```

where the difference shows in the result: `a`, a call result, is
counted, and the scope-local `h` holds a second count on its array (E2
retains `a`: 2); the inner `let`'s exit drops `h` (1), the middle
`let`'s exit releases `a` (freed, and the weak box cleared), and `@w`
is `nil`. With `h` ended at `main`'s exit the array was still alive at
`@w`, and the result was 0.

```lisp
(defstruct Holder (items: (Array i64)))

(defun main () -> i64                            ; result 499500
  (loop ((i 0) (acc 0))
    (let ((h (Holder (array 1000 i))))
      (if (< i 1000)
          (recur (+ i 1) (+ acc (array-get (. h items) 0)))
          acc))))
```

where each iteration's `h` is dropped by the scope exit that its
`recur` runs after evaluating the arguments that read it (types
§6.10), freeing its array there, and the compiled program reuses `h`'s
one slot (§8.2). Ended at `main`'s exit, `fibref`'s 1001 holders and
their arrays, some 8 MB, stayed live until `main` returned: the same
result and a clean audit, but another trace (proposed case 65's
companion is the same question for an implicit temporary).

```lisp
(defstruct Holder (items: (Array i64)))

(defun main () -> i64                            ; result 7
  (block-on (async (let ((h (Holder (array 3 7))))
                     (await (yield))
                     (array-get (. h items) 0)))))
```

where `h` is scope-local in the `async` body and lives in a slot of
the task object (§8.2): its scope stays open across the `await` and
ends when the body leaves the `let`, after the task has been resumed,
and its drop then frees the array. A `fibref` that took one call of
`resume` for the scope ended `h` when the task suspended and reported
the read after the `await` as a read of a stack object whose scope had
ended: an audit failure on a correct program.

## 80-unique-write-closes-cycle-through-cell

```lisp
;; spec:   §6, §5 (types §2.13, §6.6, §6.7)
;; expect: accept
;; result: 0
;; audit:  leak-cycle
;; A struct that only a cell holds is updated in place so that it
;; points back at that cell. The cycle passes through the cell, so the
;; audit reports it as a cycle leak, not as a cycle of immutable
;; objects.
(defstruct K (back: (Option (Cell K))))

(defun main () -> i64
  (let ((c (cell (K nil))))
    (set-field! &c back (some c))
    0))
```

Pins types §6.6 as corrected (a unique write closes a cycle only
through the cell that holds the object it writes), the `ImmutableCycle`
argument of §6.7 that rests on it, and the form rule of `set-field!`
(types §2.13; syntax §3.13, §4.3): `back` is a field name, not a
variable, and the form has three operands where the earlier typing,
`(fn ((& S) F) unit)`, had two parameters. The earlier sentence ("an
object with count 1 held by the writer's place is reachable from no
other object") was false here: the struct is reachable from `c`
throughout, and the write makes `c` reachable from the struct. Read as
ownership.md states it, the write is `(set! c (K (some c)))`, a new
version referring to the cell stored back into the cell, which is how
§6 says every cycle is made; the unique write only reuses the old
version's memory (§5). `fibc --explain` must show `c owns` without
`scope-local` (reason: stored by `(some c)`, E2). Count trace: `(K
nil)` makes `K0` (1), stored into the cell `C` (E2, moved); `c` is not
scope-local, since `(some c)` stores it (E2), so `C` is a heap cell
(1). `(set-field! &c back (some c))`: `&c` takes no copy-in; `(some
c)` retains `C` (2) and allocates nothing (§8.1); `K0` is unique
(count 1, no flag), so its field `back` := `(some C)` in place, the
`Option` moved into it. The `let` exits and releases `c` (`C` 1). At
exit `C` (1, held by `K0`) and `K0` (1, held by `C`) are live, each
count equal to the live references naming it, and `C`, a cell, lies on
the cycle `C → K0 → C`: `leak-cycle`, and nothing else. Interpreter and
compiler agree step by step: nothing is scope-local, `0` is `main`'s
value, and no parameter is involved. The call passes `c` beside `&c`,
the shape the owner decided with option (A) (types §10); a primitive
has no copy-in and no body, so here the call has one meaning and the
verdict is the decided rule's; option (B), not chosen, would have rejected
this program. Companions:

```lisp
(defstruct P (x: i64 y: i64))

(defun main () -> i64                            ; accept, 42, clean
  (let ((c (cell (P 1 2)))
        (x 40))
    (set-field! &c x (+ x 2))
    (. @c x)))
```

where the second operand names `P`'s field `x`, not the local `x`,
which the third operand reads: `c` is scope-local, the `P` in it (1, a
heap object stored at E2) is unique, so its field is written in place;
`(. @c x)` reads 42 through an acquired temporary (2); the temporary
and, at the `let`'s exit, `c`'s drop release it (freed). Resolved as a
variable, the operand would be the integer 40 and name no field.

```lisp
(defstruct P (x: i64 y: i64))

(defun main () -> i64                            ; reject: P has no field z
  (let ((c (cell (P 1 2))))
    (set-field! &c z 3)
    (. @c x)))

(defun main () -> i64                            ; reject: function with & parameters is not a value
  (let ((f array-set!)) 0))
```

(two programs), pinning the failure of the form's `HasField` constraint
(types §2.13, §3.3) and that neither `&` primitive is a value (types
§1.4, §2.13).
