# The AST dump of lIR (`lair dump-ast`)

This is the oracle format of docs/design/lair-in-fibber.md section 2.2. `lair dump-ast FILE.lir` (crates/lair/src/dump.rs)
parses FILE with `lir::parse` and prints the module in the grammar below. It parses but does not check, so it dumps
modules the checker rejects. A fibber printer (`lairf dump-ast`, `lir.dump`) must produce the same bytes. If the file does not
parse, nothing goes to standard output; standard error gets `FILE:LINE:COL: error: MESSAGE` and the exit status is 1, which is
what `lair check` does for a parse error, and the fibber tool compares that line.

## Lines

```
dump   := "module" NL  item*                      (items at depth 1)
line   := INDENT TAG [SP POS] (SP KEY "=" VALUE)* NL
INDENT := two spaces per depth
POS    := LINE ":" COL                            decimal, 1-based, as the source says
```

One line per node. The children of a node follow it, one depth deeper, in the order given below, with no separator. Where the
number of children is not fixed, the line says how to count them (`args=N`, `bindings=N`, `cases=N`) or the child kinds tell
(`bind`, `case`, `block`). Every line ends in a newline, the last included. Attribute order is fixed per tag as written below.

## Values

| Value | Written |
|-------|---------|
| string | `"` bytes `"`: `\\`, `\"`, `\n`, `\r`, `\t`, and `\xHH` (two lower-case hex digits) for every byte not in 0x20..0x7e; all other bytes as themselves. A name is its UTF-8 bytes. |
| type, function type | the lIR text of the type, as a string: `i32`, `float`, `double`, `ptr`, `<4 x i32>`, `%struct.NAME`, `[3 x i32]`, `{ }`, `{ ptr, i64 }`; function: `(fn RET (P P ...))`, `(fn tailcc void (ptr ...))`. Exactly the `Display` of `lir::Type` and `lir::FnType` (crates/lir/src/types.rs). |
| bool | `true` or `false` |
| integer literal | the signed decimal of the value; the Rust holds an `i128`, a fibber port holds (low 64 bits, sign) and prints the same digits, so `18446744073709551615` and `-9223372036854775808` both occur |
| float literal | `0x` and 16 lower-case hex digits of the IEEE-754 binary64 bits of the `f64` the reader holds (also for type `float`: the value is held as `f64`) |
| align | `none` or the decimal |
| enum | the variant name of crates/lir/src/ast.rs in lower case, no separator: `sdiv`, `fptosisat`, `lshr`, `singlethread`, `acqrel`, `umax`, `external` |
| list | `(` items separated by one space `)`; `()` when empty |

## Items (depth 1)

```
struct POS name=STR fields=(TYPE..)
global POS name=STR ty=TYPE const=BOOL linkage=ENUM hidden=BOOL      child: the initialiser expression
declare-global POS name=STR ty=TYPE hidden=BOOL
declare POS name=STR fn=FNTYPE hidden=BOOL
define POS name=STR fn=FNTYPE linkage=ENUM hidden=BOOL params=(STR@POS..)      children: block*
block POS label=STR                                                  children: expression*
```

`linkage` is `external`, `internal` or `private`.

## Expressions

`POS` is the position of the form. Children are expressions unless said.

| Tag | Attributes | Children |
|-----|-----------|----------|
| `local` | `name` | |
| `global-ref` | `name` | |
| `int` | `ty` `val` | |
| `float` | `ty` `bits` | |
| `null` | | |
| `vector` | `ty` | elements |
| `str` | `bytes` | |
| `struct-lit` | `name` (a string without `%struct.`, or `none` for `{ .. }`) | fields |
| `array` | `ty` | elements |
| `zero` | `ty` | |
| `bin` | `op` | a b |
| `un` | `op` | a |
| `overflow` | `op` | a b |
| `icmp` | `pred` | a b |
| `fcmp` | `pred` | a b |
| `cast` | `op` `ty` | a |
| `select` | | cond then else |
| `extract-element` | | vector index |
| `insert-element` | | vector value index |
| `shuffle` | | a b mask |
| `extract-value` | `idx=(N..)` (decimal integers) | aggregate |
| `insert-value` | `idx=(N..)` | aggregate value |
| `alloca` | `ty` `count=BOOL` `align` | the count expression if `count=true` |
| `load` | `ty` `volatile` `align` | ptr |
| `store` | `volatile` `align` | value ptr |
| `gep` | `inbounds` `ty` | ptr, then the indices |
| `atomic-load` | `scope` `ord` `ty` | ptr |
| `atomic-store` | `scope` `ord` | value ptr (source order) |
| `atomic-rmw` | `op` `scope` `ord` | ptr value (source order) |
| `cmpxchg` | `weak` `scope` `success` `failure` | ptr expected new |
| `fence` | `scope` `ord` | |
| `trap` | | |
| `call` | `tail` then `callee=direct name=STR args=N` or `callee=indirect fn=FNTYPE args=N` | the callee expression if indirect, then N arguments |
| `ret` | `value=BOOL` | the value if `value=true` |
| `br` | `label` | |
| `cond-br` | `then` `else` | cond |
| `switch` | `default=STR` `cases=N` | the scrutinee, then N `case` lines |
| `unreachable` | | |
| `phi` | `ty` | `bind` lines |
| `let` | `bindings=N` | N `bind` lines, then the body expressions |

`bind POS name=STR` has one child, the value. `case label=STR` has no position of its own and one child, the case value.
`ord` and `scope` use the enum rule (`seqcst`, `monotonic`, `system`, `singlethread`). Children are always in the order of the
expression fields of the variant in crates/lir/src/ast.rs, which is the source order of the operands.

## Why this and not `{:#?}`

`{:#?}` prints Rust's derived layout (`Pos { line: 1, col: 2 }`, `Some(..)`, nested indentation by delimiter) and changes with the
compiler and the field order; this format is fixed by this document, has one node per line so that `diff` locates a difference
by line, and names every literal exactly (digits of an `i128`, bits of an `f64`).

## Checks

`cargo test -p lair --lib dump` pins the format on small modules (the line text, quoting, the exact literals);
`cargo test -p lair --test dump` dumps every parseable file of cases/lir twice, requires the same bytes, no unknown tag, and at
least 45 distinct tags. `compiler/tests/native/compare-ast.sh` is the comparison with the fibber printer.
