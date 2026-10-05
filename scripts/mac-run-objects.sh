#!/bin/bash
# Runs on the target (an Apple Silicon Mac, or any aarch64 host): links each DIR/NAME.o with `cc`, runs it and compares with DIR/NAME.expect.
# The objects come from compiler/tests/native/a64-cross-mac.sh (cross-compiled on x86 Linux) or from `fibc`/`lairf` on the Mac itself.
# NAME.expect is one line:  `result N`  (the last line of standard output is N), `trap TEXT` (exit status nonzero and standard error holds TEXT),
# or `lines FILE` (the whole standard output equals the file DIR/FILE). Prints `ok NAME` or `FAIL NAME: why`, then the counts; exit 1 on any failure.
# usage: scripts/mac-run-objects.sh DIR
set -u
dir=${1:?usage: mac-run-objects.sh DIR}
pass=0; fail=0
for exp in "$dir"/*.expect; do
  [ -e "$exp" ] || continue
  n=$(basename "$exp" .expect); obj=$dir/$n.o
  read -r kind rest < "$exp"
  if [ ! -f "$obj" ]; then echo "FAIL $n: no object"; fail=$((fail+1)); continue; fi
  if ! cc "$obj" -o "$dir/$n.exe" -lm -lpthread 2> "$dir/$n.link"; then echo "FAIL $n: link: $(head -c 200 "$dir/$n.link" | tr '\n' ' ')"; fail=$((fail+1)); continue; fi
  perl -e 'alarm 20; exec @ARGV' "$dir/$n.exe" > "$dir/$n.out" 2> "$dir/$n.err"; st=$?
  case $kind in
    result) last=$(tail -n 1 "$dir/$n.out")
            if [ "$st" -eq 0 ] && [ "$last" = "$rest" ]; then echo "ok   $n"; pass=$((pass+1)); else echo "FAIL $n: status $st, last line [$last], want [$rest]"; fail=$((fail+1)); fi ;;
    trap)   if [ "$st" -ne 0 ] && grep -qF -- "$rest" "$dir/$n.err"; then echo "ok   $n"; pass=$((pass+1)); else echo "FAIL $n: status $st, stderr [$(head -c 120 "$dir/$n.err")], want [$rest]"; fail=$((fail+1)); fi ;;
    lines)  if cmp -s "$dir/$n.out" "$dir/$rest"; then echo "ok   $n"; pass=$((pass+1)); else echo "FAIL $n: output differs from $rest"; fail=$((fail+1)); fi ;;
    *) echo "FAIL $n: bad expect line"; fail=$((fail+1)) ;;
  esac
  rm -f "$dir/$n.exe"
done
echo "run-objects: $pass pass, $fail fail"
[ "$fail" -eq 0 ]
