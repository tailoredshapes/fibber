# fib.lacinia

An evolving native Fibber port of [Walmart Labs' Lacinia](https://github.com/walmartlabs/lacinia).
It provides typed schema construction, variables and input objects, fragment
and directive validation, execution, and GraphQL JSON responses. It implements a subset of Lacinia's API;
it is not a complete GraphQL implementation.

Import it explicitly:

```clojure
(ns main (:use fib.core fib.seq fib.coll fib.print)
  (:require [fib.lacinia :as g]))
```

The [complete example](../../../examples/lacinia.fib) defines a greeting resolver
and a `Person` object, compiles the schema, then executes a named query:

```graphql
query Welcome($name: String!, $details: Boolean = true) {
  greeting: hello(name: $name)
  person @include(if: $details) { ...PersonDetails }
}
fragment PersonDetails on Person { name id __typename }
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
`g/field-value`. Schema enums remain future work.

Input objects use `g/input-object` with a map of `g/argument` definitions. Add
them to the schema's `input-objects` map:

```clojure
(with (g/schema {} query-fields)
  (input-objects {"Filter" (g/input-object {
    "text" (g/argument (g/named "String"))
    "limit" (with (g/argument (g/non-null (g/named "Int")))
      (default-value (some (g/IntValue 10))))})}))
```

Input fields and resolver arguments may refer to input objects with `g/named`. Input objects
support nesting, recursive types, list coercion, and field defaults. Compilation
rejects required singular cycles and recursive default values that cannot
terminate. A nullable or list field can break a type cycle.

## Resolvers and execution

A resolver accepts `(context, arguments, parent)` and returns
`(Result g/GqlValue str)`. Context and parent are `GqlValue`; arguments are
`(Map str g/GqlValue)`. Return `Err` for a handled resolver failure. Resolvers
must satisfy Fibber's `Send` contract: immutable captures and shared atoms are
allowed; capturing a local `Cell` is rejected by the compiler.

`g/execute` takes `(compiled-schema, query, variables, context)` and returns
`GqlResult`, with `data: (Option GqlValue)` and `errors: (Vec GqlError)`.
Supply variables as a map from names without `$` to `GqlValue`. Declared types
and defaults are validated, then supplied values are coerced once. Undeclared
entries in the supplied map are ignored. An omitted nullable variable remains
absent; an explicit `NullValue` remains null. These rules also apply inside
input objects, where omission can activate a field default.

Validation and argument preparation finish before any resolver runs. Repeated compatible fields merge;
conflicting aliases fail validation. Result objects follow selection order.

`g/execute-operation` takes the same four arguments followed by an operation
name; the empty string is not a name and selects nothing (case 7527). Use it for documents with several named query operations. `g/execute`
accepts a single operation, named or anonymous. The entire document is validated,
including unselected operations and skipped fields, before execution.

Named fragments and inline fragments support object type conditions.
`@skip(if: Boolean!)` and `@include(if: Boolean!)` work on fields and fragment
spreads. Conditions are applied before active fields merge. Each named fragment
is visited once per selection set. Fragment cycles,
undefined or unused fragments, duplicate directives, incompatible variable
uses, and undefined or unused variable declarations are request errors.
Conflict validation uses the original argument expressions: distinct variables
cannot make conflicting fields compatible just by holding equal values.

`g/result-json` serializes the response envelope. Request errors omit `data`.
Execution errors retain partial data when possible; failed non-null values
bubble to the nearest nullable parent. A root failure produces `data: null`.
Errors include source locations and response paths, including aliases and list
indices. Error kinds remain available in the typed result.

Variable usage follows GraphQL's type rules. A nullable variable may supply a
non-null argument when the variable has a non-null default or the argument has
a default. Supplying explicit null in that situation produces an
`ArgumentFailure` at the affected response path without calling that field's
resolver. Directive coercion errors propagate from the selection set being
collected; a root selection failure produces `data: null`.

`g/execute-async` accepts the same arguments and returns `(Task g/GqlResult)`.
Use `join` or `@` to obtain the result. It runs a whole query on a native task;
`g/execute-operation-async` additionally accepts an operation name. Fields
within a query currently execute serially. User code that traps still
aborts the process; return `Err` for recoverable failures.

## Supported query subset and limits

Shorthand and named queries, operation selection, variables, input objects,
named/inline fragments, conditional directives, aliases, nested selections,
lists, argument defaults, field merging, and `__typename` are supported.
Strings support standard escapes and Unicode escapes. The lexer tracks UTF-8
byte offsets and one-based source locations, accepts comments and commas, and
rejects malformed numbers and strings.

Documents are limited to 1 MiB and 16,384 tokens. Selection/input nesting and
schema type-wrapper nesting are limited to 64. Fragment expansion and input
validation/coercion each have a 65,536-visit work budget; expansion also limits
depth to 64. Execution allows 65,536 field and completion visits. These budgets cover engine work, not time or memory
spent inside user resolvers. Float literals currently use libc `strtod` after
grammar validation; they require a locale that accepts the decimal point.

Mutations, subscriptions, schema
introspection beyond `__typename`, enums, interfaces, unions,
custom scalars, block strings, SDL/EDN loading, and field-level asynchronous
resolvers are future work. There is no HTTP adapter or performance comparison
with upstream Lacinia yet.

## Verification

```sh
LACINIA_FIBC=./F scripts/test-lacinia.sh
```

The script runs native cases `7500`–`7529`, builds the example, and checks its
response with Python's JSON parser. See the [design and roadmap](../../../docs/design/lacinia.md)
for architecture, provenance, and the next milestones.

For the optional independent comparison, install the pinned test dependency
into a separate Python environment:

```sh
python3 -m venv /tmp/lacinia-oracle
/tmp/lacinia-oracle/bin/pip install -r scripts/tests/lacinia/requirements.txt
LACINIA_FIBC=./F LACINIA_ORACLE_PYTHON=/tmp/lacinia-oracle/bin/python scripts/test-lacinia.sh
```

The comparison checks response data, error presence, execution error paths, and
resolver calls against `graphql-core`. It does not assert identical diagnostic
wording, source locations, or the number of static validation errors.
