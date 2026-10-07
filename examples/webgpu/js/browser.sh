#!/bin/bash
# examples/webgpu/js/browser.sh HOST.wasm KERNELS.wgsl [CHROMIUM]: the wasm host of the WebGPU kernels in Chromium headless (docs/design/webgpu.md 6).
# Serves browser.html, the glue, HOST.wasm and KERNELS.wgsl from a temporary directory over a local http server (python3), starts Chromium
# headless with WebGPU on and a DevTools port, and browser.mjs relays the page's console (what the module printed; the last line is `exit N`).
# Chromium's GPU process needs an X display even headless (ANGLE's Vulkan display is XCB): the run is under `xvfb-run` when there is one;
# the Vulkan device WebGPU computes on is the real GPU. Exit 0 when the module's run was 0, 1 otherwise, 2 when a tool is missing or the
# page did not finish in time. CHROMIUM defaults to $CHROMIUM, chromium, google-chrome, /snap/bin/chromium.
set -u
here=$(cd "$(dirname "$0")" && pwd)
wasm=${1:?HOST.wasm}; wgsl=${2:?KERNELS.wgsl}; chrome=${3:-${CHROMIUM:-}}
if [ -z "$chrome" ]; then for c in chromium google-chrome chromium-browser /snap/bin/chromium; do command -v "$c" >/dev/null 2>&1 && { chrome=$c; break; }; done; fi
[ -n "$chrome" ] || { echo "browser.sh: no chromium (set CHROMIUM)" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "browser.sh: no python3 for the http server" >&2; exit 2; }
command -v node >/dev/null 2>&1 || { echo "browser.sh: no node for the devtools client" >&2; exit 2; }
xvfb=; command -v xvfb-run >/dev/null 2>&1 && xvfb="xvfb-run -a"
T=$(mktemp -d "${TMPDIR:-/tmp}/fib-webgpu-browser.XXXXXX"); trap 'kill $pid $srv 2>/dev/null; sleep 0.5; rm -rf "$T"' EXIT
cp "$here/browser.html" "$here/fib-webgpu.mjs" "$T/"; cp "$wasm" "$T/host.wasm"; cp "$wgsl" "$T/kernels.wgsl"
port=$((20000 + RANDOM % 20000)); dev=$((port + 1))
(cd "$T" && python3 -m http.server "$port" --bind 127.0.0.1 > "$T/server.log" 2>&1) & srv=$!
sleep 1
$xvfb env ${FIB_VK_ICD:+VK_DRIVER_FILES=$FIB_VK_ICD VK_ICD_FILENAMES=$FIB_VK_ICD} "$chrome" --headless=new --no-sandbox --enable-unsafe-webgpu --enable-features=Vulkan,WebGPU --ignore-gpu-blocklist \
  --remote-debugging-port="$dev" --user-data-dir="$T/profile" --no-first-run "about:blank" > "$T/chrome.out" 2> "$T/chrome.err" &
pid=$!
node "$here/browser.mjs" "$dev" "http://127.0.0.1:$port/browser.html" "${BROWSER_TIMEOUT:-120}"
status=$?
[ $status -eq 2 ] && { echo "browser.sh: chromium's log follows"; grep -v "^$" "$T/chrome.err" | tail -n 10; }
exit $status
