# Gaps the shootout programs hit

One section per gap: what is missing, the smallest reproduction with the exact error, what the Java/C version uses, the
recommendation. (Appended by each benchmark's agent; this part is BIG, the BigInt library and pidigits.)

## A symbol cannot end in `'`: `+'`, `inc'` and the rest of Clojure's auto-promoting names cannot be written

Missing: the names `+'` `-'` `*'` `inc'` `dec'` of spec/stdlib.md §4.9 (tranche 5, the BigInt rows). The reader ends a symbol at
`'` (spec/syntax.md §1.1: a symbol is a maximal run of characters that are not whitespace, `( ) [ ] { } " ; ' ` , @ ~ \`), so
`+'` reads as the symbol `+` and then a quote.

Reproduction (`q1.fib`):

```clojure
(ns main)
(defun inc' (a: i64) -> i64 (+ a 1))
(defun main () -> i64 (inc' 4))
```

```
rejected:
q1.fib:2:13: (a: i64) is not a parameter
```

Clojure and Java: Clojure's `+'` is an ordinary symbol (the reader's quote is a macro character only at the start of a
token); Java has no counterpart (`BigInteger.add`). The Clojure pidigits entry in this suite uses interop and does not need them.

Recommendation: a language change in the reader: a `'` inside a symbol (not at its start) is a constituent, as Clojure's.
The Rust lexer, `compiler/syntax/lexer.fib` and spec/syntax.md §1.1 change together (mirror-pending). Until then the library
offers `bigint` and `(+ (bigint a) (bigint b))`, and the rows keep tranche 5. Auto-promotion (a `Long` when it fits) stays a
deviation either way (§5 T5).

## No widening multiply, no wrapping arithmetic, no `clz`: the BigInt limb is 31 bits in an `i64`

Missing: an unsigned 64 by 64 to 128 bit multiply (`mulhi`, as Rust's `u64::widening_mul` or C's `__int128`), the wrapping
`unchecked-add` family of spec §4.3 (tranche 3, "needs a new builtin L10"), and a count-leading-zeros primitive.

Effect: arithmetic traps on overflow, so a limb product must fit a signed `i64`. `lib/fib/bigint` therefore uses 31-bit limbs
((2^31-1)^2 + 2(2^31-1) < 2^62): 3% more limbs than 32-bit ones (`BigInteger` has 32-bit limbs in `int`s with `long` products, so
the two are alike here) and four times as many limb products as GMP's 64-bit limbs. This is the main reason the library cannot
approach the C column: its limb loops cost 1 to 6 ns per limb where GMP's cost a fraction of one. The bit length of a limb
is `popcount` of the limb with its top bit smeared down (5 shifts), where Java uses `Integer.numberOfLeadingZeros`.

Reproduction: `(* 4294967295 4294967295)` is `trap: integer overflow in * at i64` for a 32-bit limb product with a carry added.

Recommendation: library or builtin: `(mul-hi a b)` and `(mul-lo a b)` (or a `i128` type), `clz`, `ctz`, and the `unchecked-*` family;
then the limb can be 64 bits and the inner loops are those of GMP's `mpn_mul_1`/`mpn_addmul_1`.

## An owned `(Array i64)` parameter cannot be written in place by the callee

Missing: a way for a library function to take an array it may overwrite when the caller passes the only reference
(spec/types §2.5: the count of a callee's owned parameter is two, so `array-set!` on a parameter copies, as the comment on the
prelude's `vnodes-up` says). `lib/fib/bigint` builds each result in a `(cell (array n 0))` it owns (`&c` in/out) and trims a high
zero limb with `array-pop!`; the operands are never reused, so `numer = numer * 10` in the spigot allocates a fresh array each time.
Java's `BigInteger` is immutable too (and allocates as well); GMP's `mpz_mul_ui(numer, numer, 10)` is in place.

Recommendation: language change or library idiom: a consuming `(mul-small! &a m)` over an `&a: (Array i64)` in/out parameter works
today for the library's own loops (the in/out parameter is the callee's), but a program that holds a `BigInt` struct cannot call
it without a mutable field. A `BigInt` that is a record of one `(Cell (Array i64))` would, at the price of a mutable type in the API.
Not done: it breaks the value semantics Clojure programs expect.

## The runtime's allocator trims the heap on every free of a large array

Found by `strace -c ./pidigits-fib 5000`: 32276 `brk` calls (98.96% of the system calls) and 1.7 s of system time at N=10000
(user 4.8 s), because each BigInt operation allocates and frees an array of 60 to 120 KB, and glibc's `free` of the top chunk
returns the pages to the kernel (`M_TRIM_THRESHOLD`), which `malloc` then faults in again. With
`MALLOC_TRIM_THRESHOLD_=4000000000 MALLOC_TOP_PAD_=268435456 MALLOC_MMAP_THRESHOLD_=4000000000` the same run (N=5000) goes from
1.34 s elapsed (0.18 s system) to 1.00 s (0.00 s system).

Recommendation: runtime change (`fib.rt`, `rt/*.lir`): call `mallopt(M_TRIM_THRESHOLD, ...)` and `mallopt(M_MMAP_THRESHOLD, ...)` at start
(Java's and GMP's allocators do not hand pages back on each free), or a size-class free list for arrays. Every benchmark that
allocates and frees mid-size arrays in a loop pays this.
