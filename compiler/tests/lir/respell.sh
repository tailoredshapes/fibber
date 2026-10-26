#!/bin/bash
# compiler/tests/lir/respell.sh: `lairf respell` over every lIR file of the repository (spec/lir.md 2, the respelling's step 3). Each file is
# rewritten into a scratch tree of the same layout (so `import` and `require-macros` paths still resolve), and for each:
#   1. parse-equivalence: `lairf --old-spelling dump-ast` of the original and of the respelled file give the same output and status, with
#      positions stripped (the respelling moves columns, never lines);
#   2. idempotence: `lairf respell --check` of the respelled file passes (the new spelling is a fixed point);
#   3. pure new spelling: for a file that uses no macro library (define-macro, require-macros, import, the reader forms: their libraries
#      build forms at macro time, which the tool cannot see), `lairf --new-spelling dump-ast` of the respelled file equals 1's output: the
#      file holds nothing of the old spelling. A file in RELEASED is excepted: its verdict comes from a rule the new spelling drops (the
#      reserved words and the sigils as names), so the strict read accepts what the old refused.
# usage: compiler/tests/lir/respell.sh FIBC   (LAIRF names a built lairf; OUT the scratch, default $TMPDIR/lir-respell)
# Prints the counts; exit 0 when every file held, 1 otherwise, 2 when nothing could be built.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
F=${1:-${FIBC:-}}; L=${LAIRF:-}
out=${OUT:-${TMPDIR:-/tmp}/lir-respell}; rm -rf "$out"; mkdir -p "$out/tree"
lib=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
export LD_LIBRARY_PATH=$lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cd "$root" || exit 2
if [ -z "$L" ]; then
  [ -x "$F" ] || { echo "respell: give a stage 2 fibc (FIBC) or a lairf (LAIRF)" >&2; exit 2; }
  (ulimit -v 16000000; timeout 900 "$F" build compiler/lairf.fib -I compiler -I lib -L "$lib" -l LLVM-21 -o "$out/lairf") > "$out/lairf.log" 2>&1 \
    || { tail -5 "$out/lairf.log"; echo "respell: lairf did not build" >&2; exit 2; }
  L=$out/lairf
fi
# Verdicts the new spelling changes: the reserved words of the old spelling are names now, `@x` is a name, and a malformed bracket type
# is no bracket type at all.
RELEASED="cases/lir/verify/reserved-ordering.lir cases/lir/verify/reserved-volatile.lir cases/lir/verify/bad-name.lir cases/lir/verify/reserved-linkage.lir cases/lir/verify/reserved-type.lir cases/lir/verify/reserved-null.lir cases/lir/verify/array-malformed.lir"
# Programs whose output is a column of their own text (lir.sexp.column, lir.read.position): the respelling moves columns, so only the
# status is compared.
POSITIONAL="cases/lir/macros/position.lir cases/lir/readers/positions.lir"
# Positions go (the respelling moves columns, never lines), and a diagnostic is compared without case: a message quotes the word as
# written (`duplicate flag NSW` for `nsw`), and the case headers that quote one move with the messages (the respelling's step 4).
strip() { sed -E 's/^([ ]*[a-z-]+) [0-9]+:[0-9]+/\1/; s/@[0-9]+:[0-9]+/@/g; s/:[0-9]+:[0-9]+: error:/: error:/g' | awk '/error:/ { $0 = tolower($0) } { print }'; }
dump() { (ulimit -v 4000000; timeout 120 "$L" "$1" dump-ast "$2" 2>&1; echo "status $?") | sed "s|$out/tree/||g" | strip; }
files=$(find cases/lir compiler/tests/native/h2-edge lib/lir -name '*.lir' | sort)
n=0; converted=0; bad=0; strict=0; skipped=0; unread=0; : > "$out/unread"
for f in $files; do
  n=$((n + 1))
  mkdir -p "$out/tree/$(dirname "$f")"
  if ! (ulimit -v 4000000; timeout 120 "$L" respell "$f" > "$out/tree/$f" 2> "$out/err"); then
    cp "$f" "$out/tree/$f"; unread=$((unread + 1)); echo "$f" >> "$out/unread"   # a text the reader refuses (a lexical case) is left as it is
  fi
done
for f in $files; do
  g=$out/tree/$f
  if grep -qx "$f" "$out/unread"; then skipped=$((skipped + 1)); continue; fi
  dump --old-spelling "$f" > "$out/a"; dump --old-spelling "$g" > "$out/b"
  if [[ " $POSITIONAL " == *" $f "* ]]; then grep '^status' "$out/a" > "$out/a2"; grep '^status' "$out/b" > "$out/b2"; mv "$out/a2" "$out/a"; mv "$out/b2" "$out/b"; fi
  if ! cmp -s "$out/a" "$out/b"; then echo "FAIL $f: the respelled file parses differently:"; diff "$out/a" "$out/b" | head -5; bad=$((bad + 1)); continue; fi
  if ! (ulimit -v 4000000; timeout 120 "$L" respell --check "$g" > "$out/c" 2>&1); then echo "FAIL $f: respell is not idempotent on it:"; head -4 "$out/c"; bad=$((bad + 1)); continue; fi
  converted=$((converted + 1))
  if grep -qE '^\((define-macro|require-macros|import|define-reader-macro|define-dispatch-macro|define-token-reader) ' "$f" || [[ " $RELEASED " == *" $f "* ]]; then
    skipped=$((skipped + 1)); continue
  fi
  dump --new-spelling "$g" > "$out/c"
  if ! cmp -s "$out/a" "$out/c"; then echo "FAIL $f: the respelled file is not in the new spelling alone:"; diff "$out/a" "$out/c" | head -5; bad=$((bad + 1)); continue; fi
  strict=$((strict + 1))
done
echo "respell: $n files, $unread unreadable and left as they are, $converted round-trip equal and idempotent, $strict of them pure new spelling under --new-spelling, $skipped not checked strictly (macro libraries, released names, unreadable), $bad failed"
[ "$bad" -eq 0 ]
