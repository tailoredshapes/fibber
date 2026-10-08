#!/bin/bash
# GPU-3: atomics and block/grid reductions (docs/design/gpu.md 6.6, 6.7; spec/types.md 2.17), from source to a device.
#   1. the emitted text, no GPU needed: the PTX of examples/gpu/atomics.fib and of fib.gpu.reduce-kernels holds the atomic instructions (`atom.global.add`,
#      `atom.shared.max`, `atom.global.min`, `atom.relaxed.global.cas`, `atom.global.add.f32`) and `bar.sync`; the WGSL holds `array<atomic<u32>>`, `atomicAdd`,
#      `atomicCompareExchangeWeak`, `workgroupBarrier()`; a module with no atomic prints the plain `array<u32>`; the f32 atomic is refused by name for WGSL;
#      naga validates the WGSL, and Tint (Dawn: its uniformity analysis is why the reductions are written without a thread-dependent branch) when node has it;
#   2. the host: `fibc run examples/gpu/reduce.fib` runs the kernels over a grid of one-thread blocks and prints fibber's CPU reference (exit 0 = 0 mismatches);
#   3. the device, when there is one (an NVIDIA GPU and libcuda for PTX; an adapter and the `webgpu` npm package for WGSL; each skipped with a note when missing):
#      cuda_run.py and wgsl_run.mjs launch the kernels and compare with the reference: the integer reductions, the f32 maximum and `sum-f32` within its bound,
#      the per-block partials BIT FOR BIT with the CPU's tree order, the atomics (histogram, tally, compare-and-swap, shared-memory block maximum), and a wrong
#      `blocks` argument ending in a device trap;
#   4. planted faults: the references and the artifacts are altered in copies and the same checks must fail: a wrong reference value, a histogram whose atomic
#      add is an atomic or, a text check that cannot find `atom.shared`.
# usage: gpu-device.sh   F=stage-2 fibc (FIBC also read). Exit 0 when every check holds and every planted fault is caught, 1 otherwise, 2 no fibc.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "gpu-device: no fibc: set F" >&2; exit 2; }
export FIB_LIB=$root/lib
NAGA=${NAGA:-$HOME/.cache/fibber-scratch/tools/naga/bin/naga}
T=$(mktemp -d "${TMPDIR:-/tmp}/gpu-device.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
D=compiler/tests/native/gpu
INC=(-I examples/gpu -I examples/webgpu)
ptx() { "$F" build "${INC[@]}" --target nvptx64-nvidia-cuda "$1" --emit ptx -o "$2"; }
wgsl() { "$F" build "${INC[@]}" --target wgsl-unknown-webgpu "$1" --emit wgsl -o "$2"; }

# 1. the emitted text
if ptx $D/reduce-all.fib "$T/reduce.ptx" 2> "$T/err" && ptx examples/gpu/atomics.fib "$T/atomics.ptx" 2>> "$T/err"; then ok "PTX built: fib.gpu.reduce-kernels and examples/gpu/atomics.fib"; else no "PTX build: $(cat "$T/err")"; fi
if wgsl $D/reduce-portable.fib "$T/reduce.wgsl" 2> "$T/err" && wgsl examples/gpu/atomics.fib "$T/atomics.wgsl" 2>> "$T/err"; then ok "WGSL built: the portable reductions and examples/gpu/atomics.fib"; else no "WGSL build: $(cat "$T/err")"; fi
ptx_checks() { # ptx_checks REDUCE.ptx ATOMICS.ptx: prints what is missing, returns 1 on any
  local r=0
  for want in '.visible .entry reduce_sum_i32(' '.visible .entry reduce_partials_f32(' '.visible .entry reduce_sum_f32(' 'atom.global.add.f32' 'atom.global.max.s32' 'atom.global.min.s32' 'atom.relaxed.global.cas.b32' 'bar.sync'; do
    grep -qF -- "$want" "$1" || { echo "     missing in the reduce PTX: $want"; r=1; }
  done
  for want in '.visible .entry hist(' '.visible .entry block_max(' 'atom.global.add.u32' 'atom.global.max.s32' 'atom.global.min.s32' 'atom.global.max.u32' 'atom.relaxed.global.cas.b32' 'atom.shared.max.s32' 'bar.sync'; do
    grep -qF -- "$want" "$2" || { echo "     missing in the atomics PTX: $want"; r=1; }
  done
  return $r
}
if ptx_checks "$T/reduce.ptx" "$T/atomics.ptx"; then ok "PTX: the atom.* forms (global add/min/max/cas, shared max, f32 add), the barrier, the entries"; else no "PTX content"; fi
wgsl_checks() { # wgsl_checks REDUCE.wgsl ATOMICS.wgsl
  local r=0
  for f in "$1" "$2"; do
    for want in 'array<atomic<u32>>' 'fn fibw_atomic_add(' 'atomicAdd(' 'atomicLoad(' 'atomicStore(' 'workgroupBarrier()' 'override wg_x'; do
      grep -qF -- "$want" "$f" || { echo "     missing in $(basename "$f"): $want"; r=1; }
    done
  done
  for want in 'atomicCompareExchangeWeak(' 'fn fibw_atomic_cas(' 'fn fibw_atomic_smax(' 'atomic<u32>, 1>'; do
    grep -qF -- "$want" "$2" || { echo "     missing in the atomics WGSL: $want"; r=1; }
  done
  return $r
}
if wgsl_checks "$T/reduce.wgsl" "$T/atomics.wgsl"; then ok "WGSL: atomic buffers and workgroup words, the helpers, compare-and-exchange, the barrier"; else no "WGSL content"; fi
if wgsl examples/webgpu/kernels.fib "$T/plain.wgsl" 2> "$T/err" && grep -q 'array<u32>;' "$T/plain.wgsl" && ! grep -q 'atomic<u32>>' "$T/plain.wgsl"; then ok "WGSL: a module with no atomic keeps the plain array<u32> buffers"; else no "plain module: $(cat "$T/err")"; fi
if wgsl $D/reduce-all.fib "$T/x.wgsl" 2> "$T/err"; then no "the f32 atomic sum was accepted for WGSL"; else grep -q "an f32 atomic" "$T/err" && ok "WGSL: the f32 atomic is refused by name: $(head -c 120 "$T/err")" || no "f32 atomic refusal text: $(cat "$T/err")"; fi
if [ -x "$NAGA" ]; then
  for w in reduce atomics; do
    if "$NAGA" "$T/$w.wgsl" > "$T/naga" 2>&1; then ok "naga: $w.wgsl validation successful"; else no "naga rejects $w.wgsl: $(head -n 5 "$T/naga")"; fi
  done
else echo "note naga not found at $NAGA (scripts/fetch-webgpu-tools.sh naga): validation skipped"; fi
if command -v node > /dev/null 2>&1; then
  for w in reduce atomics; do
    node examples/webgpu/js/validate.mjs "$T/$w.wgsl" > "$T/tint" 2>&1; code=$?
    if [ $code -eq 0 ]; then ok "Tint (Dawn): $w.wgsl has no errors"; elif [ $code -eq 3 ]; then echo "note the webgpu npm package or an adapter is missing: Tint validation skipped"; break
    else no "Tint rejects $w.wgsl: $(grep -v 'Warning: max' "$T/tint" | head -n 6)"; fi
  done
fi

# 2. the host
"$F" run examples/gpu/reduce.fib > "$T/ref.txt" 2> "$T/err"; code=$?
if [ $code -eq 0 ] && grep -q "^host mismatches: 0$" "$T/ref.txt" && [ "$(grep -c '^ref ' "$T/ref.txt")" -eq 30 ]; then ok "host: examples/gpu/reduce.fib: the kernels over one-thread blocks equal the reference (0 mismatches), 30 reference lines"; else no "host run (exit $code): $(tail -n 3 "$T/ref.txt") $(head -n 3 "$T/err")"; fi

# 3. the device
cuda() { python3 $D/cuda_run.py "$@"; }
cuda badgrid "$T/reduce.ptx" > "$T/cuda.out" 2>&1; code=$?
if [ $code -eq 3 ]; then echo "note no CUDA device or libcuda: the PTX runs are skipped"; cuda_on=0
else
  cuda_on=1
  [ $code -eq 0 ] && ok "device (CUDA): $(cat "$T/cuda.out" | head -c 160)" || no "device (CUDA) badgrid: $(cat "$T/cuda.out")"
  cuda reduce "$T/reduce.ptx" "$T/ref.txt" > "$T/cuda.out" 2>&1; code=$?
  if [ $code -eq 0 ]; then ok "device (CUDA): $(grep -c '^ok' "$T/cuda.out") reduction checks equal fibber's CPU reference: $(grep partials "$T/cuda.out" | head -n 1 | cut -c1-90)"; else no "device (CUDA) reductions: $(grep -v '^ok' "$T/cuda.out" | head -n 5)"; fi
  cuda atomics "$T/atomics.ptx" > "$T/cuda.out" 2>&1; code=$?
  if [ $code -eq 0 ]; then ok "device (CUDA): $(grep -c '^ok' "$T/cuda.out") atomics checks equal the CPU's"; else no "device (CUDA) atomics: $(grep -v '^ok' "$T/cuda.out" | head -n 5)"; fi
fi
wg() { node $D/wgsl_run.mjs "$@" 2>&1 | grep -v '^Warning'; }
wg badgrid "$T/reduce.wgsl" > "$T/wg.out"; code=${PIPESTATUS[0]}
if [ "$code" -eq 3 ]; then echo "note no WebGPU adapter or the webgpu npm package: the WGSL runs are skipped"; wg_on=0
else
  wg_on=1
  grep -q "^ok" "$T/wg.out" && ok "device (WebGPU): $(head -c 160 "$T/wg.out")" || no "device (WebGPU) badgrid: $(cat "$T/wg.out")"
  wg reduce "$T/reduce.wgsl" "$T/ref.txt" > "$T/wg.out"
  if [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -ge 25 ]; then ok "device (WebGPU): $(grep -c '^ok' "$T/wg.out") reduction checks equal fibber's CPU reference"; else no "device (WebGPU) reductions: $(grep -v '^ok' "$T/wg.out" | head -n 5)"; fi
  wg atomics "$T/atomics.wgsl" > "$T/wg.out"
  if [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -ge 5 ]; then ok "device (WebGPU): $(grep -c '^ok' "$T/wg.out") atomics checks equal the CPU's"; else no "device (WebGPU) atomics: $(grep -v '^ok' "$T/wg.out" | head -n 5)"; fi
fi

# 4. planted faults
sed 's/atom.shared.max.s32/atom.shared.xxx.s32/' "$T/atomics.ptx" > "$T/p.ptx"
if ptx_checks "$T/reduce.ptx" "$T/p.ptx" > /dev/null; then no "planted: a PTX without atom.shared passed the text checks"; else ok "planted: a PTX without atom.shared fails the text checks"; fi
sed 's/atomicCompareExchangeWeak/atomicXxx/g' "$T/atomics.wgsl" > "$T/p.wgsl"
if wgsl_checks "$T/reduce.wgsl" "$T/p.wgsl" > /dev/null; then no "planted: a WGSL without compare-and-exchange passed the text checks"; else ok "planted: a WGSL without compare-and-exchange fails the text checks"; fi
if [ $cuda_on -eq 1 ]; then
  sed 's/^ref sum-i32 1000003 7 32 .*/ref sum-i32 1000003 7 32 1/' "$T/ref.txt" > "$T/ref-bad.txt"
  if cuda reduce "$T/reduce.ptx" "$T/ref-bad.txt" > "$T/cuda.out" 2>&1; then no "planted: a wrong reference sum was accepted by the device comparison"; else grep -q "^FAIL sum-i32 n=1000003 grid=7" "$T/cuda.out" && ok "planted: a wrong reference value fails the device comparison (CUDA)" || no "planted reference: $(grep -v '^ok' "$T/cuda.out" | head -n 3)"; fi
  sed 's/atom.global.add.u32/atom.global.or.b32/' "$T/atomics.ptx" > "$T/p.ptx"
  if cuda atomics "$T/p.ptx" > "$T/cuda.out" 2>&1; then no "planted: an atomic or in place of the atomic add passed the histogram"; else grep -q "^FAIL hist" "$T/cuda.out" && ok "planted: an atomic or in place of the atomic add fails the histogram on the device (CUDA)" || no "planted histogram: $(grep -v '^ok' "$T/cuda.out" | head -n 3)"; fi
fi
if [ $wg_on -eq 1 ]; then
  sed 's/atomicAdd(/atomicOr(/g' "$T/atomics.wgsl" > "$T/p.wgsl"
  wg atomics "$T/p.wgsl" > "$T/wg.out"
  if grep -q "^FAIL hist" "$T/wg.out"; then ok "planted: atomicOr in place of atomicAdd fails the histogram on the device (WebGPU)"; else no "planted WGSL histogram: $(head -n 3 "$T/wg.out")"; fi
fi

[ $bad -eq 0 ] && echo "gpu-device: every check holds" || echo "gpu-device: FAILED"
exit $bad
