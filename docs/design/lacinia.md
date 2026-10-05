# Lacinia port: initial query executor

The first milestone is an explicit `fib.lacinia` library with compiled schemas,
typed resolver values, query validation, result completion, and GraphQL JSON
responses. It uses Fibber's persistent collections and native tasks, with no
Rust compiler changes and no JVM dependency. The
[usage guide](../../lib/fib/lacinia/README.md) and
[runnable example](../../examples/lacinia.fib) describe the current API.

## Architecture

| Modules | Responsibility |
| --- | --- |
| `types`, `schema`, `coerce` | Typed schema/value models; schema and scalar/input validation |
| `lex/*`, `lexer` | Bounded tokenization, UTF-8 positions, string escapes, number grammar |
| `parse/*`, `parser` | Input expressions, variable definitions/defaults, and query documents |
| `inputs`, `variables`, `document`, `plan` | Variable use/type validation, operation selection, field merging, and coercion before resolution |
| `execute/state`, `executor` | Per-query errors, completion budget, serial resolution, non-null propagation |
| `engine` | Synchronous execution and native-task execution of independent queries |
| `json` | Ordered response data and GraphQL error envelopes |

Compiled schemas and prepared fields are immutable. Each execution owns its
error accumulator and visit counter; there is no global mutable executor state.
Resolvers carry the compiler's `Send` requirement so a compiled schema can be
used by independent native tasks. Query-level concurrency is implemented;
parallel scheduling of fields is a later milestone.

`ObjectValue` stores a map for lookup and a vector for response order. This
preserves requested field order without requiring a new collection primitive.
The cursor maintains both character and byte offsets, avoiding repeated
prefix traversal when extracting tokens. Prepared fields cache coerced
arguments and resolved schema descriptors for the duration of one request.

Request failures have no data. Execution failures include partial data or a
null root and attach locations and alias/index paths. A handled resolver error
is returned as `Err`; an unhandled Fibber trap still aborts the process.
Non-null propagation does not duplicate an existing resolver/completion error.
Engine resource budgets bound parsing and completion, but do not preempt user
resolvers. Native float parsing currently depends on libc's decimal locale.

## Executable acceptance criteria

The initial cases were introduced before implementation and failed on the absent
module. The operation-selection case failed on the absent `execute-operation`
API before this extension. Cases `7500`–`7512` exercise the following behaviors:

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
| 7510 | Variable coercion, omitted/null inputs, list expressions, type checking, native tasks |
| 7511 | Variable/argument defaults, supplied-value precedence, constant/default validation |
| 7512 | Operation selection, name uniqueness, whole-document validation, native tasks |

Run `LACINIA_FIBC=./F scripts/test-lacinia.sh`. Accepted native cases require
clean memory audits; the rejected case checks its compiler diagnostic. The
script also builds the example and independently parses its response as JSON.
This verifies the stated subset; upstream suite parity and comparative
performance have not been measured.

## Variables, defaults, and operation selection

Input expressions (`GqlInput`) are separate from resolver values (`GqlValue`),
so variables preserve their identity during field merging. Validation checks
all operations, including variable declarations, defaults, usage compatibility,
and unused variables, before coercing the selected operation's supplied values.
Nullable variables can be used at non-null locations when a non-null variable
default or a location default permits it; explicit null still fails coercion
at a non-null location. Missing variables inside list literals become null list
items and must satisfy the element type.

`execute` keeps its four-argument API and chooses a sole operation automatically.
`execute-operation` and `execute-operation-async` add a final operation-name
argument. Documents with multiple operations require a name. Unselected queries
are validated but their variables are not coerced and their resolvers do not run.
Requests still reject fragments, directives, mutation/subscription operations,
and types outside the documented scalar/list subset.

## Next milestones

1. Add named/inline fragments and `@skip`/`@include`; validate fragment cycles,
   merge compatibility, and selections before invoking resolvers.
2. Extend schemas with enums, input objects, custom scalars, interfaces, and
   unions. Add introspection and a compatible schema loading format.
3. Add mutations with serial root execution. Define an asynchronous resolver
   result and cancellation before adding parallel field completion or batching.
4. Connect to `fib.http.server` through a GraphQL request/response adapter;
   cover malformed request envelopes and transport behavior separately from
   query execution. Subscriptions need a separate streaming lifecycle.
5. Compare equivalent fixtures against upstream Lacinia, then measure parser,
   planning, completion, and resolver costs separately. Optimize from those
   measurements; consider reusable validated plans before SIMD specialization.

Each step follows red, green, refactor. Performance claims require matched
queries, results, error behavior, and reproducible measurements.

## Upstream reference and licensing

Reference source was inspected at Walmart Labs Lacinia commit
[`bd6a630b07f8af0822e55c990f369b46ce4d52de`](https://github.com/walmartlabs/lacinia/tree/bd6a630b07f8af0822e55c990f369b46ce4d52de).
Its [execution API](https://github.com/walmartlabs/lacinia/blob/bd6a630b07f8af0822e55c990f369b46ce4d52de/src/com/walmartlabs/lacinia.clj),
schema compiler, grammar, and executor/error tests informed this milestone.
[GraphQL September 2025](https://spec.graphql.org/September2025/) is the language
and response-semantics reference, subject to the explicit subset above.

Lacinia is [Apache-2.0 licensed](https://github.com/walmartlabs/lacinia/blob/bd6a630b07f8af0822e55c990f369b46ce4d52de/LICENSE).
The current Fibber implementation and cases are original code; no upstream
source files were copied. Any later copying or adaptation of upstream code
must retain its license and required notices rather than assume Fibber's root
license covers those files.
