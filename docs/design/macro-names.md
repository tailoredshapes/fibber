# Macro names: templates resolve in the macro's module (MACRO-NS)

Status: implemented in stage 2 (`compiler/expand/qualify.fib`, `compiler/expand/macrons.fib`, `compiler/macros/runner.fib`); the rule is
spec/syntax.md §3.16 "Names in a template" and "Phase separation". The Rust seed does not have it (it is frozen).

## Problem

A macro's expansion was resolved in the module that USES the macro (syntax §5). A library macro could name its own module's functions
only if the user `:use`d the library: `(:require [fib.log :as log])` then `(log/info lg "x")` failed with
`unbound name fib.log/enabled?`. The same limit made fib.log, fib.db and fib.sql ask to be `:use`d (so fib.sql's `format` shadowed
fib.print's), and a macro could call at expansion time only the prelude: fib.test's `scenario` is one long function, and fib.autodiff's
`ddx` could not recurse through a helper (docs/design/observability-and-databases.md, "Problems found in the tree").

## Decision

Clojure's syntax-quote, adapted to a language with no global namespace table:

1. **Resolution.** When a `defmacro` body is expanded, every quasiquote template in it has its level-1 symbols resolved in the macro's
   module: a top-level definition of that module, or one it sees exported by a `:use`d or implicit module, becomes `m/x` (m the
   defining module); a :private function, `def`, `extern` or method of the macro's module becomes `(var m/x)` (§3.20); `alias/x` with
   an alias of the macro's module becomes `ns/x`. Unchanged: core forms, names the template binds (an over-approximation: every
   name a binding head, `fn`, `match` clause, `impl` method or `def` in the template binds, and the field of `.`), gensyms, other
   qualified names, types (enum and protocol names) and private structs, variants and macros, a prelude macro the module does not hide
   (`str` in fib.test.core is the prelude's variadic macro, not fib.core's one-argument function), and names the macro's module does not
   define or see (the prelude, the builtins): those resolve at the site as before. Templates built by plain functions are not
   rewritten: only `defmacro` bodies (so the compiler's own code, which has no macros, is untouched).
2. **The site learns the module.** After a user macro runs, every module its expansion names by full name that the site cannot name,
   and that is the macro's module or one it depends on, is required by the site under its own name (alias = ns): in the expander's scope
   at once (a template's `m/mac` is a macro call) and in the spec the checker reads (`expand.modules/end-spec`). Dependency order holds
   because those modules were loaded before the macro's module.
3. **Own module.** In the macro's own module the qualification is taken off at the end of the module (`m/x` is `x`, `(var m/x)` is
   `(var x)`, quoted data untouched), so a module that uses its own macros expands exactly as before (the checker resolves `m/x`
   only through an alias).
4. **Macro-time helpers.** A macro body is checked with the prelude alone first (the old path, no cost); when that fails and its module
   requires or uses modules, the macro-time module is those modules (in dependency order, transitively, without their `defmacro`s, the
   implicit library only where a module names it) plus the macro, in a module of the macro's name that sees them as its module does;
   checked on demand, compiled through the JIT. The macro's own module's functions are still not available (phase separation).

## Costs and limits

- Hygiene is by qualification: a local at the site cannot capture `m/x`. Same-module macros keep the old behaviour (point 3).
- A template fragment (a pattern or binding list built in one template and spliced into another) is not seen as binding: a name in it
  that the macro's module also defines at top level is qualified. Use `gensym` there.
- A private helper in a threading position (`(-> x helper)`) becomes `(var m/helper)`, which the threading macro then treats as a call
  form; write `(helper x)`.
- `ns/x` written by hand in a module's own source now resolves (the end-of-module pass takes `ns/` off).
- Macro-time modules with helpers compile their dependencies once per macro.

## Release step

The compiler's own source has no `defmacro`, so it does not depend on the rule and builds with seed v0.1.7. The libraries migrated here
(fib.log, fib.db, fib.sql, fib.test, fib.autodiff) do depend on it; the compiler does not require them. When a seed that has this rule
is released, nothing else changes; a compiler that wants to use library macros through `:require` needs that seed (bump `SEED`).
