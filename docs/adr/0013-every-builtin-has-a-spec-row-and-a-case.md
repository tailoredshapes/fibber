# 0013. Every builtin has a spec row and a case

Status: accepted
Date: 2026-10-06
Source: spec/method.md ("nothing is done until an executable test says so"); CLAUDE.md ("How work is judged"); the builtin table `compiler/types/builtins.fib`;
the new-builtin practice of the numerical, JSON and exceptions packages (each added rows, cases and a mutant script).

## Context

The builtins are the language's surface under the library: `BuiltinSig` rows of `compiler/types/builtins.fib` (name, signature, escape kinds). The
practice for a new one is a row in the spec (spec/types.md and its siblings), a case that runs it, and a mutant (a deliberate break of the
implementation that a case must catch: `scripts/mutant-*.sh`). It was prose. Rows were added to the table with the spec and the case
following later or never, and the table had no reader but the type checker.

## Decision

Every name in the `BuiltinSig` table of `compiler/types/builtins.fib`:

1. is named in the specification (`spec/*.md`), as a whole token, and
2. is named in a case (`cases/**.fib`), as a whole token.

Both lists of exceptions below are what the tree has **today**. They only shrink: an entry whose name now has its row or its case is a
finding until it is removed; a new builtin without a row or a case is a finding and is not added to the list (that would be a decision
to write down, not an edit).

## Consequences

- A builtin added to the table without a spec row and a case fails `fibc adr` at once. The row and the case may be as small as
  the signature and one call, but they exist.
- The `sys-*` builtins are reached through `fib.os` and covered by the cases of that library, not by a case that names them: they are
  in the list of exceptions for that reason, not because they are untested.
- The requirement of a **mutant** is not encoded: no table says which mutant script covers which builtin, and a script per builtin would be
  a rule that could not fail in a useful way. A reviewer asks for it.

## Governance

```fibber fitness
(defun name-char? (b: i64) -> bool
  (or (or (and (>= b 97) (<= b 122)) (and (>= b 65) (<= b 90)))
      (or (and (>= b 48) (<= b 57)) (or (or (= b 45) (= b 33)) (or (or (= b 63) (= b 42)) (or (or (= b 47) (= b 60)) (or (or (= b 62) (= b 61)) (or (= b 43) (= b 95)))))))))

(defun byte-or-space (s: str i: i64) -> i64 (if (and (>= i 0) (< i (str-len s))) (sext i64 (str-byte-at s i)) 32))

(defun mentions? (text: str name: str) -> bool
  (loop ((from 0))
    (match (str-find text name from)
      (nil false)
      ((some k) (if (or (name-char? (byte-or-space text (- k 1))) (name-char? (byte-or-space text (+ k (str-len name)))))
                    (recur (+ k 1))
                    true)))))

(defun joined-text (repo: Repo globs: (Vec str)) -> str
  (join "\n" (mapv (fn (p: str) (unwrap-or (file-text repo p) "")) (names-matching repo globs))))

(defun builtin-names (repo: Repo) -> (Vec str) (row-strings repo "compiler/types/builtins.fib" "BuiltinSig"))

(defun unnamed-in (repo: Repo text: str where: str) -> (Vec Finding)
  (mapv (fn (n: str) (Finding "compiler/types/builtins.fib" 1 (str n " is not named in " where)))
        (filterv (fn (n: str) (not (mentions? text n))) (builtin-names repo))))

(rule "every builtin is named in the specification (spec/*.md), except the seven that are not yet"
  (allowing (unnamed-in repo (joined-text repo ["spec/*.md"]) "spec/*.md")
            ["compiler/types/builtins.fib:store-f32" "compiler/types/builtins.fib:store-f32x4" "compiler/types/builtins.fib:store-f32x8"
             "compiler/types/builtins.fib:store-f64" "compiler/types/builtins.fib:store-f64x4" "compiler/types/builtins.fib:store-i16"
             "compiler/types/builtins.fib:store-i32"])
  (plant "compiler/types/builtins.fib" "\n(def zz-row: i64 (BuiltinSig \"zz-plant-builtin\" \"(fn () unit)\" \"\" [] false))\n"))

(rule "every builtin is named in a case (cases/**.fib), except the twenty-two that no case names today"
  (allowing (unnamed-in repo (joined-text repo ["cases/**.fib"]) "cases/**.fib")
            ["compiler/types/builtins.fib:catch-active?" "compiler/types/builtins.fib:caught-message" "compiler/types/builtins.fib:caught-object"
             "compiler/types/builtins.fib:enum-params" "compiler/types/builtins.fib:finally-enter" "compiler/types/builtins.fib:finally-leave"
             "compiler/types/builtins.fib:load-i16" "compiler/types/builtins.fib:simd/kind" "compiler/types/builtins.fib:store-i16"
             "compiler/types/builtins.fib:store-i32" "compiler/types/builtins.fib:struct-field-types" "compiler/types/builtins.fib:struct-params"
             "compiler/types/builtins.fib:struct?" "compiler/types/builtins.fib:sys-clock-now" "compiler/types/builtins.fib:sys-close"
             "compiler/types/builtins.fib:sys-dup" "compiler/types/builtins.fib:sys-errno-text" "compiler/types/builtins.fib:sys-getenv"
             "compiler/types/builtins.fib:sys-isatty" "compiler/types/builtins.fib:sys-mkdir" "compiler/types/builtins.fib:sys-open"
             "compiler/types/builtins.fib:sys-pipe" "compiler/types/builtins.fib:sys-read" "compiler/types/builtins.fib:sys-rmdir"
             "compiler/types/builtins.fib:sys-seek" "compiler/types/builtins.fib:sys-thread-stripe" "compiler/types/builtins.fib:sys-unlink"
             "compiler/types/builtins.fib:sys-wall-now" "compiler/types/builtins.fib:sys-write"])
  (plant "compiler/types/builtins.fib" "\n(def zz-row: i64 (BuiltinSig \"zz-plant-builtin\" \"(fn () unit)\" \"\" [] false))\n"))

(rule "the rules above read the table, the spec and the cases (a search of nothing finds nothing)"
  (into (if (< (count (builtin-names repo)) 150) [(Finding "compiler/types/builtins.fib" 0 "fewer than 150 BuiltinSig rows")] [])
        (into (if (< (count (names-matching repo ["spec/*.md"])) 6) [(Finding "spec" 0 "fewer than 6 spec files")] [])
              (if (< (count (names-matching repo ["cases/**.fib"])) 1000) [(Finding "cases" 0 "fewer than 1000 case files")] [])))
  (plant-file "compiler/types/builtins.fib" "(ns types.builtins)\n"))
```

### What this does not check

That a spec row is *right* (the name may be mentioned in passing: whole-token matching is the strongest cheap test); that a case really
exercises the builtin and checks its result (a case that only names it passes); that a **mutant** exists for a new builtin (prose: a
reviewer asks for it); the primitive forms and conversions that have no `BuiltinSig` row (`set-field!`, `dyn`, `splat`: ADR 0003 reads
those tables); builtins that only `lib/prelude.fib` or a library wrapper reach (the `sys-*` ones are in the first exception list).
