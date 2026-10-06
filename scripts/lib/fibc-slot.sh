#!/bin/bash
# The fibc the gate's tool scripts get as FIBC: runs the real one (GATE_REAL_FIBC) under a slot of scripts/lib/slots.sh. `cases` and `test` run
# at once: whoever starts them gives them their own share (their own -j).
case ${1:-} in cases|test) exec "$GATE_REAL_FIBC" "$@" ;; esac
exec "$(dirname "${BASH_SOURCE[0]}")/slots.sh" "$GATE_REAL_FIBC" "$@"
