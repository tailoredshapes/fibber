# Language reference

The normative source is [syntax](../../spec/syntax.md),
[types](../../spec/types.md) and [ownership](../../spec/ownership.md).
Syntax §4.2 lists 24 core forms, including `defkernel`. `with-view` and
`with-view-ro` are library macros (§3.21); they do not increase that count.

| Area | Normative chapter |
|---|---|
| Reader, literals, forms and macros | syntax §§1–4 |
| Modules and visibility | syntax §5 |
| Type inference, protocols, Send, async | types |
| Borrows, counts, in-out places, exclusive views | ownership and types §§6–8 |
| Builtin signatures and escape kinds | [generated table](builtins.md) |
| Shipped facade declarations | [library index](library/INDEX.md) |

The stdlib specification is a planning table; its unbound exceptions are
explicit in ADR 0023. Historical Rust citations resolve at tag `seed-1`, as
explained in [the source map](../history/source-map.md).
