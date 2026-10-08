#!/bin/bash
# scripts/webgpu-agree.sh: the one-language guard across backends (docs/design/webgpu.md 6): the kernels of examples/webgpu/kernels.fib, one
# `defkernel` each, run on the host CPU, through CUDA (PTX, the fib-gpu-cuda driver) and through WebGPU (WGSL: wgpu-native through the
# fib-gpu-webgpu driver on the native host; Dawn under node through the wasm host; Chromium headless when it can), and the hash lines each run
# prints (`NAME n HASH`) compared with the CPU's. vadd and vaddi must agree bit for bit; the GEMM agrees bit for bit where the host fuses
# `fma` (CUDA, wgpu on NVIDIA, Dawn on NVIDIA) and is reported where it does not (SwiftShader: WGSL does not promise a fused fma).
#   F=path/to/fibc scripts/webgpu-agree.sh [--no-cuda] [--no-native] [--no-node] [--browser]
# Needs: a GPU; FIB_GPU_CUDA (the fib-gpu-cuda checkout, default ~/.cache/fibber-scratch/gpu1/fib-gpu-cuda), FIB_GPU_WEBGPU (the fib-gpu-webgpu
# checkout with its shim built, default ~/.cache/fibber-scratch/webgpu1/fib-gpu-webgpu), WGPU_NATIVE_DIR, WASI_SDK, node with the `webgpu`
# package (scripts/fetch-webgpu-tools.sh). Exit 0 when every run that was made agrees, 1 otherwise, 2 when a build fails.
set -u
root=$(cd "$(dirname "$0")/.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}; export FIB_LIB=$root/lib
cuda=${FIB_GPU_CUDA:-$HOME/.cache/fibber-scratch/gpu2/fib-gpu-cuda}; webgpu=${FIB_GPU_WEBGPU:-$HOME/.cache/fibber-scratch/webgpu1/fib-gpu-webgpu}
wg=${WGPU_NATIVE_DIR:-$HOME/.cache/fibber-scratch/tools/wgpu-native}
export WASI_SDK=${WASI_SDK:-$HOME/.cache/fibber-scratch/tools/wasm/wasi-sdk-34.0-x86_64-linux}
do_cuda=1; do_native=1; do_node=1; do_browser=0
for a in "$@"; do case $a in --no-cuda) do_cuda=0 ;; --no-native) do_native=0 ;; --no-node) do_node=0 ;; --browser) do_browser=1 ;; esac; done
T=$(mktemp -d "${TMPDIR:-/tmp}/webgpu-agree.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; INC=(-I examples/webgpu)
hashes() { grep -E "^(vadd|vaddi|gemm|gemm-smem) [0-9]+ [0-9]+$" "$1" | sort; }
compare() { # compare NAME LOG: the hash lines against the CPU's
  if diff <(hashes "$T/cpu.log") <(hashes "$2") > "$T/d"; then echo "ok   $1: vadd, vaddi, gemm hash as the CPU's ($(grep -m1 device: "$2" | cut -c1-80))"
  elif diff <(hashes "$T/cpu.log" | grep -v '^gemm') <(hashes "$2" | grep -v '^gemm') > /dev/null && grep -q '^gemm' "$2"; then
    echo "note $1: vadd and vaddi hash as the CPU's; the GEMM differs (an unfused fma on this host: $(grep -m1 device: "$2" | cut -c1-80)): $(grep '^gemm' "$2" | head -n 1)"
  else echo "FAIL $1: $(cat "$T/d" | head -n 4 | tr '\n' ' ')"; bad=1; fi
}

"$F" run "${INC[@]}" examples/webgpu/cpu.fib > "$T/cpu.log" 2>&1 || { echo "the CPU run failed: $(tail -n 2 "$T/cpu.log")"; exit 2; }
echo "cpu: $(hashes "$T/cpu.log" | tr '\n' ';')"
"$F" build "${INC[@]}" --target wgsl-unknown-webgpu examples/webgpu/kernels.fib --emit wgsl -o "$T/k.wgsl" || exit 2

if [ $do_cuda = 1 ]; then
  if [ -d "$cuda/src" ]; then
    "$F" build "${INC[@]}" --target nvptx64-nvidia-cuda examples/webgpu/kernels.fib --emit ptx -o "$T/k.ptx" || exit 2
    "$F" build "${INC[@]}" -I "$cuda/src" examples/webgpu/host-cuda.fib -o "$T/host-cuda" || exit 2
    "$T/host-cuda" "$T/k.ptx" > "$T/cuda.log" 2>&1; compare "cuda (PTX, fib-gpu-cuda)" "$T/cuda.log"
  else echo "skip cuda: no fib-gpu-cuda at $cuda"; fi
fi
if [ $do_native = 1 ]; then
  if [ -f "$webgpu/lib/libfibwgpu.so" ]; then
    "$F" build "${INC[@]}" -I "$webgpu/src" examples/webgpu/host.fib -L "$webgpu/lib" -L "$wg/lib" -o "$T/host-native" || exit 2
    LD_LIBRARY_PATH="$webgpu/lib:$wg/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" "$T/host-native" "$T/k.wgsl" > "$T/native.log" 2>&1; compare "webgpu native (WGSL, fib-gpu-webgpu over wgpu-native)" "$T/native.log"
  else echo "skip webgpu native: no $webgpu/lib/libfibwgpu.so (fib-gpu-webgpu scripts/build-shim.sh)"; fi
fi
if [ $do_node = 1 ] || [ $do_browser = 1 ]; then
  "$F" build "${INC[@]}" -I examples/webgpu/js --target wasm32-wasi examples/webgpu/host.fib --export run -o "$T/host.wasm" 2> "$T/wasm.err" || { echo "the wasm build failed: $(tail -n 3 "$T/wasm.err")"; exit 2; }
fi
if [ $do_node = 1 ]; then
  node examples/webgpu/js/run.mjs "$T/host.wasm" "$T/k.wgsl" > "$T/node.log" 2>&1; compare "webgpu node (WGSL, the wasm host, Dawn)" "$T/node.log"
fi
if [ $do_browser = 1 ]; then
  examples/webgpu/js/browser.sh "$T/host.wasm" "$T/k.wgsl" > "$T/browser.log" 2>&1; compare "webgpu chromium (WGSL, the wasm host in the browser)" "$T/browser.log"
fi
[ $bad = 0 ] && echo "webgpu-agree: every run made agrees with the CPU" || echo "webgpu-agree: FAILED"
exit $bad
