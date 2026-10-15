#!/bin/bash
# scripts/mutant-live.sh: planted faults in live mode (docs/design/live-repl.md section 6: the mutants of each package). Copies compiler/ and lib/ to a
# scratch directory, breaks ONE rule in the copy, builds a fibc from the copy and runs the test that pins the rule against it: the test must FAIL. A test
# that still passes under its mutant is a bad test. The pattern of scripts/mutant-serve.sh.
# usage: scripts/mutant-live.sh MODE
#   MODE                 what is broken                                                              test that must fail
#   stub-no-tailcall     the stub calls the body and returns instead of tail-jumping: every call of  compiler/tests/live/stubs.sh (deep.fib's
#                        a program function adds a frame, and a recursion a hundred million deep     recursion traps `stack overflow`; hello's
#                        traps `stack overflow` (LV0). A million deep (ownership cases 53, 164) is   golden names the tail jump)
#                        NOT enough: main's stack (rt/thread.lir) holds a million stub frames
#   stub-plain-load      the stub reads the slot with a plain load, not an acquire load: a caller     compiler/tests/live/stubs.sh (hello's
#                        may jump into code the swap has not finished writing (LV0)                 golden names the acquire load) and
#                                                                                                   stress.sh --tsan-only (the plain load against
#                                                                                                   the release store is a NEW TSAN report)
#   deps-stubbed-by-default  in a project the name rule is kept: the libraries' bodies are stubbed   compiler/tests/live/project.sh (the
#                        too (LT3, docs/design/live-meshql.md 4)                                      golden's `default:` line gains acme.*)
#   project-paths-not-stubbed  in a project no module of the project's own is live (only the main    compiler/tests/live/project.sh (the
#                        module): the project's own bodies are direct calls (LT3)                     golden's lines lose app.core)
#   sink-ignored         fib.test.run's `report` prints the document instead of writing `out`, the   cases/stdlib 8423 (the buffer is empty:
#                        sink `run-main-into` was given (LT1, docs/design/live-meshql.md 3.2)         result 2); compiler/tests/live/hooks.sh
#   override-ignored     `run-main-into` parses `(args)` and not the words it was given (LT1)         cases/stdlib 8423 (`--only x` not applied:
#                                                                                                     three plain rows in the buffer: result 2)
#   value-reevaluated    a session `def` is not made resident: its slot is left in the input's      compiler/tests/live/attach.sh (the atom
#                        module, so every later input gets a slot of its own, never initialised,    counter of the design's transcript does not
#                        and the value of a `def` does not persist from one input to the next (LV1) answer 1 then 2: the second read is not of
#                                                                                                   the same atom)
#   trap-not-caught      an expression's entry runs without `live-guard` (the `try` of fib.ex):     compiler/tests/live/attach.sh (`(nth [1 2] 5)`
#                        a trap in an input aborts the program instead of answering `failed` (LV1)  ends the program; the next input has nobody
#                                                                                                   to answer it)
#   patch-before-jit     a swap stores the slots (with 0: the address is not known yet) BEFORE the   compiler/tests/live/swap.sh (the worker thread
#                        new bodies are looked up, so a call during ORC's compilation of the new     calling `work` during one of the 200 swaps
#                        generation jumps to address 0 (LV2; docs/design/live-repl.md 3.1 step 4)   jumps to 0: the app dies, the next check
#                                                                                                   has nobody to answer it)
#   old-tracker-removed  a swap removes the previous generation's tracker right after the patch:    compiler/tests/live/swap.sh (the worker is
#                        a thread in the old body runs freed code (LV2; 3.3: generations are kept)   inside the old `work` when its code is freed:
#                                                                                                   the app dies)
#   summary-ignored      the summary rule is not applied: a body whose parameter was borrowed and   compiler/tests/live/swap.sh (transcript 5's
#                        is now owned (stored) is accepted; its callers hand nothing over (LV2, 3.4) second refusal does not come; `handle` then
#                                                                                                   stores a request its caller releases)
#   signature-ignored    the signature rule is not applied: a function redefined with another       compiler/tests/live/swap.sh (transcript 5's
#                        scheme (an added parameter) is accepted; its callers' calls are typed lIR  first refusal does not come; the server's
#                        of the old one (LV2, 3.4)                                                  calls of `handle` read a register for `db`)
#   defcache-stale       the def cache is keyed by the body name alone, not by the lowered          compiler/tests/live/defcache.sh (the required
#                        initialiser too: a definition that changed between two programs is        module edited on disk still reads 5 and
#                        replayed from the old one (LE1, docs/design/live-incremental.md 2.2)       ["a" "b"]; under verify the replay is refused)
#   defcache-no-items    a replayed value's constant items are not appended to the program: the     compiler/tests/live/defcache.sh (every input
#                        value names a `@def.N` the module lacks (LE1)                               reading a def is refused: no such global)
#   defcache-verify-blind  FIB_LIVE_EMIT=verify compares nothing (planted with defcache-stale, so   compiler/tests/live/defcache.sh (the verify
#                        that there is a wrong replay for it to miss)                                app reads the edited module's old values
#                                                                                                    instead of refusing the input)
#   retire-without-pause `retire` does not ask whether the program is quiescent: the old generations compiler/tests/live/stress.sh --socket-only
#                        of `f` are freed under 8 threads and the pool calling it (LV3, 3.3)        (the refusal does not come; the program dies
#                                                                                                   in freed code: nobody answers the next check)
#   resident-by-family   the queue mode checks the family name, not the body's code name, for        compiler/tests/live/queue.sh (the swapped
#                        residency: the swapped generation is itself resident, so it is never        generation is answered by name, never lowered:
#                        lowered (LE3, docs/design/live-incremental.md 2.3)                           the handler swap has no effect)
#   skip-new-specialisation  a specialisation the running program lacks is answered by name instead   compiler/tests/live/queue.sh (`probe`'s new
#                        of lowered: ORC's lookup of the missing code fails (LE3)                      `box` specialisation is missing: /probe fails)
#   literal-counter-per-program  a generation's bodies and literals do not carry the generation, so    compiler/tests/live/queue.sh (the handler's
#                        a later swap's names collide with the resident ones (LE3)                     generation collides: the swap is refused/broken)
#   specialisation-not-requested  a swap requests only the specialisations `main` reaches, not the     compiler/tests/live/queue.sh (`dup`'s f64
#                        family's registered ones: a generic reached through a resident caller is      instantiation is not swapped: /calc is wrong)
#                        missed (LE3)
#   patch-plain-store    the runtime's `fib.live-patch` (rt/live.lir, the store of a swap) is a     compiler/tests/live/stress.sh --tsan-only (the
#                        plain store, not a release store: a caller may jump into code the swap    stub's acquire load against a plain store is
#                        has not finished writing (LV3)                                             a NEW ThreadSanitizer report)
#   stale-never          a test run's answer never says `stale`: the generations at its start and   compiler/tests/live/test.sh (the two-second
#                        end are not compared (LT2, docs/design/live-meshql.md 3.7)                  scenario with a swap at one second is not stale)
#   rerun-old-registry   the suites (the registry: the scenario closures) are evaluated once, into compiler/tests/live/test.sh (the step edited at
#                        a resident def at the first run, and every later run reads that value,    the REPL is not seen by the rerun: the old
#                        instead of the entry being evaluated by every run (LT2, 3.6)               closure runs and the row still passes)
#   test-on-attach-thread  the attach thread waits for the test thread: a run holds the socket      compiler/tests/live/test.sh (`status` during the
#                        (LT2, 3.2 step 1: it answers nothing yet and goes back to its socket)     two-second scenario is not answered in 100 ms)
#   sink-shared-run      two runs at once: the `busy` refusal is gone and both write the session's compiler/tests/live/test.sh (the second `test`
#                        one report file (LT2, 3.2: one test at a time)                             during the run is started, not refused `busy`)
#   child-spawned-anyway `fibc test --connect` runs the specs as `fibc test` does, a `fibc run` per   compiler/tests/live/testlive.sh (the second
#                        file, instead of sending `test` to the session (LT6): the reports are the  run of one scenario is a spawned run's time
#                        same, the time is not                                                       again; the session's status has no last run)
#   quote-shared         a quoted form `@q.N` is not a static numbered per program (emit.statics  compiler/tests/live/quotes.sh (a spec function
#                        `program-static?`): the split exports a swap's quotes with the data, the    swapped at the REPL, then a base scenario's `run`:
#                        next input's cut declares its own `q.N` from the base and reads another   the app dies at 0xe; `specs` swapped to one
#                        program's form (docs/design/live-meshql.md 10)                            binding answers `no match clause matched`)
#   partial-load         a load whose text rules refuse a definition (a changed def) goes on to     compiler/tests/live/load.sh (the good edit of
#                        swap the good definitions of the file: the half-applied state is read      `greet` in the file with the changed def is
#                        (LM1, docs/design/live-incremental.md 3.5: all or nothing)                 swapped: "hi a" where "hello a" must stay)
#   load-reads-disk      the loader ignores the loaded text and reads the module's file: an        compiler/tests/live/load.sh (the expression
#                        expression input after a disk edit picks up a half-saved file (LM1, 3.4)   input after the half-saved file is refused)
#   prefix-dropped       the family of a loaded module's function is named without the module      compiler/tests/live/load.sh (`routes/route` is
#                        prefix (`f.route` for `f.routes.route`; LM2: the key of its hash,          not in `swapped`; the next call answers the
#                        serve.livediff): nothing is swapped, the call answers the old body (LM1,   old value)
#                        3.2; live-dev.md 6 item 2 of meshql-fib)
#   removed-ignored      a function removed from the loaded file is accepted: its stub stays and    compiler/tests/live/load.sh (the load that
#                        names a body the text no longer has (LM1, 3.5: live/Removed)               removes `helper` is not refused; the call of
#                                                                                                   `helper` after it is refused by the checker)
#   def-slot-plain-store a def's setter `live.d.NAME.set` (emit.program `live-def-setter`, what a       compiler/tests/live/defs.sh --tsan-only (the
#                        redefinition stores with) is a plain store, not a release store (LT4)      readers' acquire load against a plain store is
#                                                                                                   a NEW ThreadSanitizer report)
#   old-def-freed        a redefinition drops the old value at once, not at a retirement: a thread  compiler/tests/live/defs.sh --socket-only (a
#                        reading it reads freed memory (LT4, docs/design/live-repl.md 9)            bad sum or a crash under the stress)
#   init-run-twice       the init function of a new def runs twice (LT4: an initialiser runs once)  compiler/tests/live/defs.sh --socket-only (the
#                                                                                                   new def's side effect counts 2)
#   warm-skipped         the engine is not warmed before the first connection: the first input      compiler/tests/live/attach.sh (hello's `warm`
#                        pays the cold front end (LE5, docs/design/live-incremental.md 3.7)         is false; the first input's `reused` is 0)
#   inspect-runs-input   `inspect` without `value` evaluates the name as an input instead of        compiler/tests/live/attach.sh (the answer
#                        looking it up (LE4, 3.8)                                                   carries `stats` and no `scheme`)
#   diff-by-text         the families swapped are those whose text the input redefines, not those   compiler/tests/live/swap.sh (the `require` that
#                        whose expanded form changed (LM2, docs/design/live-incremental.md 2.4)     moves `alt` swaps nothing: /alt answers one)
#   hash-with-positions  a definition's hash covers its position: every definition after a changed  compiler/tests/live/swap.sh (the `require`
#                        one (or after a longer `ns` header) is swapped too (LM2)                   input's `swapped` lists them all)
#   watch-no-load        the watch reruns the last test on a change without loading the changed     compiler/tests/live/watch.sh (no load event;
#                        module file: the rerun runs the old code (LT5, docs/design/live-meshql.md   the rerun's row still fails)
#                        3.8)
#   watch-reruns-on-refusal  the watch reruns the last test after a load that was refused (LT5)       compiler/tests/live/watch.sh (a test event
#                                                                                                   follows the refusal)
#   rules-at-file-start  a load's live/Signature or live/Summary refusal is reported at FILE:1:1,   compiler/tests/live/load.sh (the signature
#                        not at the refused function's definition in the file                       refusal of helper is not at 11:1)
#   type-reads-disk      `:type` reads a loaded module from its file, not the session's loaded text  compiler/tests/live/load.sh (`:type` of a
#                                                                                                   call into the half-saved file is refused)
#   unheld-held          `fibc test --live` loads every spec file, one that does not compile and a   compiler/tests/live/testlive.sh (the session
#                        second of one base name too: the session is refused whole (exit 3)        never prints its socket)
#   watch-no-debounce    the watch acts on a file whose stamp changed since the last scan, stable or   compiler/tests/live/watch.sh (the slow
#                        not: a file caught half-written is loaded and refused (LT5)                  write brings a refused load event)
# environment: FIBC (a fibc that builds the mutant: the seed or a stage 2; required), MUT_OUT (scratch; default ~/.cache/fibber-scratch/mutant-live-MODE),
#              MUT_QUICK=1 runs stubs.sh --quick (hello's golden and the deep recursion; no case tables: kills both mutants in under a minute after the build)
#              and stress.sh --quick (100 swaps), defs.sh --quick (100 redefinitions).
# A mutant of lib/ alone (sink-ignored, override-ignored) builds nothing: the case's `roots:` header names the copy's lib, and FIBC runs it.
# exit: 0 when the test failed under the mutant, 1 when it passed (the mutant survived), 2 for a setup error. That the tests pass UNmutated is shown by running them.
set -uo pipefail
MODE=${1:?usage: mutant-live.sh stub-no-tailcall|stub-plain-load|deps-stubbed-by-default|project-paths-not-stubbed|sink-ignored|override-ignored|value-reevaluated|trap-not-caught|patch-before-jit|old-tracker-removed|summary-ignored|signature-ignored|retire-without-pause|patch-plain-store|defcache-stale|defcache-no-items|defcache-verify-blind|resident-by-family|skip-new-specialisation|literal-counter-per-program|specialisation-not-requested|stale-never|rerun-old-registry|test-on-attach-thread|sink-shared-run|quote-shared|partial-load|load-reads-disk|prefix-dropped|removed-ignored|def-slot-plain-store|old-def-freed|init-run-twice|warm-skipped|inspect-runs-input|child-spawned-anyway|diff-by-text|hash-with-positions|watch-no-load|watch-reruns-on-refusal|rules-at-file-start|type-reads-disk|unheld-held|watch-no-debounce}
R=$(cd "$(dirname "$0")/.." && pwd)
OUT=${MUT_OUT:-$HOME/.cache/fibber-scratch/mutant-live-$MODE}
FIBC=${FIBC:-}
[ -x "$FIBC" ] || { echo "mutant-live: no fibc to build with: set FIBC to an executable" >&2; exit 2; }
unset LD_LIBRARY_PATH
rm -rf "$OUT"; mkdir -p "$OUT"
cp -r "$R/compiler" "$R/lib" "$OUT/"
patch() { # patch FILE OLD NEW: the text must be there exactly once
  python3 - "$OUT/$1" "$2" "$3" <<'EOF' || { echo "mutant-live: the text to change is not in $1 (the source moved: update the mutant)" >&2; exit 2; }
import sys
p, old, new = sys.argv[1:4]
s = open(p).read()
if s.count(old) != 1: sys.exit(1)
open(p, 'w').write(s.replace(old, new))
EOF
}
case $MODE in
  stub-no-tailcall) patch compiler/emit/live.fib '  (if (tail-jump? ty)
      [(n/indirect-call (n/local "at") ty args true)]' '  (if false
      [(n/indirect-call (n/local "at") ty args true)]' ;;
  stub-plain-load)  patch compiler/emit/live.fib '(n/node (la/KAtomicLoad la/ScSystem la/OAcquire lt/ty-ptr (n/global slot)))' '(n/load lt/ty-ptr (n/global slot) nil)' ;;
  deps-stubbed-by-default) patch compiler/emit/live.fib '((some names) (or (= m @(. g main)) (contains? names (g-module-name g m))))' '((some names) (mods/sees-implicit (g-module-name g m) (mods/implicit-lib)))' ;;
  project-paths-not-stubbed) patch compiler/emit/live.fib '((some names) (or (= m @(. g main)) (contains? names (g-module-name g m))))' '((some names) (= m @(. g main)))' ;;
  sink-ignored) patch lib/fib/test/run.fib '(do (out (cond (= (. o format) "json")' '(do (print (cond (= (. o format) "json")' ;;
  override-ignored) patch lib/fib/test/run.fib '  (match (parse-args argv)' '  (match (parse-args (vec (args)))' ;;
  value-reevaluated) patch compiler/serve/livedefs.fib '    (if (empty? slots)
        (Ok ())' '    (if true
        (Ok ())' ;;
  trap-not-caught)  patch compiler/serve/liveexpr.fib '-> i64 (live-guard live-fd (fn () -> i64 (do "' '-> i64 ((fn () -> i64 (do "' ;;
  patch-before-jit) patch compiler/serve/liveswap.fib '                (addrs (ti/try-map (fn (nm: str) (ls/address-of sx nm)) coded))' '                (_ (publish sx names (mapv (fn (nm: str) 0) names)))
                (addrs (ti/try-map (fn (nm: str) (ls/address-of sx nm)) coded))' ;;
  old-tracker-removed) patch compiler/serve/liveswap.fib '                (_ (publish sx names addrs)))' '                (_ (publish sx names addrs))
                (_ (let ((gens (. (unwrap-or (get (. @(. sx table) families) family) (lt/Family 0 nil nil [])) gens)))
                     (if (empty? gens) (Ok ()) (lj/remove (ls/jit-of sx) (. (nth gens (- (count gens) 1)) rt) (. (nth gens (- (count gens) 1)) names))))))'
                       patch compiler/serve/liveswap.fib '[own.driver :as od] [types.init :as ti]' '[own.driver :as od] [types.init :as ti] [live.jit :as lj]' ;;
  summary-ignored)  patch compiler/serve/liverules.fib '       (nil (summary-rule c f fid family))))' '       (nil nil)))' ;;
  signature-ignored) patch compiler/serve/liverules.fib '     (match (signature-rule c f fid family session-defs)' '     (match (if true nil (signature-rule c f fid family session-defs))' ;;
  defcache-stale) patch compiler/emit/defs/cache.fib '    ((some e) (if (= (. e source) source) (some e) nil))' '    ((some e) (some e))' ;;
  defcache-no-items) patch compiler/emit/defs/cache.fib '                (do (defs-append defs it)' '                (do ()' ;;
  defcache-verify-blind) patch compiler/emit/defs/cache.fib '    ((some e) (if (= (. e source) source) (some e) nil))' '    ((some e) (some e))'
                         patch compiler/emit/defs/jit.fib '        (if (= kept got)' '        (if true' ;;
  resident-by-family) patch compiler/emit/program.fib '(contains? @(. p resident) (body-code-name p key (body-name (program-g p) key tys)))' '(contains? @(. p resident) (family-name (program-g p) key))' ;;
  skip-new-specialisation) patch compiler/emit/program.fib '(contains? @(. p resident) (body-code-name p key (body-name (program-g p) key tys)))' '(or (not (empty? tys)) (contains? @(. p resident) (body-code-name p key (body-name (program-g p) key tys))))' ;;
  literal-counter-per-program) patch compiler/emit/program.fib '(generation-name name (family-generation p (family-name (program-g p) key)))' 'name' ;;
  specialisation-not-requested) patch compiler/serve/liveswap.fib '(ec/BodyRequest f (. b owned) [] nil (. b tys-text) (. b module))' '(ec/BodyRequest f (. b owned) [] nil [] (. b module))' ;;
  retire-without-pause) patch compiler/serve/live.fib '    (if (not (and (. t pausing) (. t quiescent)))' '    (if false' ;;
  patch-plain-store) patch compiler/emit/runtime.fib '  (block entry (atomic-store release (inttoptr ptr v) slot) (ret)))' '  (block entry (store (inttoptr ptr v) slot) (ret)))' ;;
  stale-never) patch compiler/serve/livetest.fib '(Pair "stale" (j/JBool (not (= (. run gen0) gen1))))' '(Pair "stale" (j/JBool false))' ;;
  rerun-old-registry)
    patch compiler/serve/livetest.fib '    (do (set! (. (. sx rc) clauses) (rc/merge-clauses @(. (. sx rc) clauses) (lw/test-clauses (. (. sx rts) with))))' '    (do (set! (. (. sx rc) clauses) (rc/merge-clauses @(. (. sx rc) clauses) (lw/test-clauses (. (. sx rts) with))))
        (when (not (. sel runner)) (match (lsw/input sx (str "(def live-test-cache " (. sel entry) ")")) (_ ())))'
    patch compiler/serve/livewords.fib '             (str "(live-test-run/run-main-into " (. sel entry) " " words " live-out)"))' '             (str "(live-test-run/run-main-into live-test-cache " words " live-out)"))' ;;
  test-on-attach-thread) patch compiler/serve/livetest.fib '                 (do (unix/fd-close fd) (unix/fd-close efd) (reset! done (some r)) r))))
      ()))' '                 (do (unix/fd-close fd) (unix/fd-close efd) (reset! done (some r)) r))))
      (loop () (if (nil? @done) (do (unix/sleep-ns 1000000) (recur)) ()))))' ;;
  sink-shared-run) patch compiler/serve/livetest.fib '  (match @(. t running)
    ((some run) (some (busy r run)))' '  (match (if true nil @(. t running))
    ((some run) (some (busy r run)))' ;;
  child-spawned-anyway) patch compiler/driver/testlive.fib '(not (= (. w connect) "")) (connect-test (. w connect) (. w rest))' '(not (= (. w connect) "")) (dt/cmd-test (. w rest) dirs)' ;;
  quote-shared) patch compiler/emit/statics.fib '  (or (s/starts-with? name "clo.") (s/starts-with? name "vt.") (s/starts-with? name "q.")))' '  (or (s/starts-with? name "clo.") (s/starts-with? name "vt.")))' ;;
  partial-load) patch compiler/serve/liveload.fib '      (if (not (empty? refusals))
          (result file ns defs [] (refused-all file refusals))' '      (if false
          (result file ns defs [] (refused-all file refusals))' ;;
  load-reads-disk) patch compiler/expand/modules.fib '(visit-source ld ns file (unwrap-or (get (. ld texts) ns) source))' '(visit-source ld ns file source)' ;;
  prefix-dropped) patch compiler/serve/livediff.fib '(let ((prefix (if main "" (str (. (. m spec) ns) ".")))' '(let ((prefix "")' ;;
  def-slot-plain-store) patch compiler/emit/program.fib '(node/node (la/KAtomicStore la/ScSystem la/ORelease (node/local "v") (node/global slot)))' '(node/store (node/local "v") (node/global slot) nil)' ;;
  old-def-freed) patch compiler/serve/livedefs.fib '                (note-olds sx todo olds)' '                (try-let ((at (drop-address sx))) (ti/try-map (fn (v: i64) (if (= v 0) (Ok 0) (lj/call at [v]))) olds))
                (note-olds sx todo (mapv (fn (v: i64) 0) olds))' ;;
  init-run-twice) patch compiler/serve/livedefs.fib '                  (_ (ls/run-void (ls/jit-of sx) init-name)))' '                  (_ (ls/run-void (ls/jit-of sx) init-name)) (_ (ls/run-void (ls/jit-of sx) init-name)))' ;;
  removed-ignored) patch compiler/serve/liveload.fib '    (cond (and (= what "removed") (or (= k "fun") (= k "impl")))' '    (cond (and (= what "never") (or (= k "fun") (= k "impl")))' ;;
  warm-skipped) patch compiler/serve/live.fib '                (do (warm-engine l trace)' '                (do ()' ;;
  diff-by-text) patch compiler/serve/liveswap.fib '(changes-of sx olds fr)' '(ldf/Changes (vec (mapcat (fn (d: rc/Def) (if (s/starts-with? (. d key) "val:") [(str "f." (. d name))] (ldf/families-of-key (ls/live-bodies sx) (. d key)))) (filterv (fn (d: rc/Def) (replaced? sx d)) defs)))
                                (mapv (fn (d: rc/Def) (. d key)) (filterv (fn (d: rc/Def) (and (replaced? sx d) (s/starts-with? (. d key) "impl:"))) defs)) (. (changes-of sx olds fr) hashes))' ;;
  hash-with-positions) patch compiler/serve/livediff.fib '(str (pr/stx-show (canon f (gensyms-in f {}) aliases)) "\n" uses))' '(str (pr/stx-show (canon f (gensyms-in f {}) aliases)) "\n" uses " @" (. (. f pos) start)))' ;;
  watch-no-load) patch compiler/serve/live.fib '(ll/load-file (. l loads) (. l sx) f))) (. ch modules))' '(ll/load-file (. l loads) (. l sx) f))) (filterv (fn (f: str) false) (. ch modules)))' ;;
  watch-reruns-on-refusal) patch compiler/serve/live.fib '(if (and (every? (fn (f: (Pair bool (Vec (Pair str j/JVal)))) (. f fst)) loads) (some? @(. (. l tests) last-sel)))' '(if (some? @(. (. l tests) last-sel))' ;;
  rules-at-file-start) patch compiler/serve/liveload.fib '    ((some d) (pos-of text d))
    (nil (Pair 1 1))))' '    ((some d) (Pair 1 1))
    (nil (Pair 1 1))))' ;;
  type-reads-disk) patch compiler/serve/livesess.fib '(rc/type-of-expr-texts (. sx rc) (. sx eng) (. sx rts) expr @(. sx texts))' '(rc/type-of-expr (. sx rc) (. sx eng) (. sx rts) expr)' ;;
  unheld-held) patch compiler/driver/testlive.fib '(match (unheld-of e rts (. acc fst) f)' '(match (if true nil (unheld-of e rts (. acc fst) f))' ;;
  watch-no-debounce) patch compiler/serve/livewatch.fib '  (match (get prev f) ((some p) (same? p st)) (nil false)))' '  true)' ;;
  inspect-runs-input) patch compiler/serve/live.fib '(if (p/field-bool r "value") (inspect-value l r name) (inspect-lookup l r name))' '(inspect-value l r name)' ;;
  *) echo "mutant-live: unknown mode $MODE" >&2; exit 2 ;;
esac
case $MODE in
  sink-ignored|override-ignored) F=$FIBC ;;  # a mutant of lib/ alone: nothing to build
  *)
    cd "$OUT" || exit 2
    echo "mutant-live: building the $MODE mutant ..."
    ( ulimit -v 16000000; "$FIBC" build compiler/fibc.fib -I compiler -I lib -L /usr/lib/llvm-21/lib -l LLVM-21 -o "$OUT/F" ) > "$OUT/build.log" 2>&1 \
      || { echo "mutant-live: the mutant does not build (see $OUT/build.log)"; tail -5 "$OUT/build.log"; exit 2; }
    F=$OUT/F ;;
esac
cd "$R" || exit 2
quick=(); [ "${MUT_QUICK:-}" = 1 ] && quick=(--quick)
case $MODE in
  stub-no-tailcall) what="the stubs test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/stubs.sh" "$F" "${quick[@]}" > "$OUT/test.log" 2>&1; rc=$? ;;
  stub-plain-load) what="the stubs test and the TSAN stress"
     FIB_LIB="$R/lib" "$R/compiler/tests/live/stubs.sh" "$F" "${quick[@]}" > "$OUT/test.log" 2>&1; rc=$?
     TSAN_OUT="$OUT/tsan" FIB_LIB="$R/lib" "$R/compiler/tests/live/stress.sh" "$F" --tsan-only "${quick[@]}" >> "$OUT/test.log" 2>&1 || rc=1 ;;
  retire-without-pause) what="the socket stress"; FIB_LIB="$R/lib" "$R/compiler/tests/live/stress.sh" "$F" --socket-only "${quick[@]}" > "$OUT/test.log" 2>&1; rc=$? ;;
  patch-plain-store) what="the TSAN stress"; TSAN_OUT="$OUT/tsan" FIB_LIB="$R/lib" "$R/compiler/tests/live/stress.sh" "$F" --tsan-only "${quick[@]}" > "$OUT/test.log" 2>&1; rc=$? ;;
  value-reevaluated|trap-not-caught|warm-skipped|inspect-runs-input) what="the attach test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/attach.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  patch-before-jit|old-tracker-removed|summary-ignored|signature-ignored|diff-by-text|hash-with-positions) what="the swap test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/swap.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  defcache-*) what="the defcache test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/defcache.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  resident-by-family|skip-new-specialisation|literal-counter-per-program|specialisation-not-requested) what="the queue test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/queue.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  stale-never|rerun-old-registry|test-on-attach-thread|sink-shared-run) what="the live test op's test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/test.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  child-spawned-anyway|unheld-held) what="the test --live test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/testlive.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  watch-no-load|watch-reruns-on-refusal|watch-no-debounce) what="the watch test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/watch.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  quote-shared) what="the quotes test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/quotes.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  def-slot-plain-store) what="the TSAN defs test"; TSAN_OUT="$OUT/tsan" FIB_LIB="$R/lib" "$R/compiler/tests/live/defs.sh" "$F" --tsan-only > "$OUT/test.log" 2>&1; rc=$? ;;
  old-def-freed|init-run-twice) what="the defs test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/defs.sh" "$F" --socket-only "${quick[@]}" > "$OUT/test.log" 2>&1; rc=$? ;;
  partial-load|load-reads-disk|prefix-dropped|removed-ignored|rules-at-file-start|type-reads-disk) what="the load test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/load.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  deps-stubbed-by-default|project-paths-not-stubbed) what="the project test"; FIB_LIB="$R/lib" "$R/compiler/tests/live/project.sh" "$F" > "$OUT/test.log" 2>&1; rc=$? ;;
  *) what="case 8423"  # the case's `roots:` header names ../../lib: run from a copy of cases/stdlib beside the mutated lib
     mkdir -p "$OUT/cases/stdlib"; cp -r "$R"/cases/stdlib/8423-*.fib "$R/cases/stdlib/support" "$OUT/cases/stdlib/"
     ( cd "$OUT" && ulimit -v 16000000 && FIB_LIB="$OUT/lib" "$F" cases cases/stdlib --only 8423- ) > "$OUT/test.log" 2>&1; rc=$? ;;
esac
grep -E '^FAIL|FAIL ' "$OUT/test.log" | head -5 || true  # pipe-ok: a short failure listing
if [ $rc -ne 0 ]; then echo "mutant-live: $MODE KILLED by $what (exit $rc)"; exit 0; fi
echo "mutant-live: $MODE SURVIVED: $what passed under the mutant"; exit 1
