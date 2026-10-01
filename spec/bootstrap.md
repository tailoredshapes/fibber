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
| `util.text` | | string helpers the passes share |
| `syntax.pos` | `syntax/pos.rs` | `Pos` (file, line, col, byte range) |
| `syntax.form` | `syntax/form.rs` | `Stx` (a `StxKind` and its `Pos`), `IntWidth`, `FltWidth` |
| `syntax.error` | `syntax/error.rs` | `ErrKind`, `ReadError`, the messages |
| `syntax.chars`, `syntax.cursor` | `chars.rs`, `cursor.rs` | character classes; a cursor over the UTF-8 bytes |
| `syntax.number`, `syntax.literal` | `number.rs`, `literal.rs` | numbers; strings and characters |
| `syntax.reader` | `lexer.rs`, `reader.rs` | text to `Stx` |
| `syntax.print` | `print.rs` | `Stx` to text that reads back |
| `syntax.dump` | `crates/fibref/src/dump.rs` | the reader dump (§2) |
| `lair.ffi` | (the C side of `lair`'s `capi/`) | the bytes and words C reads (a `str` copied into a raw buffer, out-parameter slots), a `lair_error` as a `(Result .. str)`; every C handle is an `i64` (below) |
| `lair.jit` | `capi/{jit,check,aot}.rs` | `jit-new`, `jit-free`, `jit-add-source`, `jit-address`, `jit-c-entry`, `check-source`, `build-executable` over `lair`'s C interface (compiler.md §9) |
| `lair.call` | `capi/{call,mailbox}.rs` | `call-i64`, `call-f64`, and the mailbox: `call-new`, `call-start`, `call-wait` (`Done` or `Hook` of its arity), `call-hook-arg`, `call-hook-reply`, `call-result`, `call-fault`, `hook-addresses` |
| `lair.fibm` | `crates/fibc/src/macros/module.rs` (`Fns`) | a macro-time module as the compiler sees it: the addresses of `fibm.NAME.K`, `Stx` to and from the module's `Form` objects |
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
2. the files of `compiler/tests/reader/`, written to hit each rule of
   syntax §1 and each read error, every line of the Rust reader's own
   tests, and the inputs that killed the mutants of the reader that the
   test alone had let survive (`rmut-*`);
3. one small file for every scalar value that the whitespace and
   control classes decide, with its neighbours and look-alikes;
4. inputs generated from a seed by mutating the corpus and by composing
   tokens, equal in number on every run of the test (a few hundred,
   never thousands: the machine has been taken down by sweeps before);
5. paths that cannot be read, and the tool with no file.

The test fails if the two outputs differ in any byte. Unit tests show
that a difference in one position digit, a missing line, another exit
status or a missing final newline is each reported, and a *canary* runs
the real pipeline with a fault planted in the real reader and shows it
reported. A review of the reader by mutation (1797 mutants of the
fibber source, 1886 inputs each) is how the inputs of item 2 were found:
a test that agrees with the oracle everywhere it looks says nothing
about where it does not look.

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
| the audited interpreter keeps memory proportional to work (about 100 KB per top-level form, 1.3 GB for a 1000-deep nest) | robustness review | noted: run the interpreter audit on small inputs only; the compiled reader is linear (8 MB in 0.5 s) |
| `(args)` traps on an argument that is not UTF-8 | robustness review | fixed, and the description was wrong: a built executable did not trap, it made a `str` that was not UTF-8, and `fibc run` and `fibref run` panicked in `std::env::args`; now each invalid sequence is U+FFFD as `String::from_utf8_lossy` makes it, in all three (syntax §4.3; `crates/fibc/tests/cli/args.rs`, `crates/fibref/tests/run_io.rs`); a tool's own word before `--` that is not UTF-8 is refused (exit 2) |
| `println` ignores a failed write (a full device exits 0) | robustness review | fixed: `println` and `eprintln` write in a loop until every byte has gone and trap `println: write failed` (`eprintln: write failed`) on an error (syntax §4.5; `crates/fibc/tests/cli/writes.rs`, `crates/fibref/tests/run_io.rs`, case 192 for the normal path) |
| the reader needs 480 to 512 KB of native stack at nesting 1000 (about 500 bytes a level) | robustness review | noted: a worker thread gets the main thread's soft limit, 8 MB here, and all the reader's inputs were read correctly in `pmap` workers |
| `fibc build` had no `-L`, `-l` or rpath: a library outside the system's directories could not be named, and a found one needed `LD_LIBRARY_PATH` to run | the C interface (`-l lair`, compiler.md §9) | fixed: `fibc build FILE -o OUT -L DIR -l LIB`, each `-L` also an absolute rpath (compiler.md §1; `crates/fibc/src/link.rs`, `crates/fibc/tests/cli/link.rs`, which runs the executable without `LD_LIBRARY_PATH` and from another directory). The step is in `fibc` because `lair::aot::Options` has `libs` and no directories; it moves to `lair` when that has them |
| a struct, enum variant, `Cell` or `Option` that holds a raw `ptr` releases it when dropped: `child_fields` in `crates/fibc/src/objects.rs` treats every `ptr` field as an object (a raw `ptr` and an object pointer are both lIR `ptr`), so `fib.release` decrements the first word of the user's block and, at zero, frees it | `lair.jit`'s `Libs` struct (a heap abort: "corrupted double-linked list") | **open**, stage-1 bug: the interpreter treats `ptr` as uncounted (types §8.1) and answers 41 where the compiled program answers 40 (`crates/fibc/tests/capi/stage1.rs`, `#[ignore]`d: it fails today). Worked around in `compiler/lair/`: a block that must live in a value is held as its address, an `i64` |
| fibber has no `f64` to bits conversion, and `lair_call_i64` passes no double, so a `Flt` form cannot be handed to a macro module (`fibm.form` takes the bits as an integer) | `lair.fibm` `to-object` | worked around: `sscanf("%lf")` of `(show v)` into a slot, then `load-i64` (bit for bit over 15 values, `crates/fibc/tests/capi/macros.rs`). **Proposed**: a primitive `(bitcast i64 x)` for `f64` and `f32`, which also gives `f64::to_bits` to the reader's printer |
| the macro module's keyword table (the ids of `:i8`..`:f64` and of every other keyword a macro mentions) is in `MacroModule::keywords`, not in the module's text, so a stage-2 expander cannot name a width id past 5 | `lair.fibm` on case 105 (`(Flt 2.5 :f16)`) | **open**: fibber assumes ids 0 to 5 are i8 i16 i32 i64 f32 f64 (`compile_macro` interns them first; `crates/fibc/tests/capi/macros.rs` asserts it) and names any other `#ID` where the Rust runner says `f16`. **Proposed**: the module exports the table (a function from id to name, or its text) |
