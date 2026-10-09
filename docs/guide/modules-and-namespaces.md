---
examples: required
---

# Modules and namespaces

Use `:require [module :as alias]` for qualified calls. `:use module` introduces
exports unqualified; avoid it when names overlap. Facades can re-export modules
using `:export-from`. A file's neighbouring modules are searched first, then
`-I` roots, `FIB_LIB` and the library directory; project resolution supplies
additional roots. See [syntax §5](../../spec/syntax.md) for exact visibility
and private-name rules. String `join` means concatenation with a separator;
unqualified `join` waits for a task. Qualify overlapping names.

```fib run
(ns main (:require [fib.string :as text]))
(defun main () -> i64 (do (println (text/join ", " ["tea" "cup"])) 0))
```

```text out
tea, cup
0
```

## Names that look alike

A few names mean one thing unqualified and another under a module alias, and the rule is to **qualify, never rename** (spec/stdlib.md section 3, N15):

| you want | write | not |
|----------|-------|-----|
| wait for a task | `(join t)` or `@t` | `(str/join ..)` |
| join strings with a separator | `(str/join ", " xs)`, `(str/join xs)` | `(join ", " xs)`; the prelude's `str-join` takes a `(Vec str)` and no separator |
| is x in this collection | `(includes? x coll)` (the element first, so `->>` threads the collection) | `(str/includes? ..)` |
| does this string hold that text | `(str/includes? s sub)` (the string first, as Clojure's) | `(includes? ..)` |
| where is it in a string | `(str/index-of s value)`, `(str/last-index-of s value)` (character offsets) | `(index-of ..)` |

`(:require [fib.string :as str])` always; `(:use fib.string)` shadows the implicit `join` and `includes?` with the string ones. A `Range`, a `VSeq` or a `LSeq` is not a `Vec`, so `(if c (range n) [])` does not type-check: write `(if c (rangev n) [])` (also `mapcatv`, `sortv`, `sort-byv`), or `(vec (range n))`.

A byte offset that splits a character traps (`str-slice [0, 2) splits a character`); ask first with `(str/char-boundary? s i)`, `(str/next-boundary s i)`, `(str/prev-boundary s i)`, or count in characters (`subs`, `str/index-of`).
