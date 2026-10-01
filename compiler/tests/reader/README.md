# compiler/tests/reader

Inputs for the reader of the self-hosted compiler (`compiler/syntax/`,
spec/bootstrap.md §3). Each `.fib` file is **reader input text**: it need
not be a program, and many are not (an unclosed list, a lone `#_`, a file
that is not UTF-8). What matters is what the reader does with the bytes.

## Every file here is a test

`crates/fibc/tests/bootstrap.rs` builds `compiler/read.fib` with `fibc
build` and feeds it every `.fib` file under `cases/`, `lib/` and
`compiler/`, which includes this directory. For each file the tool must
print, byte for byte, what the Rust reader's dump
(`fibref::dump::dump_source`) is, with the same exit status; and then
again in **print mode** (`read --print FILE`): the text of
`fibref::dump::print_source`, each top-level form as the printer writes
it, with the same `== FILE` header, error line and status. Both modes run
over every input of the test (the corpus, the unicode classes, the
generated inputs, the unreadable paths), with one build of the tool, and
a failure says which mode it is in. The tool with no file, or with
`--print` alone, must print nothing to standard output, a usage line to
standard error, and exit 2. A file you add here is picked up by the next
run; there is no list to update. Since the printer is checked on every
file, any file here exercises it; a `print-*` file is one written for it.

```
export LLVM_SYS_211_PREFIX=/usr/lib/llvm-21     # /usr/lib/llvm21 on the Arch host
cargo test -p fibc --test bootstrap
```

The files here are also the starting point of the generated inputs of
that test: it mutates the corpus (deletes, duplicates and swaps stretches
of it, and inserts fragments such as `"`, `#_` and `\u{`), so a file that
reaches an unusual path gives the mutations something to break.

To compare one file by hand:

```
fibref read FILE                                    # the Rust reader's dump
fibref read --print FILE                            # the Rust printer's text
fibc build compiler/read.fib -o /tmp/read && /tmp/read FILE
/tmp/read --print FILE
```

## Naming

`<area>-NNN-<what>.fib`, with `NNN` counting from 001 within the area and
`<what>` saying in lower-case words what the file hits.

| Area | Owner of | Hits |
|---|---|---|
| `number` | `syntax/number.fib` | integer and float literals: radix, `_`, suffixes, ranges |
| `literal` | `syntax/literal.fib` | strings and characters: every escape and name |
| `reader` | `syntax/reader.fib` | tokens, delimiters, prefixes, `#_`, positions, every read error |
| `print` | `syntax/print.fib` | the text the printer writes in print mode (floats in both widths, ints, strings, chars, symbols, prefixes, nesting, errors after forms), and that it reads back; the mutation review of the printer found 31 survivors that only print mode kills |
| `rmut` | `syntax/*` | one file for each mutant of the reader (`syntax/*.fib` and the dump) that the test did not kill in a mutation review: UTF-8 length boundaries, characters next to hex digits (`:` `@` `` ` ``), commas after an unquote, `-` at the end, forbidden characters, a float followed by `a` |
| `rrobust`, `rspec` | the interpreter | inputs found while running the reader under the interpreter (a float literal; a chain of 998 quotes): named for what they hit there, and kept as inputs to the reader like the rest |
| `harness` | the test itself | generic edge inputs: empty and tiny files, byte order marks, line endings, NUL, the depth limit, multi-byte text, files that are not UTF-8 |

Never renumber a file: a failure names it.

## Rules for a file

- **Keep the bytes exact.** Several files are about bytes an editor likes
  to fix: a byte order mark, CRLF, a lone CR, NUL, no final newline,
  invalid UTF-8. Write them with `printf` or a script, and check with `xxd`.
  `.gitattributes` here turns off line-ending conversion for `.fib` files.
- **A file that is not UTF-8 is a test of `unreadable`**: both tools print
  `unreadable` for it and exit 2. They are named `not-utf8-*`.
- **One error per file.** The dump of an error is that one line, so a file
  with two errors only tests the first. Put each error in its own file.
- **Say why in a comment** at the top when the reason is not obvious from
  the name, as a `;` comment, which costs one line of every position in the
  file. A file whose point is its first byte has no comment.
- **Keep it small** (a few KB), except for the depth files, which are
  the point of being large.
- Columns count Unicode scalar values and offsets count bytes of the file
  as given, a leading byte order mark counting in offsets only
  (spec/bootstrap.md §2): include multi-byte text before the token whose
  position you want to test.
