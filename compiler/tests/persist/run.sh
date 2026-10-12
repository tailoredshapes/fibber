#!/bin/bash
# compiler/tests/persist/run.sh [FIBC [PART]]: the controls of the persistent checker (docs/design/persistent-checker.md), with a stage 2 (FIBC). Offline, about 2 minutes.
#  1. each program of this directory through `fibc emit` three ways (FIB_CHECK_CACHE unset, on, verify): the same lIR or the same rejection, the same status;
#     the programs named reject-* are rejected (status 3), the others accepted.
#  2. the same programs and the first inputs of the emit-fns golden through `emit --batch` (one process, the library kept) with FIB_CHECK_CACHE=verify, which
#     checks each input both ways: no input ends in `#### CHECK-CACHE MISMATCH`; and the blocks equal those of one process for each input.
#  3. `fibc serve`: after an edit of the prelude, and of a module of the implicit library, a program that used them is checked again with the edit (the kept
#     state is not used for another library), and once the edit is reverted it checks again.
# PART (default: all) runs one independent share, so make runs the shares side by side (mk/tools.mk, PERSIST_PARTS):
#   cmd-0 cmd-1 cmd-2  part 1 for every third program (by position in the sorted list);  batch-verify  the verify run of part 2;
#   batch-onoff  the batch with the kept library against the batch without it;  batch-single  the batch against one process for each input;  server  part 3.
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
if "$fibc" build compiler/emit.fib -I compiler -I lib -L "${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}" -l LLVM-21 -o "$out/emit" > "$out/emit.build" 2>&1; then
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
else bad "emit tool does not build: $(tail -n 5 "$out/emit.build")"; fi
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
"$fibc" --server shutdown --socket "$FIBC_SOCKET" > /dev/null 2>&1; wait "$pid" 2> /dev/null
fi

if [ -z "${PERSIST_OUT:-}" ]; then rm -rf "$out"; fi
[ "$fail" = 0 ] && echo "persist tests: ok" || { echo "persist tests: FAILED"; exit 1; }
