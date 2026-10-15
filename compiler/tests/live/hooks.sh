#!/bin/bash
# compiler/tests/live/hooks.sh: the hooks of fib.test.run a live session's `test` op needs (docs/design/live-meshql.md 3.2, package LT1):
# `argv-override` read by `run-main` instead of `(args)`, `report-sink` written instead of standard output.
#   compiler/tests/live/hooks.sh [FIBC]       FIBC is stage 2 (default $FIBC)
# hooks.fib runs hookspecs' three scenarios with the override set to the words `fibc test` passes a child (`--format json --seed 1 -j 1`) and
# the sink collecting into a buffer it writes to a file; hooks-spec.fib is the same scenarios as a plain spec file. Checks:
#   1. hooks.fib prints nothing but the result line of `fibc run`, and its result is the runner's status (1: a scenario fails)
#   2. the collected document is byte for byte what `fibc run hooks-spec.fib -- --format json --seed 1 -j 1` prints (what `fibc test`'s child prints)
#   3. the `results` of `fibc test hooks-spec.fib --format json` are the collected document's (the parent adds `files` and keeps the rows)
#   4. the same in plain format: the rows and the summary stream through the sink, nothing is printed
#   5. `--list` goes through the sink too
#   6. a selection that matches nothing: status 2, its message on standard error, nothing in the sink
# The stdlib case 8423 is the in-process proof (the override selects, the document is the one, `(args)` again when cleared); scripts/mutant-live.sh
# sink-ignored and override-ignored must turn it red. Exit 0 when every check holds.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-}
for a in "$@"; do fibc=$a; done
[ -x "$fibc" ] || { echo "usage: hooks.sh FIBC (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
unset FIB_LIVE FIB_NO_PROJECT
ulimit -v 16000000
out=${TMPDIR:-/tmp}/live-hooks-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
bad=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; bad=1; fi; }
same() { if cmp -s "$2" "$3"; then echo "ok    $1"; else echo "FAIL  $1: the texts differ"; diff "$2" "$3" | head -10; bad=1; fi; }

# 1, 2: json
"$fibc" run -I "$here" "$here/hooks.fib" -- "$out/sink.json" --format json --seed 1 -j 1 > "$out/hooks.out" 2> "$out/hooks.err"; rc=$?
ck "hooks.fib: exit 0, the result line is the runner's status 1, nothing else on standard output" "$rc $(tr '\n' '|' < "$out/hooks.out")" "0 1|"
ck "hooks.fib: nothing on standard error" "$(wc -c < "$out/hooks.err" | tr -d ' ')" 0
"$fibc" run -I "$here" "$here/hooks-spec.fib" -- --format json --seed 1 -j 1 > "$out/child.out" 2> /dev/null; rc=$?
ck "the plain spec as a child of fibc test: status 1" "$rc" 0
sed '$d' "$out/child.out" > "$out/child.json"
same "the collected document is the child's, byte for byte" "$out/sink.json" "$out/child.json"
ck "the document is one json object of three rows" "$(grep -c '^{"schema":1,"seed":1,' "$out/sink.json") $(grep -o '"suite":' "$out/sink.json" | wc -l | tr -d ' ')" "1 3"
# 3: through fibc test
"$fibc" test -I "$here" "$here/hooks-spec.fib" --format json > "$out/test.out" 2> /dev/null
ck "fibc test --format json carries the same rows" "$(grep -o '"results":\[.*\]}' "$out/test.out")" "$(grep -o '"results":\[.*\]}' "$out/sink.json")"
# 4: plain
"$fibc" run -I "$here" "$here/hooks.fib" -- "$out/sink.plain" --format plain --seed 1 -j 1 > "$out/hooks2.out" 2> /dev/null
ck "plain: nothing printed but the result line" "$(tr '\n' '|' < "$out/hooks2.out")" "1|"
"$fibc" run -I "$here" "$here/hooks-spec.fib" -- --format plain --seed 1 -j 1 > "$out/child2.out" 2> /dev/null
sed '$d' "$out/child2.out" > "$out/child2.plain"
same "plain: the rows and the summary through the sink are the child's" "$out/sink.plain" "$out/child2.plain"
ck "plain: the rows streamed (a head, three rows, a summary, a replay line)" "$(grep -c '' "$out/sink.plain")" "$(grep -c '' "$out/child2.plain")"
# 5: --list
"$fibc" run -I "$here" "$here/hooks.fib" -- "$out/sink.list" --list > "$out/hooks3.out" 2> /dev/null
ck "--list: the listing through the sink, nothing printed" "$(tr '\n' '|' < "$out/hooks3.out") $(grep -c '^Hooks/' "$out/sink.list")" "0| 3"
# 6: nothing matches
"$fibc" run -I "$here" "$here/hooks.fib" -- "$out/sink.none" --only nothing-matches > "$out/hooks4.out" 2> "$out/hooks4.err"
ck "nothing matches: status 2, the message on standard error, nothing in the sink" "$(tr '\n' '|' < "$out/hooks4.out") $(grep -c "no scenario matches 'nothing-matches'" "$out/hooks4.err") $(wc -c < "$out/sink.none" | tr -d ' ')" "2| 1 0"
[ "$bad" = 0 ] && echo "live hooks: ok" || echo "live hooks: FAILED"
exit "$bad"
