# Lacinia port: query execution and input language

The `fib.lacinia` library provides compiled schemas, typed resolver values,
variables and recursive input objects, fragments, conditional directives,
query validation, result completion, and GraphQL JSON responses.
It uses Fibber's persistent collections and native tasks, with no
Rust compiler changes and no JVM dependency. The
[usage guide](../../lib/fib/lacinia/README.md) and
[runnable example](../../examples/lacinia.fib) describe the current API.

## Architecture

| Modules | Responsibility |
| --- | --- |
| `types`, `syntax`, `schema/*`, `schema`, `coerce` | Schema/value/document models; schema and scalar validation |
| `lex/*`, `lexer` | Bounded tokenization, UTF-8 positions, string escapes, number grammar |
| `parse/*`, `parser` | Operations, variable declarations, input expressions, fragments and directives |
| `variables`, `input/*` | Variable type compatibility; input expression validation and coercion |
| `plan/*`, `plan` | Whole-document validation, bounded expansion, conflicts, conditional collection and argument preparation |
| `execute/state`, `executor` | Per-query errors, completion budget, serial resolution, non-null propagation |
| `engine` | Synchronous execution and native-task execution of independent queries |
| `json` | Ordered response data and GraphQL error envelopes |

Compiled schemas and prepared fields are immutable. Each execution owns its
error accumulator and visit counters; there is no global mutable executor state.
Resolvers carry the compiler's `Send` requirement so a compiled schema can be
used by independent native tasks. Query-level concurrency is implemented;
parallel scheduling of fields is a later milestone.

`ObjectValue` stores a map for lookup and a vector for response order. This
preserves requested field order without requiring a new collection primitive.
The cursor maintains both character and byte offsets, avoiding repeated
prefix traversal when extracting tokens. Prepared fields cache coerced
arguments and resolved schema descriptors for the duration of one request.

Input expressions retain variable references through static conflict checks;
variable substitution must not turn distinct expressions into compatible
arguments. Static validation visits all operations and all selections before
runtime conditions are considered. Each operation has its own variable scope,
including uses reached through shared fragments. Recursive input coercion
preserves omitted members and explicit null separately; schema defaults fill
only omitted members.

Fragment expansion carries condition groups down to collected fields and
retains the fragment recursion path across nested object selections. Expansion
is bounded by depth and visit count, including repeated acyclic spreads.
Collection evaluates `@skip` before `@include` and honors ancestor conditions,
then merges active fields in source order. Named fragments carry occurrence
identities so collection visits each fragment name once per selection set,
while retaining every field within the selected occurrence. Conditional fields may leave an
object with an empty projection, which completes as `{}`.

Request failures have no data. Execution failures include partial data or a
null root and attach locations and alias/index paths. A handled resolver error
is returned as `Err`; an unhandled Fibber trap still aborts the process.
Non-null propagation does not duplicate an existing resolver/completion error.

Nullable variables at non-null locations follow the GraphQL default exception:
a non-null variable default or a location default permits the use. A supplied
null can then cause an execution argument error. Prepared fields retain these
errors and prevent the affected resolver from running. Errors while collecting
nested selection sets null their containing objects and carry those objects'
response paths; list completion extends paths with each affected index.
An error in the root collection produces `data: null`.

Engine resource budgets bound parsing, expansion, input processing and
completion, but do not preempt user resolvers. Native float parsing currently
depends on libc's decimal locale.

## Executable acceptance criteria

Cases were introduced before implementation: the initial executor cases failed
on the absent module, and the query-language cases failed on the absent input
object API. Cases `7500`–`7529` exercise the following behaviors:

| Case | Behavior |
| --- | --- |
| 7500 | Valid recursive object schemas; invalid names, references, wrappers, defaults |
| 7501 | Aliases, nested objects, lists, ID conversion, selection order, `__typename` |
| 7502 | Argument defaults/coercion and validation before resolver side effects |
| 7503 | Resolver errors, partial data, root/list non-null propagation, error paths |
| 7504 | Malformed syntax/numbers/escapes, Unicode, unsupported query features |
| 7505 | Compatible field merging and conflicting aliases/arguments |
| 7506 | Scalar/list coercion, omitted versus null input, non-finite output rejection |
| 7507 | Independent executions on native tasks with shared atomic resolver state |
| 7508 | Compile-time rejection of a resolver capturing a local mutable cell |
| 7509 | Document/depth/completion limits and termination of oversized list completion |
| 7510 | Supplied variables, variable/argument defaults, omitted versus explicit null |
| 7511 | Declarations, usage compatibility, defaults and supplied-value validation before callbacks |
| 7512 | Recursive input objects, nested variables, singleton lists and input field defaults |
| 7513 | Invalid input shapes, unknown members, invalid defaults and non-terminating input cycles |
| 7514 | Named/inline fragments, nested expansion, repeated fields and response order |
| 7515 | Undefined/unused/inapplicable fragments, cycles across object fields, operation variable scopes |
| 7516 | Conditional collection before merging, inherited conditions and empty projections |
| 7517 | Validation of skipped fields, directive arguments/locations and raw expression conflicts |
| 7518 | Named operation selection, whole-document validation and native-task operation selection |
| 7519 | Variable type depth, fragment chain depth and exponential acyclic expansion budgets |
| 7520 | Dynamic argument/directive failures, partial data, response paths and skip short-circuiting |
| 7521 | Shared input schema graphs, large supplied-variable work limits and supplied-value depth limits |
| 7522 | Named fragment visit history, skipped first spreads and separate nested selection scopes |
| 7523 | Scalar and list variables, explicit null, singleton-list coercion, declaration and usage errors with no resolver calls |
| 7524 | Variable defaults against argument defaults, null defaults reaching non-null arguments (a field error at the path, as 7520), invalid defaults |
| 7525 | Named operation selection with resolver-call counts, whole-document validation and native-task selection |
| 7526 | A variable default inside an input object inside a conditional inline fragment, including a skipped fragment |
| 7527 | The same variable name declared with different types in two operations: only the selected operation's declaration applies; `""` is not an operation name |
| 7528 | Undefined variables inside skipped fragments are request errors; a variable used only in a skipped fragment counts as used |
| 7529 | Variable types nested 66 deep through `[` and `!` and malformed or non-constant defaults |

Run `LACINIA_FIBC=./F scripts/test-lacinia.sh`. Accepted native cases require
clean memory audits; the rejected case checks its compiler diagnostic. The
script also builds the example and independently parses its response as JSON.
With `LACINIA_ORACLE_PYTHON` set to a Python environment containing the pinned
`scripts/tests/lacinia/requirements.txt`, it runs the independent comparison in
`scripts/tests/lacinia/compare.py`. The reference schema and resolver values
match the native fixture. Comparisons cover response data, request versus
execution errors, execution error paths, and callback counts; diagnostic wording,
source positions and static error multiplicity are excluded.

These checks verify the documented subset. The complete upstream Lacinia suite
and comparative performance have not been measured. Some older Lacinia tests
accept nullable declarations at non-null locations based on supplied values;
this port applies GraphQL's static variable usage rules instead.

## Next milestones

1. Extend schemas with enums, custom scalars, interfaces, and unions.
   Add introspection and a compatible schema loading format.
2. Add mutations with serial root execution. Define an asynchronous resolver
   result and cancellation before adding parallel field completion or batching.
3. Connect to `fib.http.server` through a GraphQL request/response adapter;
   cover malformed request envelopes and transport behavior separately from
   query execution. Subscriptions need a separate streaming lifecycle.
4. Compare equivalent fixtures against upstream Lacinia, then measure parser,
   planning, completion, and resolver costs separately. Optimize from those
   measurements; consider reusable validated plans before SIMD specialization.

Each step follows red, green, refactor. Performance claims require matched
queries, results, error behavior, and reproducible measurements.

## Upstream reference and licensing

Reference source was inspected at Walmart Labs Lacinia commit
[`bd6a630b07f8af0822e55c990f369b46ce4d52de`](https://github.com/walmartlabs/lacinia/tree/bd6a630b07f8af0822e55c990f369b46ce4d52de).
Its [execution API](https://github.com/walmartlabs/lacinia/blob/bd6a630b07f8af0822e55c990f369b46ce4d52de/src/com/walmartlabs/lacinia.clj),
schema compiler, grammar, executor/error tests, and variable/input-object/
directive tests informed these milestones.
[GraphQL September 2025](https://spec.graphql.org/September2025/) is the language
and response-semantics reference, subject to the explicit subset above.

Lacinia is [Apache-2.0 licensed](https://github.com/walmartlabs/lacinia/blob/bd6a630b07f8af0822e55c990f369b46ce4d52de/LICENSE).
The current Fibber implementation and cases are original code; no upstream
source files were copied. Any later copying or adaptation of upstream code
must retain its license and required notices rather than assume Fibber's root
license covers those files.
