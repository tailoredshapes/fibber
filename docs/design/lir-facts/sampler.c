#define _GNU_SOURCE
#include <signal.h>
#include <sys/time.h>
#include <ucontext.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <link.h>
static unsigned long buf[1<<20]; static volatile int n;
static void h(int s, siginfo_t *si, void *uc) { (void)s;(void)si; if (n < (1<<20)) buf[n++] = ((ucontext_t*)uc)->uc_mcontext.gregs[REG_RIP]; }
static unsigned long base;
static int cb(struct dl_phdr_info *i, size_t sz, void *d) { (void)sz;(void)d; if (i->dlpi_name[0]==0 && !base) base = i->dlpi_addr; return 0; }
static void dump(void) {
  struct itimerval z = {0}; setitimer(ITIMER_PROF, &z, 0);
  dl_iterate_phdr(cb, 0);
  const char *o = getenv("SAMPLE_OUT"); FILE *f = fopen(o ? o : "samples.txt", "w");
  for (int k = 0; k < n; k++) fprintf(f, "%lx\n", buf[k] - base);
  fclose(f);
}
__attribute__((constructor)) static void init(void) {
  struct sigaction sa; memset(&sa, 0, sizeof sa); sa.sa_sigaction = h; sa.sa_flags = SA_SIGINFO | SA_RESTART;
  sigaction(SIGPROF, &sa, 0);
  struct itimerval t = {{0, 250}, {0, 250}}; setitimer(ITIMER_PROF, &t, 0);
  atexit(dump);
}
