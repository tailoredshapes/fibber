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

## Re-established in M3 (2026-09-28)

Every file here was run again against liar's `lair` at `7f4734c`
(`/home/user/liar/target/release/lair`, built 2026-09-26) before lIR was
ported, and each became a case in `cases/lir/audit/` whose header
records what liar did. Summary:

| Finding | On liar | In fibber's lIR |
|---|---|---|
| no type checker in `lair` | reproduced: without `--verify`, 16 of the 28 `tc/` programs built (e.g. `tc/call_argcount.lir` exit 169, `tc/ret_void_in_i32.lir` exit 48); with `--verify`, 7 still built, 6 of them wrong (`dup_fn`, `i8_overflow_lit` exit 44, `mismatch` exit 3, `phi_not_first` exit 9, `store_type` segfault, `use_nondominating` exit 5; `mismatch2` is in fact valid, exit 5 is right); `brcond_double.lir` panicked in codegen; `mismatch3.lir`, `phi_wrong.lir` hit `LLVM ERROR: Cannot emit physreg copy` | every `tc/` program rejected by the checker with a message (`cases/lir/audit/tc-*.lir`) |
| `own`/`drop` compile to a stack slot and nothing | reproduced: `borrow/uaf.lir` printed `after drop: 42`; `borrow/rc.lir` read freed memory | the ADR 021 layer is removed (`unknown instruction`) |
| string globals segfault on load | reproduced: `t/gstr.lir` segmentation fault | fixed: `t-gstr.lir` prints `gstr=hi-global` |
| `fence` is single-thread only | reproduced: `t/fence_mono.lir`'s verifier error shows `fence syncscope("singlethread")` | fixed: system scope by default, `singlethread` on request; `monotonic` fence rejected by the checker |
| `indirect-call` types every argument as `ptr` | reproduced: `t/icall.lir`, `t/icall2.lir` fail LLVM's verifier | fixed: the call carries `(fn R (T..))`; the old form is rejected |
| `tailcall` is `tail`, not `musttail` | reproduced: `t/tail_many.lir` segfaults (stack overflow) at depth 10^7 | fixed: `musttail`; under `ccc` differing prototypes are rejected, under `tailcc` they run (`t-tail-many-tailcc.lir`) |
| `constant` documented, not parsed | reproduced: `t/const.lir` parse error | fixed |
| top-level expressions silently ignored | new: `fuzz2.txt` lines such as `(i64 99999999999999999999999999)` passed parsing, then failed only at link time for lack of `main` | rejected: `expected a top-level form` |
| deep nesting | reproduced: `deepadd.txt` overflowed liar's parser stack | rejected at depth 512 (`crates/lir/tests/audit_inputs.rs`) |
| JIT and AOT agree (`cmp/`) | reproduced: all 26 expressions agreed | both paths run every case |
