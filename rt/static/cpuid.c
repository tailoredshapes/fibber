/* rt/static/cpuid.c: glibc's __x86_get_cpuid_feature_leaf for a static musl executable (docs/design/static-linking.md).
 *
 * The start-up CPU check of an x86-64 executable (compiler/emit/cpucheck.fib, docs/adr/0008) calls it as glibc 2.33+ defines it: it returns a pointer
 * to a struct of four words of CPUID output followed by four words of "active" bits (CPUID with the operating system's register-state support
 * folded in); the check reads the active words at byte offset 16. leaf 0 is CPUID 1, 1 is CPUID 7 subleaf 0, 2 is CPUID 0x80000001
 * (glibc's CPUID_INDEX_1, _7, _80000001). musl has no such function; this file is compiled by scripts/build-musl.sh into fibshim.o, which
 * `fibc build --static` links on x86-64. */
#include <cpuid.h>
#include <stdint.h>

struct cpuid_feature { uint32_t cpuid[4]; uint32_t active[4]; };
static struct cpuid_feature leaves[3];
static int ready;

static uint64_t xcr0(void) { uint32_t lo, hi; __asm__ volatile("xgetbv" : "=a"(lo), "=d"(hi) : "c"(0)); return ((uint64_t)hi << 32) | lo; }

static void put(int i, unsigned a, unsigned b, unsigned c, unsigned d) {
  leaves[i].cpuid[0] = a; leaves[i].cpuid[1] = b; leaves[i].cpuid[2] = c; leaves[i].cpuid[3] = d;
}

static void fill(void) {
  unsigned a, b, c, d, max = __get_cpuid_max(0, 0);
  uint64_t x = 0;
  if (__get_cpuid(1, &a, &b, &c, &d)) put(0, a, b, c, d);
  if (leaves[0].cpuid[2] & (1u << 27)) x = xcr0();                       /* OSXSAVE */
  if (max >= 7) { __cpuid_count(7, 0, a, b, c, d); put(1, a, b, c, d); }
  if (__get_cpuid(0x80000001u, &a, &b, &c, &d)) put(2, a, b, c, d);
  int ymm = (x & 6) == 6, zmm = (x & 0xe6) == 0xe6;
  for (int i = 0; i < 3; i++) for (int r = 0; r < 4; r++) leaves[i].active[r] = leaves[i].cpuid[r];
  if (!ymm) {
    leaves[0].active[2] &= ~((1u << 28) | (1u << 12) | (1u << 29));      /* AVX, FMA, F16C */
    leaves[1].active[1] &= ~(1u << 5);                                   /* AVX2 */
  }
  if (!zmm) leaves[1].active[1] &= ~((1u << 16) | (1u << 17) | (1u << 28) | (1u << 30) | (1u << 31));   /* AVX512 F DQ CD BW VL */
  ready = 1;
}

const struct cpuid_feature *__x86_get_cpuid_feature_leaf(unsigned leaf) {
  if (!ready) fill();
  return &leaves[leaf < 3 ? leaf : 0];
}
