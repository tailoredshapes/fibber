#!/bin/bash
# Package P0: the module's `target` form reaches the code generator (spec/lir.md 4.5). The same function, `fmuladd` and an 8-lane add, is built to assembly
# under different target forms and the instructions are counted: the checks that the cases of cases/lir/simd (which see the IR text and run the program) cannot make.
#   with (cpu "x86-64-v3") (features "+avx2,+fma"): vfmadd and ymm registers appear
#   with (cpu "x86-64"): no vfmadd, no ymm
#   with (cpu "x86-64-v3") (features "-fma"): no vfmadd, ymm still
#   a module target wins over FIB_TARGET_CPU in both directions, and without a target form FIB_TARGET_CPU rules as before
# usage: p0-target.sh        (LAIRF names lairf, default `lairf`; run from anywhere; LD_LIBRARY_PATH must reach libLLVM-21.so if it is not in the default path)
# Exit: 0 every check holds; 1 one fails; 2 no lairf.
set -u
lairf=${LAIRF:-lairf}
command -v "$lairf" >/dev/null 2>&1 || [ -x "$lairf" ] || { echo "p0-target: no lairf: set LAIRF" >&2; exit 2; }
T=$(mktemp -d "${TMPDIR:-/tmp}/p0-target.XXXXXX"); trap 'rm -rf "$T"' EXIT
body='(define (f <8 x double>) ((ptr p))
  (block entry
    (let ((a (load (align 8) <8 x double> p))
          (b (load (align 8) <8 x double> (getelementptr double p (i64 8))))
          (c (load (align 8) <8 x double> (getelementptr double p (i64 16)))))
      (ret (fadd (fmuladd a b c) a)))))
(define (main i32) () (block entry (ret (i32 0))))'
bad=0
asm() { # NAME TARGETFORM [ENV..]: the assembly of the module under the target form into $T/NAME.s
  local name=$1 target=$2; shift 2
  printf '%s\n%s\n' "$target" "$body" > "$T/$name.lir"
  env "$@" "$lairf" build "$T/$name.lir" -o "$T/$name.s" -O 2 --emit asm 2> "$T/$name.err" || { echo "FAIL $name: lairf build: $(head -c 200 "$T/$name.err")"; bad=1; return 1; }
}
count() { grep -c "$2" "$T/$1.s"; }
check() { # NAME WHAT PATTERN WANT(yes|no)
  local n; n=$(count "$1" "$3")
  if { [ "$4" = yes ] && [ "$n" -gt 0 ]; } || { [ "$4" = no ] && [ "$n" -eq 0 ]; }; then echo "ok   $1: $2 ($3: $n)"; else echo "FAIL $1: $2 ($3: $n)"; bad=1; fi
}
asm v3 '(target (cpu "x86-64-v3") (features "+avx2,+fma"))' FOO=1 && { check v3 "fma instruction" vfmadd yes; check v3 "256-bit registers" ymm yes; }
asm base '(target (cpu "x86-64"))' FOO=1 && { check base "no fma instruction" vfmadd no; check base "no 256-bit registers" ymm no; check base "separate multiply" mulpd yes; }
asm nofma '(target (cpu "x86-64-v3") (features "-fma"))' FOO=1 && { check nofma "fma switched off" vfmadd no; check nofma "256-bit registers stay" ymm yes; }
asm win-base '(target (cpu "x86-64"))' FIB_TARGET_CPU=x86-64-v3 && check win-base "the module's CPU beats FIB_TARGET_CPU=x86-64-v3" vfmadd no
asm win-v3 '(target (cpu "x86-64-v3") (features "+avx2,+fma"))' FIB_TARGET_CPU=x86-64 && check win-v3 "the module's CPU beats FIB_TARGET_CPU=x86-64" vfmadd yes
asm env-base '' FIB_TARGET_CPU=x86-64 && check env-base "no target form: FIB_TARGET_CPU=x86-64 rules" vfmadd no
[ $bad -eq 0 ] && echo "p0-target: every check holds"
exit $bad
