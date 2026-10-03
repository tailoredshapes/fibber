#!/bin/bash
# prof.sh TOP OUTFILE CMD [ARGS..]: run CMD under the sampling profiler (prof.c, built into $PROF_SO or beside this script)
# and print the self/inclusive report for CMD's executable (the first of CMD; for a compiler pass use its own binary).
# usage: prof.sh 20 /tmp/p.out ./bin_x          stdout of CMD goes to stderr so the report is the only stdout
set -eu
here=$(cd "$(dirname "$0")" && pwd)
top=$1; out=$2; shift 2
so=${PROF_SO:-$here/prof.so}
[ -f "$so" ] || cc -O1 -shared -fPIC -o "$so" "$here/prof.c" -ldl
ulimit -v 16000000
PROF_OUT=$out LD_PRELOAD=$so "$@" >&2
python3 "$here/prof-report.py" "$1" "$out" "$top"
