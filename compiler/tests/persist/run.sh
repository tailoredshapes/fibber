#!/bin/bash
# compiler/tests/persist/run.sh [FIBC [PART]]: the controls of the persistent checker (docs/design/persistent-checker.md), with a stage 2 (FIBC). Offline, about 2 minutes.
#  1. each program of this directory through `fibc emit` three ways (FIB_CHECK_CACHE unset, on, verify): the same lIR or the same rejection, the same status;
#     the programs named reject-* are rejected (status 3), the others accepted.
#  2. the same programs and the first inputs of the emit-fns golden through `emit --batch` (one process, the library kept) with FIB_CHECK_CACHE=verify, which
#     checks each input both ways: no input ends in `#### CHECK-CACHE MISMATCH`; and the blocks equal those of one process for each input.
#  3. `fibc serve`: after an edit of the prelude, and of a module of the implicit library, a program that used them is checked again with the edit (the kept
#     state is not used for another library), and once the edit is reverted it checks again; and a program of two modules whose first is edited (its
#     kept state, own.chain, is not used for another text): rejected after the edit, accepted once it is undone.
#  4. the program-prefix memo (own.chain, docs/design/live-repl.md package LV4): the compiler's syntax/ modules (about 2,000 lines) as the app with
#     app/main.fib as the last module, through three servers (FIB_CHECK_CACHE on, off, verify) over one transcript of edits (a one-function swap in the
#     last module three times, an edit of a middle module, a swap again): every answer of `verify` holds; the lIR of each step is the same under the three,
#     and the last step's equals `fibc emit`'s; the swap's check under `on` is under 80 ms (median of the three swaps), printed against `off`.
# PART (default: all) runs one independent share, so make runs the shares side by side (mk/tools.mk, PERSIST_PARTS):
#   cmd-0 cmd-1 cmd-2  part 1 for every third program (by position in the sorted list);  batch-verify  the verify run of part 2;
#   batch-onoff  the batch with the kept library against the batch without it;  batch-single  the batch against one process for each input;  server  part 3;
#   prefix  part 4.
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${1:-${FIBC:-fibc}}
part=${2:-all}
want() { [ "$part" = all ] || [ "$part" = "$1" ]; }
out=${PERSIST_OUT:-$(mktemp -d "${TMPDIR:-/tmp}/persist.XXXXXX")}
mkdir -p "$out"
cd "$root" || exit 2
export FIB_LIB=$root/lib
fail=0
ok() { echo "ok $1"; }
bad() { echo "FAIL $1"; fail=1; }

# ---- 1. the command line
idx=-1
for f in "$here"/*.fib; do
  idx=$((idx + 1))
  want "cmd-$((idx % 3))" || continue
  n=$(basename "$f" .fib)
  env -u FIB_CHECK_CACHE "$fibc" emit "$f" > "$out/$n.off" 2> "$out/$n.off.err"; s0=$?
  FIB_CHECK_CACHE=on "$fibc" emit "$f" > "$out/$n.on" 2> "$out/$n.on.err"; s1=$?
  FIB_CHECK_CACHE=verify "$fibc" emit "$f" > "$out/$n.ver" 2> "$out/$n.ver.err"; s2=$?
  want=0; case $n in reject-*) want=3 ;; esac
  if [ "$s0" = "$want" ] && [ "$s1" = "$s0" ] && [ "$s2" = "$s0" ] && cmp -s "$out/$n.off" "$out/$n.on" && cmp -s "$out/$n.off" "$out/$n.ver" \
     && cmp -s "$out/$n.off.err" "$out/$n.on.err" && cmp -s "$out/$n.off.err" "$out/$n.ver.err"; then
    ok "emit of $n: unset, on and verify give the same bytes and status $want"
  else bad "emit of $n: statuses $s0 $s1 $s2 (want $want, all equal), outputs $(cmp -s "$out/$n.off" "$out/$n.on" && echo same || echo differ) / $(cmp -s "$out/$n.off" "$out/$n.ver" && echo same || echo differ), $(head -c 300 "$out/$n.ver.err")"; fi
done

# ---- 2. the batch tool
if want batch-verify || want batch-onoff || want batch-single; then
# PERSIST_EMIT names an emit tool built from compiler/emit.fib by this stage 2 (the gate passes the one build/golden/tools/emit already is: three shares,
# one build); unset, the share builds its own.
if [ -n "${PERSIST_EMIT:-}" ] && [ -x "$PERSIST_EMIT" ]; then cp "$PERSIST_EMIT" "$out/emit"
elif [ -n "${PERSIST_EMIT:-}" ]; then echo "FAIL PERSIST_EMIT=$PERSIST_EMIT is not an executable"; exit 1
elif "$fibc" build compiler/emit.fib -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$out/emit" > "$out/emit.build" 2>&1; then :
else bad "emit tool does not build: $(tail -n 5 "$out/emit.build")"; fi
if [ -x "$out/emit" ]; then
  files=("$here"/*.fib)
  while IFS= read -r l; do files+=("$l"); done < <(head -n 20 "$root/compiler/tests/golden/emit-fns.files")
  if want batch-verify; then
    FIB_PRUNE=0 FIB_CHECK_CACHE=verify "$out/emit" --batch "${files[@]}" > "$out/batch.out" 2> "$out/batch.err"
    if grep -q "CHECK-CACHE MISMATCH" "$out/batch.out" || [ -s "$out/batch.err" ]; then bad "emit --batch under verify: $(grep -c 'CHECK-CACHE MISMATCH' "$out/batch.out") mismatches; $(head -c 300 "$out/batch.err")"
    else ok "emit --batch under verify: ${#files[@]} inputs, each checked both ways with the same output"; fi
  fi
  if want batch-onoff || want batch-single; then FIB_PRUNE=0 "$out/emit" --batch "${files[@]}" > "$out/batch.on" 2>&1; fi
  if want batch-onoff; then
  FIB_PRUNE=0 FIB_CHECK_CACHE=off "$out/emit" --batch "${files[@]}" > "$out/batch.off" 2>&1
  if cmp -s "$out/batch.off" "$out/batch.on"; then ok "emit --batch with the kept library equals emit --batch without it, byte for byte"; else bad "emit --batch on and off differ"; fi
  fi
  if want batch-single; then
  rm -f "$out/batch.single"; for f in "${files[@]}"; do FIB_PRUNE=0 "$out/emit" "$f" >> "$out/batch.single" 2>&1; echo "#### status $?" >> "$out/batch.single"; done
  if [ "$(grep -v '^#### status' "$out/batch.on" | md5sum)" = "$(grep -v '^#### status' "$out/batch.single" | md5sum)" ]; then ok "and equals one process for each input"; else bad "emit --batch differs from one process for each input"; fi
  fi
fi
fi


# ---- 3. the server: a library edit is seen
if want server; then
cp -r "$root/lib" "$out/lib"
export FIB_LIB=$out/lib FIBC_SOCKET=$out/s.sock
prog=$out/uses.fib
printf '(defun main () -> i64 (let ((x (second [1 2 3])) (n (vec-count [1 2 3]))) (if x n 0)))\n' > "$prog"
"$fibc" serve --socket "$FIBC_SOCKET" > "$out/serve.log" 2>&1 &
pid=$!
for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do "$fibc" --server ping --socket "$FIBC_SOCKET" > /dev/null 2>&1 && break; sleep 0.3; done
ask() { "$fibc" --server check "$prog" --socket "$FIBC_SOCKET" -q > "$out/ask.out" 2>&1; echo $?; }
[ "$(ask)" = 0 ] && ok "server: the program checks" || bad "server: the program does not check at first: $(head -c 300 "$out/ask.out")"
[ "$(ask)" = 0 ] && ok "server: and again (the kept library state is used)" || bad "server: second check failed"
sed -i 's/^(defun vec-count /(defun vec-count-gone /' "$out/lib/prelude.fib"
[ "$(ask)" != 0 ] && ok "server: after the prelude lost vec-count the program is rejected (the kept state is not used for another prelude)" || bad "server: a prelude edit was not seen"
sed -i 's/^(defun vec-count-gone /(defun vec-count /' "$out/lib/prelude.fib"
[ "$(ask)" = 0 ] && ok "server: the prelude restored, the program checks" || bad "server: restoring the prelude was not seen"
sed -i 's/^(defun second /(defun second-gone /' "$out/lib/fib/seq/consumers2.fib"
[ "$(ask)" != 0 ] && ok "server: after an implicit module lost second the program is rejected" || bad "server: an edit of an implicit module was not seen"
sed -i 's/^(defun second-gone /(defun second /' "$out/lib/fib/seq/consumers2.fib"
[ "$(ask)" = 0 ] && ok "server: the module restored, the program checks" || bad "server: restoring the module was not seen"
# a program of two modules: the first is kept by the chain (own.chain) under the key of its text; an edit of it must be seen
mkdir -p "$out/two/app"
printf '(ns app.lib)\n(defun pick (v: (Vec i64)) -> i64 (unwrap-or (first v) 0))\n' > "$out/two/app/lib.fib"
printf '(ns main (:require [app.lib :as l]))\n(defun main () -> i64 (l/pick [1 2 3]))\n' > "$out/two/main.fib"
ask2() { "$fibc" --server check "$out/two/main.fib" --socket "$FIBC_SOCKET" -q -I "$out/two" > "$out/ask2.out" 2>&1; echo $?; }
[ "$(ask2)" = 0 ] && ok "server: a program of two modules checks" || bad "server: the two-module program does not check: $(head -c 300 "$out/ask2.out")"
sed -i 's/^(defun main () -> i64 (l\/pick \[1 2 3\]))/(defun main () -> i64 (l\/pick [1 2]))/' "$out/two/main.fib"
[ "$(ask2)" = 0 ] && ok "server: its last module edited, it checks (the first module's kept state is used)" || bad "server: the last module's edit broke the check: $(head -c 300 "$out/ask2.out")"
sed -i 's/^(defun pick /(defun pick-gone /' "$out/two/app/lib.fib"
[ "$(ask2)" != 0 ] && ok "server: after the first module lost pick the program is rejected (the kept state of its old text is not used)" || bad "server: an edit of the first module was not seen (a stale kept state answered)"
sed -i 's/^(defun pick-gone /(defun pick /' "$out/two/app/lib.fib"
[ "$(ask2)" = 0 ] && ok "server: the first module restored, the program checks" || bad "server: restoring the first module was not seen"
"$fibc" --server shutdown --socket "$FIBC_SOCKET" > /dev/null 2>&1; wait "$pid" 2> /dev/null
fi

# ---- 4. the program-prefix memo: a 2,000-line app, a swap in its last module
if want prefix; then
app=$out/app; mkdir -p "$app/syntax" "$app/util"
cp "$root"/compiler/syntax/*.fib "$app/syntax/"; cp "$root/compiler/util/text.fib" "$app/util/"; cp "$here/app/main.fib" "$app/main.fib"
lines=$(cat "$app"/syntax/*.fib "$app/util/text.fib" "$app/main.fib" | wc -l)
export FIB_LIB=$root/lib
pids=()
for mode in on off verify; do
  FIB_CHECK_CACHE=$mode "$fibc" serve --socket "$out/p-$mode.sock" > "$out/p-$mode.log" 2>&1 & pids+=($!)
done
for mode in on off verify; do
  for i in $(seq 40); do "$fibc" --server ping --socket "$out/p-$mode.sock" > /dev/null 2>&1 && break; sleep 0.3; done
done
# step MODE NAME: the emit request; its lIR and its check time (ms) to files
step() {
  "$fibc" --server emit "$app/main.fib" --socket "$out/p-$1.sock" --json -I "$app" > "$out/p-$1-$2.json" 2>&1
  python3 - "$out/p-$1-$2.json" "$out/p-$1-$2" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
open(sys.argv[2] + ".lir", "w").write(d.get("lir", "") or "")
st = d.get("stats", {})
open(sys.argv[2] + ".ms", "w").write("%d %d\n" % (st.get("check_ms", -1), st.get("total_ms", -1)))
open(sys.argv[2] + ".ok", "w").write("ok" if d.get("ok") and not any("CheckCache" in x.get("code", "") for x in d.get("diagnostics", [])) else "fail")
EOF
}
swap() { sed -i "s/^(defun answer (n: i64) -> i64 .*/(defun answer (n: i64) -> i64 $1)/" "$app/main.fib"; }
steps="cold swap1 swap2 middle swap3"
for s in $steps; do
  case $s in swap1) swap "(* n 3)" ;; swap2) swap "(+ n 1)" ;; middle) printf '\n(defun persist-probe () -> i64 7)\n' >> "$app/syntax/print.fib" ;; swap3) swap "(- n 1)" ;; esac
  for mode in on off verify; do step "$mode" "$s"; done
done
for mode in on off verify; do "$fibc" --server shutdown --socket "$out/p-$mode.sock" > /dev/null 2>&1; done
wait "${pids[@]}" 2> /dev/null
env -u FIB_CHECK_CACHE "$fibc" emit "$app/main.fib" -I "$app" > "$out/p-oneshot.lir" 2> "$out/p-oneshot.err"
held=1; same=1
for s in $steps; do
  for mode in on off verify; do [ "$(cat "$out/p-$mode-$s.ok" 2>/dev/null)" = ok ] || { held=0; echo "  $mode $s: $(head -c 300 "$out/p-$mode-$s.json")"; }; done
  { cmp -s "$out/p-on-$s.lir" "$out/p-off-$s.lir" && cmp -s "$out/p-on-$s.lir" "$out/p-verify-$s.lir" && [ -s "$out/p-on-$s.lir" ]; } || { same=0; echo "  $s: the lIR differs among on, off and verify"; }
done
[ "$held" = 1 ] && ok "prefix: every step of the transcript holds under on, off and verify ($lines lines, $(ls "$app/syntax" | wc -l) syntax modules)" || bad "prefix: a step failed"
[ "$same" = 1 ] && ok "prefix: the lIR of each step is byte-identical under on, off and verify" || bad "prefix: the lIR differs"
cmp -s "$out/p-on-swap3.lir" "$out/p-oneshot.lir" && ok "prefix: and the last step's equals fibc emit's" || bad "prefix: the last step's lIR differs from fibc emit's ($(wc -c < "$out/p-oneshot.lir") bytes one-shot, $(head -c 200 "$out/p-oneshot.err"))"
grep -c 'stage.rest' "$out/p-on.log" > /dev/null 2>&1 || true
med() { sort -n | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}'; }
on_ms=$(for s in swap1 swap2 swap3; do cut -d' ' -f1 "$out/p-on-$s.ms"; done | med)
off_ms=$(for s in swap1 swap2 swap3; do cut -d' ' -f1 "$out/p-off-$s.ms"; done | med)
on_tot=$(for s in swap1 swap2 swap3; do cut -d' ' -f2 "$out/p-on-$s.ms"; done | med)
echo "  prefix timing: swap of one function in the last module, check ms (median of 3): on $on_ms (front total $on_tot), off $off_ms; cold on $(cat "$out/p-on-cold.ms"), middle-module edit on $(cat "$out/p-on-middle.ms")"
[ -n "$on_ms" ] && [ "$on_ms" -lt 80 ] && ok "prefix: the swap's check with the kept state is under 80 ms ($on_ms ms; the whole check $off_ms ms)" || bad "prefix: the swap's check is not under 80 ms: on $on_ms ms, off $off_ms ms"
fi

if [ -z "${PERSIST_OUT:-}" ]; then rm -rf "$out"; fi
[ "$fail" = 0 ] && echo "persist tests: ok" || { echo "persist tests: FAILED"; exit 1; }
