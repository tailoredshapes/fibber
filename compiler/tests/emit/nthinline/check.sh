#!/bin/bash
# compiler/tests/emit/nthinline/check.sh FIBC: the Vec read path is emitted with lIR's `inline` (spec/lir.md 4.3; compiler/emit/lower/mod.fib
# `inline-body?`, docs/design/lir-facts.md 9 item 1): nth.fib is run (it answers 0) and emitted as lIR, and the prelude's `vec-nth` and the
# `nth` method of Reducible's Vec instance must be `define internal inline`, while the program's own `sum-nth` must not be (the marker is for
# those two bodies only). The stage 2 of f748147c fails the first two. Exit 1 on any difference.
# usage: check.sh FIBC     run from anywhere; FIB_LIB defaults to the tree's lib
fibc=${1:?usage: check.sh FIBC}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
export FIB_LIB=${FIB_LIB:-$root/lib}
ulimit -v 16000000 2>/dev/null
got=$(timeout 300 "$fibc" run "$here/nth.fib"; echo "exit $?")
[ "$got" = $'0\nexit 0' ] || { echo "check.sh: nth.fib answered: $got"; exit 1; }
lir=$(timeout 300 "$fibc" emit "$here/nth.fib" 2>/dev/null) || { echo "check.sh: emit failed"; exit 1; }
bad=0
want() {   # want NAME-PREFIX yes|no
  local line; line=$(grep -o "(define [a-z ]*($1[^ ]*" <<<"$lir" | head -n 1)
  if [ -z "$line" ]; then echo "FAIL $1: not emitted"; bad=1; return; fi
  case "$line" in *" inline "*) has=yes ;; *) has=no ;; esac
  if [ "$has" = "$2" ]; then echo "ok   $1: inline=$has"; else echo "FAIL $1: inline=$has, wanted $2"; bad=1; fi
}
want 'f\.fib\.prelude\.vec-nth\.' yes
want 'm\.fib\.seq\.protocols\.Reducible\.nth\.\$fib\.prelude/Vec\.' yes
want 'f\.sum-nth' no
exit $bad
