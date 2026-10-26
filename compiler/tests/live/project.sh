#!/bin/bash
# compiler/tests/live/project.sh: the live module rule in a project (docs/design/live-meshql.md 4, package LT3; emit.live `program-module?`,
# pkg.modules `live-roots`, fibc `live-roots-env`).
#   compiler/tests/live/project.sh [FIBC] [--update]     FIBC is stage 2 (default $FIBC); --update rewrites project-stubs.golden (read the diff)
# A project `acme/app` (the git fixtures of compiler/tests/deps/fixtures.sh, offline) with one dependency, acme/util, which depends on
# acme/base: src/main.fib and src/app/core.fib are the project's, dev/live.fib an entry outside its :paths, specs/ its test paths.
# The slots (`(GLOBAL .. live.NAME ..)`) of `fibc emit` in live mode, sorted, are project-stubs.golden, one line per way of asking:
#   default   FIB_LIVE=1 in the project: the project's modules only (main, app.core); nothing of acme.util or acme.base
#   deps      --live --live-deps: the libraries' too
#   one       FIB_LIVE_DEPS=acme/base: that library's and not the other's
#   dev       an entry outside :paths: its module (the main module is always live) and the project's
#   outside   the same program outside the project (-I only): the name rule as before (every module not named fib.*)
# Then: without live mode the emit has no slot; the parent `fibc test` exports FIB_LIVE_ROOTS to its children (a scenario reads it: the
# project's src and specs, not the library) and exports nothing without live mode; the program runs under FIB_LIVE=1 with its answer.
# Then LM1 (docs/design/live-incremental.md 3.6; serve.liveload): dev/load.fib run under --live, a `load` of acme.util's file is
# live/NotLive with `--live-deps=acme/util` in the hint; under --live-deps the same load swaps acme.util/util-value and the running
# program's loop reads the new value.
# scripts/mutant-live.sh deps-stubbed-by-default and project-paths-not-stubbed must each turn the golden red. Exit 0 when every check holds.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
update=; fibc=${FIBC:-}
for a in "$@"; do case $a in --update) update=1 ;; *) fibc=$a ;; esac; done
[ -x "$fibc" ] || { echo "usage: project.sh FIBC [--update] (or FIBC in the environment)"; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset FIB_LIVE FIB_LIVE_DEPS FIB_LIVE_ROOTS FIB_NO_PROJECT
ulimit -v 16000000
mkdir -p "$HOME/.cache/fibber-scratch"
T=$(mktemp -d "$HOME/.cache/fibber-scratch/live-project.XXXXXX")
cleanup() { chmod -R u+w "$T" 2> /dev/null; rm -rf "$T"; }
trap cleanup EXIT
export FIBBER_HOME=$T/cache
. "$root/compiler/tests/deps/fixtures.sh"
make_fixtures
bad=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; bad=1; fi; }

project app "{:name \"acme/app\" :paths [\"src\"] :aliases {:test {:extra-paths [\"specs\"]}} :deps {acme/util {:git/url \"$URL_util\" :git/tag \"v1\"}}}" \
  '(ns main (:require [acme.util :as u] [app.core :as c]))
(defun main () -> i64 (c/twice (c/half (u/util-value))))'
mkdir -p "$T/p/app/src/app" "$T/p/app/dev" "$T/p/app/specs"
cat > "$T/p/app/src/app/core.fib" <<'EOF'
(ns app.core)
(defun half (x: i64) -> i64 (quot x 2))
(defun twice (x: i64) -> i64 (* 2 x))
EOF
cat > "$T/p/app/dev/live.fib" <<'EOF'
(ns main (:require [app.core :as c]))
(defun entry (x: i64) -> i64 (c/twice x))
(defun main () -> i64 (entry 4))
EOF
cat > "$T/p/app/specs/roots-spec.fib" <<'EOF'
(ns main (:use fib.core fib.seq fib.coll fib.test.core fib.test.run) (:require [fib.os :as os] [fib.string :as s]))
(defspecs-main specs
  (feature "live roots"
    (scenario "FIB_LIVE_ROOTS names the project's src and specs and not the library"
      (given [v (unwrap-or (os/getenv "FIB_LIVE_ROOTS") "")])
      (then (expect = true (and (some? (s/index-of v "/p/app/src")) (some? (s/index-of v "/p/app/specs")) (nil? (s/index-of v "remotes/util"))))))))
EOF
cd "$T/p/app" || exit 2
slots() { sed -n 's/^(GLOBAL [A-Z]* live\.\([^ ]*\) .*/\1/p' "$1" | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//'; }
emit() { # emit NAME [ENV=V..] -- ARGS..: the slots of one emit as a golden line `NAME: slots`
  local name=$1; shift
  if ! env "$@" > "$T/$name.lir" 2> "$T/$name.err"; then echo "FAIL  $name: emit failed"; head -5 "$T/$name.err"; bad=1; fi
  echo "$name: $(slots "$T/$name.lir")"
}
"$fibc" deps path > "$T/paths" 2> "$T/paths.err" || { echo "FAIL  deps path"; cat "$T/paths.err"; exit 1; }
util_src=$(sed -n 2p "$T/paths"); base_src=$(sed -n 3p "$T/paths")
{
  emit default FIB_LIVE=1 "$fibc" emit src/main.fib
  emit deps "$fibc" --live --live-deps emit src/main.fib
  emit one FIB_LIVE=1 FIB_LIVE_DEPS=acme/base "$fibc" emit src/main.fib
  emit dev FIB_LIVE=1 "$fibc" emit dev/live.fib
  cd "$T" || exit 2
  emit outside FIB_LIVE=1 "$fibc" emit -I "$util_src" -I "$base_src" p/app/src/main.fib
  cd "$T/p/app" || exit 2
} > "$T/stubs.txt"
if [ -n "$update" ]; then cp "$T/stubs.txt" "$here/project-stubs.golden"; echo "updated project-stubs.golden"; cat "$T/stubs.txt"
elif diff -u "$here/project-stubs.golden" "$T/stubs.txt" > "$T/stubs.diff"; then echo "ok    the slots of each way of asking are project-stubs.golden"
else echo "FAIL  the slots differ from project-stubs.golden"; cat "$T/stubs.diff"; bad=1; fi
# the rule's shape, independent of the golden's text
ck "default: no slot of a library" "$(grep -c 'live\.f\.acme\.' "$T/default.lir")" 0
ck "default: the project's own module has slots" "$(grep -c '^(GLOBAL [A-Z]* live\.f\.app\.core\.' "$T/default.lir")" 2
ck "deps: the libraries' functions have slots" "$(grep -c '^(GLOBAL [A-Z]* live\.f\.acme\.' "$T/deps.lir")" 2
ck "one: base's slot and not util's" "$(grep -c '^(GLOBAL [A-Z]* live\.f\.acme\.base\.' "$T/one.lir") $(grep -c '^(GLOBAL [A-Z]* live\.f\.acme\.util\.' "$T/one.lir")" "1 0"
ck "dev: an entry outside :paths is live, with the project's module (the one function it reaches)" "$(grep -c '^(GLOBAL [A-Z]* live\.f\.entry ' "$T/dev.lir") $(grep -c '^(GLOBAL [A-Z]* live\.f\.app\.core\.' "$T/dev.lir")" "1 1"
ck "outside: the name rule stubs the libraries too" "$(grep -c '^(GLOBAL [A-Z]* live\.f\.acme\.' "$T/outside.lir")" 2
"$fibc" emit src/main.fib > "$T/plain.lir" 2> /dev/null
ck "no live mode: no slot, no generation name" "$(grep -c 'live\.\|\.g0' "$T/plain.lir")" 0
r=$(FIB_LIVE=1 "$fibc" run src/main.fib 2> "$T/run.err" | tail -n 1)
ck "the program runs under FIB_LIVE=1 with the libraries direct (2 * (10 / 2))" "$r" 10
FIB_LIVE=1 "$fibc" test specs > "$T/test.out" 2> "$T/test.err"; rc=$?
ck "fibc test under FIB_LIVE=1 exports FIB_LIVE_ROOTS to its children: the scenario passes" "$rc $(grep -c '^total: 1 scenarios: 1 pass' "$T/test.out")" "0 1"
"$fibc" test specs > "$T/test2.out" 2> /dev/null; rc=$?
ck "fibc test without live mode exports nothing: the scenario fails" "$rc" 1

# LM1: a load of a library module in a live session: refused without --live-deps (the hint names the library), swapped with it
cat > "$T/p/app/dev/load.fib" <<'EOF'
(ns main (:require [acme.util :as u] [fib.unix :as unix]))
(def stop: (Atom bool) (atom false))
(def seen: (Atom i64) (atom 0))
(defun main () -> i64 (do (println "app: ready") (loop () (if @stop 0 (do (reset! seen (u/util-value)) (unix/sleep-ns 20000000) (recur))))))
EOF
mkdir -p "$T/rt"
live_start() { # live_start [WORDS..]: dev/load.fib under `run --live WORDS` in the background; sets live_pid and sock (empty when it did not come up)
  XDG_RUNTIME_DIR=$T/rt "$fibc" run --live "$@" dev/load.fib > "$T/live.out" 2> "$T/live.err" & live_pid=$!
  sock=
  for _ in $(seq 900); do
    s=$(sed -n 's/^fibc: live at //p' "$T/live.err" | head -n 1)
    if [ -n "$s" ] && [ -S "$s" ] && grep -q 'app: ready' "$T/live.out"; then sock=$s; break; fi
    kill -0 "$live_pid" 2> /dev/null || break
    sleep 0.1
  done
}
live_ask() { printf '%s\n' "$1" | XDG_RUNTIME_DIR=$T/rt "$fibc" repl --connect "$sock" --json 2> /dev/null; }
live_stop() { live_ask '{"op":"eval","text":"(reset! stop true)"}' > /dev/null; wait "$live_pid"; }
live_start
if [ -z "$sock" ]; then echo "FAIL  the live app did not come up"; tail -5 "$T/live.err"; bad=1; kill "$live_pid" 2> /dev/null
else
  a=$(live_ask "{\"op\":\"load\",\"file\":\"$util_src/acme/util.fib\"}")
  ck "a load of a library module under --live is live/NotLive" "$(echo "$a" | grep -o '"code":"live/NotLive"' | head -n 1)" '"code":"live/NotLive"'
  ck "the hint names the flag and the library" "$(echo "$a" | grep -c 'start with --live-deps=acme/util to make it live')" 1
  ck "status says the project rule" "$(live_ask '{"op":"status"}' | grep -c '"live":"project paths (')" 1
  live_stop
fi
sed 's/(\* 10 (b\/base-value))/(* 11 (b\/base-value))/' "$util_src/acme/util.fib" > "$T/util.fib"
grep -q '(\* 11 (b/base-value))' "$T/util.fib" || { echo "FAIL  the fixture's util-value is not the one this test edits"; bad=1; }
live_start --live-deps
if [ -z "$sock" ]; then echo "FAIL  the live app did not come up under --live-deps"; tail -5 "$T/live.err"; bad=1; kill "$live_pid" 2> /dev/null
else
  ck "before the load the program reads 10" "$(live_ask '{"op":"eval","text":"@seen"}' | grep -o '"value":"[0-9]*"')" '"value":"10"'
  a=$(live_ask "{\"op\":\"load\",\"file\":\"$T/util.fib\"}")
  ck "under --live-deps the library module's function swaps" "$(echo "$a" | grep -o '"swapped":\["acme.util/util-value"\]')" '"swapped":["acme.util/util-value"]'
  sleep 0.3
  ck "the running program's loop reads the new body" "$(live_ask '{"op":"eval","text":"@seen"}' | grep -o '"value":"[0-9]*"')" '"value":"11"'
  live_stop
fi
[ "$bad" = 0 ] && echo "live project: ok" || echo "live project: FAILED"
exit "$bad"
