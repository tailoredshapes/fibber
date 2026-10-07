# 0023. Every stdlib spec row names a bound definition

Status: accepted
Date: 2026-10-07
Source: spec/method.md ("nothing is done until an executable test says so"); CLAUDE.md ("When a spec rule and code disagree, stop and report"); package FOLLOWUP-1, which found
`parse-double` in the function table of spec/stdlib.md section 4 (a row with a signature and a note) and **unbound** in fibc 0.1.11: a program that called it was rejected with
`unbound name parse-double`, and nothing in the gate had ever said so.

## Context

Section 4 of `spec/stdlib.md` is the function table: one row per Clojure name or library extra, with the fibber spelling in the third column (`(parse-double s)`), a status (`keep`,
`adapt`, `alias`, `new`, `add`, `omit`), a signature and a tranche. A row is a promise that the name works. ADR 0014 holds the compiler's builtins to the spec and to a case, from the code
towards the page; nothing held the page to the code. A compile of one probe program naming every row's head (FOLLOWUP-1) found **279 of the 729 non-`omit` rows unbound** in the tree of
2026-10-07: the later tranches of the plan, the JVM interop names, and a few cheap ones that had simply been forgotten (`parse-double`, `parse-boolean`, `try-parse-int`,
`Integer/parseInt`, `str/replace`). Most of those are honest plan (the page is the design of the whole library and the tranches land in order); the failure is that no test said which were
which, so a missing row looked the same as a row not yet due.

## Decision

Every row of the function table (sections 4.1 to 4.20 of `spec/stdlib.md`) whose status is not `omit`, and whose fibber column starts with a call `(NAME ...)`, names a **bound
definition**: its head is, for an unqualified name,

1. defined at the top level of `lib/` (`defun defn defmacro def defstruct defenum defprotocol defrecord`, or a method of a `defprotocol`; a clause `name$2` is `name`), the prelude included, or
2. a name of the compiler's builtin table (`BuiltinSig` rows of `compiler/types/builtins.fib`), or
3. a core form or macro the front end knows: the name appears as a whole string in `compiler/expand/`, `compiler/types/` or `compiler/syntax/`, or as a method `(name (self ..` of a protocol there;

and for a qualified name (`str/replace`, `math/sin`, `Integer/parseInt`) it is defined at the top level of the module the qualifier names (`str` is `fib.string`, `math` is `fib.math`, `Integer` is `lib/Integer.fib`).

The rows that are **not bound today** are listed, one line each with the reason, in `docs/adr/0023-unbound-rows.tsv` (`NAME<TAB>reason`). The list **only shrinks**: a row that
is bound and still listed is a finding (remove it), and an unbound row that is not listed is a finding (implement it, or write it down with its reason, which is a decision to record, not an
edit to slip through). The target is an empty list; each package that lands a tranche removes its rows.

## Consequences

- A row added to the table for a function that does not exist fails `fibc adr` at once, so the page cannot promise a name the library lacks without saying so.
- The list is the honest map of what remains: its reasons name the tranche or the missing language feature, and a package that implements a row deletes its line.
- It is a **name** check, written to be cheap (a text rule over the parsed forms, about a second inside the `fibc adr` budget): it does not know that the name is bound in the module the
  section names for an unqualified row, nor that the signature matches the row, nor that a case runs it. Forms and macros are found through the compiler's string tables, so a name that merely
  appears as a whole string there counts; that errs towards accepting. The compile probe that found the 279 (a program of `(defun pN () NAME)` per head, scanned for `unbound name`)
  agreed with this rule on every function.

## Governance

```fibber fitness
(defun table-bar :private (line: str from: i64) -> i64
  (match (str-find line "|" from) (nil -1) ((some k) k)))

(defun table-trim :private (s: str) -> str
  (loop ((a 0) (b (str-len s)))
    (cond (and (< a b) (or (= (str-byte-at s a) 32i8) (= (str-byte-at s a) 96i8))) (recur (+ a 1) b)
          (and (< a b) (or (= (str-byte-at s (- b 1)) 32i8) (= (str-byte-at s (- b 1)) 96i8))) (recur a (- b 1))
          :else (str-slice s a b))))

;; the first n cells of a table row (a line `| a | b | c |`), trimmed of spaces and backticks
(defun table-cells :private (line: str n: i64) -> (Vec str)
  (loop ((from 1) (acc []))
    (if (= (count acc) n)
        acc
        (let ((e (table-bar line from)))
          (if (< e 0) acc (recur (+ e 1) (conj acc (table-trim (str-slice line from e)))))))))

;; the name a call form `(NAME ...)` starts with, qualifier included, or "" for anything that does not start with a call
(defun call-head :private (form: str) -> str
  (if (and (> (str-len form) 1) (= (str-byte-at form 0) 40i8))
      (str-slice form 1 (loop ((k 1))
                          (if (and (< k (str-len form)) (not (or (= (str-byte-at form k) 32i8) (= (str-byte-at form k) 41i8)))) (recur (+ k 1)) k)))
      ""))

;; the rows of sections 4.1 to 4.20 that are not `omit` and start with a call: a Finding per row, `file:name` the key, the text the name and then why
(defun table-rows (repo: Repo) -> (Vec Finding)
  (let ((lines (split-lines (unwrap-or (file-text repo "spec/stdlib.md") ""))))
    (loop ((i 0) (inside false) (acc []))
      (if (>= i (count lines))
          acc
          (let ((l (nth lines i)))
            (cond (starts-with? l "### 4.1 ") (recur (+ i 1) true acc)
                  (starts-with? l "## 5.") (recur (+ i 1) false acc)
                  (and inside (starts-with? l "| `"))
                  (let ((cells (table-cells l 3))
                        (name (call-head (if (> (count cells) 2) (nth cells 2) "")))
                        (status (if (> (count cells) 1) (nth cells 1) "")))
                    (recur (+ i 1) inside
                           (if (or (= name "") (or (= status "omit") (starts-with? name ":")))
                               acc
                               (conj acc (Finding "spec/stdlib.md" (+ i 1) (str name " is a row of the function table and nothing defines it"))))))
                  :else (recur (+ i 1) inside acc)))))))

;; the methods of the defprotocol forms of the files: a method is bound without a top-level definition of its own
(defun protocol-methods :private (repo: Repo globs: (Vec str)) -> (Vec str)
  (reduce (fn (acc: (Vec str) f: SrcFile)
            (reduce (fn (a: (Vec str) t: Top)
                      (if (= (. t head) "defprotocol")
                          (into a (mapv (fn (m: Node) (atom-text (nth (items-of m) 0)))
                                        (filterv (fn (m: Node) (> (count (items-of m)) 0)) (drop 2 (items-of (. t node))))))
                          a))
                    acc (. f tops)))
          [] (select repo globs)))

;; a clause of a multi-arity function is defined as `name$2`, `name$3`: the name is what comes before the `$`
(defun clause-name :private (k: str) -> str (match (str-find k "$" 0) (nil k) ((some i) (str-slice k 0 i))))

;; a generic type is defined as `(defstruct (Ratio t) ..)`: the name is the first atom of the list that follows the head
(defun generic-names :private (repo: Repo globs: (Vec str)) -> (Vec str)
  (reduce (fn (acc: (Vec str) f: SrcFile)
            (into acc (mapv (fn (t: Top) (atom-text (nth (items-of (nth (items-of (. t node)) 1)) 0)))
                            (filterv (fn (t: Top) (and (or (= (. t head) "defstruct") (= (. t head) "defenum"))
                                                       (and (= (. t name) "") (and (> (count (items-of (. t node))) 1) (> (count (items-of (nth (items-of (. t node)) 1))) 0)))))
                                     (. f tops)))))
          [] (select repo globs)))

(defun names-in :private (repo: Repo globs: (Vec str)) -> (Vec str)
  (into (into (mapv (fn (m: Measure) (clause-name (. m key)))
                    (definitions repo globs ["defun" "defn" "defmacro" "def" "defstruct" "defenum" "defprotocol" "defrecord"]))
              (protocol-methods repo globs))
        (generic-names repo globs)))

;; the module a qualifier names, as the files that define it: `str` is fib.string, `Math` is lib/Math.fib, `math` is fib.math
(defun module-globs :private (q: str) -> (Vec str)
  (cond (= q "str") ["lib/fib/string.fib" "lib/fib/string/**.fib"]
        (and (> (str-len q) 0) (< (str-byte-at q 0) 97i8)) [(str "lib/" q ".fib")]
        :else [(str "lib/fib/" q ".fib") (str "lib/fib/" q "/**.fib")]))

;; the text of the compiler's tables of forms and macros, and of the builtin table's own protocols (`quot` is a method of one)
(defun forms-text :private (repo: Repo) -> str
  (join "\n" (mapv (fn (p: str) (unwrap-or (file-text repo p) ""))
                   (names-matching repo ["compiler/expand/**.fib" "compiler/types/**.fib" "compiler/syntax/**.fib"]))))

(defun form-name? :private (text: str name: str) -> bool
  (or (some? (str-find text (str "\"" name "\"") 0)) (some? (str-find text (str "(" name " (self") 0))))

(defun row-name :private (f: Finding) -> str
  (let ((t (. f text))) (str-slice t 0 (unwrap-or (str-find t " " 0) (str-len t)))))

(defun unbound-rows (repo: Repo) -> (Vec Finding)
  (let ((unqualified (into (names-in repo ["lib/**.fib"]) (row-strings repo "compiler/types/builtins.fib" "BuiltinSig")))
        (forms (forms-text repo)))
    (filterv (fn (f: Finding)
               (let ((name (row-name f))
                     (slash (unwrap-or (str-find (row-name f) "/" 0) 0)))
                 (not (if (> slash 0)
                          (member? (names-in repo (module-globs (str-slice name 0 slash))) (str-slice name (+ slash 1) (str-len name)))
                          (or (member? unqualified name) (form-name? forms name))))))
             (table-rows repo))))

(defun listed-rows :private (repo: Repo) -> (Vec str)
  (mapv (fn (l: str) (str "spec/stdlib.md:" (str-slice l 0 (unwrap-or (str-find l "\t" 0) (str-len l)))))
        (filterv (fn (l: str) (not (or (= l "") (starts-with? l "#")))) (split-lines (unwrap-or (file-text repo "docs/adr/0023-unbound-rows.tsv") "")))))

(rule "every row of the stdlib function table names a bound definition, except those listed in docs/adr/0023-unbound-rows.tsv (which only shrinks)"
  (allowing (unbound-rows repo) (listed-rows repo))
  (plant-file "spec/stdlib.md" "### 4.1 x\n\n| `zz-plant` | new | `(zz-plant-row-fn x)` | `t -> t` | 1 | a row nobody defined |\n\n## 5. y\n"))

(rule "a listed row that is bound is a finding: the list names only rows that are still unbound"
  (allowing (unbound-rows repo) (listed-rows repo))
  (plant-file "docs/adr/0023-unbound-rows.tsv" "map\tplanted: this row is bound\n"))

(rule "the rule reads the table, the library and the compiler's tables (a search of nothing finds nothing)"
  (into (if (< (count (table-rows repo)) 600) [(Finding "spec/stdlib.md" 0 "fewer than 600 table rows with a call")] [])
        (if (< (count (names-in repo ["lib/**.fib"])) 2000) [(Finding "lib" 0 "fewer than 2000 bound names")] []))
  (plant-file "spec/stdlib.md" "# empty\n"))
```

### What this does not check

That the name is bound in the module a section names (an unqualified row is found as that name defined anywhere in `lib/`), that its signature is the row's, that a case runs it (ADR 0014 holds the builtins
to a case; the library's cases are `cases/stdlib`), reader syntax and rows whose fibber column is not a call, and rows of `omit` status (which are not promises). A name that is defined and does
not work is not found here: that is what the cases are for.
