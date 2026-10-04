# The shared half of the compare-*.sh scripts (docs/design/lair-in-fibber.md section 2). Source it; it is not run.
#
# A script defines: NAME (for messages), units (prints one unit per line from the command-line arguments), rust_run UNIT OUT and
# fib_run UNIT OUT (each calls `capture OUT command..`), then calls `compare_main "$@"`.
#
# Environment: LAIR (the Rust lair, default target/debug/lair), LAIRF (the fibber tool; unset or not executable means the fibber side
# is absent), JOBS (default 4, at most 4), TIMEOUT (seconds per run, default 300), VMEM (ulimit -v per run in KiB, default 4000000),
# FIB_TARGET_CPU (default x86-64-v2, exported so both sides generate the same code whatever the machine).
#
# Per unit the Rust side runs twice and the two results must be the same bytes (NONDET otherwise); a Rust run that crashes or times out
# is BROKEN. With the fibber side present its result is compared with the Rust one: SAME or DIFF; a fibber tool that exits 70, or
# says `todo:` on standard error, is PENDING (a stub). Final line: counts. Exit 0 only when every unit is SAME (and there is at
# least one); 1 for any DIFF, NONDET or BROKEN; 70 when nothing differs but some unit is PENDING; 2 for a usage problem.

# capture OUT CMD..: run CMD under the limits; OUT gets its standard output, "--stderr", its standard error and "--status N";
# OUT.status and OUT.err hold the status and the standard error alone.
capture() {
  local out=$1; shift
  ( ulimit -c 0 -v "${VMEM:-4000000}"; timeout "${TIMEOUT:-300}" "$@" > "$out.so" 2> "$out.err" ) 2> /dev/null
  echo $? > "$out.status"
  { cat "$out.so"; echo "--stderr"; cat "$out.err"; echo "--status $(cat "$out.status")"; } > "$out"
  rm -f "$out.so"
}

# the default units: every .lir file under the arguments (a file stands for itself, a directory for what it holds)
units() {
  for a in "$@"; do
    if [ -d "$a" ]; then find "$a" -name '*.lir' | sort; else echo "$a"; fi
  done
}

# skip_stage2: drop the units that are a .lir file (or FILE@N) whose header says `;; stage: 2`: the Rust lair, the oracle of these scripts, does not
# have the instructions of spec/lir.md 4.5 and 6 added by SIMD wave 2, so there is nothing to compare them with; the cases harness (`lairf cases`) runs them.
skip_stage2() {
  local u f
  while IFS= read -r u; do
    f=${u%@*}
    if [ -f "$f" ] && grep -q '^;; stage: *2' "$f"; then continue; fi
    echo "$u"
  done
}

judge() {
  local u=$1 n=$2 r=$T/$n
  rust_run "$u" "$r.a"; rust_run "$u" "$r.b"
  local st; st=$(cat "$r.a.status")
  if [ "$st" -ge 124 ]; then echo "BROKEN $u: the Rust side exited $st: $(head -c 200 "$r.a.err" | head -n 1)" > "$r.res"; return; fi
  if ! cmp -s "$r.a" "$r.b"; then echo "NONDET $u: two Rust runs differ" > "$r.res"; return; fi
  if [ "$FIBER" != yes ]; then echo "PENDING $u: no fibber tool" > "$r.res"; return; fi
  fib_run "$u" "$r.f"
  local fst; fst=$(cat "$r.f.status")
  if [ "$fst" -eq 70 ] || [ "$fst" -eq 134 ] || grep -q 'todo:' "$r.f.err"; then
    echo "PENDING $u: $(grep -h -o 'todo: [A-Za-z0-9.-]* [a-z0-9-]*' "$r.f.err" | head -n 1)" > "$r.res"; return
  fi
  if cmp -s "$r.a" "$r.f"; then echo "SAME $u" > "$r.res"
  else { echo "DIFF $u"; diff <(head -c 100000 "$r.a" | strings -a) <(head -c 100000 "$r.f" | strings -a) | head -n 6; cmp "$r.a" "$r.f" | head -n 1; } > "$r.res"; fi
}

compare_main() {
  local jobs=${JOBS:-4}
  while [ $# -gt 0 ]; do
    case $1 in
      -j) jobs=$2; shift 2 ;;
      --) shift; break ;;
      -*) echo "usage: $NAME.sh [-j N] FILE-or-DIR.." >&2; return 2 ;;
      *) break ;;
    esac
  done
  [ "$jobs" -le 4 ] 2>/dev/null || { echo "$NAME: -j is at most 4 (heavy jobs)" >&2; return 2; }
  LAIR=${LAIR:-target/debug/lair}
  [ -x "$LAIR" ] || { echo "$NAME: the Rust lair is not at $LAIR (set LAIR; cargo build -p lair)" >&2; return 2; }
  export FIB_TARGET_CPU=${FIB_TARGET_CPU:-x86-64-v2}
  FIBER=no; [ -n "${LAIRF:-}" ] && [ -x "$LAIRF" ] && FIBER=yes
  local list; list=$(units "$@" | skip_stage2)
  [ -n "$list" ] || { echo "$NAME: nothing to compare (give files or directories of .lir)" >&2; return 2; }
  T=$(mktemp -d "${TMPDIR:-$HOME/.cache/fibber-scratch}/$NAME.XXXXXX")
  export T FIBER
  local n=0 u
  while IFS= read -r u; do
    n=$((n+1))
    while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do wait -n; done
    judge "$u" "$n" &
  done <<< "$list"
  wait
  local i same=0 diff=0 pend=0 bad=0
  for i in $(seq 1 "$n"); do
    cat "$T/$i.res"
    case $(head -c 4 "$T/$i.res") in
      SAME) same=$((same+1)) ;; DIFF) diff=$((diff+1)) ;; PEND) pend=$((pend+1)) ;; *) bad=$((bad+1)) ;;
    esac
  done
  rm -rf "$T"
  [ "$FIBER" = yes ] || echo "$NAME: the fibber tool is absent (set LAIRF to a lairf): every unit is PENDING"
  echo "$NAME: same $same, different $diff, pending $pend, rust-side failures $bad (of $n)"
  [ $((diff + bad)) -eq 0 ] || return 1
  [ "$pend" -eq 0 ] || return 70
  [ "$same" -gt 0 ]
}
