/* compiler/tests/shared/host.c: a C host for libkernels (built from kernels.fib by shared.sh with --shared --header). It includes the GENERATED header, so
 * a prototype that does not match the library is a compile error or a wrong result here. Its output lines equal those of oracle.fib's main under `fibc run`.
 * Besides, the string rules are checked: the retry rule, truncation, a null or empty buffer, an embedded NUL, and that a million calls do not grow the process. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "kernels.h"

static int failed = 0;
#define CHECK(cond, what) do { if (!(cond)) { fprintf(stderr, "CHECK FAILED: %s (%s:%d)\n", what, __FILE__, __LINE__); failed = 1; } } while (0)

static long rss_kb(void) {
  FILE *f = fopen("/proc/self/status", "r"); char line[256]; long kb = -1;
  if (!f) return -1;
  while (fgets(line, sizeof line, f)) if (sscanf(line, "VmRSS: %ld kB", &kb) == 1) break;
  fclose(f); return kb;
}

/* the retry rule: call with the buffer we have, grow to the length it returned, call again; the result is NUL-terminated here only for printing */
static char *retry_greet(const char *name, int64_t n, int64_t *len_out, int *calls) {
  int64_t cap = 4; char *buf = malloc(cap); *calls = 0;
  for (;;) {
    int64_t need = greet((const uint8_t *)name, n, (uint8_t *)buf, cap); (*calls)++;
    if (need <= cap) { *len_out = need; buf = realloc(buf, need + 1); buf[need] = 0; return buf; }
    cap = need; buf = realloc(buf, cap);
  }
}

static void print_str(const char *name, const uint8_t *p, int64_t n) { printf("%s=", name); fwrite(p, 1, (size_t)n, stdout); printf("\n"); }

int main(void) {
  uint8_t out[256];
  printf("add=%lld\n", (long long)add(40, 2));
  printf("widths=%lld\n", (long long)widths(100000, -5, 300));
  printf("narrow=%d\n", (int)narrow(7));
  printf("scale=%.1f\n", scale(2.5, 3.0));
  printf("half32=%.1f\n", (double)half32(5.0f));
  printf("both=%s,%s\n", both(1, 1) ? "true" : "false", both(1, 0) ? "true" : "false");
  { double xs[3] = {1, 2, 3}, ys[3] = {4, 5, 6}; CHECK(dot(xs, ys, 3) == 32.0, "dot"); }
  printf("sum_squares=%lld\n", (long long)sum_squares(1000));
  { int64_t v[5] = {9, 9, 9, 9, 9}; iota(v, 4); CHECK(v[0] == 0 && v[3] == 3 && v[4] == 9, "iota writes n elements and no more"); }

  int64_t n = greet((const uint8_t *)"world", 5, out, sizeof out); print_str("greet", out, n);
  n = join2((const uint8_t *)"left", 4, (const uint8_t *)"right", 5, out, sizeof out); print_str("join2", out, n);
  printf("count_bytes=%lld\n", (long long)count_bytes((const uint8_t *)"h\xc3\xa9llo", 6));
  n = banner(out, sizeof out); print_str("banner", out, n);
  n = echo((const uint8_t *)"same", 4, out, sizeof out); print_str("echo", out, n);
  n = mixed(7, (const uint8_t *)"seven", 5, 1.5, out, sizeof out); print_str("mixed", out, n);
  n = empty_str(out, sizeof out); printf("empty_str=["); fwrite(out, 1, (size_t)n, stdout); printf("]\n");
  CHECK(n == 0, "an empty str result returns 0");

  /* the length is what the string needs, however small the buffer; a buffer that is large enough gets the whole string and the length */
  n = greet((const uint8_t *)"world", 5, out, sizeof out); CHECK(n == 13, "greet needs 13 bytes");
  /* truncation: the first cap bytes are written, nothing after them, the return value is still the full length */
  memset(out, 0xAA, sizeof out);
  n = greet((const uint8_t *)"world", 5, out, 5);
  CHECK(n == 13, "truncated call returns the full length");
  CHECK(memcmp(out, "hello", 5) == 0, "truncated call writes the first cap bytes");
  CHECK(out[5] == 0xAA, "truncated call writes nothing past cap");
  /* cap 0, a null buffer and a negative cap write nothing and still return the length */
  memset(out, 0xAA, sizeof out);
  CHECK(greet((const uint8_t *)"world", 5, out, 0) == 13 && out[0] == 0xAA, "cap 0 writes nothing");
  CHECK(greet((const uint8_t *)"world", 5, NULL, 0) == 13, "a null buffer with cap 0 returns the length");
  CHECK(greet((const uint8_t *)"world", 5, NULL, 64) == 13, "a null buffer with any cap writes nothing");
  CHECK(greet((const uint8_t *)"world", 5, out, -3) == 13 && out[0] == 0xAA, "a negative cap is 0");
  /* exactly the needed size fits */
  memset(out, 0xAA, sizeof out);
  CHECK(greet((const uint8_t *)"world", 5, out, 13) == 13 && memcmp(out, "hello, world!", 13) == 0 && out[13] == 0xAA, "an exact fit");
  /* the retry rule */
  { int calls; int64_t len; char *s = retry_greet("retry", 5, &len, &calls);
    CHECK(len == 13 && strcmp(s, "hello, retry!") == 0 && calls == 2, "retry: first call too small, second with the returned length gets all"); free(s);
    s = retry_greet("a", 1, &len, &calls);
    CHECK(len == 9 && strcmp(s, "hello, a!") == 0 && calls == 2, "retry with a short string"); free(s); }
  /* a str parameter is (ptr, len): not NUL terminated, may hold NUL, may be empty with any pointer, a negative length is empty */
  CHECK(count_bytes((const uint8_t *)"abcXYZ", 3) == 3, "the length bounds the string");
  CHECK(count_bytes((const uint8_t *)"a\0b", 3) == 3, "an embedded NUL is a byte");
  CHECK(count_bytes(NULL, 0) == 0, "NULL with length 0 is the empty string");
  CHECK(count_bytes((const uint8_t *)"abc", -1) == 0, "a negative length is the empty string");
  n = greet((const uint8_t *)"abcXYZ", 3, out, sizeof out); CHECK(n == 13 - 2 && memcmp(out, "hello, abc!", 11) == 0, "greet takes only len bytes");
  /* the string in a result is a copy: the host may change the buffer and call again */
  { uint8_t a[16], b[16]; int64_t na = echo((const uint8_t *)"xyz", 3, a, sizeof a); int64_t nb = echo((const uint8_t *)"uvw", 3, b, sizeof b);
    CHECK(na == 3 && nb == 3 && memcmp(a, "xyz", 3) == 0 && memcmp(b, "uvw", 3) == 0, "results are independent copies"); }

  /* no leak and no double release: a large string in and out, many times, must not grow the process */
  { static char big[4096]; memset(big, 'q', sizeof big); static uint8_t res[8192];
    for (int i = 0; i < 20000; i++) { greet((const uint8_t *)big, sizeof big, res, sizeof res); echo((const uint8_t *)big, sizeof big, res, sizeof res); }
    long before = rss_kb();
    for (int i = 0; i < 200000; i++) {
      int64_t r1 = greet((const uint8_t *)big, sizeof big, res, sizeof res); int64_t r2 = echo((const uint8_t *)big, sizeof big, res, sizeof res);
      if (r1 != 4096 + 8 || r2 != 4096) { CHECK(0, "lengths in the loop"); break; } }
    long after = rss_kb();
    printf("# rss before %ld kB after %ld kB (400000 calls of 4 KB strings)\n", before, after);
    CHECK(before < 0 || after - before < 20000, "the process does not grow: strings are released"); }
  fflush(stdout);
  return failed;
}
