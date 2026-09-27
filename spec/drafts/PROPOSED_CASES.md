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
