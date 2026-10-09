---
examples: required
---

# Exercise 1: Read a program aloud

**Aim:** identify forms, strings and comments. Save the program as `ex01.fib`.
Run each exercise with `fibc run exNN.fib`, substituting its number.

## Type and predict

```fib run
;; A semicolon starts a comment; this line does not print.
(defun main () -> i64
  (do
    (println "one form")
    (println "two forms")
    (println "a ; inside a string is text")
    0))
```

Read `(println "one form")` as “call println with this string.” Parentheses
group a form; the first item names the operation. Double quotes delimit a string.
`defun` defines a function, and `()` is main's empty parameter list. `-> i64`
declares its result type. `do` evaluates its forms in order and returns the last.

<details markdown="1">
<summary>Check your prediction</summary>

```text out
one form
two forms
a ; inside a string is text
0
```

</details>

## Change it

1. Add a fourth `println`. Predict where it appears.
2. Comment out the second print by putting `;;` at the beginning of its line.
3. Indent the program differently without changing delimiters. Predict whether
   the output changes. Restore readable indentation afterward.

## Break and repair

In a copy, misspell `println` as `printl`. The smallest complete example of that
mistake is deliberately rejected:

```fib reject "unbound name printl"
(defun main () -> i64
  (do (printl "one form") 0))
```

Save it as `ex01-broken.fib`, observe the diagnostic and repair the name. The
book checks that this program is rejected; it is not a second working solution.

## Checkpoint

Circle the delimiters that belong together. Explain why the semicolon inside
the string prints, while the first line does not. Recreate a two-message program
without looking at this page.

[← Workbench](00-workbench.md) · [Course](README.md) · [Numbers →](02-numbers.md)
