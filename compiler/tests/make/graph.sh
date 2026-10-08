#!/bin/bash
# compiler/tests/make/graph.sh: the build's own graph (Makefile, mk/*.mk; docs/design/build.md 6), checked without building anything: the
# tree is read as it is, the build directory is a scratch one holding fake products with chosen mtimes, and `make -n` / `make -q` say what
# would run. Each check prints `ok N ..` or `FAIL N ..`; exit 0 when all hold.
#   1  `make help` names every public target (every .PHONY target that is not internal)
#   2  nothing changed: `make -q` of the full gate's stamps is up to date, and `make -n gate` runs no recipe (the no-op gate)
#   3  one tool script newer than its stamp: `make -n gate` runs that stamp's recipe and no other
#   4  a compiler source newer than F: F is rebuilt, and the cases, golden, tools and ADR stamps all rerun (everything downstream of F)
#   5  a case file newer than its shard's table: that shard reruns, and no other shard
#   6  a planted fault, a tool stamp without its F prerequisite: with F out of date the planted copy of the Makefile does NOT rerun the
#      stamp, the real one does (the check can fail: it fails on the planted copy)
#   7  a stamp is not written when its command fails, and a target a failed recipe changed is removed (.DELETE_ON_ERROR)
#   8  `make -n gate` runs no download when the seed is in place (no curl, wget, gh release download in the dry run)
# usage: compiler/tests/make/graph.sh [SCRATCH]     (run anywhere; make 4 is needed)
set -u
# The test runs as a tool of the gate, inside make's environment (-j, GATE_FRESH, FIBC=F): none of it belongs to the graph under test.
unset GATE_FRESH GATE_MODE MAKEFLAGS MFLAGS MAKELEVEL FIBC BUILDER GATE_BUDGET
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd); cd "$root" || exit 2
S=${1:-${TMPDIR:-/tmp}/make-graph.$$}; rm -rf "$S"; mkdir -p "$S/build" "$S/mk"
B=$S/build
bad=0
ok() { echo "ok   $*"; }
fail() { echo "FAIL $*"; bad=$((bad + 1)); }
M() { make --no-print-directory BUILD="$B" "$@"; }   # the real Makefile against the scratch build directory
make --version | grep -q '^GNU Make 4\|^GNU Make 5' || { echo "graph.sh: GNU Make 4 is needed"; exit 2; }

# fake products: the seed, F and every stamp of the full gate, newer than everything in the tree (and older than nothing)
fake_all() {
  rm -rf "$B"; mkdir -p "$B"
  seed=$(M -s -f Makefile -p 2> /dev/null | sed -n 's/^SEED_FIBC := //p' | head -n 1)
  mkdir -p "$(dirname "$seed")"; : > "$seed"; chmod +x "$seed"
  for f in $(M -n -p 2> /dev/null | sed -n 's/^FULL_STAMPS := //p' | head -n 1); do mkdir -p "$(dirname "$f")"; echo 1 > "$f"; done
  for f in F F.lir F3 F3.lir gen-runtime adr; do : > "$B/$f"; done; chmod +x "$B/F" "$B/F3" "$B/gen-runtime" "$B/adr"
  mkdir -p "$B/golden/tools" "$B/tools" "$B/cases"; for t in read expand types own explain emit lairf; do : > "$B/golden/tools/$t"; done
  for f in $(M -n -p 2> /dev/null | grep -o "^$B/cases/[a-z]*\.[0-9]*\.txt" | sort -u); do echo "1 cases: 1 pass" > "$f"; done
  # products now (newer than any file of the tree), the stamps a moment later (newer than the products)
  touch "$seed" "$B"/F "$B"/F.lir "$B"/F3 "$B"/F3.lir "$B"/gen-runtime "$B"/adr "$B"/golden/tools/* "$B"/cases/*.txt
  sleep 0.05; touch "$B"/*.ok "$B"/golden/*.ok "$B"/tools/*.ok "$B"/cases/*.ok
}
fake_all
# the full gate's stamps, from the Makefile itself
stamps=$(M -n -p 2> /dev/null | sed -n 's/^FULL_STAMPS := //p' | head -n 1)
[ -n "$stamps" ] || { echo "graph.sh: could not read FULL_STAMPS"; exit 2; }

# 1 help names every public target
phony=$(grep -h '^\.PHONY:' Makefile mk/*.mk | sed 's/^\.PHONY://' | tr ' ' '\n' | grep -v '^$' | grep -v -E '^(FORCE|gate-stamps|quick-stamps|gate-report|tools-list|mutants-list)$' | LC_ALL=C sort -u)
listed=$(M help | awk 'NR > 2 { print $1 }' | LC_ALL=C sort -u)
missing=$(comm -23 <(echo "$phony") <(echo "$listed"))
if [ -z "$missing" ]; then ok "1 make help lists every public target ($(echo "$phony" | wc -l))"; else fail "1 make help misses: $(echo "$missing" | tr '\n' ' ')"; fi

# 2 nothing changed: up to date, no recipe
if M -q gate-stamps; then ok "2a make -q gate-stamps: up to date"; else fail "2a make -q gate-stamps says something is out of date on fake products newer than the tree"; fi
out=$(M -n gate-stamps 2>&1); rc=$?; n=$(echo "$out" | grep -c -v '^make\|^$' || true)
if [ "$rc" -eq 0 ] && [ "$n" -eq 0 ]; then ok "2b make -n gate-stamps runs no recipe"; else fail "2b make -n gate-stamps (exit $rc) would run $n lines: $(echo "$out" | head -3)"; fi

# 3 one tool stamp older than its script: only that recipe runs
touch -d '-1 day' "$B/tools/gen-skeleton.ok"
run=$(M -n gate-stamps | grep -v '^make')
if echo "$run" | grep -q 'tests/gen/skeleton.sh' && [ "$(echo "$run" | grep -c 'rm -f ')" -eq 1 ]; then ok "3 an old tool stamp reruns alone (1 recipe: gen-skeleton)"; else fail "3 expected exactly the gen-skeleton recipe, got: $(echo "$run" | grep -c 'rm -f ') recipes"; fi
touch "$B/tools/gen-skeleton.ok"

# 4 F older than a compiler source: F rebuilds and everything downstream reruns
touch -d '-1 day' "$B/F"
run=$(M -n gate-stamps | grep -v '^make')
for want in 'build compiler/fibc.fib' 'cases cases/stdlib' 'golden.sh' 'tests/gen/skeleton.sh' 'adr --strict'; do
  echo "$run" | grep -q -- "$want" || { fail "4 with F out of date, nothing runs for: $want"; want=; }
  [ -n "$want" ] && ok "4 with F out of date the dry run has: $want"
done
touch "$B/F"; sleep 0.05; touch "$B"/*.ok "$B"/golden/*.ok "$B"/tools/*.ok "$B"/cases/*.ok "$B"/cases/*.txt

# 5 one stdlib case newer than its shard's table: that shard reruns, no other
first=$(M -n -p 2> /dev/null | sed -n 's/^NAMES_stdlib.3 := //p' | head -n 1 | awk '{print $1}')
touch -d '-1 day' "$B/cases/stdlib.3.txt"
run=$(M -n gate-stamps | grep -v '^make' | grep -c 'cases cases/stdlib' || true)
if [ "$run" -eq 1 ]; then ok "5 an old shard table reruns alone (shard 3 of stdlib, first case $first)"; else fail "5 expected 1 stdlib shard to rerun, got $run"; fi
touch "$B/cases/stdlib.3.txt"; sleep 0.05; touch "$B/cases/full.ok"

# 6 the planted fault: a copy of the Makefile whose tool stamps do not depend on F
cp Makefile "$S/Makefile"; cp mk/*.mk "$S/mk/"
sed -i 's|^$(TOOLS_DIR)/$(1).ok: $(F) $(call after_bar|$(TOOLS_DIR)/$(1).ok: $(call after_bar|' "$S/mk/tools.mk"
grep -q '^$(TOOLS_DIR)/$(1).ok: $(call after_bar' "$S/mk/tools.mk" || { fail "6 the plant did not take (the tool rule changed shape?)"; }
touch -d '-1 day' "$B/F"
real=$(M -n "$B/tools/gen-skeleton.ok" | grep -c 'tests/gen/skeleton.sh' || true)
planted=$(make --no-print-directory -f "$S/Makefile" BUILD="$B" -n "$B/tools/gen-skeleton.ok" | grep -c 'tests/gen/skeleton.sh' || true)
if [ "$real" -eq 1 ] && [ "$planted" -eq 0 ]; then ok "6 a tool stamp without its F prerequisite is caught: the real graph reruns it after F, the planted copy does not"
else fail "6 real graph reran the tool stamp $real times, the planted copy $planted (want 1 and 0)"; fi
touch "$B/F"; sleep 0.05; touch "$B"/*.ok "$B"/golden/*.ok "$B"/tools/*.ok "$B"/cases/*.ok

# 7 a failing command leaves no stamp; a target a failed recipe wrote is removed
printf 'tool_zz-fail := false | \nTOOLS_FULL += zz-fail\n$(eval $(call tool_target,zz-fail))\n$(BUILD)/zz-partial:\n\techo partial > $$@; false\n' > "$S/mk/zz.mk"
printf '\ninclude $(MK)/zz.mk\n' >> "$S/Makefile"
make --no-print-directory -f "$S/Makefile" BUILD="$B" "$B/tools/zz-fail.ok" > "$S/zz.log" 2>&1
if [ ! -e "$B/tools/zz-fail.ok" ]; then ok "7a a failing tool writes no stamp"; else fail "7a the stamp was written although the command failed"; fi
make --no-print-directory -f "$S/Makefile" BUILD="$B" "$B/zz-partial" > "$S/zz2.log" 2>&1
if [ ! -e "$B/zz-partial" ]; then ok "7b a target written by a failing recipe is removed (.DELETE_ON_ERROR)"; else fail "7b $B/zz-partial survived its failed recipe"; fi

# 8 no download in the dry run of the gate when the seed is in place
dl=$(M -n gate-stamps | grep -c -E '^(curl|wget|gh release download|git clone)' || true)
if [ "$dl" -eq 0 ]; then ok "8 make -n gate-stamps lists no download"; else fail "8 the dry run would download: $dl lines"; fi

[ -n "${KEEP:-}" ] || rm -rf "$S"
echo "graph: $bad failed"
exit $((bad > 0))
