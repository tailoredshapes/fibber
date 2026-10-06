#!/bin/bash
# compiler/tests/deps/fixtures.sh: the git fixtures of the package manager's tests (docs/design/packages.md §7), sourced by run.sh. No
# network: every library is a bare repository under $T/remotes made here, reached by a file:// URL. After `make_fixtures`:
#   base    acme/base   commits B1 (base-value 1, tag v1) and B2 (base-value 2, tag v2)
#   util    acme/util   U1 (tag v1): depends on base at B1 by sha; module acme.util with a function and a macro `twice`
#   other   acme/other  O1 (tag v1): base at B1 (a diamond with util); O2 (tag v2): base at B2 (a conflict with util)
#   clash   acme/clash  K1 (tag v1): defines the module acme.util too (a collision)
#   tagged  acme/tagged G1 (tag t1: tagged-value 1), G2 (tagged-value 2; `move_tag` moves t1 there)
# The shas are in B1 B2 U1 O1 O2 K1 G1 G2; the URLs in URL_base, URL_util, ...
export GIT_AUTHOR_NAME=fixture GIT_AUTHOR_EMAIL=fixture@example.org GIT_COMMITTER_NAME=fixture GIT_COMMITTER_EMAIL=fixture@example.org
export GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'

repo() { # repo NAME: a work tree $T/work-repos/NAME and its bare remote $T/remotes/NAME.git
  git init -q -b main "$T/work-repos/$1"
  git init -q --bare "$T/remotes/$1.git"
  eval "URL_$1=file://$T/remotes/$1.git"
}
put() { # put NAME PATH TEXT: a file of the work tree
  mkdir -p "$(dirname "$T/work-repos/$1/$2")"; printf '%s\n' "$3" > "$T/work-repos/$1/$2"
}
commit() { # commit NAME MESSAGE [TAG]: the sha on standard output
  git -C "$T/work-repos/$1" add -A
  git -C "$T/work-repos/$1" commit -q -m "$2"
  [ -n "${3:-}" ] && git -C "$T/work-repos/$1" tag -f "$3" > /dev/null
  git -C "$T/work-repos/$1" push -q -f "$T/remotes/$1.git" main "refs/tags/*:refs/tags/*" 2> /dev/null
  git -C "$T/work-repos/$1" rev-parse HEAD
}
move_tag() { # move_tag NAME TAG SHA: the tag moved on the remote
  git -C "$T/work-repos/$1" tag -f "$2" "$3" > /dev/null
  git -C "$T/work-repos/$1" push -q -f "$T/remotes/$1.git" "refs/tags/$2" 2> /dev/null
}

make_fixtures() {
  mkdir -p "$T/work-repos" "$T/remotes"
  repo base; repo util; repo other; repo clash; repo tagged
  put base deps.fib '{:name "acme/base" :version "1.0.0" :paths ["src"]}'
  put base src/acme/base.fib '(ns acme.base)
(defun base-value () -> i64 1)'
  B1=$(commit base "base 1" v1)
  put base src/acme/base.fib '(ns acme.base)
(defun base-value () -> i64 2)'
  B2=$(commit base "base 2" v2)

  put util deps.fib "{:name \"acme/util\" :paths [\"src\"] :deps {acme/base {:git/url \"$URL_base\" :git/sha \"$B1\"}}}"
  put util src/acme/util.fib '(ns acme.util (:require [acme.base :as b]))
(defun util-value () -> i64 (* 10 (b/base-value)))
(defmacro twice (x) `(+ ~x ~x))'
  U1=$(commit util "util 1" v1)

  put other deps.fib "{:name \"acme/other\" :deps {acme/base {:git/url \"$URL_base\" :git/tag \"v1\" :git/sha \"$B1\"}}}"
  put other src/acme/other.fib '(ns acme.other (:require [acme.base :as b]))
(defun other-value () -> i64 (* 100 (b/base-value)))'
  O1=$(commit other "other 1" v1)
  put other deps.fib "{:name \"acme/other\" :deps {acme/base {:git/url \"$URL_base\" :git/sha \"$B2\"}}}"
  O2=$(commit other "other 2" v2)

  put clash src/acme/util.fib '(ns acme.util)
(defun util-value () -> i64 -1)'
  K1=$(commit clash "clash 1" v1)

  put tagged src/acme/tagged.fib '(ns acme.tagged)
(defun tagged-value () -> i64 1)'
  G1=$(commit tagged "tagged 1" t1)
  put tagged src/acme/tagged.fib '(ns acme.tagged)
(defun tagged-value () -> i64 2)'
  G2=$(commit tagged "tagged 2")
}

project() { # project NAME DEPS-FIB MAIN-BODY: $T/p/NAME with deps.fib and src/main.fib (`main` returns MAIN-BODY, printed by `run`)
  mkdir -p "$T/p/$1/src"
  printf '%s\n' "$2" > "$T/p/$1/deps.fib"
  printf '%s\n' "$3" > "$T/p/$1/src/main.fib"
}
