# 0022. Exclusive windows are over contiguous buffers, and the checker refuses closures over them

Status: accepted
Date: 2026-10-06
Source: docs/design/decisions-2026-10-04.md, "Exclusive views" (decisions 1 to 6); docs/design/exclusive-views.md 11; compiler/own/views.fib; lib/fib/view.fib,
lib/fib/view/tiles.fib; cases/ownership 336 to 367; scripts/mutant-views.sh.

## Context

Numeric code needs to write into a buffer in place, from several tasks at once, without the persistent `Vec`'s copy-on-write and without a
global interpreter lock: "No GIL is the language's main advantage over Python". The owner delegated the design and the lead decided: a
sigiled core that the checker sees every write through, a sigil-free macro layer (`vget`, `vset!`, `fill!`) over it, windows over
contiguous buffers only (`Array`, `MArray`, `Tensor`; not `Vec`), no closure over a view cell in the first version (parallel work goes
through a tile combinator that hands each task its own window), read-only lends, and disjoint windows that are safe across tasks.

## Decision

1. No type, function or implementation of `lib/fib/view.fib` or `lib/fib/view/` is over a `Vec`: windows are over arrays and
   tensors. (Macros match the `Vec` of the *macro-expander's* syntax tree; those are not windows and are not counted.)
2. The sigil-free layer exists: `vget`, `vset!`, `fill!`, `copy-into!`, `vlen` and `with-view` in `fib.view`, `with-tiles` in `fib.view.tiles`,
   and the splitting primitives `array-window!`, `array-window`, `array-split!`.
3. A closure that mentions a view cell is refused: the checker (`compiler/own/views.fib`) has the error
   `view cell V cannot be captured by a closure`, and case 344 pins its text as its verdict, with its sibling 347 and the mutant script
   that breaks the rule and requires a case to fail.

## Consequences

- A function that takes a `Vec` and returns a window cannot be added to the view modules; `Vec` data is copied into an array first, as the design says.
- The tiles combinator is the only supported way to run tasks over windows of one buffer.
- Loosening the closure rule (VC2) is a change of the checker, of case 344 and of this ADR together.

## Governance

```fibber fitness
(defun view-files () -> (Vec str) ["lib/fib/view.fib" "lib/fib/view/**.fib"])

(defun vec-typed-forms (repo: Repo) -> (Vec Finding)
  (reduce (fn (acc: (Vec Finding) f: SrcFile)
            (into acc (mapv (fn (t: Top) (Finding (. f path) (. t line) (str (. t head) " " (. t name) " mentions Vec: a window is over an array or a tensor")))
                            (filterv (fn (t: Top)
                                       (and (or (or (= (. t head) "defun") (= (. t head) "impl")) (or (= (. t head) "defstruct") (= (. t head) "defenum")))
                                            (member? (mapv atom-text (atoms-of (. t node))) "Vec")))
                                     (. f tops)))))
          [] (select repo (view-files))))

(defun missing-defs (repo: Repo globs: (Vec str) heads: (Vec str) names: (Vec str)) -> (Vec Finding)
  (let ((have (mapv (fn (m: Measure) (. m key)) (definitions repo globs heads))))
    (mapv (fn (n: str) (Finding (str (first globs)) 0 (str n " is not defined here")))
          (filterv (fn (n: str) (not (member? have n))) names))))

(rule "no function, struct or impl of the view modules mentions Vec"
  (vec-typed-forms repo)
  (plant "lib/fib/view.fib" "\n(defun vec-window (v: (Vec a) lo: i64 hi: i64) -> i64 lo)\n")
  (plant "lib/fib/view/tiles.fib" "\n(impl Windowed (Vec a) (win-len (self) 0))\n"))

(rule "the sigil-free macros and the splitting functions are defined"
  (into (missing-defs repo ["lib/fib/view.fib"] ["defmacro"] ["vget" "vset!" "fill!" "copy-into!" "vlen" "with-view"])
        (into (missing-defs repo ["lib/fib/view.fib"] ["defun"] ["array-window!" "array-window" "array-split!"])
              (missing-defs repo ["lib/fib/view/tiles.fib"] ["defmacro"] ["with-tiles"])))
  (plant-file "lib/fib/view.fib" "(ns fib.view)\n")
  (plant-file "lib/fib/view/tiles.fib" "(ns fib.view.tiles)\n"))

(rule "the checker refuses a closure over a view cell, and the case that pins the text is in the tree"
  (into (must-contain repo "compiler/own/views.fib" "cannot be captured by a closure")
        (into (must-contain repo "cases/ownership/344-reject-with-view-closure-reads-the-window.fib" ";; error:   view cell v cannot be captured by a closure")
              (into (missing repo ["cases/ownership/347-reject-with-view-place-is-captured-by-a-closure.fib" "scripts/mutant-views.sh"])
                    (must-contain repo "scripts/mutant-views.sh" "cases/ownership 336 to 364"))))
  (plant-file "compiler/own/views.fib" "(ns own.views)\n")
  (plant-remove "cases/ownership/344-reject-with-view-closure-reads-the-window.fib"))
```

### What this does not check

That the checker refuses *every* closure over a view cell (the cases and the mutants do: `scripts/mutant-views.sh`, run by hand and in
the performance cycle); that windows are disjoint and safe across tasks (`array-split!`'s cases and TSAN: `scripts/tsan.sh`); the error
texts of decision 6 other than the closure one (`write (splat x)`, a SIMD rule: ADR 0003 and the types golden files); whether a
window over `MArray` or `Tensor` exists (the tensor library has its own cases); a view type that holds a `Vec` under a different
name or through a protocol method (the rule reads the atoms of a form, not its types).
