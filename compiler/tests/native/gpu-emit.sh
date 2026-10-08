#!/bin/bash
# The kernel target (docs/design/gpu.md; compiler/types/targets.fib `row-nvptx64-cuda`; native.kernel; own.kernel): what `fibc build --target
# nvptx64-nvidia-cuda --emit ptx` writes for examples/gpu/kernels.fib, what it refuses, and what `--kernel-target` builds. No GPU is needed
# (fib-gpu-cuda runs the PTX; its tests are in that repository).
#   1. the table has the row, marked a kernel target, and `fibc build` accepts it without --allow-unsupported;
#   2. the PTX holds the four kernels as `.visible .entry` with their PTX names (vadd, gemm, gemm_smem, assert_positive), the launch-ABI header
#      (`// fib.kernel-sig vadd: ptr ptr ptr i64`), `fma.rn.f32` for `simd/fma`, `trap;` for the trap entry, `%tid.x` and `%ctaid.y` for the
#      index space, `.shared`, `ld.shared`, `st.shared` and `bar.sync` for `gpu/shared` and `gpu/barrier`, and NO byte-wise access
#      (`ld.global.b8`: the align-1 rule of native.lower.memory), NO runtime function (`fib_`), NO libc call (`malloc`, `write`); `--emit llvm`
#      of the same holds `ptx_kernel`, `addrspace(3)` and the LLVM 21 barrier intrinsic;
#   3. the kernels run on the host (`fibc run examples/gpu/gpu.fib`: the one-language guard, 0 mismatches against a CPU loop);
#   4. refusals: a kernel that allocates (an `array` in the body) and one that prints are refused by the kernel checker at their source
#      positions (own.kernel; the cases 8530-8569 of cases/stdlib have the rest); `--emit obj` on the kernel target is refused; `--emit ptx`
#      for the host is refused; a program with no kernel is refused; `fibc build` of a program with a kernel and no kernel target is refused
#      naming the kernels and the platform, `--kernel-target none` the same, `--kernel-target bogus` too; `--kernel-target nvptx64-nvidia-cuda`
#      builds the executable with OUT.ptx beside it and the same PTX embedded (`gpu/program-ptx`: the executable prints its length);
#   5. planted faults: a PTX with a kernel's name changed must fail check 2 (the check reads the real output, so the fault is planted
#      in a copy and the check function rerun on it).
# usage: gpu-emit.sh   F=stage-2 fibc (FIBC also read). Exit 0 when every check holds and every fault is caught, 1 otherwise, 2 no fibc.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "gpu-emit: no fibc: set F" >&2; exit 2; }
export FIB_LIB=$root/lib
T=$(mktemp -d "${TMPDIR:-/tmp}/gpu-emit.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
K=examples/gpu/kernels.fib
H=examples/gpu/gpu.fib

# 1. the row
"$F" targets > "$T/rows"
grep -q "^nvptx64-nvidia-cuda	nvptx64	cuda	.*kernel target" "$T/rows" && ok "table: nvptx64-nvidia-cuda is a kernel target" || no "table: no kernel row"
if "$F" build --target nvptx64-nvidia-cuda "$K" --emit ptx -o "$T/k.ptx" 2> "$T/err"; then ok "build --emit ptx: accepted without --allow-unsupported"; else no "build --emit ptx: $(cat "$T/err")"; fi

# 2. the PTX
ptx_checks() { # ptx_checks FILE: the content checks; prints FAIL lines, returns 1 on any
  local f=$1 r=0
  for want in '// fib.kernel-sig vadd: ptr ptr ptr i64' '// fib.kernel-sig gemm_smem: ptr ptr ptr i64' '.visible .entry vadd(' '.visible .entry gemm(' '.visible .entry gemm_smem(' '.visible .entry assert_positive(' 'fma.rn.f32' 'trap;' '%tid.x' '%ctaid.y' '.target sm_89' '.shared .align 4 .b8' 'ld.shared' 'st.shared' 'bar.sync'; do
    grep -qF -- "$want" "$f" || { echo "     missing: $want"; r=1; }
  done
  for forbid in 'ld.global.b8' 'st.global.b8' 'fib_trace' 'fib_alloc' 'malloc' 'write' 'musttail'; do
    grep -q -- "$forbid" "$f" && { echo "     present: $forbid"; r=1; }
  done
  return $r
}
if ptx_checks "$T/k.ptx"; then ok "PTX: four entries with their signatures, fma, trap, the index space, shared memory, the barrier, no byte access, no runtime, no libc"; else no "PTX content"; fi
"$F" build --target nvptx64-nvidia-cuda "$K" --emit llvm -o "$T/k.ll" 2> "$T/err" && grep -q "ptx_kernel" "$T/k.ll" && ok "LLVM IR: ptx_kernel" || no "LLVM IR: $(cat "$T/err")"
grep -q 'target triple = "nvptx64-nvidia-cuda"' "$T/k.ll" && ok "LLVM IR: the triple" || no "LLVM IR: triple"
grep -q "nvvm.read.ptx.sreg.tid.x\|llvm.nvvm.read.ptx.sreg" "$T/k.ll" && ok "LLVM IR: (sreg ..) lowered to the nvvm intrinsic" || no "LLVM IR: no sreg intrinsic"
grep -q "addrspace(3) global" "$T/k.ll" && grep -q "llvm.nvvm.barrier.cta.sync.aligned.all" "$T/k.ll" && ok "LLVM IR: (global shared ..) is addrspace(3), (barrier) the LLVM 21 barrier intrinsic" || no "LLVM IR: shared memory or barrier"

# 3. the host runs the kernels over a grid (the one-language guard)
"$F" run "$H" -I examples/gpu > "$T/run" 2>&1 && grep -q "host vadd 100000: 0 mismatches; host gemm 128: 0 mismatches" "$T/run" && [ "$(tail -n 1 "$T/run")" = 0 ] && ok "host: vadd and gemm through host-launch match the CPU loop: $(head -n 1 "$T/run" | cut -c1-80)" || no "host run: $(cat "$T/run")"

# 4. refusals
mkdir -p "$T/p"
cat > "$T/p/alloc.fib" <<'EOF'
(ns main (:require [fib.gpu :as gpu]))
(defkernel k (a: ptr n: i64) (let ((v (array 4 0.0f32))) (gpu/f32-set! a 0 (array-get v 0))))
EOF
if "$F" build --target nvptx64-nvidia-cuda "$T/p/alloc.fib" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "a kernel that allocates was accepted"; else grep -q "alloc.fib:2:39: kernel k: the builtin array: a kernel allocates nothing" "$T/err" && ok "a kernel that allocates is refused at its position: $(grep -o 'alloc.fib.*' "$T/err" | head -c 100)" || no "alloc refusal text: $(cat "$T/err")"; fi
cat > "$T/p/print.fib" <<'EOF'
(ns main (:require [fib.gpu :as gpu]))
(defkernel k (a: ptr n: i64) (println "no"))
EOF
if "$F" build --target nvptx64-nvidia-cuda "$T/p/print.fib" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "a kernel that prints was accepted"; else grep -q "print.fib:2:39: kernel k: a string literal" "$T/err" && ok "a kernel that prints is refused at the source: $(grep -o 'print.fib.*' "$T/err" | head -c 100)" || no "print refusal text: $(cat "$T/err")"; fi
cat > "$T/p/none.fib" <<'EOF'
(ns main)
(defun main () -> i64 0)
EOF
if "$F" build --target nvptx64-nvidia-cuda "$T/p/none.fib" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "a program with no kernel was accepted"; else grep -q "no kernel" "$T/err" && ok "no kernel: refused" || no "no-kernel text: $(cat "$T/err")"; fi
if "$F" build --target nvptx64-nvidia-cuda "$K" --emit obj -o "$T/x.o" 2> "$T/err"; then no "--emit obj on the kernel target was accepted"; else grep -q "PTX" "$T/err" && ok "--emit obj on the kernel target: refused" || no "obj text: $(cat "$T/err")"; fi
if "$F" build "$H" -I examples/gpu --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "--emit ptx for the host was accepted"; else grep -q "kernel target" "$T/err" && ok "--emit ptx for the host: refused" || no "host ptx text: $(cat "$T/err")"; fi
if "$F" build --target nvptx64-nvidia-cuda "$K" -o "$T/x" 2> "$T/err"; then no "an executable for the kernel target was linked"; else ok "an executable for the kernel target: refused"; fi
# a kernel built for a platform with no kernel target: refused (docs/design/decisions-2026-10-04.md, GPU); with the target: the executable, OUT.ptx, the PTX embedded
cp "$K" "$T/p/"; cp "$H" "$T/p/host.fib"
if "$F" build "$T/p/host.fib" -I "$T/p" -o "$T/host" 2> "$T/err"; then no "a program with kernels was built for the host without a kernel target"; else grep -q -E "fibc: the program has the kernels vadd, gemm, gemm-smem, assert-positive \(defkernel\) and [a-z0-9_]+-[a-z]+-[a-z0-9.-]+ has no kernel target: build with --kernel-target nvptx64-nvidia-cuda" "$T/err" && ok "no kernel target: refused naming the kernels and the platform" || no "no-kernel-target text: $(cat "$T/err")"; fi
if "$F" build "$T/p/host.fib" -I "$T/p" -o "$T/host" --kernel-target none 2> "$T/err"; then no "--kernel-target none built a program with kernels"; else grep -q "has no kernel target" "$T/err" && ok "--kernel-target none: refused" || no "kernel-target none text: $(cat "$T/err")"; fi
if "$F" build "$T/p/host.fib" -I "$T/p" -o "$T/host" --kernel-target bogus 2> "$T/err"; then no "--kernel-target bogus was accepted"; else grep -q "not a kernel target" "$T/err" && ok "--kernel-target bogus: refused" || no "bogus text: $(cat "$T/err")"; fi
if "$F" build "$T/p/host.fib" -I "$T/p" -o "$T/host" --kernel-target nvptx64-nvidia-cuda 2> "$T/err" && [ -s "$T/host.ptx" ] && grep -q "fib.kernel-sig gemm_smem" "$T/host.ptx"; then
  "$T/host" > "$T/hostrun" 2>&1; n=$(wc -c < "$T/host.ptx" | tr -d " ")
  grep -q "program-ptx: $n bytes" "$T/hostrun" && ok "--kernel-target nvptx64-nvidia-cuda: the executable, host.ptx beside it ($n bytes) and the same PTX embedded (gpu/program-ptx)" || no "embedded PTX: $(cat "$T/hostrun")"
else no "--kernel-target build: $(cat "$T/err")"; fi

# 5. the planted fault
sed 's/\.entry gemm(/.entry gemn(/' "$T/k.ptx" > "$T/planted.ptx"
if ptx_checks "$T/planted.ptx" > /dev/null; then no "planted: a renamed kernel passed the PTX checks"; else ok "planted: a renamed kernel fails the PTX checks"; fi
sed 's/ld.global.b32/ld.global.b8/' "$T/k.ptx" > "$T/planted2.ptx"
if ptx_checks "$T/planted2.ptx" > /dev/null; then no "planted: byte-wise loads passed the PTX checks"; else ok "planted: byte-wise loads fail the PTX checks"; fi

[ $bad -eq 0 ] && echo "gpu-emit: every check holds" || echo "gpu-emit: FAILED"
exit $bad
