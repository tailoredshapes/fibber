#!/bin/bash
# compiler/tests/live/stubs.sh: the checks of live mode's stubs and slots (package LV0 of docs/design/live-repl.md section 6; emit.live).
#   compiler/tests/live/stubs.sh [FIBC] [--update] [--quick] [-j N]     FIBC is stage 2 (default $FIBC); -j N cases at a time (default 4)
# 1. hello's stubs: `FIB_LIVE=1 F emit hello.fib` holds one stub and one slot per function of the program's own module (hello.fib's
#    `main` and `greet`) and none for the library's; the stubs and slots, exactly, are hello-stubs.golden (--update rewrites it; read the
#    diff). Without FIB_LIVE the same emit holds no stub and no slot: the lIR of a program is the same as before live mode existed.
# 2. a recursion a hundred million deep through a program function (deep.fib) runs under --live and answers as without: the stub is a
#    tail call, so it adds no frame; a stub that is a call and a return traps `stack overflow` (scripts/mutant-live.sh stub-no-tailcall).
#    A million deep (the ownership cases 53 and 164) does not tell them apart: main's stack (rt/thread.lir) holds a million stub frames.
# 3. the tables (not with --quick): `FIB_LIVE=1 F cases cases/ownership` and the quick gate's stdlib sample (mk/cases.mk SAMPLE_NAMES:
#    every 10th case and every expected failure) print the same table as `F cases` without it, the trace mode included (the children of
#    the harness run `fibc run --trace`: a stub allocates nothing, so no `A`/`F` line changes). A live program is also a catching one
#    (docs/design/live-repl.md 3.5, risk 5 of section 7), and a case whose header pins what the non-catching convention does (a trap
#    that abandons objects) differs under FIB_CATCH=all with or without stubs: the stubs are judged against the catching table
#    (FIB_LIVE=1 and FIB_CATCH=all must agree row for row), and the rows where the catching table differs from the plain one must be
#    exactly those of catching.expected (one `DIR/CASE` per line): a new one is a failure, as is one that no longer differs. The cases
#    catching.expected lists are always in the sample: the sample's stride is the case count over 100, so a case added to cases/stdlib
#    moves which cases the stride picks (8423 moved it from 14 to 15 and 8690 fell out, which read as "no longer differs").
# Exit 0 when every check holds.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
update=; quick=; jobs=4; fibc=${FIBC:-}
while [ $# -gt 0 ]; do
  case $1 in --update) update=1 ;; --quick) quick=1 ;; -j) jobs=${2:?-j needs a number}; shift ;; *) fibc=$1 ;; esac
  shift
done
[ -x "$fibc" ] || { echo "usage: stubs.sh FIBC [--update] [--quick] [-j N] (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE
out=${TMPDIR:-/tmp}/live-stubs-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
bad=0
run() { (ulimit -v 16000000; "$@"); }

# 1. hello
if ! FIB_LIVE=1 run "$fibc" emit -I "$here" "$here/hello.fib" > "$out/hello.live.lir" 2> "$out/hello.live.err"; then echo "FAIL hello: emit --live failed"; head -5 "$out/hello.live.err"; bad=1; fi
if ! run "$fibc" emit -I "$here" "$here/hello.fib" > "$out/hello.plain.lir" 2> "$out/hello.plain.err"; then echo "FAIL hello: emit failed"; head -5 "$out/hello.plain.err"; bad=1; fi
slots=$(sed -n 's/^(global [a-z]* live\.\([^ ]*\) .*/\1/p' "$out/hello.live.lir" | tr '\n' ' ')
# every item prints on one line: the slots (`(global .. live.NAME ..)`) and the stubs (the `define` of each NAME that has a slot)
awk -v s="$slots" 'BEGIN { n = split(s, a, " "); for (i = 1; i <= n; i++) slots[a[i]] = 1 }
  /^\(global / && $0 ~ / live\./ { print; next }
  /^\(define / { for (i = 1; i <= NF; i++) { n = $i; sub(/^\(+/, "", n); if (n in slots) { print; break } } }' "$out/hello.live.lir" > "$out/hello.stubs"
if [ -n "$update" ]; then cp "$out/hello.stubs" "$here/hello-stubs.golden"; echo "updated hello-stubs.golden ($(grep -c '^(global' "$out/hello.stubs") slots)"
elif diff -u "$here/hello-stubs.golden" "$out/hello.stubs" > "$out/hello.diff"; then echo "ok hello stubs ($(grep -c '^(global' "$out/hello.stubs") slots, $(grep -c '^(define' "$out/hello.stubs") stubs)"
else echo "FAIL hello: the stubs differ from hello-stubs.golden"; head -40 "$out/hello.diff"; bad=1; fi
# one stub per program function and none for the library: the slots are exactly main's and greet's bodies
want="f.greet f.main"
got=$(printf '%s\n' $slots | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//')
if [ "$got" = "$want" ]; then echo "ok hello: slots are the program's functions ($got)"; else echo "FAIL hello: slots are [$got], wanted [$want]"; bad=1; fi
if grep -q 'live\.' "$out/hello.plain.lir"; then echo "FAIL hello: a plain emit mentions live."; bad=1; else echo "ok hello: no stub without FIB_LIVE"; fi
if grep -q '\.g0' "$out/hello.plain.lir"; then echo "FAIL hello: a plain emit has a generation name"; bad=1; fi
# the stub loads with acquire order and jumps: both words in every stub
n_stub=$(grep -c '^(define' "$out/hello.stubs"); n_acq=$(grep -c 'atomic-load acquire ptr @live\.' "$out/hello.stubs"); n_tail=$(grep -c 'indirect-tailcall' "$out/hello.stubs")
if [ "$n_stub" -gt 0 ] && [ "$n_acq" = "$n_stub" ] && [ "$n_tail" = "$n_stub" ]; then echo "ok hello: every stub is an acquire load and a tail jump"; else echo "FAIL hello: $n_stub stubs, $n_acq acquire loads, $n_tail tail jumps"; bad=1; fi

# 2. the deep recursion
plain=$(run "$fibc" run "$here/deep.fib" 2> /dev/null | tr '\n' ' ')
r=$(FIB_LIVE=1 run "$fibc" run "$here/deep.fib" 2> "$out/deep.err" | tr '\n' ' ')
if [ "$r" = "$plain" ] && [ "${r%% *}" = 100000000 ]; then echo "ok deep recursion under FIB_LIVE=1: $r"
else echo "FAIL deep recursion under FIB_LIVE=1 (got [$r], plain [$plain]): $(head -c 300 "$out/deep.err")"; bad=1; fi
r=$(run "$fibc" run --live "$here/deep.fib" 2> /dev/null | tr '\n' ' ')
if [ "$r" = "$plain" ]; then echo "ok run --live: $r"; else echo "FAIL run --live: [$r]"; bad=1; fi

# 3. the tables
if [ -z "$quick" ]; then
  stdlib_names() { (cd cases/stdlib && for e in $(LC_ALL=C ls); do if [ -f "$e" ]; then [[ $e == *.fib ]] && echo "$e" || true; elif [ -f "$e/main.fib" ]; then echo "$e"; fi; done); }
  n=$(stdlib_names | wc -l); step=$(( n / 100 > 0 ? n / 100 : 1 ))
  sample=$({ stdlib_names | awk -v k="$step" 'NR % k == 0'; sed -n 's|^cases/stdlib/\([^ ]*\) .*|\1|p' scripts/ci-stage2.expected | sed 's|/main.fib$||'
             sed -n 's|^cases/stdlib/||p' "$here/catching.expected" | sed 's|/main.fib$||'; } | LC_ALL=C sort -u | tr '\n' ' ')
  rows() { awk 'NR > 1 && NF >= 2 && $2 != "" && $1 != "" && $0 !~ /^[0-9]+ cases:/ && $0 !~ /^$/ { print $1 }' "$1"; }  # the case names of a table
  table() { # table NAME DIR [--only ..]: the case table of a directory, plain, catching and live
    local name=$1 dir=$2; shift 2
    run "$fibc" cases "$@" -j "$jobs" > "$out/$name.plain" 2>&1
    FIB_CATCH=all run "$fibc" cases "$@" -j "$jobs" > "$out/$name.catch" 2>&1
    FIB_LIVE=1 run "$fibc" cases "$@" -j "$jobs" > "$out/$name.live" 2>&1
    if diff -u "$out/$name.catch" "$out/$name.live" > "$out/$name.diff"; then echo "ok $name: the same table with stubs as without ($(rows "$out/$name.live" | wc -l) rows; $(tail -n 1 "$out/$name.live"))"
    else echo "FAIL $name: the tables differ with the stubs"; grep '^[-+][0-9]' "$out/$name.diff" | head -20; bad=1; fi
    diff "$out/$name.plain" "$out/$name.catch" | grep -v '^> [0-9]* cases:' | sed -n "s|^> \([^ ]*\) .*|$dir/\1|p" | LC_ALL=C sort -u > "$out/$name.catching"
    grep "^$dir/" "$here/catching.expected" | LC_ALL=C sort -u > "$out/$name.expected"
    if diff "$out/$name.expected" "$out/$name.catching" > "$out/$name.cdiff"; then echo "ok $name: the catching convention changes the rows catching.expected lists ($(wc -l < "$out/$name.catching"))"
    else echo "FAIL $name: the rows the catching convention changes are not those of catching.expected"; cat "$out/$name.cdiff"; bad=1; fi
  }
  table ownership cases/ownership cases/ownership
  # shellcheck disable=SC2086
  table sample cases/stdlib cases/stdlib --only $sample
fi
[ "$bad" = 0 ] && echo "live stubs: ok" || echo "live stubs: FAILED"
exit "$bad"
