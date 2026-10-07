#!/bin/bash
# scripts/mutant-stack-guard.sh: planted faults for the stack guard and the main stack (rt/thread.lir, docs/design/stack.md; cases 8350-8354 and
# compiler/tests/stack/stack.sh). Copies compiler/ and lib/ to a scratch directory, breaks ONE rule in the runtime text of the copy
# (compiler/emit/runtime.fib, the generated form of rt/thread.lir: the break is made there, so no regeneration is needed), builds a stage 2 from the
# copy with FIBC and runs the cases that pin the rule: at least the named ones must FAIL (a program `fibc run` runs gets the runtime of the stage 2 that
# compiles it, so one build is enough).
# usage: FIBC=fibc scripts/mutant-stack-guard.sh MODE
#   MODE          what is broken                                                  what must fail
#   no-handler    SIGSEGV gets no handler                                          8350 8351 8352 8354 (status 139, nothing said)
#   no-altstack   no thread gets a signal stack: the handler cannot run            8350 8351 8352 8354
#   no-task-guard a task's thread gets no signal stack                             8351
#   any-fault     every fault is called a stack overflow                           8354 (the PROT_NONE read says "stack overflow")
#   window        the window around the end of the stack is empty                  8350 8351 8352 8354
#   no-main-stack the default main stack is 0 MiB: the process's own (8 MiB)       8353
# environment: FIBC (required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-stack-guard-MODE). Run under ulimit -v 16000000.
# exit: 0 when the named cases failed under the mutant, 1 when one survived, 2 for a setup error.
set -uo pipefail
MODE=${1:?usage: mutant-stack-guard.sh no-handler|no-altstack|no-task-guard|any-fault|window|no-main-stack}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-stack-guard-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-stack-guard: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
ulimit -v 16000000
rm -rf "$OUT/tree"; mkdir -p "$OUT/tree/cases" "$OUT/tmp"
export TMPDIR=$OUT/tmp
cp -r "$R/compiler" "$R/lib" "$OUT/tree/"; cp -r "$R/cases/stdlib" "$OUT/tree/cases/"
f=$OUT/tree/compiler/emit/runtime.fib
case $MODE in
  no-handler)    perl -0pi -e 's/\(call \@sigaction \(i32 11\) sa old\)/(add (i32 0) (i32 0))/' "$f"; must="8350 8351 8352 8354" ;;
  no-altstack)   perl -0pi -e 's/\(br \(icmp ne \(call \@sigaltstack ss \(ptr null\)\) \(i32 0\)\) undo out\)/(br out)/' "$f"; must="8350 8351 8352 8354" ;;
  no-task-guard) perl -0pi -e 's/\(call \@fib\.guard-enter \(load i64 \@fib\.thread-stack\)\)\n    \(ret\)/(ret)/' "$f"; must="8351" ;;
  any-fault)     perl -0pi -e 's/\(icmp ult \(sub addr \(sub lo w\)\) \(shl w \(i64 1\)\)\) overflow unknown/(icmp eq (i64 0) (i64 0)) overflow unknown/' "$f"; must="8354" ;;
  window)        perl -0pi -e 's/\(global internal fib\.guard-window i64 \(i64 1048576\)\)/(global internal fib.guard-window i64 (i64 0))/' "$f"; must="8350 8351 8352 8354" ;;
  no-main-stack) perl -0pi -e 's/\(block default \(ret \(i64 256\)\)\)/(block default (ret (i64 0)))/' "$f"; must="8353" ;;
  *) echo "mutant-stack-guard: unknown MODE $MODE" >&2; exit 2 ;;
esac
cmp -s "$f" "$R/compiler/emit/runtime.fib" && { echo "mutant-stack-guard: the mutation changed nothing" >&2; exit 2; }
# Twice: the first stage 2 carries the broken runtime in its emitter, and the second is the one whose OWN runtime is broken too (`fibc cases` runs each
# case as a child `fibc run`, and that process has the guard of the runtime it was built with: with only one build the unbroken handler of the
# compiler's own process would still catch the fault, and the mutant would survive for the wrong reason).
(cd "$OUT/tree" && "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F1" \
  && "$OUT/F1" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F") > "$OUT/build.log" 2>&1 \
  || { tail -5 "$OUT/build.log"; echo "mutant-stack-guard: the mutant did not build" >&2; exit 2; }
export FIB_LIB=$OUT/tree/lib
all=$(cd "$OUT/tree" && "$OUT/F" cases cases/stdlib --only 8350- 8351- 8352- 8353- 8354- 2>&1) || true
rc=0
for n in $must; do
  line=$(printf '%s\n' "$all" | grep -E "^$n-" | head -1)
  case $line in
    *" pass"*) echo "SURVIVED  $MODE: $line"; rc=1 ;;
    *) echo "killed    $MODE: ${line:0:150}" ;;
  esac
done
exit $rc
