#!/bin/bash
# scripts/bench/json/run.sh: the JSON shootout of docs/shootout/json.md on one machine. Single thread, `ulimit -v 16000000`, every fib.json number the median of 5 runs
# (the competitors' programs take the median of 5 batches themselves), the whole run under `flock /tmp/fibsuite.lock`.
# Environment (all with defaults for the layout of the shootout directory):
#   D        directory holding corpus/{twitter,citm_catalog,canada}.json, big.ndjson, fibber-bench (built with FIB_TARGET_CPU=x86-64-v3),
#            simdjson/simdjson-bench, venv/bin/python (orjson), jackson/ (the jars and JacksonBench.class), and this directory's bench.py
#   S        the directory of this script (bench.py)
set -u
D=${D:-$PWD}; S=${S:-$(cd "$(dirname "$0")" && pwd)}
ulimit -v 16000000
exec 9>/tmp/fibsuite.lock; flock 9
JARS=$D/jackson/jackson-core-2.17.2.jar:$D/jackson/jackson-databind-2.17.2.jar:$D/jackson/jackson-annotations-2.17.2.jar
median5() { # median5 CMD..: runs the command 5 times, prints the median of the number after "MB/s"
  for i in 1 2 3 4 5; do "$@" | awk '{for(i=1;i<=NF;i++) if($i=="MB/s") print $(i+1)}'; done | sort -g | sed -n 3p
}
declare -A REPS=([twitter]=60 [citm_catalog]=25 [canada]=15)
echo "machine: $(grep -m1 'model name' /proc/cpuinfo | sed 's/.*: //'), $(nproc) threads, $(date -u +%F)"
for f in twitter citm_catalog canada; do
  p=$D/corpus/$f.json; r=${REPS[$f]}
  echo "== $f ($(stat -c %s "$p") bytes)"
  for mode in dom tape write; do printf "fibber %-8s %8.1f MB/s\n" $mode "$(median5 $D/fibber-bench $mode $p $r)"; done
  if [ $f = twitter ]; then for mode in three typed; do printf "fibber %-8s %8.1f MB/s\n" $mode "$(median5 $D/fibber-bench $mode $p $r)"; done; fi
  $D/simdjson/simdjson-bench $p $r | head -2
  $D/venv/bin/python $S/bench.py $p $r
  (cd $D/jackson && java -Xmx2g -cp .:$JARS JacksonBench $p $r)
done
echo "== big.ndjson ($(stat -c %s $D/big.ndjson) bytes)"
size=$(stat -c %s $D/big.ndjson)
for i in 1 2 3; do $D/fibber-bench lines $D/big.ndjson $size; done | awk '{print}' | sort -k5 -g | sed -n 2p
$D/simdjson/simdjson-bench $D/big.ndjson 1
$D/venv/bin/python $S/bench.py $D/big.ndjson 1
(cd $D/jackson && java -Xmx2g -cp .:$JARS JacksonBench $D/big.ndjson 1)
