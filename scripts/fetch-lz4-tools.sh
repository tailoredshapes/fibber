#!/bin/bash
# scripts/fetch-lz4-tools.sh: the second oracle and the benchmark corpus for fib.compress.lz4 (docs/design/compress.md 9, ADR 0020: a download has a recorded checksum and is verified).
# Fetches into DIR (default ~/.cache/fibber-scratch/tools/lz4): the lz4 v1.10.0 source release (builds the `lz4` CLI and liblz4 with gcc -O3, plus a small benchmark
# driver `lz4bench` over LZ4_compress_default / LZ4_compress_HC / LZ4_decompress_safe) and the Silesia corpus (silesia.zip, unpacked to DIR/silesia/). Nothing is installed.
set -euo pipefail
DIR=${1:-$HOME/.cache/fibber-scratch/tools/lz4}
LZ4_URL=https://github.com/lz4/lz4/archive/refs/tags/v1.10.0.tar.gz
LZ4_SHA=537512904744b35e232912055ccf8ec66d768639ff3abe5788d90d792ec5f48b
SIL_URL=http://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip
SIL_SHA=0626e25f45c0ffb5dc801f13b7c82a3b75743ba07e3a71835a41e3d9f63c77af
mkdir -p "$DIR"; cd "$DIR"
fetch() { # fetch URL FILE SHA
  [ -f "$2" ] || curl -sfL -m 600 -o "$2" "$1"
  echo "$3  $2" | sha256sum -c - >/dev/null || { echo "fetch-lz4-tools: checksum of $2 differs from the recorded one" >&2; rm -f "$2"; exit 1; }
}
fetch "$LZ4_URL" lz4-1.10.0.tar.gz "$LZ4_SHA"
fetch "$SIL_URL" silesia.zip "$SIL_SHA"
[ -d lz4-1.10.0 ] || tar xzf lz4-1.10.0.tar.gz
[ -x lz4-1.10.0/programs/lz4 ] || make -C lz4-1.10.0 -j4 CFLAGS=-O3 lib lz4 >/dev/null
[ -d silesia ] || { mkdir silesia; python3 -c "import zipfile; zipfile.ZipFile(\"silesia.zip\").extractall(\"silesia\")"; }
cat > lz4bench.c <<'EOF'
/* lz4bench MODE FILE [LEVEL]: MODE c (fast, acceleration LEVEL), h (HC, level LEVEL), d (decompress): best of 5 passes over the whole file in 64 KiB..whole blocks as one block. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "lz4.h"
#include "lz4hc.h"
static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec + t.tv_nsec * 1e-9; }
int main(int argc, char **argv) {
  FILE *f = fopen(argv[2], "rb"); fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
  char *src = malloc(n), *dst = malloc(LZ4_compressBound(n)), *back = malloc(n); fread(src, 1, n, f); fclose(f);
  int level = argc > 3 ? atoi(argv[3]) : 1; int csize = 0; double ts[64]; int nt = 0;
  for (int r = 0; r < 5; r++) {
    double t0 = now();
    if (argv[1][0] == 'c') csize = LZ4_compress_fast(src, dst, n, LZ4_compressBound(n), level);
    else if (argv[1][0] == 'h') csize = LZ4_compress_HC(src, dst, n, LZ4_compressBound(n), level);
    else { if (r == 0) csize = LZ4_compress_default(src, dst, n, LZ4_compressBound(n)); t0 = now(); if (LZ4_decompress_safe(dst, back, csize, n) != n) return 1; }
    ts[nt++] = now() - t0;
  }
  for (int i = 0; i < nt; i++) for (int j = i + 1; j < nt; j++) if (ts[j] < ts[i]) { double x = ts[i]; ts[i] = ts[j]; ts[j] = x; }
  double best = ts[nt / 2]; printf("%s %s level %d: %ld -> %d (%.2f%%) median %.3f s = %.0f MB/s\n", argv[1], argv[2], level, n, csize, 100.0 * csize / n, best, n / best / 1e6);
  return 0;
}
EOF
gcc -O3 -Ilz4-1.10.0/lib -o lz4bench lz4bench.c lz4-1.10.0/lib/liblz4.a
echo "lz4: $DIR/lz4-1.10.0/programs/lz4   bench: $DIR/lz4bench   corpus: $DIR/silesia"
