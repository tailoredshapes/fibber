/* The C host of compiler/tests/driver/host-threads.sh: NTHREADS threads call the library at the same time, from the first call on
   (nothing calls it on the main thread first unless MAIN_FIRST is given), and every result is checked.
   usage: host LIB.so NTHREADS [main-first]  -- exit 0 when every thread saw the right numbers, 1 for a wrong answer. */
#include <stdio.h>
#include <stdlib.h>
#include <dlfcn.h>
#include <pthread.h>
typedef long long i64;
static i64 (*p_add)(i64, i64), (*p_sq)(i64), (*p_len)(i64);
static pthread_barrier_t go;
static void *worker(void *a) {
  i64 bad = 0;
  pthread_barrier_wait(&go);
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
  pthread_barrier_init(&go, 0, nt);
  for (int i = 0; i < nt; i++) pthread_create(&th[i], 0, worker, &res[i]);
  i64 bad = 0;
  for (int i = 0; i < nt; i++) { pthread_join(th[i], 0); bad += res[i]; }
  printf("%d threads: %lld wrong answers\n", nt, bad);
  return bad ? 1 : 0;
}
