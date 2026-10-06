# End-to-end training: fibber (`fib.autodiff`) against NumPy, PyTorch and JAX

Design and prototype: `docs/design/autodiff.md`. Every number below was produced in the session of 2026-10-06 with the commands quoted; the raw outputs are
kept in `~/.cache/fibber-scratch/ad/` (`bench2.txt`, `micro.txt`, `deepmem.txt`, `run/curve-*.txt`). **Nothing here shows fibber faster than the other
three on this model.** The SGD step is within 3% of NumPy's and PyTorch's; the Adam step is 1.1 to 1.6 times slower than NumPy, PyTorch and JAX; the memory
a step needs above the parameters is smaller than PyTorch's in the one deep model measured. Section 4 says what limits each row.

## 1. What was run

**The model and the data** (no downloads; `scripts/bench/autodiff/mlp.fib` and `mlp.py` compute the same numbers): an MLP classifier 784-256-10, relu hidden layer,
softmax cross-entropy, `f32`, batch 128, 300 steps over 4096 generated samples (batch `s mod 32` at step `s`). A sample is `0.5 (u - 0.5) + 0.8 s(c, j)` with `u`
an integer hash of (sample, feature), `c` the class (sample mod 10) and `s(c, j)` a class pattern; weights are `He`-uniform from the same hash, biases zero. The
hash is `((7919 n + 104729 j + 1299709 salt + 12345) * 2654435761) mod 2^32` in 64-bit integers, then `((h >> 7) mod 10^6) / 10^6`, computed in `f64` and rounded to `f32`
on both sides, so the initial loss is identical to eight digits in all five programs (2.3855264). SGD: learning rate 0.1. Adam: 0.001, 0.9, 0.999, 1e-8.

| Row | What it is |
|---|---|
| fibber tape | `ad/value-and-grad` over `ad/dense`, `ad/dense`, `ad/softmax-cross-entropy`; `ad/sgd-step` or `ad/adam-step` |
| fibber hand-written backward | the same step written out with `fib.tensor` calls (no tape, no closures), same kernels; the difference to the row above is the tape |
| NumPy hand-written backward | NumPy 2.5.3 from the pip wheel (its bundled OpenBLAS), hand-written gradients, in-place SGD, a hand-written Adam |
| PyTorch autograd | torch 2.14.1+cpu, `torch.optim.SGD` / `Adam`, `F.cross_entropy`, `loss.backward()`, `set_num_threads(1)` |
| JAX `jit(value_and_grad)` | jax 0.11.2 on CPU, the whole step jitted (compiled once before timing), SGD and Adam written as pytree arithmetic inside the jit |

**Environment.** i7-14700KF (AVX2 and FMA, no AVX-512), Linux 7.0, one thread: every process is pinned to CPU 3 (`sched_setaffinity`) with `OMP_NUM_THREADS`,
`OPENBLAS_NUM_THREADS`, `MKL_NUM_THREADS`, `BLIS_NUM_THREADS` = 1; fibber under `RLIMIT_AS` = 16 000 000 KiB. The driver holds `/tmp/fibsuite.lock` for the whole run
(the machine was shared: other agents' gates ran before and after it, not during). **The Python packages came from a venv under `~/.cache/fibber-scratch/ad/venv`
made with `python3 -m venv` and `pip install numpy torch jax jaxlib --extra-index-url https://download.pytorch.org/whl/cpu`** (network pip worked; nothing was
installed system-wide). Neither PyTorch nor JAX was present on the machine before. The system NumPy is linked to the reference BLAS (`docs/shootout/tensor.md` section 1);
the venv's wheel carries OpenBLAS, which is what a NumPy user gets, so it is the one measured.

```sh
# the commands (from the worktree root); F is a stage 2 fibc built from this tree
F build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F
python3 scripts/bench/autodiff/run.py --fibc ./F --python ~/.cache/fibber-scratch/ad/venv/bin/python --cpu 3       # the table of section 2 (about 4 minutes)
flock /tmp/fibsuite.lock python3 scripts/bench/autodiff/deepmem.py --fibc ./F --python ~/.cache/fibber-scratch/ad/venv/bin/python --cpu 3   # section 5
F build scripts/bench/autodiff/kernels.fib -I lib -o kern && flock /tmp/fibsuite.lock taskset -c 3 ./kern                                     # section 3
F build scripts/bench/autodiff/tape-overhead.fib -I lib -o to && flock /tmp/fibsuite.lock taskset -c 3 ./to 1000                              # section 6
```

`run.py` runs each configuration 5 times (one process per run), takes the median step time of each run over steps 11 to 300 (10 warm-up steps are excluded from the
statistic, not from the training), and reports the median over the 5 runs.

## 2. Results

Single thread, milliseconds per training step (forward, loss, backward, update), median of 5 runs, with the spread of the 5 medians.

| optimiser | implementation | ms/step (median of 5) | min..max over runs | peak RSS MB | RSS MB at 0 steps | max rel. loss diff vs NumPy | allocations/step |
|---|---|---|---|---|---|---|---|
| sgd | fibber tape (fib.autodiff) | 0.932 | 0.930..0.934 | 21 | 18 | 2.7e-06 | 2145 |
| sgd | fibber hand-written backward | 0.916 | 0.913..0.922 | 21 | 18 | 2.7e-06 | 1255 |
| sgd | NumPy hand-written backward | 0.897 | 0.896..0.900 | 155 | 155 | 0 (reference) | not measured |
| sgd | PyTorch autograd | 0.927 | 0.922..0.931 | 351 | 342 | 4.2e-06 | not measured |
| sgd | JAX jit(value_and_grad) | 0.893 | 0.885..0.906 | 308 | 304 | 2.4e-06 | not measured |
| adam | fibber tape (fib.autodiff) | 1.649 | 1.635..1.659 | 23 | 18 | 6.0e-05 | 3314 |
| adam | fibber hand-written backward | 1.622 | 1.602..1.630 | 23 | 18 | 5.7e-05 | 2424 |
| adam | NumPy hand-written backward | 1.537 | 1.531..1.557 | 155 | 155 | 0 (reference) | not measured |
| adam | PyTorch autograd | 1.155 | 1.152..1.167 | 356 | 342 | 9.9e-05 | not measured |
| adam | JAX jit(value_and_grad) | 1.030 | 1.028..1.042 | 336 | 331 | 5.9e-05 | not measured |

Reading it, without more than the numbers support:

- **SGD:** fibber tape 0.932 ms, NumPy 0.897 (fibber 3.9% slower), PyTorch 0.927 (0.5% slower), JAX 0.893 (4.4% slower). The tape costs 0.016 ms (1.7%) over the same step written by
  hand. The run-to-run spread of each row is under 1%, so these differences are real on this machine, but they are small and for one model shape; this is "close", not
  "parity" and not "faster".
- **Adam:** fibber tape 1.649 ms against NumPy 1.537 (7% slower), PyTorch 1.155 (43% slower), JAX 1.030 (60% slower). The 203 000-parameter update is where fibber loses (section 4).
- **Loss curves.** The loss at every one of the 300 steps was compared with NumPy's: the largest relative difference is 2.7e-6 (SGD) and 6e-5 (Adam) for fibber, 4.2e-6 and
  9.9e-5 for PyTorch, 2.4e-6 and 5.9e-5 for JAX. **Tolerance used: 1e-5 for SGD, 1e-3 for Adam** (f32 rounding differs between libraries, and Adam's division by the square root
  of a small second moment amplifies it); all rows pass. The curves agree to the printed digits at steps 1, 100, 200, 300: SGD 2.3855264, 0.0235357, 0.0087805, 0.0052979;
  Adam 2.3855264, 0.00077420, 0.00041065, 0.00025494.
- **Peak RSS** includes the runtime: Python with NumPy 155 MB, with PyTorch 342 MB, with JAX 304 MB before the first step; fibber's whole process is 18 MB, of which 12.8 MB is the
  dataset. The SGD step itself adds 3 MB (fibber), 0 (NumPy), 9 MB (PyTorch), 4 MB (JAX) over the 0-step process. These are one-process high-water marks read from `wait4`, in
  1-MB steps of the allocator; they say that a fibber training process is an order of magnitude smaller than a Python one, not that its step needs less memory (section 5 measures that).
- **Allocations per step** (`FIB_TRACE=1`, lines `A` of 3 steps minus 1 step, halved): 2145 (SGD, tape), 1255 (SGD, hand-written), 3314 and 2424 (Adam). Python's allocations were not measured.
  Most are not the tape's: each `fib.tensor` call allocates 20 to 90 small objects (descriptors, shape vectors, packing buffers: `mmul` 71, `dense` 87, `softmax` 20, `sum-axis` 34,
  the relu mask 74, `ad/softmax-cross-entropy` forward and backward 612). The tape adds 890 per SGD step (a closure, an entry and a link per op, plus the gradient vectors). If an allocation
  costs about 20 ns (an assumption, not measured here) the whole count is about 40 us of a 932 us step.

## 3. Where the time goes (one SGD step, the kernels)

`scripts/bench/autodiff/kernels.fib` and `kernels.py`, best of 200 (100 for the Adam rows), milliseconds, one thread, under the lock (`micro.txt`):

| piece (batch 128, 784-256-10) | fibber | NumPy + OpenBLAS |
|---|---|---|
| `x @ w1` 128x784x256, f32 | 0.357 | 0.368 |
| `x^T @ dz1` 784x128x256 (the weight gradient) | 0.339 | 0.328 |
| `dz1 @ w1^T` 128x256x784 (not run by the tape: the input is a constant) | 0.381 | 0.358 |
| second layer forward, 128x256x10 | 0.015 | 0.011 |
| second layer weight gradient, 256x128x10 | 0.015 | 0.008 |
| relu mask of 128x256 | 0.135 with `where`/`greater`; **0.020** with `relu(h/h)` (used) | 0.010 |
| bias gradient, 128x256 summed over rows | 0.040 with `sum-axis 0`; **0.0066** as a product with a row of ones (used) | 0.003 |
| SGD update of the 4 tensors | 0.031 | 0.054 (with temporaries) |
| Adam update of the 4 tensors (+ two zero-tensor allocations for the state) | 1.48 with scalar `sqrt` (1.90 with the first version, `exp(log v / 2)`) | 0.64 by difference of the two NumPy rows of section 2 (not measured alone) |
| `log` then `exp` of one 784x256 tensor, f32 | 0.938 (2.3 ns an element each) | not measured |
| elementwise `mul` of two 784x256 tensors | 0.016 | not measured |
| tape forward (leaves, 2 dense, cross-entropy) | 0.406 | |
| the same forward with `fib.tensor` calls, no tape | 0.375 | |
| tape forward + backward | 0.866 | |
| one JVP (forward mode, one direction) | 1.170 | |

What the table says. **The three matrix products are as fast as OpenBLAS's** on this CPU and size (0.357 / 0.339 against 0.368 / 0.328 ms): the fibber GEMM is not what separates the rows, which is the
result of `docs/shootout/tensor.md` repeated inside a training step. The forward costs 0.375 ms without a tape and 0.406 with it (the recording is 0.03 ms). Forward plus backward through the tape is
0.866 ms, two products (0.357 + 0.339 = 0.70 ms) and about 0.17 ms of glue and bookkeeping. The same step in NumPy takes 0.897 ms with the update, so the remaining fibber-NumPy difference is
elementwise glue in the tens of microseconds. The two workarounds recovered about 0.2 ms: the first complete step (an unlocked run before them) took 1.16 ms, with them 0.93 ms under the lock.
A JVP of the network in one direction is 3.1 times the forward pass (1.17 against 0.375 ms) and 1.35 times a full gradient: the right trade only for a Jacobian with few inputs.

## 4. What limits each row

- **fibber SGD (0.932 ms).** 0.70 ms is the two big products, 0.03 ms the update; the rest is the dense epilogues, the softmax cross-entropy on 128x10 (612 allocations), the relu mask (0.020 ms) and the tape (0.016 ms).
  The profile is flat: there is no single remaining target worth more than 3%.
- **fibber Adam (1.649 ms, 0.72 ms more than SGD).** The update touches 203 530 floats ten times, and each pass allocates a fresh tensor. The first version's vector `exp(log v / 2)` took 0.94 ms for
  one tensor (the vector `log` and `exp` of `fib.tensor` cost 2.3 ns an element: they compute in `f64` and round); a scalar `map` with `math/sqrt` brought the step from 1.96 to 1.65 ms. The remaining gap to PyTorch (1.155) and JAX
  (1.030) is the elementwise passes (inferred: PyTorch's optimiser is C++ and JAX's is compiled by XLA, which fuses elementwise code): fibber does nine separate passes over each tensor. Fix: a vector `sqrt` (`simd/sqrt`) in
  `fib.tensor`, an `adam-update` kernel that does the whole update in one pass, and in-place writes through windows (docs/design/autodiff.md 9). Not measured: which of the nine passes costs most.
- **NumPy (0.897 / 1.537 ms).** Three OpenBLAS products and fast elementwise glue with in-place updates (`w1 -= lr * dw1` allocates one temporary); Adam as ten whole-array expressions with temporaries.
- **PyTorch (0.927 / 1.155 ms).** BLAS-class products, a C++ autograd engine and an optimiser implemented in C++; the step has about 15 ops, so a per-op overhead of a few microseconds (not measured) would be under 10% here. The explanation is inferred, not profiled.
- **JAX (0.893 / 1.030 ms).** Adam costs 0.14 ms more than SGD, which suggests XLA fuses the optimiser's elementwise work (inferred, not profiled). Dispatch of one jitted call is included in the timing.

## 5. Memory high-water mark of a step: a deep model

`scripts/bench/autodiff/deep.fib` and `deepmem.py`: 12 dense layers of width 1024 (relu), batch 256, `f32`, loss `mean(h*h)`, one forward and backward. The parameters are 12 x 4 MiB = 50 MB, the data 1 MiB, each saved
activation 1 MiB, the gradients of all 12 weights 48 MB. Peak RSS of the process with 0 steps and with 1 step (median of 3 runs each); the difference is what the step needs above the parameters and the data:

| implementation | peak RSS MB, 0 steps | peak RSS MB, 1 step | the step needs (MB) |
|---|---|---|---|
| fibber tape | 51 | 104 | **53** |
| PyTorch autograd | 307 | 373 | 66 |

The step needs the 48 MB of gradients (all 12 are alive at the end) plus about 5 MB: the saved activations are released as the backward reaches them, so they are not additive with the late gradients. PyTorch's
step needs 18 MB more than the gradients: its saved tensors are also freed as the backward proceeds, so the difference is allocator and graph-node overhead, not a different algorithm. **One model, one machine,
RSS granularity of a page-rounded allocator: this supports "comparable, a little smaller", not a ratio.** The loss agrees to six digits (1.6355649e-10 against 1.6355881e-10; the loss is tiny because the
weights are small, which does not change the memory). A transformer block was not built, so its memory was not measured (autodiff.md 5.8 has the arithmetic).

## 6. The cost of the tape per op (the case against Python's overhead)

`scripts/bench/autodiff/tape-overhead.fib 1000`: a chain of 1000 multiplies of a one-element tensor by a constant, so the kernels cost almost nothing; best of 20, microseconds per op, one thread:

| | us per op |
|---|---|
| the chain with `fib.tensor` calls, no tape | 0.277 |
| the same with tape recording (forward only) | 0.351 |
| forward and backward | 0.957 |

Recording costs 0.07 us an op (a closure, an entry, a chain link, an id) and walking back 0.6 us an op (the closure call, the gradient `mul`, the accumulation). PyTorch eager spends several microseconds per op
in Python and ATen dispatch, so for a model made of many small ops the comparison would favour fibber; **that comparison was not run** (the benchmark above has about 15 ops per step), and no per-op PyTorch figure is
claimed here.

## 7. Not measured, and caveats

- One model (784-256-10) and one batch size; a larger model moves the balance toward the products, a smaller one toward per-op overhead and allocations. No convolution, attention or layernorm model: they do not exist on the tape yet.
- Single thread only. fibber's data-parallel gradient (`fib.autodiff.parallel`) is tested for determinism and equality, not benchmarked; PyTorch and JAX multi-threaded were not run.
- NumPy's row is hand-written gradients, not an autodiff system; it is the lower bound of Python-side overhead, and the right comparison for "what hand-tuned code costs".
- JAX's compile time is excluded (one warm-up call before timing); PyTorch's first-iteration costs are in the 10 warm-up steps, which the statistic skips.
- The machine is shared; the lock excludes the other agents' gates and benchmarks but not unrelated processes (a game-streaming server and a web service were running on the machine; each configuration ran 5 times and the spread is under 1%, which suggests low noise on the pinned CPU).
- The `relu(h/h)` mask has a known limit (`h = +inf` gives 0), and the bias-gradient trick changes the f32 summation order from `sum-axis`'s to an fma chain (both fixed orders).
- Allocations per step were not measured for Python; RSS was read once per process with `wait4`.


## 8. 2026-10-06, second run: the tensor-library gaps of sections 3 and 4 closed

Sections 1 to 7 above are the first run and are kept as they were. This section is the same benchmark after the package of `lib/fib/tensor` (`docs/shootout/tensor.md` section 10): vector `sqrt`, `relu-grad`/`relu-mask` over lane compares,
lane kernels for `sum-axis`/`maximum-axis`/`minimum-axis`, and a fused `adam-step`; `fib.autodiff` now calls `t/adam-step`, `t/relu-grad` and `t/sum-axis 0` (its three workarounds are gone: the scalar-map `sqrt`, `relu(y/y)` and the
bias gradient as a product with a row of ones). The Adam results are bit-identical to the first run's (same operations in the same order); the relu gradient differs only for a NaN or infinite incoming gradient under a zero mask (now 0, as
PyTorch's `threshold_backward`), and the bias gradient is now the ordered column sum (`t/sum-axis 0`) instead of a fused-multiply-add chain, so its last bits differ. The machine was **loaded** (load average 5 to 18 from other agents' gates; the lock excludes
benchmarks, not unrelated processes), so the spreads below are wider than in section 2 and a difference under about 10% is not established.

```sh
F build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o F      # F from this tree, built with the v0.1.7 seed
FIB_LIB=$PWD/lib python3 scripts/bench/autodiff/run.py --fibc F --python ~/.cache/fibber-scratch/ad/venv/bin/python --cpu 3 --out ~/.cache/fibber-scratch/t4/run     # the table (same venv: no installs)
FIB_LIB=$PWD/lib F build scripts/bench/autodiff/kernels.fib -I lib -o kern && flock /tmp/fibsuite.lock taskset -c 3 ./kern                                    # the kernel rows
```

### 8.1 Results (median of 5 runs, ms per step, one thread)

| optimiser | implementation | ms/step | min..max over runs | max rel. loss diff vs NumPy | allocations/step |
|---|---|---|---|---|---|
| sgd | fibber tape (fib.autodiff) | 0.972 | 0.929..0.980 | 2.7e-06 | 1983 |
| sgd | fibber hand-written backward | 0.922 | 0.901..0.975 | 2.7e-06 | 1071 |
| sgd | NumPy hand-written backward | 0.955 | 0.905..1.046 | 0 (reference) | |
| sgd | PyTorch autograd | 0.985 | 0.921..1.023 | 4.2e-06 | |
| sgd | JAX jit(value_and_grad) | 0.953 | 0.911..0.996 | 2.4e-06 | |
| adam | fibber tape (fib.autodiff) | **1.220** | 1.210..1.301 | 6.0e-05 | 2152 |
| adam | fibber hand-written backward | 1.199 | 1.194..1.283 | 5.7e-05 | 1240 |
| adam | NumPy hand-written backward | 1.631 | 1.545..1.794 | 0 (reference) | |
| adam | PyTorch autograd | 1.262 | 1.170..1.283 | 9.9e-05 | |
| adam | JAX jit(value_and_grad) | 1.125 | 1.023..1.251 | 5.9e-05 | |

The loss curves still match NumPy's within the tolerances of section 2 (SGD 1e-5, Adam 1e-3): the largest relative differences are 2.7e-6 and 6.0e-5, the same as in the first run (the Adam update is bit-identical to the old one). Peak RSS: fibber 20 MB.
Allocations per step fell from 3314 to 2152 (Adam, tape).

**Does Adam match PyTorch and JAX now?** The fibber Adam step went from 1.649 to 1.220 ms (-26%). In this run it is **3% faster than PyTorch (1.262) and 8% slower than JAX (1.125)**; the run-to-run ranges of the three overlap (fibber 1.210..1.301, PyTorch 1.170..1.283,
JAX 1.023..1.251), so the honest reading is "level with PyTorch, behind JAX by a margin the noise of this run does not settle". In the first run (quiet machine) PyTorch took 1.155 and JAX 1.030; those two did not get faster, the machine got noisier. A rerun on a quiet machine is needed to claim more.
The Adam step costs 0.25 ms more than the SGD step in fibber (0.28 for PyTorch, 0.17 for JAX in the same run); it was 0.72 ms.

### 8.2 The kernel rows (best of 100 to 200, ms, `kern`)

| piece | first run | now |
|---|---|---|
| relu mask of 128x256 (`where`/`greater` -> `relu-grad` is one pass: 0.0079 in `gaps.fib`) | 0.135 (`where`), 0.020 (`relu(h/h)`) | 0.039 (`where`, still measured in `kern`), 0.025 (`relu(h/h)`); `t/relu-grad` 0.008 |
| bias gradient 128x256: `sum-axis 0` | 0.040 | 0.0035 (product with ones: 0.0084) |
| Adam update of the 4 tensors, `ad/adam-step` including the two zero-state allocations | 1.48 | 0.806 |
| `log` then `exp` of 784x256 | 0.938 | 1.053 (not changed; the vector `sqrt` replaces it) |
| three matrix products (0.357, 0.339, 0.381 first run) | | 0.427, 0.382, 0.439 (the machine is slower now: +12%) |

(`kern` runs `ad/adam-step` once per call from a fresh zero state; the end-to-end step reuses chained state.) In `gaps.fib` the update of the four tensors alone is 0.157 ms (`t/adam-step`), against 1.121 ms for the nine-pass composition with a vector `sqrt` and 1.666 ms with the scalar-map `sqrt`.

### 8.3 Where the remaining time goes (profile)

One Adam step is 1.22 ms. By section 3's pieces: the two big products, 0.43 + 0.38 ms in the loaded `kern` run (0.70 ms in the quiet first run), are about 65% of it, and they are at OpenBLAS parity (`docs/shootout/tensor.md`); the Adam update is the next piece, then the glue.
**A large part of the update is not arithmetic.** `adam-step` on the four MLP tensors takes 0.157 ms in a loop that reuses its inputs, but a probe that chains the state as training does (`scripts/bench/autodiff/adam-chain.fib`: each step's outputs are the next step's inputs) took **0.367 ms**, and with glibc's large-block cache enlarged

```sh
MALLOC_MMAP_THRESHOLD_=67108864 MALLOC_TRIM_THRESHOLD_=268435456 MALLOC_TOP_PAD_=67108864 ./probe     # 0.176 ms; without the variables 0.367 ms
MALLOC_MMAP_THRESHOLD_=67108864 MALLOC_TRIM_THRESHOLD_=268435456 MALLOC_TOP_PAD_=67108864 taskset -c 3 ./mlp tape adam 300     # step_ms 1.001; without: 1.200 (SGD: 0.909 against 0.901)
```

it took 0.176 ms; the whole Adam step went from 1.200 to 1.001 ms (17%) and the SGD step did not change (the SGD update allocates no large block). The explanation consistent with this is that each 800 KB tensor is above glibc's mmap threshold, so every fresh result is an `mmap`, a run of page faults and an `munmap`.
That is the runtime's allocator, not `fib.tensor`: the lever is outside this package (a size-class cache or a raised threshold in the runtime would help every program that allocates large tensors, the GEMM outputs included). Not done here; the experiment above is the evidence (one machine, glibc malloc, `perf` was not permitted, so this is inferred from the experiment, not profiled).
With the allocator effect removed, fibber's Adam step would be about 1.00 ms against JAX's 1.12 and PyTorch's 1.26 measured in the same run (different moments, loaded machine: indicative only).
The rest is small: the softmax cross-entropy on 128x10 (about 600 allocations), 2152 allocations per step in all, the tape (0.016 ms).
