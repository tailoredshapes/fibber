#!/bin/bash
# Per-record tally of the ownership dump: `fibref own` against the fibber tool, one process per file.
# usage: compare-bodies.sh [-j N] [-o "OPTIONS"] FIBREF TOOL FILE..
#   FIBREF  the Rust tool (target/debug/fibref), TOOL the built compiler/own.fib binary
#   -o      options given to both tools, e.g. "--sections body,summary" (spec/bootstrap.md section 7)
# The pass is all or nothing per file (a stub that traps loses the file), so compare.sh -c own says only
# how many files are identical. This script cuts each output at its top-level lines (one record per
# `body`, `facts`, `summary`, `taken` and `error` line with the indented lines under it, and one for
# the exit status), pairs the records of the two outputs by kind, module, head line and occurrence
# number in the module, and counts per kind how many are identical and how many differ or are missing
# from either side: a port that gets 70 of 100 bodies right shows it, which a file tally cannot.
# Output: the tally per kind, then the first ten differing records. The raw outputs of every file that
# has a difference are kept in $CMP_OUT (default $HOME/.cache/fibber-scratch/compare-bodies) as NAME.rust
# and NAME.fib. Run from the repository root. Each process runs under ulimit -v 4000000 and a 120 s timeout.
jobs=4; opts=""
while getopts "j:o:" o; do case $o in j) jobs=$OPTARG;; o) opts=$OPTARG;; esac; done
shift $((OPTIND-1)); rust=$1; tool=$2; shift 2
out=${CMP_OUT:-$HOME/.cache/fibber-scratch/compare-bodies}; rm -rf "$out"; mkdir -p "$out"
# One line per record: KEY TAB TEXT, the line breaks and tabs of the text made \037 and \036.
records() {
  awk '
    BEGIN { OFS = "\t"; LF = "\037"; TAB = "\036" }
    function flush() { if (key != "") print key, text; key = ""; text = "" }
    /^[ \t]/ || /^$/ { if (key != "") { gsub(/\t/, TAB); text = text LF $0 }; next }
    { flush() }
    /^== / { mod = ""; next }
    /^-- module / { mod = $3; next }
    {
      head = $0; kind = $1
      if (kind == "summary") head = $1 " " $2
      if (kind == "error" || kind == "status") head = kind
      key = kind "|" mod "|" head "|" ++count[mod "|" kind "|" head]
      gsub(/\t/, TAB); text = $0
    }
    END { flush() }'
}
one() {
  f=$1; n=$(echo "$f" | tr '/' '_')
  (ulimit -v 4000000; timeout 120 "$rust" own $opts "$f" 2>&1; echo "status $?") > "$out/$n.rust"
  (ulimit -v 4000000; timeout 120 "$tool" $opts "$f" 2>&1; echo "status $?") > "$out/$n.fib"
  records < "$out/$n.rust" | LC_ALL=C sort -t $'\t' -k1,1 > "$out/$n.r"
  records < "$out/$n.fib" | LC_ALL=C sort -t $'\t' -k1,1 > "$out/$n.f"
  LC_ALL=C join -t $'\t' -a 1 -a 2 -e '<missing>' -o 0,1.2,2.2 "$out/$n.r" "$out/$n.f" |
    awk -F '\t' -v f="$f" '{ split($1, k, "|"); print ($2 == $3 ? "same" : "DIFF"), k[1], $1, f }' > "$out/$n.tally"
  rm -f "$out/$n.r" "$out/$n.f"
  if grep -q '^DIFF' "$out/$n.tally"; then :; else rm -f "$out/$n.rust" "$out/$n.fib"; fi
}
export -f one records; export rust tool opts out
printf '%s\n' "$@" | xargs -P "$jobs" -I{} bash -c 'one {}'
cat "$out"/*.tally > "$out/tally.txt" 2>/dev/null
rm -f "$out"/*.tally
echo "files $#"
awk '{ n[$2]++; if ($1 == "same") s[$2]++ } END { for (k in n) printf "%-8s same %d, different %d\n", k ":", s[k], n[k] - s[k] }' "$out/tally.txt" | sort
grep '^DIFF' "$out/tally.txt" | head -10 | cut -d' ' -f3-
