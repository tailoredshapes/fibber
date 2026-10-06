#!/bin/bash
# scripts/mutant-crypto-encoding.sh: the mutation review of fib.crypto.encoding (the only code of ours in fib.crypto). Copies lib/fib/crypto to a
# scratch directory, applies ONE mutant to the copy, and runs specs/crypto-encoding-spec.fib against it (the scratch copy is first on the module
# path): at least one scenario must FAIL (or the spec must not compile or must trap). A mutant under which every scenario passes means the
# spec has a hole. Nothing in the tree changes.
#
# usage: FIBC=/path/to/fibc scripts/mutant-crypto-encoding.sh [MUTANT..]        (default: all)
#   no-padding        base64 encodes without "=" (RFC 4648 section 4)
#   url-alphabet      base64url uses + and / (the standard alphabet)
#   hex-upper-value   unhex reads a-f one too low
#   lax-trailing-bits the decoder accepts non-zero unused trailing bits
#   tail-shift        the two-byte tail of base64 takes the wrong sextet
#   decode-size       a three-character tail decodes to one byte
#   odd-hex           unhex accepts an odd number of digits
#   pad-anywhere      the decoder accepts padding without checking the length is a multiple of four
#   hex-nibble        hex writes the low nibble first
#   nopad-accepts-pad the -nopad decoder accepts "=" padding
# exit: 0 when every mutant was killed, 1 when one survived, 2 for a setup error.
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:?set FIBC to a fibc (the seed is fine: this is library code)}
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-crypto-encoding}
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(no-padding url-alphabet hex-upper-value lax-trailing-bits tail-shift decode-size odd-hex pad-anywhere hex-nibble nopad-accepts-pad)
ulimit -v 16000000 || true
rm -rf "$OUT"; mkdir -p "$OUT"

sub() { # sub FILE PERL-SUBSTITUTION: a pattern that is not found is a setup error
  cp "$1" "$1.orig"; perl -0pi -e "$2" "$1"
  cmp -s "$1" "$1.orig" && { echo "mutant-crypto-encoding: pattern not found: $2" >&2; exit 2; }
}
apply() {
  local f=$1/fib/crypto/encoding.fib
  case $2 in
    no-padding)        sub "$f" 's/\(defun base64 \(bytes: \(Array i8\)\) -> str \(b64-encode bytes std-alphabet true\)\)/(defun base64 (bytes: (Array i8)) -> str (b64-encode bytes std-alphabet false))/' ;;
    url-alphabet)      sub "$f" 's/ghijklmnopqrstuvwxyz0123456789-_/ghijklmnopqrstuvwxyz0123456789+\//' ;;
    hex-upper-value)   sub "$f" 's/\(- c 87\)/(- c 88)/' ;;
    lax-trailing-bits) sub "$f" 's/\(not \(= slack 0\)\) \(Err/(not (= slack slack)) (Err/' ;;
    tail-shift)        sub "$f" 's/\(if \(= rest 2\) \(put \(\+ j 2\) \(shr w 6\)\)/(if (= rest 2) (put (+ j 2) (shr w 5))/' ;;
    decode-size)       sub "$f" 's/\(= rest 3\) 2 :else 0/(= rest 3) 1 :else 0/' ;;
    odd-hex)           sub "$f" 's/\(if \(not \(= \(rem n 2\) 0\)\) \(Err \(invalid-argument "unhex: an odd number of digits"\)\)/(if false (Err (invalid-argument "unhex: an odd number of digits"))/' ;;
    pad-anywhere)      sub "$f" 's/\(if \(and \(= \(rem n 4\) 0\) \(not \(= \(rem m 4\) 1\)\)\) \(Pair pads m\)/(if (not (= (rem m 4) 1)) (Pair pads m)/' ;;
    hex-nibble)        sub "$f" 's/\(array-get hex-digits \(shr b 4\)\)\)\n\s+\(array-set! &out \(\+ \(\* 2 i\) 1\) \(array-get hex-digits \(bit-and b 15\)\)/(array-get hex-digits (bit-and b 15)))\n                (array-set! &out (+ (* 2 i) 1) (array-get hex-digits (shr b 4))/' ;;
    nopad-accepts-pad) sub "$f" 's/\(= pads 0\) \(if \(= \(rem m 4\) 1\) \(Pair 0 -1\) \(Pair 0 m\)\)/(not (= (rem m 4) 1)) (Pair pads m)/' ;;
    *) echo "unknown mutant $2" >&2; exit 2 ;;
  esac
}

# the unmutated copy must pass, or a "killed" mutant proves nothing
mkdir -p "$OUT/base/fib"; cp -r "$R/lib/fib/crypto" "$R/lib/fib/crypto.fib" "$OUT/base/fib/"
"$FIBC" test "$R/specs/crypto-encoding-spec.fib" -I "$OUT/base" > "$OUT/base.log" 2>&1 || { echo "mutant-crypto-encoding: the unmutated copy fails: see $OUT/base.log" >&2; exit 2; }
echo "unmutated: $(grep '^total:' "$OUT/base.log")"
survived=0
for m in "${MUTANTS[@]}"; do
  d=$OUT/$m; mkdir -p "$d/fib"; cp -r "$R/lib/fib/crypto" "$R/lib/fib/crypto.fib" "$d/fib/"
  apply "$d" "$m"
  if "$FIBC" test "$R/specs/crypto-encoding-spec.fib" -I "$d" > "$d.log" 2>&1; then
    echo "SURVIVED  $m"; survived=1
  else
    echo "killed    $m  ($(grep -c '^  FAIL\|^  TRAP' "$d.log") failing, $(grep '^total:' "$d.log" | sed 's/^total: //; s/ in .*//'))"
  fi
done
exit $survived
