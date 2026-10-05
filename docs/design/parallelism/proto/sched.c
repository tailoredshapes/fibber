// sched.c: prototype of two schedulers for fork-join tasks, on the same task protocol as rt/task.lir:
//   A  "mutex": one run queue under one pthread mutex, one condvar broadcast at every enqueue and every completion, malloc'd list nodes
//      (a transcription of fib.enqueue / fib.dequeue-wait / fib.idle-wait / fib.task-complete); join claims the task itself with a
//      cmpxchg on the state when nobody started it (fib.drive), else runs other queued tasks, else sleeps on the condvar.
//   B  "chase-lev": one Chase-Lev deque per worker (Le, Pop, Cohen, Nardelli 2013, C11 atomics), the owner pushes and pops at the
//      bottom, thieves steal at the top from a random victim; join = claim the task itself, else pop own, else steal, else back off.
// Workload: fork-join fib(N): ptask(n) = n<=CUT ? sfib(n) : fork(n-1), run n-2 inline, join, add.
//   usage: sched A|B W CUT N RUNS     (W = workers including the main thread)
//          sched micro                 (uncontended push+pop cost of each queue)
#define _GNU_SOURCE
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
static void *aligned_alloc64(size_t n) { void *p = NULL; if (posix_memalign(&p, 64, n)) abort(); return p; }

typedef struct task { int n; long result; atomic_int state; /* 0 pending 1 running 2 done */ } task;
static int IMPL, CUT, NW;
static atomic_int stop_flag;
static __thread int me;
static __thread unsigned rng;
static atomic_long forks;

static long sfib(int n) { return n < 2 ? n : sfib(n - 1) + sfib(n - 2); }
static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1e3 + t.tv_nsec / 1e6; }

// ---- task allocation: a per-thread bump chunk, never freed (the prototype has no counts) ----
static __thread task *chunk; static __thread int chunk_left;
static task *new_task(int n) {
  if (!chunk_left) { chunk = malloc(sizeof(task) * 4096); chunk_left = 4096; }
  task *t = chunk++; chunk_left--; t->n = n; t->result = 0; atomic_init(&t->state, 0); return t;
}

// ---- A: the mutex queue ----
typedef struct node { task *t; struct node *next; } node;
static node *qh, *qt; static pthread_mutex_t qm = PTHREAD_MUTEX_INITIALIZER; static pthread_cond_t qc = PTHREAD_COND_INITIALIZER;
static void a_push(task *t) {
  node *n = malloc(sizeof(node)); n->t = t; n->next = NULL;
  pthread_mutex_lock(&qm);
  if (!qt) qh = qt = n; else { qt->next = n; qt = n; }
  pthread_cond_broadcast(&qc); pthread_mutex_unlock(&qm);
}
static task *pop_locked(void) { node *n = qh; if (!n) return NULL; qh = n->next; if (!qh) qt = NULL; task *t = n->t; free(n); return t; }
static task *a_get(void) { pthread_mutex_lock(&qm); task *t = pop_locked(); pthread_mutex_unlock(&qm); return t; }
static void a_complete(void) { pthread_mutex_lock(&qm); pthread_cond_broadcast(&qc); pthread_mutex_unlock(&qm); }
static void a_idle(task *mine) {  // fib.idle-wait
  pthread_mutex_lock(&qm);
  if (!qh && atomic_load_explicit(&mine->state, memory_order_acquire) != 2) pthread_cond_wait(&qc, &qm);
  pthread_mutex_unlock(&qm);
}

// ---- B: Chase-Lev deques ----
#define DQ 8192
typedef struct { atomic_long top, bottom; _Atomic(task *) buf[DQ]; char pad[64]; } deque;
static deque *dqs;
static int b_push(task *t) {
  deque *d = &dqs[me]; long b = atomic_load_explicit(&d->bottom, memory_order_relaxed), tp = atomic_load_explicit(&d->top, memory_order_acquire);
  if (b - tp >= DQ - 1) return 0;
  atomic_store_explicit(&d->buf[b & (DQ - 1)], t, memory_order_relaxed);
  atomic_thread_fence(memory_order_release);
  atomic_store_explicit(&d->bottom, b + 1, memory_order_relaxed);
  return 1;
}
static task *b_pop(void) {
  deque *d = &dqs[me]; long b = atomic_load_explicit(&d->bottom, memory_order_relaxed) - 1;
  atomic_store_explicit(&d->bottom, b, memory_order_relaxed);
  atomic_thread_fence(memory_order_seq_cst);
  long t = atomic_load_explicit(&d->top, memory_order_relaxed); task *x = NULL;
  if (t <= b) {
    x = atomic_load_explicit(&d->buf[b & (DQ - 1)], memory_order_relaxed);
    if (t == b) { if (!atomic_compare_exchange_strong_explicit(&d->top, &t, t + 1, memory_order_seq_cst, memory_order_relaxed)) x = NULL;
                  atomic_store_explicit(&d->bottom, b + 1, memory_order_relaxed); }
  } else atomic_store_explicit(&d->bottom, b + 1, memory_order_relaxed);
  return x;
}
static task *b_steal_from(int v) {
  deque *d = &dqs[v]; long t = atomic_load_explicit(&d->top, memory_order_acquire);
  atomic_thread_fence(memory_order_seq_cst);
  long b = atomic_load_explicit(&d->bottom, memory_order_acquire); task *x = NULL;
  if (t < b) { x = atomic_load_explicit(&d->buf[t & (DQ - 1)], memory_order_relaxed);
               if (!atomic_compare_exchange_strong_explicit(&d->top, &t, t + 1, memory_order_seq_cst, memory_order_relaxed)) return NULL; }
  return x;
}
static task *b_get(void) {
  task *x = b_pop(); if (x) return x;
  if (NW == 1) return NULL;
  rng = rng * 1664525u + 1013904223u; int v = (rng >> 8) % NW; if (v == me) v = (v + 1) % NW;
  return b_steal_from(v);
}

// ---- common protocol ----
static long ptask(int n);
static void run(task *t) { long r = ptask(t->n); t->result = r; atomic_store_explicit(&t->state, 2, memory_order_release); if (IMPL == 'A') a_complete(); }
static int claim(task *t) { int z = 0; return atomic_compare_exchange_strong_explicit(&t->state, &z, 1, memory_order_acq_rel, memory_order_acquire); }
static void fork_task(task *t) {
  atomic_fetch_add_explicit(&forks, 1, memory_order_relaxed);
  if (IMPL == 'A') a_push(t); else if (!b_push(t)) { if (claim(t)) run(t); }
}
static long join_task(task *t) {
  int spins = 0;
  for (;;) {
    if (atomic_load_explicit(&t->state, memory_order_acquire) == 2) return t->result;
    if (claim(t)) { run(t); return t->result; }
    task *u = IMPL == 'A' ? a_get() : b_get();
    if (u) { if (claim(u)) run(u); spins = 0; continue; }
    if (IMPL == 'A') a_idle(t); else if (++spins > 64) sched_yield();
  }
}
static long ptask(int n) {
  if (n <= CUT) return sfib(n);
  task *c = new_task(n - 1); fork_task(c);
  long r = ptask(n - 2);
  return r + join_task(c);
}
static void *worker(void *arg) {
  me = (int)(long)arg; rng = 12345u + me * 7919u; int spins = 0;
  if (IMPL == 'A') {
    for (;;) {
      pthread_mutex_lock(&qm); task *t;
      while (!(t = pop_locked())) { if (atomic_load(&stop_flag)) { pthread_mutex_unlock(&qm); return NULL; } pthread_cond_wait(&qc, &qm); }
      pthread_mutex_unlock(&qm);
      if (claim(t)) run(t);
    }
  }
  for (;;) {
    task *t = b_get();
    if (t) { if (claim(t)) run(t); spins = 0; }
    else { if (atomic_load(&stop_flag)) return NULL; if (++spins > 64) sched_yield(); }
  }
}

static int cmpd(const void *a, const void *b) { double x = *(double *)a, y = *(double *)b; return (x > y) - (x < y); }

static void micro(void) {
  int N = 20000000; task t; double t0;
  NW = 1; me = 0; dqs = aligned_alloc64(sizeof(deque)); memset(dqs, 0, sizeof(deque));
  t0 = now(); for (int i = 0; i < N; i++) { b_push(&t); b_pop(); } printf("chase-lev push+pop uncontended: %.2f ns\n", (now() - t0) * 1e6 / N);
  t0 = now(); for (int i = 0; i < N / 10; i++) { a_push(&t); a_get(); } printf("mutex queue push+pop uncontended (malloc node, broadcast): %.2f ns\n", (now() - t0) * 1e6 / (N / 10));
}

int main(int argc, char **argv) {
  if (argc >= 2 && !strcmp(argv[1], "micro")) { micro(); return 0; }
  IMPL = argv[1][0]; NW = atoi(argv[2]); CUT = atoi(argv[3]); int N = atoi(argv[4]), runs = atoi(argv[5]);
  dqs = aligned_alloc64(sizeof(deque) * NW); memset(dqs, 0, sizeof(deque) * NW);
  pthread_t th; me = 0; rng = 1;
  for (int i = 1; i < NW; i++) pthread_create(&th, NULL, worker, (void *)(long)i);
  double ts[64]; long expect = sfib(N);
  for (int r = 0; r < runs; r++) {
    atomic_store(&forks, 0); double t0 = now(); long v = ptask(N); ts[r] = now() - t0;
    if (v != expect) { printf("WRONG %ld != %ld\n", v, expect); return 1; }
  }
  double sorted[64]; memcpy(sorted, ts, sizeof(double) * runs); qsort(sorted, runs, sizeof(double), cmpd);
  printf("%c W=%d CUT=%d N=%d forks=%ld median_ms=%.2f min_ms=%.2f\n", IMPL, NW, CUT, N, (long)atomic_load(&forks), sorted[runs / 2], sorted[0]);
  atomic_store(&stop_flag, 1); pthread_mutex_lock(&qm); pthread_cond_broadcast(&qc); pthread_mutex_unlock(&qm);
  return 0;
}
