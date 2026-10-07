#!/bin/bash
# scripts/mutant-json.sh: planted faults in fib.json (docs/design/json.md 6). Copies lib/ of this tree to a scratch directory, applies ONE mutant, runs the specs that
# guard that part with a stage 2 (FIBC; default the gate's F of this tree) pointing FIB_LIB at the copy: at least one scenario must FAIL (a wrong answer, a trap or a build
# error all count). A mutant under which every spec passes means a part has no test that can fail.
# usage: scripts/mutant-json.sh [MUTANT..]        (default: all)
#   escape-wrong      writer: the escape of a newline is `\m`                          (specs/json-spec.fib, json-prop-spec.fib)
#   swar-control      writer: the eight-byte scan lets byte 0x1f through (< 31)          (specs/json-prop-spec.fib)
#   simd-boundary     stage 1: a block that ends exactly at the end of the text is read from the padding (off by one at the tail)  (specs/json-simd-spec.fib)
#   simd-backslash    stage 1: the odd-backslash-run mask loses the even/odd parity      (specs/json-simd-spec.fib)
#   simd-instring     stage 1: the in-string state is not carried into the next block   (specs/json-simd-spec.fib)
#   simd-esc-carry    stage 1: an odd run of backslashes ending a block does not escape the first byte of the next  (specs/json-simd-spec.fib)
#   simd-token-carry  stage 1: a token that crosses a block boundary starts again in the next block  (specs/json-simd-spec.fib)
#   simd-control      stage 1: a control byte inside a string is not flagged             (specs/json-simd-spec.fib)
#   write-vec-control writer: the 32-byte vector scan misses bytes 0x10 to 0x1f in a string of 32 bytes or more (specs/json-prop-spec.fib)
#   lines-col         lines: an error's column ignores the indentation of its line      (specs/json-lines-spec.fib)
#   lines-number      lines: an error's line number is one too high                      (specs/json-lines-spec.fib)
#   lines-newline     lines: the vector newline search lands one byte late               (specs/json-lines-spec.fib)
#   codec-path-index  defjson: an element's index in the mismatch path is always 0       (specs/json-codec-spec.fib)
#   codec-missing-rec defjson: a missing required record field is decoded (traps)       (specs/json-codec-spec.fib)
#   write-indent      writer: the indentation of a level is one space                  (specs/json-spec.fib)
#   write-comma       writer: the comma between the second and third element is missing (specs/json-spec.fib json-prop-spec.fib)
#   fast-comma-state  fast tape: a comma is accepted where a value is expected          (specs/json-fast-spec.fib)
#   fast-number-delim fast tape: the byte after a number is not checked (1x)           (specs/json-fast-spec.fib)
#   fast-close-kind   fast tape: a ] may close an object and a } an array              (specs/json-fast-spec.fib)
#   fast-escape-flag  fast tape: a string with a backslash is not told apart (no escape flag, no escape check)  (specs/json-fast-spec.fib)
#   fast-depth        fast tape: the depth limit is `<=`                                (specs/json-fast-spec.fib)
#   fast-string-end   fast tape: the end of a string is one past its closing quote      (specs/json-fast-spec.fib)
#   float-tie         Eisel-Lemire: an exact halfway case rounds up, not to even         (specs/json-floats-spec.fib)
#   float-print       Schubfach: the lower interval bound is exclusive for even mantissas (specs/json-floats-spec.fib)
#   clinger           Clinger fast path accepts a mantissa above 2^53                    (specs/json-floats-spec.fib)
#   depth-off-by-one  parser: nesting limit is `>` where it must be `>=`                 (specs/json-spec.fib)
#   tape-depth        tape reader: the same                                              (specs/json-api-spec.fib)
#   leading-zero      number grammar: a leading zero is accepted                         (specs/json-spec.fib)
#   utf8-overlong     UTF-8 check: C0 and C1 lead bytes are accepted                     (specs/json-spec.fib)
#   lone-surrogate    a lone low surrogate escape is accepted                            (specs/json-spec.fib)
#   dup-last-wins     duplicate keys: the first value wins                               (specs/json-spec.fib)
# environment: FIBC, MUT_OUT (scratch). exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-json: no stage 2 fibc: set FIBC" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-json}
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(escape-wrong swar-control simd-boundary simd-backslash simd-instring simd-esc-carry simd-token-carry simd-control write-vec-control lines-col lines-number lines-newline codec-path-index codec-missing-rec write-indent write-comma fast-comma-state fast-number-delim fast-close-kind fast-escape-flag fast-depth fast-string-end float-tie float-print clinger depth-off-by-one tape-depth leading-zero utf8-overlong lone-surrogate dup-last-wins)

sub() { perl -0pi -e "$2" "$1"; cmp -s "$1" "$1.orig" && { echo "mutant-json: pattern not found in $1: $2" >&2; return 1; }; return 0; }
mutate() { # mutate NAME TREE -> sets SPECS
  local f t=$2; local j=$t/lib/fib/json
  case $1 in
    escape-wrong)     f=$j/escape.fib; cp $f $f.orig; SPECS="json-spec json-prop-spec"; sub $f 's/\(= c 10\) 110/(= c 10) 109/' ;;
    swar-control)     f=$j/escape.fib; cp $f $f.orig; SPECS="json-prop-spec json-spec"; sub $f 's/\(unchecked-multiply ones 32\)/(unchecked-multiply ones 31)/' ;;
    simd-boundary)    f=$j/stage1.fib; cp $f $f.orig; SPECS="json-simd-spec"; sub $f 's/\(<= \(\+ i 64\) n\)/(< (+ i 64) n)/' ;;
    simd-backslash)   f=$j/stage1.fib; cp $f $f.orig; SPECS="json-simd-spec"; sub $f 's/\(bit-not even-bits\)\) \(bit-not follows\)/even-bits) (bit-not follows)/' ;;
    simd-instring)    f=$j/stage1.fib; cp $f $f.orig; SPECS="json-simd-spec"; sub $f 's/\(sar inq 63\)/0/' ;;
    simd-esc-carry)   f=$j/stage1.fib; cp $f $f.orig; SPECS="json-simd-spec"; sub $f 's/\(esc-carry bs carry\) \(sar inq 63\)/0 (sar inq 63)/' ;;
    simd-token-carry) f=$j/stage1.fib; cp $f $f.orig; SPECS="json-simd-spec"; sub $f 's/\(shr tok 63\)/0/' ;;
    simd-control)     f=$j/stage1.fib; cp $f $f.orig; SPECS="json-simd-spec"; sub $f 's/\(bit-and \(ctl-mask v\) inq\)/0/' ;;
    write-vec-control) f=$j/escape.fib; cp $f $f.orig; SPECS="json-prop-spec"; sub $f 's/-32i8\)\) \(splat \(Simd i8 32\) 0i8\)/-16i8)) (splat (Simd i8 32) 0i8)/' ;;
    lines-col)        f=$j/lines.fib; cp $f $f.orig; SPECS="json-lines-spec"; sub $f 's/\(\+ \(. e column\) \(- start bol\)\)/(. e column)/' ;;
    lines-number)     f=$j/lines.fib; cp $f $f.orig; SPECS="json-lines-spec"; sub $f 's/\(\+ no \(- \(. e line\) 1\)\)/(+ no (. e line))/' ;;
    lines-newline)    f=$j/lines.fib; cp $f $f.orig; SPECS="json-lines-spec"; sub $f 's/\(if \(= m 0\) \(recur \(\+ i 32\)\) \(\+ i \(ctz m\)\)\)/(if (= m 0) (recur (+ i 32)) (+ i (ctz m) 1))/' ;;
    codec-path-index) f=$j/codec.fib; cp $f $f.orig; SPECS="json-codec-spec"; sub $f 's/\(recur \(next-node d j\) \(\+ k 1\)\)/(recur (next-node d j) k)/' ;;
    codec-missing-rec) f=$j/codec.fib; cp $f $f.orig; SPECS="json-codec-spec"; sub $f 's/\(if \(< i 0\) nil ~built\)/~built/' ;;
    write-indent)     f=$j/writefast.fib; cp $f $f.orig; SPECS="json-spec json-prop-spec"; sub $f 's/\(\* level \(\. w indent\)\)/(* level 1)/' ;;
    write-comma)      f=$j/writefast.fib; cp $f $f.orig; SPECS="json-spec json-prop-spec"; sub $f 's/\(if \(> i 0\) \(wchar b cap q 44\) q\)/(if (> i 1) (wchar b cap q 44) q)/g' ;;
    fast-comma-state) f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(and \(= st 1\) \(> dp 0\)\)/(and (or (= st 1) (= st 0)) (> dp 0))/' ;;
    fast-number-delim) f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(and \(>= e 0\) \(or \(= e n\) \(or \(= e next\) \(ws\? \(byte-at p e\)\)\)\)\)/(>= e 0)/' ;;
    fast-close-kind)  f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(= arr \(= c 93\)\)/true/' ;;
    fast-escape-flag) f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(!= \(bit-and m \(- \(shl 1 w\) 1\)\) 0\)/false/; s/\(if \(= m 0\) \(recur \(\+ i 32\)\) true\)/(recur (+ i 32))/' ;;
    fast-depth)       f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(< dp max-depth\)/(<= dp max-depth)/' ;;
    fast-string-end)  f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(node k-str \(\+ a 1\)\) b\)/(node k-str (+ a 1)) (+ b 1))/' ;;
    float-tie)        f=$t/lib/fib/core/eisel.fib; cp $f $f.orig; SPECS="json-floats-spec"; sub $f 's/\(= \(bit-and mant 3\) 1\)/(= (bit-and mant 3) 7)/' ;;
    float-print)      f=$j/dtoa.fib; cp $f $f.orig; SPECS="json-floats-spec json-prop-spec"; sub $f 's/upin \(<= \(\+ vbl out\) \(shl sp10 2\)\)/upin (<= (+ vbl 1) (shl sp10 2))/' ;;
    clinger)          f=$j/number.fib; cp $f $f.orig; SPECS="json-floats-spec"; sub $f 's/\(<= nd 15\)/(<= nd 18)/; s/\(<= w 9007199254740992\)/(<= w 900719925474099200)/' ;;
    depth-off-by-one) f=$j/parse.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(>= depth \(\. o max-depth\)\)/(> depth (. o max-depth))/g' ;;
    tape-depth)       f=$j/tapebuild.fib; cp $f $f.orig; SPECS="json-api-spec json-testsuite-spec"; sub $f 's/\(>= \@dp max-depth\)/(> @dp max-depth)/' ;;
    leading-zero)     f=$j/number.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(and \(> d1 1\) \(= \(at s n i0\) 48\)\)/(and false (= (at s n i0) 48))/' ;;
    utf8-overlong)    f=$j/utf8.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(< b0 194\) 0/(< b0 192) 0/' ;;
    lone-surrogate)   f=$j/parse.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(and \(>= u 56320\) \(< u 57344\)\) \(do \(fail-at/(and false (< u 57344)) (do (fail-at/' ;;
    dup-last-wins)    f=$j/parse.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(assoc vals dup \@vc\)/vals/' ;;
    *) echo "mutant-json: unknown mutant $1" >&2; return 1 ;;
  esac
}

survived=0
for m in "${MUTANTS[@]}"; do
  t=$OUT/$m; rm -rf "$t"; mkdir -p "$t"; cp -r "$R/lib" "$t/lib"
  SPECS=""
  mutate "$m" "$t" || { echo "SETUP ERROR $m"; exit 2; }
  killed=0; why=""
  for s in $SPECS; do
    log=$t/$s.log
    (cd "$R" && FIB_LIB=$t/lib "$FIBC" test "specs/$s.fib" > "$log" 2>&1) && rc=0 || rc=$?
    if [ $rc -ne 0 ] || grep -q "^  FAIL\|^  TRAP\|ERROR" "$log"; then killed=1; why="$s: $(grep -m1 '^  FAIL\|^  TRAP\|ERROR' "$log" | sed 's/^ *//')"; break; fi
  done
  if [ $killed = 1 ]; then echo "killed   $m   ($why)"; else echo "SURVIVED $m   (every spec passed: $SPECS)"; survived=1; fi
  rm -rf "$t/lib"
done
[ $survived = 0 ]
