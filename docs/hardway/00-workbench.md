---
examples: required
---

# Exercise 0: Set up your workbench

**Aim:** edit a file, run it and build a native executable. No previous terminal
experience is assumed. An editor changes a file; a terminal runs commands.

## Prepare

Follow [installation and platform setup](../tutorial/01-install-and-hello.md#platform-setup-and-release-layout).
Keep the unpacked `bin/` and `share/` directories together. Confirm the compiler
is available with `fibc --version`. Building needs the platform's C compiler,
`cc`; the release includes LLVM, so you need no LLVM installation for this course.

Create a directory for your work and enter it:

```sh
mkdir fibber-hardway
cd fibber-hardway
pwd
```

`mkdir` creates a directory, `cd` changes the shell's current directory and `pwd`
prints it. Your editor must save files in this same directory. Use plain text,
not a word processor, and save the following as `ex00.fib`.

## Type and predict

```fib run
(defun main () -> i64
  (do
    (println "workbench ready")
    0))
```

<details markdown="1">
<summary>Check your prediction</summary>

`fibc run` prints:

```text out
workbench ready
0
```

`./ex00` prints only `workbench ready`. The `./` asks the shell to run a file in
the current directory. Its exit status is zero; `echo $?` immediately afterward
prints `0`. The compiler's `run` command prints main's result; the executable
passes it to the shell as a status.

</details>

Run these commands after typing the program, before uncovering the answer above:

```sh
fibc run ex00.fib
fibc build ex00.fib -o ex00
./ex00
```

## Change it

1. Change the message to your name. Run the file, then rebuild and run `./ex00`.
2. Change main's last value to `7`. Predict both runs and the native exit status.
3. Restore `0`. Make a second file `ex00-again.fib` by typing from memory.

## Break and repair

Save a copy as `ex00-broken.fib`. Remove one closing parenthesis. Run it and
record the first diagnostic. Restore the parenthesis and run again. Do not
rebuild the original executable until you can explain which file you changed.

## Checkpoint

- Show that your editor and terminal use the same directory.
- Explain why editing `ex00.fib` does not change an already-built `./ex00`.
- Explain the extra `0` from `fibc run` without calling it your printed message.

[Course](README.md) · [Next: read a program aloud →](01-reading.md)
