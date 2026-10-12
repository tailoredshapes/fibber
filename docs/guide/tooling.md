---
examples: required
---

# Tooling

`fibc run -O 0` uses fast development code generation; `build` defaults to `-O 2`.
`check MODULE..` checks every definition in named modules and dependencies;
exit 0 means checked, 3 means rejected and 2 means unloadable. `explain FILE`
shows ownership decisions. `emit` prints lIR; `emit-dump` is a compiler tool.
`serve` retains front-end state over a Unix socket, `--server` sends requests
with local fallback, and `repl` keeps definitions in a session
([below](#the-repl)). See [the dev-loop record](../design/dev-loop.md) for protocol and limits.
`lsp` serves editor requests over stdio; see [VS Code setup](../../editors/vscode/README.md).
All flags are in the [generated CLI reference](../reference/cli.md).
`make help` lists contributor targets. `make adr` invokes a standalone runner;
there is no `fibc adr` command.

```fib run
(defun main () -> i64 (do (println "hello from fibber") 0))
```

```text out
hello from fibber
0
```

## The REPL

`fibc repl [-I DIR]..` is the quickest way to try an idea. Type a definition
(`defun`, `defrecord`, `defenum`, `defprotocol`, `defmacro`, `require`, `use` and
the rest) or one expression; an expression is compiled, run and its value printed.

```text
$ fibc repl
fib> (defun sq (x: i64) -> i64 (* x x))
; defined sq
fib> (defun cube (x: i64) -> i64 (* x (sq x)))
; defined cube
fib> (cube 3)
27
fib> (defun sq (x: i64) -> str "no")
<session>:2:34: cannot unify str with i64 [type/Unify] (in the definition of cube)
; refused; the session is unchanged
```

- A redefinition replaces the old one only if every definition that depends on it still
  checks; otherwise nothing changes and the message names the one that broke.
- Ownership and type errors are reported as `fibc check` reports them. A trap ends only
  the evaluated program (`; the program trapped (status 134); the session is unchanged`).
- A session is its source text. Each expression runs as a fresh program, so values
  (an `atom`, a `def`'s result) do not persist from one input to the next.
- Commands: `:type EXPR`, `:doc NAME`, `:time EXPR`, `:load FILE`, `:reload`, `:defs`,
  `:reset`, `:help`, `:quit`. On a terminal there is line editing and a history in
  `~/.fibc_history`; ctrl-c stops a running program, not the session.
- It is a development tool, not a second implementation of the language: a case's
  verdict comes from the compiled run ([ADR 0005](../adr/0005-the-interpreter-is-a-development-tool-not-an-oracle.md)); the design is
  [dev-loop §9](../design/dev-loop.md#9-the-repl-as-built-dv7-2026-10-07).
