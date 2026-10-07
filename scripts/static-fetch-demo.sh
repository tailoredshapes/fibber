#!/bin/bash
# The static resolver demo (docs/design/dns.md): builds examples/static-demo/fetch.fib with `fibc build --static`, puts it alone into a `FROM scratch` image (no libc, no
# /etc/nsswitch.conf, no CA store; docker adds /etc/resolv.conf and /etc/hosts to any container), and runs it with the host's /etc/resolv.conf bind-mounted read-only (on Lambda the platform writes one) and the host network
# (the resolv.conf of a desktop names a stub on 127.0.0.53). The program resolves and fetches http://NAME/ with the native resolver and client. The image is removed at the end.
#   FIBC=<fibc> FIB_MUSL_DIR=<dir of the musl pieces> [NAME=example.com] scripts/static-fetch-demo.sh
set -u
root=$(cd "$(dirname "$0")/.." && pwd); cd "$root"
F=${FIBC:-fibc}; export FIB_LIB=$root/lib
name=${NAME:-example.com}
T=$(mktemp -d "${TMPDIR:-/tmp}/static-fetch.XXXXXX"); trap 'docker image rm fibber-static-fetch:demo > /dev/null 2>&1; rm -rf "$T"' EXIT
"$F" build --static examples/static-demo/fetch.fib -o "$T/fetch" || exit 1
ls -l "$T/fetch" | awk '{print "binary:", $5, "bytes"}'
file "$T/fetch" 2>/dev/null | cut -d, -f1-2
printf 'FROM scratch\nCOPY fetch /fetch\nENTRYPOINT ["/fetch"]\n' > "$T/Dockerfile"
docker build -q -t fibber-static-fetch:demo "$T" > /dev/null || exit 1
docker image ls fibber-static-fetch:demo --format 'image: {{.Repository}}:{{.Tag}} {{.Size}}'
echo "--- the image holds only /fetch (no libc, no /etc/nsswitch.conf; docker itself adds /etc/resolv.conf and /etc/hosts to a running container), an unknown name:"
docker run --rm --network host --entrypoint /fetch fibber-static-fetch:demo "no-such-host.invalid" 2>&1 | head -3
echo "--- with the host's resolv.conf bind-mounted:"
docker run --rm --network host -v /etc/resolv.conf:/etc/resolv.conf:ro fibber-static-fetch:demo "$name"
