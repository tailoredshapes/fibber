# Learn Fibber the Hard Way

Learn by typing, predicting, running and repairing small programs. This is an
original exercise course for Fibber. You do not need to know Lisp or another
programming language. You do need a text editor, a terminal and time to repeat
an exercise when your prediction differs from the result.

The [short tutorial](../tutorial/01-install-and-hello.md) is a quick tour. This
course asks you to build the habits behind that tour. Its finish line is a
tested native command that counts words and nonblank lines in a text file.

## How to work

1. **Type it.** Put each program in the named file. Copying is fine when typing
   is inaccessible; still read and explain each line before running it.
2. **Predict it.** Write down the output, including the final main result from
   `fibc run`. Do this before uncovering the answer.
3. **Run it.** Compare your prediction with the exact output. Keep a notebook of
   differences. A surprise is a question to investigate.
4. **Change it.** Complete the drills. Change one thing at a time, then predict
   and run again. Keep the original file so you can return to it.
5. **Break it.** Save deliberate mistakes in a separate file. Record the
   diagnostic, explain it in your own words and make the smallest repair.
6. **Explain it.** Answer the checkpoint without looking at the code. Continue
   when you can explain the result and reproduce it.

Run commands at the `$` shell prompt; do not type that prompt. Fibber source goes
in `.fib` files. The book's code blocks omit line numbers so numbers never become
part of your program. Code and shell commands are different languages.

Most examples end `main` with `0`. The JIT command prints that result after your
program's output. A native executable uses it as its exit status instead.
Expected-output answers are folded away: predict first, then open them.

There is no deadline. Spend one session on an exercise, or several. Do the
checkpoint from memory the next day before continuing. Use the
[library index](../reference/library/INDEX.md) when a drill asks you to find a
function; do not guess an API from another Lisp.

## Your route

| Exercise | Practice | Ready when you can… |
|---|---|---|
| [0. Set up your workbench](00-workbench.md) | editor, terminal, run/build | run the same file two ways |
| [1. Read a program aloud](01-reading.md) | forms, strings, comments | explain every delimiter |
| [2. Name a calculation](02-numbers.md) | arithmetic, `let`, types | predict a changed calculation |
| [3. Work with text](03-strings.md) | strings, escapes, module aliases | build a sentence from values |
| [4. Make a decision](04-decisions.md) | booleans and `if` | cover a boundary and both branches |
| [5. Write a function](05-functions.md) | parameters, returns, `do` | separate a value from printing |
| [6. Keep a collection](06-vectors.md) | vectors, persistence, indexing | preserve the old value |
| [7. Look up a key](07-maps.md) | maps and defaults | distinguish missing from zero |
| [8. Repeat with a loop](08-loops.md) | `loop`, `recur`, termination | trace each iteration on paper |
| [9. Transform a collection](09-transformations.md) | `mapv`, `filterv`, `reduce` | describe the shape of each stage |
| [10. Handle alternatives](10-matching.md) | structs, enums, `Option` | handle both present and absent |
| [11. Follow ownership](11-ownership.md) | borrowing and escaping values | read an explanation without adding lifetimes |
| [12. Change a local value](12-mutation.md) | cells and in-out places | diagnose an alias rejection |
| [13. Keep failures visible](13-failures.md) | `Result`, traps, safe lookup | choose an error policy deliberately |
| [14. Write a behaviour spec](14-specs.md) | tests and a planted failure | show a test detecting wrong behaviour |
| [15. Start a project](15-project.md) | namespaces, paths, dependencies | run and test from a project directory |
| [16. Ship a useful command](16-capstone.md) | files, arguments, native exit codes | pass the acceptance table and explain the program |

## When you get stuck

Read the first diagnostic before fixing later ones. Check spelling, balanced
delimiters and the file you actually ran. Reduce the program to the smallest
version that still surprises you. Ask for help with that version, the command,
the output and your prediction. Save the answer in your notebook.

The starting programs and deliberate rejection cases are checked by
`make doc-examples`; the capstone also has executable acceptance checks. Drills
are assignments for you to solve, not claims that an unrun modification passes.
Fibber is pre-1.0; [known limits](../policy/limits.md) describe current boundaries.

[Begin with exercise 0 →](00-workbench.md)
