#!/bin/bash
# PRUNE-1 (compiler/emit/prune.fib): the lIR of a program does not hold what nothing reaches, and holds everything something does.
# Each check asserts on `fibc emit` output (what is absent, what is present) or runs the built executable (a `def` that is needed must still
# be made, an initialiser that does something must still run). The absent checks are paired with the same text from FIB_PRUNE=0, which must hold
# the thing: a check that could not fail is not a check.
#   compiler/tests/emit/prune.sh FIBC [SCRATCH]       exit 0 when all hold
set -u
FIBC=${1:?usage: prune.sh FIBC [SCRATCH]}
R=$(cd "$(dirname "$0")/../../.." && pwd); S=${2:-$HOME/.cache/fibber-scratch/prune}
mkdir -p "$S"; cd "$R" || exit 2
export FIB_LIB=$R/lib
ulimit -v 16000000
bad=0
fail() { echo "FAIL $1"; bad=1; }
emit() { # NAME  -> $S/NAME.lir (pruned) and $S/NAME.off.lir (FIB_PRUNE=0)
  "$FIBC" emit "$S/$1.fib" > "$S/$1.lir" 2> "$S/$1.err" || { fail "$1: emit: $(head -c 200 "$S/$1.err")"; return 1; }
  FIB_PRUNE=0 "$FIBC" emit "$S/$1.fib" > "$S/$1.off.lir" 2> "$S/$1.err" || { fail "$1: emit (FIB_PRUNE=0)"; return 1; }
}
has() { grep -qF -- "$2" "$S/$1"; }
absent() { # NAME TEXT WHAT: gone when pruned, there when not
  if has "$1.lir" "$2"; then fail "$1: $3 is still in the pruned text"; else
    if has "$1.off.lir" "$2"; then echo "ok $1: $3 pruned"; else fail "$1: $3 is not in the unpruned text either (the check cannot fail)"; fi; fi
}
never() { # NAME TEXT WHAT: in neither text, because the emitter only asks for the bodies a program reaches (emit.mono's queue)
  if has "$1.lir" "$2" || has "$1.off.lir" "$2"; then fail "$1: $3 is in the text"; else echo "ok $1: $3 not emitted"; fi
}
present() { if has "$1.lir" "$2"; then echo "ok $1: $3 kept"; else fail "$1: $3 was dropped"; fi; }
run_status() { # NAME WANT-STATUS [WANT-OUTPUT]
  "$FIBC" build "$S/$1.fib" -o "$S/$1" > "$S/$1.blog" 2>&1 || { fail "$1: build: $(head -c 200 "$S/$1.blog")"; return; }
  "$S/$1" > "$S/$1.out" 2>&1; local rc=$?
  if [ "$rc" != "$2" ]; then fail "$1: status $rc, want $2"; return; fi
  if [ $# -ge 3 ] && [ "$(cat "$S/$1.out")" != "$3" ]; then fail "$1: output '$(head -c 100 "$S/$1.out")', want '$3'"; return; fi
  echo "ok $1: status $rc"
}

# hello world: the runtime keeps what printing a string needs and drops the number formatting, the exceptions, the object copy
cat > "$S/hello.fib" <<'EOF'
(defun main () -> i64 (do (println "hello") 0))
EOF
emit hello && {
  absent hello "(fib.show-double " "the double formatter"
  absent hello "(fib.raise-object " "the exception raise"
  absent hello "(fib.copy-object " "the object copy"
  present hello "f.fib.prelude.write-str" "the string writer"
}
run_status hello 0 hello

# printing a double needs the formatter: not dropped
cat > "$S/dbl.fib" <<'EOF'
(defun main () -> i64 (do (println 1.5) 0))
EOF
emit dbl && present dbl "(fib.show-double " "the double formatter of a program that prints one"
run_status dbl 0 1.5

# a program that uses one function of a big module holds that function and not the others
cat > "$S/one.fib" <<'EOF'
(ns main (:require [fib.string :as s]))
(defun main () -> i64 (count (s/trim "  ab  ")))
EOF
emit one && {
  present one "fib.string.trim" "the trim it calls"
  never one "fib.string.split" "split"
  never one "fib.string.replace" "replace"
}
run_status one 2

# `def`s made at run time: a module's unused ones are not made (fib.unix makes a dozen); the needed one is, and what it reads
cat > "$S/defs.fib" <<'EOF'
(ns main (:require [fib.unix :as unix]))
(defun mk (n: i64) -> i64 (* n 2))
(def a: i64 (mk 10))
(def unused: i64 a)
(def called: i64 (mk 5))
(def b: i64 (+ a 1))
(def used: i64 (mk b))
(defun main () -> i64 used)
EOF
emit defs && {
  absent defs "def unused: " "the unread def that only reads another def"
  present defs "def called: " "the unread def that calls a function (it might trap)"
  absent defs "def O_CREAT: " "fib.unix's O_CREAT"
  present defs "def used: " "the def main reads"
  present defs "def b: " "the def the used def reads"
  present defs "def a: " "the def that def reads"
}
run_status defs 42

# a needed def must be made: if it were dropped the slot would be zero (this fails, status 0, with a pruner that drops every def)
cat > "$S/needed.fib" <<'EOF'
(defun mk (n: i64) -> i64 (* n 2))
(def x: i64 (mk 21))
(defun reads-x () -> i64 x)
(defun main () -> i64 (reads-x))
EOF
emit needed && present needed "def x: " "the def a function main calls reads"
run_status needed 42

# an initialiser that does something stays, read or not (it prints), in its order
cat > "$S/effect.fib" <<'EOF'
(def noisy: i64 (do (println "boot") 1))
(defun main () -> i64 (do (println "main") 0))
EOF
emit effect && present effect "def noisy: " "the def whose initialiser prints"
run_status effect 0 "$(printf 'boot\nmain')"

# a def of an atom that a function reads is kept (and the swap on it works)
cat > "$S/atom.fib" <<'EOF'
(def counter: (Atom i64) (atom 7))
(defun main () -> i64 (swap! counter inc))
EOF
emit atom && present atom "def counter: " "the atom def"
run_status atom 8

# an unread def whose initialiser can trap still traps before main (syntax 3.19, case 1706): directly, through a function, by overflow, by division by zero
trap_def() { # NAME BODY-OF-THE-FILE WANT-IN-OUTPUT
  printf '%s\n(defun main () -> i64 0)\n' "$2" > "$S/$1.fib"
  emit "$1" || return
  present "$1" "def $3: " "the unread def that traps"
  "$FIBC" build "$S/$1.fib" -o "$S/$1" > "$S/$1.blog" 2>&1 || { fail "$1: build"; return; }
  ( "$S/$1" > "$S/$1.out" 2>&1 ) 2> /dev/null; local rc=$?
  if [ "$rc" != 0 ] && grep -q "def $3" "$S/$1.out"; then echo "ok $1: traps before main (status $rc)"; else fail "$1: status $rc, output '$(head -c 100 "$S/$1.out")', want a trap naming def $3"; fi
}
trap_def trap-direct '(def broken: i64 (trap "config missing"))' broken
trap_def trap-fn '(defun load-config (n: i64) -> i64 (if (> n 0) (trap "config missing") n))
(def broken: i64 (load-config 1))' broken
trap_def trap-overflow '(def seed: i64 9223372036854775807)
(def too-big: i64 (+ seed 1))' too-big
trap_def trap-div '(def zero: i64 0)
(def ratio: i64 (quot 10 zero))' ratio
exit $bad
