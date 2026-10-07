# fib.json.schema: JSON Schema for fib.json

Status: implemented (draft 7). Library: `lib/fib/json/schema.fib` (the facade, `(:require [fib.json.schema :as js])`) over
`lib/fib/json/schema/*.fib`. Tests: `specs/json-schema-spec.fib` (offline, in the gate), `scripts/json-schema-suite.sh` (the
JSON-Schema-Test-Suite), `scripts/mutant-json-schema.sh` (planted faults).

Asked for by meshql-fib: a meshql restlette carries a JSON Schema for its documents, and the other implementations (meshql-java with
networknt, meshobj with ajv, meshql-rs with jsonschema) refuse a write that does not conform.

## Use

```clojure
(match (js/compile-schema schema)                      ; (Result JsonSchema str), once
  ((Err m) ..)                                         ; a regex that does not compile, a $ref that names nothing, a non-schema
  ((Ok s) (js/validate s doc)))                        ; (Vec SchemaViolation): every violation; [] when doc conforms
(js/valid? s doc)
(js/violation-text v)                                  ; "/eggs: -1 is less than the minimum of 0"
(js/compile-with schema (with (js/schema-options) (resources {"http://example.com/other.json" other}) (formats false)))
```

A `SchemaViolation` has the instance location (a JSON Pointer), the schema location (a JSON Pointer from the root schema through the
`$ref`s followed), the keyword, and a sentence.

## Choices

- **Draft 7.** What meshql's restlette schemas are written in, and what meshql-java assumes (`SpecVersion.V7`). A `$schema` naming another
  draft is read as draft 7. Later drafts' keywords (`$defs`, `dependentRequired`, `unevaluatedProperties`, `$anchor`, ...) are unknown
  keywords and are ignored, as draft 7 ignores any unknown keyword.
- **Compile once, interpret the schema.** Compiling walks the schema once: every subschema an `$id` names is indexed by absolute URI (with
  draft 7's rules: an `$id` beside a `$ref` is ignored like every sibling of a `$ref`; `"#name"` names a schema without moving the base),
  every regex is compiled, and every `$ref` is resolved, so a schema that cannot work is an `Err` at compile time and never a surprise
  during validation. Validation then walks schema and document together; there is no code generation.
- **Every violation, not the first.** A restlette's 400 names everything that is wrong. `valid?` is `validate` and `empty?`.
- **Numbers as JSON Schema compares them**: 1, 1.0 and 1e0 are one number and 1.0 is an integer; two i64 compare exactly; `multipleOf` on
  floats accepts a quotient within a relative 1e-9 of a whole number (0.3 is a multiple of 0.1); an integer too large to divide
  (1e308 by 0.5) is a multiple of any `m` whose reciprocal is whole.
- **Strings are counted in characters**, not bytes (`maxLength` of "💩💩" is 2).
- **Formats are asserted by default** (as networknt does, and ajv with ajv-formats): date, time, date-time, email, hostname, ipv4, ipv6,
  uri, uri-reference, json-pointer, relative-json-pointer, regex, uri-template. An unknown format passes. `(formats false)` turns them off.
- **Regexes are fib.regex's**, not ECMA 262's. They agree on the syntax schemas use; ECMA-only constructs and the Unicode meaning of `\d`,
  `\w`, `\s` differ (the suite's optional `ecmascript-regex` tests record which).
- **The draft-07 meta-schema is built in** (`fib.json.schema.meta`), so a schema can be validated against it without a network. Other
  documents a `$ref` names are given to `compile-with` as resources: nothing is fetched.
- **Guards**: evaluation deeper than 256 nested schemas is a violation ("the schema refers to itself without end"), so `{"$ref": "#"}`
  cannot hang a validation.

## Evidence

- The JSON-Schema-Test-Suite at a pinned commit, the archive's sha256 checked (`scripts/fetch-json-schema-suite.sh`, ADR 0020): **all 929
  required draft 7 tests pass**; 772 of the 911 optional ones. The 139 that do not are listed with their reasons in
  `specs/json-schema-suite.expected`, and `scripts/json-schema-suite.sh` fails when that list and the run disagree in either direction: the
  internationalized formats (idn-hostname, idn-email, iri, iri-reference) and the IDNA rules of `xn--` hostnames, which need Unicode tables;
  ECMAScript regex semantics; `contentMediaType` and `contentEncoding`, which draft 7 makes annotations; one cross-draft `$ref`.
- `specs/json-schema-spec.fib`: 15 scenarios (compiling, violation locations and messages, numbers, `$ref` by pointer, `$id`, plain name,
  across documents, the meta-schema, combinators, formats on and off, the guards).
- `scripts/mutant-json-schema.sh`: 14 planted faults (numbers, integers, multipleOf rounding, required, character counting, locations, `$ref`,
  if/then/else, oneOf, additionalProperties with patternProperties, `$id`, the meta-schema, leap seconds, the format default); the spec kills
  every one.

## Not done

Drafts 2019-09 and 2020-12 (`$defs`, `$anchor`, `unevaluated*`, `dependentSchemas`, vocabularies); the internationalized formats;
annotations output; short-circuiting `valid?` (it collects every violation first).
