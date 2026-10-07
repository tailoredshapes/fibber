#!/bin/bash
# LIB-2 item 5e: `fibc test` compiles a spec file with the roots of ITS project (the nearest deps.fib above the file: its :paths and its test paths), not only
# those of the working directory. A project in a scratch directory with src/plib.fib and a spec that requires `plib`; `fibc test FILE` is run from a directory with no
# deps.fib: it must find the module (before the change: `module plib is not at .../specs/plib.fib`), and the scenario must pass.
# usage: test-project-root.sh STAGE2        (FIB_LIB must name lib/; run from anywhere)  Exit: 0 every check holds, 1 one fails.
s2=${1:?usage: test-project-root.sh STAGE2}
T=$(mktemp -d "${TMPDIR:-/tmp}/projroot.XXXXXX"); trap 'rm -rf "$T"' EXIT
bad=0
ck() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: wanted [$3] got [$2]"; bad=1; fi; }
mkdir -p "$T/proj/src" "$T/proj/specs" "$T/elsewhere"
printf '{:name "t/p" :paths ["src"]}\n' > "$T/proj/deps.fib"
printf '(ns plib)\n(defun twice (x: i64) -> i64 (* 2 x))\n' > "$T/proj/src/plib.fib"
cat > "$T/proj/specs/p-spec.fib" <<'EOF2'
(ns main (:use fib.core fib.seq fib.coll fib.test.core fib.test.run) (:require [plib :as p]))
(defspecs-main specs
  (feature "project root"
    (scenario "a module of the project's src is found from another working directory"
      (then (expect = 8 (p/twice 4))))))
EOF2
out=$(cd "$T/elsewhere" && "$s2" test "$T/proj/specs/p-spec.fib" 2>&1)
ck "the spec of another project compiles and its scenario passes" "$(echo "$out" | grep -c '^1 scenarios: 1 pass')" "1"
ck "no module is reported missing" "$(echo "$out" | grep -c 'is not at')" "0"
# a spec with no deps.fib above it still finds the modules beside it (as before)
mkdir -p "$T/plain"
printf '(ns plib2)\n(defun thrice (x: i64) -> i64 (* 3 x))\n' > "$T/plain/plib2.fib"
cat > "$T/plain/q-spec.fib" <<'EOF2'
(ns main (:use fib.core fib.seq fib.coll fib.test.core fib.test.run) (:require [plib2 :as p]))
(defspecs-main specs
  (feature "beside the file"
    (scenario "a module beside the spec is found" (then (expect = 12 (p/thrice 4))))))
EOF2
out=$(cd "$T/elsewhere" && "$s2" test "$T/plain/q-spec.fib" 2>&1)
ck "a spec with no project still finds the modules beside it" "$(echo "$out" | grep -c '^1 scenarios: 1 pass')" "1"
exit $bad
