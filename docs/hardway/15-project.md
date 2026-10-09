---
examples: required
---

# Exercise 15: Start a project

**Aim:** run and test from a project directory, then separate an exported
function from the application. Keep this project beside your exercise files.

## Create and inspect

From your `fibber-hardway` directory:

```sh
fibc new tally
cd tally
fibc run src/main.fib
fibc test
fibc deps tree
```

Inspect `deps.fib`, `src/` and the generated spec before editing anything.
`deps.fib` declares the project's source paths and dependencies. Keep commands
in this directory so project discovery finds it. Keep a generated `deps.lock`
in version control when dependencies are resolved and locked.

## Type and predict

Replace `src/main.fib` with this program:

```fib run
(ns main (:require [fib.string :as text]))
(defun main () -> i64
  (do
    (println (text/join ", " ["read" "count" "report"]))
    0))
```

<details markdown="1">
<summary>Check your prediction</summary>

```text out
read, count, report
0
```

</details>

## Change it

1. Run the replacement main and run the generated spec. Inspect what the spec
   actually tests; passing that spec does not prove your new main is correct.
2. Create `src/tally/price.fib` with namespace `tally.price`. Move exercise 14's
   `total` function there, preserving its signature. A library module needs no main.
3. Copy exercise 14 into `specs/price-spec.fib`. Remove its inline `total`
   definition, require `[tally.price :as price]` in both main and that spec,
   and call `price/total`. Keep the scenarios and expected values. Run main,
   `fibc test specs/price-spec.fib`, then `fibc test` to check discovery too.

The mapping is literal: `tally.price` maps to `tally/price.fib` under a source
root, here `src`. Ordinary public declarations are available through the module
alias. Use [the namespace guide](../guide/modules-and-namespaces.md) for details.

## Break and repair

Rename the module file to `prices.fib` without changing the namespace or require.
Observe the missing-module diagnostic. Repair the path/declaration agreement.
Then run from the parent directory and explain why relative file paths and
project discovery can differ. Return to the project directory before continuing.

## Checkpoint

Draw the path from the project's source root to `tally.price`. Explain what
`deps tree` tells you, and what it cannot tell you about whether your arithmetic
is correct. You now have a place to build the final command.

[← Behaviour specs](14-specs.md) · [Course](README.md) · [Capstone →](16-capstone.md)
