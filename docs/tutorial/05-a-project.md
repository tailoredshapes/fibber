---
examples: required
---

# A project

Create and test a project:

```sh
fibc new hello
cd hello
fibc run src/main.fib
fibc test
fibc deps tree
```

The generated `deps.fib` declares source paths. Keep `deps.lock` in git to pin
remote dependencies. For a library project use `fibc new NAME --lib`. Each
module's namespace maps dots to directories. The example uses a shipped module;
put it in `src/main.fib` to try qualified names.
[Next: concurrency](06-concurrency.md).

```fib run
(ns main (:require [fib.string :as text]))
(defun main () -> i64 (do (println (text/join ", " ["tea" "cup"])) 0))
```

```text out
tea, cup
0
```
