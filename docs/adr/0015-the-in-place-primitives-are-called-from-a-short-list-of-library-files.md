# 0015. The in-place primitives are called from a short list of library files

Status: accepted
Date: 2026-10-06
Source: CLAUDE.md ("The in-place primitives `array-take!`, `array-push!`, `array-pop!` and `cell-update!` ... exist in the compiler in fibber only; `lib/prelude.fib` calls
them"); spec/types.md 2.13.1; docs/design/in-place-update.md; scripts/mutant-unique.sh.

## Context

`array-take!`, `array-push!`, `array-pop!` and `cell-update!` (spec/types.md 2.13.1) move a value out of an array or a cell and write it back,
in place when the value is unique and on a copy when it is not. They are what makes a persistent `Vec` or `Map` update in place, and they
carry obligations the type checker does not check: after `array-take!` of an object element the slot is null until it is written again
(the caller must not read it); a function with an `array-take!` does not pass failures on (decisions-2026-10-04.md); `cell-update!`'s function
runs once with the cell empty. Application code uses the library's `update!`, `conj`, `assoc` and `pop`, which call the primitives
correctly. A call from anywhere else is a place where those obligations are met by hand.

## Decision

Outside the compiler, the primitives are called (live code, not comments) only from `lib/prelude.fib` and the library files on the list
below, each of which has a reason: `fib.core.cells` (`update!`, a wrapper), `fib.bigint.mag` (`array-pop!` shortens a unique magnitude),
`fib.http.server.reader` and `fib.os.files` (`array-push!` builds a byte buffer). A new caller in `lib/` is a decision to write down.
The list is exact: an entry that no longer matches is a finding. The compiler's own program text may use them freely: it is the code
that lowers them, and it is compiled by a compiler that has them.

The decision also keeps the four **defined**: each is a row of the builtin table, a signature line in spec/types.md 2.13.1, and is
named by a case that checks it (the take, push, pop and update cases of `cases/stdlib`), and the mutant that breaks uniqueness
(`scripts/mutant-unique.sh`) stays.

## Consequences

- `fib.json.buffer` (a growable byte buffer that does not use `array-push!`: its note says why) and any new buffer type do not call the
  primitives; they build on `fib.core` operations.
- A library that wants in-place updates uses `update!` on a cell, or `with-view`, or goes through the prelude.
- The compiler is free to use the primitives; the rule on it is only that they stay defined.

## Governance

```fibber fitness
(defun inplace-names () -> (Vec str) ["array-take!" "array-push!" "array-pop!" "cell-update!"])

(defun live-uses (repo: Repo globs: (Vec str)) -> (Vec Finding)
  (reduce (fn (acc: (Vec Finding) n: str)
            (into acc (mapv (fn (f: Finding) (Finding (. f file) (. f line) (str n " is called here")))
                            (grep-live repo globs n))))
          [] (inplace-names)))

(rule "outside the compiler, live code calls the in-place primitives only from the prelude and the listed library files"
  (allowing (live-uses repo ["lib/**.fib" "scripts/**.fib" "editors/**.fib"])
            ["lib/prelude.fib:array-take!" "lib/prelude.fib:array-push!" "lib/prelude.fib:array-pop!"
             "lib/fib/core/cells.fib:cell-update!" "lib/fib/bigint/mag.fib:array-pop!"
             "lib/fib/http/reader.fib:array-push!" "lib/fib/os/files.fib:array-push!"])
  (plant "lib/fib/json/buffer.fib" "\n(defun grow (&b: (Array i8) x: i8) -> unit (array-push! &b x))\n")
  (plant "lib/fib/tensor/storage.fib" "\n(defun last-of (&b: (Array i8)) -> i8 (array-pop! &b))\n"))

(rule "each primitive is a row of the builtin table and a signature line of spec/types.md"
  (into (mapv (fn (n: str) (Finding "compiler/types/builtins.fib" 0 (str n " is not a BuiltinSig row")))
              (filterv (fn (n: str) (not (member? (row-strings repo "compiler/types/builtins.fib" "BuiltinSig") n))) (inplace-names)))
        (into (must-contain repo "spec/types.md" "array-take!: ∀a. (fn ((& (Array a)) i64) a)")
              (into (must-contain repo "spec/types.md" "array-push!: ∀a. (fn ((& (Array a)) a) unit)")
                    (into (must-contain repo "spec/types.md" "array-pop! : ∀a. (fn ((& (Array a))) a)")
                          (must-contain repo "spec/types.md" "cell-update!: ∀a. (fn ((Cell a) (fn :send (a) a)) unit)")))))
  (plant-file "compiler/types/builtins.fib" "(ns types.builtins)\n")
  (plant-file "spec/types.md" "# types\n"))

(rule "a case names each primitive, and the mutant that breaks uniqueness is in the tree"
  (into (must-contain repo "cases/stdlib/4009-the-take-primitives-leave-a-whole-array-when-it-is-shared-and-a-clean-audit-when-it-is-not.fib" "array-take!")
        (into (must-contain repo "cases/stdlib/4011-1000-conj-on-a-unique-vec-allocate-one-shell-each.fib" "array-push!")
              (into (must-contain repo "cases/stdlib/4013-1000-pop-on-a-unique-vec-allocate-one-shell-each.fib" "array-pop!")
                    (into (must-contain repo "cases/stdlib/7877-cell-update-whose-function-traps-is-fatal.fib" "cell-update!")
                          (missing repo ["scripts/mutant-unique.sh"])))))
  (plant-remove "cases/stdlib/4013-1000-pop-on-a-unique-vec-allocate-one-shell-each.fib")
  (plant-remove "scripts/mutant-unique.sh"))
```

### What this does not check

That a permitted call meets its obligations (the null slot, the region without failures): `compiler/tests/own` and the cases do, one
by one; that the compiler's own calls are right; that a library file calls the primitive through an alias or a macro (`update!` on a
cell is the intended route and is not a call of the primitive in the caller's text); `cases/` and `compiler/tests/` (a test may call
them to test them); that the four cases named above are the only coverage (they are the ones this rule can name).
