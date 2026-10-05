#!/bin/bash
# The fuzz run of the language server: builds the server (build.sh) unless SERVER names one, shows the fuzzer can find a crash and a hang
# (fuzz.js --selftest), runs the regression cases of the findings (hardening.js) and then fuzz.js on a few hundred hostile buffers under
# `ulimit -v 8000000`. A finding is a failure; each is saved as JSON under FUZZ_OUT (`node fuzz.js --server S --minimise FILE` shrinks it).
# usage: fuzz.sh [COUNT=300] [SEED=1]     FIBC names the fibc (default `fibc`); FIB_LIB the library; SERVER=PATH skips the build;
#        FUZZ_KINDS=a,b limits the kinds (bytes soup prefix mutated nesting huge flat corpus frames); FUZZ_DEADLINE ms per request (default 20000)
# Exit: 0 no finding; 1 a finding or a failed case; 2 no node or no fibc.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
count=${1:-300}; seed=${2:-1}
command -v node >/dev/null 2>&1 || { echo "fuzz: no node on the PATH" >&2; exit 2; }
export FIB_LIB=${FIB_LIB:-$root/lib}
server=${SERVER:-}
if [ -z "$server" ]; then
  command -v "${FIBC:-fibc}" >/dev/null 2>&1 || [ -x "${FIBC:-}" ] || { echo "fuzz: no fibc: set FIBC" >&2; exit 2; }
  server=${TMPDIR:-/tmp}/lsp-fuzz-$$
  "$here/build.sh" "$server" || { echo "fuzz: FAILED to build the server" >&2; exit 1; }
fi
out=${FUZZ_OUT:-${TMPDIR:-/tmp}/lsp-fuzz-findings}
cd "$root" || exit 2
node "$here/fuzz.js" --selftest || exit 1
node "$here/hardening.js" --server "$server" || { echo "fuzz: the regression cases FAILED" >&2; exit 1; }
args=(--server "$server" --arg -I --arg compiler --count "$count" --seed "$seed" --deadline "${FUZZ_DEADLINE:-20000}" --out "$out")
[ -n "${FUZZ_KINDS:-}" ] && args+=(--kinds "$FUZZ_KINDS")
node "$here/fuzz.js" "${args[@]}" || { echo "fuzz: FINDINGS (saved under $out)" >&2; exit 1; }
echo "fuzz: ok"
