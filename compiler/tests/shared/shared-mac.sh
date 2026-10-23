#!/bin/bash
# compiler/tests/shared/shared-mac.sh: the shared-library check on an Apple Silicon Mac, RUN BY HAND (the gate runs on Linux and does not run this; mac-check does not either:
# compiler/tests/expected-macos.txt). A stage 2 built on the Mac from this tree (scripts/mac-check.sh --with-f) builds kernels.fib as libkernels.dylib with
# --shared --header; a Swift tool that `import kernels` (the generated module.modulemap) calls every export and must print what `fibc run` prints for oracle.fib;
# the dylib shows exactly the exports (nm -gU, with the leading underscore) and its install name is @rpath/libkernels.dylib. With SOLID=DIR (fib-solid) the renderer is built
# as libsolid.dylib too, and a Swift tool's JSON for three models must equal `fibc run` of fib-solid's demo/render-main.fib, byte for byte.
# usage: compiler/tests/shared/shared-mac.sh FIBC        (from the repository root; writes under $MAC_SCRATCH, default $HOME/fibber-a64-scratch/shared; installs nothing)
set -uo pipefail
fibc=${1:?usage: shared-mac.sh FIBC}; fibc=$(cd "$(dirname "$fibc")" && pwd)/$(basename "$fibc")
root=$PWD; here=$root/compiler/tests/shared
export FIB_LIB=${FIB_LIB:-$root/lib}
scratch=${MAC_SCRATCH:-$HOME/fibber-a64-scratch/shared}; rm -rf "$scratch"; mkdir -p "$scratch"
llvm=${MAC_LLVM:-/opt/homebrew/opt/llvm@21}; export DYLD_FALLBACK_LIBRARY_PATH=$llvm/lib
fail=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fail=1; fi; }
[ "$(uname -m)" = arm64 ] || { echo "not arm64"; exit 2; }
d=$scratch/k; mkdir -p "$d"
ex=(--export add --export widths --export narrow --export scale --export half32 --export both --export dot --export sum-squares=sum_squares --export iota
    --export greet --export join2 --export count-bytes=count_bytes --export banner --export echo --export mixed --export empty-str=empty_str)
"$fibc" build "$here/kernels.fib" -I "$root/lib" --shared --header "$d/kernels.h" "${ex[@]}" -o "$d/libkernels.dylib" 2> "$d/build.err"; ck "build libkernels.dylib --shared --header" $? 0
[ -s "$d/build.err" ] && { echo "--- build stderr:"; head -c 800 "$d/build.err"; }
ck "install name" "$(otool -D "$d/libkernels.dylib" | tail -n 1)" "@rpath/libkernels.dylib"
ck "nm -gU: exactly the exports" "$(nm -gU "$d/libkernels.dylib" | awk '{print $3}' | sort | tr '\n' ' ')" \
   "_add _banner _both _count_bytes _dot _echo _empty_str _fib_error _fib_status _greet _half32 _iota _join2 _mixed _narrow _scale _sum_squares _widths "
ck "module.modulemap" "$(cat "$d/module.modulemap")" 'module kernels {
  header "kernels.h"
  export *
}'
cc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only -x c "$d/kernels.h"; ck "header is valid C11" $? 0
swiftc -O -I "$d" -L "$d" -lkernels -Xlinker -rpath -Xlinker "$d" "$here/main.swift" -o "$d/tool" 2> "$d/swiftc.err"; ck "swiftc: import kernels through the module map" $? 0
[ -s "$d/swiftc.err" ] && head -c 800 "$d/swiftc.err"
"$d/tool" > "$d/tool.out" 2> "$d/tool.err"; ck "the Swift tool's own checks (exit 0)" $? 0; [ -s "$d/tool.err" ] && head -c 600 "$d/tool.err"
cat "$here/kernels.fib" "$here/oracle.fib" > "$d/ko.fib"
"$fibc" run -I "$root/lib" "$d/ko.fib" > "$d/oracle.out" 2>/dev/null; ck "fibc run of the same functions" $? 0
sed '$d' "$d/oracle.out" > "$d/oracle.lines"
if cmp -s "$d/tool.out" "$d/oracle.lines"; then echo "ok    the Swift tool printed what fibc run printed ($(wc -l < "$d/oracle.lines" | tr -d ' ') lines)"; else echo "FAIL  Swift and fibc run differ:"; diff "$d/tool.out" "$d/oracle.lines" | head; fail=1; fi
if [ -n "${SOLID:-}" ]; then
  s=$scratch/s; mkdir -p "$s"
  "$fibc" build "$here/solid-lib.fib" -I "$root/lib" -I "$SOLID/src" --shared --header "$s/solid.h" --export render -o "$s/libsolid.dylib" 2> "$s/build.err"; ck "build libsolid.dylib" $? 0
  ck "nm -gU: render and the two every library has" "$(nm -gU "$s/libsolid.dylib" | awk '{print $3}' | tr '\n' ' ')" "_fib_error _fib_status _render "
  swiftc -O -I "$s" -L "$s" -lsolid -Xlinker -rpath -Xlinker "$s" "$here/solid-main.swift" -o "$s/tool" 2> "$s/swiftc.err"; ck "swiftc: import solid" $? 0; [ -s "$s/swiftc.err" ] && head -c 800 "$s/swiftc.err"
  models=('(cube 1 2 3)' '(difference (cube 10 10 10) (sphere 6))' '(cube 1 2')
  "$s/tool" "${models[@]}" > "$s/tool.out" 2> "$s/tool.err"; ck "the Swift tool ran" $? 0
  : > "$s/ref.out"
  for m in "${models[@]}"; do "$fibc" run -I "$root/lib" -I "$SOLID/src" "$SOLID/demo/render-main.fib" -- "$m" 2>/dev/null | sed '$d' >> "$s/ref.out"; done
  if cmp -s "$s/tool.out" "$s/ref.out"; then echo "ok    the JSON of 3 models is byte-identical to fibc run ($(wc -c < "$s/ref.out" | tr -d ' ') bytes)"; else echo "FAIL  the JSON differs"; cmp "$s/tool.out" "$s/ref.out"; fail=1; fi
else echo "      (SOLID=DIR not set: the renderer part was not run)"; fi
[ $fail = 0 ] && echo "shared-mac: ok" || echo "shared-mac: FAILED"
exit $fail
