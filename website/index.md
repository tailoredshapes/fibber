---
title: A Lisp with a native instinct
description: Lisp syntax, static types and native code. Fibber borrows first and counts second, with memory safety and no garbage collector.
examples: required
hide:
  - navigation
  - toc
  - footer
---

<div class="fib-home" markdown="1">

<section class="fib-hero" markdown="1">

<div class="fib-hero-copy" markdown="1">

<p class="fib-eyebrow">FIBBER / A NATIVE LISP</p>

# Lisp syntax.<br>Native code.<br><span>No garbage collector.</span>

Write expressive programs with macros and static types. Fibber brings ownership
checking to Lisp, then compiles your code through LLVM to a native binary.

<div class="fib-actions" markdown="1">

[Try Fibber →](../docs/tutorial/01-install-and-hello.md){ .fib-button .fib-button-primary }
[Explore the library](../docs/reference/library/INDEX.md){ .fib-button .fib-button-secondary }

</div>

<p class="fib-release">{{ release_version }} · Pre-1.0 · Linux x86-64 &amp; Apple Silicon</p>

</div>

<div class="fib-code-window" markdown="1">

<div class="fib-code-title"><span class="fib-status-dot"></span> hello.fib <span>LISP → NATIVE</span></div>

```fib run
(ns main)

(defun greet (name: str) -> str
  (str "hello, " name))

(defun main () -> i64
  (do (println (greet "world"))
      0))
```

<div class="fib-code-command" markdown="1">

<pre><code>fibc run hello.fib</code></pre>

</div>

```text out
hello, world
0
```

</div>

</section>

<section class="fib-principles" markdown="1">

<div markdown="1">

<span class="fib-number">01 / MEMORY</span>

## Borrow first. Count second.

Values within their creating scope live and die with it. Escaping values are
reference counted. The compiler chooses; you write neither lifetimes nor
retain/release calls.

[Meet the ownership model →](../docs/tutorial/04-ownership-by-example.md)

</div>

<div markdown="1">

<span class="fib-number">02 / EXPRESSION</span>

## Lisp, with static types.

Functions, persistent collections, protocols and pattern matching. Macros
operate on forms at compile time. Start small, then shape the language around
your program.

[Write your first functions →](../docs/tutorial/03-functions-and-types.md)

</div>

<div markdown="1">

<span class="fib-number">03 / EXECUTION</span>

## A compiler in Fibber.

The self-hosted compiler lowers through LLVM. Use the JIT while developing,
then build a native executable. SIMD, tensors and a work-stealing pool support
numerical and parallel work.

[Build and deploy →](../docs/guide/targets-and-deployment.md)

</div>

</section>

<section class="fib-get-started" markdown="1">

<div markdown="1">

<p class="fib-eyebrow">START WITH HELLO</p>

## From source to a binary.

Download the release, verify its checksum and unpack it. Keep `bin` and `share`
together, and add `bin` to your `PATH`. Save the example above as `hello.fib`.

Linux needs glibc 2.33+ and an AVX2/FMA CPU. macOS needs Apple Silicon.
Building an executable needs `cc` (Xcode command-line tools on a Mac).

[Installation & checksums](../docs/tutorial/01-install-and-hello.md){ .fib-inline-link }

</div>

<div class="fib-install" markdown="1">

=== "Linux x86-64"

    [Download {{ release_version }}](https://github.com/tailoredshapes/fibber/releases/download/v{{ release_version }}/fibc-{{ release_version }}-linux-x86_64.tar.gz)

    ```sh
    tar xzf fibc-{{ release_version }}-linux-x86_64.tar.gz
    export PATH="$PWD/fibc-{{ release_version }}-linux-x86_64/bin:$PATH"
    fibc run hello.fib
    fibc build hello.fib -o hello
    ./hello
    ```

=== "macOS arm64"

    [Download {{ release_version }}](https://github.com/tailoredshapes/fibber/releases/download/v{{ release_version }}/fibc-{{ release_version }}-darwin-arm64.tar.gz)

    ```sh
    tar xzf fibc-{{ release_version }}-darwin-arm64.tar.gz
    export PATH="$PWD/fibc-{{ release_version }}-darwin-arm64/bin:$PATH"
    fibc run hello.fib
    fibc build hello.fib -o hello
    ./hello
    ```

</div>

</section>

<section class="fib-library" markdown="1">

<p class="fib-eyebrow">TAKE THE NEXT STEP</p>

## A small start. Room to grow.

<div class="fib-library-grid" markdown="1">

<div markdown="1">

### Everyday programs

Persistent collections, strings, JSON and JSON Schema, regex, file and OS
services, HTTP and LZ4 compression.

[Browse modules →](../docs/reference/library/INDEX.md)

</div>

<div markdown="1">

### Numerical work

Typed tensors, SIMD kernels, matrix multiplication and reverse-mode automatic
differentiation. GPU kernels are an experimental opt-in with external drivers.

[Explore numerics →](../docs/guide/numerics-simd-tensors.md)

</div>

<div markdown="1">

### A development loop

Create a project, run behaviour specs, inspect ownership decisions and use
completion and diagnostics in VS Code.

[Set up your tools →](../docs/guide/tooling.md)

</div>

</div>

</section>

<aside class="fib-status" markdown="1">

**Built in the open. Still pre-1.0.** APIs can change. Platform coverage and
known limitations are explicit; the TLS client is **unaudited**.
[Read the limits](../docs/policy/limits.md) ·
[Contribute](../CONTRIBUTING.md) ·
[View the source](https://github.com/tailoredshapes/fibber)

</aside>

</div>
