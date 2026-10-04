# What the shootout taught us: proposed improvements

Source: the ten programs of the Benchmarks Game written in fibber, Java, Clojure and C by the shootout agents
(`scripts/shootout/*/`, the findings in `docs/shootout/gaps.md`). Every number below is an agent's measurement on a shared,
sometimes loaded machine (median of 3 to 5 runs, runs serialised on one lock): **indicative, not final**. The final table
comes from `scripts/shootout/run.sh` on a quiet machine. Payoffs marked *estimate* are mine, not measured.

Caveats that apply to everything: single-threaded in all four languages; the Java, Clojure and C programs are written by
agents who know those languages better than fibber, so the comparison is tilted against us; the C twin of k-nucleotide uses
a hand-written hash table.

## Where we stand (full size, elapsed)

| Benchmark | fibber | Java | Clojure (whole) | C | fibber / Java |
|---|---|---|---|---|---|
| regex-redux (25M) | 14.68 s | 36.97 s | 46.89 s | 9.09 s (PCRE2 JIT) | **0.40** |
| mandelbrot | 10.40 s | 10.46 s | 11.13 s | 10.32 s | 0.99 |
| reverse-complement | 0.75 s | 0.76 s | 1.58 s | 0.37 s | 0.99 |
| fasta | 2.64 s | 2.72 s | 3.22 s | 2.37 s | 0.97 |
| spectral-norm | 1.35 s | 1.11 s | 1.37 s | 0.51 s | 1.22 |
| pidigits (10000) | 5.23 s | 2.82 s | 3.66 s | 0.50 s (GMP) | 1.85 |
| binary-trees (21) | 7.35 s | 2.73 s | 3.76 s | 10.44 s | 2.7 |
| fannkuch-redux (12) | 74.26 s | 26.50 s | 31.94 s | 23.94 s | 2.80 |
| k-nucleotide | 45.84 s | 9.77 s | 10.35 s | 2.39 s | 4.7 |
| n-body (50M) | 12.04 s | 2.02 s | 2.58 s | 1.62 s | 5.96 |

Read: ahead of Java on regex-redux, level on three, within 25% on one, within 2x on one, 2.7x to 6x behind on four. Memory
is 1 to 3 MB on most programs against Java's 40 MB and Clojure's 100+ MB, and start-up is 0.00 s against 0.01 s (JVM) and
0.25 s (Clojure). Hello-sized programs and byte or float loops are where we are strong.

## What is strong (keep it, do not break it)

- **The regex engine** (Pike VM plus lazy DFA, linear time) beats `java.util.regex` 2.5x and is 1.6x from C's PCRE2 JIT.
- **Float loops and byte output** compile to what C does (mandelbrot, fasta, reverse-complement).
- **Memory footprint** and start-up.
- **Differential testing** against Java's `BigInteger` and `java.util.regex` found no bug in the final libraries.

## The proposals, in order of payoff for effort

Layers: **own** = ownership checker (`compiler/own`), **emit** = emitter (`compiler/emit`), **rt** = runtime
(`crates/fibc/rt/*.lir`), **lib** = standard library, **lang** = language or reader, **tool** = tooling.

### 1. A scalar read through `@cell` should not retain and release the cell's content (own, emit)
- Evidence: n-body does a retain and release of the array around every `(array-get @a i)` (about 17 pairs per body pair,
  ten pairs per step, found by `objdump`); regex-redux: a counting pass over 50 MB took 160 ms with the table in a cell and
  78 ms with the loop taking it as a borrowed parameter; the compiler itself paid for it in H2's name tables (`add-global`
  was 12% of samples until the sites moved to `update!`).
- Proposal: a deref of a cell consumed within one expression by a scalar-returning read (or any borrowed position) is a
  derived borrow of the cell content. Nothing can mutate the cell during the expression. Same soundness shape as batch 4's
  element-read rule.
- Moves: n-body, spectral-norm, fannkuch-redux, regex-redux, and any program keeping state in a cell. n-body is the largest
  single gap (5.96x), so this is the first lever to pull. Payoff *estimate*: n-body to under 3x.

### 2. Forwarded `&` parameters and owned array parameters should move the content, not copy it (own)
- Evidence: fannkuch-redux 30% in `array-slice` and `array-alloc`, because a forwarded `&p` is `acquire`, where a function
  owning the cell gets `move in` (case 261); inlining the helper by hand took N=10 from 0.55 s to 0.39 s. IO-3: 2.8 s with a
  helper writing through `&`, 0.012 s with the write inlined. BigInt: an owned array parameter cannot be updated in place
  by the callee, so every pidigits step allocates fresh arrays. n-body: `:owned` on a `defun` parameter is refused, and
  `array-with` on a borrowed array copies every call (1.0 s per million steps against 0.30).
- Proposal: (a) a forwarded `&p` moves the content in and back, as a cell argument does; (b) accept `:owned` on `defun`
  parameters so the callee may update a unique array in place.
- Moves: fannkuch-redux, k-nucleotide and reverse-complement helpers, pidigits, and every program that factors its loop
  into helpers. Without it, programmers inline by hand, which is a bad idiom to teach. Payoff *estimate*: fannkuch-redux to
  about 1.5x.

### 3. Do not allocate nullary enum variants (emit, types)
- Evidence: binary-trees written naturally as `(defenum Tree (Leaf) (Node l r))` takes 13.3 s and 1.05 GB at depth 21,
  against 7.2 to 8.8 s and 0.39 GB for the `(Option Node)` version. The agent suspects each `Leaf` is a heap allocation but
  did not read the IR. The same finding appeared in the lair port's AST (payload-free variants allocate, measured by
  `FIB_TRACE`).
- Proposal: represent a variant with no payload as a static or immediate value. Affects every enum-heavy program,
  including the compiler.
- Moves: binary-trees, and the natural form of any tree. Payoff *estimate*: binary-trees to about 1.5x Java in the natural
  program, with a similar win across the compiler.

### 4. Make the Map hot path and `nth` on small Vecs cheap (lib, own, emit)
- Evidence: k-nucleotide: `map-get` is 73% of self time and `mnode-assoc` 18%, about 52 ns per update against about 11 ns
  for `HashMap`. IO-4: `(assoc m k (+ 1 (get m k 0)))` as one expression is 3.4x slower than binding the count first
  (1480 ms against 440 ms for 1.25M updates), and `(update m k (fnil f 0))` is as slow as the nested form. Spectral-norm:
  `nth` on a `Vec` is a non-inlined trie walk at about 4.7 ns, and is 85% of the Vec version's time (7.28 s against 1.35 s
  for the `Array` version).
- Proposals: (a) apply the derived-borrow rule to the trie descent of `map-get` so it takes no counts; (b) find why
  reading and writing the same key in one expression costs 3.4x (a `get` result held across an `assoc` forces a copy?)
  and fix it, or make `update` with a default in-place when the map is unique; (c) an inlined fast path for `nth` on a
  `Vec` whose elements are all in the tail.
- Moves: k-nucleotide (4.7x), spectral-norm's Vec version, and everything that counts things in maps. Payoff *estimate*:
  k-nucleotide to about 2x Java.

### 5. Stop the runtime from trimming the heap on every large free (rt)
- Evidence: pidigits spends 1.5 s of 5.23 s in system time; `strace -c` shows 32,276 `brk` calls in a 5000-digit run,
  from freed 60 to 120 KB arrays. With three malloc tuning variables raised, the run went from 1.34 s to 1.00 s.
- Proposal: set `mallopt` (trim threshold, top pad, mmap threshold) at start-up, or give large arrays size-class free
  lists. H2 separately suggests a bump allocator for fresh small blocks, since `fib.alloc-slow` calls `malloc` for each.
  One-line first step.
- Moves: pidigits and every program that allocates and frees mid-size arrays. Payoff: about 25% on pidigits (measured with
  the environment variables).

### 6. Cheaper freeing of whole structures (rt)
- Evidence: binary-trees spends about 40% of its time freeing trees tree by tree (`drop.8` 24.6%, `fib.drop-one` 10.2%,
  `fib.release-slow` 4.9%), where Java's copying collector frees nothing. P4 proposed a leaf fast path in `fib.drop` earlier
  and nobody built it.
- Proposal: a leaf fast path in `fib.drop`; consider a region or arena for allocations with a shared lifetime.
- Moves: binary-trees, with item 3. Payoff *estimate*: about 1.3x on binary-trees.

### 7. Mutable unboxed arrays and the numeric builtins (lib, lang)
- `MArray`, `double-array`, `long-array`, `aget` and `aset` are specified in `spec/stdlib.md` and not implemented (hit by
  n-body, fannkuch-redux, spectral-norm, k-nucleotide and regex-redux; every program fell back to an `(Array T)` in a cell).
- BigInt needs a **widening multiply**, **wrapping `unchecked-*` arithmetic** and **`clz`**; without them the limb is 31
  bits where GMP uses 64, so four times as many limb products. Division is about half of pidigits' time. Payoff *estimate*:
  pidigits to about Java's speed.
- **`sqrt` as a builtin** (spec L11; n-body's library `math/sqrt` goes through libm and compiles to `vsqrtsd`, so the cost is
  small, but it is still an `unsafe` extern inside the library), **`str-byte-at` as an intrinsic** (a runtime call, 8% of
  regex-redux).

### 8. Integer index arithmetic costs against C (emit; needs investigation first)
- Evidence: after local fixes, fannkuch-redux's profile is 100% in `main` with no allocation, yet it is 3.1x C and spectral-norm
  is 2.65x C. The agents suspect overflow-checked `i64` index arithmetic, bounds checks and `i64` array elements against C's
  `int`, but **nobody read the generated IR to confirm**; `-O3` and replacing `quot` with `shr` each changed spectral-norm by
  under 10%.
- Proposal: first read the lIR and the machine code for fannkuch's inner loop and count the trap branches; then consider
  eliding overflow checks for loop counters bounded by an array length, or an `i32` index type.

### 9. Smaller language and library gaps (found, each with a reproduction in `gaps.md`)
- **Reader:** a symbol cannot contain `'` after its first character, so `+'`, `inc'` and the auto-promoting family cannot
  be written. Clojure allows it. Reader change, no memory-safety cost.
- **`:varargs` externs** reject extra arguments (`snprintf takes 3 argument(s), got 4`) where `spec/syntax.md` §3.15 says
  extras are allowed. Hit by 0A and n-body. A spec and code disagreement: the code should follow the spec.
- **Missing library:** `format` with `%f` (n-body added `fib.fmt/format-f64` over `strfromd`; it rounds the exact value as C
  does where Java rounds the shortest decimal half-up), a string builder (spec Q26), a growable byte buffer, array
  concatenation and block copy (`array-push!` is quadratic: 20,000 pushes 4 ms, 40,000 15 ms), read-all for a file
  descriptor, `str/replace`, the `#"..."` regex literal, and `*command-line-args*` (programs use `(args)`).
- **A top-level `def` with a computed initialiser** is unsupported (n-body writes `solar-mass` as a literal).
- **Integer `/` makes a Ratio**, so ported code needs `quot`; this is a documented difference, a porting note.

### 10. Tooling that every agent tripped on
- The sampling profiler `scripts/bench/tools/prof.sh` **segfaults on stage-2 binaries** and `perf` is blocked, so most
  profiles above are by disassembly, `strace`, or eliding code. A fixed copy exists in
  `~/.cache/fibber-scratch/PERFB-B/prof2.c`. Fixing it (or getting `perf_event_paranoid` lowered) is a prerequisite for the
  next round.
- The Rust `fibref` cannot read the current prelude (`unbound name array-push!`), so rule 6's interpreter half cannot check
  new features; known cost of the freeze.
- `/usr/bin/time` has 10 ms resolution; `run.sh` should run benchmarks long enough that this does not matter.
- GitHub Actions is blocked by the account's billing limit, so CI has been red since the lair work landed.

## Suggested order

1. Items 1 and 2 (own): the same family of rule as batch 4, with soundness cases that must be able to fail.
2. Items 5 and 3: one-line runtime change plus the nullary-variant change.
3. Item 8's investigation, then item 4, then item 6.
4. Item 7 once a widening multiply and `MArray` are designed.
5. The small items of 9 whenever convenient; the `:varargs` fix and the reader change first, since two agents hit them.
6. Re-run the whole suite on a quiet machine after each batch, with the profiler fixed first.

Targets, as *estimates* once these land: n-body about 2x Java, fannkuch about 1.5x, binary-trees about 1.5x, k-nucleotide about
2x, pidigits about 1.0x to 1.3x, spectral-norm about 1.0x; the programs already level with Java stay level.
