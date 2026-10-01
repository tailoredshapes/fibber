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
| float | `flt 2.5 f32` (the `show` text of types §2.12 at its width, then the width; since 2026-10-01 that text is Clojure's, Java's `Double.toString`: `flt 0.001 f64`, `flt 1.0E7 f64`, `flt 1.4E-45 f32`) |
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
an f32 float; the printer's layout is its own and is not `show`'s, so
`1e-5` prints as `1e-5` where the dump says `1.0E-5`, and `5e-324` where
the dump says `4.9E-324`), a string and a character escaped as in syntax §1.1 with
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
| the compiled `show` of a float differed from the interpreter's in exact ties (`2^-25`) and just above powers of two: `fib.show-fp` took the first `printf` precision that reads back | the dump of float literals, the printer (`print-009`) | fixed: `show` is the shortest digits that read back, the nearest of them, a tie going up (types §2.12), as Rust prints; `crates/fibc/tests/floats.rs` compares 64 000 values, case 187; since 2026-10-01 the layout of `show` is Clojure's (types §2.12, stdlib design C12) and the printer reads the digits off either form of it (`syntax/print.fib`), taking Java's two digits where one would do back to Rust's one by reading candidates back with `strtod`/`strtof` (`print-*`, `t0c-001-*` to `t0c-003-*`) |
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
| the interpreter's macro evaluator kept the checked macro-time module of a macro by the macro's name alone, so two modules' macros of one name ran as the first one used (`(+ (m/same 1) (same 1))` with `m`'s `same` = `x * 100` and `util`'s = `x + 1` was 200 interpreted, 102 compiled: the JIT's runner keys by `ns/name`; case 005 could not tell, both of its `thrice`s are 15) | the expansion dump (§5) on `cases/modules/005` | fixed: `MacroEvaluator::checked` is keyed by `MacroDef::key` (`crates/fibref/src/eval/macros.rs`); `eval::tests::macros::two_modules_macros_of_one_name_are_told_apart_by_module` read 200 before and reads 102 |

## 5. The expander (M6 step 2)

Status: **Proposed**. The expander is the second pass of stage 2. It takes
the forms the reader made and returns the program's core forms: macros
expanded, `defstruct` and `defenum` registered, `derive` written out, `[..]`
and `{..}` made calls, `nil` made `(Nil)`, a top-level `(do ..)` spliced
(syntax §2, §3.16, §4). It is `crates/fibref/src/expand/` (about 3,350
lines), the module driver `crates/fibref/src/modules.rs`, and the two
runners of user macros, `eval::macros` (the interpreter's, the oracle) and
`macros/` of `fibc` (the JIT's, which stage 2b reaches through
`compiler/lair/expand.fib`). Another set of agents ports it to
`compiler/expand/`; what they are judged against, and the types they must
agree on first, are here. Method rule 3 applies as it did to the reader:
the port is done when the test of §5.6 says its output is the Rust
expander's, byte for byte, and not before.

### 5.1 The expansion dump

`fibref expand [OPTION..] [--] FILE..` and `compiler/expand.fib` with the
same words print the same text. For each FILE, the header line `== FILE`
and then one of

- the line `unreadable`, when the file cannot be read as UTF-8 text;
- one **error record** (below), when the program does not read or load
  before any module is expanded: the file's own read error is the record
  of the reader dump (§2) and nothing else; a module it loads that does
  not read is that record with `@FILE` (below) after its position; the
  other ways a load fails are in the table below;
- for each module of the program, in dependency order and the main module
  last (syntax §5), the line `-- module NS FILE` (the module's `ns`, which
  is `main` for a file with no `ns` form, and the path of its file as the
  driver made it: the main file as given, and module `a.b` at
  `DIR/a/b.fib` with `DIR` the directory of the main file as Rust's
  `Path::parent` gives it, so that a file given with no `/` has its modules
  at `a/b.fib`; `Path::join` also drops a repeated or trailing `/`, and the
  test gives normalized absolute paths; a module not beside it is looked
  for in the built-in root of `roots.rs`, **Proposed**, which tranche 0
  adds with `-I DIR` and `FIB_LIB`: neither is an option of `expand` yet,
  the dump uses `modules::try_load`, the default), then the module's
  **expanded top-level forms** in the line format of the reader dump (§2:
  one line per node, depth first, two spaces a level, `LINE:COL
  START..END` at the end of each), or, instead of them, one error record,
  which ends the file's dump (a later module is not expanded).

The exit status is 0 if every file expanded, 1 if one ended in an error
record, 2 if one was unreadable (the larger wins). With no file, with only
options, with an option that is not one, or with a limit that is not
decimal digits, the tool prints a usage line to standard error, nothing
to standard output, and exits 2. An argument that starts with `--` is an
option until the first file or a lone `--`; after that it is a file name
(`fibref::expand_dump::parse_args`, which `fibref` itself calls).

**A position** is printed as the reader dump prints it. A position in
another file than the one of the module being dumped ends `@FILE`
(`1:2 1..5@lib/prelude.fib`): a form that a macro or a definition of
another file brings, and the built-in definitions (`<builtin>`, `<prelude>`).
The file of a position is observable, since the messages the expander
writes name it (`assert failed at FILE:L:C: ..`, `cond: no clause matched
at FILE:L:C`).

**An error record** is `error KIND LINE:COL START..END: MESSAGE`, `KIND`
the name of a variant of `ExpandErrorKind` (the 25 of `error.rs`,
`expand_dump::expand_kind_name`; a new variant is a compile error there
until it is named), of `ReadErrorKind` (§2) for a read error, or one of
`BadNs` (an `ns` form that does not parse, at the form), `ModuleMissing`,
`ModuleCycle` and `ModuleMismatch` (no position: `0:0 0..0`), and
`MESSAGE` its `Display` text. A module that is not at its file says `module
NS is not at FILE` and not why: the operating system's words are not the
same in every language a tool is written in. A message may hold a line
break, so a dump is a sequence of lines only up to its error, which is the
last record of its file.

**The context** a file starts from is fresh each time: `ExpandCtx::new()`,
then the expander's own prelude (`expand::PRELUDE_SOURCE`, a constant
the port repeats) and `lib/prelude.fib`, expanded with no runner in the
scope `fib.prelude` and ended (`types::prelude_forms`); the tool reads
`lib/prelude.fib` from its working directory, which the test makes the
repository root. The prelude's output is not part of a program's dump;
`--prelude` prints it.

| Option | What it does |
|---|---|
| `--prelude` | each FILE is a library prelude, not a program: one section `-- module fib.prelude FILE`, holding the expanded forms of `PRELUDE_SOURCE` (every position `@<prelude>`) and then those of FILE, which both are expanded in the one scope and with no runner. With `lib/prelude.fib` it is the prelude every program expands in |
| `--context` | after the forms of each module, before it is ended, `-- context NS` and the lines of §5.2 |
| `--no-runner` | a call of a user macro is `MacroNeedsEvaluator` (`NoRunner`); the default is the interpreter's macro evaluator, for the stage that has a runner (§5.4) |
| `--max-steps N`, `--max-depth N`, `--max-forms N` | the limits of `ExpandCtx` (syntax §3.16) once the prelude is expanded, so that a small one reaches its error on a small input |

### 5.2 What the dump must include, and the context

The port is judged on everything it can be seen to do. The forms say what
the expander built and where each node is (syntax §1.3: a form a macro
builds carries the call's position, one taken from the input its own; so
the position of every node of an expansion is compared), the `defmacro` forms
the program keeps (with their bodies expanded: quasiquotes rewritten to
`Form`-building calls, prelude macros expanded, literals rewritten), a
spliced `do`, every impl `derive` writes (heads, `:where` contexts,
method bodies), the names of the gensyms (so the order the counter is
used in), and the errors with their kinds, positions and messages.

What the forms do not say is what the expander **remembers**, and a later
module, a later macro call and the reflection calls read it. It is not
needed by a later pass as such (the checker takes the expanded forms, the
module specs and the prelude's forms; of the expander it uses the
constants `PRELUDE_NS`, `is_core` and `marker_index`, and a fresh
`ExpandCtx` for the built-in `Option` and `Form` enums it declares its own
types from, `types/init.rs`), but it is cheap to print and it is what pins
the rules the forms show only sometimes:
that a redefined struct replaces the old one, that a private type stops
being seen after its module, that a macro of another module is told apart
by its module, and how many steps and forms a top-level form was charged.
So `--context` prints, in this order, after each module:

```
-- context NS
gensyms N                       gensyms handed out so far, the prelude's included
counters steps S forms F        the counts of the last top-level form of the module
scope "NS"                      the module being expanded
  use "M"                       each module it :uses, in the order written
  alias "A" "M"                 each :require alias, sorted by alias
reexport "NS" "M" ..            each module that re-exports, sorted by module, its list in order
private "T"                     the :private struct and enum names of this module, sorted
hidden "T"                      those of the modules before it, sorted
macro "NS/NAME" public|private  then, indented: at POS, params "x" "y", rest "r" or -,
                                 body N and the N forms at depth 2
struct "NAME"                   then params N, the N forms, fields N, and for each field
                                 its name form and its type form
enum "NAME"                     then params N and the forms, variants N, and for each
                                 `variant "V" K` and each of its K fields `field "n"` (or `-`)
                                 with its type form below
```

The macros, structs and enums are sorted by key or name as byte strings
(a `Map` has no order the dump may rely on; fibber compares `str` byte
by byte: `(< "abc" "abd")`, `(< "é" "z")` is false). The prelude's own
types are most of any context (214 lines in the prelude's), so a program
lists only the entries that are **not as they were when its prelude had
been expanded**: new ones and redefined ones, compared as the text of the
entry with every position naming its file. `--prelude --context` lists
all, the built-in `Option` and `Form` among them. A module's context
adds up: a later module lists what an earlier one defined too.

### 5.3 Layout, and who ports what

`compiler/expand/` holds the expander, the modules named after the Rust
files they replace (§1). Five agents work in parallel, on disjoint
modules; the line counts are the Rust files' at the date of this section
(2026-10-01, before library tranche 0 changes `quasi.rs`, `derive` and the
`prelude` macros, §5.7).

| Module | Replaces | Lines | What | Porter |
|---|---|---|---|---|
| `expand.error` | `error.rs` | 260 | `ExpandErrKind`, `ExpandError`, the names and the messages | 1 |
| `expand.build` | `build.rs` | 97 | `sym`, `list`, `vector`, `call`, `unit`, `int`, `string`, `boolean`, `keyword`, `head-name`, `malformed`, `check-arity` | 1 |
| `expand.heads` | `heads.rs` | 72 | the core forms, the definitions, `primitive-operand` | 1 |
| `expand.private` | `private.rs` | 48 | `marker-index`, `take-marker`, `put-marker`, `type-name` | 1 |
| `expand.collections` | `collections.rs` | 51 | `vec-literal`, `map-literal`, `prelude-ns` names | 1 |
| `expand.modules` | `modules.rs` | 345 | `spec-of`, `try-load`, `begin-spec`, `expand-all`; the driver (file names, order, cycles) | 1 |
| `expand.types` | `types.rs` (and the structs of `runner.rs`, `ctx.rs`, `modules.rs`) | 307 | the data (skeleton) and `parse-defstruct`, `parse-defenum`, `mentions`, `variant-as-form`, the built-in `Option` and `Form` | 2 |
| `expand.ctx` | `ctx.rs`, `inspect.rs`, `reflect.rs`, `admit.rs`; the `NoRunner` of `runner.rs` | 558 | the context and every `ctx-*` function; `Runner`, `no-runner` | 2 |
| `syntax.number` | `syntax/number.rs`'s `check_literal` | 35 | `check-literal`, which `admit` needs and the reader's port does not have (§5.7) | 2 |
| `expand.core` | `core.rs` | 154 | `Role`, `expr-plan`, `skip-annotations`, `body-start`, `annotated-name` | 3 |
| `expand.quasi` | `quasi.rs` | 162 | the quasiquote rewrite, with an explicit stack | 3 |
| `expand.derive` | `derive/{mod,structs,enums}.rs` | 420 | `derive` | 3 |
| `expand.prelude` | `prelude/{mod,logic,forms,loops}.rs` | 492 | the macros of syntax §4.4 (`when` .. `dbg`, `for-each`, `range`) | 4 |
| `expand.dumpctx` | `expand_dump/context.rs` | 197 | the context lines of §5.2 | 4 |
| `expand.args` | `expand_dump/args.rs` | 184 | the words of §5.1 | 4 |
| `syntax.dump` | `dump.rs`'s `span_in`, `dump_form` with a home | 60 | `dump-stx` with a home file (an additive function: the reader's dump does not change) | 4 |
| `expand.expr` | `expr.rs` | 207 | `Expander`, `expand-head`, `plan-expr`, `finish-form`, the user-macro call | 5 |
| `expand.walk` | `walk.rs` | 166 | the walk of a tree by roles | 5 |
| `expand.top` | `top.rs`, `runner.rs`'s `parse_params` | 258 | the top-level forms, `defmacro`, the `:private` markers | 5 |
| `expand.program` | `expand/mod.rs` | 121 | `expand-program`, `expand-expr`, `prelude-source`, `expand-prelude`, `prelude-forms` | 5 |
| `expand.dump` and `compiler/expand.fib` | `expand_dump/mod.rs`, `main.rs`'s `expand` | 286 | the dump of §5.1, the tool | 5 |

About 900 lines of Rust each: 873, 900, 736, 933 and 1,038 for the
porters 1 to 5 (4,480 in all, the tests of `modules.rs` not counted). The
three modules of the skeleton are written already:
`compiler/expand/{error,types,ctx}.fib` hold the types every porter must
agree on (`ExpandErrKind`, `ExpandError`, `MacroDef`, `StructInfo`,
`EnumInfo`, `Limits`, `ModuleScope`, `ModuleSpec`, `Loaded`, `LoadErr`,
`ExpandCtx`, `Runner`) and the functions of `ctx` and `error` as stubs
that trap `todo: MODULE FUNCTION`; they compile and run in both tools.
Nothing in them changes without telling the other porters.

**The dependencies** (a module needs the ones above it; the Rust files'
`use` lines): `error`, `build`, `heads`, `private`, `collections`;
`types`, `ctx` (with `reflect`, `admit`, `inspect`), `core`; `quasi`,
`derive`, `prelude`; `expr`; `walk`, `top`; `program`; `modules`, `dump`.
One cycle of the Rust crate is cut: `ctx.rs` uses `MacroDef` of `runner.rs`
and `runner.rs` uses `ExpandCtx`, so `MacroDef` is in `expand.types` and
the runner in `expand.ctx`. Modules are checked in dependency order, so
functions that call each other must be in one module (syntax §5): none
of the Rust modules above calls back into the one that calls it, apart
from that cycle.

**The order of work.** First, each porter makes its modules with every
public function as a `todo:` stub of its final signature (the Rust one,
`Form` as `Stx`, `Result` as `util.result`'s), so that the others compile
against it; the porters of `ctx` and of `build` finish those first, since
every other module calls them. The porter of the driver wires `walk`,
`top` and `program` early, so that `compiler/expand.fib` runs on programs
and each module that lands moves the comparison. A module is tested
through `fibref expand FILE` and the tool on small programs, and through
the test of §5.6 on the corpus; the Rust unit tests of
`crates/fibref/src/expand/tests/` (1,400 lines) are what to read for
what each rule is, and each becomes an input file
`compiler/tests/expand/NAME.fib`, which the corpus picks up. The porters
do not start the checker.

### 5.4 The two stages

**Stage 2a** has no user macros: every program is compared as `--no-runner`
expands it. A program with no `defmacro` expands as it would with a runner
(none is called); one with it expands up to its first call of a user macro,
which is the record `MacroNeedsEvaluator`, so the registration of macros
(`defmacro` kept in the program with its body expanded, `MacroDef`s in the
context, the arity check before the runner, `MacroNamesCoreForm`, private
and qualified lookup, `AmbiguousMacro`) is compared at 2a too.

**Stage 2b** runs them: the runner is `compiler/lair/expand.fib` and the C
interface (`lair.jit`, `lair.call`, `lair.fibm`) over a macro-time module
the way `fibc`'s `JitRunner` does it, and the oracle for it is the
interpreter's evaluator, `eval::macros` (the two Rust runners agree on the
cases: `crates/fibc/tests/macros.rs`). Every program is compared with the
runner as well (no `--no-runner`), and the programs a limit must stop.
The interpreter's evaluator and the JIT's runner were found to disagree
twice while this section was written (§4, the last two rows): once in
which macro of a name runs (fixed), once in the positions of a result's
nodes that are forms the macro was given (open): the oracle is the one that
follows syntax §1.3, so **a 2b port that is a port of `JitRunner` cannot
match it** on `expand_modules`-style inputs until one of the two is
changed.

The stage is `BOOTSTRAP_EXPAND_STAGE=2a|2b` for one run, else the first
line of `compiler/expand/stage` that is not a comment (`2a` now; the
porters change it to `2b` in the commit that makes macros run), else `2a`;
any other value is an error and not a default.

### 5.5 The context in fibber

No global state. `ExpandCtx` (`compiler/expand/ctx.fib`) is a struct of
`Cell`s over immutable `Map`s, `Vec`s and structs, as the reader's cursor
is a struct of `Cell`s: every function takes it and changes it with
`set!`, and what Rust's `&mut ExpandCtx` borrowed is shared by the
callers. Its fields are the Rust ones: `limits`, `gensyms`, `macros`
(by `ns/name`), `exports` (by `ns`), `scope`, `structs`, `enums`,
`private-types`, `hidden-types`, `call-pos`, `steps`, `forms`; the sets are
`Map`s of `true`. A runner is a `Runner`, a struct of one function
`(fn (MacroDef (Vec Stx) ExpandCtx) (Result Stx ExpandError))`: stage 2a's
is `no-runner`, stage 2b's a closure over the state of
`lair/expand.fib`. The skeleton runs in both tools (a context made, a
struct added to a `Cell`ed `Map`, `no-runner` called through its field:
result and audit clean).

**What the later passes take** (so that the types agree before the checker
is started): `expand-all` of `expand.modules` returns `(Result (Vec
Expanded) ExpandError)`, an `Expanded` being a module's `ModuleSpec` and
its forms, in dependency order, the main module last; `prelude-forms` of
`expand.program` returns the prelude's forms as `(Result (Vec Stx) str)`
(the error as text, as `types::prelude_forms` does); `expand.heads` and
`expand.private` give the checker `is-core` and `marker-index`, and
`expand.collections` `prelude-ns`; and `ctx-new` followed by
`ctx-enum-info` of `"Option"` and `"Form"` gives it the variants and the
field types of the two built-in enums, which `--prelude --context` pins.

### 5.6 How the expander is judged

`crates/fibc/tests/bootstrap_expand.rs` (with `bootstrap_expand/`) builds
`compiler/expand.fib` with `fibc` and compares its output and exit status,
byte for byte, with `fibref::expand_dump::expand_files`, which is what
`fibref expand` prints, over groups of files, each run in batches of 100,
at most four processes at a time, each capped at 4 GiB of address space
and two minutes, from the repository root:

1. every program of `cases/ownership`, `cases/modules` (a directory with a
   `main.fib` is one program), `lib/` (but `lib/prelude.fib`) and
   `compiler/` (every file as a main file: the 984 inputs of
   `compiler/tests/reader/` among them, each a reader input, and the
   edge inputs of `compiler/tests/expand/` the porters add), each told
   apart by whether any module of it defines a macro (a `defmacro` form
   anywhere, in the program's modules, or in the text of a file that
   does not load);
2. `lib/prelude.fib` as a prelude (`--prelude`), plain and with its context;
3. generated programs from fixed seeds: 360 in all (`gen.rs`), from four
   seeds, in a pattern that repeats every twelve: mutations of the cases
   that keep them reading (delete a form, duplicate one, swap two, wrap
   one in `(do ..)`, rename a symbol), token soup of the prelude macros
   with operands at random, well formed and not (taken from
   `PRELUDE_MACROS`, so a macro added later is reached), programs of user
   macros (quasiquote and splice, `gensym`, reflection, macros that build
   definitions or call macros, one that shadows a prelude macro, calls with
   a wrong arity), programs of several modules (the same macro name in two
   modules, a `:private` macro and struct, `:use` and `:require` in the
   ways of syntax §5, and a few that do not load: a module that is missing,
   a cycle, a name that does not match its file, a malformed `ns`, one that
   does not read) and programs that only a limit stops (`loopy`, `grow`,
   `deep`);
4. fixed inputs (`edge.rs`), one for each way expansion can fail, the
   rewrites the cases show once, the module errors, and nesting of 150 to
   900 levels, a vector of 1,000 elements and an `and` of 2,100 operands (the Rust expander walks with
   an explicit stack, and a port that recurses on the native stack would
   not survive them);
5. paths that cannot be read, and the tool run with words that name no
   program.

Each is run plain, with `--context` and under three smaller limits
(`--max-steps 2`, `--max-depth 10`, `--max-forms 150`); at stage 2b with
and without a runner, and the generated programs under the evaluator are
capped at 500 steps and 50,000 forms (a mutated macro may call itself,
and the default limits would let it run for minutes). The counts of the
standard run, and the list of programs that need a runner and of those
that do not, are printed (`--nocapture`) and the lists written to
`target/tmp/bootstrap_expand/stage-2a.list` and `stage-2b.list`.

**The tool does not exist yet**, so until `compiler/expand.fib` does the test
`the_self_hosted_expander_matches_the_rust_expander` prints a loud message
and skips; it fails once the file exists and the output differs
(`BOOTSTRAP_EXPANDER=path` builds another source in its place, and a path
that is not a file is an error, not a skip). Everything it is made of is
judged without the tool: the oracle runs over every group of stage 2b and
reaches each of 31 named kinds of error (the 25 of `ExpandErrorKind`, the
four of loading, two of reading; a list in the test, not derived from the
enums); the generators are deterministic and reach every prelude macro;
stand-in tools that replay the oracle, faithful or with a character changed,
a line missing, another status, a missing final newline or a crash, are
passed or reported, and localized to the one file of a batch; a fault
planted in the replay of the whole corpus is reported and names its file;
and small tools written in fibber and built by `fibc`, which print the
oracle's text, one character of it changed, another status or a trap, are
passed or reported through the real build, run and comparison.

### 5.7 Doubts and gaps

- **The oracle moves.** The expander is being changed by library tranche 0
  (`spec/stdlib.md` §7: E1, E10, B2, E12, E7) while this is written: `derive
  Hash` now calls `fib.prelude/hash-combine`, quasiquote builds
  `fib.prelude/List` and `fib.prelude/concat`, a bare macro name two `:use`d
  modules define is `AmbiguousMacro`, `(:export-from ..)` and `reexport`
  exist. The dump follows whatever the working tree's expander does, so the
  comparison is always with the current Rust; but the porters port the
  committed expander, and each such change is a change to their module:
  `quasi`, `derive`, `prelude`, `ctx`, `modules` (which now looks for a
  module under library roots too: `roots.rs`). A port finished before
  tranche 0 lands is a port that tranche 0 reopens.
- `crates/fibref/src/syntax/number.rs`'s `check_literal` (an `Int` must fit
  its width, a `Flt` must be finite and an `f32` one is rounded) has no
  counterpart in `compiler/syntax/number.fib`, and `admit` needs it.
- The messages of `assert`, `dbg` and `cond` hold a `Pos` (`FILE:L:C`) and
  a `Form` as `syntax.print` writes it, so the file names given to the tool
  are part of its output; the test passes absolute paths.
- A struct, enum or macro is kept in a hash map in Rust and has no
  registration order: the dump sorts, so the port needs a sort of strings
  (`<` on `str` exists in both tools).
- The Rust expander walks, rewrites quasiquotes, admits results and checks
  depth with explicit stacks because macros build trees up to 2,000
  levels deep; the port's stack is the native one (8 MB, 4 GiB of address
  space in the test), so these four are written with an explicit stack too.
- **Positions of what a macro was given** (§4, the last row): the JIT's
  runner and so `lair/expand.fib` give every node of a macro's result the
  call's position, the interpreter's evaluator (the oracle) keeps the
  position of a form that came from the arguments. Until the JIT's runner
  or the port records the argument objects it hands the macro and maps a
  result node that is one of them back to its position, stage 2b cannot
  match the oracle on a macro that returns one of its arguments (cases
  `cases/modules/005`, `006`, `012`).
- 2b's messages for a macro that fails at run time (`MacroFailed`) are the
  evaluator's; if the JIT's differ the porter reports it and does not
  change either.
| `JitRunner` (and `lair.fibm`'s `to-stx`) gives every node of a macro's result the position of the call, so a form the macro took from its arguments loses its own position; the interpreter's evaluator keeps it, as syntax §1.3 says ("a form taken from the input keeps its own"): `(+ (m/thrice 5) ..)` has `int 5 i64 12:13 437..438` interpreted and `int 5 i64 12:6 430..439` compiled | the position-sensitive comparison of the two runners over `cases/modules` (cases 005, 006 and 012 of the module cases differ; `jit_expansions_equal_the_interpreters` compares `Form`s, which ignore positions, so nothing saw it) | **open, reported, not fixed**: `crates/fibc/tests/macros.rs` `jit_and_interpreter_expand_the_module_cases_alike_at_every_position` fails and is `#[ignore]`d with this reason. The oracle of stage 2b (§5.4) is the interpreter's evaluator, which follows §1.3; a 2b port that follows `JitRunner` will differ from it in these positions until the JIT's runner records the objects it made of the arguments and gives a result node that is one of them its position (the interpreter's `recording_inputs`), or the port does |
