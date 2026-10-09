# 0003. New core-form names must not collide with compiler functions

Status: accepted
Date: 2026-10-05
Source: CLAUDE.md, "The self-hosting trap" (found with `splat` and `native-lanes`).

## Context

The compiler is written in fibber and compiled by itself. A core form or builtin the front end knows (`splat`, `native-lanes`,
`has-fma`) is resolved by name before a module's own definitions. If the compiler's own source defines a function with that name, then
once stage 2 compiles the compiler the form captures the call: stage 2 builds stage 3, but they disagree, or stage 3 does not build
at all. The seed (an older compiler) does not know the new form, so the quick gate and a build by the seed look fine; only the full gate
(fixed point) shows it, after a long build.

## Decision

The name of a core form, a primitive form or a conversion is not the name of anything defined in `compiler/` (outside its tests). The
names are read from the tables that make them forms: `core-forms`, `definitions`, `conversions` (`compiler/expand/heads.fib`) and the
string literals of `lw-is-primitive-form` (`compiler/types/lower/call.fib`, the primitive forms). A person adding a form adds it to one
of those tables, and this rule then covers it.

Builtin names (the `BuiltinSig` rows of `compiler/types/builtins.fib`) are held to the same rule with a shrinking allow-list: four
names are defined in `compiler/` today and compile (`trap`, `array-with`, `struct-fields`, `spawn`; the seed knew those builtins before
the definitions were written). New collisions are refused.

## Consequences

- A new form's collision is found in seconds by `make adr`, not at the end of a fixed-point build.
- A compiler function may not be named after a core form even when the form is not yet used in the compiler.
- The core and primitive forms have no collisions today; the builtins have the four listed.

## Governance

```fibber fitness
(rule "no definition in compiler/ has the name of a core, primitive or conversion form"
  (defined-among repo ["compiler/**.fib" "!compiler/tests/**"]
    (into (into (string-atoms-in repo "compiler/expand/heads.fib" "core-forms") (string-atoms-in repo "compiler/expand/heads.fib" "definitions"))
          (into (string-atoms-in repo "compiler/expand/heads.fib" "conversions")
                (string-atoms-in repo "compiler/types/lower/call.fib" "lw-is-primitive-form"))))
  (plant "compiler/emit/zz-plant.fib" "\n(defun splat (x: i64) -> i64 x)\n"))

(rule "no new definition in compiler/ has the name of a builtin (the old ones are allowed, and only those)"
  (allowing (defined-among repo ["compiler/**.fib" "!compiler/tests/**"] (row-strings repo "compiler/types/builtins.fib" "BuiltinSig"))
            ["compiler/driver/header.fib:trap" "compiler/emit/lower/builtins.fib:array-with" "compiler/native/cases/exec.fib:spawn"
             "compiler/expand/types.fib:struct-fields" "compiler/lir/check/aggr.fib:struct-fields" "compiler/types/lower/decl.fib:struct-fields"])
  (plant "compiler/emit/zz-plant.fib" "\n(defun cell (x: i64) -> i64 x)\n"))

(rule "the tables the names are read from are not empty (the rules above would pass on nothing)"
  (into (if (< (count (string-atoms-in repo "compiler/expand/heads.fib" "core-forms")) 20) [(Finding "compiler/expand/heads.fib" 0 "core-forms has fewer than 20 names")] [])
        (into (if (< (count (string-atoms-in repo "compiler/types/lower/call.fib" "lw-is-primitive-form")) 5) [(Finding "compiler/types/lower/call.fib" 0 "lw-is-primitive-form has fewer than 5 names")] [])
              (if (< (count (row-strings repo "compiler/types/builtins.fib" "BuiltinSig")) 100) [(Finding "compiler/types/builtins.fib" 0 "fewer than 100 BuiltinSig rows")] [])))
  (plant-file "compiler/types/builtins.fib" "(ns types.builtins)\n"))
```

### What this does not check

Names that only the *seed* knows and the tree does not yet list; a form added to the front end without a row in the tables above (the
rule reads the tables, not the code that tests a name); collisions in `lib/` (a library function with a form's name is a different
trap: a user program, not the compiler, captures it); a function defined by a macro expansion rather than a top-level `defun`;
`compiler/tests/`. The allow-list entries are `file:name` of a definition: six entries for four names, since `struct-fields` is defined
in three files.
