#!/bin/bash
# The stage 2 truth about main, for CI: runs stage 2's `cases` over cases/ownership, cases/modules and cases/stdlib and fails unless
# the set of cases that do not pass equals the set in scripts/ci-stage2.expected (the known gaps, compiler/mirror-pending/H1-gaps.md).
# A new failure fails; so does an expected failure that now passes (remove its line then). The expected file has one line per case,
# `DIR/CASE STATUS`, status as the table prints it (FAIL, PENDING, HEADER); an OPEN case is one whose own header says it fails
# today (a known gap kept honest in the case itself: if it starts to pass the harness reports FAIL), so it is not listed; lines starting with # and blank lines are ignored.
# usage: scripts/ci-stage2.sh F      F is a stage 2 fibc; FIB_LIB must name lib/ (F links LLVM; with `-l LLVM-21` shared, LLVM's directory is F's rpath)
# Limits (seconds), each the time a directory may take: LIMIT_OWNERSHIP, LIMIT_MODULES, LIMIT_STDLIB.
# Also (scripts/gate.sh sets them; CI does not): CI_STAGE2_JOBS=N runs N cases at a time (`cases -j N`); CI_STAGE2_ONLY=FILE holds the
# names (one per line, as `cases --only` takes them) of the stdlib cases to run, and the expected set is then the lines of the expected
# file for the cases that were run. CI_STAGE2_SHARDS=K runs the stdlib cases in K processes, ownership in a quarter of that: one `cases`
# compiles its cases one after the other (`-j` only runs what is compiled side by side), so shards are what make compiling parallel. The
# directories then run side by side too, and with GATE_SLOTS (scripts/lib/slots.sh) each shard holds a slot while it runs. Unset: one
# `cases` process for each directory, one after the other's results, as CI has always run it.
# The build's comparison recipe (mk/cases.mk, docs/design/build.md): `ci-stage2.sh --from DIR TABLE:CASEDIR:K..` runs nothing; the shard tables
# DIR/TABLE.i.txt (i < K, with .err .code .secs beside them) were made by Make, and this collects them, compares the non-passing set with
# the expected file and, for a full run of a directory, checks the floor of scripts/case-floor.expected. A TABLE named otherwise than its
# CASEDIR (`sample:stdlib:4`) is a sample: the expected lines are those of the cases that ran, and the floor does not apply.
set -u
root=${GATE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
here=$(cd "$(dirname "$0")" && pwd)
from=
if [ "${1:-}" = --cases-line ]; then   # the gate's `cases:` line from the shard files: `cases: ownership 141s (exit 0), modules 7s (exit 0), ..`
  d=${2:?usage: ci-stage2.sh --cases-line DIR TABLE:K..}; shift 2; sep=; line="cases:"
  for spec in "$@"; do
    IFS=: read -r name k <<< "$spec"; secs=0; code=0
    for ((i = 0; i < k; i++)); do
      s=$(cat "$d/$name.$i.secs" 2>/dev/null || echo 0); c=$(cat "$d/$name.$i.code" 2>/dev/null || echo none)
      [ "$s" -gt "$secs" ] && secs=$s
      if [ "$c" = none ] || [ "$code" = none ]; then code=none; elif [ "$c" -eq 124 ] || [ "$code" -eq 124 ]; then code=124; elif [ "$c" -gt "$code" ]; then code=$c; fi
    done
    line="$line$sep $name ${secs}s (exit $code)"; sep=,
  done
  echo "$line"; exit 0
fi
if [ "${1:-}" = --from ]; then from=${2:?usage: ci-stage2.sh --from DIR TABLE:CASEDIR:K..}; shift 2; f=make; out=$from
else f=${1:?usage: ci-stage2.sh F | ci-stage2.sh --from DIR TABLE:CASEDIR:K..}; out=${CI_STAGE2_OUT:-$(mktemp -d)}; fi
mkdir -p "$out"
cd "$root"
bad=0
: > "$out/actual"

# The cases of a directory as `cases --only` names them (the order of `ls`, bytes): a *.fib file, or a directory with a main.fib.
case_names() {
  local e
  for e in $(cd "$1" && LC_ALL=C ls); do
    if [ -f "$1/$e" ]; then case $e in *.fib) echo "$e" ;; esac; elif [ -f "$1/$e/main.fib" ]; then echo "$e"; fi
  done
}

# launch NAME LIMIT SHARDS: starts the run of cases/NAME in the background. With one shard it is `cases DIR [--only ..] [-j N]` as ever; with
# K the cases are dealt out in turn to K processes. A shard's time limit counts the wait for its slot.
launch() {
  local name=$1 limit=$2 k=$3 dir=cases/$1 i j
  local -a names=() jflag=()
  [ -n "${CI_STAGE2_JOBS:-}" ] && jflag=(-j "$CI_STAGE2_JOBS")
  if [ "$name" = stdlib ] && [ -n "${CI_STAGE2_ONLY:-}" ]; then mapfile -t names < "$CI_STAGE2_ONLY"
  elif [ "$k" -gt 1 ]; then mapfile -t names < <(case_names "$dir"); fi
  if [ "${#names[@]}" -gt 0 ] && [ "${#names[@]}" -lt "$k" ]; then k=${#names[@]}; fi
  echo "$k" > "$out/$name.nshards"; echo "${#names[@]}" > "$out/$name.nnames"
  for ((i = 0; i < k; i++)); do
    local -a only=()
    if [ "${#names[@]}" -gt 0 ]; then
      for ((j = i; j < ${#names[@]}; j += k)); do only+=("${names[j]}"); done
      only=(--only "${only[@]}")
    fi
    ( start=$(date +%s)
      "$here/lib/slots.sh" timeout "$limit" "$f" cases "$dir" "${only[@]}" "${jflag[@]}" > "$out/$name.$i.txt" 2> "$out/$name.$i.err"
      echo $? > "$out/$name.$i.code"; echo $(( $(date +%s) - start )) > "$out/$name.$i.secs" ) &
  done
}

# collect NAME: puts the shards of NAME together: $out/NAME.txt (one table and one count line, as `cases` prints them), NAME.err, NAME.code
# (124 if a shard timed out, else the largest exit status) and NAME.secs (the slowest shard).
collect() {
  local name=$1 k i code=0 secs=0 c s
  k=$(cat "$out/$name.nshards")
  if [ "$k" -eq 1 ]; then
    for s in txt err code secs; do cp "$out/$name.0.$s" "$out/$name.$s"; done
    return
  fi
  : > "$out/$name.err"
  for ((i = 0; i < k; i++)); do
    c=$(cat "$out/$name.$i.code"); cat "$out/$name.$i.err" >> "$out/$name.err"
    if [ "$c" -eq 124 ]; then code=124; elif [ "$code" -ne 124 ] && [ "$c" -gt "$code" ]; then code=$c; fi
    s=$(cat "$out/$name.$i.secs"); [ "$s" -gt "$secs" ] && secs=$s
  done
  echo "$code" > "$out/$name.code"; echo "$secs" > "$out/$name.secs"
  { echo "case  status  detail"
    for ((i = 0; i < k; i++)); do awk '$1 ~ /\.fib$/ && NF >= 2' "$out/$name.$i.txt"; done | LC_ALL=C sort -s -k1,1
    for ((i = 0; i < k; i++)); do grep -E '^[0-9]+ cases: ' "$out/$name.$i.txt"; done | awk '
      { all += $1
        for (i = 3; i < NF; i++) { w = $(i+1); gsub(/,/, "", w)
          if (w == "pass") p += $i; else if (w == "fail") f += $i; else if (w == "pending") q += $i
          else if (w == "header") h += $i; else if (w == "open") o += $i } }
      END { printf "\n%d cases: %d pass, %d fail, %d pending, %d header error", all, p, f, q, h
            if (o > 0) printf ", %d open", o
            printf "\n"
            if (o > 0) printf "OPEN: %d of %d cases fail as their `open` label says; an open case is not a pass.\n", o, all
            if (q > 0) printf "PENDING: %d of %d cases did not run; pending is not a pass.\n", q, all }'
  } > "$out/$name.txt"
}

sample=
if [ -n "$from" ]; then   # the tables are there: TABLE:CASEDIR:K, the names dealt out as Make dealt them (mk/cases.mk)
  pairs=()
  for spec in "$@"; do
    IFS=: read -r name cdir k <<< "$spec"
    echo "$k" > "$out/$name.nshards"
    if [ "$name" = "$cdir" ]; then case_names "cases/$cdir" | wc -l > "$out/$name.nnames"; else echo 0 > "$out/$name.nnames"; sample=$name; fi
    pairs+=("$name:-:$k:$cdir")
  done
else
  shards=${CI_STAGE2_SHARDS:-1}
  pairs=("ownership:${LIMIT_OWNERSHIP:-400}:$(( (shards + 3) / 4 )):ownership" "modules:${LIMIT_MODULES:-120}:1:modules" "stdlib:${LIMIT_STDLIB:-1800}:$shards:stdlib")
  if [ "$shards" -le 1 ]; then   # one process at a time, as ever
    for pair in "${pairs[@]}"; do IFS=: read -r name limit k _ <<< "$pair"; launch "$name" "$limit" "$k"; wait; done
  else
    for pair in "${pairs[@]}"; do IFS=: read -r name limit k _ <<< "$pair"; launch "$name" "$limit" "$k"; done
    wait
  fi
  [ -n "${CI_STAGE2_ONLY:-}" ] && sample=stdlib
fi
for pair in "${pairs[@]}"; do
  IFS=: read -r name limit k cdir <<< "$pair"; dir=cases/$cdir
  echo "== $f cases $dir (limit ${limit}s)"
  collect "$name"
  code=$(cat "$out/$name.code")
  echo "exit $code after $(cat "$out/$name.secs")s"
  tail -n 6 "$out/$name.txt"
  awk '$1 ~ /\.fib$/ && NF >= 2 && $2 != "pass" && $2 != "OPEN"' "$out/$name.txt" | cut -c1-1500
  if [ "$code" -eq 124 ]; then echo "ci-stage2: $dir timed out"; bad=1; continue; fi
  total=$(sed -n 's/^\([0-9][0-9]*\) cases: .*/\1/p' "$out/$name.txt" | tail -n 1)
  if [ -z "$total" ] || [ "$total" -eq 0 ]; then echo "ci-stage2: $dir printed no case count (exit $code)"; tail -n 20 "$out/$name.err"; bad=1; continue; fi
  named=$(cat "$out/$name.nnames")
  if [ "$(cat "$out/$name.nshards")" -gt 1 ] && [ "$named" -gt 0 ] && [ "$total" -ne "$named" ]; then
    echo "ci-stage2: $dir ran $total cases in its shards, but $named were dealt out (a name that is the prefix of another?)"; bad=1; continue
  fi
  awk -v d="$dir" '$1 ~ /\.fib$/ && NF >= 2 && $2 != "pass" && $2 != "OPEN" { print d "/" $1 " " $2 }' "$out/$name.txt" >> "$out/actual"
  # The case-count floor (scripts/case-floor.expected): a full run of a directory that ran fewer cases than its floor fails (a merge may have lost cases).
  if [ "$name" = "$cdir" ] && [ -n "$from" ]; then
    floor=$(awk -v d="$cdir" '$1 == d { print $2 }' "$here/case-floor.expected")
    if [ -n "$floor" ] && [ "$total" -lt "$floor" ]; then echo "ci-stage2: $dir ran $total cases, the floor is $floor (scripts/case-floor.expected): a merge may have lost cases"; bad=1; fi
  fi
done
sort -o "$out/actual" "$out/actual"
grep -v -e '^#' -e '^[[:space:]]*$' scripts/ci-stage2.expected | sort > "$out/expected"
if [ -n "$sample" ]; then   # a sample: only the expected lines of the stdlib cases that ran, all of the other directories'
  awk '$1 ~ /\.fib$/ { print "cases/stdlib/" $1 }' "$out/$sample.txt" | sort > "$out/ran"
  awk 'NR==FNR { ran[$1]=1; next } $1 !~ /^cases\/stdlib\// || ($1 in ran)' "$out/ran" "$out/expected" > "$out/expected.sel"
  mv "$out/expected.sel" "$out/expected"
fi
if ! diff -u "$out/expected" "$out/actual"; then
  echo "ci-stage2: the cases that do not pass differ from scripts/ci-stage2.expected (- expected only, + actual only)"
  bad=1
fi
if [ "$bad" -ne 0 ]; then echo "ci-stage2: FAILED"; exit 1; fi
echo "ci-stage2: ok ($(wc -l < "$out/actual") expected non-passing)"
