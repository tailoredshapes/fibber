# lIR audit (from liar)

Findings from an adversarial audit of liar's lIR layer, kept here as
the starting test list for hardening lIR (spec/method.md, rule 7).
Every `.lir` file here was run against liar's `lair` at commit
`7f4734c`; the outcomes are recorded in the session that produced them
and must be re-established, not trusted, when lIR is brought over.

| Dir | Contents |
|-----|----------|
| `t/` | Valid programs. `run.sh f.lir` verifies, builds and runs one. |
| `tc/` | Invalid programs `lair` accepted without `--verify`. |
| `borrow/` | Programs the "safety" layer (ADR 021) should reject and does not. |
| `cmp/` | Expressions compared between the JIT (`lir`) and AOT (`lair`). |
| `probe/` | A driver for lir-core's checker, borrow checker and parser. |
| `fuzz2.txt`, `deepadd.txt` | Malformed and pathological inputs. |

Headline findings: `lair` runs no type checker; `own`/`drop` compile
to a stack slot and nothing; string globals segfault on load; `fence`
is single-thread only; `indirect-call` types every argument as `ptr`;
`tailcall` is `tail`, not `musttail`.
