#!/bin/bash
# The command line of the compiler in fibber (compiler/fibc.fib, built by the Rust fibc), as a program: exit statuses, what is printed on
# standard output and error, `-I` and FIB_LIB, `run` with arguments, `build` with `-L` and `-l`, `emit-dump` and `explain` with roots.
# usage: cli.sh STAGE2 [STAGE1]    STAGE1 (the Rust fibc) is only to compare `explain` and `emit` with, when given.
# Run from the repository root with LD_LIBRARY_PATH holding liblair.so (target/debug) in the environment.
# Prints `ok NAME` or `FAIL NAME` for each check; exit status 0 only if none failed. Scratch files go under $HOME/.cache/fibber-scratch/.
s2=$1; s1=$2; fail=0; root=$PWD
unset FIB_LIB   # the roots of a check are its own; the library is `lib` of the working directory (the repository root)
t=$(mktemp -d "$HOME/.cache/fibber-scratch/cli.XXXXXX"); trap 'rm -rf "$t"' EXIT
ck() { if [ "$2" = "$3" ]; then echo "ok $1"; else echo "FAIL $1: got [$2] want [$3]"; fail=1; fi; }
mkdir -p "$t/a/geo" "$t/b" "$t/prog"
cp cases/modules/001-require-alias-and-use/util.fib "$t/b/"; cp cases/modules/001-require-alias-and-use/geo/point.fib "$t/a/geo/"
cp cases/modules/001-require-alias-and-use/main.fib "$t/prog/"
echo '(defun main () -> i64 (+ 1 2))' > "$t/three.fib"
echo '(defun main () -> i64 7)' > "$t/seven.fib"
echo '(defun main () -> i64 "s")' > "$t/bad.fib"
echo '(defun main () -> i64 (do (println (str-join (vec (args)))) 0))' > "$t/args.fib"
echo '(extern lair_hook1_address () -> i64)
(defun main () -> i64 (if (= (unsafe (lair_hook1_address)) 0) 1 0))' > "$t/lair.fib"

ck "help prints the usage and exits 0" "$($s2 help | head -1; echo $?)" "$(printf 'usage: fibc <command>\n0')"
ck "--version prints the version and exits 0" "$($s2 --version; echo $?)" "$(printf 'fibc %s\n0' "$(cat VERSION)")"
ck "-V is --version" "$($s2 -V)" "fibc $(cat VERSION)"
$s2 --version x > /dev/null 2>&1; ck "--version with a word after it: exit 2" $? 2
$s2 > /dev/null 2>&1; ck "no command: exit 2" $? 2
$s2 bogus > /dev/null 2>&1; ck "an unknown command: exit 2" $? 2
ck "cases of a missing directory: exit 2" "$($s2 cases "$t/nodir" 2>&1; echo $?)" "$(printf 'fibc: cannot read cases in %s/nodir: No such file or directory (os error 2)\n2' "$t")"
ck "gen is the Rust compiler's" "$($s2 gen 2>&1; echo $?)" "$(printf 'fibc: `gen` is a command of the Rust compiler (the test harness)\n2')"
$s2 -I x help > /dev/null 2>&1; ck "-I with a command that reads no program: exit 2" $? 2
ck "an unreadable file: the operating system's words, exit 2" "$($s2 emit "$t/nonexist.fib" 2>&1; echo $?)" "$(printf 'fibc: cannot read %s/nonexist.fib: No such file or directory (os error 2)\n2' "$t")"
ck "a directory is not a program" "$($s2 emit "$t" 2>&1; echo $?)" "$(printf 'fibc: cannot read %s: Is a directory (os error 21)\n2' "$t")"
ck "run prints main's result and exits 0" "$($s2 run "$t/three.fib"; echo $?)" "$(printf '3\n0')"
ck "run at -O 2, with --trace in front" "$($s2 run --trace -O 2 "$t/three.fib" 2>/dev/null | tail -1; echo $?)" "$(printf '3\n0')"
ck "run passes the words after -- as (args)" "$($s2 run "$t/args.fib" -- a 'b c' -I x)" "$(printf 'ab c-Ix\n0')"
ck "a rejected program: the words, exit 3" "$($s2 run "$t/bad.fib" 2>&1 | head -1; $s2 run "$t/bad.fib" >/dev/null 2>&1; echo $?)" "$(printf 'rejected:\n3')"
ck "-I after the file, glued or apart, finds the modules" "$($s2 run "$t/prog/main.fib" -I "$t/a" -Ib 2>&1 | head -1)" "rejected:"
ck "-I finds the modules (apart, glued, in any place before --)" "$($s2 run -I"$t/a" "$t/prog/main.fib" -I "$t/b")" "17"
ck "FIB_LIB finds the modules" "$(FIB_LIB="$t/a:$t/b" $s2 run "$t/prog/main.fib")" "17"
ck "-I and FIB_LIB together" "$(FIB_LIB="$t/b" $s2 run -I "$t/a" "$t/prog/main.fib")" "17"
ck "no root: the module is not found, and why" "$($s2 emit "$t/prog/main.fib" 2>&1 | head -2 | tr '\n' ' ')" "rejected: module geo.point is not at $t/prog/geo/point.fib: No such file or directory (os error 2) "
ck "roots tried are named: beside the file, then each root" "$($s2 emit -I "$t/b" "$t/prog/main.fib" 2>&1 | sed -n 2p)" "module geo.point is not at $t/prog/geo/point.fib: No such file or directory (os error 2); nor at $t/b/geo/point.fib"
ck "emit-dump reads the roots" "$($s2 emit-dump --sections main -I "$t/a" -I "$t/b" "$t/prog/main.fib" | sed -n 2p; echo $?)" "$(printf ';; == section main\n0')"
ck "emit-dump --layout reads the roots" "$($s2 emit-dump --layout -I "$t/a" -I "$t/b" "$t/prog/main.fib" | sed -n 3p | cut -c1-14)" "bool mangle bo"
ck "emit into a pipe that is closed is not death by SIGPIPE: exit 0, as the Rust's" "$($s2 emit "$t/three.fib" | true; echo ${PIPESTATUS[0]})" 0
ck "emit into a pipe that is closed: nothing on standard error" "$($s2 emit "$t/three.fib" 2>"$t/pipe.err" | true; cat "$t/pipe.err")" ""
ck "explain reads the roots" "$($s2 explain -I "$t/a" -I "$t/b" "$t/prog/main.fib" | head -1 | cut -c1-5)" "defun"
$s2 build "$t/seven.fib" -o "$t/seven" > /dev/null 2>&1; ck "build makes an executable whose status is main's" "$("$t/seven" > /dev/null; echo $?)" 7
$s2 build "$t/three.fib" -o "$t/three" -O 0 > /dev/null 2>&1; ck "build -O 0" "$("$t/three"; echo $?)" 3
ck "build refuses a rejected program, writes nothing" "$($s2 build "$t/bad.fib" -o "$t/never" >/dev/null 2>&1; echo $?; ls "$t/never" 2>&1 | head -c 0)" 3
ck "build: -o is required" "$($s2 build "$t/three.fib" > /dev/null 2>&1; echo $?)" 2
ck "build: a -L that is not a directory: exit 2, nothing built" "$($s2 build "$t/three.fib" -o "$t/x" -L "$t/nonexist" 2>&1; echo $?; test -e "$t/x" && echo built)" "$(printf 'fibc: -L %s/nonexist: not a directory that can be read\n2' "$t")"
ck "build without -l lair cannot link a program that calls lair" "$($s2 build "$t/lair.fib" -o "$t/lair" > /dev/null 2>&1; echo $?)" 5
lib=target/debug   # where liblair.so is: the first directory of LD_LIBRARY_PATH that has it, else target/debug
for d in ${LD_LIBRARY_PATH//:/ }; do if [ -e "$d/liblair.so" ]; then lib=$d; break; fi; done
ck "build -L DIR -l lair links it" "$($s2 build "$t/lair.fib" -o "$t/lair" -L "$lib" -l lair > /dev/null 2>&1; echo $?; "$t/lair"; echo $?)" "$(printf '0\n0')"
# The rpath: each -L is also an absolute rpath (as the Rust compiler's is), so the executable runs from another directory with no LD_LIBRARY_PATH,
# and only because of it: with the library moved away it does not start.
mkdir -p "$t/rp"; cp "$lib/liblair.so" "$t/rp/"
$s2 build "$t/lair.fib" -o "$t/lair-rpath" -L "$t/rp" -l lair > /dev/null 2>&1
ck "build -L DIR writes an rpath: the executable runs from /, with no LD_LIBRARY_PATH" "$(cd /; env -u LD_LIBRARY_PATH "$t/lair-rpath" 2>&1; echo $?)" 0
mv "$t/rp" "$t/rp-moved"
ck "build -L DIR: the rpath is what finds the library (moved away, it does not start)" "$(cd /; env -u LD_LIBRARY_PATH "$t/lair-rpath" 2>&1 | grep -c 'liblair.so: cannot open')" 1
ck "build -L DIR: a relative directory is an absolute rpath" "$($s2 build "$t/lair.fib" -o "$t/lair-rel" -L "$(realpath --relative-to="$root" "$t/rp-moved")" -l lair > /dev/null 2>&1; cd /; env -u LD_LIBRARY_PATH "$t/lair-rel" 2>&1; echo $?)" 0
mkdir -p "$t/jc"; cp cases/ownership/01-return-part-of-argument.fib "$t/jc/"
ck "cases DIR -j 2 runs and counts as without it, exit 0" "$($s2 cases "$t/jc" -j 2 | tail -1; echo $?)" "$(printf '1 cases: 1 pass, 0 fail, 0 pending, 0 header error\n0')"
ck "cases -j 0: usage on standard error, exit 2" "$($s2 cases "$t/jc" -j 0 2>&1 >/dev/null | head -1; $s2 cases "$t/jc" -j 0 >/dev/null 2>&1; echo $?)" "$(printf 'usage: fibc <command>\n2')"
if [ -n "$s1" ]; then
  for c in emit explain; do
    cmp -s <($s1 $c "$t/prog/main.fib" -I "$t/a" -I "$t/b" 2>&1) <($s2 $c "$t/prog/main.fib" -I "$t/a" -I "$t/b" 2>&1); ck "$c equals the Rust compiler's, with roots" $? 0
  done
fi
exit $fail
