#!/bin/bash
# compiler/tests/repl/run.sh: scripted REPL sessions (docs/design/dev-loop.md section 9). Each NAME.in is the inputs of a session, NAME.transcript the exact output of
# `fibc repl --echo` on them (the prompt and each input line echoed, so the file reads as the session). The test runs every session and diffs its output with
# the transcript: any difference fails, and so does a missing transcript. The sessions are independent and run side by side.
#   compiler/tests/repl/run.sh [FIBC]          check
#   compiler/tests/repl/run.sh --update [FIBC] write the transcripts from the REPL's present output (read the diff before committing)
# Every session runs from the repository root, so that `:load compiler/tests/repl/lib1.fib` finds its file. Exit 0 when every session matches.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
update=; [ "${1:-}" = --update ] && { update=1; shift; }
fibc=${1:-${FIBC:-fibc}}
export FIB_LIB=${FIB_LIB:-$root/lib}
out=${TMPDIR:-/tmp}/repl-test-$$; mkdir -p "$out"; trap 'rm -rf "$out"' EXIT
cd "$root" || exit 2
names=(); for f in "$here"/*.in; do names+=("$(basename "$f" .in)"); done
for n in "${names[@]}"; do
  ( ulimit -v 16000000; HOME=$out timeout 900 "$fibc" repl --echo < "$here/$n.in" > "$out/$n.out" 2>&1; echo $? > "$out/$n.rc" ) &
done
wait
for n in "${names[@]}"; do sed -i -E 's/[0-9]+ ms/N ms/g' "$out/$n.out"; done   # the `:time` lines: the counts stay, the milliseconds do not
bad=0
for n in "${names[@]}"; do
  if [ -n "$update" ]; then cp "$out/$n.out" "$here/$n.transcript"; echo "updated $n.transcript"; continue; fi
  if [ ! -f "$here/$n.transcript" ]; then echo "FAIL $n: no transcript"; bad=1; continue; fi
  if [ "$(cat "$out/$n.rc")" != 0 ]; then echo "FAIL $n: the REPL exited with status $(cat "$out/$n.rc")"; bad=1; fi
  if diff -u "$here/$n.transcript" "$out/$n.out" > "$out/$n.diff"; then echo "ok $n"; else echo "FAIL $n: output differs from the transcript"; head -40 "$out/$n.diff"; bad=1; fi
done
[ "$bad" = 0 ] && echo "repl transcripts: ok" || echo "repl transcripts: FAILED"
# the terminal: raw mode, editing, history, continuation, ctrl-c and ctrl-d on a pseudo-terminal
if [ -z "$update" ]; then python3 "$here/terminal.py" "$fibc" || bad=1; fi
exit "$bad"
