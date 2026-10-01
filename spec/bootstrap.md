# Bootstrap: the compiler in fibber (M6)

Status: **Proposed**, step by step. The owner agreed to the shape of M6
on 2026-09-30 (stage 2 is written in `compiler/`, one pass at a time,
each compared against the Rust pass it replaces before the next begins:
reader, expander, types, ownership, emitter). Nothing here is **Decided**
until the owner signs it; method.md applies throughout: a pass is done
when a test that can fail says it matches the Rust one, not before.

## 1. Layout

`compiler/` holds stage 2. Its modules follow syntax §5: a module `a.b`
is `compiler/a/b.fib` and a main program is a file directly under
`compiler/` (`read.fib` is the reader tool). Modules are named after the
Rust module they replace, so a reviewer can put the two side by side:

| Module | Replaces | What |
|---|---|---|
| `util.result` | (`Result`) | `(Result a b)`: `Ok` or `Err`; the prelude has `Option` only |
| `util.text` | (none) | string helpers the passes share |
| `syntax.pos` | `syntax/pos.rs` | `Pos` (file, line, col, byte range) |
| `syntax.form` | `syntax/form.rs` | `Stx` (a `StxKind` and its `Pos`), `IntWidth`, `FltWidth` |
| `syntax.error` | `syntax/error.rs` | `ErrKind`, `ReadError`, the messages |
| `syntax.chars`, `syntax.cursor` | `chars.rs`, `cursor.rs` | character classes; a cursor over the UTF-8 bytes |
| `syntax.number`, `syntax.literal` | `number.rs`, `literal.rs` | numbers; strings and characters |
| `syntax.lexer` | `lexer.rs` | tokens: delimiters, the prefix reader macros, `#_`, and atoms (through `syntax.number` and `syntax.literal`); skips whitespace, comments and separator commas |
| `syntax.reader` | `reader.rs` | the tokens of `syntax.lexer` to `Stx`, the explicit stack of open frames written as recursion bounded by the nesting cap of 1000 |
| `syntax.print` | `print.rs` | `Stx` to text that reads back |
| `syntax.dump` | `crates/fibref/src/dump.rs` | the reader dump (§2) |
| `lair.ffi` | (the C side of `lair`'s `capi/`) | the bytes and words C reads (a `str` copied into a raw buffer, out-parameter slots), a `lair_error` as a `(Result .. str)`; every C handle is an `i64` (below) |
| `lair.jit` | `capi/{jit,check,aot}.rs` | `jit-new`, `jit-free`, `jit-add-source`, `jit-address`, `jit-c-entry`, `check-source`, `build-executable` over `lair`'s C interface (compiler.md §9) |
| `lair.call` | `capi/{call,mailbox}.rs` | `call-i64`, `call-f64`, and the mailbox: `call-new`, `call-start`, `call-wait` (`Done` or `Hook` of its arity), `call-hook-arg`, `call-hook-reply`, `call-result`, `call-fault`, `hook-addresses` |
| `lair.fibm` | `crates/fibc/src/macros/module.rs` (`Fns`) | a macro-time module as the compiler sees it: the addresses of `fibm.NAME.K`, its keyword table, `Stx` to and from the module's `Form` objects |
| `lair.expand` | `macros/mod.rs` (`JitRunner::run`), `bridge.rs` | one macro run from fibber: arguments to objects, the hooks (`gensym`, `struct?`, `struct-fields`) answered by a fibber loop, the result back to `Stx` |

`compiler/jit-demo.fib` is the program that uses the five `lair.*` modules
(`basic`: a session, errors, executables, a hook round trip; `macro`: one
real macro module run from fibber). It is built with `fibc build ... -L DIR
-l lair` and judged by `crates/fibc/tests/capi.rs`.

The rules of CLAUDE.md apply to fibber source as to Rust where they can:
a file under 500 lines, a function under 50, no global mutable state
(a `Cell` lives inside an object that is passed explicitly, as the
cursor's does), every pass a function of its arguments.

**`Stx` is not `Form`.** The built-in `Form` (syntax §3.16) has no
position, because a macro sees data. The compiler needs a position on
every node for every error (§1.3), so it reads into `Stx` and converts
to and from `Form` only at the macro boundary (the expander's step).

## 2. The reader dump

`fibref read FILE..` and `compiler/read.fib FILE..` print the same text:
for each file a header line `== FILE`, then the dump of its forms, or of
its first error, or the line `unreadable` when the file cannot be read
(or is not UTF-8). Exit status 0 if every file read, 1 if one did not
read, 2 if one was unreadable (the larger wins). With no file (and with
`--print` alone) the tool prints a usage line to standard error, nothing
to standard output, and exits 2.

The dump has one line per node, depth first, two spaces of indent per
level, each ending `LINE:COL START..END` (the node's position: line and
column 1-based, column in Unicode scalar values, offsets in bytes of
the source as given, a leading byte order mark counting in offsets and
not in columns):

| Node | Line, before the position |
|---|---|
| symbol, keyword | `sym "name"`, `kw "name"` |
| integer | `int 42 i64` (value, width always written) |
| float | `flt 2.5 f32` (the `show` text of types §2.12 at its width, then the width) |
| string | `str "text"` |
| character | `chr U+0041` (four or more uppercase hex digits) |
| boolean, nil | `bool true`, `nil` |
| list, vector, map | `list 3`, `vec 3`, `map 4`: the count of items, which follow one level deeper |

Quoted text escapes `"` as `\"`, `\` as `\\`, LF, TAB and CR as `\n`,
`\t`, `\r`, every other character below U+0020 and U+007F as `\xNN`
(uppercase), and writes every other character as it is. An error is one
record, `error KIND LINE:COL START..END: MESSAGE`, where `KIND` is the
name of the variant of `ReadErrorKind` and `MESSAGE` its `Display` text
(the text after the position in `ReadError`'s `Display`). A message
quotes the offending text as it is, so it holds a line break when that
text does (`"a\` followed by a raw newline is a bad escape): a dump is
therefore a sequence of lines only up to its error, which is always the
last record, and a reader of a dump looks for `error ` at the start of
its first line.

`fibref read --print FILE..` and `compiler/read.fib --print FILE..` (the
flag is the first argument) print the printer's text instead, so that the
printer is compared on real files and not only on trees built by hand:
after each `== FILE` header, one line per top-level form holding the text
of `syntax/print.rs` (`syntax.print`): prefix forms in their long spelling
(`'x` is `(quote x)`), an integer with its width suffix unless `i64`, a
float in Rust's `{:?}` layout (the shortest digits that read back,
positional from 1e-4 up to 1e16, scientific outside, `-0.0`, `f32` after
an f32 float), a string and a character escaped as in syntax §1.1 with
every forbidden character as `\u{HEX}`. The text has no line break and
reads back to an equal form. A file that does not read prints the line
`error ...` of the dump, and the exit statuses are those of the dump.

## 3. How the reader is judged

`crates/fibc/tests/bootstrap.rs` builds `compiler/read.fib` with `fibc`
and compares its output and exit status, byte for byte, with
`fibref::dump` (the dump of §2, and again with `--print`) over:

1. every `.fib` file under `cases/`, `lib/` and `compiler/`, and the
   prelude of the expander;
2. the files of `compiler/tests/reader/` (983 `.fib` files when this
   was written), written to hit each rule of syntax §1 and each
   read error, and the inputs that killed the mutants of the reader that
   the test alone had let survive (`rmut-*`, `rmut2-*`). Every variant of
   `ReadErrorKind` occurs among them: `fibref read
   compiler/tests/reader/*.fib | grep -a '^error ' | awk '{print $2}' |
   sort -u | wc -l` prints 19, and the enum has 19 variants; no test
   derives that count from the enum, so a variant added later is not
   noticed. Nothing checks that the inputs of the Rust reader's own unit
   tests (`crates/fibref/src/syntax/tests/`) are among these files;
3. one small file for every scalar value that the whitespace and
   control classes decide, with its neighbours and look-alikes;
4. inputs generated from a seed by mutating the corpus and by composing
   tokens, equal in number on every run of the test (a few hundred,
   never thousands: the machine has been taken down by sweeps before).
   A unit test (`crates/fibc/tests/bootstrap/fuzz.rs`) checks that the
   standard run reaches each of 19 named read errors at least twice and
   each node kind at least ten times; the 19 names are a list in the
   test, not derived from `ReadErrorKind`;
5. paths that cannot be read, and the tool with no file.

The test fails if the two outputs differ in any byte. Unit tests show
that a difference in one position digit, a missing line, another exit
status or a missing final newline is each reported, and a *canary* runs
the real pipeline with a fault planted in the real reader and shows it
reported. The `rmut-*` and `rmut2-*` inputs of item 2 come from two
one-off reviews of the reader by mutation (the first: 1797 mutants of
the fibber source, each run over 1886 inputs): a test that agrees with
the oracle everywhere it looks says nothing about where it does not
look, so each mutant the test let survive became an input. The mutants,
the scripts and the per-mutant tables of those reviews were not kept in
the repository, so the counts are a record of what was done and cannot
be re-run from here; what the repository keeps of them is the inputs.

## 4. Gaps found on the way

Each gap in the language or the library that the compiler met, and what
was done. The compiler finds gaps the library was not scoped against;
each one is decided by the owner or recorded as a proposal here.

| Gap | Found by | Resolution |
|---|---|---|
| the compiled `show` of a float differed from the interpreter's in exact ties (`2^-25`) and just above powers of two: `fib.show-fp` took the first `printf` precision that reads back | the dump of float literals, the printer (`print-009`) | fixed: `show` is the shortest digits that read back, the nearest of them, a tie going up (types §2.12), as Rust prints; `crates/fibc/tests/floats.rs` compares 64 000 values, case 187 |
| `read-file` of a directory was `(some "")` compiled and `nil` interpreted; of a file in `/proc` or a pipe the compiled one read nothing; a path with a NUL was cut at it | `compiler/read.fib` on a directory | fixed: reads to the end and checks the error flag, a NUL in a path is `nil` (and `false` for `write-file`) (syntax §4.3; `crates/fibc/tests/files.rs`, case 188) |
| the interpreter had no `strtod`/`strtof` for `extern`, so the reader could not be run under the audit on any float literal | `compiler/syntax/number.fib` | added to the interpreter, decimal only (syntax §3.15, **Proposed**; case 189) |
| a failed `malloc` was a write through a null pointer (SIGSEGV) | robustness review | fixed: the trap `out of memory` (`crates/fibc/tests/cli.rs`) |
| `(alloc n)` is zeroed by the interpreter and is plain `malloc` compiled; the spec is silent | the `strtod` work | fixed: `(alloc n)` is `n` zero bytes in both (compiled: `calloc`), specified in syntax §3.15; case 190 (a block freed and allocated again reads zero; compiled with `malloc` it read about 200 of the 196631 bytes as zero) |
| the audited interpreter keeps memory proportional to work, and the work is the tree: 80 to 90 KB per node read (a lone top-level symbol 80 KB, a form `(a 1)` of three nodes 250 KB, a symbol in a list of 100 about 90 KB), and a nest grows faster, with the square of its depth: maximum resident set of `fibref run compiler/read.fib` 25848 KB at depth 100, 55188 KB at 200, 171428 KB at 400 and 744668 KB at 800, so about 1.2 GB at the cap of 1000 by extrapolation (not run) | robustness review | noted: run the interpreter audit on small inputs only. The compiled reader is linear in the size of the input, at a rate that depends on its kind. Measured on 2026-10-01 with a reader built by `target/debug/fibc build compiler/read.fib`, dump to `/dev/null`, one process at a time, at 1, 4 and 8 MiB (each kind took twice as long at twice the size). At 8 MiB: a comment (`;xxxx..`) 0.5 s and 17396 KB; commas 0.5 s and 17804 KB; forms `(a 1)` 3.0 s and 906188 KB; one string 4.6 s and 572204 KB; one symbol 4.9 s and 572060 KB; integers `12345 ` 6.8 s and 469536 KB; the digit `1 ` repeated 19.4 s and 1080300 KB (the tree is held whole). The cost goes with the number of tokens (about 4.6 microseconds for each of the 4.2 million atoms of the last), not with the bytes. So "8 MB in 0.5 s" is the best case, a comment or a run of commas, and 19.4 s the worst measured |
| `(args)` traps on an argument that is not UTF-8 | robustness review | fixed, and the description was wrong: a built executable did not trap, it made a `str` that was not UTF-8, and `fibc run` and `fibref run` panicked in `std::env::args`; now each invalid sequence is U+FFFD as `String::from_utf8_lossy` makes it, in all three (syntax §4.3; `crates/fibc/tests/cli/args.rs`, `crates/fibref/tests/run_io.rs`); a tool's own word before `--` that is not UTF-8 is refused (exit 2) |
| `println` ignores a failed write (a full device exits 0) | robustness review | fixed: `println` and `eprintln` write in a loop until every byte has gone and trap `println: write failed` (`eprintln: write failed`) on an error (syntax §4.5; `crates/fibc/tests/cli/writes.rs`, `crates/fibref/tests/run_io.rs`, case 192 for the normal path) |
| the reader needs 480 to 512 KB of native stack at nesting 1000 (about 500 bytes a level) | robustness review | noted: a worker thread gets the main thread's soft limit, 8 MB here, and all the reader's inputs were read correctly in `pmap` workers |
| `fibc build` had no `-L`, `-l` or rpath: a library outside the system's directories could not be named, and a found one needed `LD_LIBRARY_PATH` to run | the C interface (`-l lair`, compiler.md §9) | fixed: `fibc build FILE -o OUT -L DIR -l LIB`, each `-L` also an absolute rpath (compiler.md §1; `crates/fibc/tests/cli/link.rs`, which runs the executable without `LD_LIBRARY_PATH` and from another directory). The link step is `lair`'s: `lair::aot::Options` has `lib_dirs` beside `libs` (lir.md §11; `crates/lair/tests/link.rs`), and `lair build` takes `-L`. `lair_build_executable` (§9) has no directory argument, so a stage-2 `build -L` cannot be written through it yet |
| a struct, enum variant, `Cell` or `Option` that holds a raw `ptr` releases it when dropped: `child_fields` in `crates/fibc/src/objects.rs` treats every `ptr` field as an object (a raw `ptr` and an object pointer are both lIR `ptr`), so `fib.release` decrements the first word of the user's block and, at zero, frees it | `lair.jit`'s `Libs` struct (a heap abort: "corrupted double-linked list") | fixed, a stage-1 bug: the interpreter treats `ptr` as uncounted (types §8.1) and answered 41 where the compiled program answered 40. A raw `ptr` is now its own lIR type, `LirTy::Raw` (`crates/fibc/src/value.rs`), which `lir_ty` gives the fibber type `ptr` and which prints, lays out and passes as `ptr`; what is counted follows that type and not the lIR text: the drop, trace and copy of an object (`crates/fibc/src/objects/walk.rs`), the counted flag of a `Vec`'s elements and of a vector pattern's rest, the layout class of a body at `ptr` (`mono.rs`: it was keyed with the objects, so a generic function retained and released its `ptr` arguments), and the name of an `Array`, `Cell` or `Atom` of it (`fib.array.raw`, not `.ptr`). Case 193 holds one block in 17 kinds of container and reads its count while the container lives and after it is dropped: 10339289685 compiled before, 17179869183 after, as interpreted (`crates/fibc/tests/capi/stage1.rs`, no longer ignored, is the minimal program). An `Atom` of `ptr`, a `spawn` that returns one and an `async` that captures one stay rejected, since `ptr` is not `Send` (types §5.3). `compiler/lair/` still holds a block that must live in a value as its address, an `i64`, as it did to work around this |
| fibber has no `f64` to bits conversion, and `lair_call_i64` passes no double, so a `Flt` form cannot be handed to a macro module (`fibm.form` takes the bits as an integer) | `lair.fibm` `to-object` | fixed, a language gap: four builtin functions `(f64->bits x) -> i64`, `(bits->f64 n) -> f64`, `(f32->bits x) -> i32` and `(bits->f32 n) -> f32` (syntax §4.3, types §2.12), the IEEE 754 pattern with a NaN's payload and a zero's sign kept. Compiled they are one `bitcast` each (`lower/arith.rs` `float_bits`); the interpreter holds an `f32` widened to `f64`, which the hardware conversion quiets a signalling NaN on, so it widens and narrows a NaN by hand (`eval/float_bits.rs`). `lair.fibm` `to-object` calls `f64->bits` and the `sscanf` is gone; the reader's printer can use them too. Case 194 (18 probes: 1.0, 0.1, -0.0, 5e-324, the maximum, the infinities, NaN payloads at both widths, generated patterns, struct fields, a generic function, macros at expansion time); `crates/fibc/tests/float_bits.rs` folds 3000 xorshift patterns at both widths and with the exponent forced to all ones, in the interpreter, the JIT and Rust, to one `i64`; `crates/fibc/tests/capi/macros.rs` `floats_go_in_and_come_out_of_a_module_bit_for_bit` runs through `f64->bits` now |
| the macro module's keyword table (the ids of `:i8`..`:f64` and of every other keyword a macro mentions) was in `MacroModule::keywords`, not in the module's text, so a stage-2 expander could not name a width id past 5 and named it `#6` where the Rust runner said `f16` | `lair.fibm` on case 105 (`(Flt 2.5 :f16)`) | fixed: the module exports `fibm.kw-count.K` and `fibm.kw.K` (id to name as a `str` object, null for an id that is none; `abi.rs`, compiler.md §6). `fibm-lookup` reads the table once and finds the six width ids by name instead of assuming 0 to 5; a table that lacks one is a lookup error, so `to-object` has none to report. The Rust runner (`macros/module.rs`) reads the same accessors, so there is one table and no `MacroModule::keywords`. `crates/fibc/tests/capi/macros.rs`: case 105 now equals the Rust runner's message exactly (`trap: a Flt form of width :f16`), and `:i128`, an `Int` of `:f32` and a `Flt` of `:i8` are named the same way |
