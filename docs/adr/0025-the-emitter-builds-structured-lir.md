# 0025. The emitter builds structured lIR

Status: accepted
Date: 2026-10-12
Source: the owner's decision to start compiler algorithm work with a structured emitter, before changing copying or dense tables.

## Context

The emitter assembled instructions and modules as strings. Native compilation
then read those strings back into `lir.ast`. Constant evaluation cached parsed
pieces, but still serialized each generated function, constant, and session
entry. Async lowering and reachability also inspected and rewrote instruction
text. This made representation changes harder to check independently.

## Decision

Generated operands, instructions, functions, globals, and module sections use
`lir.ast` and `lir.types` throughout emission. The native backend, constant
sessions, macro runner, server, and REPL receive `LModule` values directly.
Native compilation still verifies those modules before LLVM or cache access.

The maintained runtime under `rt/` and external `.lir` files remain source
inputs and use the reader at their boundary. `emit`, diagnostic dumps, the C
backend interface, and LIRO serialization may print trees. Generated code is
never printed and read back to obtain its production representation.
`FIB_AST_VERIFY` retains an optional independent reader check of printed output;
its planted missing-runtime fault must be detected.

Async operand substitution, phi edges, branch destinations, and symbol
reachability use node variants. Quoted text does not identify a local operand
or a global dependency. Rewrites preserve types, positions, calling conventions,
atomic orderings, and intrinsic flags.

Copying primitives and dense type tables are separate changes. This decision
provides the representation in which their effects can be tested safely.

## Consequences

- `fibc emit` uses the established LIR printer. Golden changes in spacing are
  reviewed against the old compiler's ordered instructions and operands.
- Emitter tests construct nodes and retain independent golden expectations.
  Checks that count operations or identify blocks inspect external emitted LIR
  as trees, so line wrapping cannot change the answer.
- Native verification remains mandatory. A malformed generated module is an
  error before code generation or reuse of compiled objects.
- This change does not promise a particular compilation speedup. Phase timings,
  peak memory, fixed points, native cases, and platform gates supply evidence.

## Governance

```fibber fitness
(rule "the frontend and macro runner hand generated modules to AST entry points"
  (into (must-contain repo "compiler/driver/front.fib" "ec/compile-executable-ast")
        (into (must-contain repo "compiler/driver/commands.fib" "native/run-module")
              (must-contain repo "compiler/macros/runner.fib" "jit/jit-add-module")))
  (plant-file "compiler/driver/front.fib" "(ns driver.front)\n")
  (plant-file "compiler/macros/runner.fib" "(ns macros.runner)\n"))

(rule "builder operands and generated functions are structured values"
  (into (must-contain repo "compiler/emit/value.fib" "VVal expr: LExpr")
        (into (must-contain repo "compiler/emit/program.fib" "funcs: (Cell (Vec la/LFunction))")
              (must-contain repo "compiler/emit/ir.fib" "instr: LExpr")))
  (plant-file "compiler/emit/value.fib" "(ns emit.value)\n"))

(rule "generated native modules undergo the whole-module checker"
  (must-contain repo "compiler/native/pipeline.fib" "(check/check m)")
  (plant-file "compiler/native/pipeline.fib" "(ns native.pipeline)\n"))
```

These checks pin the architecture's entry points. The emitter unit suites and
native gates check their behavior; the fitness rules alone do not prove every
rewrite or ownership operation correct.
