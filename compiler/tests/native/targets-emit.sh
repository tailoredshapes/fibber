#!/bin/bash
# Code generation for the targets of the table (compiler/types/targets.fib, docs/design/targets.md) that this machine cannot run: wasm32-wasip1,
# wasm32-unknown-unknown and riscv64-unknown-linux-gnu. Nothing is linked or run here (no wasm-ld, WASI sysroot, wasmtime or riscv64 sysroot).
#   0. the table: `fibc targets` prints a row for each triple; the runtime's C functions of the table are exactly the `declare`s of rt/*.lir;
#   1. lowering and code generation succeed for the accepted lIR cases (cases/lir/{simd,instr,mapping}) through `lairf build --emit obj` and for fibber
#      programs (cases/ownership, the SIMD cases cases/stdlib/62xx, tensor kernels 7003 and 7007, the kernel targets/kernel.fib) through
#      `fibc build --target T --emit obj`; what does not lower is compared with targets-emit.expected (a new failure, or a fixed one, fails the script);
#   2. the object is what the row says (wasm: the magic \0asm and `file`'s WebAssembly; riscv64: ELF RISC-V, RVC, double-float ABI), the module's data
#      layout is the row's, and the pointer width of that layout is the row's pointer-bits; node (if present) validates the wasm object;
#   3. wasm32 tail calls are return_call (the row's +tail-call); the assembly of the kernel holds f64x2.mul, f64x2.add, f32x4.mul and f32x4.add (wasm32, SIMD128), fmadd.d (riscv64 RV64GC), and with a V CPU
#      (FIB_TARGET_CPU=spacemit-x60) vfmul.vv, vfadd.vv and a vector fused multiply-add; a <2 x double> fmuladd is one f64x2.mul on wasm32;
#   4. planted faults make the checks fail: a wrong triple (x86-64's assembly), a wrong data layout (riscv64's against the wasm row), a wrong pointer
#      width (wasm64), wasm32 without the row's features (no return_call), a wrong vector width (<4 x double> is two f64x2.mul), RV64GC without V (no vfmul), a corrupt wasm object, a runtime function
#      missing from the table, a nonsense triple. A check that passes with its fault planted is a failure of this script.
# usage: targets-emit.sh [--quick]   F=stage-2 fibc, L=lairf (FIBC, LAIRF also read). --quick: 12 lIR cases and 10 programs per triple.
# Exit: 0 every check holds and every planted fault is caught; 1 otherwise; 2 no tools. Run from anywhere.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
F=${F:-${FIBC:-fibc}}; L=${L:-${LAIRF:-lairf}}
command -v "$F" >/dev/null 2>&1 || [ -x "$F" ] || { echo "targets-emit: no fibc: set F" >&2; exit 2; }
command -v "$L" >/dev/null 2>&1 || [ -x "$L" ] || { echo "targets-emit: no lairf: set L" >&2; exit 2; }
export FIB_LIB=$root/lib
quick=; [ "${1:-}" = --quick ] && quick=1
T=$(mktemp -d "${TMPDIR:-/tmp}/targets-emit.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; bad=1; }
triples="wasm32-wasip1 wasm32-unknown-unknown riscv64-unknown-linux-gnu"
expected=compiler/tests/native/targets-emit.expected
kernel=compiler/tests/native/targets/kernel.fib

# 0. the table
"$F" targets > "$T/table" 2> "$T/err" || { no "fibc targets failed: $(head -c 200 "$T/err")"; exit 1; }
col() { awk -F'\t' -v t="$1" -v c="$2" 'NR==1 { for (i=1;i<=NF;i++) if ($i==c) k=i } NR>1 && $1==t { print $k }' "$T/table"; }
for t in $triples x86_64-unknown-linux-gnu aarch64-apple-darwin aarch64-unknown-linux-gnu arm64-apple-ios; do
  [ -n "$(col "$t" triple)" ] || no "the table has no row $t"
done
[ $bad -eq 0 ] && ok "the table has a row for each of the $(($(wc -l < "$T/table") - 1)) targets"
rt_decls() { grep -ho '^(declare [^ ]*' rt/*.lir | awk '{print $2}' | sort -u; }
table_fns() { sed -n '/(defun runtime-c-functions/,/^$/p' compiler/types/targets.fib | grep -o '"[^"]*"' | tr -d '"' | sort -u; }
if diff <(rt_decls) <(table_fns) > "$T/d"; then ok "runtime-c-functions is exactly the $(rt_decls | wc -l) declares of rt/*.lir"; else no "runtime-c-functions and rt/*.lir differ: $(tr '\n' ' ' < "$T/d")"; fi

# 1. lowering and code generation
if [ -n "$quick" ]; then lirs=$(grep -l '^;; expect: accept' cases/lir/simd/*.lir cases/lir/instr/*.lir | head -12)
else lirs=$(grep -l '^;; expect: accept' cases/lir/simd/*.lir cases/lir/instr/*.lir cases/lir/mapping/*.lir); fi
tensors="cases/stdlib/7003-tensor-linear-algebra-and-empty-inner-dimensions.fib cases/stdlib/7007-tensor-native-kernels-block-boundaries-and-tails.fib"
if [ -n "$quick" ]; then fibs="cases/ownership/01-return-part-of-argument.fib cases/ownership/02-structural-sharing.fib cases/ownership/41-top-level-macro-splices-forms.fib cases/stdlib/6201-f64-lanewise-arithmetic-agrees-with-the-scalar-operation.fib cases/stdlib/6200-f32-lanewise-arithmetic-agrees-with-the-scalar-operation.fib cases/stdlib/6216-splat-and-the-literal-operand-make-every-lane-the-scalar.fib cases/stdlib/6255-float-functions-agree-with-the-scalar-references.fib $tensors $kernel"
else fibs="$(grep -l -E '^;; expect: (accept|trap)' cases/ownership/*.fib | head -40) $(grep -l -E '^;; expect: (accept|trap)' cases/stdlib/62[0-5]*.fib | head -60) $tensors $kernel"; fi
: > "$T/failed"
for t in $triples; do
  n=0; f=0
  for lir in $lirs; do
    if "$L" build "$lir" --target "$t" -o "$T/x.o" -O 2 --emit obj 2> "$T/err"; then n=$((n+1)); else f=$((f+1)); echo "$t $lir" >> "$T/failed"; echo "  $t $lir: $(head -c 150 "$T/err" | tr '\n' ' ')"; fi
  done
  ok "$t: $n of $((n+f)) lIR cases lower and generate code"
  n=0; f=0
  for p in $fibs; do
    iargs=(); for r in $(sed -n 's/^;; roots: *//p' "$p" | head -1); do iargs+=(-I "$(dirname "$p")/$r"); done
    if "$F" build --target "$t" -I compiler -I lib "${iargs[@]}" "$p" -o "$T/p.o" --emit obj 2> "$T/err"; then n=$((n+1)); else f=$((f+1)); echo "$t $p" >> "$T/failed"; echo "  $t $p: $(head -c 150 "$T/err" | tr '\n' ' ')"; fi
  done
  ok "$t: $n of $((n+f)) fibber programs compile through fibc build --target --emit obj"
done
ran=" $(echo $lirs $fibs) "   # one line, single spaces
want_failed() { grep -v '^#' "$expected" | grep -v '^$' | while read -r t p; do case $ran in *" $p "*) echo "$t $p" ;; esac; done | sort; }
if diff <(want_failed) <(sort "$T/failed") > "$T/d"; then ok "what does not lower is exactly targets-emit.expected ($(wc -l < "$T/failed") rows)"
else no "what does not lower differs from targets-emit.expected (< expected, > now): $(tr '\n' ' ' < "$T/d")"; fi

# 2. object format, data layout, pointer width
magic() { head -c 4 "$1" | od -An -c | tr -d ' \n'; }
file_says() { case $1 in wasm32-*) echo 'WebAssembly (wasm) binary module' ;; riscv64-*) echo 'ELF 64-bit LSB relocatable, UCB RISC-V, RVC, double-float ABI' ;; esac; }
layout_of_ll() { grep -o 'target datalayout = "[^"]*"' "$1" | sed 's/.*= "//; s/"$//'; }
ptr_bits() { local p; p=$(tr '-' '\n' <<< "$1" | grep -m1 '^p:' | cut -d: -f2); echo "${p:-64}"; }
for t in $triples; do
  "$F" build --target "$t" -I compiler -I lib $kernel -o "$T/k-$t.o" --emit obj 2>/dev/null
  say=$(file "$T/k-$t.o" 2>/dev/null)
  case $say in *"$(file_says "$t")"*) ok "$t: object is $(file_says "$t")" ;; *) no "$t: object is [$say], want $(file_says "$t")" ;; esac
  case $t in wasm32-*) [ "$(magic "$T/k-$t.o")" = '\0asm' ] && ok "$t: the object starts with \\0asm" || no "$t: the object starts with [$(magic "$T/k-$t.o")]" ;; esac
  "$F" build --target "$t" -I compiler -I lib $kernel -o "$T/k-$t.ll" --emit llvm 2>/dev/null
  have=$(layout_of_ll "$T/k-$t.ll"); want=$(col "$t" data-layout)
  [ -n "$have" ] && [ "$have" = "$want" ] && ok "$t: data layout is the row's ($want)" || no "$t: data layout [$have], the row says [$want]"
  [ "$(ptr_bits "$have")" = "$(col "$t" pointer-bits)" ] && ok "$t: pointers are $(col "$t" pointer-bits) bits, as the row says" || no "$t: pointer width $(ptr_bits "$have"), the row says $(col "$t" pointer-bits)"
done
wasm_valid() { node -e 'process.exit(WebAssembly.validate(require("fs").readFileSync(process.argv[1])) ? 0 : 1)' "$1" 2>/dev/null; }
if command -v node > /dev/null; then
  for t in wasm32-wasip1 wasm32-unknown-unknown; do wasm_valid "$T/k-$t.o" && ok "$t: node validates the object (WebAssembly.validate)" || no "$t: node does not validate the object"; done
  node -e 'const m=new WebAssembly.Module(require("fs").readFileSync(process.argv[1])); console.log(WebAssembly.Module.imports(m).filter(i=>i.kind=="function").map(i=>i.name).join(" "))' "$T/k-wasm32-wasip1.o" > "$T/imports" 2>/dev/null
  lacks=$(col wasm32-wasip1 libc-lacks | tr ',' '\n'); miss=$(for i in $(cat "$T/imports"); do grep -qx "$i" <<< "$lacks" && echo "$i"; done | tr '\n' ' ')
  echo "  note wasm32-wasip1: the kernel imports $(wc -w < "$T/imports") functions; of them wasi-libc lacks (row): ${miss:-none}"
else echo "  skip node is not installed: the wasm objects are not validated"; fi

# 3. instructions
wasm_check() { local m; for m in 'f64x2\.mul' 'f64x2\.add' 'f32x4\.mul' 'f32x4\.add'; do grep -qE "$m" "$1" || { echo "no $m"; return 1; }; done; }
rv_check() { grep -qE 'fmadd\.d' "$1" || { echo "no fmadd.d"; return 1; }; }
rvv_check() { local m; for m in 'vfmul\.vv' 'vfadd\.vv' 'vf(madd|macc)\.v'; do grep -qE "$m" "$1" || { echo "no $m"; return 1; }; done; }
asm_of() { "$F" build --target "$1" -I compiler -I lib $kernel -o "$2" --emit asm 2>/dev/null; }
asm_of wasm32-wasip1 "$T/w.s" && { miss=$(wasm_check "$T/w.s") && ok "wasm32-wasip1: kernel assembly has f64x2.mul/add and f32x4.mul/add" || no "wasm32-wasip1 kernel: $miss"; } || no "wasm32-wasip1: the kernel did not compile"
asm_of riscv64-unknown-linux-gnu "$T/r.s" && { miss=$(rv_check "$T/r.s") && ok "riscv64 RV64GC: kernel assembly has fmadd.d" || no "riscv64 kernel: $miss"; } || no "riscv64: the kernel did not compile"
grep -qE 'attribute[[:space:]]+5, "rv64i2p1_m2p0_a2p1_f2p2_d2p2_c2p0' "$T/r.s" && ok "riscv64 RV64GC: the arch attribute is rv64imafdc" || no "riscv64: arch attribute is [$(grep -m1 -E 'attribute[[:space:]]+5' "$T/r.s")]"
tails=cases/ownership/02-structural-sharing.fib
"$F" build --target wasm32-wasip1 -I compiler -I lib $tails -o "$T/t.s" --emit asm 2>/dev/null
c=$(grep -c 'return_call' "$T/t.s"); [ "$c" -gt 0 ] && ok "wasm32-wasip1: $tails makes its tail calls return_call ($c), the tail-call feature of the row" || no "wasm32-wasip1: no return_call in $tails"
FIB_TARGET_CPU=spacemit-x60 asm_of riscv64-unknown-linux-gnu "$T/rv.s" && { miss=$(rvv_check "$T/rv.s") && ok "riscv64 with V (spacemit-x60): kernel assembly has vfmul.vv, vfadd.vv and a vector fma" || no "riscv64 V kernel: $miss"; } || no "riscv64 V: the kernel did not compile"
printf '(define (f <2 x double>) ((ptr p))\n  (block entry (ret (fmuladd (load (align 8) <2 x double> p) (load (align 8) <2 x double> (getelementptr double p (i64 2))) (load (align 8) <2 x double> (getelementptr double p (i64 4)))))))\n(define (main i32) () (block entry (ret (i32 0))))\n' > "$T/w2.lir"
sed 's/<2 x double>/<4 x double>/g; s/(i64 2)/(i64 4)/; s/(i64 4)))))))$/(i64 8)))))))/' "$T/w2.lir" > "$T/w4.lir"
mul_count() { "$L" build "$1" --target wasm32-wasip1 -o "$T/c.s" -O 2 --emit asm 2>/dev/null && grep -cE 'f64x2\.mul' "$T/c.s"; }
c2=$(mul_count "$T/w2.lir"); [ "$c2" = 1 ] && ok "wasm32: a 2 x double fmuladd is one f64x2.mul (one SIMD128 register)" || no "<2 x double> fmuladd gives [$c2] f64x2.mul, want 1"

# 4. planted faults
FIB_TARGET_CPU=x86-64 asm_of x86_64-unknown-linux-gnu "$T/x.s"
if ! wasm_check "$T/x.s" > /dev/null && ! rv_check "$T/x.s" > /dev/null; then ok "planted wrong triple (x86-64): the wasm and riscv checks fail, as they must"; else no "planted wrong triple was NOT caught"; fi
if [ "$(layout_of_ll "$T/k-riscv64-unknown-linux-gnu.ll")" != "$(col wasm32-wasip1 data-layout)" ]; then ok "planted wrong data layout (riscv64's for the wasm row): the layout check fails, as it must"; else no "planted wrong data layout was NOT caught"; fi
"$L" emit-llvm "$T/w2.lir" --target wasm64-unknown-unknown > "$T/w64.ll" 2>/dev/null
if [ -s "$T/w64.ll" ] && [ "$(ptr_bits "$(layout_of_ll "$T/w64.ll")")" != "$(col wasm32-wasip1 pointer-bits)" ]; then ok "planted wrong pointer width (wasm64, $(ptr_bits "$(layout_of_ll "$T/w64.ll")") bits): the pointer check fails, as it must"; else no "planted wrong pointer width was NOT caught"; fi
c4=$(mul_count "$T/w4.lir"); if [ "$c4" != 1 ]; then ok "planted wrong vector width (<4 x double>): $c4 f64x2.mul, the width check fails, as it must"; else no "planted wrong vector width was NOT caught"; fi
FIB_TARGET_CPU=generic "$F" build --target wasm32-wasip1 -I compiler -I lib $tails -o "$T/t0.s" --emit asm 2>/dev/null
c0=$(grep -c 'return_call' "$T/t0.s"); [ -s "$T/t0.s" ] && [ "$c0" = 0 ] && ok "planted wasm32 without the row's features (FIB_TARGET_CPU=generic): no return_call, the tail-call check fails, as it must" || no "planted missing tail-call feature was NOT caught ($c0)"
if ! rvv_check "$T/r.s" > /dev/null; then ok "planted RV64GC without V: the V check fails, as it must"; else no "the V check passed on RV64GC"; fi
if command -v node > /dev/null; then head -c 200 "$T/k-wasm32-wasip1.o" > "$T/cut.o"; if ! wasm_valid "$T/cut.o"; then ok "planted corrupt wasm object (cut at 200 bytes): node refuses it, as it must"; else no "a cut wasm object validated"; fi; fi
if ! diff <(rt_decls) <(table_fns | grep -vx madvise) > /dev/null; then ok "planted runtime function missing from the table (madvise): the table check fails, as it must"; else no "a missing runtime function was NOT caught"; fi
if "$L" build "$T/w2.lir" --target notanarch-unknown-wasi -o "$T/bad.o" --emit obj 2> "$T/err"; then no "a nonsense triple was accepted"; else ok "planted nonsense triple refused: $(head -c 100 "$T/err" | tr '\n' ' ')"; fi
[ $bad -eq 0 ] && echo "targets-emit: every check holds and every planted fault is caught"
exit $bad
