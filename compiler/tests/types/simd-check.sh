#!/bin/bash
# The judge of SIMD wave 2 package P1 (lane vectors in the type checker; new in stage 2, the frozen Rust has no oracle):
# runs the stage-2 types tool on each compiler/tests/types/simd/NAME.fib and compares the lines that matter with NAME.expect.
#   usage: [FIBC=F] simd-check.sh TOOL [-u] [NAME..]   TOOL = the built compiler/types.fib; -u rewrites the .expect files (review them!)
#   The files named oNN-*.fib are ownership cases: `FIBC explain FILE` (a stage-2 fibc, required for them) is compared whole.
# The lines that matter: each `fun` line (the type the checker gave a function) and the error record (`error`, `message`,
# `position`) of a file that does not type-check; `unit scc`, headers and positions of unrelated lines are left out.
# Exit 0 when every file matches. Run from the repository root.
tool=$1; shift
update=0; [ "${1:-}" = "-u" ] && { update=1; shift; }
dir=compiler/tests/types/simd
names=("$@"); [ ${#names[@]} -eq 0 ] && names=($(ls $dir/*.fib | sed 's|.*/||; s|\.fib$||'))
bad=0
for n in "${names[@]}"; do
  case $n in
    o*) got=$( (ulimit -v 4000000; timeout 120 "${FIBC:?FIBC names the stage-2 fibc for the o cases}" explain $dir/$n.fib 2>&1) ) ;;
    *) got=$( (ulimit -v 4000000; timeout 120 "$tool" $dir/$n.fib 2>&1) | grep -E '^(fun |error|message|kind|  message|  kind)|error' | sed -E 's/ [0-9]+:[0-9]+ [0-9]+\.\.[0-9]+$//' ) ;;
  esac
  if [ $update = 1 ]; then printf '%s\n' "$got" > $dir/$n.expect; echo "wrote $n"; continue; fi
  if [ "$got" = "$(cat $dir/$n.expect 2>/dev/null)" ]; then echo "same $n"; else echo "DIFF $n"; diff <(printf '%s\n' "$got") $dir/$n.expect | head -8; bad=1; fi
done
exit $bad
