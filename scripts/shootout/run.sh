#!/bin/bash
# The language shootout runner: builds and times every benchmark under scripts/shootout/*/ (a directory with a
# sizes.txt) in fibber, Java, Clojure and C, checks every output against the md5 in sizes.txt, prints a markdown
# table and writes the raw rows as TSV.
#
#   FIBC=/path/to/bin/fibc scripts/shootout/run.sh [NAME..] [--size small|full] [--langs fib,java,clj,c]
#                                                    [-n RUNS] [--out FILE]
#   scripts/shootout/run.sh --render FILE.tsv        # only print the table of an earlier run
#
# NAME..   benchmark directories to run (default: every scripts/shootout/*/ with a sizes.txt whose name does
#          not start with `_`; `_selftest` and `_selftest-bad` are run when named).
# --size   small (default) or full, or a comma list: small,full.
# -n       runs per measurement (default 5 at small, 3 at full); the median by elapsed time is reported.
# --out    raw TSV (default $SHOOT_SCRATCH/rows.tsv).
# Environment: FIBC (else bin/fibc of the tree, else fibc on PATH), FIB_LIB (default <tree>/lib),
#   SHOOT_SCRATCH (build products and cached inputs; default ~/.cache/fibber-shootout), SHOOT_CLJ (directory of the
#   Clojure jars; default ~/.cache/fibber-scratch/B4-D/clj), SHOOT_JAVA_FLAGS (flags for java and clj alike),
#   SHOOT_FASTA_DIR (the fasta benchmark directory; default scripts/shootout/fasta), SHOOT_TIMEOUT (seconds per
#   run, default 3600), SHOOT_NOLOCK=1 (do not take /tmp/fibsuite.lock), SHOOT_CC (default gcc).
# Protocol (see README.md): every timed run holds `flock /tmp/fibsuite.lock`; fibber runs under ulimit -v 16000000;
# /usr/bin/time -f '%e %U %M' gives elapsed s, user CPU s and peak RSS KB. Exit status 1 if any row is FAIL.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
scratch=${SHOOT_SCRATCH:-$HOME/.cache/fibber-shootout}
cljdir=${SHOOT_CLJ:-$HOME/.cache/fibber-scratch/B4-D/clj}
fastadir=${SHOOT_FASTA_DIR:-$here/fasta}
timeout_s=${SHOOT_TIMEOUT:-3600}
cc=${SHOOT_CC:-gcc}
lock=/tmp/fibsuite.lock

sizes=small; langs=fib,java,clj,c; runs=; out=; render=; names=()
while [ $# -gt 0 ]; do
  case $1 in
    --size) sizes=$2; shift 2 ;;
    --langs) langs=$2; shift 2 ;;
    -n) runs=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --render) render=$2; shift 2 ;;
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    -*) echo "run.sh: unknown option $1" >&2; exit 2 ;;
    *) names+=("${1%/}"); shift ;;
  esac
done

HDR='bench\tsize\tn\tlang\tstatus\telapsed_s\tuser_s\trss_kb\tprog_s\tmd5\tnote'

# ---------------------------------------------------------------- the table (TSV -> markdown)
render_table() {
  awk -F'\t' -v OFS='\t' '
  function cell(b, s, l,    k, v) {
    k = b SUBSEP s SUBSEP l
    if (!(k in st)) return "-"
    if (st[k] != "ok") return st[k]
    v = sprintf("%.2f / %.2f / %.0f", el[k], us[k], rss[k] / 1024)
    if (l == "clj" && pr[k] != "") v = sprintf("%.2f (%.2f) / %.2f / %.0f", el[k], pr[k], us[k], rss[k] / 1024)
    return v
  }
  function ratio(b, s, num, den, useprog,    kn, kd, d) {
    kn = b SUBSEP s SUBSEP num; kd = b SUBSEP s SUBSEP den
    if (st[kn] != "ok" || st[kd] != "ok") return "-"
    d = el[kd]; if (useprog) { if (pr[kd] == "") return "-"; d = pr[kd] }
    if (d <= 0) return "-"
    return sprintf("%.2f", el[kn] / d)
  }
  NR == 1 { next }
  {
    k = $1 SUBSEP $2 SUBSEP $4
    st[k] = $5; el[k] = $6; us[k] = $7; rss[k] = $8; pr[k] = $9; nn[$1 SUBSEP $2] = $3; note[k] = $11
    if (!(($1 SUBSEP $2) in seen)) { seen[$1 SUBSEP $2] = 1; order[++cnt] = $1 SUBSEP $2 }
  }
  END {
    print "| benchmark | size | N | fibber | Java | Clojure | C | fib/Java | fib/clj (wall) | fib/clj (in-prog) | fib/C |"
    print "|---|---|---|---|---|---|---|---|---|---|---|"
    for (i = 1; i <= cnt; i++) {
      split(order[i], p, SUBSEP); b = p[1]; s = p[2]
      if (b ~ /^_hello$/) continue
      printf "| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n", b, s, nn[b SUBSEP s],
        cell(b, s, "fib"), cell(b, s, "java"), cell(b, s, "clj"), cell(b, s, "c"),
        ratio(b, s, "fib", "java", 0), ratio(b, s, "fib", "clj", 0), ratio(b, s, "fib", "clj", 1), ratio(b, s, "fib", "c", 0)
    }
    print ""
    print "Cells are `elapsed s / user s / peak RSS MB` (median run); Clojure shows `wall (in-program) / user / RSS`."
    print "A cell of FAIL means the output md5 differed from sizes.txt or the program failed: no number is given."
    print ""
    for (k in st) if (st[k] ~ /^FAIL/ && note[k] != "") { split(k, q, SUBSEP); printf "- FAIL %s %s %s: %s\n", q[1], q[2], q[3], note[k] }
    hd = 0
    for (i = 1; i <= cnt; i++) {
      split(order[i], p, SUBSEP); if (p[1] != "_hello") continue
      if (!hd) { print ""; print "Baselines (hello world, median elapsed s / user s / RSS MB):"; print ""; print "| fibber | Java | Clojure (`clojure.main -e nil`) | C |"; print "|---|---|---|---|"; hd = 1 }
      s = p[2]
      printf "| %s | %s | %s | %s |\n", cell("_hello", s, "fib"), cell("_hello", s, "java"), cell("_hello", s, "clj"), cell("_hello", s, "c")
    }
  }' "$1"
}

if [ -n "$render" ]; then render_table "$render"; exit 0; fi

# ---------------------------------------------------------------- setup
fibc=${FIBC:-}
[ -z "$fibc" ] && [ -x "$root/bin/fibc" ] && fibc=$root/bin/fibc
[ -z "$fibc" ] && fibc=$(command -v fibc || true)
export FIB_LIB=${FIB_LIB:-$root/lib}
mkdir -p "$scratch"/{bin,java,clj,input,tmp}
out=${out:-$scratch/rows.tsv}
printf "$HDR\n" > "$out"
jars=$cljdir/clojure-1.12.0.jar:$cljdir/core.specs.alpha-0.4.74.jar:$cljdir/spec.alpha-0.5.238.jar
jflags=${SHOOT_JAVA_FLAGS:-}
want_lang() { case ",$langs," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
fail_any=0
tmp=$scratch/tmp

# row BENCH SIZE N LANG STATUS ELAPSED USER RSS PROG MD5 NOTE
row() { printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$@" >> "$out"; }

# timed_median RUNS EXPECT_MD5 INPUT LANG CMD...   sets RES_STATUS RES_E RES_U RES_M RES_P RES_MD5 RES_NOTE
timed_median() {
  local n=$1 expect=$2 input=$3 lang=$4; shift 4
  local i rows=() bad="" e u m p md pre=()
  [ -z "${SHOOT_NOLOCK:-}" ] && pre=(flock "$lock")
  RES_NOTE=""; RES_MD5="-"; RES_P=""
  for i in $(seq "$n"); do
    if [ "$lang" = fib ]; then
      "${pre[@]}" /usr/bin/time -f '%e %U %M' -o "$tmp/t" timeout "$timeout_s" bash -c 'ulimit -v 16000000; exec "$@"' _ "$@" < "$input" > "$tmp/stdout" 2> "$tmp/stderr"   # linux-only: GNU time -f/-o for wall seconds and peak memory (BSD time has -l)
    else
      "${pre[@]}" /usr/bin/time -f '%e %U %M' -o "$tmp/t" timeout "$timeout_s" "$@" < "$input" > "$tmp/stdout" 2> "$tmp/stderr"   # linux-only: GNU time -f/-o for wall seconds and peak memory (BSD time has -l)
    fi
    local rc=$?
    if [ $rc -ne 0 ]; then bad="exit status $rc: $(head -c 200 "$tmp/stderr" | tr '\n\t' '  ')"; break; fi
    read -r e u m < <(tail -n 1 "$tmp/t")
    md=$(md5sum < "$tmp/stdout" | cut -d' ' -f1)
    RES_MD5=$md
    if [ -n "$expect" ] && [ "$md" != "$expect" ]; then bad="md5 $md, expected $expect"; break; fi
    p=""
    if [ "$lang" = clj ]; then
      # in-program time: the last number on the last stderr line that has one (seconds; `ms` after it means ms)
      p=$(awk '/in-program|clj-compute/ { l = $0; lab = 1 } !lab && /[0-9]/ { l = $0 } END { if (l == "") exit; if (match(l, /[0-9]+(\.[0-9]+)?([eE]-?[0-9]+)?/)) { v = substr(l, RSTART, RLENGTH) + 0; rest = substr(l, RSTART + RLENGTH); if (rest ~ /^ *ms/ || substr(l, 1, RSTART - 1) ~ /-ms[: ]*$/) v = v / 1000; printf "%.4f", v } }' "$tmp/stderr")
      if grep -q -i 'reflection warning\|boxed math warning' "$tmp/stderr"; then RES_NOTE="clj warnings on stderr"; fi
    fi
    rows+=("$e $u $m ${p:-_}")
  done
  if [ -n "$bad" ]; then RES_STATUS=FAIL; RES_E=""; RES_U=""; RES_M=""; RES_NOTE=$bad; return; fi
  read -r RES_E RES_U RES_M RES_P < <(printf '%s\n' "${rows[@]}" | sort -g | awk '{a[NR]=$0} END{print a[int((NR+1)/2)]}')
  [ "$RES_P" = _ ] && RES_P=""
  RES_STATUS=ok
}

# ---------------------------------------------------------------- baselines
hello() {
  local n=${1:-5} size=baseline
  if want_lang fib && [ -n "$fibc" ]; then
    if "$fibc" build "$root/scripts/bench/hello.fib" -I "$root/lib" -o "$scratch/bin/hello-fib" > "$tmp/build.log" 2>&1; then
      timed_median "$n" "" /dev/null fib "$scratch/bin/hello-fib"; row _hello $size - fib "$RES_STATUS" "$RES_E" "$RES_U" "$RES_M" "" - "$RES_NOTE"
    else row _hello $size - fib FAIL "" "" "" "" - "build failed"; fi
  fi
  if want_lang java; then
    mkdir -p "$scratch/java/_hello"
    printf 'public class Hello { public static void main(String[] a) { System.out.println("hi"); } }\n' > "$scratch/java/_hello/Hello.java"
    javac -d "$scratch/java/_hello" "$scratch/java/_hello/Hello.java" && {
      timed_median "$n" "" /dev/null java java $jflags -cp "$scratch/java/_hello" Hello; row _hello $size - java "$RES_STATUS" "$RES_E" "$RES_U" "$RES_M" "" - "$RES_NOTE"; }
  fi
  if want_lang clj && [ -f "$cljdir/clojure-1.12.0.jar" ]; then
    timed_median "$n" "" /dev/null clj java $jflags -cp "$jars" clojure.main -e nil; row _hello $size - clj "$RES_STATUS" "$RES_E" "$RES_U" "$RES_M" "" - "$RES_NOTE"
  fi
  if want_lang c; then
    printf '#include <stdio.h>\nint main(void){puts("hi");return 0;}\n' > "$tmp/hello.c"
    $cc -O3 -march=native -o "$scratch/bin/hello-c" "$tmp/hello.c" && {
      timed_median "$n" "" /dev/null c "$scratch/bin/hello-c"; row _hello $size - c "$RES_STATUS" "$RES_E" "$RES_U" "$RES_M" "" - "$RES_NOTE"; }
  fi
}

# ---------------------------------------------------------------- inputs for the stdin benchmarks
# needs_stdin NAME: does the benchmark read the fasta output on stdin?
needs_stdin() {
  case $1 in k-nucleotide|reverse-complement|regex-redux) return 0 ;; esac
  grep -q '^stdin[[:space:]]\+fasta' "$here/$1/sizes.txt" 2>/dev/null
}
# fasta_input N: prints the path of the cached fasta output of N (generated by the Java fasta twin once)
fasta_input() {
  local n=$1 f gen; gen=$(md5sum < "$fastadir/fasta.java" 2>/dev/null | cut -c1-8); f=$scratch/input/fasta-$n-$gen.txt
  if [ ! -s "$f" ]; then
    if [ ! -f "$fastadir/fasta.java" ]; then echo "no fasta twin at $fastadir/fasta.java" >&2; return 1; fi
    mkdir -p "$scratch/java/fasta"
    javac -d "$scratch/java/fasta" "$fastadir/fasta.java" || return 1
    java $jflags -cp "$scratch/java/fasta" fasta "$n" > "$f.part" || { rm -f "$f.part"; return 1; }
    mv "$f.part" "$f"
    md5sum < "$f" | cut -d' ' -f1 > "$f.md5"
  fi
  # the fasta benchmark's own sizes.txt, if it has this N, says what the md5 must be
  local want=""
  [ -f "$fastadir/sizes.txt" ] && want=$(awk -v n="$n" '{ hasn = 0; md = ""; for (i = 2; i <= NF; i++) { if ($i == n) hasn = 1; if ($i ~ /^[0-9a-f]{32}$/) md = $i } if (hasn && md != "") { print md; exit } }' "$fastadir/sizes.txt")
  if [ -n "$want" ] && [ "$want" != "$(cat "$f.md5")" ]; then echo "fasta input $n has md5 $(cat "$f.md5"), sizes.txt says $want" >&2; return 1; fi
  echo "$f"
}

# ---------------------------------------------------------------- one benchmark
# src DIR NAME EXT: the source file of a benchmark: NAME.EXT, else NAME without hyphens .EXT (n-body -> nbody.java)
src() {
  local f=$1/$2.$3 g=$1/${2//-/}.$3 h=$1/${2//-/_}.$3
  if [ -f "$f" ]; then echo "$f"; elif [ -f "$g" ]; then echo "$g"; elif [ -f "$h" ]; then echo "$h"; fi
}

build_all() {  # NAME: builds the languages in $langs into $scratch; sets HAVE_<lang> flags
  local name=$1 d=$here/$1 s; CLJ_NS=; CLJ_MODE=; CLJ_FILE=; JAVA_CLASS=
  HAVE_fib=; HAVE_java=; HAVE_clj=; HAVE_c=; BUILD_NOTE_fib=; BUILD_NOTE_java=; BUILD_NOTE_clj=; BUILD_NOTE_c=
  if want_lang fib; then
    s=$(src "$d" "$name" fib)
    if [ -z "$s" ]; then BUILD_NOTE_fib="no $name.fib"
    elif [ -z "$fibc" ]; then BUILD_NOTE_fib="no fibc (set FIBC)"
    elif "$fibc" build "$s" -I "$root/lib" -o "$scratch/bin/$name-fib" > "$tmp/build-fib.log" 2>&1; then HAVE_fib=1
    else BUILD_NOTE_fib="fibc build failed: $(head -c 300 "$tmp/build-fib.log" | tr '\n\t' '  ')"; fi
  fi
  if want_lang java; then
    s=$(src "$d" "$name" java)
    if [ -z "$s" ]; then BUILD_NOTE_java="no $name.java"
    else
      # the class to run: the first top-level `class NAME` of the file (binary-trees.java declares binarytrees)
      JAVA_CLASS=$(sed -n -E 's/^(public )?(final )?(abstract )?class[[:space:]]+([A-Za-z0-9_$]*).*/\4/p' "$s" | head -1)
      [ -z "$JAVA_CLASS" ] && JAVA_CLASS=${name//-/}
      rm -rf "$scratch/java/$name"; mkdir -p "$scratch/java/$name"
      if javac -d "$scratch/java/$name" "$s" > "$tmp/build-java.log" 2>&1; then HAVE_java=1
      else BUILD_NOTE_java="javac failed: $(head -c 300 "$tmp/build-java.log" | tr '\n\t' '  ')"; fi
    fi
  fi
  if want_lang clj; then
    s=$(src "$d" "$name" clj)
    if [ -z "$s" ]; then BUILD_NOTE_clj="no $name.clj"
    elif [ ! -f "$cljdir/clojure-1.12.0.jar" ]; then BUILD_NOTE_clj="no Clojure jars in $cljdir"
    else
      # a file whose top level calls -main (`(apply -main *command-line-args*)`, `(when .. (-main ..))`) is a script:
      # `clojure.main FILE N`; otherwise it must declare an ns and is run as `clojure.main -m NS N`
      CLJ_MODE=ns; CLJ_FILE=$s
      if grep -q -E '^\((when|if|apply|-main)[^;]*-main|^\(-main' "$s"; then CLJ_MODE=script
      else
        CLJ_NS=$(sed -n -E 's/^\(ns[[:space:]]+([^][[:space:]()]*).*/\1/p' "$s" | head -1)
      fi
      if [ "$CLJ_MODE" = ns ] && [ -z "$CLJ_NS" ]; then BUILD_NOTE_clj="$s: no top-level -main call and no (ns NAME ..) form"
      else
        if [ "$CLJ_MODE" = ns ]; then
          local rel; rel=$(echo "$CLJ_NS" | tr '-' '_' | tr '.' '/')
          rm -rf "$scratch/clj/$name"; mkdir -p "$scratch/clj/$name/$(dirname "$rel")"
          cp "$s" "$scratch/clj/$name/$rel.clj"
        else mkdir -p "$scratch/clj/$name"; fi
        HAVE_clj=1
      fi
    fi
  fi
  if want_lang c; then
    s=$(src "$d" "$name" c)
    # cflags: extra gcc arguments (`-lgmp`), from the file `cflags` of the directory; -ffp-contract=off when the README asks for it
    local extra=""
    [ -f "$d/cflags" ] && extra=$(tr '\n' ' ' < "$d/cflags")
    grep -q -e '-ffp-contract=off' "$d/README.md" 2>/dev/null && case " $extra " in *ffp-contract*) ;; *) extra="-ffp-contract=off $extra" ;; esac
    if [ -z "$s" ]; then BUILD_NOTE_c="no $name.c"
    elif $cc -O3 -march=native $extra -o "$scratch/bin/$name-c" "$s" -lm $extra > "$tmp/build-c.log" 2>&1; then HAVE_c=1
    else BUILD_NOTE_c="$cc failed: $(head -c 300 "$tmp/build-c.log" | tr '\n\t' '  ')"; fi
  fi
}

# sizes_line NAME SIZE: prints "N MD5" from sizes.txt (lines: `SIZE N 1000 [md5 ]HEX`, any order after the size word)
sizes_line() {
  awk -v want="$2" '$1 == want { for (i = 2; i <= NF; i++) { if ($i ~ /^[0-9a-f]{32}$/) md = $i; else if ($i ~ /^[0-9]+$/) n = $i } }
                    END { if (n != "") print n, md }' "$here/$1/sizes.txt"
}

run_bench() {  # NAME SIZE
  local name=$1 size=$2 n md r input=/dev/null l
  read -r n md <<< "$(sizes_line "$name" "$size")"
  if [ -z "${n:-}" ]; then echo "run.sh: $name has no '$size' line in sizes.txt: skipped" >&2; return; fi
  if [ -z "${md:-}" ]; then echo "run.sh: $name $size has no md5 in sizes.txt: every row is FAIL" >&2; fi
  r=${runs:-$([ "$size" = full ] && echo 3 || echo 5)}
  if needs_stdin "$name"; then
    if ! input=$(fasta_input "$n" 2> "$tmp/input.log"); then
      for l in fib java clj c; do want_lang $l && row "$name" "$size" "$n" $l FAIL "" "" "" "" - "input: $(head -c 300 "$tmp/input.log" | tr '\n\t' '  ')"; done
      fail_any=1; return
    fi
  fi
  build_all "$name"
  for l in fib java clj c; do
    want_lang $l || continue
    local have; eval "have=\${HAVE_$l}"
    local note; eval "note=\${BUILD_NOTE_$l}"
    if [ -z "$have" ]; then row "$name" "$size" "$n" $l "n/a" "" "" "" "" - "$note"; echo "run.sh: $name $l: $note" >&2; continue; fi
    case $l in
      fib) timed_median "$r" "${md:-none}" "$input" fib "$scratch/bin/$name-fib" "$n" ;;
      java) timed_median "$r" "${md:-none}" "$input" java java $jflags -cp "$scratch/java/$name" "$JAVA_CLASS" "$n" ;;
      clj) if [ "$CLJ_MODE" = script ]; then
             timed_median "$r" "${md:-none}" "$input" clj java $jflags -cp "$jars" clojure.main "$CLJ_FILE" "$n"
           else
             timed_median "$r" "${md:-none}" "$input" clj java $jflags -cp "$jars:$scratch/clj/$name" clojure.main -m "$CLJ_NS" "$n"
           fi ;;
      c) timed_median "$r" "${md:-none}" "$input" c "$scratch/bin/$name-c" "$n" ;;
    esac
    [ "$RES_STATUS" = FAIL ] && fail_any=1
    row "$name" "$size" "$n" $l "$RES_STATUS" "$RES_E" "$RES_U" "$RES_M" "$RES_P" "$RES_MD5" "$RES_NOTE"
    echo "run.sh: $name $size $l $RES_STATUS ${RES_E:-} ${RES_NOTE:-}" >&2
  done
}

# ---------------------------------------------------------------- main
if [ ${#names[@]} -eq 0 ]; then
  for d in "$here"/*/; do
    b=$(basename "$d")
    case $b in _*) continue ;; esac
    [ -f "$d/sizes.txt" ] && names+=("$b")
  done
fi
for b in "${names[@]}"; do
  [ -f "$here/$b/sizes.txt" ] || { echo "run.sh: $b: no scripts/shootout/$b/sizes.txt" >&2; exit 2; }
done
echo "run.sh: fibc=${fibc:-none} ($( [ -n "$fibc" ] && "$fibc" --version 2>&1 | head -1 )); java=$(java -version 2>&1 | head -1); benchmarks: ${names[*]:-none}; sizes: $sizes" >&2
hello "${runs:-5}"
for b in "${names[@]}"; do
  for s in ${sizes//,/ }; do run_bench "$b" "$s"; done
done
render_table "$out"
echo
echo "raw rows: $out"
exit $fail_any
