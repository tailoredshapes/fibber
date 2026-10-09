# Documentation audit corrections (DOCS-1)

Status: corrections to the October 9 DOCS-0 audit. The original audit remains
dated evidence, including its older scratch compiler's limitations. The current
compiler for validation is built from this checkout with the pinned 0.1.13 seed.

| Findings | Resolution |
|---|---|
| §1.1 release/seed contradictions | README and ROADMAP name 0.1.13; v0.1.3/v0.1.5 passages that describe bootstrap history are explicitly historical. Darwin releases are published since 0.1.7. |
| §1.1 development-loop/editor status | Current tooling guide documents check, serve/client, REPL and LSP; VS Code setup names the actual 0.2.0 package and npm client dependency. |
| §1.1 HTTP/TLS and external dependencies | Explicit HTTPS factory and UNAUDITED client TLS are documented. Localhost SSH URLs are removed from newcomer installation/dependency instructions; external packages need an accessible URL. |
| §1.1 golden/method/performance claims | Maintained stage-2 goldens and current validation replace interpreter-oracle claims. Dated performance records retain their commands and dates on a separate page. |
| §1.1 known failure and release dependencies | Failure 1707 is listed under limits. Contrary to the audit's implication, Make already checks release binary dependencies (`scripts/lib/check-shipped.sh`, `binary.ok`) and builds/runs hello from the packaged layout (`tree.ok`). Those checks are retained, not duplicated. |
| §1.2 command surface | CLI help includes static/link/export/executable emission options and drops retired Rust commands; their explicit refusal now says retired. ADR instructions name the actual standalone `make adr`. CLI reference is generated from current help. |
| §1.3 library omissions | The README summary, guides and generated facade/declaration index include JSON/schema, compression, crypto/TLS, DNS, observability, SQL, Merkle, AD and the other shipped modules. External driver packages are distinguished. |
| §1.4 status headlines | Current status precedes original historical plans; later implementation evidence is named, with unbuilt portions retained. Recommendations are archived with a redirect; superpowers planning records are retained and scoped as history. |
| §1.5 Rust source pointers | Specification chapters scope retired source citations to tag seed-1 and link a retrieval/current-source map. Historical pointers and normative rules are preserved rather than mechanically rewritten into unrelated stage-2 paths. |
| §1.5 planned standard-library names | The table is explicitly a plan, linked to ADR 0023's unbound exceptions and the generated declaration inventory. Builtin signatures come from the maintained Fibber table. |
| §2 release promises | Stability classes, compatibility/deprecation policy, platform promises and 1.0 acceptance criteria are explicitly proposed. They require owner approval; tests do not choose the promise. |
| §3 newcomer gaps | Seven tutorial steps, task guides, CLI/builtin/environment/library/target references, policy/limits, SECURITY, CONTRIBUTING, a changelog summary and an examples index provide a route through the current surface. |
| §4 unchecked documentation | `make doc-examples` checks marked programs, exact stdout, compile/reject examples, verdict/audit cases, relative-link destinations and reference drift. Full/Mac gates include it; quick gates select README/tutorial examples. Examples and a generated project's command sequence have smoke checks. |

## Corrections to the audit itself

`with-view` is **not a spec/code disagreement**: syntax §3.21 explicitly states
it is not a core form and describes the macros over checker markers. The expander
agrees. No language rule or core-form count was changed to resolve this false alarm.

`compiler/fibref/` and `compiler/gen/` contain contract stubs. Directory presence
does not establish completed interpreter, independent heap audit or generator
ports. Their status remains planned, with that limitation stated in the method.

The audit's migration counts disagree (26 versus 62). The checker reports actual
selected counts; it does not repeat the claimed pass rate from the older GPU compiler.

## Check boundaries

The Python checker is the fallback allowed by §4.3. `fib case` deliberately
delegates the twenty Appendix A programs to the existing case harness, including
result, rejection and memory audit headers. Pending/open results are not passes.
Unmarked fences in dated design records are sketches; strict tutorial/guide files
require a marked example and do not silently treat an unmarked Lisp program as checked.

Generated library pages are source-linked **declaration inventories**, including
re-export chains. They do not invent inferred type schemes, per-export “since”
versions or approved stability classes. Full signatures, fields, variants and
protocol methods remain linked to their source declarations. The environment
census includes test/driver constants, labelled as an inventory rather than a
promise that every constant is a supported knob.

Local link checks verify destinations, not remote repository availability or
every Markdown anchor. Native driver/GPU runs, real TLS security auditing,
Mac execution and two-platform newcomer release testing are separate evidence;
the in-tree smoke checks do not claim them. Future 1.0 acceptance work remains
explicit in [the stability proposal](../policy/stability.md).
