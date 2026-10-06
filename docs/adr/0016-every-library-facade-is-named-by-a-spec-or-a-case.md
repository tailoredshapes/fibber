# 0016. Every library facade is named by a spec or a case

Status: accepted
Date: 2026-10-06
Source: spec/method.md ("nothing is done until an executable test says so"); CLAUDE.md (`lib/`: "the library's cases" under `cases/stdlib/`); docs/design/test-harness.md
(`specs/`).

## Context

The library is `lib/prelude.fib` and `lib/fib/`: a facade module per area (`fib.core`, `fib.coll`, `fib.json`, `fib.tensor`, ...) in
`lib/fib/*.fib`, over parts in the directory of the same name. The facade is what a program names (`(:use fib.coll)`); the parts are
reached through it. A library area with no case and no spec is code that nothing runs, and in a Lisp that compiles what a program
names, it is not even type-checked by the gate.

## Decision

Every facade of `lib/fib/*.fib` is named, as a whole module name, by at least one file of `specs/*.fib` or `cases/**.fib`: a
program that the gate runs `:use`s or `:require`s it. The parts are covered through their facade; this rule does not ask for a case per
part (that would ask for 150 more, and `fibc cases` already reaches the parts through the facades). Every facade has a case today, so the
list of known gaps is empty. A gap that someone must add (a facade whose cases are still to come) is an allow-list entry with the facade's
name, and the list only shrinks.

## Consequences

- A new area of the library (a facade) arrives with a case or a spec, or `fibc adr` refuses it.
- Naming is not testing: a case that merely `:use`s a facade satisfies the rule. The depth of the coverage is judged by a person
  (the case lists, the mutant scripts); this rule keeps the floor.
- A facade that is renamed or removed leaves no mention of the old name behind to hide a gap.

## Governance

```fibber fitness
(defun module-char? (b: i64) -> bool
  (or (or (and (>= b 97) (<= b 122)) (and (>= b 65) (<= b 90)))
      (or (and (>= b 48) (<= b 57)) (or (or (= b 45) (= b 46)) (or (or (= b 33) (= b 63)) (or (= b 42) (or (= b 47) (or (= b 60) (or (= b 62) (or (= b 61) (or (= b 43) (= b 95))))))))))))

(defun byte-or-space (s: str i: i64) -> i64 (if (and (>= i 0) (< i (str-len s))) (sext i64 (str-byte-at s i)) 32))

(defun names-module? (text: str name: str) -> bool
  (loop ((from 0))
    (match (str-find text name from)
      (nil false)
      ((some k) (if (or (module-char? (byte-or-space text (- k 1))) (module-char? (byte-or-space text (+ k (str-len name)))))
                    (recur (+ k 1))
                    true)))))

(defun tests-text (repo: Repo) -> str
  (join "\n" (mapv (fn (p: str) (unwrap-or (file-text repo p) "")) (names-matching repo ["specs/*.fib" "cases/**.fib"]))))

(defun uncovered-facades (repo: Repo allow: (Vec str)) -> (Vec Finding)
  (let ((text (tests-text repo)))
    (allowing (mapv (fn (f: SrcFile) (Finding (. f path) 1 (str (module-of f) " is named by no spec and no case")))
                    (filterv (fn (f: SrcFile) (not (names-module? text (module-of f)))) (module-files repo ["lib/fib/*.fib"])))
              allow)))

(rule "every facade of lib/fib/*.fib is named by a spec or a case (no known gaps)"
  (uncovered-facades repo [])
  (plant-file "lib/fib/zz-plant.fib" "(ns fib.zz-plant)\n"))

(rule "the rule reads the library and the tests (a search of nothing finds nothing)"
  (into (if (< (count (names-matching repo ["lib/fib/*.fib"])) 25) [(Finding "lib/fib" 0 "fewer than 25 facade modules")] [])
        (into (if (< (count (names-matching repo ["specs/*.fib"])) 10) [(Finding "specs" 0 "fewer than 10 specs")] [])
              (if (< (count (names-matching repo ["cases/**.fib"])) 1000) [(Finding "cases" 0 "fewer than 1000 case files")] [])))
  (plant-remove "lib/fib")
  (plant-remove "specs")
  (plant-remove "cases/stdlib"))
```

### What this does not check

That a case *exercises* the facade's functions (naming is not testing); the parts of a facade (covered through it); `lib/prelude.fib`
(the implicit part of every program: every case runs it); a facade written in a directory other than `lib/fib/` (there is none); the
mutant scripts, which are the depth check (`scripts/mutant-*.sh`).
