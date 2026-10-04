#!/bin/bash
# The `cc` command line of `build-executable` (package A, gate 3): `lair build` and `lairf build` run with a fake `cc` first on PATH that records its
# arguments (one per line) and, per scenario, fails or succeeds; the recorded arguments, the standard error and the exit status of the two tools must
# be equal, argument for argument (the cc shim of scripts/package.sh rewrites the word after `-rpath`, so the order and the spelling are the contract).
# Scenarios: no library, a library directory (relative, absolute, a symlink to one) and libraries, two of each, a missing -L, a -L that is a file, a -L with
# a ':' and with a '$', a failing cc (its standard error is the error text), no cc at all.
# usage: a-cc-args.sh        run from the repository root: LAIR=target/debug/lair LAIRF=path/to/lairf
# Exit: 0 all scenarios equal; 1 a difference; 2 a usage problem.
set -u
LAIR=${LAIR:-target/debug/lair}
[ -x "$LAIR" ] && [ -n "${LAIRF:-}" ] && [ -x "$LAIRF" ] || { echo "a-cc-args: set LAIR and LAIRF to executables" >&2; exit 2; }
export FIB_TARGET_CPU=${FIB_TARGET_CPU:-x86-64-v2}
T=$(mktemp -d "${TMPDIR:-$HOME/.cache/fibber-scratch}/a-cc-args.XXXXXX")
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin" "$T/work/a dir" "$T/work/b" "$T/work/a:b"
mkdir -p "$T/work/d\$x"
ln -s "$T/work/b" "$T/work/link-b"
touch "$T/work/afile"
cat > "$T/work/p.lir" <<'EOF'
(define (main i32) ()
  (block entry (ret (i32 0))))
EOF
cat > "$T/bin/cc" <<'EOF'
#!/bin/sh
: > "$CC_LOG"
for a in "$@"; do printf '%s\n' "$a" >> "$CC_LOG"; done
case "$CC_MODE" in
  fail) echo "fake linker: undefined reference to \`nothing'" >&2; exit 3 ;;
  *) exit 0 ;;
esac
EOF
chmod +x "$T/bin/cc"

fail=0; n=0
# scenario NAME MODE PATH-KIND ARGS..: both tools run in $T/work with the same output path
scenario() {
  local name=$1 mode=$2 pathkind=$3; shift 3
  local tool path
  n=$((n + 1))
  for tool in rust fib; do
    local bin=$LAIR; [ $tool = fib ] && bin=$LAIRF
    path="$T/bin:$PATH"; [ "$pathkind" = nocc ] && path=/nonexistent
    ( cd "$T/work" && CC_LOG="$T/$tool.log" CC_MODE=$mode PATH=$path "$bin" build p.lir -o "$T/work/out" "$@" > "$T/$tool.so" 2> "$T/$tool.err"; echo $? > "$T/$tool.rc" )
    [ -f "$T/$tool.log" ] || : > "$T/$tool.log"
    [ "$pathkind" = nocc ] && rm -f "$T/$tool.log" && : > "$T/$tool.log"
    sed -i "s#$T#TMP#g" "$T/$tool.err" "$T/$tool.log"
    mv "$T/$tool.log" "$T/$tool.cmd"
  done
  if cmp -s "$T/rust.cmd" "$T/fib.cmd" && cmp -s "$T/rust.err" "$T/fib.err" && cmp -s "$T/rust.rc" "$T/fib.rc" && cmp -s "$T/rust.so" "$T/fib.so"; then
    echo "SAME $name (rc $(cat "$T/fib.rc"), $(wc -l < "$T/fib.cmd") cc arguments)"
  else
    fail=1; echo "DIFF $name"
    diff "$T/rust.cmd" "$T/fib.cmd" | head -n 8; diff "$T/rust.err" "$T/fib.err" | head -n 4; echo "rc: rust $(cat "$T/rust.rc") fib $(cat "$T/fib.rc")"
  fi
}
W=$T/work
scenario plain ok cc
scenario one-lib ok cc -l foo
scenario one-dir-abs ok cc -L "$W/b" -l foo
scenario one-dir-relative ok cc -L b -l foo
scenario dir-with-space ok cc -L "a dir" -l foo
scenario dir-through-symlink ok cc -L "$W/link-b" -l foo
scenario two-dirs-two-libs ok cc -L b -L "a dir" -l x -l y
scenario libs-order ok cc -l z -L b -l a -l m
scenario with-O3 ok cc -O 3 -L b -l q
scenario missing-dir ok cc -L "$W/nonexistent" -l foo
scenario dir-is-a-file ok cc -L "$W/afile" -l foo
scenario dir-with-colon ok cc -L "$W/a:b" -l foo
scenario dir-with-dollar ok cc -L "$W/d\$x" -l foo
scenario cc-fails fail cc -L b -l foo
scenario no-cc ok nocc -L b
if [ "$fail" = 0 ]; then echo "a-cc-args: all $n scenarios equal"; else echo "a-cc-args: DIFFERENCES"; fi
exit $fail
