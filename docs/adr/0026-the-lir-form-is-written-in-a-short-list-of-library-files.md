# 0026. The lir form is written in a short list of library files

Status: accepted
Date: 2026-10-21
Source: the owner, 2026-10-20 ("best practices should be the easiest thing, foot gunning is always allowed as long as the hoops make it obvious thats what you're doing");
docs/design/inline-lir.md; spec/syntax.md 3.15a; ADR 0011 (native access) and 0015 (the in-place primitives) are the same shape.

## Context

`(unsafe (lir [p: T ..] -> R "body"))` (spec/syntax.md 3.15a) is a call of a function whose body is lIR text: ordered atomics, a pause instruction, the cache line, raw
address arithmetic. The lIR checker checks the body's types, dominance and orderings' legality, not that an ordering is *enough*, that a pointer is valid or aligned,
or that two threads agree about who writes a word. That is what `unsafe` says about everything inside it, and `lir` says it in the loudest way the language has. Application and library
code should not need it: `fib.atomic` has the loads, stores, swaps, fences, `spin-hint` and `cache-line` with their orderings named, and inlines into its caller.

## Decision

In the library (`lib/`, `scripts/`, `editors/`) the form is written in `lib/fib/atomic.fib` and nowhere else. A new library file that writes one is a decision to write down here, with its
reason. The list is exact: an entry that no longer matches is a finding. Code outside the library (an application, a package such as fib-disruptor, the cases and the compiler's tests) may
write the form, and `fibc explain unsafe FILE` lists every such site of a program, the library's included, with the atomics each one uses: that listing is the hoop for code the rule does not
cover. The form stays defined: spec/syntax.md 3.15a names it, and the cases of `cases/stdlib` 8800 to 8806 and 8810 check it.

A body may not call a function. Native access belongs to the platform layer and the drivers (ADR 0011): a `lir` body that called `sched_yield` or `malloc` would be an `extern` written
where the rule cannot see it. The platform layer provides the call (`fib.os.time/thread-yield`), and a body is for what the language itself lacks.

## Consequences

- A queue, a flag or a counter shared between threads is written with `fib.atomic`; the author of a new atomic operation adds it there, to the list of one file.
- A reviewer who sees `lir` in a library diff knows it is a decision, and finds the reason here.

## Governance

```fibber fitness
(defun lir-uses (repo: Repo globs: (Vec str)) -> (Vec Finding)
  (mapv (fn (f: Finding) (Finding (. f file) (. f line) "lir is written here")) (grep-live repo globs "(lir [")))

(rule "in the library, the lir form is written only in lib/fib/atomic.fib"
  (allowing (lir-uses repo ["lib/**.fib" "scripts/**.fib" "editors/**.fib"])
            ["lib/fib/atomic.fib:lir"])
  (plant "lib/fib/json/buffer.fib" "\n(defun peek (p: ptr) -> i64 (unsafe (lir [p: ptr] -> i64 \"(BLOCK entry (RET (ATOMIC-LOAD ACQUIRE I64 p)))\")))\n")
  (plant "lib/fib/tensor/storage.fib" "\n(defun peek (p: ptr) -> i64 (unsafe (lir [p: ptr] -> i64 \"(BLOCK entry (RET (ATOMIC-LOAD ACQUIRE I64 p)))\")))\n"))

(rule "the form is in the spec and a case checks each part of it"
  (into (must-contain repo "spec/syntax.md" "### 3.15a `lir`")
        (into (missing repo ["cases/stdlib/8800-a-lir-form-is-a-function-whose-body-is-lir.fib"
                             "cases/stdlib/8802-reject-lir-outside-unsafe.fib"
                             "cases/stdlib/8804-reject-a-lir-body-the-lir-checker-refuses.fib"
                             "cases/stdlib/8810-fib-atomic-orders-loads-stores-and-swaps.fib"
                             "compiler/tests/live/lir.sh"])
              (must-contain repo "lib/fib/atomic.fib" "docs/design/inline-lir.md")))
  (plant-file "spec/syntax.md" "# syntax\n")
  (plant-remove "cases/stdlib/8802-reject-lir-outside-unsafe.fib")
  (plant-remove "compiler/tests/live/lir.sh"))
```

### What this does not check

That a permitted body is right: the cases and fib-disruptor's specs (which run on x86-64 and, once, on an Apple M1 Ultra, where a weakened release is caught) are the evidence; that code outside
`lib/` is careful, which `fibc explain unsafe` makes findable and nothing makes correct; a site written through a macro (a macro's expansion is not the text of the file).
