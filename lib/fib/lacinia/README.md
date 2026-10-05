# fib.lacinia

An initial native Fibber port of [Walmart Labs' Lacinia](https://github.com/walmartlabs/lacinia).
This milestone provides typed schema construction, query validation, synchronous
execution, and GraphQL JSON responses. It implements a subset of Lacinia's API;
it is not a complete GraphQL implementation.

Import it explicitly:

```clojure
(ns main (:use fib.core fib.seq fib.coll fib.print)
  (:require [fib.lacinia :as g]))
```

The [complete example](../../../examples/lacinia.fib) defines a greeting resolver
and a `Person` object, compiles the schema, then executes a named query:

```graphql
query Welcome {
  greeting: hello(name: "Fibber")
  person { name id __typename }
}
```

```sh
./F build examples/lacinia.fib -I lib -o /tmp/fibber-lacinia
/tmp/fibber-lacinia
```

```json
{"data":{"greeting":"Hello, Fibber","person":{"name":"Ada","id":"7","__typename":"Person"}}}
```

## Schema and values

`g/schema` takes an object-type map and a query-field map. `g/object-type` takes
a field map. Fields use `g/field` for lookup from the parent object or
`g/resolving` for an explicit resolver. Type constructors are `g/named`,
`g/list-of`, and `g/non-null`; the built-in scalars are `Int`, `Float`, `String`,
`Boolean`, and `ID`.

`g/argument` creates an argument definition. Set a field's `arguments` map with
`with`; set an argument's optional `default-value` to supply a default. Missing
nullable arguments remain absent from the resolver's map. Explicit `null`
remains present, and defaults apply only to omitted arguments. List inputs
support singleton coercion. GraphQL `Int` is restricted to signed 32-bit values.

`g/compile-schema` returns `(Result g/GqlCompiledSchema (Vec g/GqlError))`.
Compile once and reuse the immutable schema. Compilation rejects invalid or
reserved names, unknown types, invalid wrappers and defaults, and empty objects
or query roots. Object types may refer to one another recursively.

Values use the `GqlValue` enum: `NullValue`, `BooleanValue`, `IntValue`,
`FloatValue`, `StringValue`, `EnumValue`, `ListValue`, and `ObjectValue`.
Construct application objects with `g/object`; retrieve a member with
`g/field-value`. The enum includes representations for future features;
schema enums and input objects are not supported yet.

## Resolvers and execution

A resolver accepts `(context, arguments, parent)` and returns
`(Result g/GqlValue str)`. Context and parent are `GqlValue`; arguments are
`(Map str g/GqlValue)`. Return `Err` for a handled resolver failure. Resolvers
must satisfy Fibber's `Send` contract: immutable captures and shared atoms are
allowed; capturing a local `Cell` is rejected by the compiler.

`g/execute` takes `(compiled-schema, query, variables, context)` and returns
`GqlResult`, with `data: (Option GqlValue)` and `errors: (Vec GqlError)`.
Pass supplied variable values in the variables map. Validation and argument
coercion finish before any resolver runs. Repeated compatible fields merge;
conflicting aliases fail validation. Result objects follow selection order.

Variables declare built-in scalar or list types, for example:

```graphql
query Welcome($name: String! = "Fibber") { hello(name: $name) }
```

Supply `{"name" (g/StringValue "Ada")}` to override that default. Omitted
variables use their declaration default; an omitted variable without a default
leaves its argument absent so the argument's default can apply. Explicit null
never selects a default. Variable types are checked at every use, independently
of the supplied value; undefined and unused variables fail validation.

`g/execute` automatically chooses the query when the document has one operation.
For multiple operations, use `g/execute-operation` with
`(compiled-schema, query, variables, context, operation-name)`.
`g/execute-operation-async` takes the same arguments and returns a task.
Operation names must be unique, and anonymous operations must stand alone.
The whole document is validated before execution, including unselected queries;
only the selected query's supplied variables are coerced and resolvers invoked.

`g/result-json` serializes the response envelope. Request errors omit `data`.
Execution errors retain partial data when possible; failed non-null values
bubble to the nearest nullable parent. A root failure produces `data: null`.
Errors include source locations and response paths, including aliases and list
indices. Error kinds remain available in the typed result.

`g/execute-async` accepts the same arguments and returns `(Task g/GqlResult)`.
Use `join` or `@` to obtain the result. It runs a whole query on a native task;
fields within a query currently execute serially. User code that traps still
aborts the process; return `Err` for recoverable failures.

## Supported query subset and limits

Shorthand and named queries, variable definitions and defaults, operation
selection, aliases, nested selections, lists, scalar literals, argument defaults,
field merging, and `__typename` are supported.
Strings support standard escapes and Unicode escapes. The lexer tracks UTF-8
byte offsets and one-based source locations, accepts comments and commas, and
rejects malformed numbers and strings.

Documents are limited to 1 MiB and 16,384 tokens. Selection/input nesting and
schema type-wrapper nesting are limited to 64. Execution allows 65,536 field
and completion visits. These budgets cover engine work, not time or memory
spent inside user resolvers. Float literals currently use libc `strtod` after
grammar validation; they require a locale that accepts the decimal point.

Fragments, directives, mutations, subscriptions, schema
introspection beyond `__typename`, enums, input objects, interfaces, unions,
custom scalars, block strings, SDL/EDN loading, and field-level asynchronous
resolvers are future work. There is no HTTP adapter or performance comparison
with upstream Lacinia yet.

## Verification

```sh
LACINIA_FIBC=./F scripts/test-lacinia.sh
```

The script runs native cases `7500`–`7512`, builds the example, and checks its
response with Python's JSON parser. See the [design and roadmap](../../../docs/design/lacinia.md)
for architecture, provenance, and the next milestones.
