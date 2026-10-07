#!/bin/bash
# scripts/mutant-json-schema.sh: planted faults in fib.json.schema (docs/design/json-schema.md). Copies lib/ of this tree to a scratch directory,
# applies ONE mutant, runs specs/json-schema-spec.fib with a stage 2 (FIBC; default the gate's F of this tree) pointing FIB_LIB at the copy: a
# scenario must FAIL (a wrong answer, a trap or a build error all count). A mutant under which the spec passes is a part no test can fail.
# usage: scripts/mutant-json-schema.sh [MUTANT..]        (default: all)
#   number-spelling   value: 1 and 1.0 are different numbers                   float-integer     value: no float is an integer
#   multiple-exact    value: multipleOf allows no rounding (0.3 / 0.1)    required-ignored  scalar: a missing required property passes
#   length-bytes      scalar: string lengths count bytes                        ipath-wrong       eval: a violation's location loses its last step
#   ref-ignored       eval: $ref is not followed                                 if-swapped        eval: then and else are swapped
#   oneof-as-anyof    eval: oneOf accepts more than one match                    additional-pats   eval: additionalProperties ignores patternProperties
#   id-ignored        index: $id names nothing                                   meta-missing      api: the draft-07 meta-schema is not built in
#   leap-anytime      formats: a leap second is valid at any time                formats-off       types: formats are not asserted by default
set -uo pipefail
R=$(cd "$(dirname "$0")/.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "mutant-json-schema: no stage 2 fibc: set FIBC" >&2; exit 2; }
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-json-schema}
MUTANTS=("$@")
[ ${#MUTANTS[@]} -gt 0 ] || MUTANTS=(number-spelling float-integer multiple-exact required-ignored length-bytes ipath-wrong ref-ignored if-swapped
                                     oneof-as-anyof additional-pats id-ignored meta-missing leap-anytime formats-off)

sub() { perl -0pi -e "$2" "$1"; cmp -s "$1" "$1.orig" && { echo "mutant-json-schema: pattern not found in $1: $2" >&2; return 1; }; return 0; }
mutate() { # mutate NAME TREE
  local f d=$2/lib/fib/json/schema
  case $1 in
    number-spelling)  f=$d/value.fib; cp $f $f.orig; sub $f 's/\(if \(and \(number\? x\) \(number\? y\)\) \(= \(compare-numbers x y\) 0\)/(if false (= (compare-numbers x y) 0)/' ;;
    float-integer)    f=$d/value.fib; cp $f $f.orig; sub $f 's/\(\(JFloat f\) \(whole-float\? f\)\)/((JFloat f) false)/' ;;
    multiple-exact)   f=$d/value.fib; cp $f $f.orig; sub $f 's/\(\* 1\.0e-9 \(if/(* 0.0 (if/' ;;
    required-ignored) f=$d/scalar.fib; cp $f $f.orig; sub $f 's/\(if \(some\? \(a\/json-get obj name\)\) o/(if true o/' ;;
    length-bytes)     f=$d/scalar.fib; cp $f $f.orig; sub $f 's/\(count \(str-chars s\)\)/(str-len s)/' ;;
    ipath-wrong)      f=$d/eval.fib; cp $f $f.orig; sub $f 's/\(str \(\. at ipath\) "\/" \(pointer-token token\)\)/(str (. at ipath) "\/")/' ;;
    ref-ignored)      f=$d/eval.fib; cp $f $f.orig; sub $f 's/\(match \(a\/json-get node "\$ref"\)/(match (a\/json-get node "\$nope")/' ;;
    if-swapped)       f=$d/eval.fib; cp $f $f.orig; sub $f 's/base depth\)\) "then" "else"\)/base depth)) "else" "then")/' ;;
    oneof-as-anyof)   f=$d/eval.fib; cp $f $f.orig; sub $f 's/\(!= \(valid-count "oneOf"\) 1\)/(< (valid-count "oneOf") 1)/' ;;
    additional-pats)  f=$d/eval.fib; cp $f $f.orig; sub $f 's/\(some\? \(first \(filter \(fn \(p: \(Pair str Json\)\) \(matches\? s \(\. p fst\) \(\. m fst\)\)\) pats\)\)\)\)/false)/' ;;
    id-ignored)       f=$d/index.fib; cp $f $f.orig; sub $f 's/\(match \(str-member j "\$id"\)/(match (str-member j "\$nope")/' ;;
    meta-missing)     f=$d/api.fib; cp $f $f.orig; sub $f 's/\(match \(p\/parse \(meta\/draft-07-text\)\) \(\(Ok m\) \(ix\/walk \(ix\/Index \{meta\/draft-07-uri m\} \[\] \[\]\) m meta\/draft-07-uri 0\)\) \(\(Err _\) \(ix\/empty-index\)\)\)/(ix\/empty-index)/' ;;
    leap-anytime)     f=$d/formats.fib; cp $f $f.orig; sub $f 's/\(or \(< se 60\) \(and \(= h 23\) \(= mi 59\)\)\)/true/' ;;
    formats-off)      f=$d/types.fib; cp $f $f.orig; sub $f 's/\(SchemaOptions \{\} true\)/(SchemaOptions {} false)/' ;;
    *) echo "mutant-json-schema: unknown mutant $1" >&2; return 1 ;;
  esac
}

survived=0
for m in "${MUTANTS[@]}"; do
  t=$OUT/$m; rm -rf "$t"; mkdir -p "$t"; cp -r "$R/lib" "$t/lib"
  mutate "$m" "$t" || { echo "SETUP ERROR $m"; exit 2; }
  log=$t/spec.log
  (cd "$R" && FIB_LIB=$t/lib "$FIBC" test specs/json-schema-spec.fib > "$log" 2>&1) && rc=0 || rc=$?
  if [ $rc -ne 0 ] || grep -q "^  FAIL\|^  TRAP\|ERROR" "$log"; then echo "killed   $m   ($(grep -m1 '^  FAIL\|^  TRAP\|ERROR' "$log" | sed 's/^ *//'))"
  else echo "SURVIVED $m"; survived=1; fi
  rm -rf "$t/lib"
done
[ $survived = 0 ]
