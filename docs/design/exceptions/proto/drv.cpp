// driver: MODE_* macro picks how a failure is caught
#include <cstdio>
#include <cstdlib>
#include <csetjmp>
#include <ctime>
#include <cstdint>
struct R { long v; char e; };
extern "C" {
#if defined(MODE_result)
  R f_0(long, long);
#else
  long f_0(long, long);
#endif
  char thrown;
  static long live = 0;
  void *fib_alloc() { live++; return malloc(16); }
  void fib_release(void *p) { live--; free(p); }
  void do_abort() { abort(); }
  void do_throw() { throw 42; }
  static jmp_buf jb;
  void do_longjmp() { longjmp(jb, 1); }
}
static double now() { timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec + t.tv_nsec * 1e-9; }
int main(int argc, char** argv) {
  long N = argc > 1 ? atol(argv[1]) : 20000000;
  long M = argc > 2 ? atol(argv[2]) : 200000;
  long acc = 0;
  double t0 = now();
  for (long i = 0; i < N; i++) {
#if defined(MODE_result)
    acc += f_0(i, 0).v;
#else
    acc += f_0(i, 0);
#endif
  }
  double t1 = now();
  printf("happy: %.2f ns per chain call (acc %ld, live %ld)\n", (t1 - t0) / N * 1e9, acc, live);
  long caught = 0;
  t0 = now();
#if defined(MODE_plain)
  (void)M;
#else
  for (long i = 0; i < M; i++) {
#if defined(MODE_invoke)
    try { acc += f_0(i, 1); } catch (int) { caught++; }
#elif defined(MODE_result)
    R r = f_0(i, 1); if (r.e) caught++;
#elif defined(MODE_flag)
    thrown = 0; acc += f_0(i, 1); if (thrown) caught++;
#elif defined(MODE_jmp)
    if (setjmp(jb) == 0) acc += f_0(i, 1); else caught++;
#endif
  }
#endif
  t1 = now();
  printf("caught: %ld throws, %.1f ns per throw through the chain (live objects left %ld)\n", caught, caught ? (t1 - t0) / caught * 1e9 : 0.0, live);
  return acc == 12345 ? 1 : 0;
}
