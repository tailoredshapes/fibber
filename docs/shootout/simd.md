# SIMD shootout: first end-to-end evidence (SIMD wave 4, package P5)

What the lane vectors of main (aaba407: `<<..>>`, `(Simd T n)`, `splat`, masks, `lane`/`with-lane`, `hsum`, `fib.simd`)
buy on real kernels, written with only what exists today (no change to `compiler/` or `lib/`), and what stops each kernel
from going faster. The ranked gap list is in section 4; it is the input for P4b (vector loads and stores, `fma`/`sqrt`,
wrapping, shuffles) and later work.

## 1. What was measured

Seven kernels, each a scalar fibber program, a SIMD fibber program (`<name>-simd.fib` beside it), Java and C:

| kernel | program | SIMD layout and lane use |
|---|---|---|
| n-body (50M steps) | `scripts/shootout/n-body/n-body-simd.fib` | one `f64x4` per body for position and one for velocity (x y z and a 0.0 lane); the pair update is vector arithmetic, no `Six` record; the squared distance is summed `(dx2 + dy2) + dz2` from the lanes, so the output is bit-identical to the scalar one |
| spectral-norm (5500) | `.../spectral-norm/spectral-norm-simd.fib` | lanes are four consecutive rows; every lane sums over j in the scalar order, so the digits are identical by construction (no cross-lane reduction) |
| mandelbrot (16000) | `.../mandelbrot/mandelbrot-simd.fib` | the eight pixels of one output byte are one `f64x8`; a sticky mask keeps escaped lanes dead; the loop ends when no lane is live |
| dot product | `.../dot-product/dot-product-simd.fib` | `(Array f64x4)`, four independent accumulators, one `hsum` at the end |
| saxpy | `.../saxpy/saxpy-simd.fib` | `(Array f64x4)`, `y = 0.5*x + y`, four vectors per trip |
| matmul 2048x2048 | `.../matmul/matmul-simd.fib` | i-k-j with a register-blocked micro-kernel: four `f64x4` accumulators (16 columns of C) over the whole k loop; B and C are `(Array f64x4)` |
| fannkuch-redux | not done | see section 5 |

The three new kernels have scalar fibber, Java and C programs too (`dot-product`, `saxpy`, `matmul` under
`scripts/shootout/`, each with a `sizes.txt` and so also picked up by `run.sh`). Their data are chosen so every product and sum is a
multiple of 0.5 below 2^53: the result is exact in any order, so the SIMD program's summation order changes nothing and one md5
serves all programs. For dot product and saxpy there are two shapes, `-mem` (one pass over 1e8 doubles, array fill included) and
`-cache` (arrays of 32768 doubles, 61035 repetitions, 2e9 element operations in cache).

Every row below passed the md5 of the scalar program's output (`FAIL` rows would be shown as such). The md5 are in the
`TABLE` of `scripts/shootout/simd-bench.sh`; the three original ones are those of their `sizes.txt`, the new ones were
taken from the C program and agree with all fibber and Java builds.

No reordering was needed anywhere: n-body, spectral-norm and mandelbrot print the same bytes as the scalar program (the md5 of the
`full` size in their `sizes.txt`) and n-body is bit-identical in every lane (checked at 500000 steps and 50M).

## 2. Method

```
# stage 2 of this tree, built by the v0.1.5 seed (scripts/fetch-seed.sh; the seed's fibc is in ~/.cache/fibber-scratch/seed-main)
ulimit -v 16000000
~/.cache/fibber-scratch/seed-main/fibc-0.1.5-linux-x86_64/bin/fibc build compiler/fibc.fib -I compiler -I lib \
    -L /usr/lib/llvm-21/lib -l LLVM-21 -o ~/.cache/fibber-scratch/p5/F
# the table
FIBC=~/.cache/fibber-scratch/p5/F scripts/shootout/simd-bench.sh -n 5 --out rows.tsv          # all kernels, three CPUs
FIBC=~/.cache/fibber-scratch/p5/F scripts/shootout/simd-bench.sh -n 5 --cpus host,v3,base matmul    # one kernel
```

`simd-bench.sh` builds each fibber program with `FIB_LIB=<tree>/lib fibc build X.fib -I lib -o BIN` (default `-O 2`) three times with
`FIB_TARGET_CPU` **unset** (the host), `x86-64-v3` and `x86-64`; C is `gcc -O3 -march=native` (with `-ffp-contract=off` where a
README asks for it); Java is `javac` and the default `java` flags (openjdk 27). Every timed run takes
`flock /tmp/fibsuite.lock`, runs under `ulimit -v 16000000`, and is the whole process (EPOCHREALTIME around it); the table is the
median of 5 (the minimum is in the TSV). Machine: Intel i7-14700KF (AVX2 and FMA, no AVX-512), 28 cores, Linux 7.0.0-34. The
machine is shared: other agents' jobs outside the lock were running (load average about 2.5 at the start), so differences of a few
percent are noise; the repeated n-body row (5.98 s and 6.03 s scalar, 1.75 s and 1.79 s SIMD) shows the spread.
Host and `x86-64-v3` give the same code (the host has AVX2, so its preferred width is 256 bits).

## 3. Results

Wall seconds, median of 5, all outputs verified. Ratios: `scalar/SIMD` is how many times faster the SIMD fibber program is than the
scalar fibber program at the same CPU setting; `SIMD/C` below 1 means SIMD fibber beats C.

| kernel | fibber scalar | fibber SIMD, host | SIMD, v3 | SIMD, x86-64 | scalar/SIMD (host) | Java | C | SIMD(host)/C |
|---|---|---|---|---|---|---|---|---|
| n-body 50M | 5.98 | 1.75 | 1.83 | 2.11 | 3.4 | 1.84 | 1.55 | 1.13 |
| spectral-norm 5500 | 1.27 | 0.52 | 0.52 | 0.66 | 2.4 | 1.13 | 0.55 | 0.94 |
| mandelbrot 16000 | 10.82 | 2.20 | 2.17 | 3.15 | 4.9 | 10.64 | 10.78 | 0.20 |
| dot-cache (2e9 elems, array 32768) | 0.83 | 0.26 | 0.27 | 0.29 | 3.2 | 0.86 | 0.77 | 0.34 |
| saxpy-cache (2e9 elems, array 32768) | 1.57 | 0.46 | 0.40 | 0.49 | 3.4 | 0.52 | 0.16 | 2.9 |
| matmul 2048 | 7.68 | 5.58 | 5.34 | 5.99 | 1.4 | 3.89 | 1.98 | 2.8 |
| dot-mem (1e8, one pass) | 0.71 | 0.69 | 0.67 | 0.70 | 1.0 | 0.72 | 0.57 | 1.2 |
| saxpy-mem (1e8, one pass) | 0.79 | 0.73 | 0.73 | 0.73 | 1.1 | 0.79 | 0.63 | 1.1 |

The scalar fibber programs at the other two CPU settings, for the baseline cost (the scalar programs hardly change, the SIMD
ones do):

| kernel | scalar host | scalar v3 | scalar x86-64 | SIMD host | SIMD v3 | SIMD x86-64 | cost of the baseline CPU, SIMD |
|---|---|---|---|---|---|---|---|
| n-body | 5.98 | 6.22 | 6.15 | 1.75 | 1.83 | 2.11 | +21% |
| spectral-norm | 1.27 | 1.28 | 1.35 | 0.52 | 0.52 | 0.66 | +27% |
| mandelbrot | 10.82 | 10.67 | 11.12 | 2.20 | 2.17 | 3.15 | +43% |
| dot-cache | 0.83 | 0.87 | 0.84 | 0.26 | 0.27 | 0.29 | +9% |
| saxpy-cache | 1.57 | 1.60 | 1.31 | 0.46 | 0.40 | 0.49 | +7% (noisy) |
| matmul | 7.68 | 7.57 | 6.85 | 5.58 | 5.34 | 5.99 | +7% |

Reading the numbers:

- **Packed code pays where there is arithmetic to pack.** mandelbrot 4.9x (the design document's estimate was 3x), n-body 3.4x
  (estimate 1.5 to 2x; the layout here is the three coordinates in lanes and the arithmetic is bit-identical, not the SoA of
  the design), spectral-norm 2.4x (estimate 2.5x), in-cache dot product and saxpy 3.2x and 3.4x. Mandelbrot is 4.9x faster than
  Java and than C, which are both scalar here (the Benchmarks Game rules accept any algorithm that prints the same bytes).
- The `-mem` rows do not move (1.0x and 1.1x): one pass over 1.6 GB is dominated by allocating and filling the arrays (page faults),
  not by the loop; they are included because "sum of 1e8 f64" was asked for, and they say the in-cache rows are the ones that
  measure the kernel.
- The in-cache dot product beats C and Java 3x only because their sum is a latency chain (no `-ffast-math`, and Java may not
  reassociate): the exact data make our reordering legal for this program, not for floating-point sums in general.
- matmul is the weak one: 1.4x over scalar, 2.8x behind C (gcc fuses, blocks and unrolls) and 1.4x behind Java.
- `x86-64` (SSE2 only, 128 bits) costs the SIMD programs 7 to 43%: the 256-bit vectors are split in two. The compute-bound
  mandelbrot loses most. Host and `x86-64-v3` are the same within noise. The scalar programs gain nothing from the host CPU.

## 4. What limits each kernel: ranked gaps

Found by reading the assembly of the built programs (`objdump -d -M intel`; everything is inlined into `main`, so the loops are in
`main`'s body) and the emitted lIR. The ranking is by how much time it plausibly costs across the kernels, with the
evidence per kernel.

1. **Bounds checks and checked index arithmetic in every vector loop, and no vector load/store with one range check per block.**
   matmul's inner `k` loop has, per trip, 4 vector loads, 4 `vmulpd` and 4 `vaddpd` but also about 30 scalar instructions: a bounds
   compare and branch for each of the four `b` elements and one for `a`, four `jo` after the checked index `add`s and an `imul`+`jo`
   for `k*nb`. That is the whole gap to C (5.6 s against 2.0 s) once the multiply-adds are accounted for. dot-cache's trip over 4
   vectors has 8 compare-and-branch pairs. spectral-norm is the exception, LLVM proves `j < n` is in range there and the loop has no
   checks (it is tight: `vdivpd` bound, 4 rows unrolled by 4), and the n-body pair update has 6 bounds checks per pair. P4b's
   vector loads and stores that check the range of a whole block once (or checks hoisted out of the loop) are the largest item.
2. **No vector load or store from an `(Array f64)`: the data layout has to change.** Every SIMD program above holds its arrays as
   `(Array f64x4)` from the start and fills them with a lane literal `<<(double (rem i 7)) ..>>`; an existing `(Array f64)`
   (what `fib.core` functions, `parse`, a `Vec` conversion produce) cannot be used by a SIMD loop without an element-by-element copy.
   spectral-norm had to keep its scalar vector `v` and read it with `splat` of one element per `j` (one `vbroadcastsd` per
   vector; harmless there, but a loop that needs `v[j..j+3]` as a vector cannot be written). Strided and masked tail loads
   (`load-masked`, design 2.13 spectral-norm) are not possible either.
3. **No `fma`.** The host has FMA3 (`vfmadd231pd`), no program above can use it: matmul, dot, saxpy, spectral-norm and n-body all
   emit `vmulpd` then `vaddpd`. For matmul the micro-kernel is two ops where gcc has one; for the latency-bound accumulators it is
   also one more cycle in the chain. (At `x86-64` there is no FMA instruction; the builtin must lower to mul and add there.)
4. **No vector `sqrt` (or any `fsqrt`) builtin besides the scalar `math/sqrt`, and `hsum`'s order is the halving tree.**
   n-body's pair needs `sqrt` of the squared distance; here the three lanes are extracted and summed in scalar order
   (`vshufpd`, `vextractf128`, `vaddsd` x2) to stay bit-identical, then `vsqrtsd`, `vdivsd`, and a `vbroadcastsd` back to a
   vector. The SoA formulation of the design (ten pairs in three vectors, `vsqrtpd`, `vdivpd`) is not expressible, and that
   formulation is where n-body would get close to 4x. A left-to-right (ordered) `hsum` variant, or `lane` shuffles, would also
   avoid the extract.
5. **Masks: no mask-to-bits and a poor register form.** mandelbrot's mask is an `<8 x i1>`; per iteration LLVM turns the two `ymm`
   compare results (`vcmplepd`) into one i1 vector with `vpackssdw`, `vextracti128`, `vpackssdw`, `vpshufd`, then `vpand` with the
   live mask, then `vpsllw`, `vpacksswb`, `vpmovmskb` and a test for `simd/any`: 9 instructions beside about 20 floating-point
   ones of the iteration. With the mask kept in 64-bit-lane form, `any` would be one `vtestpd`/`vmovmskpd`. The output byte
   itself is assembled with `(lane m k)` eight times, which LLVM turns into one `vpmovmskb` plus 8 and/shift pairs, once per byte, fine; a
   `mask-bits` builtin would make it explicit and cheap on all targets. mandelbrot also tests `any` every iteration; the
   scalar loop's early exit has no cost, the vector one does.
6. **The unique-write protocol around every `array-set!` of a vector.** n-body's `advance` does, before each of its two
   vector stores per pair, `test`/`cmp` of the array's flags and count, with the `fib.array-slice` copy path out of line and
   all live `ymm` registers (4 of them) spilled to the stack on that path. Not a cost per se in the hot path (predicted branches),
   but it forces the spills' code and register pressure. A borrowed-unique `&a` parameter that is proved unique once per call
   would remove it; this is not SIMD-specific.
7. **Boxed records.** The scalar n-body builds a `Six` per pair (one of the differences to the SIMD one, which
   returns nothing and writes both velocities in place; not isolated by an experiment). The SIMD program avoids it by structure; any kernel that returns
   several vectors from a helper would build a record (a vector in a record field is an `[k x i64]` object field, spec 1.9). The
   `Pair f64x4 f64x4` return of an early draft of the matmul micro-kernel was dropped for this reason (it would allocate).
8. **`:native` lane counts could not be used.** There is no lane-index vector (`iota`) and no `native-lanes`, so every program fixes
   `f64x4` or `f64x8`. `f64x8` on AVX2 is two `ymm` (mandelbrot gets its ILP that way) and on `x86-64` four `xmm`; a
   `:native` program would be 4 lanes there and lose the interleaving. This is about the programs' form, not a measured loss.
9. **Dynamic `lane`/`with-lane` and `hsum` per row.** Not exercised in a hot loop; spectral-norm avoids them by construction.
10. **`-mem` kernels are bound by array creation** (`(array n 0.0)` zero-fill, then the fill loop), not by the vector loop; worth a
    look as an allocation benchmark of its own (`calloc`-backed zero arrays, no second pass), unrelated to SIMD.

Also noticed, not SIMD: `fib.tracing` is loaded and tested inside the n-body outer loop (a byte load and branch, with
`snprintf`/`write` on the taken path); it costs a predicted branch.

## 5. What was not done

- **fannkuch-redux:** not attempted. Its inner loop reverses a prefix of an array of up to 12 small integers and counts flips;
  the natural SIMD form is a variable permutation (`pshufb`/`vpermd`) of an `i8x16`, which needs shuffles (not in main; P4b).
  Without them the program would be scalar with extra lane traffic.
- **`:native` lane counts and `f64x8` on a 512-bit CPU:** the host has no AVX-512, so neither was measured.
- **Clojure:** not run for the three new kernels (no `.clj` programs written); the three existing ones have Clojure programs in
  `run.sh`'s table, not repeated here.
- **Sizes below `full`:** only the `full` size of the three original kernels was timed (small sizes pass the same md5 check:
  n-body 500000 and mandelbrot 2000 and spectral-norm 1500 were verified while writing the programs).
- **A quiet machine:** the runs were taken under `/tmp/fibsuite.lock` but not with the machine otherwise idle (see section 2).
- **User time and RSS** were not recorded (wall time only).
