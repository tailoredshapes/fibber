/* The C host of compiler/tests/driver/host-threads.sh: NTHREADS threads call the library at the same time, from the first call on
   (nothing calls it on the main thread first unless MAIN_FIRST is given), and every result is checked.
   usage: host LIB.so NTHREADS [main-first]  -- exit 0 when every thread saw the right numbers, 1 for a wrong answer. */
#include <stdio.h>
#include <stdlib.h>
#include <dlfcn.h>
#include <pthread.h>
typedef long long i64;
static i64 (*p_add)(i64, i64), (*p_sq)(i64), (*p_len)(i64);
/* A barrier of a mutex and a condition variable: macOS has no pthread_barrier_t. The last of `need` threads to arrive starts the next
   generation and wakes the others; a thread waits until the generation it arrived in has ended (which also ignores spurious wake-ups). */
static struct { pthread_mutex_t m; pthread_cond_t c; int need, arrived; unsigned gen; } go = { PTHREAD_MUTEX_INITIALIZER, PTHREAD_COND_INITIALIZER, 0, 0, 0 };
static void barrier_wait(void) {
  pthread_mutex_lock(&go.m);
  unsigned gen = go.gen;
  if (++go.arrived == go.need) { go.arrived = 0; go.gen++; pthread_cond_broadcast(&go.c); }
  else while (gen == go.gen) pthread_cond_wait(&go.c, &go.m);
  pthread_mutex_unlock(&go.m);
}
static void *worker(void *a) {
  i64 bad = 0;
  barrier_wait();
  for (int i = 0; i < 2000; i++) {
    if (p_sq(100) != 328350) bad++;
    if (p_add(i, 1) != i + 1) bad++;
    if (p_len(i) != 6 + (i64)snprintf(0, 0, "%d", i)) bad++;
  }
  *(i64 *)a = bad;
  return 0;
}
int main(int argc, char **argv) {
  void *h = dlopen(argv[1], RTLD_NOW);
  if (!h) { printf("dlopen: %s\n", dlerror()); return 2; }
  p_add = dlsym(h, "add"); p_sq = dlsym(h, "sum-squares"); p_len = dlsym(h, "text-len");
  int nt = atoi(argv[2]);
  if (argc > 3) { if (p_sq(100) != 328350) return 1; }
  pthread_t th[64]; i64 res[64];
  go.need = nt;
  for (int i = 0; i < nt; i++) pthread_create(&th[i], 0, worker, &res[i]);
  i64 bad = 0;
  for (int i = 0; i < nt; i++) { pthread_join(th[i], 0); bad += res[i]; }
  printf("%d threads: %lld wrong answers\n", nt, bad);
  return bad ? 1 : 0;
}
