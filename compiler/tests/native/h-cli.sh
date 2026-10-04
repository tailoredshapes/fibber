#!/bin/bash
# The command line of lairf (native.cli) against `lair`: a list of invocations, each run by both tools, standard output, standard error and exit
# status compared (the unit tests of cli.rs's argument handling: every subcommand, every flag, every usage and option error).
# usage: h-cli.sh        run from the repository root, LAIR=target/debug/lair LAIRF=path/to/lairf; exit 0 when every invocation is the same
LAIR=${LAIR:-target/debug/lair}
[ -x "$LAIR" ] && [ -x "${LAIRF:-}" ] || { echo "h-cli: set LAIR and LAIRF to executables" >&2; exit 2; }
export FIB_TARGET_CPU=${FIB_TARGET_CPU:-x86-64-v2}
T=$(mktemp -d "${TMPDIR:-/tmp}/h-cli.XXXXXX")
G=compiler/tests/native/h-cases/sub-a
OK=$G/ok-exit.lir
BAD=compiler/tests/native/h-cases/sub-b/accept-no-main.lir
ARGSF=cases/lir/instr/calls.lir
n=0; bad=0
# one TOOL ARGS..: output, error, status in one file
one() { local tool=$1; shift; ( cd "$PWD"; "$tool" "$@" > "$T/so" 2> "$T/se"; echo "status $?" > "$T/st" ); cat "$T/so"; echo "--stderr"; cat "$T/se"; cat "$T/st"; }
t() {
  n=$((n+1))
  one "$LAIR" "$@" > "$T/a" 2>&1; one "$LAIRF" "$@" > "$T/b" 2>&1
  if cmp -s "$T/a" "$T/b"; then echo "SAME   $*"; else bad=$((bad+1)); echo "DIFF   $*"; diff "$T/a" "$T/b" | head -n 6; fi
}
t
t nonsense
t check
t check a b
t check "$OK"
t check "$BAD"
t check /nonexistent.lir
t emit-llvm
t emit-llvm "$OK"
t emit-llvm "$BAD"
t emit-llvm /nonexistent.lir
t dump-ast "$OK"
t dump-ast
t dump-ast a b
t dump-ast /nonexistent.lir
t run
t run "$OK"
t run -O 2 "$OK"
t run -O 9 "$OK"
t run -O x "$OK"
t run -O
t run -O 3
t run "$BAD"
t run /nonexistent.lir
t run "$ARGSF" one two
t fuzz-one "$OK"
t fuzz-one -O 1 "$OK"
t fuzz-one
t fuzz-one "$OK" extra
t build
t build "$OK"
t build "$OK" -o
t build "$OK" -o "$T/x" --bogus
t build "$OK" -o "$T/x" -O 7
t build "$OK" -o "$T/x" -O
t build "$OK" -o "$T/x" --emit
t build "$OK" -o "$T/x" --emit wasm
t build "$OK" -o "$T/x" -L
t build "$OK" -o "$T/x" -l
t build "$OK" -o "$T/x" -L /nonexistent
t build "$OK" -o "$T/x" -l nonexistentlib
t build "$BAD" -o "$T/x"
t build "$BAD" -o "$T/x" --emit llvm
t build "$OK" -o "$T/x" --emit llvm -O 2
t build /nonexistent.lir -o "$T/x"
t cases
t cases /nonexistent
t cases compiler/tests/native/h-cases/sub-a
# build outputs: the same bytes for the object, assembly and LLVM text
for k in obj asm llvm; do
  for o in 0 2; do
    n=$((n+1))
    "$LAIR" build "$OK" -o "$T/r.$k" --emit $k -O $o > /dev/null 2>&1; "$LAIRF" build "$OK" -o "$T/f.$k" --emit $k -O $o > /dev/null 2>&1
    if cmp -s "$T/r.$k" "$T/f.$k"; then echo "SAME   build --emit $k -O $o"; else bad=$((bad+1)); echo "DIFF   build --emit $k -O $o"; fi
  done
done
# an executable: built with each tool, run, same status and output
n=$((n+1))
"$LAIR" build "$ARGSF" -o "$T/re" > /dev/null 2>&1; "$LAIRF" build "$ARGSF" -o "$T/fe" > /dev/null 2>&1
"$T/re" > "$T/re.out" 2>&1; rs=$?; "$T/fe" > "$T/fe.out" 2>&1; fs=$?
if [ "$rs" = "$fs" ] && cmp -s "$T/re.out" "$T/fe.out"; then echo "SAME   build executable (status $rs)"; else bad=$((bad+1)); echo "DIFF   build executable ($rs vs $fs)"; fi
rm -rf "$T"
echo "h-cli: $((n-bad)) same, $bad different (of $n)"
[ "$bad" -eq 0 ]
