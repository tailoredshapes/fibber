---
examples: required
---

# Exercise 3: Work with text

**Aim:** distinguish a name from literal text, use escapes and qualify a library
function. Save `ex03.fib`.

## Type and predict

```fib run
(ns main (:require [fib.string :as text]))
(defun main () -> i64
  (let ((name "Ada")
        (cups 3))
    (do
      (println (str "Hello, " name "."))
      (println (str name " has " cups " cups."))
      (println "She said \"hello\".")
      (println (text/join " / " ["tea" "coffee" "water"]))
      0)))
```

`str` builds text from its arguments. `\"` puts a quote inside a string without
ending it. The namespace declaration gives `fib.string` the alias `text`;
`text/join` joins strings with a separator. Unqualified `join` has a different
job: waiting for a task. Keep this string call qualified.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
Hello, Ada.
Ada has 3 cups.
She said "hello".
tea / coffee / water
0
```

</details>

## Change it

1. Change the name and number without editing the message templates.
2. Print a string containing `\n`. Predict the number of output lines.
3. Join the same drinks with `", "`, then an empty separator.

## Break and repair

Remove the backslash before an interior quote in a copy. Observe how the reader
now sees the rest of the line. Restore the escape. Then misspell the `text`
alias on the join call and repair the namespace/call agreement.

## Checkpoint

Explain the difference between `name` and `"name"`. Build a greeting with a
different alias for `fib.string`, and explain why both declarations and calls
must agree. Reference: [module names](../guide/modules-and-namespaces.md).

[← Numbers](02-numbers.md) · [Course](README.md) · [Decisions →](04-decisions.md)
