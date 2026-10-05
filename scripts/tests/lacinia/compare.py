#!/usr/bin/env python3
"""Compare the documented query subset with a pinned independent GraphQL engine."""
import itertools
import json
import subprocess
import sys

from graphql import build_schema, get_operation_ast, graphql_sync, parse, validate
from graphql.execution.values import get_variable_values


SDL = '''
input Filter { text: String = "base", limit: Int! = 10,
               terms: [String!], nested: Filter }
input Emptyable { value: String }
type Person { name: String, id: ID, friend: Person }
type Query {
  echo(text: String = "default"): String
  required(text: String!): String
  strict(text: String!): String!
  present(text: String): Boolean
  filter(input: Filter): String
  empty(input: Emptyable): String
  count: Int
  person: Person
  emptyPerson: Person
  people: [Person]
  requiredPeople: [Person!]
}
'''
FIXTURES = {
    "none": {}, "string": {"x": "supplied"}, "null": {"x": None},
    "int": {"x": 3}, "true": {"x": True}, "false": {"x": False},
    "object": {"input": {"limit": 3}},
    "nested": {"terms": "one", "text": "leaf"},
    "list": {"x": ["one", None]}, "extra": {"unused": 42},
}


def reference(query, fixture, operation):
    schema = build_schema(SDL)
    calls = 0

    def resolver(name):
        def resolve(parent, info, **arguments):
            nonlocal calls
            calls += 1
            if name in ("echo", "required", "strict"):
                return arguments.get("text")
            if name == "present":
                return "text" in arguments
            if name in ("filter", "empty"):
                return json.dumps(arguments, sort_keys=True, separators=(",", ":"))
            if name == "count":
                return 0
            person = {"name": "Ada", "id": 7, "friend": {"name": "Bob", "id": 8}}
            if name == "emptyPerson":
                return None
            if name == "people":
                return [person, person, None]
            if name == "requiredPeople":
                return [person, person]
            return person
        return resolve

    for name, field in schema.query_type.fields.items():
        field.resolve = resolver(name)
    supplied = FIXTURES[fixture]
    request_error = False
    try:
        document = parse(query)
        errors = validate(schema, document)
        selected = get_operation_ast(document, operation or None)
        request_error = bool(errors) or selected is None
        if not request_error:
            request_error = isinstance(
                get_variable_values(schema, selected.variable_definitions, supplied), list)
    except Exception:
        request_error = True
    result = graphql_sync(schema, query, variable_values=supplied,
                          operation_name=operation or None)
    data = {} if request_error else {"data": result.data}
    paths = [error.path or [] for error in result.errors or []]
    return data, bool(result.errors), paths, calls


def fixtures():
    cases = []

    def add(query, fixture="none", operation=""):
        cases.append((query, fixture, operation))

    for declaration in ("String", "String!", 'String = "yes"', "String = null", "Int", "[String]"):
        for fixture in ("none", "string", "null", "int", "extra"):
            for field in ("echo", "required", "present"):
                add(f"query($x: {declaration}) {{ {field}(text: $x) count }}", fixture)
    for source in ('{limit: 3, text: null}', '{terms: "one"}', '{nested: {}}',
                   '{terms: ["one", null]}', '{unknown: 3}', '{limit: null}', '{}', 'null', '3'):
        add(f"{{ filter(input: {source}) }}")
    add("query($input: Filter) { filter(input: $input) }", "object")
    add("query($input: Filter = {limit: 7}) { filter(input: $input) }")
    add("query($terms: [String!], $text: String!, $limit: Int) { "
        "filter(input: {terms: $terms, nested: {text: $text}, limit: $limit}) }", "nested")
    add("query($x: [String!]) { filter(input: {terms: $x}) }", "list")
    add("{ empty(input: {}) }")
    for first, second in itertools.product(("true", "false"), repeat=2):
        for directives in (f"@skip(if: {first}) @include(if: {second})",
                           f"@include(if: {second}) @skip(if: {first})"):
            add(f"{{ person {directives} {{ name }} }}")
            add(f"{{ ...P {directives} }} fragment P on Query {{ count }}")
    for fixture in ("none", "true", "false", "null", "string"):
        add("query($x: Boolean = true) { count @include(if: $x) }", fixture)
        add("query($x: Boolean = true) { person { name @include(if: $x) } count }", fixture)
        for field in ("emptyPerson", "people", "requiredPeople"):
            add(f"query($x: Boolean = true) {{ {field} {{ name @include(if: $x) }} count }}", fixture)
        add("query($x: Boolean = true) { count @include(if: $x) @skip(if: true) }", fixture)
        add("query($x: Boolean = true) { ...P @skip(if: true) } "
            "fragment P on Query { count @include(if: $x) }", fixture)
    for query in (
        "fragment P on Person { name friend { id } } { person { ...P id } }",
        "{ person { ... on Person { name } ... { id } } }",
        "{ person { name @skip(if: true) } person { id } }",
        "{ ...P ...P } fragment P on Query { person { name } }",
        "{ person { name } person { name } }",
        "{ person { name @include(if: false) } }",
        "{ nope @skip(if: true) }",
        "{ required @skip(if: true) }",
        "{ count @include }",
        "{ count @include(if: true, extra: 1) }",
        "{ count @include(if: true) @include(if: false) }",
        "{ x: count x: echo @skip(if: true) }",
        "{ ...Missing @skip(if: true) }",
        "{ ...A } fragment A on Query { ...B } fragment B on Query { ...A }",
        "{ person { ...P } } fragment P on Person { friend { ...P } }",
        "{ count } fragment P on Query { count }",
        "{ ...P } fragment P on Person { name }",
        "query($x: String) { ...P } fragment P on Query { echo(text: $x) }",
        "query($x: String) { ...P } query Other { ...P } "
        "fragment P on Query { echo(text: $x) }",
        "query Same { count } query Same { echo }",
        "{ count } query Other { echo }",
        "query Named @skip(if: true) { count }",
        "{ ...P } fragment P on Query @skip(if: true) { count }",
    ):
        add(query)
    operations = "query One { count } query Two($x: String = \"two\") { echo(text: $x) }"
    for operation in ("", "One", "Two", "Missing"):
        add(operations, operation=operation)
    add("query Good { count } query Bad { missing }", operation="Good")
    add("query Named { count }", operation="Named")
    for fixture in ("none", "string", "null"):
        add('query($x: String = "yes") { strict(text: $x) count }', fixture)
    for fixture in ("none", "true", "false", "null", "string"):
        for body in (
            "...P ...P @include(if: $x)",
            "...P @skip(if: true) ...P @include(if: $x)",
            "...P @include(if: false) ...P @include(if: $x)",
            "... on Query { ...P } ...P @include(if: $x)",
            "...P ...Q",
        ):
            extra = " fragment Q on Query { ...P @include(if: $x) }" if body == "...P ...Q" else ""
            add(f"query($x: Boolean = true) {{ {body} }} "
                "fragment P on Query { count person { name } }" + extra, fixture)
    add("query($x: Boolean = true) { ...P ...P @include(if: $x) } "
        "fragment P on Query { count @skip(if: true) }", "null")
    add("{ person { ...P friend { ...P } } } fragment P on Person { name }")
    return cases


def main():
    engine = sys.argv[1]
    cases = fixtures()
    for query, fixture, operation in cases:
        actual = subprocess.run([engine, query, fixture, operation], check=True,
                                capture_output=True, text=True).stdout.splitlines()
        response, calls = json.loads(actual[0]), int(actual[1])
        data = {"data": response["data"]} if "data" in response else {}
        paths = [error.get("path", []) for error in response.get("errors", [])]
        observed = data, bool(response.get("errors")), paths, calls
        expected = reference(query, fixture, operation)
        # Validation may report several independent errors; only execution paths
        # and their multiplicity are part of this comparison.
        if "data" not in expected[0]:
            observed = observed[:2] + ([], calls)
            expected = expected[:2] + ([], expected[3])
        if observed != expected:
            raise SystemExit(f"Mismatch: {query!r}, {fixture=}, {operation=}\n"
                             f"Fibber: {observed!r}\nGraphQL: {expected!r}")
    print(f"PASS {len(cases)} independent GraphQL comparisons (data, errors, paths, resolver calls)")


if __name__ == "__main__":
    main()
