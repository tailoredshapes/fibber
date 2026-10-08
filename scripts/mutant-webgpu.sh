#!/bin/bash
# scripts/mutant-webgpu.sh: the WebGPU emitter's tests must catch a broken emitter (docs/design/webgpu.md 8). Each mutant below is one edit of
# compiler/native/wgsl*.fib in a copy of the tree; a stage 2 is built from it with F and compiler/tests/native/wgsl-emit.sh is run against the
# copy: the mutant is caught when the script fails. No GPU is needed. Each mutant costs a build of stage 2 (a few minutes): run it after a
# change of the emitter, not in the gate (like the mutation reviews, it runs in the background).
#   F=path/to/fibc scripts/mutant-webgpu.sh [--only NAME..]     exit 0 when every mutant is caught, 1 otherwise
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
F=${F:-${FIBC:-fibc}}; [ -x "$F" ] || command -v "$F" > /dev/null 2>&1 || { echo "mutant-webgpu: no fibc: set F" >&2; exit 2; }
S=${MUTANT_SCRATCH:-$HOME/.cache/fibber-scratch/mutant-webgpu}; mkdir -p "$S"
only=(); while [ $# -gt 0 ]; do case $1 in --only) shift; while [ $# -gt 0 ]; do only+=("$1"); shift; done ;; *) shift ;; esac; done
bad=0
mutant() { # mutant NAME FILE SED-EXPR
  local name=$1 file=$2 expr=$3
  if [ ${#only[@]} -gt 0 ]; then local want=0; for o in "${only[@]}"; do [ "$o" = "$name" ] && want=1; done; [ $want = 1 ] || return; fi
  local d=$S/$name; rm -rf "$d"; mkdir -p "$d"
  (cd "$root" && cp -r compiler lib examples cases scripts "$d/")
  sed -i "$expr" "$d/$file"
  if (cd "$root" && diff -q "$file" "$d/$file" > /dev/null); then echo "FAIL $name: the mutation did not apply"; bad=1; return; fi
  if ! (cd "$d" && FIB_LIB=$d/lib "$F" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$d/F" > "$d/build.log" 2>&1); then
    echo "ok   $name: the mutant does not build ($(tail -n 1 "$d/build.log" | cut -c1-100))"; return
  fi
  if (cd "$d" && F=$d/F compiler/tests/native/wgsl-emit.sh > "$d/test.log" 2>&1); then echo "FAIL $name: the tests passed on the mutant"; bad=1
  else echo "ok   $name: caught: $(grep -m1 '^FAIL' "$d/test.log" | cut -c1-120)"; fi
  rm -rf "$d"
}
mutant binding-off-by-one   compiler/native/wgsl.fib       's/(str "@group(0) @binding(" (+ i 2) ")/(str "@group(0) @binding(" (+ i 3) ")/'
# An "i64 accepted" mutant of the type table alone is equivalent: an i64 kernel is refused by three independent gates (the type table, the
# literal printer, the arithmetic printer), so one edit leaves it refused and the tests rightly pass; the i64 refusal is tested by case 8571.
mutant barrier-fn-checks-trap compiler/native/wgsl/func.fib  's/(not (contains? (. (. wf wx) barriers) (. (. wf f) name)))/true/'
mutant overflow-unchecked   compiler/native/wgsl.fib       's/return OvI32(r, ((a ^ r) \& (b ^ r)) < i32(0));/return OvI32(r, false);/'
mutant workgroup-size-fixed compiler/native/wgsl.fib       's/@compute @workgroup_size(wg_x, wg_y, wg_z)/@compute @workgroup_size(64, 1, 1)/'
mutant trap-without-flag    compiler/native/wgsl.fib       's/fn fibw_trap() { fibw_trapped = true; atomicStore(&fibw_flag.flag, 1u); }/fn fibw_trap() { fibw_trapped = true; }/'
mutant index-space-swapped  compiler/native/wgsl/expr.fib  's/(= name "tid.x") "i32(fibw_lid.x)"/(= name "tid.x") "i32(fibw_wid.x)"/'
# GPU-5: the f32 atomic loop and the subgroup forms (cases 8578 and 8580 are accept cases; the golden kernels.wgsl must not gain a subgroup line)
mutant f32-atomic-refused   compiler/native/wgsl/atomic.fib 's/((RFAdd) (some "fibw_atomic_fadd"))/((RFAdd) nil)/'
mutant shuffle-down-is-up   compiler/native/wgsl/subgroup.fib 's/"shfl-down" "subgroupShuffleDown"/"shfl-down" "subgroupShuffleUp"/'
mutant subgroups-always-on  compiler/native/wgsl/subgroup.fib 's/(if on (str "enable subgroups;/(if true (str "enable subgroups;/'
mutant header-line-dropped  compiler/native/wgsl/subgroup.fib 's/(if on "\/\/ fib.requires: subgroups/(if false "\/\/ fib.requires: subgroups/'
[ $bad = 0 ] && echo "mutant-webgpu: every mutant is caught" || echo "mutant-webgpu: FAILED"
exit $bad
