#!/bin/bash
# scripts/mutant-lir2c.sh [MODE..]: planted faults in the C backend (docs/design/lir2c.md section 4), each killed by a test. For each mode a
# copy of compiler/c is deformed by one sed, lir2c is rebuilt from it (the copy shadows the tree through -I), and the backend's checks run:
# the differential harness over the cases that target the fault, or a textual check of the C. A mode is `killed` when a check fails,
# `SURVIVED` when none does (then the fault is real and the tests are too weak). Exit 1 when any survives. Without modes, all of them.
#   wrap-trap      the overflow flag of sadd/ssub/smul-overflow is always 0: wrapping where lIR traps
#   signed-shift   lshr shifts the signed value (an arithmetic shift)
#   strict-alias   the access typedefs lose may_alias: a load may be moved past a store of another type
#   musttail       the musttail attribute is dropped: a tail call is a plain call and the stack grows (judged at -O0, where the C compiler
#                  does no tail-call optimisation of its own; at -O2 gcc rescues these programs by its sibling-call pass and inlining)
#   atomic-order   seq_cst is written as relaxed
#   lane-order     shufflevector takes its operands in the other order
#   phi-copy       an edge assigns its phis one by one, without temporaries, and a use of a phi-bound name reads the phi variable itself (the
#                  lost-copy / swap problem: the printer's edges read the let-bound copies, so both halves are planted to show the copy matters)
#   fallthrough    an unconditional br emits no goto: the block falls through to the next one
#   align          the aligned(1) of an unaligned access is dropped (the access is written as an aligned one)
# Environment: FIBC (a stage 2), LAIRF, CC (default gcc), OUT (scratch, default $TMPDIR/lir2c-mutants).
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/.." && pwd)
: "${FIBC:?set FIBC to a stage 2}" "${LAIRF:?set LAIRF to a lairf binary}"
out=${OUT:-${TMPDIR:-/tmp}/lir2c-mutants}; mkdir -p "$out"
cc=${CC:-gcc}
modes=("$@"); [ ${#modes[@]} -eq 0 ] && modes=(wrap-trap signed-shift strict-alias musttail atomic-order lane-order phi-copy fallthrough align)
cases=$root/compiler/tests/c-backend/cases; jscases=$root/compiler/tests/js/cases
status=0
for mode in "${modes[@]}"; do
  d=$out/$mode; rm -rf "$d"; mkdir -p "$d/c"; cp "$root"/compiler/c/*.fib "$d/c/"
  targets=()
  case $mode in
    wrap-trap)    sed -i 's|(emit! cf (str dst ".f1 = (uint8_t)" bi "(" x ", " y ", \&" dst ".f0);"))|(emit! cf (str dst ".f0 = " (bin-c (match op ((OvSAdd) BAdd) ((OvSSub) BSub) ((OvSMul) BMul)) e x y) "; " dst ".f1 = 0;"))|' "$d/c/expr.fib"
                  targets=("$jscases/overflow-trap.lir" "$cases/arith-edges.lir") ;;
    signed-shift) sed -i 's|static inline T fib_lshr##N(T a, T b) { return (T)((U)a >> ((U)b \& M)); }|static inline T fib_lshr##N(T a, T b) { return (T)(a >> ((U)b \& M)); }|' "$d/c/prelude.fib"
                  targets=("$cases/arith-edges.lir") ;;
    strict-alias) sed -i 's|__attribute__((may_alias" (if unaligned ", aligned(1)" "") "))|__attribute__((" (if unaligned "aligned(1)" "") "))|' "$d/c/cx.fib"
                  targets=("$cases/alias-align.lir") ;;
    musttail)     sed -i 's|(cond must (emit! cf (str "__attribute__((musttail)) return " text ";"))|(cond (and must false) (emit! cf (str "__attribute__((musttail)) return " text ";")) must (emit! cf (str "return " text ";"))|' "$d/c/func.fib"
                  targets=("$cases/tail-lanes.lir" "$root/cases/lir/audit/t-cf.lir" "$root/cases/lir/audit/t-tail-many-tailcc.lir") ;;
    atomic-order) sed -i 's|((OSeqCst) "__ATOMIC_SEQ_CST")|((OSeqCst) "__ATOMIC_RELAXED")|' "$d/c/mem.fib"
                  targets=() ;;
    lane-order)   sed -i 's|(emit! cf (str dst " = __builtin_shufflevector(" a ", " b|(emit! cf (str dst " = __builtin_shufflevector(" b ", " a|' "$d/c/vec.fib"
                  targets=("$cases/tail-lanes.lir") ;;
    phi-copy)     sed -i 's|(if (<= (count vals) 1)|(if true|' "$d/c/func.fib"   # no temporaries: the copies of an edge run one after the other ..
                  sed -i 's|    ((KLocal n) (some (local-name n)))|    ((KLocal n) (some (match (get (. cf binds) n) ((some b) (match (. b kind) ((KPhi _ _) (phi-name (. b pos))) (_ (local-name n)))) (nil (local-name n)))))|' "$d/c/expr.fib"   # .. and read the phi variables themselves
                  targets=("$jscases/branches.lir") ;;
    fallthrough)  sed -i 's|((KBr l) (goto! cf from l))|((KBr l) (if (= (count (unwrap-or (get (. cf phis) l) \[\])) 0) () (goto! cf from l)))|' "$d/c/func.fib"
                  targets=("$cases/fallthrough.lir") ;;
    align)        sed -i 's|(if unaligned ", aligned(1)" "")|""|' "$d/c/cx.fib"
                  targets=("$cases/alias-align.lir") ;;
    *) echo "unknown mode $mode"; exit 2 ;;
  esac
  if cmp -s <(cat "$root"/compiler/c/*.fib) <(cat "$d"/c/*.fib); then echo "$mode: the sed changed nothing (the source moved: fix the script)"; status=1; continue; fi
  cp "$root/compiler/lir2c.fib" "$d/lir2c.fib"   # the tool beside the deformed c/: its own directory is searched first, so the copy shadows compiler/c
  if ! (cd "$root" && "$FIBC" build "$d/lir2c.fib" -I "$d" -I compiler -I lib -o "$d/lir2c") > "$d/build.log" 2>&1; then
    echo "$mode: killed at build time (the mutant does not compile)"; continue; fi
  if [ "$mode" = atomic-order ]; then
    n=$("$d/lir2c" emit "$root/cases/lir/instr/atomic.lir" 2>/dev/null | grep -c '__ATOMIC_SEQ_CST')
    if [ "$n" -eq 0 ]; then echo "$mode: killed (the C of instr/atomic.lir has no __ATOMIC_SEQ_CST)"; else echo "$mode: SURVIVED"; status=1; fi
    continue
  fi
  flags=""   # the musttail mutant is judged at -O0: at -O2 gcc's own sibling-call optimisation and inlining rescue these programs (docs/design/lir2c.md 4)
  [ "$mode" = musttail ] && flags="-std=gnu11 -O0 -march=native -ffp-contract=off -fno-math-errno -w"
  CFLAGS=$flags LIR2C=$d/lir2c "$root/compiler/tests/c-backend/diff.sh" -j 2 --cc "$cc" "${targets[@]}" > "$d/diff.log" 2>&1
  if grep -qE '^(differ|refused|cc-error|timeout) ' "$d/diff.log"; then
    echo "$mode: killed by $(grep -E '^(differ|refused|cc-error|timeout) ' "$d/diff.log" | awk '{print $2}' | tr '\n' ' ')"
  else echo "$mode: SURVIVED ($(tail -1 "$d/diff.log"))"; status=1; fi
done
exit $status
