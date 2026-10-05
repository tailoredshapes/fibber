#!/bin/bash
# compiler/tests/adr/run.sh: the executable-ADR tool checked against itself (docs/design/executable-adrs.md §8). It builds compiler/adr.fib
# and judges what the tool reports, so a tool that stopped noticing a violation fails here:
#   1. the tree's own ADRs: every one accepted, exit 0, in a few seconds;
#   2. the light reader of fib.test.arch agrees with the compiler's reader on every source file (reader-agree.fib);
#   3. a scratch copy of the tree with one real violation planted for each pilot ADR: each is reported `violated`, exit 1, and the
#      violation is named; the same copy with the plants removed is accepted again (the tree itself is never touched);
#   4. synthetic ADRs for the rules of the tool: a failing spec, a rule that cannot fail, a rule with no plant, a block that does not
#      compile (with the Markdown line), no blocks (unverified; --strict makes it exit 1), superseded and proposed (not run, even
#      with a failing block), a missing Status, a block outside Governance, --only, --status, --format json, --stamp (idempotent, one line);
#   5. exit 2 for a usage error.
# usage: compiler/tests/adr/run.sh        environment: FIBC (a stage 2; default the tree's gate F), SCRATCH (default ~/.cache/fibber-scratch/adr-test)
# Exit 0 when every check held.
set -uo pipefail
R=$(cd "$(dirname "$0")/../../.." && pwd)
FIBC=${FIBC:-$HOME/.cache/fibber-scratch/gate-$(basename "$R")/F}
[ -x "$FIBC" ] || { echo "adr test: no fibc: set FIBC" >&2; exit 2; }
S=${SCRATCH:-$HOME/.cache/fibber-scratch/adr-test}
rm -rf "$S"; mkdir -p "$S/tmp"; export TMPDIR=$S/tmp FIBC FIB_LIB=$R/lib
ulimit -v 16000000
fails=0
check() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fails=$((fails+1)); fi; }
status_of() { echo "$1" | awk -v n="$2" '$1==n && $2!="violated:" && $2!="invalid:" {print $2; exit}'; }

(cd "$R" && "$FIBC" build compiler/adr.fib -I compiler -I lib -o "$S/adr") || { echo "adr test: build failed" >&2; exit 2; }
ADR=$S/adr

# 1. the tree's own ADRs
t0=$(date +%s.%N)
out=$(cd "$R" && "$ADR"); st=$?
t1=$(date +%s.%N)
echo "$out"
check "1 the tree's ADRs: exit status" "$st" 0
check "1 the tree's ADRs: all accepted" "$(echo "$out" | grep -E '^[0-9]+ ADRs:')" "7 ADRs: 7 accepted, 0 violated, 0 unverified, 0 invalid, 0 not run"
echo "      (wall time of that run: $(awk -v a="$t0" -v b="$t1" 'BEGIN{printf "%.1f", b-a}') s)"

# 2. the light reader against the real one
out=$(cd "$R" && "$FIBC" run compiler/tests/adr/reader-agree.fib -I compiler -I lib 2>&1)
echo "$out" | head -3
check "2 light reader and real reader agree on compiler/ and lib/" "$(echo "$out" | grep -c ' source files, .* 0 disagree$')" 1

# 3. a scratch copy of the tree with a violation planted for each pilot
mk_root() {
  rm -rf "$1"; mkdir -p "$1/cases/stdlib"
  for d in compiler lib scripts rt docs spec .github editors; do cp -a "$R/$d" "$1/$d"; done
  cp -a "$R"/*.md "$R"/SEED "$R"/VERSION "$R"/.gitlab-ci.yml "$1/" 2>/dev/null
  cp -a "$R"/cases/stdlib/9* "$1/cases/stdlib/" 2>/dev/null
}
P=$S/plant; mk_root "$P"
echo 'cargo build --release' >> "$P/scripts/gate.sh"                                                      # 0002
echo '    (call @fib.fail-task t b n)' >> "$P/rt/str.lir"                                                  # 0001
printf '(ns zz)\n(defun splat (x: i64) -> i64 x)\n' > "$P/compiler/emit/zz.fib"                            # 0003
printf '\n(defun zz (a: f64) -> f64 (if (> (native-lanes f64) 2) (simd/fma a a a) a))\n' >> "$P/lib/fib/tensor/linalg.fib"   # 0004
printf '#!/bin/bash\n' > "$P/scripts/compare-interp.sh"                                                    # 0005
{ echo '(ns zz2)'; for i in $(seq 600); do echo ';; filler'; done; } > "$P/compiler/emit/zz2.fib"        # 0006
echo '<!-- measure rmw-window/array 2.5 ratio planted | cmd: none -->' >> "$P/docs/design/exclusive-views.md"  # 0007
out=$("$ADR" --root "$P"); st=$?
echo "$out" | head -40
check "3 planted violations: exit status" "$st" 1
for n in 0001 0002 0003 0004 0005 0006 0007; do check "3 ADR $n is violated by its plant" "$(status_of "$out" $n)" violated; done
check "3 the table counts" "$(echo "$out" | grep -E '^[0-9]+ ADRs:')" "7 ADRs: 0 accepted, 7 violated, 0 unverified, 0 invalid, 0 not run"
check "3 the 0003 violation is named" "$(echo "$out" | grep -c 'zz.fib:2: splat is defined here')" 1
check "3 the 0006 violation is named" "$(echo "$out" | grep -c 'compiler/emit/zz2.fib is 601')" 1
check "3 the 0007 violation is named" "$(echo "$out" | grep -c 'rmw-window/array = 2.5, it must be under 1.3')" 1
check "3 the tree itself is untouched (no plant in it)" "$(grep -c 'cargo build --release' "$R/scripts/gate.sh"; ls "$R/compiler/emit/zz.fib" 2>/dev/null | wc -l)" "$(printf '0\n0')"
rm -rf "$P"; mk_root "$P"
out=$("$ADR" --root "$P"); st=$?
check "3 the same copy without the plants: exit status" "$st" 0
check "3 ... all accepted" "$(echo "$out" | grep -E '^[0-9]+ ADRs:')" "7 ADRs: 7 accepted, 0 violated, 0 unverified, 0 invalid, 0 not run"
rm -rf "$P"

# 4. synthetic ADRs
mk_root "$S/root"; D=$S/synth; mkdir -p "$D"
adr() { # NUMBER STATUS-LINE BODY...: a minimal ADR file
  local n=$1 st=$2; shift 2
  printf '# %s. Synthetic %s\n\n%s\n\n## Context\n\nc\n\n## Decision\n\nd\n\n## Consequences\n\nq\n\n## Governance\n\n%s\n' "$n" "$n" "$st" "$*" > "$D/$n-synthetic.md"
}
FAIL_SPEC=$'```fibber spec\n(feature "f" (scenario "wrong" (then (expect = 3 (+ 1 1)))))\n```'
OK_SPEC=$'```fibber spec\n(feature "f" (scenario "right" (then (expect = 2 (+ 1 1)))))\n```'
VACUOUS=$'```fibber fitness\n(rule "nothing" (grep-live repo ["scripts/gate.sh"] "no-such-text-zzz") (plant "scripts/gate.sh" "\\nx\\n"))\n```'
NOPLANT=$'```fibber fitness\n(rule "unplanted" (grep-live repo ["scripts/gate.sh"] "cargo"))\n```'
NOCOMPILE=$'```fibber spec\n(feature "f"\n  (scenario "bad"\n    (then (expect = 3 (+ 1 "a")))))\n```'
adr 1001 "Status: accepted" "$OK_SPEC"
adr 1002 "Status: accepted" "$FAIL_SPEC"
adr 1003 "Status: accepted" "$VACUOUS"
adr 1004 "Status: accepted" "$NOPLANT"
adr 1005 "Status: accepted" "$NOCOMPILE"
adr 1006 "Status: accepted" "prose only"
adr 1007 "Status: superseded by 1001" "$FAIL_SPEC"
adr 1008 "Status: proposed" "$FAIL_SPEC"
adr 1009 "Context only" "$OK_SPEC"
printf '# 1010. Outside\n\nStatus: accepted\n\n## Context\n\n%s\n\n## Governance\n\nx\n' "$OK_SPEC" > "$D/1010-outside.md"
adr 1011 "Status: accepted" "$OK_SPEC"$'\n\n```fibber measure\n(measure "m" :recorded "docs/design/exclusive-views.md" :key "fill-window/array" :under 1.3)\n```'
adr 1012 "Status: accepted" $'```fibber measure\n(measure "m" :recorded "docs/design/exclusive-views.md" :key "no-such-key" :under 1.3)\n```'
out=$("$ADR" --root "$S/root" "$D"); st=$?
echo "$out"
check "4 synthetic: exit status" "$st" 1
check "4 passing spec: accepted"             "$(status_of "$out" 1001)" accepted
check "4 failing spec: violated"             "$(status_of "$out" 1002)" violated
check "4 rule that cannot fail: violated"    "$(status_of "$out" 1003)" violated
check "4 ... and says so"                    "$(echo "$out" | grep -c 'the rule cannot fail here (vacuous)')" 1
check "4 rule with no plant: violated"       "$(status_of "$out" 1004)" violated
check "4 block that does not compile: violated" "$(status_of "$out" 1005)" violated
check "4 ... with the Markdown line of the error" "$(echo "$out" | grep -c 'does not compile: .*1005-synthetic.md:2[0-9]:')" 1
check "4 no blocks: unverified"              "$(status_of "$out" 1006)" unverified
check "4 superseded: not run"                "$(echo "$out" | awk '$1==1007 {print $2, $3, $4}')" "superseded by 1001"
check "4 proposed: not run"                  "$(status_of "$out" 1008)" proposed
check "4 no Status line: invalid"            "$(status_of "$out" 1009)" invalid
check "4 block outside Governance: invalid"  "$(status_of "$out" 1010)" invalid
check "4 recorded measure within threshold: accepted" "$(status_of "$out" 1011)" accepted
check "4 measure without a record: violated" "$(status_of "$out" 1012)" violated
"$ADR" --root "$S/root" "$D" --only 1001 > "$S/o1.txt"; st=$?
check "4 --only 1001: exit status" "$st" 0
check "4 --only 1001: one row" "$(grep -c '^1001 ' "$S/o1.txt")" 1
check "4 --only: no other row" "$(grep -c '^1002 ' "$S/o1.txt")" 0
check "4 --strict: unverified fails" "$("$ADR" --root "$S/root" "$D" --only 1006 --strict >/dev/null; echo $?)" 1
check "4 unverified without --strict passes" "$("$ADR" --root "$S/root" "$D" --only 1006 >/dev/null; echo $?)" 0
check "4 --status: lines of number and status" "$("$ADR" --root "$S/root" "$D" --only 1001,1002 --status | tr '\n' ' ')" "1001 accepted 1002 violated "
check "4 --format json: schema" "$("$ADR" --root "$S/root" "$D" --only 1001 --format json | grep -c '^{"schema":1,"adrs":\[{"adr":"1001"')" 1
cp "$D/1001-synthetic.md" "$S/before.md"
"$ADR" --root "$S/root" "$D" --only 1001 --stamp > "$S/stamp1.txt"
check "4 --stamp wrote the file" "$(grep -c 'stamped' "$S/stamp1.txt")" 1
check "4 --stamp: one line added" "$(diff "$S/before.md" "$D/1001-synthetic.md" | grep -c '^[<>]')" 1
check "4 --stamp: that line" "$(diff "$S/before.md" "$D/1001-synthetic.md" | grep '^>')" "> <!-- derived: accepted -->"
"$ADR" --root "$S/root" "$D" --only 1001 --stamp > "$S/stamp2.txt"
check "4 --stamp twice writes nothing the second time" "$(grep -c 'stamped' "$S/stamp2.txt")" 0

# 4b. measures: the recorded number in the fast path, the quoted command with --rerun
{ echo '# records'; echo '<!-- measure fast 1.1 ratio 2026-10-05 | cmd: echo "measure fast 1.2" -->'
  echo '<!-- measure drifted 1.1 ratio 2026-10-05 | cmd: echo "measure drifted 1.9" -->'
  echo '<!-- measure silent 1.1 ratio 2026-10-05 | cmd: true -->'; } > "$S/root/docs/records.md"
M=$S/measures; mkdir -p "$M"
for k in fast drifted silent; do
  printf '# 20%s. Measure %s\n\nStatus: accepted\n\n## Governance\n\n```fibber measure\n(measure "%s" :recorded "docs/records.md" :key "%s" :under 1.3)\n```\n' "${#k}0" "$k" "$k" "$k" > "$M/20${#k}0-$k.md"
done
out=$("$ADR" --root "$S/root" "$M"); st=$?
check "4b fast path reads the records: all three hold (1.1 under 1.3)" "$st" 0
out=$("$ADR" --root "$S/root" "$M" --rerun); st=$?
echo "$out"
check "4b --rerun: exit status (one drifted, one silent)" "$st" 1
check "4b --rerun: a re-measured value within the threshold holds" "$(status_of "$out" 2040)" accepted
check "4b --rerun: a value that drifted over the threshold is violated" "$(status_of "$out" 2070)" violated
check "4b ... with the value" "$(echo "$out" | grep -c 'drifted = 1.9, it must be under 1.3')" 1
check "4b --rerun: a command that prints no measure line is violated" "$(status_of "$out" 2060)" violated

# 5. usage
"$ADR" --format xml > /dev/null 2>&1; check "5 bad --format: exit 2" "$?" 2
"$ADR" --only 9999 --root "$S/root" "$D" > /dev/null 2>&1; check "5 --only that matches nothing: exit 2" "$?" 2
"$ADR" --root "$S/no-such-dir" > /dev/null 2>&1; check "5 no such root: exit 2" "$?" 2

echo "adr test: $fails failed"
exit $((fails > 0))
