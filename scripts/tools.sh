#!/bin/bash
# The tool tests of the gate, since MAKE-1 a wrapper over the Makefile: the queue this script held is mk/tools.mk (one stamp build/tools/NAME.ok
# per script, depending on F, the script and its inputs; `make tools` runs them all, `make tools-quick` the quick gate's, `make build/tools/NAME.ok`
# one). Prints one line per script (`ok NAME N s` or `FAIL NAME`), exit 0 when every one passed.
#   scripts/tools.sh [--quick|--full] [FIBC] [OUTDIR]      FIBC: a stage 2 to build with (BUILDER); default the seed. OUTDIR is not read: build/tools/out is the scratch.
#   TOOLS_ONLY names (space separated) to run only those; TOOLS_JOBS how many at once (make -j; default 3); TOOLS_TIMEOUT seconds per script (default 900)
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=${GATE_ROOT:-$(cd "$here/.." && pwd)}
mode=full
case ${1:-} in --quick) mode=quick; shift ;; --full) shift ;; esac
builder=(); [ -n "${1:-}" ] && builder=(BUILDER="$1")
cd "$root" || exit 2
if [ -n "${TOOLS_ONLY:-}" ]; then targets=(); for n in $TOOLS_ONLY; do targets+=("build/tools/$n.ok"); done
elif [ "$mode" = quick ]; then targets=(tools-quick); else targets=(tools); fi
make -k -j"${TOOLS_JOBS:-3}" "${targets[@]}" TOOLS_TIMEOUT="${TOOLS_TIMEOUT:-900}" "${builder[@]}" > build/tools.log 2>&1
bad=0
if [ -n "${TOOLS_ONLY:-}" ]; then stamps=("${targets[@]}"); else mapfile -t stamps < <(make -s tools-list TOOLS_MODE="$mode"); fi
for s in "${stamps[@]}"; do
  n=${s#build/tools/}; n=${n%.ok}
  if make -q "$s" "${builder[@]}" 2> /dev/null; then echo "ok $n $(cat "$s") s"; else echo "FAIL $n (log: ${s%.ok}.log)"; bad=1; tail -n 12 "${s%.ok}.log" 2> /dev/null | sed 's/^/    /'; fi
done
exit $bad
