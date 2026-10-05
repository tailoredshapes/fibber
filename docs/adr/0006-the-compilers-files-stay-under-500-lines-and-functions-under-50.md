# 0006. The compiler's files stay under 500 lines and its functions under 50

Status: accepted
Date: 2026-10-05
Source: CLAUDE.md ("Rust standards" and "`compiler/` is where the work is": fibber source follows the same limits as Rust where it can).

## Context

The compiler and the library are fibber programs that a person or an agent must be able to read in pieces. The project's rule, kept
for the Rust and carried over to fibber, is files under 500 lines and functions under 50, split by responsibility. The rule was prose;
files and functions drifted past it where splitting was hard (generated code, a macro that cannot call helpers).

## Decision

Every `.fib` file of `compiler/` and `lib/` outside the tests has fewer than 500 lines, and every function (`defun`, `defmacro`,
`defn`) fewer than 50. The exceptions are listed below with the size each has **today**. The list only shrinks: an entry may not
grow, must be lowered when its file or function shrinks, and is removed when it is back within the limit. A new exception is a decision
to be written down, not an edit of the list.

## Consequences

- Today's exceptions, honestly: `compiler/emit/runtime.fib` is generated from `rt/*.lir` (compiler/tests/emit/runtime.sh checks that it equals
  what the generator makes), so the limit does not apply to it and it is excluded from the rule; `lib/prelude.fib` (721) is the
  language's embedded prelude; `compiler/expand/ctx.fib` (525) is over by 25. Functions: `compiler/native/support.fib:mailbox-source`
  (333, a function that holds the text of a runtime source), `compiler/types/lower/call.fib:special-kind` (52) and the `scenario`
  macro of `fib.test.core` (73: a stage-2 macro sees the prelude only and cannot call a helper, docs/design/test-harness.md §10).
- `compiler/tests/` is not measured: some test inputs are long on purpose (a reader test of nesting depth 1001 has 1001 lines).
- "Lines" are physical lines of the file or of the form, counted by the light reader of `fib.test.arch`: a comment inside a function counts.

## Governance

```fibber fitness
(limit "every .fib file of compiler/ and lib/ outside the tests has fewer than 500 lines (shrink-only allow-list)"
  (file-lengths repo ["compiler/**.fib" "lib/**.fib" "!compiler/tests/**" "!compiler/emit/runtime.fib"])
  499
  ["compiler/expand/ctx.fib=525" "lib/prelude.fib=721"]
  (plant-file "compiler/emit/zz-plant.fib" (join "\n" (mapv (fn (i: i64) ";; a line") (range 600))))
  (plant "compiler/expand/ctx.fib" "\n;; one line more than the list allows\n"))

(limit "every function of compiler/ and lib/ outside the tests has fewer than 50 lines (shrink-only allow-list)"
  (fn-lengths repo ["compiler/**.fib" "lib/**.fib" "!compiler/tests/**"])
  49
  ["compiler/native/support.fib:mailbox-source=333" "compiler/types/lower/call.fib:special-kind=52" "lib/fib/test/core.fib:scenario=73"]
  (plant-file "compiler/emit/zz-plant.fib"
              (str "(defun zz-long () -> i64\n" (join "\n" (mapv (fn (i: i64) "  (+ 1 1)") (range 60))) "\n  0)\n")))
```

### What this does not check

The tests (`compiler/tests/`) and `cases/`; Rust (there is none); that a function is *readable* (a 49-line function can be bad);
that a split is by responsibility rather than by line count (taste, not checkable); `lib/fib/**/README.md` and other prose;
generated files other than `runtime.fib` (none today; `runtime.fib` itself is excluded because its size follows `rt/`). A function written with a head the light reader does not know (`defn` is
counted; a `defmacro`-generated function is not).
