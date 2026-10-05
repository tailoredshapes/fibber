// rc.c: what a shared reference count costs under N-thread read-mostly access (the hidden cost of parallel sharing in a refcounted language).
// Each thread does N iterations: pick object (i*7+tid) % K, retain it (+1), release it (-1), as fib.retain / fib.release do. Header layout of
// fibber: 16 bytes (i64 count, i32 type-id, i32 flags) followed by the payload, objects packed 32 bytes apart (two per cache line) unless PAD.
//   plain    each thread has its own K objects, non-atomic count (the unshared fast path)
//   atomic   one set of K objects for all threads, lock xadd / ldadd (the SHARED path: atomicrmw add monotonic, sub acq_rel)
//   atomicpad  as atomic, one object per 64-byte line
//   flag     one set for all threads, retain/release read the flags word and skip (the IMMORTAL / frozen path): no write at all
//   cache    one set for all threads, per-thread table of 64 slots keyed by object: the delta is a plain add in the thread's table; a conflict or every
//            4096 operations flushes the slots with one atomic add per nonzero slot (a per-thread count cache: the count is exact after a flush only)
//   usage: rc MODE K T N
#define _GNU_SOURCE
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
static void *aligned_alloc64(size_t n) { void *p = NULL; if (posix_memalign(&p, 64, n)) abort(); return p; }
typedef struct { _Atomic long count; int tid; _Atomic int flags; long payload; } obj;  // 32 bytes
typedef struct { obj o; char pad[32]; } padobj;
static int MODE, K, T; static long N;
static obj *shared; static padobj *sharedpad;
static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1e3 + t.tv_nsec / 1e6; }
static atomic_int started, go, finished;
static void *run(void *arg) {
  long id = (long)arg; obj *mine = NULL; long sink = 0;
  if (MODE == 'p') { mine = aligned_alloc64(sizeof(obj) * K); for (int i = 0; i < K; i++) { mine[i].count = 1; mine[i].flags = 0; } }
  struct { obj *o; long d; } slot[64]; memset(slot, 0, sizeof slot); long since = 0;
  atomic_fetch_add(&started, 1); while (!atomic_load(&go)) ;
  for (long i = 0; i < N; i++) {
    int k = (int)((i * 7 + id) % K);
    switch (MODE) {
      case 'p': { obj *o = &mine[k]; long c = atomic_load_explicit(&o->count, memory_order_relaxed); atomic_store_explicit(&o->count, c + 1, memory_order_relaxed); sink += o->payload; c = atomic_load_explicit(&o->count, memory_order_relaxed); atomic_store_explicit(&o->count, c - 1, memory_order_relaxed); break; }  // non-atomic: plain load/add/store
      case 'a': { obj *o = &shared[k]; atomic_fetch_add_explicit(&o->count, 1, memory_order_relaxed); sink += o->payload; atomic_fetch_sub_explicit(&o->count, 1, memory_order_acq_rel); break; }
      case 'P': { obj *o = &sharedpad[k].o; atomic_fetch_add_explicit(&o->count, 1, memory_order_relaxed); sink += o->payload; atomic_fetch_sub_explicit(&o->count, 1, memory_order_acq_rel); break; }
      case 'f': { obj *o = &shared[k]; if (atomic_load_explicit(&o->flags, memory_order_relaxed) & 8) { sink += o->payload; break; } break; }
      case 'c': { obj *o = &shared[k]; int s = k & 63;
                  if (slot[s].o != o) { if (slot[s].o && slot[s].d) atomic_fetch_add_explicit(&slot[s].o->count, slot[s].d, memory_order_relaxed); slot[s].o = o; slot[s].d = 0; }
                  slot[s].d += 1; sink += o->payload; slot[s].d -= 1;
                  if (++since == 4096) { since = 0; for (int j = 0; j < 64; j++) if (slot[j].o && slot[j].d) { atomic_fetch_add_explicit(&slot[j].o->count, slot[j].d, memory_order_relaxed); slot[j].d = 0; } }
                  break; }
    }
  }
  atomic_fetch_add(&finished, 1);
  return (void *)sink;
}
int main(int argc, char **argv) {
  MODE = argv[1][0]; K = atoi(argv[2]); T = atoi(argv[3]); N = atol(argv[4]);
  shared = aligned_alloc64(sizeof(obj) * K); sharedpad = aligned_alloc64(sizeof(padobj) * K);
  for (int i = 0; i < K; i++) { shared[i].count = 1; shared[i].flags = 8; shared[i].payload = i; sharedpad[i].o.count = 1; sharedpad[i].o.payload = i; }
  pthread_t th[64]; for (long i = 0; i < T; i++) pthread_create(&th[i], NULL, run, (void *)i);
  while (atomic_load(&started) < T) ; double t0 = now(); atomic_store(&go, 1); while (atomic_load(&finished) < T) ; double dt = now() - t0;
  for (int i = 0; i < T; i++) pthread_join(th[i], NULL);
  printf("%c K=%d T=%d N=%ld wall_ms=%.1f ns_per_pair_per_thread=%.2f\n", MODE, K, T, N, dt, dt * 1e6 / N);
  return 0;
}
