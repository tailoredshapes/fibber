#!/usr/bin/env bash
# Focused file-based observability graph: compiled specs, TCP receiver and official collector.
# OTLP_FIBC/FIBC select an existing compiler; OTLP_TEST_OUT selects a reusable build/result store.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
fibc=${OTLP_FIBC:-${FIBC:-$root/build/F}}
fibc=$(command -v "$fibc") || { echo "test-otlp: build F or set OTLP_FIBC" >&2; exit 2; }
fibc=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$fibc")
scratch=${OTLP_TEST_OUT:-$root/build/otel-checks}
if [ ! -e /tmp/fibsuite.lock ]; then
  (set -o noclobber; : > /tmp/fibsuite.lock) 2>/dev/null || true
fi
exec 9</tmp/fibsuite.lock
flock -x 9
exec make -C "$root" observability "OBS_FIBC=$fibc" "BUILD=$scratch"
