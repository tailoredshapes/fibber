#!/usr/bin/env python3
"""Planted-fault evidence for the heap unit programs (compiler/tests/fibref/heap-*.fib), run by heap-mutants.sh.

Each mutant breaks ONE rule of compiler/fibref (cascade order, the double-free check, a leak class, cycle detection, the unique test, a stack rule, ...)
in a scratch copy and builds the unit programs against the copy (-I OVERLAY before -I compiler). A mutant is KILLED when some unit program fails (a FAIL
line and a non-zero exit); it SURVIVES when every program still passes (a gap in the tests: the script exits 1); it is INVALID when its pattern is not found
exactly once or the mutated tree does not build (also exit 1: a mutant that cannot compile proves nothing).
usage: heap-mutants.py ROOT FIBC SCRATCH [-j N] [NAME-PREFIX ...]
"""
import os, re, shutil, subprocess, sys
from concurrent.futures import ThreadPoolExecutor

# (name, file under compiler/fibref, old text, new text); `old` must occur exactly once in the file.
M = [
 ("cascade-order-wrong", "heap/cascade.fib", "(into rest (. r push))", "(into (. r push) rest)"),
 ("cascade-skips-children", "heap/cascade.fib", "(Ok (HpVisit p2 (if (= c2 0) (hp-held-refs-rev h id pending) (vec-empty))))", "(Ok (HpVisit p2 (vec-empty)))"),
 ("double-free-undetected", "heap/cascade.fib", "(if (or (not (hp-live? h id)) (= c 0))", "(if false"),
 ("retain-of-freed-accepted", "heap/life.fib", "((Ok true) (match (hp-live-object h id HpRetain)", "((Ok true) (match (Ok ())"),
 ("leak-class-ignores-count", "heap/audit.fib", "(and (contains? cel n) (= (nth (. g counts) n) (nth (. g held) n)))", "(contains? cel n)"),
 ("cell-cycle-reach-dropped", "heap/audit.fib", "(into rest (nth (. g adj) node))", "rest"),
 ("immutable-cycle-missed", "heap/audit.fib", "(hp-cyclic-nodes (hp-immutable-edges g))", "#{}"),
 ("dangling-not-reported", "heap/audit.fib", "(conj (. e dangling) (HpDangling holder field target))", "(. e dangling)"),
 ("pinned-reported-as-leak", "heap/audit.fib", "(hp-pinned-graph h)", "#{}"),
 ("cycle-missed-entirely", "heap/scc.fib", "(into #{} @(. t cyclic))", "#{}"),
 ("self-loop-not-a-cycle", "heap/scc.fib", "(if (or (> (count members) 1) (hp-self-loop? adj node))", "(if (> (count members) 1)"),
 ("scc-lowlink-not-propagated", "heap/scc.fib", "((some p) (hp-t-lower t p (nth @(. t low) node)))", "((some p) ())"),
 ("write-unique-on-shared-accepted", "heap/unique.fib", "(cond (hp-shared? h id) (some HpShared)", "(cond false (some HpShared)"),
 ("write-unique-count-ignored", "heap/unique.fib", "(not= (hp-count-of h id) 1) (some (HpCount (hp-count-of h id)))", "false (some (HpCount (hp-count-of h id)))"),
 ("write-unique-weak-ignored", "heap/unique.fib", "(hp-flag? h id fl-hasweak) (some HpHasWeak)", "false (some HpHasWeak)"),
 ("write-unique-stores-itself", "heap/unique.fib", "(= (ival-obj v) (some id)) (Err (HeNotUnique id HpStoresItself))", "false (Err (HeNotUnique id HpStoresItself))"),
 ("write-unique-place-not-checked", "heap/unique.fib", "(cond (not= (kind-code (hp-kind-of h place)) 1) (Err (HeNotUnique place HpPlaceNotCell))", "(cond false (Err (HeNotUnique place HpPlaceNotCell))"),
 ("store-does-not-retain", "heap/state.fib", "((some t) (do (hp-bump h t) ()))", "((some t) ())"),
 ("immortal-counted", "heap/state.fib", ":else (Ok (not (hp-immortal? h id)))))", ":else (Ok true)))"),
 ("write-does-not-release-old", "heap/access.fib", "((some t) (hp-plan-release h t (some (HpPending id field v))))", "((some t) (Ok (vec-empty)))"),
 ("weak-does-not-mark-has-weak", "heap/access.fib", "(do (if (hp-immortal? h id) () (hp-set-flag h id fl-hasweak))", "(do (if (hp-immortal? h id) () ())"),
 ("upgrade-does-not-retain", "heap/access.fib", ":else (do (if (hp-counted? h id) (do (hp-bump h id) ()) ())", ":else (do (if (hp-counted? h id) () ())"),
 ("upgrade-of-stack-accepted", "heap/access.fib", "(hp-stack? h id) (Err (HeWeakToStack id))", "false (Err (HeWeakToStack id))"),
 ("write-to-immutable-accepted", "heap/access.fib", "(cond (not (kind-mutable? (hp-kind-of h id))) (Err (HeWriteToImmutable id))", "(cond false (Err (HeWriteToImmutable id))"),
 ("read-of-taken-slot-accepted", "heap/access.fib", "((ITaken) (Err (HeTakenSlot id field)))", "((ITaken) (Ok v))"),
 ("stack-ref-in-heap-accepted", "heap/store.fib", "(and (= holder holder-heap) (hp-stack? h id)) (Err (HeStackRefInHeap id))", "false (Err (HeStackRefInHeap id))"),
 ("stack-ref-into-outer-scope-accepted", "heap/store.fib", "(and (>= holder 0) (hp-stack? h id) (> (hp-scope-of h id) holder))", "false"),
 ("weak-to-stack-accepted", "heap/store.fib", "(hp-stack? h id) (Err (HeWeakToStack id))", "false (Err (HeWeakToStack id))"),
 ("wrong-slot-count-accepted", "heap/store.fib", "(and (kind-mutable? kind) (not= (count fields) 1))", "false"),
 ("scope-ends-in-allocation-order", "heap/life.fib", "(let [dropped (hp-reversed (nth @(. h scope-objs) scope))]", "(let [dropped (nth @(. h scope-objs) scope)]"),
 ("mutable-immortal-accepted", "heap/life.fib", "(cond (kind-mutable? kind) (Err (HeMutableImmortal kind))", "(cond false (Err (HeMutableImmortal kind))"),
 ("sharing-passes-through-immortal", "heap/shared.fib", "(cond (or (hp-immortal? h id) (contains? seen id)) (recur seen order rest)", "(cond (contains? seen id) (recur seen order rest)"),
 ("sharing-ignores-dangling-ref", "heap/shared.fib", "((some t) (hp-live-object h t HpShare))", "((some t) (Ok ()))"),
 ("sharing-a-stack-object-accepted", "heap/shared.fib", "(hp-stack? h id) (Err (HeSharedStack id))", "false (Err (HeSharedStack id))"),
 ("sharing-a-cell-accepted", "heap/shared.fib", "(= (kind-code (hp-kind-of h (nth ids i))) 1) (some (nth ids i))", "false (some (nth ids i))"),
 ("live-weak-not-crossing", "heap/shared.fib", "((some id) (if (hp-live? h id) (some id) nil))", "((some id) nil)"),
 ("immortalise-a-cell-accepted", "heap/immortal.fib", "(kind-mutable? (hp-kind-of h id)) (Err (HeMutableImmortal (hp-kind-of h id)))", "false (Err (HeMutableImmortal (hp-kind-of h id)))"),
 ("trace-ordinal-off", "trace.fib", "(let [o (+ (. n next) 1)]", "(let [o (+ (. n next) 2)]"),
 ("trace-ignores-immortalised-start", "trace.fib", "((EvImmortalised _) (+ i 1))", "((EvImmortalised _) 0)"),
 ("trace-ignores-init-done", "trace.fib", "(EvInitDone (+ i 1))", "(EvInitDone 0)"),
 ("steal-ignores-uniqueness", "heap/inplace.fib", "(_ (match (hp-not-unique h id)", "(_ (match (if true nil (some HpShared))"),
 ("move-in-retains-and-keeps-the-slot", "heap/inplace.fib", "(hp-set-field h place 0 ITaken)", "(hp-retain-stored h v)"),
 ("put-into-a-full-slot-accepted", "heap/inplace.fib", "(_ (Err (HeSlotNotTaken id field)))", "(_ (Ok ()))"),
 ("array-push-in-place-without-roomy", "heap/inplace.fib", "(and (hp-array-unique? h arr) (hp-flag? h arr fl-roomy) (< n roomy-capacity))", "(and (hp-array-unique? h arr) (< n roomy-capacity))"),
 ("array-take-does-not-null-the-slot", "heap/inplace.fib", "((some _) (hp-set-field h a i ITaken))", "((some _) ())"),
 ("array-pop-copy-does-not-retain", "heap/inplace.fib", "(do (hp-retain-stored h v)", "(do ()"),
 ("array-copy-does-not-release-the-old", "heap/inplace.fib", "(hp-apply-release h order)", "()"),
]

PROGRAMS = ["core", "modes", "inplace", "adv", "fuzz"]

def run_mutant(args):
    root, fibc, scratch, k, (name, rel, old, new) = args
    ov = os.path.join(scratch, "m%d" % k)
    shutil.rmtree(ov, ignore_errors=True)
    shutil.copytree(os.path.join(root, "compiler", "fibref"), os.path.join(ov, "fibref"))
    path = os.path.join(ov, "fibref", rel)
    text = open(path).read()
    if text.count(old) != 1:
        return name, "INVALID", "pattern found %d times in %s" % (text.count(old), rel)
    open(path, "w").write(text.replace(old, new))
    env = dict(os.environ, FIB_LIB=os.environ.get("FIB_LIB", os.path.join(root, "lib")))
    llvm = os.environ.get("LLVM_LIBDIR", "/usr/lib/llvm-21/lib")
    for p in PROGRAMS:
        exe = os.path.join(ov, p)
        b = subprocess.run([fibc, "build", "compiler/tests/fibref/heap-%s.fib" % p, "-I", ov, "-I", "compiler", "-I", "lib", "-L", llvm, "-l", "LLVM-21", "-o", exe],
                           cwd=root, env=env, capture_output=True, text=True)
        if b.returncode != 0:
            return name, "INVALID", "does not build: " + b.stderr.strip().splitlines()[0] if b.stderr.strip() else "does not build"
        r = subprocess.run([exe], cwd=root, env=env, capture_output=True, text=True)
        os.remove(exe)
        if r.returncode != 0:
            fails = [l for l in r.stdout.splitlines() if l.startswith("FAIL")]
            what = fails[0][:110] if fails else "exit %d: %s" % (r.returncode, (r.stderr.strip().splitlines() or [""])[0][:90])
            shutil.rmtree(ov, ignore_errors=True)
            return name, "KILLED", "heap-%s, %d failing test(s); first: %s" % (p, len(fails), what)
    shutil.rmtree(ov, ignore_errors=True)
    return name, "SURVIVED", "every unit program passed"

def main(argv):
    root, fibc, scratch = argv[1], argv[2], argv[3]
    rest = argv[4:]
    jobs = 2
    if rest[:1] == ["-j"]:
        jobs, rest = int(rest[1]), rest[2:]
    os.makedirs(scratch, exist_ok=True)
    chosen = [(i, m) for i, m in enumerate(M) if not rest or any(m[0].startswith(p) for p in rest)]
    with ThreadPoolExecutor(max_workers=jobs) as ex:
        results = list(ex.map(run_mutant, [(root, fibc, scratch, i, m) for i, m in chosen]))
    for name, verdict, why in results:
        print("%-9s %-40s %s" % (verdict, name, why), flush=True)
    killed = sum(1 for r in results if r[1] == "KILLED")
    print("mutants: %d killed of %d (%d survived, %d invalid)" % (killed, len(results), sum(1 for r in results if r[1] == "SURVIVED"), sum(1 for r in results if r[1] == "INVALID")))
    return 0 if killed == len(results) else 1

sys.exit(main(sys.argv))
