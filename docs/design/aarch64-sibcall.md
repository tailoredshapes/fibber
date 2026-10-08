# AArch64: a sibling call out of a `tailcc` function corrupts the stack pointer (LLVM-REPRO)

**Verdict: (A) an LLVM backend bug.** fibber emitted an unmarked, valid call. LLVM's own `tailcallelim` pass marks it `tail`, and the AArch64
backend then compiles it to a sibling branch without popping the caller's stack arguments. The same fault reproduces with no fibber in the
program: in plain C through clang's `swiftasynccall`, and in a 12-line `.ll` fed to `llc`. It is present in every LLVM from 17.0.6 to 23.1.2
that was tried, and in the source of `main`. The fibber mitigation (`notail` on a call between a `tailcc` function and a `ccc` one on AArch64,
`compiler/native/lower/calls.fib`, tested by `compiler/tests/native/a64-sibcall.sh`) stays: it is the right workaround, not a cover for our fault.

Run it: `make repro-aarch64-sibcall LLC=/usr/lib/llvm-21/bin/llc` (llc alone, no AArch64 hardware). Files: `docs/repro/aarch64-sibcall/`.

## The minimal reproducer

`docs/repro/aarch64-sibcall/min.ll`: a `tailcc` function of ten `i64` arguments (the 9th and 10th are passed on the stack, 16 bytes) whose last
action is a call of a `ccc` function.

```llvm
target triple = "aarch64-unknown-linux-gnu"
@sink = global i64 0
declare void @callee(i32)
define tailcc void @f(i64 %a, i64 %b, i64 %c, i64 %d, i64 %e, i64 %f, i64 %g, i64 %h, i64 %i, i64 %j) {
  %s = add i64 %a, %j
  store volatile i64 %s, ptr @sink
  call void @callee(i32 7)        ; unmarked
  ret void
}
```

`min-tail.ll` is the same with `tail call`; `min-frame.ll` adds a 32-byte local frame so that an `add sp` shows in the epilogue.

## Was it our choice to mark the call `tail`? No

* fibber (lair) emits a plain `call` for an lIR `call` and `musttail` only for an lIR `tailcall` (spec/lir.md 7.3). The 0.1.12 compiler's
  IR for `cases/lir/instr/stackargs-sibcall.lir` (`fibber-prefix.ll`, lair `-O0` output with today's `notail` removed) has `call void @srand(i32 7)`
  and no `tail`.
* `opt -passes=tailcallelim` on that unmarked `min.ll` produces `tail call void @callee(i32 7)`; `opt -O1` and `-O2` do too (`function-attrs`
  alone does not). So LLVM adds the marker and then lowers it wrongly: `opt -O2 min.ll | llc -O2` gives `b callee`, `llc -O2 min.ll` alone gives `bl callee`.
* Writing `tail` by hand is valid. LangRef (21.1.8, `call`): "The `tail` marker is a hint that can be ignored." It promises only that the callee
  does not access the caller's allocas. Matching conventions and prototypes are required of `musttail` only, and fibber never emits `musttail` for
  a `tailcc` to `ccc` call (lIR's checker rejects such a `tailcall`: spec/lir.md 7.3, rule 1). An optimisation that may be ignored must not change
  the program, so a miscompile with `tail` is the backend's fault whoever wrote the marker.

## The assembly

`llc -mtriple=aarch64-linux-gnu -O2 min-frame.ll` (LLVM 21.1.8), the `tail call` form. Incoming stack arguments: 16 bytes.

```
f:
	sub	sp, sp, #32            ; frame 32
	ldr	x8, [sp, #40]          ; reads the 10th argument: the argument area starts at sp+32
	mov	x0, xzr
	str	x8, [sp], #32          ; pops 32: the frame only
	b	callee                 ; sibling branch: sp is now 16 below where the caller of f expects it
```

The same function with `notail call` (what fibber emits today) and with the plain call that `-O0` or `llc` alone keep:

```
f:
	sub	sp, sp, #48            ; frame 32 + 16 for the saved x30
	str	x30, [sp, #32]
	ldr	x8, [sp, #56]
	mov	x0, xzr
	str	x8, [sp]
	bl	callee
	ldr	x30, [sp, #32]
	add	sp, sp, #64            ; 48 + the 16 bytes of incoming arguments: a tailcc callee pops its arguments
	ret
```

This is the `#0x90` against `#0xa0` of the DARWIN-2 crash: the 16 bytes of stack arguments are missing from the pop. The caller of a `tailcc`
function assumes the callee popped its arguments (`DoesCalleeRestoreStack`), and then `ret` or a call returns through a frame 16 bytes off; the
crash reports had pc equal to lr, a stack address.

## Why LLVM gets it wrong (AArch64ISelLowering.cpp, 21.1.8 and `main`)

`AArch64TargetLowering::isEligibleForTailCallOptimization` returns `CCMatch` early only when the callee's convention is one that guarantees
tail calls (`canGuaranteeTCO`), that is when the callee is `tailcc`. For a `ccc` callee it falls through to the sibling-call checks, which test the
callee's results, preserved registers and that the callee's stack arguments fit `getBytesInStackArgArea()`. Nothing tests that the caller is itself a
callee-pops convention (`tailcc`, `swifttailcc`). `LowerCall` then sets `IsSibCall` (because the callee is not `Tail`), the call becomes a `TCRETURN`
with no adjustment, and the epilogue does not add the caller's `ArgumentStackToRestore`. X86 refuses this case (its `x86_64` output for `min-tail.ll` is `callq` and
`retq $40`), which is why only AArch64 shows it.

## Compilers and versions

`min.ll` is the unmarked source, `min-tail.ll` the marked one; "opt|llc" is `opt -O2 -S min.ll | llc -O2` of the same version. `bl` is correct, `b` is the fault.

| LLVM | where | llc min.ll | llc min-tail.ll | opt-O2 \| llc |
|------|-------|-----------|-----------------|---------------|
| 17.0.6 | apt, Ubuntu | bl | **b** | **b** |
| 18.1.8 | apt, Ubuntu | bl | **b** | **b** |
| 19.1.7 | apt, Ubuntu | bl | **b** | **b** |
| 20.1.8 | apt, Ubuntu | bl | **b** | **b** |
| 21.1.8 | apt, Ubuntu | bl | **b** | **b** |
| 22.1.2 | apt, Ubuntu | bl | **b** | **b** |
| 23.1.2 | Homebrew `llvm`, Mac | bl | **b** | **b** |
| `main` | source read (`isEligibleForTailCallOptimization`) | | | same structure: no caller check |

Not tried: LLVM 16 and older (no package on this box); a build of `main` (the release 23.1.3 of 2026-10-06 is the latest; its tarball was not fetched).

## The same shape in C (`docs/repro/aarch64-sibcall/swifttail.c`)

clang has no `tailcc` attribute; `__attribute__((swiftasynccall))` is LLVM `swifttailcc`, which shares the callee-pops lowering. The program has a
`swiftasynccall` function of ten `int64_t` arguments that ends in a call of an ordinary C function; clang writes the call unmarked and its own
`-O1` pipeline marks it `tail` (`tail call void @callee(i32 noundef 7)` in `-emit-llvm`). The function compiles to `b callee` and the loop in
`drive` loses 16 bytes of stack per call. Result of running the arm64 binary (`-O0` is the control: it prints `ok`):

| compiler | -O0 | -O1 | -O2 | -O3 |
|----------|-----|-----|-----|-----|
| Apple clang 21.0.0 (clang-2100.3.34.2), Mac M1 Ultra | ok | SIGSEGV (139) | SIGSEGV | SIGSEGV |
| Homebrew clang 21.1.8, Mac | ok | SIGSEGV | SIGSEGV | SIGSEGV |
| Homebrew clang 23.1.2, Mac | ok | SIGSEGV | SIGSEGV | SIGSEGV |
| Ubuntu clang 21.1.8, run under qemu-aarch64 on x86 | ok | SIGSEGV | SIGSEGV | not run |

The fibber program itself, with the `notail` mark removed (`fibber-prefix.ll`), built by clang 21 for aarch64-linux and run under qemu-aarch64:
`-O1` and `-O2` segfault; the same module as fibber emits it today (`notail`) prints `stackargs 509500` and `stackargs-fp 20.5` at `-O2`. (At `-O0`
the clang-built binary of the module hangs both with and without the mark under qemu; that is a different matter, not looked into, and not what lairf's `-O 0` does.)

Rust: `extern "C"` cannot name `tailcc`, and the stable language has no way to write the callee-pops convention, so the `.ll` and the C file are the reproducers.
No Rust toolchain was used.

## Upstream

Searched `llvm/llvm-project` issues (read-only, 2026-10-08) for: tailcc, swifttailcc, sibling call, AArch64, stack arguments, callee pops.
No report of this fault. Related and different: #168956 (x86 musttail sibcall miscompilation, closed), #199691 (x86 musttail with stack arguments, closed),
#223545 and #217156 (AArch64 non-sibcall tail calls with FPDiff, closed), #213811 (AArch64 GlobalISel split arguments for tailcc, open), #206718 (AArch64 `byval` in
tail calls, open), #167181 (AArch64 `musttail` from `tailcc`, open; a different call shape). None covers a `ccc` callee called from a `tailcc` caller.

Filed 2026-10-08: https://github.com/llvm/llvm-project/issues/230214. A candidate fix with a lit test (SelectionDAG and GlobalISel, `isEligibleForTailCallOptimization`) exists as a local commit on `main`, not yet a pull request.

### Ready-to-file report

**Title:** [AArch64] Sibling call from a tailcc/swifttailcc function with stack arguments to a C function does not pop the caller's arguments

**Body:** On AArch64 a `tailcc` (or `swifttailcc`) function that has stack-passed arguments and ends in a call of a `ccc` function is compiled to a
sibling branch (`b callee`) when the call is marked `tail`, without popping the 16 bytes (here) of incoming stack arguments that its callee-pops convention
requires. The caller's caller then continues with sp off by that amount. `opt -O1` and above add `tail` themselves to a plain call (`tailcallelim`), so
ordinary IR with no marker is miscompiled by `opt -O2 | llc`. x86-64 handles the same IR correctly (`retq $40`).

```llvm
target triple = "aarch64-unknown-linux-gnu"
declare void @callee(ptr)
define tailcc void @f(i64 %a, i64 %b, i64 %c, i64 %d, i64 %e, i64 %f, i64 %g, i64 %h, i64 %i, i64 %j) {
  %x = alloca [4 x i64]
  store volatile i64 %j, ptr %x
  tail call void @callee(ptr null)
  ret void
}
```

`llc -mtriple=aarch64-linux-gnu -O2`: `sub sp, sp, #32 ... str x8, [sp], #32 ; b callee`. Expected: the epilogue also releases the 16 bytes of
incoming arguments (`add sp, sp, #48` in total, or the call kept as `bl callee` followed by `add sp, sp, #64` as with `notail`). Seen with LLVM 17.0.6, 18.1.8,
19.1.7, 20.1.8, 21.1.8, 22.1.2, 23.1.2. Cause: `AArch64TargetLowering::isEligibleForTailCallOptimization` checks `CCMatch` only for guaranteed-TCO callees and never checks that a
callee-pops caller (`CallingConv::Tail`, `SwiftTail`) has no stack arguments to restore; `IsSibCall` is then set because the callee is not `Tail`. A C reproducer through
`__attribute__((swiftasynccall))` is attached (`swifttail.c`; segfaults at -O1 and above on arm64 macOS and Linux).
Suggested fix: in `isEligibleForTailCallOptimization`, return false for a sibling call when `DoesCalleeRestoreStack(CallerCC, ...)` and the caller has stack arguments
(`FuncInfo->getArgumentStackToRestore()` or the incoming argument area is not empty).

## Installs on this box (Ubuntu 26.04, apt, for the version table)

`llvm-17` 17.0.6, `llvm-18` 18.1.8, `llvm-19` 19.1.7, `llvm-20` 20.1.8, `llvm-22` 22.1.2 (llc and opt for the table; `llvm-21` and `clang-21` were there). No other change.
Mac: nothing installed; Apple clang and Homebrew `llvm@21` and `llvm` (23.1.2) were already there; files under `~/fibber-a64-scratch/repro` only.
