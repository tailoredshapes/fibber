#!/bin/bash
# The WebGPU kernel target (docs/design/webgpu.md; compiler/types/targets.fib `row-wgsl-webgpu`; native.wgsl): what `fibc build --target
# wgsl-unknown-webgpu --emit wgsl` writes for examples/webgpu/kernels.fib, and what it refuses. No GPU is needed.
#   1. the table has the row, marked a kernel target, and `fibc build` accepts it without --allow-unsupported;
#   2. the WGSL holds the four kernels as `@compute @workgroup_size(wg_x, wg_y, wg_z)` entries with their `// fib.kernel-sig` signature lines, the
#      binding model (the uniform at 0, the flag at 1, buf0 at 2), the override constants, `fma(` for `simd/fma`, the checked `+` of vaddi as
#      `fibw_sadd_ovf(`, the trap as `atomicStore(&fibw_flag.flag, 1u)`, the index space as `fibw_lid`/`fibw_wid`, and NO i64, f64, u64 or `ptr<`
#      (a pointer is `Ptr`), NO runtime function (`fib_alloc`), NO libc name; it equals the golden compiler/tests/native/wgsl/kernels.wgsl
#      (--update rewrites the golden from this fibc: read the diff);
#   3. naga validates it (`naga FILE.wgsl`: the tool of scripts/fetch-webgpu-tools.sh, skipped with a note when absent: NAGA names it) and so does Tint, through Dawn
#      under node (examples/webgpu/js/validate.mjs; also skipped with a note);
#   4. the same file still runs for the host (`fibc run`: 0) and cpu.fib prints the three reference hashes;
#   5. the cases/stdlib/857x and 858x webgpu cases: each builds for the host (the case harness checks that) and its `;; webgpu-target = VERDICT | TEXT` line
#      holds for the WebGPU target (accept: the WGSL has `fib.kernel-sig k:`; reject: the refusal names the kernel and contains TEXT);
#   6. refusals: `--emit ptx`, `--emit llvm` and `--emit obj` on the WebGPU target; `--emit wgsl` on nvptx64 and for the host; an executable;
#   7. planted faults: a WGSL with a binding index off by one, one without the override workgroup size, one whose trap never sets the flag, one
#      whose checked + is a plain + must each fail check 2 (planted in a copy, the check function rerun on it).
# usage: wgsl-emit.sh [--update]   F=stage-2 fibc (FIBC also read); NAGA=path/to/naga. Exit 0 when every check holds and every fault is caught, 1 otherwise, 2 no fibc.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "wgsl-emit: no fibc: set F" >&2; exit 2; }
export FIB_LIB=$root/lib
NAGA=${NAGA:-$HOME/.cache/fibber-scratch/tools/naga/bin/naga}
T=$(mktemp -d "${TMPDIR:-/tmp}/wgsl-emit.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0; ok() { echo "ok   $*"; }; no() { echo "FAIL $*"; bad=1; }
K=examples/webgpu/kernels.fib; INC=(-I examples/webgpu); GOLD=compiler/tests/native/wgsl/kernels.wgsl
emit() { "$F" build "${INC[@]}" --target wgsl-unknown-webgpu "$1" --emit wgsl -o "$2"; }

# 1. the row
"$F" targets > "$T/rows"
grep -q "^wgsl-unknown-webgpu	wgsl	webgpu	.*kernel target" "$T/rows" && ok "table: wgsl-unknown-webgpu is a kernel target" || no "table: no webgpu row"
if emit "$K" "$T/k.wgsl" 2> "$T/err"; then ok "build --emit wgsl: accepted without --allow-unsupported"; else no "build --emit wgsl: $(cat "$T/err")"; fi

# 2. the WGSL
wgsl_checks() { # wgsl_checks FILE: the content checks; prints what is wrong, returns 1 on any
  local f=$1 r=0
  for want in '// fib.kernel-sig vadd: ptr ptr ptr i32' '// fib.kernel-sig vaddi: ptr ptr ptr i32' '// fib.kernel-sig gemm: ptr ptr ptr i32' '// fib.kernel-sig gemm_smem: ptr ptr ptr i32' '// fib.kernel-sig assert_positive: ptr i32' \
              '@compute @workgroup_size(wg_x, wg_y, wg_z)' 'fn vadd(' 'fn gemm(' 'fn assert_positive(' 'override wg_x: u32' \
              '@group(0) @binding(0) var<uniform> fibw_params' '@group(0) @binding(1) var<storage, read_write> fibw_flag' '@group(0) @binding(2) var<storage, read_write> buf0' \
              'fma(' 'fibw_sadd_ovf(' 'atomicStore(&fibw_flag.flag, 1u)' 'fibw_lid' 'fibw_wid' 'workgroupBarrier()' 'var<workgroup> fibw_sh0: array<u32, 1024>' 'fibw_sh1' 'case 16u: { return fibw_sh0'; do
    grep -q -- "$want" "$f" || { echo "     missing: $want"; r=1; }
  done
  for forbid in 'i64' 'f64' 'u64' 'ptr<' 'fib_alloc' 'malloc' 'printf'; do
    grep -q -- "$forbid" "$f" && { echo "     present: $forbid"; r=1; }
  done
  return $r
}
if wgsl_checks "$T/k.wgsl"; then ok "WGSL: four entries, the signatures, the binding model, the overrides, fma, the checked +, the trap, the index space; no i64/f64/pointer"; else no "WGSL content"; fi
if [ "${1:-}" = --update ]; then mkdir -p "$(dirname "$GOLD")"; cp "$T/k.wgsl" "$GOLD"; echo "updated $GOLD"; fi
if [ -f "$GOLD" ]; then
  if diff -q "$GOLD" "$T/k.wgsl" > /dev/null; then ok "golden: $GOLD is what this fibc writes"; else no "golden: differs from $GOLD (diff below; --update after an intended change)"; diff "$GOLD" "$T/k.wgsl" | head -n 20; fi
else no "golden: no $GOLD (run with --update once)"; fi

# 3. naga
if [ -x "$NAGA" ]; then
  if "$NAGA" "$T/k.wgsl" > "$T/naga" 2>&1; then ok "naga $("$NAGA" --version 2>/dev/null | head -n 1): validation successful"; else no "naga rejects the WGSL: $(head -n 5 "$T/naga")"; fi
else echo "note naga not found at $NAGA (scripts/fetch-webgpu-tools.sh naga): validation skipped"; fi
# 3b. Dawn's Tint (the strict one: its uniformity analysis refuses a barrier that naga accepts)
if command -v node > /dev/null 2>&1; then
  node examples/webgpu/js/validate.mjs "$T/k.wgsl" > "$T/tint" 2>&1; code=$?
  if [ $code -eq 0 ]; then ok "Tint (Dawn, the webgpu npm package): no errors"; elif [ $code -eq 3 ]; then echo "note the webgpu npm package or an adapter is missing (scripts/fetch-webgpu-tools.sh node): Tint validation skipped"
  else no "Tint rejects the WGSL: $(grep -v 'Warning: max' "$T/tint" | head -n 6)"; fi
else echo "note node not found: Tint validation skipped"; fi

# 4. the host still runs it
"$F" run "${INC[@]}" "$K" > "$T/run" 2>&1 && [ "$(cat "$T/run")" = 0 ] && ok "host: fibc run of the kernels file: 0 (the kernels are ordinary functions there)" || no "host run of the kernels: $(tail -n 3 "$T/run")"
"$F" run "${INC[@]}" examples/webgpu/cpu.fib > "$T/cpu" 2>&1 && grep -q "^gemm 256 " "$T/cpu" && ok "host: cpu.fib prints the three reference hashes" || no "cpu.fib: $(tail -n 3 "$T/cpu")"

# 5. the cases
for c in cases/stdlib/85[78][0-9]-webgpu-*.fib; do
  line=$(sed -n 's/^;; webgpu-target = \([a-z]*\) | \(.*\)$/\1\t\2/p' "$c" | head -n 1)
  verdict=${line%%	*}; text=${line#*	}
  if emit "$c" "$T/c.wgsl" 2> "$T/cerr"; then
    if [ "$verdict" = accept ] && grep -q -- "$text" "$T/c.wgsl"; then ok "case $(basename "$c"): accepted, the WGSL has '$text'"; else no "case $(basename "$c"): accepted but expected $verdict '$text'"; fi
  else
    if [ "$verdict" = reject ] && grep -q "kernel k" "$T/cerr" && grep -q -- "$text" "$T/cerr"; then ok "case $(basename "$c"): $(head -c 110 "$T/cerr")"; else no "case $(basename "$c"): expected $verdict '$text', got: $(cat "$T/cerr")"; fi
  fi
done

# 6. refusals
for kind in ptx llvm obj; do
  if "$F" build "${INC[@]}" --target wgsl-unknown-webgpu "$K" --emit $kind -o "$T/x" 2> "$T/err"; then no "--emit $kind on the WebGPU target was accepted"; else grep -q "emit wgsl" "$T/err" && ok "--emit $kind on the WebGPU target: refused" || no "--emit $kind text: $(cat "$T/err")"; fi
done
if "$F" build "${INC[@]}" --target nvptx64-nvidia-cuda "$K" --emit wgsl -o "$T/x" 2> "$T/err"; then no "--emit wgsl on nvptx64 was accepted"; else grep -q "wgsl-unknown-webgpu" "$T/err" && ok "--emit wgsl on nvptx64: refused" || no "nvptx wgsl text: $(cat "$T/err")"; fi
if "$F" build "${INC[@]}" "$K" --emit wgsl -o "$T/x" 2> "$T/err"; then no "--emit wgsl for the host was accepted"; else grep -q "WebGPU kernel target" "$T/err" && ok "--emit wgsl for the host: refused" || no "host wgsl text: $(cat "$T/err")"; fi
if "$F" build "${INC[@]}" --target wgsl-unknown-webgpu "$K" -o "$T/x" 2> "$T/err"; then no "an executable for the WebGPU target was linked"; else ok "an executable for the WebGPU target: refused"; fi

# 7. the planted faults
plant() { # plant NAME SED: the check must fail on the mutated copy
  sed "$2" "$T/k.wgsl" > "$T/planted.wgsl"
  if diff -q "$T/k.wgsl" "$T/planted.wgsl" > /dev/null; then no "planted $1: the plant did not apply"; return; fi
  if wgsl_checks "$T/planted.wgsl" > /dev/null; then no "planted $1: passed the WGSL checks"; else ok "planted $1: fails the WGSL checks"; fi
}
plant "binding index off by one" 's/@binding(2) var<storage, read_write> buf0/@binding(3) var<storage, read_write> buf0/'
plant "workgroup size not an override" 's/@workgroup_size(wg_x, wg_y, wg_z)/@workgroup_size(64, 1, 1)/'
plant "trap never sets the flag" 's/atomicStore(&fibw_flag.flag, 1u)/fibw_trapped = true/'
plant "checked + as a plain +" 's/fibw_sadd_ovf(\([^,]*\), \([^)]*\))/OvI32(\1 + \2, false)/'

[ $bad -eq 0 ] && echo "wgsl-emit: every check holds" || echo "wgsl-emit: FAILED"
exit $bad
