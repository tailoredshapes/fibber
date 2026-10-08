#!/bin/bash
# The mutants of the kernel-subset checker (compiler/own/kernel.fib; docs/design/gpu.md 3, ADR 0014): each rule is removed in a copy of the
# tree, a stage 2 is built from the copy, and the kernel cases of cases/stdlib (8530-8569; the atomics and reductions 8550-8555; 8578-8581 the f32 atomics and warp builtins on the host) are run with it: the mutant must FAIL at least one
# case (the one that plants the thing the rule refuses), or the rule is not tested. Every mutant builds a compiler (about two minutes each).
#   scripts/mutant-gpu.sh FIBC [--only NAME..]       FIBC: a stage 2 (the builder); NAME among the mutants below
# Exit 0 when every mutant is caught, 1 otherwise. Scratch under ~/.cache/fibber-scratch/mutant-gpu.
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
fibc=${1:?usage: mutant-gpu.sh FIBC [--only NAME..]}; shift
only=(); [ "${1:-}" = --only ] && { shift; only=("$@"); }
out=$HOME/.cache/fibber-scratch/mutant-gpu; mkdir -p "$out"
K=compiler/own/kernel.fib
# name|sed expression that removes one rule (name@FILE|sed: the rule is in FILE, a path under the tree, not compiler/own/kernel.fib)
mutants=(
  'no-string|s/((ELit (LitStr _)) (fail cx (. e pos) "a string literal: a kernel has no strings (a trap'"'"'s message is the exception)"))/((ELit (LitStr _)) ())/'
  'no-closure|s/((EFn _) (fail cx (. e pos) "a closure: a kernel has no function values (call a named function; it is checked too)"))/((EFn _) ())/'
  'no-simd|s/((ESimd _ _) (fail cx (. e pos) "a Simd value: a kernel is scalar code, the GPU supplies the lanes (docs\/design\/gpu.md 5.3)"))/((ESimd _ _) ())/'
  'no-builtin-list|s/(not (allowed-builtin? name)) (fail cx (. e pos)/(and false (not (allowed-builtin? name))) (fail cx (. e pos)/'
  'no-recursion|s/(some? (find-first (fn (x: i64) (= x f)) @(. cx stack)))/false/'
  'no-dyn|s/((EDyn _ _ _ _) (fail cx (. e pos) "dyn: a kernel has no dynamic dispatch"))/((EDyn _ _ _ _) ())/'
  'no-heap-type|s/(_ (some (str "heap data of type "/(_ (if true nil (some (str "heap data of type "/'
  'no-ctor|s/((EGlobal (GCtor t _)) (fail cx (. e pos) (str "a constructor of "/((EGlobal (GCtor t _)) (if true () (fail cx (. e pos) (str "a constructor of "/'
  'no-extern|s/((EGlobal (GExtern x)) (fail cx (. e pos) (str "a call of the extern "/((EGlobal (GExtern x)) (if true () (fail cx (. e pos) (str "a call of the extern "/'
  'no-object-cell|s/(some "a cell of an object: a kernel has cells of scalars only")/nil/'
  'atomic-add-is-sub@compiler/emit/lower/gpu.fib|s/(starts-with? rest "add-") (some "add")/(starts-with? rest "add-") (some "sub")/'
  'select-swapped@compiler/emit/lower/gpu.fib|s/(str-join \["(select " (v-text c) " " (v-text x) " " (v-text y) ")"\])/(str-join ["(select " (v-text c) " " (v-text y) " " (v-text x) ")"])/'
  'f32-max-is-min@compiler/emit/lower/gpu.fib|s/(= name "gpu\/atomic-max-f32") (some "fmax")/(= name "gpu\/atomic-max-f32") (some "fmin")/'
  'warp-size-on-host-is-32@compiler/emit/lower/gpu.fib|s/(if kt "(sreg warpsize)" "(i32 1)")/(if kt "(sreg warpsize)" "(i32 32)")/'
  'ballot-on-host-inverted@compiler/emit/lower/gpu.fib|s/(select " (v-text c) " (i32 1) (i32 0))/(select " (v-text c) " (i32 0) (i32 1))/'
  'shuffle-builtins-not-allowed|s/"gpu\/subgroup-size" "gpu\/lane-id" "gpu\/ballot"\])/"gpu\/lane-id" "gpu\/ballot"])/'
  'cas-swaps-expected-and-new@compiler/emit/lower/gpu.fib|s/(v-text p) " " (v-text x) " " (v-text n) ") 0)"/(v-text p) " " (v-text n) " " (v-text x) ") 0)"/'
)
bad=0; n=0
for m in "${mutants[@]}"; do
  name=${m%%|*}; expr=${m#*|}; K=compiler/own/kernel.fib
  case "$name" in *@*) K=${name#*@}; name=${name%%@*};; esac
  if [ ${#only[@]} -gt 0 ]; then skip=1; for o in "${only[@]}"; do [ "$o" = "$name" ] && skip=0; done; [ $skip = 1 ] && continue; fi
  n=$((n + 1)); d=$out/$name; rm -rf "$d"; mkdir -p "$d"
  cp -r "$root/compiler" "$root/lib" "$d/"
  sed -i "$expr" "$d/$K"
  if cmp -s "$root/$K" "$d/$K"; then echo "FAIL $name: the mutation did not apply (the rule's text moved: fix the sed in this script)"; bad=1; continue; fi
  # the mutant needs the heap-type rule's balance: a sed that unbalances parentheses shows as a build failure, which also counts as caught? No: a mutant that does not build tests nothing.
  if ! (cd "$d" && FIB_LIB=$d/lib "$fibc" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$d/F" > "$d/build.log" 2>&1); then
    echo "FAIL $name: the mutant does not build ($(tail -n 1 "$d/build.log"))"; bad=1; continue
  fi
  (cd "$root" && FIB_LIB=$root/lib "$d/F" cases cases/stdlib --only 853 854 855 857 858 -j 4 > "$d/cases.log" 2>&1)
  if grep -q " 0 fail," "$d/cases.log"; then echo "FAIL $name: every kernel case still passes without the rule (the rule is not tested)"; bad=1
  else echo "ok   $name: caught by $(grep -c 'FAIL' "$d/cases.log") case(s): $(grep 'FAIL' "$d/cases.log" | awk '{print $1}' | tr '\n' ' ')"; fi
done
[ $n -eq 0 ] && { echo "mutant-gpu: no mutant selected"; exit 1; }
[ $bad -eq 0 ] && echo "mutant-gpu: every mutant is caught ($n)" || echo "mutant-gpu: FAILED"
exit $bad
