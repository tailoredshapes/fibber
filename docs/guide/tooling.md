---
examples: required
---

# Tooling

`fibc run -O 0` uses fast development code generation; `build` defaults to `-O 2`.
`check MODULE..` checks every definition in named modules and dependencies;
exit 0 means checked, 3 means rejected and 2 means unloadable. `explain FILE`
shows ownership decisions. `emit` prints lIR; `emit-dump` is a compiler tool.
`serve` retains front-end state over a Unix socket, `--server` sends requests
with local fallback, and `repl` keeps definitions in a session. See
[the dev-loop record](../design/dev-loop.md) for protocol and limits.
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
