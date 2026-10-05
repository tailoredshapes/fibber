#!/bin/bash
# Package A64-0: AArch64 code generation without an AArch64 machine (docs/design/aarch64.md 3). For aarch64-linux-gnu, aarch64-apple-darwin and arm64-apple-ios:
#   1. lowering and code generation succeed (lairf build --emit obj) for every accepted case of cases/lir/{simd,instr,mapping} and a sample of fibber programs
#      (cases/ownership, cases/stdlib/62xx: f64x2, f32x4, `<<..>>`, fma), through `fibc emit --target`;
#   2. the object is what the triple says (Mach-O arm64 for the two Apple triples, ELF aarch64 for Linux) and the module's data layout is LLVM's for it (emit-llvm);
#   3. the assembly of compiler/tests/native/a64/kernel.fib (apple-m1, +neon +fp-armv8) holds fmla on 2d and 4s, fadd on 2d and 4s, and fma.lir's <2 x double>
#      fmuladd is exactly one fmla (a 128-bit vector is one NEON register);
#   4. planted faults make the checks fail: a wrong triple (x86-64: no fmla), a wrong data layout (the string of another triple), a wrong vector width (<4 x double> is two fmla,
#      not one). A check that passes with its fault planted is a failure of this script.
# usage: a64-emit.sh [--quick]    F=stage-2 fibc, L=lairf (LAIRF/FIBC also read), LD_LIBRARY_PATH reaching libLLVM-21. --quick: 12 lIR and 8 fibber programs.
# Exit: 0 every check holds and every planted fault is caught; 1 otherwise; 2 no tools. Run from anywhere. Nothing here runs AArch64 code: see a64-cross-mac.sh.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}; L=${L:-${LAIRF:-lairf}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "a64-emit: no fibc: set F" >&2; exit 2; }
command -v "$L" >/dev/null 2>&1 || [ -x "$L" ] || { echo "a64-emit: no lairf: set L" >&2; exit 2; }
export FIB_LIB=$root/lib
quick=; [ "${1:-}" = --quick ] && quick=1
T=$(mktemp -d "${TMPDIR:-/tmp}/a64-emit.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; bad=1; }
triples="aarch64-unknown-linux-gnu aarch64-apple-darwin arm64-apple-ios"
layout_of() { case $1 in aarch64-unknown-linux-gnu) echo 'e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32' ;; *) echo 'e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32' ;; esac; }
file_says() { case $1 in aarch64-unknown-linux-gnu) echo 'ELF 64-bit LSB relocatable, ARM aarch64' ;; *) echo 'Mach-O 64-bit arm64 object' ;; esac; }

# 1. lowering succeeds
if [ -n "$quick" ]; then lirs=$(grep -l '^;; expect: accept' cases/lir/simd/*.lir cases/lir/instr/*.lir | head -12)
else lirs=$(grep -l '^;; expect: accept' cases/lir/simd/*.lir cases/lir/instr/*.lir cases/lir/mapping/*.lir); fi
if [ -n "$quick" ]; then fibs="cases/ownership/01-return-part-of-argument.fib cases/ownership/02-structural-sharing.fib cases/ownership/41-top-level-macro-splices-forms.fib cases/stdlib/6201-f64-lanewise-arithmetic-agrees-with-the-scalar-operation.fib cases/stdlib/6200-f32-lanewise-arithmetic-agrees-with-the-scalar-operation.fib cases/stdlib/6216-splat-and-the-literal-operand-make-every-lane-the-scalar.fib cases/stdlib/6255-float-functions-agree-with-the-scalar-references.fib compiler/tests/native/a64/kernel.fib"
else fibs="$(grep -l -E '^;; expect: (accept|trap)' cases/ownership/*.fib | head -40) $(grep -l -E '^;; expect: (accept|trap)' cases/stdlib/62[0-5]*.fib | head -60) compiler/tests/native/a64/kernel.fib"; fi
for t in $triples; do
  n=0; f=0
  for lir in $lirs; do
    if "$L" build "$lir" --target "$t" -o "$T/x.o" -O 2 --emit obj 2> "$T/err"; then n=$((n+1)); else f=$((f+1)); echo "  $t $lir: $(head -c 150 "$T/err" | tr '\n' ' ')"; fi
  done
  [ $f -eq 0 ] && ok "$t: lowering and code generation of $n lIR cases" || no "$t: $f of $((n+f)) lIR cases failed"
  n=0; f=0
  for p in $fibs; do
    iargs=(); for r in $(sed -n 's/^;; roots: *//p' "$p" | head -1); do iargs+=(-I "$(dirname "$p")/$r"); done
    if "$F" emit --target "$t" -I compiler -I lib "${iargs[@]}" "$p" > "$T/p.lir" 2> "$T/err" && "$L" build "$T/p.lir" --target "$t" -o "$T/p.o" -O 2 --emit obj 2>> "$T/err"; then n=$((n+1)); else f=$((f+1)); echo "  $t $p: $(head -c 150 "$T/err" | tr '\n' ' ')"; fi
  done
  [ $f -eq 0 ] && ok "$t: $n fibber programs compile through fibc emit --target and lairf" || no "$t: $f of $((n+f)) fibber programs failed"
  # 2. object format and data layout
  say=$(file "$T/x.o" 2>/dev/null)
  case $say in *"$(file_says "$t")"*) ok "$t: object is $(file_says "$t")" ;; *) no "$t: object is [$say], want $(file_says "$t")" ;; esac
  "$L" emit-llvm cases/lir/simd/fma.lir --target "$t" > "$T/fma.ll" 2>/dev/null
  want=$(layout_of "$t")
  if grep -qF "target datalayout = \"$want\"" "$T/fma.ll"; then ok "$t: data layout $want"; else no "$t: data layout is [$(grep -o 'target datalayout = "[^"]*"' "$T/fma.ll")], want $want"; fi
done

# 3. NEON instructions
mk_kernel_asm() { # TRIPLE OUT: the assembly of the fibber kernel for the triple
  "$F" emit --target "$1" -I compiler -I lib compiler/tests/native/a64/kernel.fib > "$T/k.lir" 2>/dev/null || return 1
  "$L" build "$T/k.lir" --target "$1" -o "$2" -O 2 --emit asm 2>/dev/null
}
neon_check() { # ASM: fmla, fadd on 2d and 4s; prints the first missing pattern
  local m
  for m in 'fmla(\.2d|[[:space:]]+v[0-9]+\.2d)' 'fmla(\.4s|[[:space:]]+v[0-9]+\.4s)' 'fadd(\.2d|[[:space:]]+v[0-9]+\.2d)' 'fadd(\.4s|[[:space:]]+v[0-9]+\.4s)'; do
    grep -qE "$m" "$1" || { echo "no $m"; return 1; }
  done
}
for t in $triples; do
  if mk_kernel_asm "$t" "$T/k-$t.s"; then
    miss=$(neon_check "$T/k-$t.s") && ok "$t: kernel assembly has fmla and fadd on .2d and .4s" || no "$t: kernel assembly: $miss"
  else no "$t: the kernel did not compile"; fi
done
printf '(target (cpu "apple-m1") (features "+neon,+fp-armv8"))\n(define (f <2 x double>) ((ptr p))\n  (block entry (ret (fmuladd (load (align 8) <2 x double> p) (load (align 8) <2 x double> (getelementptr double p (i64 2))) (load (align 8) <2 x double> (getelementptr double p (i64 4)))))))\n(define (main i32) () (block entry (ret (i32 0))))\n' > "$T/w2.lir"
sed 's/<2 x double>/<4 x double>/g; s/(i64 2)/(i64 4)/; s/(i64 4)))))))$/(i64 8)))))))/' "$T/w2.lir" > "$T/w4.lir"
fmla_count() { "$L" build "$1" --target "${2:-arm64-apple-darwin}" -o "$T/c.s" -O 2 --emit asm 2>/dev/null && grep -cE 'fmla' "$T/c.s"; }
c2=$(fmla_count "$T/w2.lir"); [ "$c2" = 1 ] && ok "a 2 x double fma with +neon,+fp-armv8 is one fmla" || no "<2 x double> fmuladd gives [$c2] fmla, want 1"

# 4. planted faults must be caught
x86() { "$F" emit --target x86_64-unknown-linux-gnu -I compiler -I lib compiler/tests/native/a64/kernel.fib > "$T/kx.lir" 2>/dev/null && FIB_TARGET_CPU=x86-64 "$L" build "$T/kx.lir" --target x86_64-unknown-linux-gnu -o "$T/kx.s" -O 2 --emit asm 2>/dev/null; }
if x86 && ! neon_check "$T/kx.s" > /dev/null; then ok "planted wrong triple (x86-64): the NEON check fails, as it must"; else no "planted wrong triple was NOT caught"; fi
"$L" emit-llvm cases/lir/simd/fma.lir --target x86_64-unknown-linux-gnu > "$T/x86.ll" 2>/dev/null
if ! grep -qF "target datalayout = \"$(layout_of arm64-apple-darwin)\"" "$T/x86.ll"; then ok "planted wrong data layout (x86-64's): the layout check fails, as it must"; else no "planted wrong data layout was NOT caught"; fi
c4=$(fmla_count "$T/w4.lir"); if [ "$c4" != 1 ]; then ok "planted wrong vector width (<4 x double>): $c4 fmla, the width check fails, as it must"; else no "planted wrong vector width was NOT caught"; fi
if "$L" build "$T/w2.lir" --target notanarch-unknown-linux -o "$T/bad.o" --emit obj 2> "$T/err"; then no "a nonsense triple was accepted"; else ok "planted nonsense triple refused: $(head -c 100 "$T/err" | tr '\n' ' ')"; fi
[ $bad -eq 0 ] && echo "a64-emit: every check holds and every planted fault is caught"
exit $bad
