#!/bin/bash
# Mutation check of fib.autodiff (lib/fib/autodiff, docs/design/autodiff.md 6): each mutant plants one fault in the library, runs the specs
# (specs/ad-ops-spec.fib, specs/ad-core-spec.fib) and passes only if at least one scenario FAILS (`fibc test` exit status 1). Status 2 (a spec that no longer
# compiles) is reported as an ERROR, not a kill: a mutant must compile. The library is read when a program is built, so no stage 2 is rebuilt.
# Sources are restored after every mutant and on exit; run it on a clean tree.
# usage: mutant-autodiff.sh F        F: a stage 2 fibc.   Exit: 0 every mutant killed, 1 one survived, 2 a plant changed nothing or did not compile.   ONLY=name runs one.
set -u
F=${1:?usage: mutant-autodiff.sh F}
root=$(cd "$(dirname "$0")/.." && pwd)
out=$HOME/.cache/fibber-scratch/mutant-autodiff
mkdir -p "$out/orig"
export FIB_LIB=$root/lib
ulimit -v 16000000
files="lib/fib/autodiff/ops.fib lib/fib/autodiff/run.fib lib/fib/autodiff/tape.fib lib/fib/autodiff/nn.fib lib/fib/autodiff/optim.fib lib/fib/autodiff/parallel.fib lib/fib/autodiff/forward.fib"
for f in $files; do mkdir -p "$out/orig/$(dirname "$f")"; cp "$root/$f" "$out/orig/$f"; done
restore() { for f in $files; do cp "$out/orig/$f" "$root/$f"; done; }
trap restore EXIT
survived=0; bad=0
mutant() { # NAME FILE SED-EXPRESSION..
  local name=$1 file=$2; shift 2; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  for expr in "$@"; do sed -i -e "$expr" "$root/$file"; done
  if cmp -s "$root/$file" "$out/orig/$file"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; bad=1; return 0; fi
  local res code
  res=$(cd "$root" && "$F" test specs/ad-ops-spec.fib specs/ad-core-spec.fib -j 4 --seed 1 2>&1); code=$?
  case $code in
    1) echo "killed   $name: $(echo "$res" | grep '^total:' | sed 's/ in .*//')" ;;
    0) echo "SURVIVED $name: $(echo "$res" | grep '^total:')"; survived=1 ;;
    *) echo "ERROR    $name: status $code: $(echo "$res" | grep -A2 rejected | head -3 | tr '\n' ' ')"; bad=1 ;;
  esac
}
O=lib/fib/autodiff/ops.fib; R=lib/fib/autodiff/run.fib; T=lib/fib/autodiff/tape.fib; N=lib/fib/autodiff/nn.fib; P=lib/fib/autodiff/optim.fib; D=lib/fib/autodiff/parallel.fib; W=lib/fib/autodiff/forward.fib
case-mutant() { # NAME FILE SED-EXPRESSION: killed by the audit of case 7710 (a leak is a failed case)
  local name=$1 file=$2 expr=$3; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  sed -i -e "$expr" "$root/$file"
  if cmp -s "$root/$file" "$out/orig/$file"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; bad=1; return 0; fi
  local res; res=$(cd "$root" && "$F" cases cases/stdlib --only 7710 -j 1 2>&1 | tail -1)
  case $res in *" 0 fail"*) echo "SURVIVED $name: $res"; survived=1 ;; *) echo "killed   $name: $res" ;; esac
}
# wrong backward formulas
mutant tanh-drops-the-one-minus $O 's/(t\/mul g (t\/sub (scalar out 1.0) (t\/mul out out)))/(t\/mul g (t\/mul out out))/'
mutant sub-forgets-the-sign $O 's/(unbroadcast (t\/scale (lit (. g seed) -1.0) g) sb)/(unbroadcast g sb)/'
mutant relu-mask-passes-zeros $O 's/(t\/relu (t\/div y y))/(t\/shift (lit (. y seed) 1.0) (t\/scale (lit (. y seed) 0.0) y))/'
mutant bias-column-sums-doubled $O 's/(t\/mmul (t\/full \[1 (nth (t\/shape g) 0)\] (one (. g seed))) g)/(t\/mmul (t\/full [1 (nth (t\/shape g) 0)] (lit (. g seed) 2.0)) g)/'
mutant mean-divides-by-n-plus-one $O 's/k (\/ 1.0 (double n))/k (\/ 1.0 (double (+ n 1)))/'
mutant matmul-right-gradient-is-doubled $N 's/(t\/mmul (t\/transpose av) g)/(t\/mmul (t\/transpose av) (t\/scale (lit (. g seed) 2.0) g))/'
mutant dense-bias-gradient-ignores-the-activation $N 's/(if nb \[(unbroadcast gz bs)\] \[\])/(if nb [(unbroadcast g bs)] [])/'
mutant cross-entropy-drops-the-batch-mean $N 's/(t\/scale (lit (. p seed) k) g)/g/'
mutant fused-dense-drops-the-activation $N 's/(let \[y (t\/dense x w b act)\]/(let [y (t\/dense x w b :identity)]/'
# a dropped broadcast reduction
mutant add-skips-the-reduction-for-b $O 's/(fn () (unbroadcast g sa)) (fn () (unbroadcast g sb)))/(fn () (unbroadcast g sa)) (fn () g))/'
mutant unbroadcast-skips-axes-of-extent-one $O 's/(t\/sum-axis ax true g)/g/'
# a missing accumulation for a tensor used twice
mutant accumulate-overwrites $R 's/((some old) (assoc gs id (some (t\/add old g))))/((some old) (assoc gs id (some g)))/'
# scopes
mutant stop-gradient-keeps-the-id $T 's/(defun stop-gradient (x: (Tracked a)) -> (Tracked a) (Tracked (. x val) -1 (. x tape)))/(defun stop-gradient (x: (Tracked a)) -> (Tracked a) x)/'
mutant no-grad-still-records $T 's/(= @(. (. x tape) live) 1)/true/'
# memory: a saved tensor not kept (released too early), and the tape's chain held to the end of the pass (released too late)
mutant exp-does-not-save-its-output $O 's/(fn (g) (t\/mul g out))))/(fn (g) (t\/mul g (t\/exp (. a val))))))/'
mutant backward-holds-the-whole-chain $R 's/(loop \[gs gs0\]/(loop [gs gs0 keep @(. tp entries)]/' 's/(recur (step gs e))/(recur (step gs e) keep)/'
# a leak: the closure holds the Tracked input, which carries the tape: tape -> entry -> closure -> Tracked -> tape, a cycle no count frees
case-mutant closure-captures-the-tape $T 's/(do (push-entry tp (TapeEntry id \[(. a id)\] (fn (g) \[(back g)\])))/(do (push-entry tp (TapeEntry id [(. a id)] (fn (g) [(back (do (. a tape) g))])))/'
# data-parallel: the shard gradients are combined in a fixed tree; a tree that loses an odd tail, or a mean over the wrong count, is wrong
mutant shard-tree-drops-the-odd-tail $D 's/(nth level (+ (\* 2 k) 1))) (nth level (\* 2 k))))/(nth level (+ (* 2 k) 1))) (t\/sub (nth level (* 2 k)) (nth level (* 2 k)))))/'
mutant shard-mean-uses-the-wrong-scale $D 's/(t\/scale k (tree-add xs))/(tree-add xs)/'
# forward mode: a product rule with a term missing
mutant dual-product-rule-drops-a-term $W 's/(t\/add (t\/mul (. a d) (. b p)) (t\/mul (. a p) (. b d)))))$/(t\/mul (. a d) (. b p))))/'
# the optimisers
mutant adam-without-bias-correction $P 's/c1 (- 1.0 (ipow b1 n))/c1 1.0/'
mutant sgd-adds-the-gradient $P 's/(- 0.0 lr)/lr/'
restore
[ $bad -ne 0 ] && exit 2
exit $survived
