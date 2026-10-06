# 0013. Library code has no global mutable state

Status: accepted
Date: 2026-10-06
Source: CLAUDE.md (the Rust standard "no global state", carried over); spec/bootstrap.md ("no global mutable state"); docs/design/observability-and-databases.md 6.1;
docs/design/crypto.md 3; docs/design/autodiff.md ("there is no global tape"); spec/syntax.md (`def`, run-time `def`s).

## Context

The language allows one global that can change: a `def` whose initialiser is an `Atom` (`(def counter: (Atom i64) (atom 0))`, spec/syntax.md
`def`). Rust's rule "no global state" was carried over to fibber and stated again in the design of logging, tracing, databases, crypto and
autodiff: providers, loggers, tapes and contexts are **values the program passes**, never a registry or a default. A global makes a
library depend on initialisation order, shares one atom among every task that touches it, and makes tests that run in parallel share
configuration. The rule was prose, and the language still lets a library write the global.

## Decision

No top-level `def` of `lib/` holds mutable state: its form does not mention `atom`, `cell`, `volatile`, `adder` or the types of those. In
`compiler/` the same holds, except the entries of the allow-list below, which may only shrink. A library that needs state takes it as a
parameter or builds it in a function.

## Consequences

- A library cannot keep a counter, a cache or a registry in the module: the caller owns it. Callers pass a value (a provider, a logger,
  a tape, a `DataSource`).
- The allow-list has one entry today: `compiler/native/call.fib:the-host`, the loaded JIT host that `native.call` shares between the
  macro runner and the driver (`docs/design/lair-in-fibber.md`: an explicit `dispose` is the plan). It is removed when that is passed.
- `thread-local` state does not exist in the language (lIR has no thread-local storage), so there is nothing else to check there.

## Governance

```fibber fitness
(defun state-names () -> (Vec str) ["atom" "Atom" "cell" "Cell" "volatile" "Volatile" "adder" "Adder" "Mutex"])

(defun mutable-def? (t: Top) -> bool
  (let ((as (mapv atom-text (filterv (fn (a: Node) (not (string-atom? a))) (atoms-of (. t node))))))
    (and (= (. t head) "def") (not (empty? (filterv (fn (n: str) (member? as n)) (state-names)))))))

(defun mutable-defs (repo: Repo globs: (Vec str)) -> (Vec Finding)
  (reduce (fn (acc: (Vec Finding) f: SrcFile)
            (into acc (mapv (fn (t: Top) (Finding (. f path) (. t line) (str (. t name) " is a def of mutable state")))
                            (filterv mutable-def? (. f tops)))))
          [] (select repo globs)))

(rule "no top-level def of lib/ is an atom, a cell, a volatile or an adder"
  (mutable-defs repo ["lib/**.fib"])
  (plant "lib/fib/core/cells.fib" "\n(def counter: (Atom i64) (atom 0))\n")
  (plant "lib/fib/otel/ids.fib" "\n(def cache: (Cell i64) (cell 0))\n"))

(rule "compiler/ has no such def except the shrink-only allow-list (outside the tests)"
  (allowing (mutable-defs repo ["compiler/**.fib" "!compiler/tests/**"]) ["compiler/native/call.fib:the-host"])
  (plant "compiler/emit/compile.fib" "\n(def counter: (Atom i64) (atom 0))\n"))

(rule "the rules above read the tree: lib/ has at least 200 files with forms, and the compiler's one known def is seen"
  (into (if (< (count (filterv (fn (f: SrcFile) (not (empty? (. f tops)))) (select repo ["lib/**.fib"]))) 200)
            [(Finding "lib" 0 "fewer than 200 lib files with forms")] [])
        (if (empty? (mutable-defs repo ["compiler/native/call.fib"])) [(Finding "compiler/native/call.fib" 0 "the-host is not seen as a def of an atom")] []))
  (plant-file "compiler/native/call.fib" "(ns native.call)\n"))
```

### What this does not check

State that is not a top-level `def` (a module-level `extern` global of C, a runtime global in lIR: `rt/*.lir` has many and the runtime is
not a library); a `def` that holds mutable state without naming `atom`, `cell`, `volatile` or `adder` (a call to a function that returns
a cell); `scripts/`, `cases/` and `compiler/tests/`; a global made by a macro expansion rather than a written `def`.
