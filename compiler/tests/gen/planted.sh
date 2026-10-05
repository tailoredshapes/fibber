#!/bin/bash
# Planted faults of the fibgen port (docs/design/fibgen-port.md 5.2 rule 3): each fault is an exact one-place change to a source file; the script applies it, runs the check that must
# catch it, requires that check to FAIL, restores the file and requires the restored file to equal the saved copy. A fault that does not fail its check is a check that cannot fail.
# usage: planted.sh [NAME..]       (no name: all)     FIBC names a stage 2 built from the tree; TMPDIR is the scratch directory.
# Faults:  draw-order    two draws of lambdas.fib swapped                      caught by compare.sh pipelines
#          size-rule     the source length bound 3 + 2*size becomes 4 + 2*size    caught by compare.sh pipelines
#          escape        the printer's backslash escape loses a backslash      caught by unit-print
#          verdict       the model's filter keeps the elements it should drop  caught by compare.sh pipelines (model column) and unit-model-pipelines
#          take-ahead    `take n` gives one element more (pulls its source once more)   caught by unit-model-pipelines
#          width         the layout width 80 becomes 79                         caught by texts-check.sh
#          send          (ty-send? (YCell _)) becomes true                      caught by unit-types
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
fibc=${FIBC:-fibc}
export FIB_LIB=${FIB_LIB:-$root/lib}
tmp=${TMPDIR:-/tmp}/planted-$$; mkdir -p "$tmp"; trap 'rm -rf "$tmp"' EXIT
cd "$root" || exit 2

build_tool() { "$fibc" build compiler/fibgen.fib -I compiler -I lib -o "$tmp/fibgen" 2> "$tmp/build.err" || { cat "$tmp/build.err"; return 1; }; }

# fault NAME FILE OLD NEW CHECK...   (CHECK is a command run after building; it must fail)
fault() {
  local name=$1 file=$2 old=$3 new=$4; shift 4
  cp "$file" "$tmp/saved"
  python3 - "$file" "$old" "$new" <<'EOF' || { echo "planted $name: the text to change was not found in $file"; return 2; }
import sys
f, old, new = sys.argv[1:4]
s = open(f).read()
if s.count(old) != 1:
    sys.exit(1)
open(f, 'w').write(s.replace(old, new))
EOF
  local rc
  if [ "$1" = unit ]; then "$here/unit.sh" "$2" "$tmp" > "$tmp/out" 2>&1; rc=$?
  else build_tool > "$tmp/out" 2>&1 && { "$@" > "$tmp/out" 2>&1; rc=$?; } || rc=9; fi
  cp "$tmp/saved" "$file"
  cmp -s "$tmp/saved" "$file" || { echo "planted $name: RESTORE FAILED"; return 2; }
  head -3 "$tmp/out"
  if [ $rc -ne 0 ]; then echo "planted $name: caught (check exit $rc), file restored"; return 0; fi
  echo "planted $name: NOT CAUGHT"; return 1
}

want=${*:-draw-order size-rule escape verdict take-ahead width send}
status=0
for f in $want; do
  case $f in
    draw-order) fault $f compiler/gen/lambdas.fib '(let ((k (lam-int g lo hi)) (a (lambdas-term g (- d 1) params)))' '(let ((a (lambdas-term g (- d 1) params)) (k (lam-int g lo hi)))' \
                  "$here/compare.sh" "$tmp/fibgen" pipelines 1 60 ;;
    size-rule)  fault $f compiler/gen/pipelines.fib '(hi (+ 3 (* 2 (. g size))))' '(hi (+ 4 (* 2 (. g size))))' "$here/compare.sh" "$tmp/fibgen" pipelines 1 60 ;;
    escape)     fault $f compiler/gen/print/sexp.fib '(= n 92i32) "\\\\"' '(= n 92i32) "\\"' unit print ;;
    verdict)    fault $f compiler/gen/model/pipelines.fib '(if (= b keep) (Ok (some v))' '(if (not (= b keep)) (Ok (some v))' "$here/compare.sh" "$tmp/fibgen" pipelines 1 60 ;;
    take-ahead) fault $f compiler/gen/model/pipelines.fib '(if (<= @left 0)' '(if (< @left 0)' unit model-pipelines ;;
    width)      fault $f compiler/gen/print/sexp.fib '(if (<= (+ indent (count one)) 80)' '(if (<= (+ indent (count one)) 79)' "$here/texts-check.sh" "$tmp/fibgen" pipelines 150 ;;
    send)       fault $f compiler/gen/ty.fib '((YHolder) false) ((YCell x) false)' '((YHolder) false) ((YCell x) true)' unit types ;;
    *) echo "planted: unknown fault $f"; status=2 ;;
  esac
  rc=$?; [ $rc -gt $status ] && status=$rc
done
[ $status -eq 0 ] && echo "planted: all caught, all restored"
exit $status
