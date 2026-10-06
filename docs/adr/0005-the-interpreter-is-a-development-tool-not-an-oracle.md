# 0005. The interpreter is a development tool, not an oracle

Status: accepted
Date: 2026-10-04
Source: docs/design/decisions-2026-10-04.md, "Amendment, later on 2026-10-04: the interpreter is a development tool"; docs/design/dev-loop.md.

## Context

spec/method.md rule 6 made the interpreter the oracle: every case and generated program ran in both the reference interpreter and the
compiled code, and the two had to agree before a feature was done. That tied the compiler to a second implementation whose speed,
completeness and bugs were the project's, and it made the interpreter's design a constraint (independence, a divergence table, audit
fidelity). The owner decided it is not worth it.

## Decision

The interpreter (`compiler/fibref/`, the port of `fibref`) is a development tool: it helps an agent or a person get from an edit to a
result quickly. It is not the oracle of anything. Nothing in the compiler's path, the gate, the case harness or CI requires it or
compares against it; the compiled program, the specs, the cases and the recorded goldens are what judge.

## Consequences

- The interpreter may be slow, partial or missing without blocking a release; it may also diverge from stage 2 and that is a bug in
  the interpreter, found by a person, not a gate failure.
- Rule 6 of spec/method.md described the old arrangement; it was amended on 2026-10-06 (the owner agreed the edit) to say the compiler is
  checked by cases, the fixed point, goldens, specs and ADRs. This ADR records the decision and checks only what can be checked about
  the tree.
- The development loop (`docs/design/dev-loop.md`) is where interpreter work is judged: by how fast it gets an agent to a result.

## Governance

```fibber fitness
(rule "the owner's decision is on record (the sentence this ADR rests on)"
  (must-contain repo "docs/design/decisions-2026-10-04.md" "The interpreter is a development tool")
  (plant-file "docs/design/decisions-2026-10-04.md" "# nothing\n"))

(rule "no module outside compiler/fibref/ requires an interpreter module"
  (requires-into repo ["compiler/**.fib" "lib/**.fib" "!compiler/fibref/**" "!compiler/fibref.fib" "!compiler/tests/**"] "fibref." [])
  (plant-file "compiler/driver/zz-plant.fib" "(ns driver.zz-plant (:require [fibref.eval :as e]))\n"))

(rule "the gate, the batch integrator and CI never run an interpreter"
  (grep-live repo ["scripts/gate.sh" "scripts/batch.sh" "scripts/ci-stage2.sh" "scripts/lib/*.sh" ".gitlab-ci.yml" ".github/workflows/*.yml"] "fibref")
  (plant "scripts/gate.sh" "\n\"$F\" fibref cases cases/ownership\n"))

(rule "no script compares compiled results with an interpreter's"
  (present repo ["scripts/**/*compare-interp*" "scripts/**/*interp-vs*" "compiler/tests/**/*compare-interp*" "compiler/tests/**/*oracle-interp*"])
  (plant-file "scripts/compare-interp.sh" "#!/bin/bash\n"))
```

### What this does not check

That the wording of spec/method.md rule 6 stays as amended (prose, not checkable here). That the interpreter
works, is fast or exists. A comparison written in a language other than shell or fibber, or under a name the globs do not list.
