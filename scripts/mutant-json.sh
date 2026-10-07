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
#   JSON-3 (each one planted in the piece that was added; the spec that guards it is named):
#   doc-comma         docwrite: no comma before the second element of an array                (specs/json-doc-spec.fib)
#   doc-colon         docwrite: the colon after a key is a comma                              (specs/json-doc-spec.fib)
#   doc-string-end    docwrite: the closing quote of a string is written one byte late        (specs/json-doc-spec.fib)
#   doc-close-kind    docwrite: an object is closed with ] and an array with }                (specs/json-doc-spec.fib)
#   doc-count         docwrite: a container that closes does not count as a child of its parent (specs/json-doc-spec.fib)
#   enc-vec-comma     defjson encoder: no comma between the elements of a Vec after the second (specs/json-encode-spec.fib)
#   enc-key-comma     defjson encoder: no comma before the second field                       (specs/json-encode-spec.fib)
#   enc-option-null   defjson encoder: a missing Option is `false`, not `null`                (specs/json-encode-spec.fib)
#   enc-finish        defjson encoder: the last byte of the result is lost                    (specs/json-encode-spec.fib)
#   stream-eof-merge  lines-seq: a document that does not end in its chunk is an error, not completed by reading on (specs/json-stream-spec.fib)
#   stream-skip       lines-seq: the cursor after a long line is one byte too early                        (specs/json-stream-spec.fib)
#   stream-order      reduce-lines-par-map: the results of a chunk are folded in reverse order (specs/json-stream-spec.fib)
#   stream-head       reduce-lines-par-map: the line that straddles two reads is dropped      (specs/json-stream-spec.fib)
#   utf8-fast-surrogate  UTF-8 fast path: ED is taken as an ordinary three-byte lead          (specs/json-stream-spec.fib)
#   tol-line-comment  tolerant: a // comment ends at a carriage return, not a newline          (specs/json-stream-spec.fib)
#   tol-array-comma   tolerant: a trailing comma before ] is not accepted                     (specs/json-stream-spec.fib)
#   tol-default-on    tolerant: comments are accepted by the default options                    (specs/json-stream-spec.fib)
#   tol-nan           tolerant: NaN is spelled NaX                                             (specs/json-stream-spec.fib)
#   index-first       ObjIndex: every lookup answers the first value                           (specs/json-stream-spec.fib)
#   sink-chunk        write-sink: a piece is 65537 bytes                                       (specs/json-stream-spec.fib)
#   dtoa-pairs        float printing: the two digits of a pair are swapped                     (specs/json-floats-spec.fib json-prop-spec.fib)
#   dtoa-zeros        float printing: one zero too few after `0.` for a small number           (specs/json-floats-spec.fib json-prop-spec.fib)
#   plain-tail        writer: the overlapping last word of a string is read one byte early (the last byte is never tested)       (specs/json-prop-spec.fib)
# environment: FIBC, MUT_OUT (scratch). exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-json: no stage 2 fibc: set FIBC" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-json}
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(escape-wrong swar-control simd-boundary simd-backslash simd-instring simd-esc-carry simd-token-carry simd-control write-vec-control lines-col lines-number lines-newline codec-path-index codec-missing-rec write-indent write-comma fast-comma-state fast-number-delim fast-close-kind fast-escape-flag fast-depth fast-string-end float-tie float-print clinger depth-off-by-one tape-depth leading-zero utf8-overlong lone-surrogate dup-last-wins doc-comma doc-colon doc-string-end doc-close-kind doc-count enc-vec-comma enc-key-comma enc-option-null enc-finish stream-eof-merge stream-skip stream-order stream-head utf8-fast-surrogate tol-line-comment tol-array-comma tol-default-on tol-nan index-first sink-chunk dtoa-pairs dtoa-zeros plain-tail)

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
    write-comma)      f=$j/writefast.fib; cp $f $f.orig; SPECS="json-spec json-prop-spec"; sub $f 's/\(if first pos \(wchar b cap pos 44\)\)/(if first pos (wchar b cap pos 59))/; s/\(wbyte b pos 44\)/(wbyte b pos 59)/' ;;
    fast-comma-state) f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(and \(= st 1\) \(> dp 0\)\)/(and (or (= st 1) (= st 0)) (> dp 0))/' ;;
    fast-number-delim) f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(and \(>= r 0\) \(or \(= e n\) \(or \(= e next\) \(ws\? \(byte-at p e\)\)\)\)\)/(>= r 0)/' ;;
    fast-close-kind)  f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(= arr \(= c 93\)\)/true/' ;;
    fast-escape-flag) f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(!= \(bit-and m \(- \(shl 1 w\) 1\)\) 0\)/false/; s/\(if \(= m 0\) \(recur \(\+ i 32\)\) true\)/(recur (+ i 32))/' ;;
    fast-depth)       f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(< dp max-depth\)/(<= dp max-depth)/' ;;
    fast-string-end)  f=$j/tapefast.fib; cp $f $f.orig; SPECS="json-fast-spec"; sub $f 's/\(node k-str \(\+ a 1\)\) b\)/(node k-str (+ a 1)) (+ b 1))/' ;;
    float-tie)        f=$j/eisel.fib; cp $f $f.orig; SPECS="json-floats-spec"; sub $f 's/\(= \(bit-and mant 3\) 1\)/(= (bit-and mant 3) 7)/' ;;
    float-print)      f=$j/dtoa.fib; cp $f $f.orig; SPECS="json-floats-spec json-prop-spec"; sub $f 's/upin \(<= \(\+ vbl out\) \(shl sp10 2\)\)/upin (<= (+ vbl 1) (shl sp10 2))/' ;;
    clinger)          f=$j/number.fib; cp $f $f.orig; SPECS="json-floats-spec"; sub $f 's/\(<= nd 15\)/(<= nd 18)/; s/\(<= w 9007199254740992\)/(<= w 900719925474099200)/' ;;
    depth-off-by-one) f=$j/parse.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(>= depth \(\. o max-depth\)\)/(> depth (. o max-depth))/g' ;;
    tape-depth)       f=$j/tapebuild.fib; cp $f $f.orig; SPECS="json-api-spec json-testsuite-spec"; sub $f 's/\(>= \@dp max-depth\)/(> @dp max-depth)/' ;;
    leading-zero)     f=$j/number.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(and \(> d1 1\) \(= \(at s n i0\) 48\)\)/(and false (= (at s n i0) 48))/' ;;
    utf8-overlong)    f=$j/utf8.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(< b0 194\) 0/(< b0 192) 0/' ;;
    lone-surrogate)   f=$j/parse.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(and \(>= u 56320\) \(< u 57344\)\) \(do \(fail-at/(and false (< u 57344)) (do (fail-at/' ;;
    dup-last-wins)    f=$j/parse.fib; cp $f $f.orig; SPECS="json-spec"; sub $f 's/\(assoc vals dup \@vc\)/vals/' ;;
    doc-comma)        f=$j/docwrite.fib; cp $f $f.orig; SPECS="json-doc-spec"; sub $f 's/\(if \(> c 0\) \(do \(dw-put o q 44\) \(\+ q 1\)\) q\)\)\)\)\)/(if (> c 1) (do (dw-put o q 44) (+ q 1)) q)))))/' ;;
    doc-colon)        f=$j/docwrite.fib; cp $f $f.orig; SPECS="json-doc-spec"; sub $f 's/\(do \(dw-put o q 58\) \(\+ q 1\)\)/(do (dw-put o q 44) (+ q 1))/' ;;
    doc-string-end)   f=$j/docwrite.fib; cp $f $f.orig; SPECS="json-doc-spec"; sub $f 's/\(dw-put o \(\+ q1 \(\+ \(- b a\) 1\)\) 34\)/(dw-put o (+ q1 (+ (- b a) 2)) 34)/' ;;
    doc-close-kind)   f=$j/docwrite.fib; cp $f $f.orig; SPECS="json-doc-spec"; sub $f 's/\(if \(= \(sk-get sk \(- dp 1\) 1\) 1\) 125 93\)/(if (= (sk-get sk (- dp 1) 1) 1) 93 125)/' ;;
    doc-count)        f=$j/docwrite.fib; cp $f $f.orig; SPECS="json-doc-spec"; sub $f 's/\(dw-count sk \(- dp 1\)\)\n/(do)\n/' ;;
    enc-vec-comma)    f=$j/codecform.fib; cp $f $f.orig; SPECS="json-encode-spec"; sub $f 's/\(if \(> ~i 0\) \(fib.json.writefast\/wchar b cap pp 44\) pp\)/(if (> ~i 1) (fib.json.writefast\/wchar b cap pp 44) pp)/' ;;
    enc-key-comma)    f=$j/codec.fib; cp $f $f.orig; SPECS="json-encode-spec"; sub $f 's/\(if \(= i 0\) "\\"" ",\\""\)/(if (< i 2) "\\"" ",\\"")/' ;;
    enc-option-null)  f=$j/codecform.fib; cp $f $f.orig; SPECS="json-encode-spec"; sub $f 's/\(fib.json.writefast\/wnull b cap pp\)/(fib.json.writefast\/wbool b cap pp false)/' ;;
    enc-finish)       f=$j/codec.fib; cp $f $f.orig; SPECS="json-encode-spec"; sub $f 's/\(ptr\+ \(raw ba\) 24\) n\)\) out\)\)/(ptr+ (raw ba) 24) (- n 1))) out))/' ;;
    stream-eof-merge) f=$j/stream.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(= \(\. er code\) e-eof\)/false/' ;;
    stream-skip)      f=$j/stream.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(some v\) \(some \(Pair \(Ok v\) \(at-pos st e\)\)\)/(some v) (some (Pair (Ok v) (at-pos st (- e 1))))/' ;;
    stream-order)     f=$j/stream.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(reduce f acc rs\)/(reduce f acc (reverse rs))/' ;;
    stream-head)      f=$j/stream.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(fold-lines-of head 0 \(array-len head\) o g f acc\) acc\)/acc acc)/' ;;
    utf8-fast-surrogate) f=$j/utf8.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(and \(!= b0 237\) \(< \(\+ j 2\) n\)\)/(< (+ j 2) n)/' ;;
    tol-line-comment) f=$j/parse.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(= \(at s n m\) 10\)\) m \(recur/(= (at s n m) 13)) m (recur/' ;;
    tol-array-comma)  f=$j/parse.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(and \(tol-commas\? o\) \(= \(at s n p2\) 93\)\)/(and false (= (at s n p2) 93))/' ;;
    tol-default-on)   f=$j/parse.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(!= \(bit-and \(\. o numbers\) 16\) 0\)/(!= (bit-and (. o numbers) 16) 1)/' ;;
    tol-nan)          f=$j/parse.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/"NaN" \(JFloat/"NaX" (JFloat/' ;;
    index-first)      f=$j/access.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(some \(nth \(\. x vals\) i\)\)/(some (nth (. x vals) 0))/' ;;
    sink-chunk)       f=$j/io.fib; cp $f $f.orig; SPECS="json-stream-spec"; sub $f 's/\(min n \(\+ i 65536\)\)/(min n (+ i 65537))/' ;;
    dtoa-pairs)       f=$j/dtoa.fib; cp $f $f.orig; SPECS="json-floats-spec json-prop-spec"; sub $f 's/\(dput p k \(zext i64 \(str-byte-at digit-pairs \(\+ \(\* r 2\) 1\)\)\)\)/(dput p k (zext i64 (str-byte-at digit-pairs (* r 2))))/' ;;
    dtoa-zeros)       f=$j/dtoa.fib; cp $f $f.orig; SPECS="json-floats-spec json-prop-spec"; sub $f 's/\(< i \(\+ at \(- 1 e\)\)\)/(< i (+ at (- 0 e)))/' ;;
    plain-tail)       f=$j/escape.fib; cp $f $f.orig; SPECS="json-prop-spec"; sub $f 's/\(\+ 24 \(- n 8\)\)/(+ 24 (- n 9))/' ;;
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
