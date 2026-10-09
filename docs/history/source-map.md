# Historical source references

Rust citations in the specification refer to tag `seed-1`. Retrieve a cited
file without restoring Rust into the active tree:

```sh
git show seed-1:crates/fibref/src/types/builtins.rs
```

| Historical area | Current source |
|---|---|
| fibref reader, expansion, types, ownership | `compiler/syntax`, `expand`, `types`, `own` |
| fibc commands, lowering, emission | `compiler/driver`, `emit` |
| lair/lir checking and LLVM lowering | `compiler/lir`, `native`, `llvm` |
| fibref evaluator/audited heap | retired; `compiler/fibref` is a planned port, not an oracle |
| fibgen random programs | retired; `compiler/gen` contains port contracts |
| builtin signature table | `compiler/types/builtins.fib`, maintained in Fibber |

See [Rust retirement](../rust-legacy.md) for losses and recovery instructions.
Historical measurements retain their original tools, dates and assumptions.
The [recommendations](chatgpt-recommendations.md) are an external-model proposal.
`docs/superpowers/specs` contains retained dated planning records; directory
presence does not imply that every plan was approved or completed.
