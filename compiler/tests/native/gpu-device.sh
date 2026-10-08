#!/bin/bash
# GPU-3: atomics and block/grid reductions (docs/design/gpu.md 6.6, 6.7; spec/types.md 2.17), from source to a device.
#   1. the emitted text, no GPU needed: the PTX of examples/gpu/atomics.fib and of fib.gpu.reduce-kernels holds the atomic instructions (`atom.global.add`,
#      `atom.shared.max`, `atom.global.min`, `atom.relaxed.global.cas`, `atom.global.add.f32`) and `bar.sync`; the WGSL holds `array<atomic<u32>>`, `atomicAdd`,
#      `atomicCompareExchangeWeak`, `workgroupBarrier()`; a module with no atomic prints the plain `array<u32>`; the f32 atomic is ACCEPTED for WGSL as a
#      compare-and-exchange loop (`fibw_atomic_fadd`, GPU-5); the warp forms (GPU-5) are `shfl.sync.*` and `vote.sync.ballot` in the PTX and `enable subgroups;`,
#      `subgroupShuffleXor(` and the header line `// fib.requires: subgroups` in the WGSL, and a module that uses none prints nothing of subgroups;
#      naga validates the WGSL where it can, and Tint (Dawn: its uniformity analysis is why the reductions are written without a thread-dependent branch) when node has it;
#      a WGSL that requires subgroups is REJECTED BY NAME when the adapter lacks the feature (FIB_WEBGPU_NO_SUBGROUPS=1 plays an adapter without it);
#   2. the host: `fibc run examples/gpu/reduce.fib` runs the kernels over a grid of one-thread blocks and prints fibber's CPU reference (exit 0 = 0 mismatches);
#   3. the device, when there is one (an NVIDIA GPU and libcuda for PTX; an adapter and the `webgpu` npm package for WGSL; each skipped with a note when missing):
#      cuda_run.py and wgsl_run.mjs launch the kernels and compare with the reference: the integer reductions, the f32 maximum and `sum-f32` within its bound,
#      the per-block partials BIT FOR BIT with the CPU's tree order, the atomics (histogram, tally, compare-and-swap, shared-memory block maximum), and a wrong
#      `blocks` argument ending in a device trap; GPU-5: the warp-shuffle reductions against the same reference (integers and the f32 maximum exact, the partials' sum
#      within the bound), a probe of every shuffle, the lane, the width and the ballot against a model of the warp, a block below the warp ending in a trap, and the f32
#      atomics (add within the bound, max and min exact) on PTX (`atom.global.add.f32`) and on WGSL (the loop);
#   4. planted faults: the references and the artifacts are altered in copies and the same checks must fail: a wrong reference value, a histogram whose atomic
#      add is an atomic or, a text check that cannot find `atom.shared`; GPU-5: a butterfly with a wrong offset, an f32 atomic add that subtracts, a WGSL that lost
#      its `enable subgroups;`, a subgroups module that is not rejected.
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
if wgsl $D/reduce-all.fib "$T/reduce.wgsl" 2> "$T/err" && wgsl examples/gpu/atomics.fib "$T/atomics.wgsl" 2>> "$T/err"; then ok "WGSL built: every reduction (the f32 atomic sum too) and examples/gpu/atomics.fib"; else no "WGSL build: $(cat "$T/err")"; fi
if ptx $D/warp-all.fib "$T/warp.ptx" 2> "$T/err" && ptx $D/f32-atomics.fib "$T/f32.ptx" 2>> "$T/err"; then ok "PTX built: the warp-shuffle reductions, the probe and the f32 atomics"; else no "PTX warp build: $(cat "$T/err")"; fi
if wgsl $D/warp-all.fib "$T/warp.wgsl" 2> "$T/err" && wgsl $D/f32-atomics.fib "$T/f32.wgsl" 2>> "$T/err"; then ok "WGSL built: the warp-shuffle reductions, the probe and the f32 atomics"; else no "WGSL warp build: $(cat "$T/err")"; fi
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
# GPU-5: the f32 atomic, accepted; the warp forms
f32_checks() { # f32_checks REDUCE.wgsl F32.wgsl
  local r=0
  for want in 'fn fibw_atomic_fadd(' 'fn fibw_atomic_fmax(' 'fn fibw_atomic_fmin(' 'atomicCompareExchangeWeak(' 'bitcast<f32>(fibw_atomic_fadd(' 'bitcast<f32>(fibw_atomic_fmax(' 'bitcast<f32>(fibw_atomic_fmin('; do
    grep -qF -- "$want" "$2" || { echo "     missing in the f32 atomics WGSL: $want"; r=1; }
  done
  grep -qF -- 'bitcast<f32>(fibw_atomic_fadd(' "$1" || { echo "     missing in the reduce WGSL (reduce_sum_f32): bitcast<f32>(fibw_atomic_fadd("; r=1; }
  return $r
}
if f32_checks "$T/reduce.wgsl" "$T/f32.wgsl"; then ok "WGSL: the f32 atomic add, max and min are accepted as compare-and-exchange loops (the old refusal is gone)"; else no "WGSL f32 atomics content"; fi
if grep -qF 'atom.global.add.f32' "$T/f32.ptx"; then ok "PTX: the f32 atomic add stays native (atom.global.add.f32)"; else no "PTX f32 atomic add"; fi
warp_ptx_checks() { # warp_ptx_checks WARP.ptx
  local r=0
  for want in 'shfl.sync.bfly.b32' 'shfl.sync.down.b32' 'shfl.sync.up.b32' 'shfl.sync.idx.b32' 'vote.sync.ballot.b32' '%laneid' 'bar.sync' '.visible .entry reduce_warp_sum_i32(' '.visible .entry reduce_warp_partials_f32(' '.visible .entry warp_probe('; do
    grep -qF -- "$want" "$1" || { echo "     missing in the warp PTX: $want"; r=1; }
  done
  return $r
}
if warp_ptx_checks "$T/warp.ptx"; then ok "PTX: shfl.sync (bfly, down, up, idx), vote.sync.ballot, %laneid, the barrier, the entries"; else no "warp PTX content"; fi
warp_wgsl_checks() { # warp_wgsl_checks WARP.wgsl
  local r=0
  for want in '// fib.requires: subgroups' 'enable subgroups;' 'subgroupShuffleXor(' 'subgroupShuffleDown(' 'subgroupShuffleUp(' 'subgroupShuffle(' 'subgroupBallot(' '@builtin(subgroup_size)' '@builtin(subgroup_invocation_id)' 'workgroupBarrier()'; do
    grep -qF -- "$want" "$1" || { echo "     missing in the warp WGSL: $want"; r=1; }
  done
  return $r
}
if warp_wgsl_checks "$T/warp.wgsl"; then ok "WGSL: enable subgroups, the header line, the shuffles, the ballot, the entry builtins"; else no "warp WGSL content"; fi
if grep -qi 'subgroup' "$T/plain.wgsl" "$T/atomics.wgsl"; then no "a module with no warp form prints subgroups"; else ok "WGSL: a module with no warp form prints nothing of subgroups"; fi
grep -q '^warning: wgsl: function .*state machine' "$T/err" && ok "WGSL: a function the relooper cannot structure warns and falls back to the state machine: $(grep -m1 '^warning: wgsl' "$T/err" | cut -c1-110)" || no "no fallback warning for the loop with several exits: $(head -c 200 "$T/err")"
if [ -x "$NAGA" ]; then
  for w in reduce atomics f32; do
    if "$NAGA" "$T/$w.wgsl" > "$T/naga" 2>&1; then ok "naga: $w.wgsl validation successful"; else no "naga rejects $w.wgsl: $(head -n 5 "$T/naga")"; fi
  done
  if "$NAGA" "$T/warp.wgsl" > "$T/naga" 2>&1; then ok "naga: warp.wgsl validation successful"; else echo "note naga does not take warp.wgsl (subgroups): $(head -n 2 "$T/naga" | tr '\n' ' ' | cut -c1-120)"; fi
else echo "note naga not found at $NAGA (scripts/fetch-webgpu-tools.sh naga): validation skipped"; fi
if command -v node > /dev/null 2>&1; then
  for w in reduce atomics f32 warp; do
    node examples/webgpu/js/validate.mjs "$T/$w.wgsl" > "$T/tint" 2>&1; code=$?
    if [ $code -eq 0 ]; then ok "Tint (Dawn): $w.wgsl has no errors"; elif [ $code -eq 3 ]; then echo "note the webgpu npm package or an adapter is missing: Tint validation skipped"; break
    elif [ $code -eq 4 ]; then echo "note the adapter lacks the subgroups feature: $w.wgsl was rejected by name ($(head -c 100 "$T/tint"))"
    else no "Tint rejects $w.wgsl: $(grep -v 'Warning: max' "$T/tint" | head -n 6)"; fi
  done
fi

# 1b. GPU-4: kernels that branch around a barrier in one function (compiler/tests/native/gpu/branch.fib). The WGSL printer structures the control flow
# (if/else, loop, break, continue: native.wgsl.reloop) so Tint sees the barrier in uniform control flow; the old loop-and-switch form of such a function is rejected.
if ptx $D/branch.fib "$T/branch.ptx" 2> "$T/err" && wgsl $D/branch.fib "$T/branch.wgsl" 2>> "$T/err"; then ok "branch.fib built for PTX and WGSL"; else no "branch.fib build: $(cat "$T/err")"; fi
[ -s "$T/err" ] && no "branch.fib printed a warning (a fallback to the state machine): $(head -c 200 "$T/err")" || ok "branch.fib: no fallback warning (every function structured)"
branch_text() { # branch_text BRANCH.wgsl: the branchy kernels hold their barriers and no state machine
  local r=0
  grep -qF 'workgroupBarrier()' "$1" || { echo "     no workgroupBarrier() in branch.wgsl"; r=1; }
  grep -q 'switch L {' "$1" && { echo "     branch.wgsl has a loop-and-switch state machine"; r=1; }
  [ "$(grep -c 'fn \(rev\|tree_sum\|parity\|early\)(' "$1")" -eq 4 ] || { echo "     a kernel of branch.fib is missing"; r=1; }
  return $r
}
if branch_text "$T/branch.wgsl"; then ok "WGSL: branch.fib has its four kernels, the barriers and no loop-and-switch state machine"; else no "branch.wgsl content"; fi
if [ -x "$NAGA" ]; then "$NAGA" "$T/branch.wgsl" > "$T/naga" 2>&1 && ok "naga: branch.wgsl validation successful" || no "naga rejects branch.wgsl: $(head -n 5 "$T/naga")"; fi
if command -v node > /dev/null 2>&1; then
  node examples/webgpu/js/validate.mjs "$T/branch.wgsl" > "$T/tint" 2>&1; code=$?
  if [ $code -eq 0 ]; then ok "Tint (Dawn): branch.wgsl has no errors (a barrier after thread-dependent branches)"; elif [ $code -eq 3 ]; then echo "note no WebGPU adapter: Tint check of branch.wgsl skipped"
  else no "Tint rejects branch.wgsl: $(grep -v 'Warning: max' "$T/tint" | head -n 6)"; fi
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
  cuda probe "$T/warp.ptx" > "$T/cuda.out" 2>&1; code=$?
  if [ $code -eq 0 ]; then ok "device (CUDA): $(head -c 150 "$T/cuda.out")"; else no "device (CUDA) warp probe: $(cat "$T/cuda.out")"; fi
  cuda warp "$T/warp.ptx" "$T/ref.txt" > "$T/cuda.out" 2>&1; code=$?
  if [ $code -eq 0 ] && [ "$(grep -c '^ok' "$T/cuda.out")" -ge 13 ]; then ok "device (CUDA): $(grep -c '^ok' "$T/cuda.out") warp-shuffle reduction checks equal fibber's CPU reference ($(grep 'warp partials' "$T/cuda.out" | head -n 1 | cut -c1-110))"; else no "device (CUDA) warp reductions: $(grep -v '^ok' "$T/cuda.out" | head -n 5)"; fi
  cuda f32atomics "$T/f32.ptx" > "$T/cuda.out" 2>&1; code=$?
  if [ $code -eq 0 ]; then ok "device (CUDA): $(grep -c '^ok' "$T/cuda.out") f32 atomics checks equal the CPU's: $(head -n 1 "$T/cuda.out" | cut -c1-120)"; else no "device (CUDA) f32 atomics: $(grep -v '^ok' "$T/cuda.out" | head -n 5)"; fi
  cuda branch "$T/branch.ptx" > "$T/cuda.out" 2>&1; code=$?
  if [ $code -eq 0 ] && [ "$(grep -c '^ok' "$T/cuda.out")" -eq 4 ]; then ok "device (CUDA): 4 branch-around-barrier checks equal the CPU's"; else no "device (CUDA) branch: $(grep -v '^ok' "$T/cuda.out" | head -n 5)"; fi
fi
wg() { node $D/wgsl_run.mjs "$@" 2>&1 | grep -v '^Warning'; }
wg badgrid "$T/reduce.wgsl" > "$T/wg.out"; code=${PIPESTATUS[0]}
if [ "$code" -eq 3 ]; then echo "note no WebGPU adapter or the webgpu npm package: the WGSL runs are skipped"; wg_on=0
else
  wg_on=1
  grep -q "^ok" "$T/wg.out" && ok "device (WebGPU): $(head -c 160 "$T/wg.out")" || no "device (WebGPU) badgrid: $(cat "$T/wg.out")"
  wg reduce "$T/reduce.wgsl" "$T/ref.txt" > "$T/wg.out"
  if [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -ge 25 ]; then ok "device (WebGPU): $(grep -c '^ok' "$T/wg.out") reduction checks equal fibber's CPU reference"; else no "device (WebGPU) reductions: $(grep -v '^ok' "$T/wg.out" | head -n 5)"; fi
  wg branch "$T/branch.wgsl" > "$T/wg.out"
  if [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -eq 4 ]; then ok "device (WebGPU): 4 branch-around-barrier checks equal the CPU's"; else no "device (WebGPU) branch: $(grep -v '^ok' "$T/wg.out" | head -n 5)"; fi
  wg atomics "$T/atomics.wgsl" > "$T/wg.out"
  if [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -ge 5 ]; then ok "device (WebGPU): $(grep -c '^ok' "$T/wg.out") atomics checks equal the CPU's"; else no "device (WebGPU) atomics: $(grep -v '^ok' "$T/wg.out" | head -n 5)"; fi
  wg f32atomics "$T/f32.wgsl" > "$T/wg.out"
  if [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -ge 2 ]; then ok "device (WebGPU): the f32 atomics, a compare-and-exchange loop, equal the CPU's: $(head -n 1 "$T/wg.out" | cut -c1-130)"; else no "device (WebGPU) f32 atomics: $(grep -v '^ok' "$T/wg.out" | head -n 5)"; fi
  wg probe "$T/warp.wgsl" > "$T/wg.out"; sub=${PIPESTATUS[0]}
  if [ "$sub" -eq 4 ]; then echo "note the adapter lacks the subgroups feature: the module was rejected by name ($(head -c 110 "$T/wg.out")); the warp runs are skipped"; sub_on=0
  else
    sub_on=1
    [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -ge 1 ] && ok "device (WebGPU): $(head -c 150 "$T/wg.out")" || no "device (WebGPU) warp probe: $(grep -v '^ok' "$T/wg.out" | head -n 5)"
    wg warp "$T/warp.wgsl" "$T/ref.txt" > "$T/wg.out"
    if [ "$(grep -c '^FAIL' "$T/wg.out")" -eq 0 ] && [ "$(grep -c '^ok' "$T/wg.out")" -ge 13 ]; then ok "device (WebGPU): $(grep -c '^ok' "$T/wg.out") warp-shuffle reduction checks equal fibber's CPU reference"; else no "device (WebGPU) warp reductions: $(grep -v '^ok' "$T/wg.out" | head -n 5)"; fi
  fi
fi
# a WGSL that requires subgroups is rejected BY NAME where the adapter lacks the feature (played by FIB_WEBGPU_NO_SUBGROUPS=1); a module that does not require them is not
if command -v node > /dev/null 2>&1; then
  FIB_WEBGPU_NO_SUBGROUPS=1 node $D/wgsl_run.mjs probe "$T/warp.wgsl" > "$T/rej.out" 2>&1; code=$?
  if [ $code -eq 3 ]; then echo "note no WebGPU adapter or the webgpu npm package: the rejection is not played"
  elif [ $code -eq 4 ] && grep -q "requires the WebGPU .subgroups. feature" "$T/rej.out"; then ok "an adapter without subgroups rejects the warp module by name: $(grep rejected "$T/rej.out" | head -c 120)"; else no "rejection without subgroups (exit $code): $(head -n 3 "$T/rej.out")"; fi
  FIB_WEBGPU_NO_SUBGROUPS=1 node $D/wgsl_run.mjs badgrid "$T/reduce.wgsl" > "$T/rej.out" 2>&1; code=$?
  if [ $code -ne 3 ]; then [ $code -ne 4 ] && ok "an adapter without subgroups still takes a module that does not require them (exit $code)" || no "a module without warp forms was rejected for subgroups"; fi
fi

# 4. planted faults
sed 's/atom.shared.max.s32/atom.shared.xxx.s32/' "$T/atomics.ptx" > "$T/p.ptx"
if ptx_checks "$T/reduce.ptx" "$T/p.ptx" > /dev/null; then no "planted: a PTX without atom.shared passed the text checks"; else ok "planted: a PTX without atom.shared fails the text checks"; fi
sed 's/atomicCompareExchangeWeak/atomicXxx/g' "$T/atomics.wgsl" > "$T/p.wgsl"
if wgsl_checks "$T/reduce.wgsl" "$T/p.wgsl" > /dev/null; then no "planted: a WGSL without compare-and-exchange passed the text checks"; else ok "planted: a WGSL without compare-and-exchange fails the text checks"; fi
sed '0,/workgroupBarrier();/s//workgroupBarrier(); switch L {/' "$T/branch.wgsl" > "$T/p.wgsl"
if branch_text "$T/p.wgsl" > /dev/null; then no "planted: a branch.wgsl with a state machine passed the text checks"; else ok "planted: a state machine in branch.wgsl fails the text checks"; fi
sed '0,/workgroupBarrier();/s//if (fibw_lid.x < 3u) { workgroupBarrier(); }/' "$T/branch.wgsl" > "$T/p.wgsl"
if command -v node > /dev/null 2>&1 && node examples/webgpu/js/validate.mjs "$T/branch.wgsl" > /dev/null 2>&1; then
  if node examples/webgpu/js/validate.mjs "$T/p.wgsl" > "$T/tint" 2>&1; then no "planted: a barrier under a thread-dependent if was accepted by Tint"; else grep -q "uniform control flow" "$T/tint" && ok "planted: a barrier under a thread-dependent if is rejected by Tint (uniform control flow)" || no "planted Tint: $(head -n 3 "$T/tint")"; fi
fi
if [ $cuda_on -eq 1 ]; then
  sed -E 's/bar\.sync[[:space:]]+0;//' "$T/branch.ptx" > "$T/p.ptx"
  if cuda branch "$T/p.ptx" > "$T/cuda.out" 2>&1; then no "planted: branch kernels without bar.sync passed the device comparison"; else grep -q "^FAIL rev" "$T/cuda.out" && ok "planted: branch kernels without bar.sync fail the device comparison (CUDA)" || no "planted branch PTX: $(head -n 3 "$T/cuda.out")"; fi
  sed 's/^ref sum-i32 1000003 7 32 .*/ref sum-i32 1000003 7 32 1/' "$T/ref.txt" > "$T/ref-bad.txt"
  if cuda reduce "$T/reduce.ptx" "$T/ref-bad.txt" > "$T/cuda.out" 2>&1; then no "planted: a wrong reference sum was accepted by the device comparison"; else grep -q "^FAIL sum-i32 n=1000003 grid=7" "$T/cuda.out" && ok "planted: a wrong reference value fails the device comparison (CUDA)" || no "planted reference: $(grep -v '^ok' "$T/cuda.out" | head -n 3)"; fi
  sed 's/atom.global.add.u32/atom.global.or.b32/' "$T/atomics.ptx" > "$T/p.ptx"
  if cuda atomics "$T/p.ptx" > "$T/cuda.out" 2>&1; then no "planted: an atomic or in place of the atomic add passed the histogram"; else grep -q "^FAIL hist" "$T/cuda.out" && ok "planted: an atomic or in place of the atomic add fails the histogram on the device (CUDA)" || no "planted histogram: $(grep -v '^ok' "$T/cuda.out" | head -n 3)"; fi
fi
# GPU-5: the text checks catch a lost directive; the device probes catch a butterfly that shuffles the wrong way and an f32 add that subtracts
sed 's/^enable subgroups;//' "$T/warp.wgsl" > "$T/p.wgsl"
if warp_wgsl_checks "$T/p.wgsl" > /dev/null; then no "planted: a WGSL without enable subgroups passed the text checks"; else ok "planted: a WGSL without enable subgroups fails the text checks"; fi
sed 's/, 1, 31, -1;/, 2, 31, -1;/' "$T/warp.ptx" > "$T/p.ptx"
if cmp -s "$T/warp.ptx" "$T/p.ptx"; then no "planted warp PTX: the plant did not apply"; elif [ $cuda_on -eq 1 ]; then
  cuda warp "$T/p.ptx" "$T/ref.txt" > "$T/cuda.out" 2>&1
  if grep -q "^FAIL warp" "$T/cuda.out"; then ok "planted: a butterfly whose last step has the wrong offset fails the warp reductions on the device (CUDA): $(grep '^FAIL' "$T/cuda.out" | head -n 1 | cut -c1-90)"; else no "planted warp reduction passed on the device: $(head -n 3 "$T/cuda.out")"; fi
  sed 's/shfl.sync.idx.b32/shfl.sync.up.b32/' "$T/warp.ptx" > "$T/p.ptx"
  cuda probe "$T/p.ptx" > "$T/cuda.out" 2>&1
  if grep -q "^FAIL probe" "$T/cuda.out"; then ok "planted: an index shuffle that shuffles up fails the probe on the device (CUDA)"; else no "planted probe passed on the device: $(head -n 3 "$T/cuda.out")"; fi
  sed 's/atom.global.add.f32/atom.global.max.s32/' "$T/f32.ptx" > "$T/p.ptx"
  cuda f32atomics "$T/p.ptx" > "$T/cuda.out" 2>&1
  if grep -q "^FAIL f32 atomic add" "$T/cuda.out"; then ok "planted: an integer max in place of the f32 atomic add fails on the device (CUDA)"; else no "planted f32 add passed on the device: $(head -n 3 "$T/cuda.out")"; fi
fi
if [ $wg_on -eq 1 ] && [ "${sub_on:-0}" -eq 1 ]; then
  sed 's/subgroupShuffleXor(p0, bitcast<u32>(p1))/subgroupShuffleXor(p0, bitcast<u32>(p1) ^ 1u)/' "$T/warp.wgsl" > "$T/p.wgsl"
  wg warp "$T/p.wgsl" "$T/ref.txt" > "$T/wg.out"
  if grep -q "^FAIL warp" "$T/wg.out"; then ok "planted: a butterfly with a wrong lane mask fails the warp reductions on the device (WebGPU)"; else no "planted WGSL warp reduction passed: $(head -n 3 "$T/wg.out")"; fi
  sed 's/subgroupBallot(/subgroupBallot(!/g' "$T/warp.wgsl" > "$T/p.wgsl"
  wg probe "$T/p.wgsl" > "$T/wg.out"
  if grep -q "^FAIL probe" "$T/wg.out"; then ok "planted: an inverted ballot fails the probe on the device (WebGPU)"; else no "planted WGSL probe passed: $(head -n 3 "$T/wg.out")"; fi
fi
if [ $wg_on -eq 1 ]; then
  sed 's/let nb = bitcast<u32>(a + v)/let nb = bitcast<u32>(a - v)/g' "$T/f32.wgsl" > "$T/p.wgsl"
  if cmp -s "$T/f32.wgsl" "$T/p.wgsl"; then no "planted f32 WGSL: the plant did not apply"; else
    wg f32atomics "$T/p.wgsl" > "$T/wg.out"
    if grep -q "^FAIL f32 atomic add" "$T/wg.out"; then ok "planted: an f32 atomic add that subtracts fails on the device (WebGPU)"; else no "planted WGSL f32 add passed: $(head -n 3 "$T/wg.out")"; fi
  fi
  sed 's/workgroupBarrier();//' "$T/branch.wgsl" > "$T/p.wgsl"
  wg branch "$T/p.wgsl" > "$T/wg.out"
  if grep -q "^FAIL rev" "$T/wg.out"; then ok "planted: branch kernels without workgroupBarrier fail the device comparison (WebGPU)"; else no "planted branch WGSL: $(head -n 3 "$T/wg.out")"; fi
  sed 's/atomicAdd(/atomicOr(/g' "$T/atomics.wgsl" > "$T/p.wgsl"
  wg atomics "$T/p.wgsl" > "$T/wg.out"
  if grep -q "^FAIL hist" "$T/wg.out"; then ok "planted: atomicOr in place of atomicAdd fails the histogram on the device (WebGPU)"; else no "planted WGSL histogram: $(head -n 3 "$T/wg.out")"; fi
fi

[ $bad -eq 0 ] && echo "gpu-device: every check holds" || echo "gpu-device: FAILED"
exit $bad
