#!/bin/bash
# The kernel target (docs/design/gpu.md; compiler/types/targets.fib `row-nvptx64-cuda`; native.kernel): what `fibc build --target
# nvptx64-nvidia-cuda --emit ptx` writes for examples/gpu/kernels.fib, and what it refuses. No GPU is needed (fib-gpu-cuda runs the PTX).
#   1. the table has the row, marked a kernel target, and `fibc build` accepts it without --allow-unsupported;
#   2. the PTX holds the three kernels as `.visible .entry` with their PTX names (vadd, gemm, assert_positive), `fma.rn.f32` for `simd/fma`,
#      `trap;` for the trap entry, `%tid.x` and `%ctaid.y` for the index space, and NO byte-wise access (`ld.global.b8`: the align-1 rule of
#      native.lower.memory), NO runtime function (`fib_`), NO libc call (`malloc`, `write`); `--emit llvm` of the same holds `ptx_kernel`;
#   3. the same file still builds and runs for the host (`fibc run`: 0);
#   4. refusals: a kernel that allocates (an `array` in the body) is refused naming the runtime function and the path; a kernel that calls
#      `println` is refused naming the C function; `--emit obj` on the kernel target is refused; `--emit ptx` for the host is refused;
#      a program with no kernel is refused; lIR with `(sreg bogus)` is refused by the parser;
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

# 1. the row
"$F" targets > "$T/rows"
grep -q "^nvptx64-nvidia-cuda	nvptx64	cuda	.*kernel target" "$T/rows" && ok "table: nvptx64-nvidia-cuda is a kernel target" || no "table: no kernel row"
if "$F" build --target nvptx64-nvidia-cuda "$K" --emit ptx -o "$T/k.ptx" 2> "$T/err"; then ok "build --emit ptx: accepted without --allow-unsupported"; else no "build --emit ptx: $(cat "$T/err")"; fi

# 2. the PTX
ptx_checks() { # ptx_checks FILE: the content checks; prints FAIL lines, returns 1 on any
  local f=$1 r=0
  for want in '.visible .entry vadd(' '.visible .entry gemm(' '.visible .entry assert_positive(' 'fma.rn.f32' 'trap;' '%tid.x' '%ctaid.y' '.target sm_89'; do
    grep -qF -- "$want" "$f" || { echo "     missing: $want"; r=1; }
  done
  for forbid in 'ld.global.b8' 'st.global.b8' 'fib_' 'malloc' 'write' 'musttail'; do
    grep -q -- "$forbid" "$f" && { echo "     present: $forbid"; r=1; }
  done
  return $r
}
if ptx_checks "$T/k.ptx"; then ok "PTX: three entries, fma, trap, the index space, no byte access, no runtime, no libc"; else no "PTX content"; fi
"$F" build --target nvptx64-nvidia-cuda "$K" --emit llvm -o "$T/k.ll" 2> "$T/err" && grep -q "ptx_kernel" "$T/k.ll" && ok "LLVM IR: ptx_kernel" || no "LLVM IR: $(cat "$T/err")"
grep -q 'target triple = "nvptx64-nvidia-cuda"' "$T/k.ll" && ok "LLVM IR: the triple" || no "LLVM IR: triple"
grep -q "nvvm.read.ptx.sreg.tid.x\|llvm.nvvm.read.ptx.sreg" "$T/k.ll" && ok "LLVM IR: (sreg ..) lowered to the nvvm intrinsic" || no "LLVM IR: no sreg intrinsic"

# 3. the host still runs it
"$F" run "$K" > "$T/run" 2>&1 && [ "$(cat "$T/run")" = 0 ] && ok "host: fibc run of the kernels file is 0" || no "host run: $(cat "$T/run")"

# 4. refusals
mkdir -p "$T/p"; cp examples/gpu/gpu.fib "$T/p/"
cat > "$T/p/alloc.fib" <<'EOF'
(ns main (:require [gpu :as gpu]))
(gpu/defkernel k (a: ptr n: i64) (let ((v (array 4 0.0f32))) (gpu/f32-set! a 0 (array-get v 0))))
EOF
if "$F" build --target nvptx64-nvidia-cuda "$T/p/alloc.fib" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "a kernel that allocates was accepted"; else grep -q "reaches the runtime function fib\." "$T/err" && grep -q "<-" "$T/err" && ok "a kernel that allocates is refused with the path: $(head -c 120 "$T/err")" || no "alloc refusal text: $(cat "$T/err")"; fi
cat > "$T/p/print.fib" <<'EOF'
(ns main (:require [gpu :as gpu]))
(gpu/defkernel k (a: ptr n: i64) (println "no"))
EOF
if "$F" build --target nvptx64-nvidia-cuda "$T/p/print.fib" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "a kernel that prints was accepted"; else grep -q "reaches the runtime function\|reaches the C function" "$T/err" && ok "a kernel that prints is refused: $(head -c 100 "$T/err")" || no "print refusal text: $(cat "$T/err")"; fi
cat > "$T/p/none.fib" <<'EOF'
(ns main)
(defun main () -> i64 0)
EOF
if "$F" build --target nvptx64-nvidia-cuda "$T/p/none.fib" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "a program with no kernel was accepted"; else grep -q "no kernel" "$T/err" && ok "no kernel: refused" || no "no-kernel text: $(cat "$T/err")"; fi
if "$F" build --target nvptx64-nvidia-cuda "$K" --emit obj -o "$T/x.o" 2> "$T/err"; then no "--emit obj on the kernel target was accepted"; else grep -q "PTX" "$T/err" && ok "--emit obj on the kernel target: refused" || no "obj text: $(cat "$T/err")"; fi
if "$F" build "$K" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "--emit ptx for the host was accepted"; else grep -q "kernel target" "$T/err" && ok "--emit ptx for the host: refused" || no "host ptx text: $(cat "$T/err")"; fi
if "$F" build --target nvptx64-nvidia-cuda "$K" -o "$T/x" 2> "$T/err"; then no "an executable for the kernel target was linked"; else ok "an executable for the kernel target: refused"; fi
printf '(define (f i32) () (block entry (ret (sreg bogus))))\n' > "$T/bad.lir"
if "$F" build --target nvptx64-nvidia-cuda "$T/bad.lir" --emit ptx -o "$T/x.ptx" 2> "$T/err"; then no "a .lir file was built"; else ok "a .lir program is not a fibber program (the lIR parser's sreg check is lairf's: compiler/tests/lir)"; fi

# 5. the planted fault
sed 's/\.entry gemm(/.entry gemn(/' "$T/k.ptx" > "$T/planted.ptx"
if ptx_checks "$T/planted.ptx" > /dev/null; then no "planted: a renamed kernel passed the PTX checks"; else ok "planted: a renamed kernel fails the PTX checks"; fi
sed 's/ld.global.b32/ld.global.b8/' "$T/k.ptx" > "$T/planted2.ptx"
if ptx_checks "$T/planted2.ptx" > /dev/null; then no "planted: byte-wise loads passed the PTX checks"; else ok "planted: byte-wise loads fail the PTX checks"; fi

[ $bad -eq 0 ] && echo "gpu-emit: every check holds" || echo "gpu-emit: FAILED"
exit $bad
