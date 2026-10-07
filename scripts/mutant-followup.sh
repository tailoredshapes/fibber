#!/bin/bash
# scripts/mutant-followup.sh: planted faults for package FOLLOWUP-1 (parse-double and the integer parsers, unwrap-or-else and or-trap, str/replace and split-literal, trim-newline).
# Copies lib/ and the stdlib cases to a scratch directory, breaks ONE rule of the library in the copy and runs the cases that pin it with FIBC: every one must FAIL (a hung case is cut
# by the timeout and counts as failed). A case that still passes survived. The library is read from source, so no stage 2 is built.
# usage: scripts/mutant-followup.sh [MODE..]        (default: all)
#   MODE            what is broken                                                                                       cases that must fail
#   pd-tie          Eisel-Lemire: an exact halfway case rounds up, not to even                                           8280-
#   pd-junk         parse-double: trailing text after a number is accepted ("12abc")                                      8280-
#   pd-hex          parse-double: the digits scan accepts the letters a to f (hex digits)                                 8280-
#   int-range       try-parse-int: a value past the i32 range is not refused                                              8281-
#   or-else-eager   unwrap-or-else runs its thunk for a `some` too                                                        8282-
#   or-trap-msg     or-trap traps with a fixed message, not the caller's                                                   8282-
#   replace-first   str/replace-first replaces every match                                                                 8283-
#   replace-split   replace and split step into a character by one byte after an empty match (cuts a character)          8283-
#   split-limit     split-literal ignores its limit                                                                        8284-
#   trim-cr         trim-newline leaves a carriage return                                                                  8285-
# environment: FIBC (a stage 2 or the seed fibc; required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-followup), MUT_TIMEOUT (seconds per case set, default 180).
# exit: 0 when every case failed under every mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-followup: no fibc: set FIBC to an executable" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-followup}
MODES=("$@")
[ ${#MODES[@]} -gt 0 ] || MODES=(pd-tie pd-junk pd-hex int-range or-else-eager or-trap-msg replace-first replace-split split-limit trim-cr)
survived=0
ulimit -v 16000000

# mut FILE PERL-EXPRESSION (in the copy): apply, and fail the script when nothing changed (the source moved)
mut() {
  local f=$OUT/tree/$1; cp "$f" "$f.orig"; perl -0pi -e "$2" "$f"
  if cmp -s "$f" "$f.orig"; then echo "mutant-followup: the mutation of $1 changed nothing (the source moved?)" >&2; exit 2; fi
  rm "$f.orig"
}

for MODE in "${MODES[@]}"; do
  rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"; export TMPDIR=$OUT/tmp
  cp -r "$R/lib" "$OUT/tree/"; cp -r "$R/cases/stdlib" "$OUT/tree/cases/"
  case $MODE in
    pd-tie)        CASE=8280-; mut lib/fib/core/eisel.fib 's/\(= \(bit-and mant 3\) 1\)/(= (bit-and mant 3) 7)/' ;;
    pd-junk)       CASE=8280-; mut lib/fib/core/decimal.fib 's/\(or \(and has-e \(= i3 jx\)\) \(< i3 n\)\)/(and has-e (= i3 jx))/' ;;
    pd-hex)        CASE=8280-; mut lib/fib/core/decimal.fib 's/\(if \(and \(>= b 48\) \(<= b 57\)\) \(recur/(if (and (>= b 48) (<= b 102)) (recur/' ;;
    int-range)     CASE=8281-; mut lib/fib/core/parse.fib 's/\(if \(< next least\) nil \(recur \(\+ j 1\) next\)\)/(recur (+ j 1) next)/' ;;
    or-else-eager) CASE=8282-; mut lib/fib/core/opt.fib 's/\(\(some x\) x\)\n    \(nil \(f\)\)\)\)\n\n;; Clojure/((some x) (do (f) x))\n    (nil (f))))\n\n;; Clojure/' ;;
    or-trap-msg)   CASE=8282-; mut lib/fib/core/forms.fib 's/\(nil \(trap ~msg\)\)/(nil (trap "or-trap"))/' ;;
    replace-first) CASE=8283-; mut lib/fib/string/replace.fib 's/\(replace-by p s to 1\)/(replace-by p s to -1)/' ;;
    replace-split) CASE=8283-; mut lib/fib/string/pattern.fib 's/\(\+ en \(width-of \(byte-at s en\)\)\)/(+ en 1)/' ;;
    split-limit)   CASE=8284-; mut lib/fib/string/pattern.fib 's/\(\[s: str sep: str limit: i64\] -> \(Vec str\) \(split-by sep s limit\)\)/([s: str sep: str limit: i64] -> (Vec str) (split-by sep s 0))/' ;;
    trim-cr)       CASE=8285-; mut lib/fib/string/trim.fib 's/\(= \(str-byte-at s \(- j 1\)\) 13i8\)/(= (str-byte-at s (- j 1)) 0i8)/' ;;
    *) echo "mutant-followup: unknown MODE $MODE" >&2; exit 2 ;;
  esac
  cd "$OUT/tree" || exit 2
  res=$(FIB_LIB=$OUT/tree/lib timeout "${MUT_TIMEOUT:-180}" "$FIBC" cases cases/stdlib --only "$CASE" 2>&1) || true
  rows=$(printf '%s\n' "$res" | grep -E "^$CASE" | cut -c1-200 | head -1)
  if printf '%s\n' "$res" | grep -q " 0 fail"; then echo "SURVIVED  $MODE: $rows"; survived=1
  else echo "killed    $MODE: ${rows:-$CASE ($(printf '%s\n' "$res" | tail -1))}"; fi
  cd "$R" || exit 2
done
exit $survived
