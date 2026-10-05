#!/bin/bash
# Mutation check of the exclusive-view rules (compiler/own/views.fib, own/walk/call.fib, lib/fib/view.fib, lib/fib/view/tiles.fib) and of the inline lowering
# of windows (compiler/emit/lower/window.fib; spec/types.md 6.15): each mutant plants
# one fault in one rule, builds a stage 2 with it, runs the cases that pin that rule (cases/ownership 336 to 364) and passes only if at least one of
# them FAILS. A mutant that survives means a case is missing. The sources are restored after every mutant (and on exit); run it on a clean tree.
# usage: mutant-views.sh BUILDER [OUTDIR]      BUILDER: a fibc that builds compiler/fibc.fib (a seed or a stage 2)
# Exit: 0 every mutant was killed, 1 one survived, 2 the plant did not apply or the build failed.   ONLY=name runs one mutant.
set -u
builder=${1:?usage: mutant-views.sh BUILDER [OUTDIR]}
root=$(cd "$(dirname "$0")/.." && pwd)
out=${2:-$HOME/.cache/fibber-scratch/mutant-views}
mkdir -p "$out/tmp" "$out/orig"
export FIB_LIB=$root/lib TMPDIR=$out/tmp
ulimit -v 16000000
files="compiler/own/views.fib compiler/own/walk/call.fib lib/fib/view.fib lib/fib/view/tiles.fib compiler/emit/lower/window.fib"
for f in $files; do mkdir -p "$out/orig/$(dirname "$f")"; cp "$root/$f" "$out/orig/$f"; done
restore() { for f in $files; do cp "$out/orig/$f" "$root/$f"; done; }
trap restore EXIT
survived=0
mutant() { # NAME FILE SED-EXPRESSION CASE-PREFIXES..
  local name=$1 file=$2 expr=$3; shift 3; [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && return 0
  restore
  sed -i -e "$expr" "$root/$file"
  if cmp -s "$root/$file" "$out/orig/$file"; then echo "PLANT-FAILED $name: the sed expression changed nothing"; exit 2; fi
  (cd "$root" && "$builder" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$out/Fm") > "$out/$name.build" 2>&1 \
    || { echo "BUILD-FAILED $name: $(tail -2 "$out/$name.build" | tr '\n' ' ')"; exit 2; }
  local res
  if [ "$1" = "ir" ]; then # killed when the lIR test of the lowering fails
    if (cd "$root" && WINDOWS_SCRATCH="$out/windows" compiler/tests/emit/windows.sh "$out/Fm" > "$out/$name.ir" 2>&1); then res="the lIR test passed"; else res="the lIR test failed (0 pass)"; fi
    case $res in *"passed"*) res=" 0 fail" ;; esac
  else
    res=$(cd "$root" && "$out/Fm" cases cases/ownership --only "$@" -j 3 2>&1 | tail -1)
  fi
  case $res in
    *" 0 fail"*) echo "SURVIVED $name: $res"; survived=1 ;;
    *) echo "killed   $name: $res" ;;
  esac
}
V=compiler/own/views.fib
# L1 off: any place is a private cell
mutant l1-off $V 's/(if (contains? (. cx private) t)$/(if true/' 346- 347-
# L2 off for the extent (the body may use the owner) and for the lender operands
mutant l2-extent-off $V 's/l2b (match (find-hit (. cx g) mode t ext) ((some p) \[(view-error p text)\]) (nil \[\]))\]/l2b []]/' 338- 339- 353-
mutant l2-operands-off $V 's/l2a (match (operands-hit (. cx g) args t mode) ((some p) \[(view-error p text)\]) (nil \[\]))/l2a []/' 340-
# a read-only lend freezes nothing (a writer is allowed inside a read lend), and a read-only lend freezes reads too (no many readers)
mutant ro-freezes-nothing $V 's/mode (if (= (. m kind) 2) 1 0)/mode (if (= (. m kind) 2) 7 0)/' 353-
mutant ro-freezes-reads $V 's/mode (if (= (. m kind) 2) 1 0)/mode 0/' 354-
# L3 off: a lender may be called anywhere
mutant l3-off $V 's/(and (not unsafe) (not under) (lender? g (. cx scoped) f))/false/' 348-
# L6 off: a scoped type may be constructed anywhere
mutant l6-off $V 's/(if (and (not unsafe) (contains? (. cx scoped) t))/(if false/' 351-
# VC1, the half after the ownership pass: a view cell may be read anywhere
mutant vc1-owned-off $V 's/reads (if (empty? (vec cells)) \[\] (non-peeks (. d body) cells peeks \[\]))/reads []/' 341- 342- 343-
# S1 on the parameters of scoped type off
mutant s1-param-off $V 's/bad (and (not (. q amp)) (scoped-binding? p scoped (. q binding)) (< i (count summary))/bad (and false (< i (count summary))/' 352-
# VC2, VC3, VC4, L5: the searches that find them never hit
mutant vc2-off $V 's/(search-error (. cx g) 3 v ext/(search-error (. cx g) 99 v ext/' 344-
mutant vc3-off $V 's/(search-error (. cx g) 4 v ext/(search-error (. cx g) 99 v ext/' 350-
mutant vc4-off $V 's/(search-error (. cx g) 5 v ext/(search-error (. cx g) 99 v ext/' 349-
mutant l5-off $V 's/(search-error (. cx g) 6 v ext/(search-error (. cx g) 99 v ext/' 345-
# the walker lets a view cell be a peek at an escaping position (a window stored through a protocol method is then not seen)
mutant strict-peek-off compiler/own/walk/call.fib 's/(or (not (contains? @(. w strict) b)) (not (. (nth params i) escapes)))/true/' 343-
# the lender: no unique test (the owner's other holder sees the writes), always a copy (no in-place write), bounds against the buffer, a cut one off
L=lib/fib/view.fib
mutant lender-no-unique $L 's/^  (when (> (array-len @a) 0) (array-set! &a 0 (array-get @a 0))))/  unit)/' 356- 358-
mutant lender-always-copies $L 's/^  (when (> (array-len @a) 0) (array-set! &a 0 (array-get @a 0))))/  (when (> (array-len @a) 0) (let ((keep @a)) (do (array-set! \&a 0 (array-get @a 0)) (array-len keep)))))/' 357-
# the library's own checks (the slow path of the inline access, the whole access for a type the lowering leaves alone): against the buffer, not the window
mutant window-get-bounds-of-the-buffer $L 's/(do (win-check (win-len w) i)/(do (win-check 1000000 i)/' 373-
mutant window-set-bounds-of-the-buffer $L 's/(do (win-check n i)$/(do (win-check 1000000 i)/' 359- 372-
mutant split-cut-off-by-one $L 's/(unsafe (WinPair (Win base k seed) (Win (ptr+ base (\* w k)) (- n k) seed)))/(unsafe (WinPair (Win base (+ k 1) seed) (Win (ptr+ base (* w k)) (- n k) seed)))/' 362-
mutant range-check-off $L 's/(when (or (< lo 0) (> lo hi) (> hi n))/(when false/' 361-
# the inline lowering of windows (emit.lower.window): the bound off by one, no bound, one register pair for every window (a store to the wrong base
# after a split), the element width of 8 for every type, the lowering of a read left out, and the hand-over of the view cell itself left out
W=compiler/emit/lower/window.fib
mutant window-bound-off-by-one $W 's/"(icmp ult " (v-text i) " " (v-text n) ")"/"(icmp ule " (v-text i) " " (v-text n) ")"/' 359- 372- 373-
mutant window-bound-dropped $W 's/(ok (fb-val (. cx b) (str-join \["(icmp ult " (v-text i) " " (v-text n) ")"\]) LirI1))/(ok (VVal "(i1 1)" LirI1))/' 359- 372- 373-
mutant window-registers-shared $W 's/(assoc @(. cx hoisted) (v-text w) fl)/(assoc @(. cx hoisted) "wreg" fl)/; s/(match (get @(. cx hoisted) (v-text w))$/(match (get @(. cx hoisted) "wreg")/' 371- 362-
mutant window-width-of-8 $W 's/" (i64 " (str width) "))"/" (i64 8))"/' 374-
mutant window-read-not-lowered $W 's/(starts-with? name "f.fib.view.win-get.")/(starts-with? name "f.fib.view.win-gXt.")/' ir
mutant window-own-cell-off compiler/own/walk/call.fib 's/^        (contains? @(. w strict) b) PsOwnCell$/        false PsOwnCell/' ir
# with-tiles: overlapping tiles (each starts one element early), no join (the combinator returns while the tasks run), no unique test before the tasks
T=lib/fib/view/tiles.fib
mutant tiles-overlap $T 's/(lo (quot (\* n t) k))/(lo (if (= t 0) 0 (- (quot (* n t) k) 1)))/' 368- 375-
mutant tiles-no-join $T 's/(doseq \[t @tasks\] (join t))/unit/' 377-
mutant tiles-no-unique $T 's/^      (array-set! &a 0 (array-get @a 0))$/      unit/' 378-
[ $survived -eq 0 ] && echo "mutant-views: every mutant was killed"
exit $survived
