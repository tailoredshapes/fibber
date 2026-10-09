---
examples: required
---

# Ship a program

Start with a normal optimized executable:

```sh
FIB_TARGET_CPU=x86-64-v3 fibc build hello.fib -O 2 -o hello
```

That CPU setting is for x86-64 deployment. `fibc build --static` needs musl
pieces (or a release packaged with them). It produces a Linux binary suitable
for a scratch image; provision certificates and configuration yourself.
`--emit c` emits a translation unit and `--via c` builds through `cc`.
WASI builds need a wasm linker and WASI sysroot. See
[targets and deployment](../guide/targets-and-deployment.md) before cross-building.
The program below can be built as `hello` and exits successfully.

```fib run
(defun main () -> i64 (do (println "hello from fibber") 0))
```

```text out
hello from fibber
0
```
