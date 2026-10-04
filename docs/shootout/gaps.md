# Gaps found by the shootout programs

One section per gap: what is missing, the smallest reproduction, what the Java/C version uses, and a recommendation.
Sections are tagged with the package that found them.

## RE-1. Reading an array out of a `Cell` retains and releases it at every use

- **Missing.** A loop that reads `@c` (a `Cell` holding an `(Array t)`) and indexes it makes a `fib.retain` and a `fib.release` call
  around each `array-get`, because the read of the cell gives the loop a counted reference. The calls are not inlined (the runtime is
  separate), so a table lookup costs two calls and a count update.
- **Reproduction.** `(defun f (c: (Cell (Array i64)) n: i64) -> i64 (loop [i 0 s 0] (if (< i n) (recur (+ i 1) (+ s (array-get @c i))) s)))`,
  then `fibc emit` shows `(call @fib.retain t..)` and `(call @fib.release t..)` inside the loop (also visible in the lIR of
  `f.fib.regex.dfa.dfa-forward` before the rewrite).
- **Java/C.** A field read; no count.
- **Evidence.** regex-redux, one counting pass over 50 MB: 160 ms with the table in a cell, 78 ms when the scan loop takes the table as a borrowed
  parameter (`fast-forward` in `lib/fib/regex/dfa.fib`, which the library now uses: the program holds a snapshot of the table for the loop, and
  writes new entries through the cell outside it). Writing a loop that way is a contortion a fibber programmer should not need.
- **Recommendation (compiler).** A `@cell` read whose only use is an `array-get`/`array-len` in the same expression needs no retain: the cell keeps
  the array alive. Failing that, an in-place borrow form for a cell's content.

## RE-2. `str-byte-at` is a call into the runtime

- **Missing.** A byte of a `str` costs a function call; `fib.str-byte-at` is 8.0% of the self time of regex-redux, in a loop that is otherwise
  one load, one table lookup, one table lookup.
- **Reproduction.** `fibc emit` of any loop over `(str-byte-at s i)`: `(call @fib.str-byte-at p1 t..)`.
- **Java/C.** `charAt` is an intrinsic; C indexes.
- **Recommendation (compiler).** An intrinsic: bounds check, load of the byte, as `array-get` has.

## RE-3. No string builder

- **Missing.** `StrBuf` (spec §9 Q26). `str-concat` in a loop is quadratic. `fib.regex` builds its results by hand: sums the piece lengths,
  fills one `(Array i8)` and calls `str-from-bytes` (`join-pieces`, `replace-literal` in `lib/fib/regex/api.fib`), as `str/join` does.
- **Java/C.** `StringBuilder`; `realloc`.
- **Recommendation (library).** `StrBuf` in `fib.string` over a growable `(Array i8)`, as Q26 says; `replace-all` would use it.

## RE-4. No way to read all of a file descriptor

- **Missing.** `fib.unix` has `fd-read` (at most n bytes); a program that wants stdin as one `str` writes the loop and the copy itself
  (`read-chunks`, `join-chunks` in `scripts/shootout/regex-redux/regex-redux.fib`: 115 ms of the 50 MB).
- **Java/C.** `System.in.readAllBytes()`; a `read` loop.
- **Recommendation (library).** `(unix/fd-read-all fd)` -> `(Result (Array i8) i64)` growing by doubling, and `str-from-bytes`.

## RE-5. The reference interpreter cannot run this tree's library

- **Missing.** `fibref` (the Rust interpreter) rejects the current prelude, so method rule 6 (interpreter and compiled agree) cannot be
  checked for any case that uses the library; the cases of `6000` and `6001` were run compiled only (`fibc cases`, stage 2).
- **Reproduction.** `fibref run scripts/bench/hello.fib` with `FIB_LIB=lib`: `lib/prelude.fib:214:33: unbound name array-pop!`
  (also `array-take!`); with the embedded prelude: `lib/fib/core/cells.fib:23:4: unbound name fib.prelude/cell-update!`.
- **Recommendation.** Bring the interpreter up to the prelude, or record that rule 6's interpreter half is retired since the flip.

## RE-6. The `stdlib_table` test fails before any change

- **Reproduction.** `cargo test -p fibref --test stdlib_table`: `spec/stdlib.md §4: line 1358: the tranche `L6` is no number`
  (7 of 38 tests fail; unrelated to `fib.regex`, which only adds rows to §4.9 and cases `6000`, `6001`).
- **Recommendation.** Fix the row, or the test's reading of it. So `covers:` of the new cases is checked by hand against §4.9.

## RE-7. No regex literal and no `str/replace`

- **Missing.** `#"re"` (E14, a reader rule) and `str/replace`/`re-quote-replacement` in `fib.string` (tranche 3, 4). The programs call
  `re-pattern` on strings, so a backslash is written `\\`; `fib.regex` has its own `replace-all`, `replace-first`, `re-quote-replacement`.
- **Java/C.** `Pattern.compile("..")`, with the same doubled backslash.
- **Recommendation.** The reader rule; then `str/replace` over `Pattern` can call `fib.regex`'s functions.
