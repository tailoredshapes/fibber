#!/bin/bash
# `cases` of the compiler in fibber (compiler/driver/harness.fib) judged on planted faults: each check copies a real case into a scratch
# directory, breaks one verdict of its header (or its program), and requires the harness to say so (a test that cannot fail is worse than
# none), after requiring the unbroken copy to pass. Also the refusals: a bad header, a prefix no case starts with, a directory with no case.
# usage: cases-check.sh STAGE2   Run from the repository root.
# Prints `ok NAME` or `FAIL NAME`; exit status 0 only if none failed.
s2=$1; fail=0
t=$(mktemp -d "$HOME/.cache/fibber-scratch/cases-check.XXXXXX"); trap 'rm -rf "$t"' EXIT
ck() { if [ "$2" = "$3" ]; then echo "ok $1"; else echo "FAIL $1: got [$2] want [$3]"; fail=1; fi; }
row() { $s2 cases "$t" 2>&1 | grep -E "^a.fib " | sed -E 's/  +/ /g'; }
ckp() { if [[ "$2" == "$3"* ]]; then echo "ok $1"; else echo "FAIL $1: got [$2] want [$3...]"; fail=1; fi; }
status() { $s2 cases "$t" >/dev/null 2>&1; echo $?; }
ok=cases/ownership/01-return-part-of-argument.fib
rej=cases/ownership/107-reject-dyn-send-of-value-reaching-a-cell.fib
trp=cases/ownership/103-trap-division-by-zero.fib
leak=cases/ownership/15-cycle-through-cell-leaks.fib

cp $ok $t/a.fib; ck "a good accept case passes" "$(row)" "a.fib pass"; ck "and the status is 0" "$(status)" 0
sed -E 's/^;; result: *1$/;; result: 2/' $ok > $t/a.fib
ckp "a wrong result fails" "$(row)" "a.fib FAIL result: expected 2, got 1"; ck "and the status is 1" "$(status)" 1
sed -E 's/^;; audit: *clean$/;; audit: leak-cycle/' $ok > $t/a.fib
ckp "a run that leaked nothing fails a leak-cycle verdict" "$(row)" "a.fib FAIL audit: expected leak-cycle,"
cp $leak $t/a.fib; ck "a leaked cycle passes leak-cycle" "$(row)" "a.fib pass"
sed -E 's/^;; audit: *leak-cycle$/;; audit: clean/' $leak > $t/a.fib
ckp "a leak fails audit: clean" "$(row)" "a.fib FAIL audit: expected clean, got"
sed -E 's/^;; result: *1$/;; result: 1\n;; allocs: <= 0/' $ok > $t/a.fib
ckp "allocs <= 0 fails a program that allocates" "$(row)" "a.fib FAIL allocs: expected at most"
cp $rej $t/a.fib; ck "a reject case passes" "$(row)" "a.fib pass"
sed -E 's/^;; error: .*/;; error: zzz no such text/' $rej > $t/a.fib
ckp "a reject with another message fails" "$(row)" "a.fib FAIL rejected, but the error does not contain the"
sed -E 's/^;; expect: accept/;; expect: reject\n;; error: boom/; /^;; (result|audit):/d' $ok > $t/a.fib
ckp "a reject verdict on a program that compiles fails" "$(row)" "a.fib FAIL expected reject with \"boom\", but comp"
cp $trp $t/a.fib; ck "a trap case passes" "$(row)" "a.fib pass"
sed -E 's/^;; trap: .*/;; trap: zzz/' $trp > $t/a.fib
ckp "a trap with another message fails" "$(row)" "a.fib FAIL trapped, but the trap does not contai"
sed -E 's/^;; expect: trap/;; expect: accept/; s/^;; trap: .*/;; result: 1\n;; audit: clean/' $trp > $t/a.fib
ckp "a trapping program under accept fails" "$(row)" "a.fib FAIL the run trapped: integer"
printf ';; spec: x\n;; expect: accept\n(defun main () -> i64 1)\n' > $t/a.fib
ck "a header missing a key is a HEADER row" "$(row)" "a.fib HEADER line 2: missing required header key \`result\`"; ck "and the status is 1" "$(status)" 1
printf ';; spec: x\n;; bogus: 1\n' > $t/a.fib
ck "an unknown key is a HEADER row" "$(row)" "a.fib HEADER line 2: unknown header key \`bogus\`"
printf '(defun main () -> i64 1)\n' > $t/a.fib
ck "no header is a HEADER row" "$(row)" "a.fib HEADER line 1: empty header: line 1 is not \`;; key: value\`"
printf ';; spec: x\n;; expect: accept\n;; result: 1\n;; audit: clean\n(defun main () -> i64 "s")\n' > $t/a.fib
ckp "an accept case the checker refuses fails" "$(row)" "a.fib FAIL expected accept, but rejected:"
printf ';; spec: x\n;; expect: accept\n;; result: 1\n;; audit: clean\n;; open: L1\n(defun main () -> i64 "s")\n' > $t/a.fib
ckp "an open case that fails is OPEN, not a failure" "$(row)" "a.fib OPEN L1:"; ck "and the status is 0" "$(status)" 0
printf ';; spec: x\n;; expect: accept\n;; result: 1\n;; audit: clean\n;; open: L1\n(defun main () -> i64 1)\n' > $t/a.fib
ck "an open case that passes is a FAIL" "$(row)" "a.fib FAIL the item landed: remove \`open\`"
ck "a prefix no case starts with is exit 2" "$($s2 cases $t --only zz 2>&1; echo $?)" "$(printf 'fibc: no case matches zz in %s\n2' "$t")"
mkdir -p $t/empty; $s2 cases $t/empty >/dev/null 2>&1; ck "a directory with no case is not a pass" "$?" 1
# -j N: the same table and counts as one at a time, in the same order, and a planted fault is still reported (exit 1).
rm -rf "$t"/*; mkdir "$t/par"
for c in $ok $rej $trp $leak; do cp "$c" "$t/par/$(basename "$c")"; done
sed -E 's/^;; result: *1$/;; result: 2/' $ok > "$t/par/00-planted-wrong-result.fib"
printf ';; spec: x\n;; bogus: 1\n' > "$t/par/99-bad-header.fib"
$s2 cases "$t/par" > "$t/seq.txt" 2>&1; sst=$?; $s2 cases "$t/par" -j 8 > "$t/par8.txt" 2>&1; pst=$?
$s2 cases "$t/par" -j3 > "$t/par3.txt" 2>&1
ck "-j 8 prints the rows and counts of one at a time, byte for byte" "$(cmp "$t/seq.txt" "$t/par8.txt" && echo same)" same
ck "-j3 (glued) too" "$(cmp "$t/seq.txt" "$t/par3.txt" && echo same)" same
ck "the planted wrong result is a FAIL row under -j 8" "$(grep -c '^00-planted-wrong-result.fib *FAIL' "$t/par8.txt")" 1
ck "the bad header is a HEADER row under -j 8" "$(grep -c '^99-bad-header.fib *HEADER' "$t/par8.txt")" 1
ck "and the counts line says 4 pass, 1 fail, 1 header error" "$(grep -c '^6 cases: 4 pass, 1 fail, 0 pending, 1 header error' "$t/par8.txt")" 1
ck "the status is 1 under -j 8 as at -j 1" "$pst/$sst" "1/1"
ck "the rows are in file-name order" "$(awk '$1 ~ /\.fib$/ {print $1}' "$t/par8.txt" | sort -c && echo sorted)" sorted
mkdir "$t/tmpd"; TMPDIR="$t/tmpd" $s2 cases "$t/par" -j 8 > /dev/null 2>&1
ck "-j 8 leaves no scratch directory or file behind" "$(ls -A "$t/tmpd" | wc -l)" 0
$s2 cases "$t/par" -j 0 > /dev/null 2>&1; ck "-j 0 is a usage error: exit 2" $? 2
$s2 cases "$t/par" -j > /dev/null 2>&1; ck "-j with no number is a usage error: exit 2" $? 2
$s2 cases "$t/par" -j x > /dev/null 2>&1; ck "-j x is a usage error: exit 2" $? 2
exit $fail
