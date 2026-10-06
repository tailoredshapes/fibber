#!/bin/bash
# compiler/tests/deps/run.sh: the package manager (docs/design/packages.md) as a program, offline: local bare repositories made by
# fixtures.sh in a temporary directory, reached by file:// URLs, and a cache ($FIBBER_HOME) of the test's own. Each check compares an exit
# status or a line of output with the one wanted, so each can fail; scripts/mutant-deps.sh plants faults in the resolver and shows that a
# check here fails for each.
# usage: compiler/tests/deps/run.sh STAGE2         (run from the repository root; FIB_LIB defaults to the tree's lib)
set -uo pipefail
F=${1:?usage: run.sh STAGE2}
here=$(cd "$(dirname "$0")" && pwd)
export FIB_LIB=${FIB_LIB:-$PWD/lib}
ulimit -v 16000000
mkdir -p "$HOME/.cache/fibber-scratch"
T=$(mktemp -d "$HOME/.cache/fibber-scratch/deps-test.XXXXXX")
cleanup() { chmod -R u+w "$T" 2> /dev/null; rm -rf "$T"; }
trap cleanup EXIT
export FIBBER_HOME=$T/cache
unset FIB_NO_PROJECT
. "$here/fixtures.sh"
make_fixtures
fail=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fail=1; fi; }
has() { if grep -q -- "$3" "$2"; then echo "ok    $1"; else echo "FAIL  $1: no [$3] in:"; sed 's/^/        /' "$2" | head -8; fail=1; fi; }
o=$T/out; e=$T/err
run() { "$F" "$@" > "$o" 2> "$e"; echo $?; }
last() { tail -n 1 "$o"; }

# 1. A project with two levels of git dependencies (app -> util -> base), the dependency's macro used across the boundary.
project app "{:name \"acme/app\" :deps {acme/util {:git/url \"$URL_util\" :git/tag \"v1\"}}}" \
  '(ns main (:require [acme.util :as u]))
(defun main () -> i64 (u/twice (u/util-value)))'
cd "$T/p/app" || exit 2
ck "transitive: run exits 0" "$(run run src/main.fib)" 0
ck "transitive: util's function, base's value and util's macro (2 * 10 * 1)" "$(last)" 20
ck "transitive: the lock has both libraries" "$(grep -c ':name "acme/' deps.lock)" 2
has "transitive: the lock records the tag's commit" deps.lock ":git/sha \"$U1\" :git/tag \"v1\""
has "transitive: the lock records the dependency's tree id" deps.lock ":tree \"$(git -C "$T/work-repos/base" rev-parse "$B1^{tree}")\""
ck "transitive: tree exits 0" "$(run deps tree)" 0
ck "transitive: tree shows base under util" "$(sed -n 2,3p "$o" | tr '\n' '|')" "  acme/util $URL_util ${U1:0:12} (tag v1)|    acme/base $URL_base ${B1:0:12}|"
ck "path: the project's root first, then util, then base" \
  "$(run deps path >/dev/null; sed "s|$T|T|; s|/[0-9a-f]\{40\}/|/SHA/|" "$o" | tr '\n' ' ')" \
  "T/p/app/src T/cache/git/file/${T#/}/remotes/util/SHA/src T/cache/git/file/${T#/}/remotes/base/SHA/src "
checkout=$FIBBER_HOME/git/file${T}/remotes/util/$U1
ck "cache: the checkout is at its sha" "$(git -C "$checkout" rev-parse HEAD)" "$U1"
ck "cache: the checkout is read-only" "$(touch "$checkout/src/x" 2> /dev/null && echo writable || echo read-only)" read-only

# 2. The search path: a library comes before FIB_LIB (a module of the same name there is not taken).
mkdir -p "$T/fiblib/acme"; printf '(ns acme.util)\n(defun util-value () -> i64 -5)\n(defmacro twice (x) x)\n' > "$T/fiblib/acme/util.fib"
cp "$FIB_LIB/prelude.fib" "$T/fiblib/" 2> /dev/null
ck "order: run with FIB_LIB holding acme.util" "$(FIB_LIB=$FIB_LIB:$T/fiblib run run src/main.fib)" 0
ck "order: the library's acme.util wins over FIB_LIB's" "$(last)" 20

# 3. Offline and locked: the remotes moved away, the lock and the warm cache are all a build needs.
mv "$T/remotes" "$T/remotes.away"
ck "locked: builds with no remote" "$(run run --locked src/main.fib)" 0
ck "locked: same answer" "$(last)" 20
ck "offline+locked (--frozen): builds with no remote" "$(run run --frozen src/main.fib)" 0
FIBBER_HOME=$T/cold ck "offline, cold cache: refused" "$(FIBBER_HOME=$T/cold run run --offline src/main.fib)" 2
has "offline, cold cache: says what is missing" "$e" "is not in the cache .*--offline forbids fetching it"
mv "$T/remotes.away" "$T/remotes"
cp deps.fib deps.fib.orig; printf '{:name "acme/app" :deps {}}\n' > deps.fib
ck "locked: a lock that deps.fib no longer matches is refused" "$(run run --locked src/main.fib)" 2
has "locked: says why" "$e" "was not made from this deps.fib"
mv deps.fib.orig deps.fib

# 4. A tampered checkout is found.
chmod u+w "$checkout/src/acme/util.fib"; printf '(ns acme.util)\n(defun util-value () -> i64 666)\n(defmacro twice (x) x)\n' > "$checkout/src/acme/util.fib"
ck "tamper: build refused" "$(run run src/main.fib)" 2
has "tamper: names the changed checkout" "$e" "the checkout's files were changed"
chmod -R u+w "$checkout"; rm -rf "$checkout"
ck "tamper: removed, fetched again" "$(run run src/main.fib)" 0
ck "tamper: right answer again" "$(last)" 20
bcheck=$FIBBER_HOME/git/file${T}/remotes/base/$B1
chmod -R u+w "$bcheck"; git -C "$bcheck" commit -q --allow-empty -m moved
ck "tamper: a checkout at another commit is refused" "$(run run src/main.fib)" 2
has "tamper: says which commit" "$e" "not the locked $B1"
chmod -R u+w "$bcheck"; rm -rf "$bcheck"

# 5. A diamond at one commit is fine; at two commits it is a conflict naming both chains; :override chooses.
project diamond "{:name \"acme/d\" :deps {acme/util {:git/url \"$URL_util\" :git/sha \"$U1\"} acme/other {:git/url \"$URL_other\" :git/tag \"v1\"}}}" \
  '(ns main (:require [acme.util :as u] [acme.other :as x]))
(defun main () -> i64 (+ (u/util-value) (x/other-value)))'
cd "$T/p/diamond" || exit 2
ck "diamond: same commit, exit 0" "$(run run src/main.fib)" 0
ck "diamond: 10 + 100" "$(last)" 110
ck "diamond: base is locked once" "$(grep -c ':name "acme/base"' deps.lock)" 1
project conflict "{:name \"acme/c\" :deps {acme/util {:git/url \"$URL_util\" :git/sha \"$U1\"} acme/other {:git/url \"$URL_other\" :git/tag \"v2\"}}}" \
  '(ns main (:require [acme.util :as u] [acme.other :as x]))
(defun main () -> i64 (+ (u/util-value) (x/other-value)))'
cd "$T/p/conflict" || exit 2
ck "conflict: refused" "$(run run src/main.fib)" 2
has "conflict: says CONFLICT" "$e" "CONFLICT: acme/base is requested twice"
has "conflict: the first chain" "$e" "acme/c -> acme/util -> acme/base: $URL_base $B1"
has "conflict: the second chain" "$e" "acme/c -> acme/other -> acme/base: $URL_base $B2"
ck "conflict: no lock written" "$([ -e deps.lock ] && echo yes || echo no)" no
printf '{:name "acme/c" :deps {acme/util {:git/url "%s" :git/sha "%s"} acme/other {:git/url "%s" :git/tag "v2"}}\n :override {acme/base {:git/url "%s" :git/sha "%s"}}}\n' \
  "$URL_util" "$U1" "$URL_other" "$URL_base" "$B2" > deps.fib
ck "override: builds" "$(run run src/main.fib)" 0
ck "override: base at B2 for both (20 + 200)" "$(last)" 220

# 6. A tag that moved after locking is refused; `deps update` takes it.
project tagged "{:name \"acme/t\" :deps {acme/tagged {:git/url \"$URL_tagged\" :git/tag \"t1\"}}}" \
  '(ns main (:require [acme.tagged :as t]))
(defun main () -> i64 (t/tagged-value))'
cd "$T/p/tagged" || exit 2
ck "tag: first build" "$(run run src/main.fib)" 0
ck "tag: t1 is G1" "$(last)" 1
move_tag tagged t1 "$G2"
ck "tag moved: a build keeps the locked commit" "$(run run src/main.fib)" 0
ck "tag moved: still G1" "$(last)" 1
ck "tag moved: deps fetch refuses" "$(run deps fetch)" 1
has "tag moved: says so" "$e" "the tag t1 of $URL_tagged has moved: it named $G1 when locked and names $G2"
chmod -R u+w "$FIBBER_HOME/git/file${T}/remotes/tagged"; rm -rf "$FIBBER_HOME/git/file${T}/remotes/tagged"
ck "tag moved, cold cache: the build refuses to fetch" "$(run run src/main.fib)" 2
has "tag moved, cold cache: says so" "$e" "has moved"
ck "tag moved: deps update takes the new commit" "$(run deps update acme/tagged)" 0
run run src/main.fib > /dev/null; ck "tag moved: now G2" "$(last)" 2
has "tag moved: the lock has G2" deps.lock ":git/sha \"$G2\""

# 7. Two libraries that define one module.
project clash "{:name \"acme/k\" :deps {acme/util {:git/url \"$URL_util\" :git/sha \"$U1\"} acme/clash {:git/url \"$URL_clash\" :git/tag \"v1\"}}}" \
  '(ns main (:require [acme.util :as u]))
(defun main () -> i64 (u/util-value))'
cd "$T/p/clash" || exit 2
ck "collision: refused" "$(run run src/main.fib)" 2
has "collision: names the module and both libraries" "$e" "the module acme.util is defined twice: by acme/util (.*) and by acme/clash"

# 8. deps.fib errors at their positions; a URL that is not https, ssh or file.
mkdir -p "$T/p/bad"; cd "$T/p/bad" || exit 2; mkdir -p src; printf '(ns main)\n(defun main () -> i64 0)\n' > src/main.fib
printf '{:name "x"\n :dpes {}}\n' > deps.fib
ck "syntax: an unknown key" "$(run run src/main.fib)" 2
has "syntax: at its line and column" "$e" "deps.fib:2:2: unknown key :dpes in deps.fib"
printf '{:name "x"\n :deps {a/b {:git/url "https://x/y.git" :git/sha "abc"}}' > deps.fib
ck "syntax: an unclosed map" "$(run run src/main.fib)" 2
has "syntax: the reader's message with a position" "$e" "deps.fib:[0-9]*:[0-9]*: "
printf '{:name "x"\n :deps {a/b {:git/url "https://x/y.git"\n             :git/sha "abc"}}}\n' > deps.fib
ck "syntax: a short sha" "$(run run src/main.fib)" 2
has "syntax: says the sha is short" "$e" "deps.fib:2:13: :git/sha is a full 40-digit commit id"
printf '{:deps {a/b {:git/url "http://example.org/y.git" :git/tag "v1"}}}\n' > deps.fib
ck "url: http is refused" "$(run run src/main.fib)" 2
has "url: says how to allow it" "$e" "is not https, ssh or file"

# 9. deps add edits deps.fib in place and locks.
project adder "$(printf ';; my project\n{:name "acme/a"\n :deps {acme/util {:git/url "%s" :git/sha "%s"}}}' "$URL_util" "$U1")" '(ns main)
(defun main () -> i64 0)'
cd "$T/p/adder" || exit 2
ck "add: exit 0" "$(run deps add acme/other --git "$URL_other" --tag v1)" 0
ck "add: the comment kept and the entry under the first" "$(sed -n '1p;4p' deps.fib | tr '\n' '|')" \
  ";; my project|        acme/other {:git/url \"$URL_other\" :git/tag \"v1\"}}}|"
ck "add: locked (util, base, other)" "$(grep -c ':name "acme/' deps.lock)" 3
cp deps.fib before
ck "add: a conflicting library is refused" "$(run deps add acme/b2 --git "$URL_other" --tag v2)" 1
ck "add: deps.fib unchanged after a refusal" "$(cmp -s deps.fib before && echo same || echo changed)" same

# 10. fibc new, then fibc test and run in the new project.
cd "$T" || exit 2
ck "new: exit 0" "$(run new hello)" 0
cd "$T/hello" || exit 2
ck "new: test passes" "$(run test)" 0
has "new: one scenario passed" "$o" "total: 1 scenarios: 1 pass"
run run src/main.fib > /dev/null; ck "new: run" "$(head -n 1 "$o")" "hello, world"
ck "new --lib: no main" "$(cd "$T" && "$F" new hlib --lib > /dev/null && [ -e hlib/src/main.fib ] && echo main || echo none)" none
ck "new: a bad name" "$(run new Bad)" 2

# 11. A :local/root library (a directory, no commit), and one that would shadow the bundled library.
mkdir -p "$T/locals/lx/src/lx" "$T/locals/shadow/src/fib"
printf '{:name "lx"}\n' > "$T/locals/lx/deps.fib"
printf '(ns lx.thing)\n(defun thing () -> i64 7)\n' > "$T/locals/lx/src/lx/thing.fib"
printf '(ns fib.core)\n(defun thing () -> i64 8)\n' > "$T/locals/shadow/src/fib/core.fib"
project loc '{:name "acme/l" :deps {lx {:local/root "../../locals/lx"}}}' '(ns main (:require [lx.thing :as t]))
(defun main () -> i64 (t/thing))'
cd "$T/p/loc" || exit 2
ck "local: builds" "$(run run src/main.fib)" 0
ck "local: the directory's module" "$(last)" 7
has "local: locked by its relative path" deps.lock ':local/root "../../locals/lx"'
printf '{:name "acme/l" :deps {lx {:local/root "../../locals/lx"} sh {:local/root "../../locals/shadow"}}}\n' > deps.fib
ck "shadow: a library with a module of the bundled library is refused" "$(run run src/main.fib)" 2
has "shadow: says so" "$e" "the module fib.core of sh .* has the name of a module of the bundled library"

# 12. Two fetches at once into a cold cache: both succeed, one whole checkout each, no partial directory left.
cd "$T/p/app" || exit 2
export FIBBER_HOME=$T/race
"$F" deps fetch > "$T/r1" 2>&1 & p1=$!
"$F" deps fetch > "$T/r2" 2>&1 & p2=$!
wait $p1; s1=$?; wait $p2; s2=$?
ck "race: both fetches exit 0" "$s1 $s2" "0 0"
ck "race: one checkout of util, none partial" "$(ls "$T/race/git/file${T}/remotes/util" | tr '\n' ' ')" "$U1 "

[ $fail = 0 ] && echo "deps: all checks passed" || echo "deps: FAILED"
exit $fail
