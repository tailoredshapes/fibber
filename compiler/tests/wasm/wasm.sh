#!/bin/bash
# compiler/tests/wasm/wasm.sh: the checks of the wasm32-wasi build that a case of the shared suites cannot make (docs/design/wasm.md 7): the cases run on wasm through
# suite.py, and these run only here because their answer depends on the target's 32-bit memory or on node's stack.
#   compiler/tests/wasm/wasm.sh FIBC [--quick]       FIBC: a stage 2.   Needs node; WASI_SDK (or WASM_LD and WASI_SYSROOT) names the wasm toolchain.
# Prints `ok NAME` or `FAIL NAME (why)` per check, then the count. Exit 0 all held, 1 one failed, 77 (skipped, like automake) when node or the toolchain is not there.
#   hello          a program builds, links with wasm-ld against wasi-libc and runs under node:wasi: stdout, exit status
#   grow           a 768 MiB block is allocated and touched: the memory grows (memory.grow through wasi-libc's malloc) up to the limit the link gave
#   oversize       a request of 2^33 + 4096 bytes is out of memory, not its low 32 bits (4096) made to look like success
#   write          a 200000-byte write reaches standard output whole (the length of a 32-bit `write` is the whole length, not a part)
#   stack          a non-tail recursion that outruns node's stack ends with `trap: stack overflow` and status 134 (run.mjs maps V8's RangeError; docs/design/stack.md)
#   tail           ten million tail calls between functions of different signatures run in node's default stack with the tail-call extension, and overflow without it
#   movemask       the lanes of a 32- and a 64-lane mask are in order on wasm (LLVM's WebAssembly backend gets `bitcast <32 x i1>` wrong)
#   vec-of-objects a vector, an array and a map of objects are read at the element size of the 32-bit target
#   simd128        the module has v128 instructions when simd128 is on, and none when it is off
#   export         `--export` makes a library a JavaScript host calls with typed arrays (examples/wasm)
set -u
fibc=${1:?usage: wasm.sh FIBC [--quick]}
quick=${2:-}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
command -v node >/dev/null || { echo "skip: node is not installed"; exit 77; }
if [ -z "${WASI_SDK:-}" ]; then for d in "$HOME"/.cache/fibber-scratch/tools/wasm/wasi-sdk-*; do [ -d "$d" ] && WASI_SDK=$d; done; export WASI_SDK; fi
if [ -z "${WASI_SDK:-}" ] && ! command -v wasm-ld >/dev/null && [ -z "${WASM_LD:-}" ]; then echo "skip: no wasm-ld (set WASI_SDK, or WASM_LD and WASI_SYSROOT: docs/design/wasm.md 3)"; exit 77; fi
tmp=$(mktemp -d "$HOME/.cache/fibber-scratch/wasm-test-XXXXXX")
trap 'rm -rf "$tmp"' EXIT
ulimit -v 16000000
bad=0; n=0
ok() { n=$((n+1)); echo "ok $1"; }
fail() { n=$((n+1)); bad=$((bad+1)); echo "FAIL $1 ($2)"; }
build() { # SRC OUT [extra..]: the result of main on standard output (FIB_RESULT_STDOUT), as `fibc run` prints it; `buildx` returns it as the status
  local src=$1 out=$2; shift 2
  FIB_RESULT_STDOUT=1 "$fibc" build "$src" -o "$out" --target wasm32-wasi -I "$root/lib" "$@" 2> "$tmp/build.err"
}
buildx() { local src=$1 out=$2; shift 2; "$fibc" build "$src" -o "$out" --target wasm32-wasi -I "$root/lib" "$@" 2> "$tmp/build.err"; }
run() { node --no-warnings "$here/run.mjs" "$@"; }

cat > "$tmp/hello.fib" <<'EOF'
(defun main () -> i64 (do (println "hello wasm") 42))
EOF
if buildx "$tmp/hello.fib" "$tmp/hello.wasm"; then
  out=$(run "$tmp/hello.wasm" 2>&1); rc=$?
  [ "$out" = "hello wasm" ] && ok hello-stdout || fail hello-stdout "got: $out"
  [ $rc -eq 42 ] && ok hello-status || fail hello-status "status $rc"
else fail hello "build: $(head -1 "$tmp/build.err")"; fi

cat > "$tmp/grow.fib" <<'EOF'
(extern write (i32 ptr i64) -> i64)
(defun main () -> i64
  (unsafe
    (let ((p (alloc 805306368)))
      (do (store-i8 (ptr+ p 805306367) 7i8)
          (write 3i32 p 0)
          (let ((r (sext i64 (load-i8 (ptr+ p 805306367)))))
            (do (free p) r))))))
EOF
if build "$tmp/grow.fib" "$tmp/grow.wasm"; then
  [ "$(run "$tmp/grow.wasm" 2>&1 | tail -1)" = 7 ] && ok grow || fail grow "a 768 MiB block was not usable"
else fail grow "build: $(head -1 "$tmp/build.err")"; fi

cat > "$tmp/over.fib" <<'EOF'
(extern write (i32 ptr i64) -> i64)
(defun main () -> i64
  (unsafe
    (let ((p (alloc 8589938688)))
      (do (store-i8 p 1i8) (write 3i32 p 0) 7))))
EOF
if build "$tmp/over.fib" "$tmp/over.wasm"; then
  out=$(run "$tmp/over.wasm" 2>&1); rc=$?
  { [ $rc -ne 0 ] && echo "$out" | grep -q "trap: out of memory"; } && ok oversize || fail oversize "status $rc: $(echo "$out" | tail -1)"
else fail oversize "build: $(head -1 "$tmp/build.err")"; fi

cat > "$tmp/write.fib" <<'EOF'
(defun main () -> i64
  (let ((s (str-join (mapv (fn (i: i64) "0123456789") (range 20000)))))
    (do (print s) (count s))))
EOF
if buildx "$tmp/write.fib" "$tmp/write.wasm"; then
  got=$(run "$tmp/write.wasm" 2>/dev/null | wc -c)
  [ "$got" -eq 200000 ] && ok write || fail write "$got bytes of 200000"
else fail write "build: $(head -1 "$tmp/build.err")"; fi

cat > "$tmp/deep.fib" <<'EOF'
(defun climb (n: i64) -> i64 (if (= n 0) 0 (- (* 2 n) (climb (- n 1)))))
(defun main () -> i64 (climb 1000000000))
EOF
if buildx "$tmp/deep.fib" "$tmp/deep.wasm"; then
  out=$(run "$tmp/deep.wasm" 2>&1); rc=$?
  { [ "$rc" = 134 ] && echo "$out" | grep -q "^trap: stack overflow"; } && ok stack || fail stack "status $rc: $(echo "$out" | head -c 160)"
else fail stack "build: $(head -1 "$tmp/build.err")"; fi

[ "$quick" = --quick ] || {
C=$root/cases/stdlib/8250-mutual-tail-calls-of-different-shapes-do-not-grow-the-stack.fib
if build "$C" "$tmp/tail.wasm" -O 0 -I "$root/cases/stdlib/support"; then
  [ "$(run "$tmp/tail.wasm" 2>&1 | tail -1)" = 10000007 ] && ok tail || fail tail "ten million tail calls did not finish in node's default stack"
else fail tail "build: $(head -1 "$tmp/build.err")"; fi
if FIB_TARGET_FEATURES=+simd128,-tail-call build "$C" "$tmp/tailoff.wasm" -O 0 -I "$root/cases/stdlib/support"; then
  run "$tmp/tailoff.wasm" 2>&1 | grep -q "Maximum call stack size exceeded" && ok tail-control || fail tail-control "without the tail-call extension the recursion did not overflow: the check proves nothing"
else fail tail-control "build: $(head -1 "$tmp/build.err")"; fi
}

M=$root/cases/stdlib/7980-movemask-puts-lane-zero-in-bit-zero-at-every-lane-count.fib
if build "$M" "$tmp/mm.wasm" -I "$root/cases/stdlib/support"; then
  [ "$(run "$tmp/mm.wasm" 2>&1 | tail -1)" = 0 ] && ok movemask || fail movemask "a mask came out in the wrong lane order"
else fail movemask "build: $(head -1 "$tmp/build.err")"; fi

python3 "$here/suite.py" --fibc "$fibc" "$root/cases/stdlib" --only 8251 8252 -j 2 > "$tmp/vo.out" 2>&1
tail -1 "$tmp/vo.out" | grep -q " 0 fail" && ok vec-of-objects || fail vec-of-objects "$(tail -3 "$tmp/vo.out" | head -2 | tr '\n' ' ')"

S=$root/cases/stdlib/6200-*.fib
SIMD=$(ls $S | head -1)
if build "$SIMD" "$tmp/simd.wasm" -I "$root/cases/stdlib/support" --emit asm; then
  grep -q "v128\|f64x2\|i32x4\|f32x4" "$tmp/simd.wasm" && ok simd128-on || fail simd128-on "no v128 instruction in the assembly"
else fail simd128-on "build: $(head -1 "$tmp/build.err")"; fi
if FIB_TARGET_FEATURES=+tail-call,-simd128 build "$SIMD" "$tmp/nosimd.wasm" -I "$root/cases/stdlib/support" --emit asm; then
  grep -q "v128\|f64x2\|i32x4\|f32x4" "$tmp/nosimd.wasm" && fail simd128-off "v128 instructions with simd128 off" || ok simd128-off
else fail simd128-off "build: $(head -1 "$tmp/build.err")"; fi

if "$fibc" build "$root/examples/wasm/kernels.fib" -o "$tmp/k.wasm" --target wasm32-wasi --export add --export dot --export sum-squares=sumSquares --export fib-vec-length=vecLength 2> "$tmp/build.err"; then
  out=$(node --no-warnings "$root/examples/wasm/kernels.mjs" "$tmp/k.wasm" 2>&1 | tr '\n' ' ')
  [ "$out" = "add(40, 2) = 42n dot = 300 sumSquares(1000) = 332833500n vecLength(5) = 5n " ] && ok export || fail export "$out"
else fail export "build: $(head -1 "$tmp/build.err")"; fi

echo "$n checks, $bad failed"
[ $bad -eq 0 ]
