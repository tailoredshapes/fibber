#!/bin/bash
# The language server's tests: builds the server (build.sh) and replays the recorded transcripts and this port's own scenarios against it
# (replay.js), then, with `planted`, shows the comparator failing on each planted fault (range, drop, code, framing).
# usage: server.sh [OUT] [planted]      FIBC names the fibc (default `fibc`); REAL=1 uses the real L0 to L2 modules instead of standin/;
#                                    SERVER=PATH skips the build and replays against PATH (with SERVER_ARGS, e.g. `lsp`, for `fibc`)
# Exit: 0 all held (and every planted fault was caught); 1 a check failed or a fault went unnoticed; 2 no node or no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
out=${1:-${TMPDIR:-/tmp}/lsp-serve-$$}
mode=${2:-}
command -v node >/dev/null 2>&1 || { echo "lsp tests: no node on the PATH" >&2; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
server=${SERVER:-}
if [ -z "$server" ]; then
  command -v "${FIBC:-fibc}" >/dev/null 2>&1 || [ -x "${FIBC:-}" ] || { echo "lsp tests: no fibc: set FIBC" >&2; exit 2; }
  "$here/build.sh" "$out" || { echo "lsp tests: FAILED to build the server" >&2; exit 1; }
  server=$out
fi
args=()
for a in ${SERVER_ARGS:-}; do args+=(--arg "$a"); done
ulimit -v "${ULIMIT_V:-16000000}"
node "$here/replay.js" --server "$server" "${args[@]}" || { echo "lsp tests: FAILED" >&2; exit 1; }
if [ "$mode" = planted ]; then
  for fault in range drop code framing; do
    if node "$here/replay.js" --server "$server" "${args[@]}" --fault "$fault" > "${TMPDIR:-/tmp}/lsp-fault-$fault.out" 2>&1; then
      echo "lsp tests: planted fault $fault was NOT caught" >&2; exit 1
    fi
    echo "planted fault $fault caught: $(grep -m1 -A2 '^FAIL' "${TMPDIR:-/tmp}/lsp-fault-$fault.out" | tr '\n' ' ' | cut -c1-200)"
  done
fi
echo "lsp tests: ok"
