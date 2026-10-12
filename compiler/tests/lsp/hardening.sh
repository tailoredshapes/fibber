#!/bin/bash
# compiler/tests/lsp/hardening.sh OUT: the language server's hardening (the `lsp-hardening` tool stamp). Two independent jobs side by side:
#   - fuzz.js --selftest (a fake server; one probe waits out its hang deadline, about 8 s) runs while the server is built;
#   - build.sh OUT builds the server, then hardening.js --parallel runs its cases side by side (the latency case alone at the end).
# Exit 0 when all held. FIBC names stage 2 (as build.sh).
set -u
here=$(cd "$(dirname "$0")" && pwd)
out=${1:?usage: hardening.sh OUT}
node "$here/fuzz.js" --selftest > "$out.selftest.log" 2>&1 &
st=$!
rc=0
"$here/build.sh" "$out" && node "$here/hardening.js" --server "$out" --parallel || rc=1
wait "$st" || { echo "FAIL fuzz.js --selftest"; rc=1; }
cat "$out.selftest.log"
exit $rc
