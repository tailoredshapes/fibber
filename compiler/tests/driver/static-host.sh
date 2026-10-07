#!/bin/bash
# compiler/tests/driver/static-host.sh: `fibc build --static` for a target that is not Linux says so. It used to say `FIB_TARGET_TRIPLE is set: an executable cannot
# be linked for another target here ...`, blaming a variable that only `--static` had set (driver.static leaves the host's triple in it for a non-Linux target; a Mac
# user who wrote `fibc build --static x.fib` read it as their own mistake). No musl pieces are needed: the refusal comes before the link.
#   a --static build for a Darwin target: refused, naming the Linux-only rule and the target, and not blaming FIB_TARGET_TRIPLE
#   the same through FIB_STATIC=1 and FIB_TARGET_TRIPLE
#   a build for the Darwin target without --static: still the old refusal (that one is about FIB_TARGET_TRIPLE)
#   the message for the host itself (a Mac): `this host is darwin-arm64`, from the function the refusal calls (this host is Linux, so it is called directly)
#   --emit obj for the Darwin target with --static is no executable and is not refused
# usage: static-host.sh STAGE2        (run from the repository root; FIB_LIB defaults to the tree's lib)
set -uo pipefail
s2=${1:?usage: static-host.sh STAGE2}
root=$PWD
export FIB_LIB=${FIB_LIB:-$root/lib}
ulimit -v 16000000
t=$(mktemp -d "$HOME/.cache/fibber-scratch/static-host.XXXXXX"); trap 'rm -rf "$t"' EXIT
fail=0
ck() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got [$2] want [$3]"; fail=1; fi; }
has() { if grep -qF -- "$3" "$2"; then echo "ok    $1"; else echo "FAIL  $1: no [$3] in: $(head -c 300 "$2")"; fail=1; fi; }
hasnt() { if grep -qF -- "$3" "$2"; then echo "FAIL  $1: found [$3]"; fail=1; else echo "ok    $1"; fi; }
echo '(defun main () -> i64 7)' > "$t/seven.fib"
unset FIB_STATIC FIB_TARGET_TRIPLE

"$s2" build --static --target aarch64-apple-darwin "$t/seven.fib" -o "$t/x" > "$t/o1" 2>&1; ck "--static --target darwin: refused (exit 5)" $? 5
has "--static --target darwin: Linux-only, and the target" "$t/o1" "--static builds are Linux-only (musl): the target is aarch64-apple-darwin"
hasnt "--static --target darwin: does not blame FIB_TARGET_TRIPLE" "$t/o1" "FIB_TARGET_TRIPLE is set"
FIB_STATIC=1 FIB_TARGET_TRIPLE=arm64-apple-darwin "$s2" build "$t/seven.fib" -o "$t/x" > "$t/o2" 2>&1; ck "FIB_STATIC=1 with a darwin triple: refused (exit 5)" $? 5
has "FIB_STATIC=1 with a darwin triple: Linux-only, and the target" "$t/o2" "--static builds are Linux-only (musl): the target is arm64-apple-darwin"
FIB_TARGET_TRIPLE=aarch64-apple-darwin "$s2" build "$t/seven.fib" -o "$t/x" > "$t/o3" 2>&1; ck "a darwin target without --static: refused (exit 5)" $? 5
has "a darwin target without --static: the old message, about FIB_TARGET_TRIPLE" "$t/o3" "FIB_TARGET_TRIPLE is set"
hasnt "a darwin target without --static: no word of --static" "$t/o3" "Linux-only"
"$s2" build --static --target aarch64-apple-darwin "$t/seven.fib" --emit obj -o "$t/x.o" > "$t/o4" 2>&1; ck "--static --target darwin --emit obj: not an executable, not refused" $? 0

cat > "$t/msg.fib" <<'EOF'
(ns main (:use native.target))
(defun main () -> i64
  (do (println (static-refusal "arm64-apple-darwin25.0.0" "arm64-apple-darwin25.0.0"))
      (println (static-refusal "aarch64-apple-darwin" "x86_64-pc-linux-gnu"))
      0))
EOF
if "$s2" build "$t/msg.fib" -I "$root/compiler" -I "$root/lib" -o "$t/msg" > "$t/b" 2>&1; then
  "$t/msg" > "$t/o5"
  ck "the host message" "$(sed -n 1p "$t/o5")" "--static builds are Linux-only (musl): this host is darwin-arm64"
  ck "the target message" "$(sed -n 2p "$t/o5")" "--static builds are Linux-only (musl): the target is aarch64-apple-darwin"
else echo "FAIL  the message program did not build: $(head -c 300 "$t/b")"; fail=1; fi
echo "static-host: $([ $fail = 0 ] && echo ok || echo FAILED)"
exit $fail
