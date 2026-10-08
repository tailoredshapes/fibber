#!/bin/bash
# The UB hunt (docs/design/lir2c.md section 4): the C of each program compiled with -fsanitize=undefined,address and run; every report of
# the sanitizers is a bug of the printer (the bar is zero), and the run must still end as the native run does (exit status, standard output).
#   compiler/tests/c-backend/ubsan.sh [-j N] [--cc CC] [--emit DIR..] [FILE.lir|DIR]..   (the inputs of diff.sh)
# Environment: FIBC, LAIRF, LIR2C, CC (default gcc), UBSAN_TIMEOUT (seconds, default 120). A row per program: `clean NAME`, `report NAME: WHAT`
# (a sanitizer line), `differ NAME: WHAT` (the sanitized run ended otherwise than the native one), `skip NAME: WHY`; then the counts. Exit 1
# on any report or difference. Leaks are not undefined behaviour (and the audit cases count them on purpose): detect_leaks=0. The runtime
# installs its own SIGSEGV handler on a signal stack (rt/thread.lir), so ASan's handlers are off.
set -u
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../../.." && pwd)
jobs=4; emits=(); inputs=(); cc=${CC:-gcc}
while [ $# -gt 0 ]; do
  case $1 in -j) jobs=$2; shift ;; --cc) cc=$2; shift ;; --emit) emits+=("$2"); shift ;; *) inputs+=("$1") ;; esac; shift
done
: "${LAIRF:?}" "${LIR2C:?}"; TMO=${UBSAN_TIMEOUT:-120}
CFLAGS="-std=gnu11 -O1 -g -march=native -ffp-contract=off -fno-math-errno -fno-delete-null-pointer-checks -w -fsanitize=undefined,address -fno-sanitize-recover=all"
work=$(mktemp -d "${TMPDIR:-/tmp}/ubsan.XXXXXX"); trap 'rm -rf "$work"' EXIT
list=$work/list; : > "$list"
for x in ${inputs[@]+"${inputs[@]}"}; do
  if [ -d "$x" ]; then grep -lrE --include='*.lir' "^;; expect: accept" "$x" | sort >> "$list"; else echo "$x" >> "$list"; fi
done
for d in ${emits[@]+"${emits[@]}"}; do : "${FIBC:?}"; ad=$(cd "$d" && pwd); grep -lE '^;; expect: +(accept|trap)' "$ad"/*.fib | sort >> "$list"; done
one() {
  f=$1; name=${f#"$root"/}; d=$work/run/$(echo "$name" | tr '/' '_'); mkdir -p "$d"
  ulimit -c 0; ulimit -v unlimited 2> /dev/null; exec 2> /dev/null   # ASan reserves 16 TB of address space (and uses little of it)
  if [ "${f%.fib}" != "$f" ]; then
    inc=(); for r in $(sed -n 's/^;; roots: *//p' "$f" | head -1); do inc+=(-I "$(dirname "$f")/$r"); done
    (cd "$root" && timeout "$TMO" "$FIBC" emit "$f" "${inc[@]}") > "$d/e.lir" 2> "$d/e.err" || { echo "skip $name: fibc emit failed"; return; }
    f=$d/e.lir
  fi
  (timeout "$TMO" "$LAIRF" run "$f" > "$d/n.out" 2> "$d/n.err") 2> /dev/null; ns=$?
  [ $ns -eq 124 ] && { echo "skip $name: the native run timed out"; return; }
  # cases that fault on purpose to reach the runtime's own SIGSEGV handler (rt/thread.lir), which the sanitizer runtime would replace
  case $name in *8354-fork-child-overflow*) echo "skip $name: wild fault by design (the runtime's SIGSEGV handler is the subject)"; return ;; esac
  "$LIR2C" "$f" -o "$d/p.c" --cc "$cc" --allow-plain-tail-calls 2> "$d/t.err" || { echo "skip $name: lir2c refused: $(grep -m1 error: "$d/t.err" | head -c 150)"; return; }
  # shellcheck disable=SC2086
  "$cc" $CFLAGS -o "$d/p" "$d/p.c" -lm -lpthread 2> "$d/cc.err" || { echo "differ $name: cc failed: $(grep -m1 error "$d/cc.err" | head -c 150)"; return; }
  (ASAN_OPTIONS=detect_leaks=0:handle_segv=0:handle_sigbus=0:handle_abort=0:use_sigaltstack=0:allocator_may_return_null=1 UBSAN_OPTIONS=print_stacktrace=1 \
     timeout "$TMO" "$d/p" > "$d/c.out" 2> "$d/c.err") 2> /dev/null; cs=$?
  if grep -qE 'runtime error:|ERROR: .*Sanitizer' "$d/c.err"; then echo "report $name: $(grep -m1 -E 'runtime error:|ERROR: .*Sanitizer' "$d/c.err" | head -c 220)"; return; fi
  what=""
  [ $ns -ne $cs ] && what="exit $cs, native $ns"
  cmp -s "$d/n.out" "$d/c.out" || what="${what:+$what; }stdout differs"
  if [ -z "$what" ]; then echo "clean $name"; else echo "differ $name: $what"; fi
}
export -f one; export LAIRF LIR2C FIBC TMO root work cc CFLAGS
xargs -P "$jobs" -I{} bash -c 'one "$@"' _ {} < "$list" > "$work/rows"
sort -k2 "$work/rows" > "$work/sorted"; grep -v '^clean ' "$work/sorted"
for v in clean report differ skip; do printf '%s %s  ' "$v" "$(grep -c "^$v " "$work/sorted")"; done; echo
grep -qE '^(report|differ) ' "$work/sorted" && exit 1
exit 0
