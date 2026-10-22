#!/bin/bash
# compiler/tests/shared/shared.sh: `fibc build --shared --header` (docs/design/shared-libraries.md), end to end on Linux x86-64:
#   refusals       a --shared build with no export, with --emit, a header for a symbol C cannot name, a parameter that does not cross: each refused, the reason in the message
#   kernels        kernels.fib (every kind of export: ints of each width, floats, bool, ptr+len, unit, str in, str out) built as libkernels.so with its header and module map;
#                  the header is valid C11 and C++; the library shows exactly the declared symbols (nm -D), has no text relocation and needs only libc/libm/libpthread;
#                  host.c (which includes the GENERATED header) prints what oracle.fib's main prints under `fibc run`, and checks truncation, the retry rule, a NULL buffer,
#                  an embedded NUL and that 400000 string calls do not grow the process
#   renderer       the fib-solid renderer (solid-lib.fib: render(text) -> JSON) as libsolid.so; a C host's JSON equals `fibc run` of fib-solid's demo/render-main.fib byte for byte,
#                  for three models, one an error and one whose JSON needs the retry
# The Mac (a .dylib, Swift through the module map) is shared-mac.sh, run by hand: the gate runs on Linux.
# usage: shared.sh STAGE2 [refusals|kernels|renderer]..      (from the repository root; SOLID=DIR names fib-solid, default ../fib-solid)
# linux-only-file: reads ELF (readelf, nm -D) and /proc/self/status; the Mac's check is shared-mac.sh
# Exit: 0 all held, 1 a check failed, 77 the renderer part could not run because fib-solid is missing (the status line then says SKIPPED, loudly).
set -uo pipefail
s2=${1:?usage: shared.sh STAGE2 [part..]}; shift
s2=$(cd "$(dirname "$s2")" && pwd)/$(basename "$s2")
root=$PWD
export FIB_LIB=${FIB_LIB:-$root/lib}
here=$root/compiler/tests/shared
SOLID=${SOLID:-$(cd "$root/.." && pwd)/fib-solid}
parts=${*:-refusals kernels renderer}
ulimit -v 12000000
scratch=${FIB_SCRATCH:-${TMPDIR:-$HOME/.cache/fibber-scratch}}; mkdir -p "$scratch"
t=$(mktemp -d "$scratch/shared.XXXXXX"); trap 'rm -rf "$t"' EXIT
fail=0; skipped=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fail=1; fi; }
has() { if grep -qF -- "$3" "$2"; then echo "ok    $1"; else echo "FAIL  $1: no [$3] in: $(head -c 400 "$2")"; fail=1; fi; }
hasnt() { if grep -qF -- "$3" "$2"; then echo "FAIL  $1: found [$3]"; fail=1; else echo "ok    $1"; fi; }
want() { if [ -e "$2" ]; then echo "ok    $1"; else echo "FAIL  $1: $2 does not exist"; fail=1; fi; }
part() { case " $parts " in *" $1 "*) return 0;; *) return 1;; esac; }

refusals() {
  echo "== refusals"
  local d=$t/ref; mkdir -p "$d"
  echo '(defun main () -> i64 7)' > "$d/seven.fib"
  "$s2" build "$d/seven.fib" --shared -o "$d/libseven.so" > "$d/o1" 2>&1; ck "--shared without an export: refused (exit 2)" $? 2
  has "--shared without an export: says why" "$d/o1" "--shared needs at least one export"
  [ ! -e "$d/libseven.so" ]; ck "--shared without an export: no file made" $? 0
  "$s2" build "$d/seven.fib" --shared --emit obj --export main -o "$d/x.o" > "$d/o2" 2>&1; ck "--shared with --emit: refused (exit 2)" $? 2
  has "--shared with --emit: says why" "$d/o2" "does not combine with --emit"
  "$s2" build "$d/seven.fib" --header "$d/x.h" -o "$d/x" > "$d/o3" 2>&1; ck "--header on an executable: refused (exit 2)" $? 2
  has "--header on an executable: says why" "$d/o3" "--header goes with --shared or --emit obj"
  printf '(defun sum-squares (n: i64) -> i64 (* n n))\n' > "$d/dash.fib"
  "$s2" build "$d/dash.fib" --shared --header "$d/dash.h" --export sum-squares -o "$d/libdash.so" > "$d/o4" 2>&1; ck "a symbol C cannot name with --header: refused (exit 2)" $? 2
  has "a symbol C cannot name: says to rename it" "$d/o4" "not a C identifier"
  has "a symbol C cannot name: suggests the rename" "$d/o4" "--export sum-squares=sum_squares"
  printf '(defun total (v: (Vec i64)) -> i64 0)\n' > "$d/vec.fib"
  "$s2" build "$d/vec.fib" --shared --export total -o "$d/libvec.so" > "$d/o5" 2>&1; ck "a Vec parameter: refused" $? 4
  has "a Vec parameter: names the parameter and what crosses" "$d/o5" "parameter v of total must be an integer"
  printf '(defun mk () -> (Vec i64) [1 2])\n' > "$d/vecr.fib"
  "$s2" build "$d/vecr.fib" --shared --export mk -o "$d/libvecr.so" > "$d/o6" 2>&1; ck "a Vec result: refused" $? 4
  has "a Vec result: says what crosses" "$d/o6" "the result of mk must be"
  "$s2" build "$d/seven.fib" --shared --export nosuch -o "$d/libno.so" > "$d/o7" 2>&1; ck "an export that is no function: refused" $? 4
  has "an export that is no function: names it" "$d/o7" "the program has no function nosuch"
}

kernels() {
  echo "== kernels"
  local d=$t/k; mkdir -p "$d"
  local ex=(--export add --export widths --export narrow --export scale --export half32 --export both --export dot --export sum-squares=sum_squares --export iota
            --export greet --export join2 --export count-bytes=count_bytes --export banner --export echo --export mixed --export empty-str=empty_str)
  "$s2" build "$here/kernels.fib" -I "$root/lib" --shared --header "$d/kernels.h" "${ex[@]}" -o "$d/libkernels.so" > "$d/build.out" 2>&1; ck "build --shared --header" $? 0
  [ -s "$d/build.out" ] && head -c 600 "$d/build.out"
  want "libkernels.so" "$d/libkernels.so"; want "kernels.h" "$d/kernels.h"; want "module.modulemap" "$d/module.modulemap"
  [ ! -e "$d/libkernels.so.lair.o" ] && [ ! -e "$d/libkernels.so.lair.exports" ]; ck "no object or list left beside the library" $? 0
  # the module map
  ck "module.modulemap" "$(cat "$d/module.modulemap" 2>/dev/null)" 'module kernels {
  header "kernels.h"
  export *
}'
  # the header: every prototype, byte for byte
  local protos=(
    'int64_t add(int64_t a, int64_t b);'
    'int64_t widths(int32_t a, int8_t b, int16_t c);'
    'int32_t narrow(int64_t n);'
    'double scale(double x, double k);'
    'float half32(float x);'
    'uint8_t both(uint8_t a, uint8_t b);'
    'double dot(void* xs, void* ys, int64_t n);'
    'int64_t sum_squares(int64_t n);'
    'void iota(void* buf, int64_t n);'
    'int64_t greet(const uint8_t* name, int64_t name_len, uint8_t* out, int64_t out_cap);'
    'int64_t join2(const uint8_t* a, int64_t a_len, const uint8_t* b, int64_t b_len, uint8_t* out, int64_t out_cap);'
    'int64_t count_bytes(const uint8_t* s, int64_t s_len);'
    'int64_t banner(uint8_t* out, int64_t out_cap);'
    'int64_t echo(const uint8_t* s, int64_t s_len, uint8_t* out, int64_t out_cap);'
    'int64_t mixed(int64_t n, const uint8_t* s, int64_t s_len, double k, uint8_t* out, int64_t out_cap);'
    'int64_t empty_str(uint8_t* out, int64_t out_cap);')
  local p; for p in "${protos[@]}"; do
    if grep -qxF -- "$p" "$d/kernels.h"; then echo "ok    header: $p"; else echo "FAIL  header lacks: $p"; fail=1; fi; done
  has "header: extern C guard" "$d/kernels.h" 'extern "C" {'
  has "header: include guard" "$d/kernels.h" '#ifndef FIB_KERNELS_H'
  has "header: names the renamed fibber function" "$d/kernels.h" '/* fibber: sum-squares */'
  cc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only -x c "$d/kernels.h" 2> "$d/cc.err"; ck "header is valid C11 (-Wall -Wextra -Werror -pedantic)" $? 0; [ -s "$d/cc.err" ] && head -c 400 "$d/cc.err"
  if command -v c++ > /dev/null; then c++ -std=c++17 -Wall -Wextra -Werror -fsyntax-only -x c++ "$d/kernels.h" 2> "$d/cxx.err"; ck "header is valid C++17" $? 0; fi
  # the library: exactly the declared symbols (and fib_status, fib_error, which every library has: docs/design/stack-check.md 6), position independent, light on dependencies
  local want_syms="add banner both count_bytes dot echo empty_str fib_error fib_status greet half32 iota join2 mixed narrow scale sum_squares widths"
  local got_syms; got_syms=$(nm -D --defined-only "$d/libkernels.so" | awk '{print $3}' | sort | tr '\n' ' ' | sed 's/ $//')
  ck "nm -D --defined-only: exactly the exports" "$got_syms" "$want_syms"
  ck "readelf: no text relocation" "$(readelf -d "$d/libkernels.so" | grep -c TEXTREL)" 0
  local needed; needed=$(readelf -d "$d/libkernels.so" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | sed 's/\.so.*//' | sort | tr '\n' ' ' | sed 's/ $//')
  case "$needed" in "libc"|"libc libm"|"libc libm libpthread"|"libc libpthread"|"libc libm libpthread"*) echo "ok    NEEDED: $needed";; *) echo "FAIL  NEEDED: [$needed]"; fail=1;; esac
  ck "readelf: SONAME" "$(readelf -d "$d/libkernels.so" | sed -n 's/.*(SONAME).*\[\(.*\)\]/\1/p')" libkernels.so
  # the host
  cc -O2 -Wall -Wextra -Werror -o "$d/host" "$here/host.c" -I"$d" -L"$d" -lkernels -Wl,-rpath,"$d" 2> "$d/hostcc.err"; ck "host.c compiles against the generated header" $? 0; [ -s "$d/hostcc.err" ] && head -c 600 "$d/hostcc.err"
  "$d/host" > "$d/host.out" 2> "$d/host.err"; ck "host.c: every check held (exit 0)" $? 0; [ -s "$d/host.err" ] && head -c 600 "$d/host.err"
  grep -v '^#' "$d/host.out" > "$d/host.lines"
  cat "$here/kernels.fib" "$here/oracle.fib" > "$d/ko.fib"
  "$s2" run -I "$root/lib" "$d/ko.fib" > "$d/oracle.out" 2> "$d/oracle.err"; ck "fibc run of the same functions" $? 0
  head -n -1 "$d/oracle.out" > "$d/oracle.lines"
  if cmp -s "$d/host.lines" "$d/oracle.lines"; then echo "ok    the C host printed what fibc run printed ($(wc -l < "$d/oracle.lines") lines)"; else echo "FAIL  host and fibc run differ:"; diff "$d/host.lines" "$d/oracle.lines" | head -n 10 || true; fail=1; fi
  grep '^#' "$d/host.out"
  # --emit obj with --header: the same header for a host that links the object itself
  "$s2" build "$here/kernels.fib" -I "$root/lib" --emit obj --header "$d/o/kernels.h" "${ex[@]}" -o "$d/kernels.o" > "$d/emit.out" 2>&1; ck "--emit obj --header into a missing directory is refused" $? 2
  mkdir -p "$d/o"
  "$s2" build "$here/kernels.fib" -I "$root/lib" --emit obj --header "$d/o/kernels.h" "${ex[@]}" -o "$d/kernels.o" > "$d/emit.out" 2>&1; ck "--emit obj --header" $? 0
  cmp -s "$d/kernels.h" "$d/o/kernels.h"; ck "the header is the same for --emit obj" $? 0
}

renderer() {
  echo "== renderer"
  if [ ! -d "$SOLID/src/solid" ] || [ ! -f "$SOLID/demo/render-main.fib" ]; then
    echo "SKIPPED  the renderer part: fib-solid is not at $SOLID (SOLID=DIR names it); NOTHING of the fib-solid library was checked"; skipped=1; return
  fi
  local d=$t/s; mkdir -p "$d"
  local t0=$SECONDS
  "$s2" build "$here/solid-lib.fib" -I "$root/lib" -I "$SOLID/src" --shared --header "$d/solid.h" --export render -o "$d/libsolid.so" > "$d/build.out" 2>&1; ck "build the renderer --shared --header" $? 0
  [ -s "$d/build.out" ] && head -c 600 "$d/build.out"
  echo "      (build $((SECONDS - t0)) s, $(wc -c < "$d/libsolid.so" 2>/dev/null) bytes)"
  has "solid.h: the render prototype" "$d/solid.h" 'int64_t render(const uint8_t* text, int64_t text_len, uint8_t* out, int64_t out_cap);'
  ck "nm -D --defined-only: render and the two every library has" "$(nm -D --defined-only "$d/libsolid.so" | awk '{print $3}' | sort | tr '\n' ' ')" "fib_error fib_status render "
  cc -O2 -Wall -Wextra -Werror -o "$d/solid-host" "$here/solid-host.c" -I"$d" -L"$d" -lsolid -Wl,-rpath,"$d" 2> "$d/cc.err"; ck "solid-host.c compiles against the generated header" $? 0; [ -s "$d/cc.err" ] && head -c 600 "$d/cc.err"
  local i=0 m
  for m in '(cube 1 2 3)' '(difference (cube 10 10 10) (sphere 6))' '(cube 1 2'; do
    i=$((i + 1))
    "$d/solid-host" "$m" > "$d/host$i.json" 2> "$d/host$i.err"; ck "model $i: the C host ran ($m)" $? 0
    "$s2" run -I "$root/lib" -I "$SOLID/src" "$SOLID/demo/render-main.fib" -- "$m" > "$d/ref$i.out" 2> "$d/ref$i.err"; ck "model $i: fibc run of render-main.fib" $? 0
    ck "model $i: fibc run ends with main's result" "$(tail -n 1 "$d/ref$i.out")" 0; head -n -1 "$d/ref$i.out" > "$d/ref$i.json"
    if cmp -s "$d/host$i.json" "$d/ref$i.json"; then echo "ok    model $i: the JSON is byte-identical ($(wc -c < "$d/ref$i.json") bytes; $(grep '^#' "$d/host$i.err"))"; else echo "FAIL  model $i: the JSON differs"; cmp "$d/host$i.json" "$d/ref$i.json" | head -n 3 || true; fail=1; fi
  done
  [ "$(wc -c < "$d/ref2.json")" -gt 200 ]; ck "model 2 is longer than the host's first buffer, so the retry rule ran" $? 0
  has "model 2 needed two calls" "$d/host2.err" "in 2 call(s)"
  has "model 3 is an error answer" "$d/ref3.json" '"ok":false'
}

for p in $parts; do case "$p" in refusals|kernels|renderer) "$p";; *) echo "unknown part $p"; fail=1;; esac; done
if [ $fail != 0 ]; then echo "shared: FAILED"; exit 1; fi
if [ $skipped != 0 ]; then echo "shared: SKIPPED (the renderer part did not run: fib-solid is missing; the rest held)"; exit 77; fi
echo "shared: ok"
