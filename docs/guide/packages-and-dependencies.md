---
examples: required
---

# Packages and dependencies

A project has `deps.fib` with `:paths`, `:deps` and optional `:aliases`.
`fibc deps add NAME --git URL --tag TAG` adds a remote dependency; replace
URL with an accessible repository, not a localhost SSH address. Local roots
use `:local/root`. Commit `deps.lock`: it pins commit and git tree IDs. `--locked`
refuses stale locks, `--offline` avoids fetching, and `--frozen` combines both.
Publish a library by committing its source, `deps.fib`, tests and API docs,
then tagging the intended commit. Version promises are currently pre-1.0;
see [stability](../policy/stability.md). Macros execute at build time, so a
locked dependency still needs to be trusted. Full format:
[package design](../design/packages.md).

```fib run
(ns main (:require [fib.string :as text]))
(defun main () -> i64 (do (println (text/join ", " ["tea" "cup"])) 0))
```

```text out
tea, cup
0
```

## Projects and dependencies

A project is a directory with a `deps.fib`; libraries come from git commits (or local directories). `fibc new`
makes one:

```
fibc new hello                  # hello/deps.fib, src/hello/core.fib, src/main.fib, specs/hello-spec.fib
cd hello
fibc deps add acme/util --git https://github.com/acme/util.git --tag v1.2.0
fibc run src/main.fib           # resolves, fetches, writes deps.lock; no -I needed
fibc test                       # runs specs/*-spec.fib with the project's roots
fibc deps tree
```

`deps.fib` is fibber data in the shape of Clojure's `deps.edn`:

```text
{:name "hello" :version "0.1.0"
 :paths ["src"]
 :deps {acme/util {:git/url "https://github.com/acme/util.git" :git/tag "v1.2.0"}
        acme/json {:git/url "https://github.com/acme/json.git" :git/sha "<40-hex commit>"}
        mine/x    {:local/root "../x"}}
 :aliases {:test {:extra-paths ["specs"]}}}
```

Every command that compiles (`run`, `build`, `test`, `emit`, `explain`) finds `deps.fib` in the working directory or
above, resolves it against `deps.lock` (commit it: it pins every commit and its git tree id) and adds the project's
`:paths` and each library's to the module roots. The same library wanted at two commits is an error naming who
asked for what (choose with `:override`); a tag that moved since it was locked is refused until `fibc deps update
NAME`; two libraries defining one module are refused. `--locked` refuses a stale lock, `--offline` never fetches,
`--frozen` is both. Checkouts are cached read-only under `~/.cache/fibber/git` (`$FIBBER_HOME`) and verified on
every build. **A dependency's macros run at compile time** (like Rust's proc-macros): depend only on code you
trust. Resolution itself runs only `git`. Design and limits: docs/design/packages.md.
