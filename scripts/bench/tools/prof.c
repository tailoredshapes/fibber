/* LD_PRELOAD sampling profiler (perf is unavailable when perf_event_paranoid=4).
   cc -O1 -shared -fPIC -o prof.so prof.c -ldl ; PROF_OUT=file LD_PRELOAD=./prof.so ./prog
   SIGPROF every 1 ms of CPU time; records the pc and up to 5 return-address candidates; at exit writes one line per sample,
   "leaf < caller ..." as name+offset-in-module (exported symbols only, so offsets are resolved with nm); scripts/bench/tools/prof-report.sh aggregates. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <ucontext.h>
#include <execinfo.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#define MAXS 2000000
#define NF 32
static void *(*samples)[NF];
static volatile long ns;
static unsigned long lo, hi;
#include <link.h>
static int textrange(struct dl_phdr_info *i, size_t sz, void *d) {
  (void)sz; (void)d;
  for (int j = 0; j < i->dlpi_phnum; j++)
    if (i->dlpi_phdr[j].p_type == PT_LOAD && (i->dlpi_phdr[j].p_flags & PF_X)) {
      lo = i->dlpi_addr + i->dlpi_phdr[j].p_vaddr;
      hi = lo + i->dlpi_phdr[j].p_memsz;
    }
  return 1; /* the first object is the executable */
}
static void handler(int sig, siginfo_t *si, void *uc) {
  (void)sig; (void)si; (void)uc;
  if (ns >= MAXS) return;
  /* leaf pc from the signal context; then up to 5 words of the stack that point into the same
     module's text (return-address candidates: a heuristic, as the generated code keeps no frame pointer) */
  ucontext_t *c = uc;
  unsigned long pc = c->uc_mcontext.gregs[REG_RIP], sp = c->uc_mcontext.gregs[REG_RSP];
  samples[ns][0] = (void *)pc;
  int k = 1;
  for (int w = 0; w < 6000 && k < NF; w++) {
    unsigned long v = ((unsigned long *)sp)[w];
    if (v >= lo && v < hi && v != pc && ((unsigned char *)v)[-5] == 0xe8) samples[ns][k++] = (void *)v;
  }
  for (; k < NF; k++) samples[ns][k] = 0;
  ns++;
}
static void dump(void) {
  struct itimerval z = {{0, 0}, {0, 0}};
  setitimer(ITIMER_PROF, &z, 0);
  const char *o = getenv("PROF_OUT");
  FILE *f = fopen(o ? o : "prof.out", "w");
  for (long s = 0; s < ns; s++) {
    for (int i = 0; i < NF; i++) {
      Dl_info d;
      if (!samples[s][i]) break;
      if (dladdr(samples[s][i], &d) && d.dli_fbase) fprintf(f, "%s%s@%s+%lx", i ? " < " : "", d.dli_sname ? d.dli_sname : "", strrchr(d.dli_fname, '/') ? strrchr(d.dli_fname, '/') + 1 : d.dli_fname, (unsigned long)((char *)samples[s][i] - (char *)d.dli_fbase));
      else fprintf(f, "%s?", i ? " < " : "");
    }
    fputc('\n', f);
  }
  fclose(f);
}
__attribute__((constructor)) static void init(void) {
  samples = malloc(sizeof(void *[NF]) * MAXS);
  dl_iterate_phdr(textrange, 0);
  struct sigaction sa = {0};
  sa.sa_sigaction = handler;
  sa.sa_flags = SA_SIGINFO | SA_RESTART;
  sigaction(SIGPROF, &sa, 0);
  struct itimerval t = {{0, 1000}, {0, 1000}};
  setitimer(ITIMER_PROF, &t, 0);
  atexit(dump);
}
