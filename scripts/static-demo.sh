#!/bin/bash
# scripts/static-demo.sh: the static-binary demo (docs/design/static-linking.md 5): builds examples/static-demo with `fibc build --static` for x86-64 and aarch64,
# puts each into a `FROM scratch` Docker image (tag fibber-static-demo:*, removed at the end), runs the batch mode and the HTTP server in them, runs the aarch64
# image under `docker run --platform linux/arm64` (binfmt/qemu-user) when that works, and proves a Lambda custom-runtime `bootstrap` against a local fake of the
# Runtime API (examples/static-demo/fake-runtime-api.py: python3, no AWS).
# usage: static-demo.sh   FIBC=fibc (a stage 2)  FIB_MUSL_DIR=dir of the musl pieces (scripts/build-musl.sh)  Exit 0 when every step held, 1 otherwise, 2 no tools.
set -u
root=$(cd "$(dirname "$0")/.." && pwd); cd "$root"
F=${FIBC:-fibc}; export FIB_LIB=$root/lib
command -v docker > /dev/null && command -v python3 > /dev/null || { echo "static-demo: needs docker and python3" >&2; exit 2; }
T=$(mktemp -d "${TMPDIR:-/tmp}/static-demo.XXXXXX"); bad=0
cleanup() { docker rm -f fibber-static-demo-srv > /dev/null 2>&1; docker image rm fibber-static-demo:amd64 fibber-static-demo:arm64 > /dev/null 2>&1; rm -rf "$T"; }
trap cleanup EXIT
step() { echo "== $*"; }
fail() { echo "FAIL $*"; bad=1; }
echo '{"name":"fibber","tags":["static","musl"],"n":3.5}' > "$T/data.json"
printf '[{"name":"ada","numbers":[1,2,3,4]},{"name":"grace","numbers":[10,20]},{"numbers":[]}]\n' > "$T/events.json"
printf 'FROM scratch\nCOPY demo /demo\nCOPY data.json /data.json\nENTRYPOINT ["/demo"]\nCMD ["batch", "/data.json"]\n' > "$T/Dockerfile"
for arch in amd64 arm64; do
  triple=x86_64-unknown-linux-gnu; [ $arch = arm64 ] && triple=aarch64-unknown-linux-gnu
  step "build --static for $arch"; mkdir -p "$T/$arch"; cp "$T/data.json" "$T/Dockerfile" "$T/$arch/"
  "$F" build --static --target $triple examples/static-demo/demo.fib -o "$T/$arch/demo" || { fail "build $arch"; continue; }
  "$F" build --static --target $triple examples/static-demo/bootstrap.fib -o "$T/$arch/bootstrap" || fail "build bootstrap $arch"
  ls -l "$T/$arch/demo" "$T/$arch/bootstrap" | awk '{print $5, $9}'
  (docker build -q --platform linux/$arch -t fibber-static-demo:$arch "$T/$arch" > /dev/null) || { fail "docker build $arch"; continue; }
  docker image ls fibber-static-demo:$arch --format '{{.Repository}}:{{.Tag}} {{.Size}}'
  step "run the $arch image (FROM scratch)"
  out=$(docker run --rm --platform linux/$arch fibber-static-demo:$arch 2>&1); echo "$out"
  echo "$out" | grep -q "sum of squares below 1000000 over 4 tasks: 333332833333500000" || fail "batch $arch"
  step "Lambda bootstrap $arch against the fake Runtime API (host network, scratch image)"
  mkdir -p "$T/$arch/l"; cp "$T/$arch/bootstrap" "$T/$arch/l/bootstrap"
  printf 'FROM scratch\nCOPY bootstrap /var/runtime/bootstrap\nENTRYPOINT ["/var/runtime/bootstrap"]\n' > "$T/$arch/l/Dockerfile"
  docker build -q --platform linux/$arch -t fibber-static-demo:$arch-lambda "$T/$arch/l" > /dev/null
  python3 examples/static-demo/fake-runtime-api.py 19001 "$T/events.json" > "$T/fake-$arch.log" 2>&1 &
  sleep 1
  out=$(docker run --rm --network host --platform linux/$arch -e AWS_LAMBDA_RUNTIME_API=127.0.0.1:19001 fibber-static-demo:$arch-lambda 2>&1); echo "$out"
  wait; [ $? -eq 0 ] && cat "$T/fake-$arch.log" || fail "fake runtime API saw no complete run ($arch)"
  docker image rm fibber-static-demo:$arch-lambda > /dev/null 2>&1
done
step "the HTTP server in the amd64 scratch image"
docker run -d --rm --name fibber-static-demo-srv -p 18080:8080 fibber-static-demo:amd64 serve 8080 > /dev/null
sleep 1; curl -s -i localhost:18080/json | tr -d '\r'; curl -s localhost:18080/ | grep -q "static fibber binary" || fail "server"
[ $bad -eq 0 ] && echo "static-demo: every step held" || echo "static-demo: FAILED"; exit $bad
