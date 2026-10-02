#!/bin/bash
# The judge of the macro module (package E11: emit.macros.abi, emit.macros.front, emit.compile `compile-macro`, the macro arms of
# emit.lower.cx and emit.lower.builtins): for every `defmacro` of each FILE, `fibc emit-dump --macro NAME FILE` (the Rust, the oracle)
# against what the emitter tool compiler/emit.fib prints for the same words, byte for byte with the exit status. A macro passes only
# when (1) the two outputs are equal, (2) the Rust printed a module (the line `;; == macro KEY`, the keyword table and the exported
# surface of the ABI: two equal refusals do not count, they are tallied apart as `same-record`), and (3) for a program of this
# directory, macro-*.fib, the module holds the mark of the construct the program is about (so it cannot pass on an unrelated text).
# A tool that reports a stub reached (`todo: PACKAGE NAME`, exit 70, or a trap, exit 134) is PENDING, never a pass. A macro whose Rust
# output is a record of its expansion (`MacroFailed`, `BadLiteral`: the Rust expander RUNS the macros of the file, which stage 2 cannot
# until stage 2b wires a runner) and whose tool output differs (a module, or the record of checking the macro alone) is tallied as
# `evaluator`: neither a pass nor a failure.
# Run from the repository root with FIB_LIB set (cases/stdlib/support is added to it); RECORDS=1 lists the equal refusals.
# With no FILE: the corpus files that define a macro, the library files that do, and the programs compiler/tests/emit/macro-*.fib
# (each also by the key of its macro, `--macro main/NAME`).
# usage: macros.sh FIBC TOOL [FILE..]    (TOOL: `fibc build compiler/emit.fib -I compiler -I lib -L target/debug -l lair -o TOOL`)
FIBC=${1:?fibc}; TOOL=${2:?tool}; shift 2
here=$(dirname "$0")
if [ $# -eq 0 ]; then
  set -- $(grep -l '(defmacro' -r cases/ownership cases/modules cases/stdlib lib 2>/dev/null | sort) "$here"/macro-*.fib
fi
export FIB_LIB="${FIB_LIB:-$PWD/lib}:$PWD/cases/stdlib/support"   # the stdlib programs `:use` tl.check
same=0; record=0; diff=0; pending=0; bad=0; evaluator=0
T=${TMPDIR:-$HOME/.cache/fibber-scratch}/macros-sh.$$; mkdir -p "$T"

# The texts (one per line) a macro-*.fib program must leave in the module (grep -F), by file name and macro name.
mark() {
  case "$1/$2" in
    macro-trap/*) echo '@fib.trap-at (string "';;
    macro-div/*) echo '(call @fib.trap-at-c (string "'; echo 'by zero"))'; echo '(string "integer overflow in - at i64"))';;
    macro-keywords/*) echo '(block kw6 ';; macro-rest/nothing) echo '(define (fibm.entry.0 ptr) ()';;
    macro-rest/wrap) echo '(ptr a1)';; macro-rest/many) echo '(ptr a3)';; macro-literals/*) echo '(double 1e21)';;
    macro-reflect/*) echo '@fibm.gensym-hook';; *) echo '';;
  esac
}

# The words for each macro: FILE NAME, and FILE main/NAME for the programs of this directory.
words() {
  for f in "$@"; do
    for name in $(sed -n 's/^ *(defmacro \([^ ()]*\).*/\1/p' "$f"); do
      echo "$f $name"
      case "$f" in */macro-*.fib) echo "$f main/$name";; esac
    done
  done
}

while read -r f name; do
  id="$f $name"; base=$(basename "$f" .fib); bare=${name#main/}
  (ulimit -v 4000000; timeout 120 "$FIBC" emit-dump --macro "$name" "$f" > "$T/rust" 2> "$T/rust.err"; echo "status $?" >> "$T/rust")
  (ulimit -c 0 -v 4000000; timeout 120 "$TOOL" --macro "$name" "$f" > "$T/fib" 2> "$T/fib.err"; echo "status $?" >> "$T/fib")
  if grep -q '^status 70$\|^status 134$' "$T/fib"; then
    pending=$((pending+1)); echo "pending $id: $(grep -h -o 'todo: [A-Za-z0-9]* [a-z0-9-]*' "$T/fib" "$T/fib.err" | head -1)"; continue
  fi
  if ! cmp -s "$T/rust" "$T/fib"; then
    if grep -q '^error \(MacroFailed\|BadLiteral\)' "$T/rust"; then
      evaluator=$((evaluator+1)); echo "evaluator $id: $(grep -m1 '^error ' "$T/rust" | cut -c1-110)"
    else
      diff=$((diff+1)); echo "DIFFERENT $id"; diff "$T/rust" "$T/fib" | head -"${LINES_SHOWN:-6}" | cut -c1-200
    fi
    continue
  fi
  if ! grep -q '^;; == macro ' "$T/rust"; then
    record=$((record+1)); [ -n "$RECORDS" ] && echo "same-record $id: $(sed -n 2p "$T/rust" | cut -c1-100)"; continue
  fi
  m=$(mark "$base" "$bare"); missing=""
  while IFS= read -r one; do [ -n "$one" ] && ! grep -qF -- "$one" "$T/fib" && missing=$one; done <<< "$m"
  if ! grep -qF 'fibm.kw-count.0' "$T/rust" || ! grep -qF 'fibm.set-trap-hook.0' "$T/rust"; then
    bad=$((bad+1)); echo "BAD $id: the Rust text has no keyword table or ABI surface"
  elif [ -n "$missing" ]; then bad=$((bad+1)); echo "BAD $id: the text has no '$missing'"
  else same=$((same+1)); fi
done < <(words "$@")
rm -rf "$T"
echo "macros: same $same, same-record $record, different $diff, pending $pending, evaluator $evaluator, bad $bad"
[ $diff -eq 0 ] && [ $pending -eq 0 ] && [ $bad -eq 0 ]
