#!/bin/bash
# scripts/mutant-persist.sh: planted faults in the persistent checker (docs/design/persistent-checker.md). Copies compiler/ and lib/ of this tree to a scratch
# directory, applies ONE mutant (the checker's kept state drops a piece of state, or shares one with the programs, or a key ignores what it must not),
# builds the unit program compiler/tests/own/unit-staged.fib and the `emit` tool from the copy with a stage 2 (FIBC), and runs the two controls:
#   unit   the unit program (programs through the kept state against the whole check, the key tests)
#   batch  `emit --batch` under FIB_CHECK_CACHE=verify over compiler/tests/persist/*.fib and the first 20 inputs of the emit-fns golden (each input checked both
#          ways; the kept state compared before and after)
# A mutant is KILLED when at least one control fails (a wrong answer, a mismatch, a trap or a build error all count). A mutant that both pass means a piece of
# state has no test that can fail.
# usage: scripts/mutant-persist.sh [MUTANT..]        (default: all; MUT_JOBS mutants at a time, default 3)
#   instance-index    types.persist: the copy of the globals starts with an empty instance index
#   expr-counter      types.persist: the copy of the globals starts its expression ids at 0
#   elab-counter      types.persist: the copy of the globals starts its elaboration ids at 0
#   store-nodes       types.persist: the copy of the checker starts with an empty substitution
#   resolutions       types.persist: the copy of the checker's tables has no resolutions
#   summaries         own.staged: the ownership pass of the program starts without the summaries of the library
#   program-takes     own.staged: the functions the program takes as values are not added to the library's
#   method-takes      own.staged: the library's method values are not run against the program's instances
#   hoisted           own.staged: the methods the library's pass decided ahead of their place are forgotten
#   known             own.staged: the methods of the library are not among those the program's pass may decide ahead
#   tables-shared     types.table: a copy of a table is the table itself (a program writes into the kept state)
#   key-ignored       own.staged: the kept state is used for any key
#   key-no-prelude    expand.shared: the key of the library does not hold the text of the prelude
#   key-no-texts      expand.shared: the key of the library does not hold the text of its modules
#   elab-ids          types.elab: the elaboration takes its ids from the counter of lowering
#   reelaborate       own.staged: the program's elaboration starts at the first row (the library's bodies are elaborated again)
# Run on request, not by default: they SURVIVE, because the plant changes nothing a program can observe today. method-takes: no library body can take a
# method value without a resolved instance (the emitter refuses it: "a method value without a resolved instance"), so there is no library value to run
# against the program's instances. hoisted, known: a method decided ahead of its place is decided again with the same result (only the order of the
# bodies in the ownership program changes, which the emitter does not read). reelaborate: elaborating a body twice gives the same body.
#   no-intact-check   own.staged: the control that a run left the kept state alone always says yes (with tables-shared this one must live)
# The program-prefix memo (own.chain, docs/design/live-repl.md package LV4); the unit program's chain tests and the server control see these:
#   chain-key-ignored         own.chain: the kept state after a program module is used whatever the key (a stale state answers an edited module)
#   prefix-key-ignores-text   serve.incr: the keys the chain keeps a program module's state under hold the module's name, not its text (stale)
# The third control, `server`, runs for the mutants named in SERVER_MUTANTS only: it builds `fibc` from the copy (a minute) and runs the server
# part of compiler/tests/persist/run.sh with it (a two-module program whose first module is edited must be rejected).
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
fibc=${FIBC:-$root/build/F}
out=${MUT_OUT:-${TMPDIR:-/tmp}/mutant-persist}
llvm=${LLVM_LIBDIR:-/usr/lib/llvm-21/lib}
jobs=${MUT_JOBS:-3}
all="instance-index expr-counter elab-counter store-nodes resolutions summaries program-takes tables-shared key-ignored key-no-prelude key-no-texts elab-ids chain-key-ignored prefix-key-ignores-text"
server_mutants=" prefix-key-ignores-text "
want=("$@"); [ ${#want[@]} -eq 0 ] && want=($all)
mkdir -p "$out"

# plant NAME DIR: edit the copy in DIR for mutant NAME (file, old text, new text); the edit must change the file
plant() {
  python3 - "$1" "$2" <<'EOF'
import sys
name, d = sys.argv[1], sys.argv[2]
M = {
 'instance-index': ('compiler/types/persist.fib', '(cell @(. g instance-index))', '(cell (map-empty))'),
 'expr-counter': ('compiler/types/persist.fib', '(cell @(. g expr-count))', '(cell 0)'),
 'elab-counter': ('compiler/types/persist.fib', '(cell @(. g elab-count))', '(cell 0)'),
 'store-nodes': ('compiler/types/persist.fib', '(Store (cell @(. s nodes))', '(Store (cell (map-empty))'),
 'resolutions': ('compiler/types/persist.fib', '(cell @(. t resolutions))', '(cell (map-empty))'),
 'summaries': ('compiler/own/staged.fib', '(TopState (with (. top prog) (value-taken vt) (methods-taken mt))', '(TopState (with (. top prog) (summaries (map-empty)) (value-taken vt) (methods-taken mt))'),
 'program-takes': ('compiler/own/staged.fib', '(vt (into (. (. top prog) value-taken) (vec (value-taken-of tvs))))', '(vt (. (. top prog) value-taken))'),
 'method-takes': ('compiler/own/staged.fib', '''(mt (into (into (. (. top prog) methods-taken) (vec (methods-taken-of p tvs 0)))
                     (vec (methods-taken-of p (. pre taken) (. c insts)))))''', '''(mt (into (. (. top prog) methods-taken) (vec (methods-taken-of p tvs 0))))'''),
 'hoisted': ('compiler/own/staged.fib', '[] (. top hoisted)) mine)))', '[] (set-empty)) mine)))'),
 'known': ('compiler/own/staged.fib', '(known (into (. pre known) (vec (method-units-in mine))))', '(known (method-units-in mine))'),
 'tables-shared': ('compiler/types/table.fib', '''(defun table-copy (t: (Table a)) -> (Table a)
  (Table (cell @(. t rows))))''', '''(defun table-copy (t: (Table a)) -> (Table a)
  t)'''),
 'key-ignored': ('compiler/own/staged.fib', '((some p) :when (= @(. m key) key) (Ok p))', '((some p) (Ok p))'),
 'key-no-prelude': ('compiler/expand/shared.fib', '''(show (hash prelude)) ":" (show (str-len prelude))''', '''"0"'''),
 'key-no-texts': ('compiler/expand/shared.fib', '''(str-join (mapv (fn (t: str) (str-join [(show (hash t)) ":" (show (str-len t)) ","])) texts))''', '''(show (count texts))'''),
 'elab-ids': ('compiler/types/elab.fib', '(g-next-elab-expr (. el g))', '(g-next-expr (. el g))'),
 'reelaborate': ('compiler/own/staged.fib', '(elaborate-from g (. ck t) (. c funs) (. c defs) (. c insts))', '(elaborate-from g (. ck t) 0 0 0)'),
 'no-intact-check': ('compiler/own/staged.fib', '(or (not (= (cache-mode) "verify")) (= (state-print (. pre lw) (. pre ck)) (. pre print)))', 'true'),
 'chain-key-ignored': ('compiler/own/chain.fib', '(match (get @(. m chain) (nth keys (- i 1)))', '(match (first (vals @(. m chain)))'),
 'prefix-key-ignores-text': ('compiler/serve/incr.fib', '''(defun check-keys :private (keys: (Vec i64) loaded: (Vec Loaded)) -> (Vec str)
  (mapv show keys))''', '''(defun check-keys :private (keys: (Vec i64) loaded: (Vec Loaded)) -> (Vec str)
  (mapv (fn (l: Loaded) (. (. l spec) ns)) loaded))'''),
}
f, old, new = M[name]
p = d + '/' + f
s = open(p).read()
if old not in s:
    sys.exit('mutant %s: the text to change is not in %s' % (name, f))
open(p, 'w').write(s.replace(old, new, 1))
EOF
}

# one NAME: the verdict line
one() {
  local n=$1 d=$out/$1 r
  rm -rf "$d"; mkdir -p "$d"
  cp -r "$root/compiler" "$root/lib" "$d/"
  if [ "$n" = no-intact-check ]; then plant tables-shared "$d" || { echo "ERROR $n: could not plant"; return 2; }; fi
  plant "$n" "$d" || { echo "ERROR $n: could not plant"; return 2; }
  local unit=lived batch=lived
  ( cd "$d" && "$fibc" build compiler/tests/own/unit-staged.fib -I compiler -I lib -L "$llvm" -l LLVM-21 -o "$d/unit" > "$d/unit.build" 2>&1 ) || unit=build-error
  ( cd "$d" && "$fibc" build compiler/emit.fib -I compiler -I lib -L "$llvm" -l LLVM-21 -o "$d/emit" > "$d/emit.build" 2>&1 ) || batch=build-error
  if [ "$unit" = lived ]; then ( cd "$d" && FIB_LIB=$d/lib timeout 600 "$d/unit" > "$d/unit.out" 2>&1 ) || unit=failed; fi
  if [ "$batch" = lived ]; then
    local files=("$root"/compiler/tests/persist/*.fib)
    while IFS= read -r l; do files+=("$root/$l"); done < <(head -n 20 "$root/compiler/tests/golden/emit-fns.files")
    ( cd "$d" && FIB_LIB=$d/lib FIB_PRUNE=0 FIB_CHECK_CACHE=verify timeout 900 "$d/emit" --batch "${files[@]}" > "$d/batch.out" 2> "$d/batch.err" )
    if grep -q "CHECK-CACHE MISMATCH" "$d/batch.out" || [ -s "$d/batch.err" ]; then batch=failed; fi
    # a clean run of the unmutated tool on the same inputs is the baseline of "failed": any difference from it counts, a changed status too
    if [ "$batch" = lived ] && ! cmp -s "$out/base.out" <(sed "s|$d/lib|$root/lib|g" "$d/batch.out"); then batch=differs; fi   # the copy of lib/ is at another path
  fi
  local server=not-run
  if [[ "$server_mutants" == *" $n "* ]]; then
    server=lived
    ( cd "$d" && "$fibc" build compiler/fibc.fib -I compiler -I lib -L "$llvm" -l LLVM-21 -o "$d/fibc" > "$d/fibc.build" 2>&1 ) || server=build-error
    if [ "$server" = lived ]; then
      ( cd "$root" && PERSIST_OUT=$d/persist timeout 900 "$root/compiler/tests/persist/run.sh" "$d/fibc" server > "$d/server.out" 2>&1 ) || server=failed
    fi
  fi
  if [ "$unit" = lived ] && [ "$batch" = lived ] && [ "$server" != failed ] && [ "$server" != build-error ]; then echo "SURVIVED $n (unit: $unit, batch: $batch, server: $server)"; return 1; fi
  echo "KILLED $n (unit: $unit, batch: $batch, server: $server)"
}

# the baseline: the unmutated tool and its output on the inputs of the batch control
( cd "$root" && "$fibc" build compiler/emit.fib -I compiler -I lib -L "$llvm" -l LLVM-21 -o "$out/emit-base" > "$out/emit-base.build" 2>&1 ) || { echo "ERROR: the unmutated emit tool does not build"; exit 2; }
files=("$root"/compiler/tests/persist/*.fib)
while IFS= read -r l; do files+=("$root/$l"); done < <(head -n 20 "$root/compiler/tests/golden/emit-fns.files")
( cd "$root" && FIB_LIB=$root/lib FIB_PRUNE=0 FIB_CHECK_CACHE=off "$out/emit-base" --batch "${files[@]}" > "$out/base.out" 2>&1 )

status=0
pids=()
for n in "${want[@]}"; do
  ( one "$n" > "$out/$n.verdict" 2>&1; echo $? > "$out/$n.rc" ) &
  pids+=($!)
  while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do sleep 1; done
done
wait
for n in "${want[@]}"; do cat "$out/$n.verdict"; [ "$(cat "$out/$n.rc")" = 0 ] || status=1; done
exit $status
