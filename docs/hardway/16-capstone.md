---
examples: required
---

# Exercise 16: Ship a useful command

**Aim:** build `tally`, a native command that reports nonblank lines and words
in a UTF-8 text file. This is a small program with a stated contract, not a clone
of the system `wc` command. Stay in the project from exercise 15.

## State the contract first

The command accepts exactly one file path. A **nonblank line** has content after
trimming whitespace. For this exercise a **word** is a nonempty token separated
by ASCII spaces or tabs within that line. LF and CRLF end lines. A lone CR and
other Unicode separators are outside this word-separator contract. Unicode
letters inside a word are supported. An empty file reports zero and zero.

Output is exactly `lines=N words=N` followed by a newline. Success exits `0`.
Wrong argument count prints usage on stderr and exits `2`. An unreadable file
or invalid UTF-8 prints a read error on stderr and exits `1`.

Before looking at the implementation, write a function outline: read arguments,
validate their count, read the file, calculate totals and report a result.
Identify the pure calculation separately from file I/O and printing.

## Type and predict

Put this starting implementation in `src/main.fib`. Explain each function using
the earlier exercises. Predict the no-argument run before reading the answer.

```fib run
(ns main (:use fib.core fib.seq fib.coll)
  (:require [fib.string :as text]))

(defstruct Totals (lines: i64 words: i64))

(defun words-in (line: str) -> i64
  (let ((normal (text/replace line "\t" " "))
        (tokens (text/split-literal normal " ")))
    (count (filterv (fn (word: str) (not (text/blank? word))) tokens))))

(defun tally (input: str) -> Totals
  (let ((lines (filterv (fn (line: str) (not (text/blank? line)))
                       (text/split-lines input))))
    (Totals (count lines) (reduce + 0 (mapv words-in lines)))))

(defun report-file (path: str) -> i64
  (match (read-file path)
    (nil (do (eprintln (str "cannot read " path)) 1))
    ((some input)
      (let ((totals (tally input)))
        (do
          (println (str "lines=" (. totals lines) " words=" (. totals words)))
          0)))))

(defun main () -> i64
  (let ((arguments (args)))
    (if (= (count arguments) 1)
        (report-file (nth arguments 0))
        (do (eprintln "usage: tally FILE") 2))))
```

<details markdown="1">
<summary>Check the no-argument run</summary>

`fibc run src/main.fib` prints `usage: tally FILE` on stderr, and prints main's
result on stdout:

```text out
2
```

That `2` is the JIT's result display. Build the executable to check the actual
process exit status. `read-file` returns an `Option str`, preserving read/UTF-8
failure rather than substituting empty text.

</details>

## Make fixtures and build

```sh
printf 'red blue\n\n green\tgold \n' > sample.txt
printf '' > empty.txt
fibc run src/main.fib -- sample.txt
fibc build src/main.fib -o tally
./tally sample.txt
echo $?
```

The JIT run prints `lines=2 words=4`, then `0`. The native command prints only
`lines=2 words=4`; the immediately following `echo $?` prints its status `0`.
Spaces in file paths are fine when the shell argument is quoted.

## Acceptance checks

Create a fixture for each applicable row. Check stdout, stderr and the native
exit status separately. Read `$?` immediately after the command: another command
would replace it. Do not count a usage error as a successful empty-file result.

| Input | Stdout | Stderr | Exit status |
|---|---|---|---|
| `sample.txt` above | `lines=2 words=4` | empty | 0 |
| empty file | `lines=0 words=0` | empty | 0 |
| spaces/tabs and blank lines only | `lines=0 words=0` | empty | 0 |
| `red` with no final newline | `lines=1 words=1` | empty | 0 |
| `red\r\nblue\r\n` | `lines=2 words=2` | empty | 0 |
| `café tea\n` | `lines=1 words=2` | empty | 0 |
| missing path | empty | `cannot read PATH` | 1 |
| directory path | empty | `cannot read PATH` | 1 |
| file containing byte `FF` (invalid UTF-8) | empty | `cannot read PATH` | 1 |
| no arguments | empty | `usage: tally FILE` | 2 |
| two arguments | empty | `usage: tally FILE` | 2 |

Every message in the table has a final newline. The documentation checker builds
this program and exercises these rows; readers should reproduce them themselves.
For invalid UTF-8, `printf '\377' > invalid.txt` writes a single invalid byte.

## Change it

1. Move `Totals`, `words-in` and `tally` into `src/tally/counts.fib`, with
   namespace `tally.counts`. Require that module from main under an alias.
2. Write `specs/counts-spec.fib` for the pure `tally` function, checking both fields.
   Include empty input, repeated spaces, tabs, CRLF and the no-final-newline case.
3. Repeat every native acceptance row after the module move. Behaviour specs
   for the pure calculation do not check argument handling or file failures.

## Break and repair

Remove the token filter in `words-in`. Multiple spaces now introduce empty
tokens counted as words. Make a fixture `red  blue` and show the wrong result
before restoring the filter. Add a regression spec for that fixture.

Separately, change the usage branch to return `0`. Show the acceptance check that
detects this wrong exit status even though the message looks right. Restore `2`.

## Final checkpoint

Demonstrate a clean acceptance table, a caught planted failure and a repaired
run. Explain why the missing-file case cannot count as an empty file, why one
successful sample is insufficient, and why an already-built binary must be
rebuilt after editing its source. Ask someone else to run it on a new fixture.

You have completed the course when you can recreate this program from its
contract and the library reference, without copying the implementation.
Next try [concurrency](../tutorial/06-concurrency.md),
[protocols and enums](../guide/protocols-records-and-enums.md), or
[numerics](../guide/numerics-simd-tensors.md). Check
[target and deployment support](../guide/targets-and-deployment.md) before
shipping a binary to another machine.

[← Projects](15-project.md) · [Return to the course](README.md)
