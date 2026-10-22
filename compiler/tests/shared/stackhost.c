/* compiler/tests/shared/stackhost.c: the host of stackcheck.sh (docs/design/stack-check.md 6). Calls the library's recursions past the stack of the thread it is on and
 * checks that the call returns (status 1, message "stack overflow") instead of killing the process, that the next call answers, that another thread's status is its own,
 * and that threads with small and large stacks, and threads started in rounds, all behave the same. Prints one `ok`/`FAIL` line a check; exit 1 when one failed. */
#include <dlfcn.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef int64_t (*fn1)(int64_t);
typedef int64_t (*fn2)(const uint8_t *, int64_t, int64_t);
typedef int64_t (*fnlabel)(int64_t, uint8_t *, int64_t);
typedef void (*fnunit)(int64_t);
typedef int32_t (*fnstatus)(void);
typedef int64_t (*fnerror)(uint8_t *, int64_t);

static fn1 depth, grow, idx;
static fn2 keep_and_dive;
static fnlabel label;
static fnunit deep_unit;
static fnstatus status;
static fnerror error;
static int failed = 0;

static void check(const char *name, int cond) {
  printf("%s    %s\n", cond ? "ok  " : "FAIL", name);
  if (!cond) failed = 1;
}

static int msg_is(const char *want) {
  uint8_t buf[256];
  int64_t n = error(buf, sizeof buf);
  return n == (int64_t)strlen(want) && memcmp(buf, want, (size_t)n) == 0;
}

/* One thread's whole sequence; returns the number of failed expectations (as a pointer-sized value). */
static int rounds(int64_t huge) {
  int bad = 0;
  if (depth(1000) != 1000 || status() != 0) bad++;
  if (depth(huge) != 0 || status() != 1 || !msg_is("stack overflow")) bad++;
  if (depth(2000) != 2000 || status() != 0 || error(NULL, 0) != 0) bad++;
  if (grow(huge) != 0 || status() != 1 || !msg_is("stack overflow")) bad++;
  if (grow(300) <= 0 || status() != 0) bad++;
  return bad;
}

static void *thread_rounds(void *arg) {
  int n = (int)(intptr_t)arg, bad = 0;
  for (int i = 0; i < n; i++) bad += rounds(1000000000LL);
  return (void *)(intptr_t)bad;
}

static void *thread_status_isolation(void *arg) {
  (void)arg;
  int ok = depth(1000000000LL) == 0 && status() == 1;
  return (void *)(intptr_t)(ok ? 0 : 1);
}

static void *thread_small(void *arg) { /* a 256 KiB stack: 100000 frames cannot fit */
  (void)arg;
  int ok = depth(100000) == 0 && status() == 1 && msg_is("stack overflow") && depth(200) == 200 && status() == 0;
  return (void *)(intptr_t)(ok ? 0 : 1);
}

static void *thread_big(void *arg) { /* a 64 MiB stack: a million frames fit (about 32 MB) */
  (void)arg;
  int ok = depth(1000000) == 1000000 && status() == 0 && depth(1000000000LL) == 0 && status() == 1 && depth(1000000) == 1000000;
  return (void *)(intptr_t)(ok ? 0 : 1);
}

static int run_thread(void *(*f)(void *), void *arg, size_t stack) {
  pthread_attr_t a;
  pthread_t t;
  void *res = (void *)1;
  pthread_attr_init(&a);
  if (stack) pthread_attr_setstacksize(&a, stack);
  if (pthread_create(&t, &a, f, arg) != 0) return 1;
  pthread_join(t, &res);
  pthread_attr_destroy(&a);
  return (int)(intptr_t)res;
}

int main(int argc, char **argv) {
  if (argc < 2) return 2;
  void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
  if (!h) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 2; }
  depth = (fn1)dlsym(h, "depth"); grow = (fn1)dlsym(h, "grow"); idx = (fn1)dlsym(h, "idx");
  keep_and_dive = (fn2)dlsym(h, "keep_and_dive"); label = (fnlabel)dlsym(h, "label"); deep_unit = (fnunit)dlsym(h, "deep_unit");
  status = (fnstatus)dlsym(h, "fib_status"); error = (fnerror)dlsym(h, "fib_error");
  if (!depth || !grow || !idx || !keep_and_dive || !label || !deep_unit || !status || !error) { fprintf(stderr, "dlsym failed\n"); return 2; }
  int64_t huge = 1000000000LL;

  /* the host's main thread (its stack is the process's: ulimit -s) */
  check("a depth that fits answers (1000)", depth(1000) == 1000 && status() == 0);
  check("a depth that fits answers (100000 frames)", depth(100000) == 100000 && status() == 0);
  check("a recursion past the stack returns 0 with status 1", depth(huge) == 0 && status() == 1);
  check("the message is `stack overflow`", msg_is("stack overflow"));
  check("the next call answers and clears the status", depth(5000) == 5000 && status() == 0 && error(NULL, 0) == 0);
  check("a recursion that allocates in every frame overflows the same way", grow(huge) == 0 && status() == 1 && msg_is("stack overflow"));
  check("and allocation and free work after it", grow(500) > 0 && status() == 0 && depth(10) == 10);
  uint8_t sbuf[16];
  check("a str parameter: overflow returns 0, status 1", keep_and_dive((const uint8_t *)"abcdef", 6, huge) == 0 && status() == 1);
  check("a str parameter: the next call answers", keep_and_dive((const uint8_t *)"abcdef", 6, 10) == 16 && status() == 0);
  check("a str result: overflow returns length 0, status 1", label(huge, sbuf, sizeof sbuf) == 0 && status() == 1);
  check("a str result: the next call answers", label(12, sbuf, sizeof sbuf) == 8 && memcmp(sbuf, "depth=12", 8) == 0 && status() == 0);
  deep_unit(huge);
  check("a unit result: overflow returns, status 1", status() == 1);
  deep_unit(100);
  check("a unit result: the next call returns, status 0", status() == 0);
  check("another trap (index out of range) is a status, not an abort", idx(10) == 0 && status() == 1 && !msg_is("stack overflow"));
  check("and the call after it answers", idx(1) == 2 && status() == 0);

  /* threads the library did not start */
  check("a second host thread overflows safely (status is its own)", run_thread(thread_status_isolation, NULL, 0) == 0);
  check("the first thread's status is not the second's", depth(10) == 10 && status() == 0);
  check("a host thread with a 256 KiB stack overflows safely and recovers", run_thread(thread_small, NULL, 256 * 1024) == 0);
  check("a host thread with a 64 MiB stack: 1000000 frames answer, then overflow, then answer", run_thread(thread_big, NULL, 64u << 20) == 0);
  /* the same, from many threads at once, and from threads that start and end in rounds (stack addresses reused) */
  pthread_t ts[8];
  void *res[8];
  int bad = 0;
  for (int i = 0; i < 8; i++) pthread_create(&ts[i], NULL, thread_rounds, (void *)(intptr_t)40);
  for (int i = 0; i < 8; i++) { pthread_join(ts[i], &res[i]); bad += (int)(intptr_t)res[i]; }
  check("8 threads at once, 40 rounds each of ok / overflow / ok / allocating overflow / ok", bad == 0);
  bad = 0;
  for (int r = 0; r < 60; r++) {
    static const size_t sizes[4] = {256 * 1024, 512 * 1024, 2u << 20, 8u << 20};
    bad += run_thread(thread_status_isolation, NULL, sizes[r % 4]);
  }
  check("60 threads of 256 KiB to 8 MiB started and ended in turn, each overflowing once", bad == 0);
  check("the main thread still answers", depth(1000) == 1000 && status() == 0);
  return failed;
}
