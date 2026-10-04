/* ceiling experiment: mandelbrot with AVX2 intrinsics, 8 pixels (two 4-lane vectors) per output byte, same arithmetic order as mandelbrot.fib */
#include <immintrin.h>
#include <stdio.h>
#include <stdlib.h>

static inline int inside4(__m256d cr, __m256d ci) {
  __m256d zr = _mm256_setzero_pd(), zi = zr, tr = zr, ti = zr;
  __m256d four = _mm256_set1_pd(4.0), two = _mm256_set1_pd(2.0);
  __m256d esc = _mm256_setzero_pd();
  for (int i = 0; i <= 50; i++) {
    esc = _mm256_or_pd(esc, _mm256_cmp_pd(_mm256_add_pd(tr, ti), four, _CMP_GT_OQ));
    if (_mm256_movemask_pd(esc) == 15) break;
    if (i == 50) break;
    __m256d zi2 = _mm256_add_pd(_mm256_mul_pd(_mm256_mul_pd(two, zr), zi), ci);
    __m256d zr2 = _mm256_add_pd(_mm256_sub_pd(tr, ti), cr);
    zr = zr2; zi = zi2; tr = _mm256_mul_pd(zr2, zr2); ti = _mm256_mul_pd(zi2, zi2);
  }
  return (~_mm256_movemask_pd(esc)) & 15;
}

int main(int argc, char **argv) {
  long n = atol(argv[1]);
  long w = (n + 7) / 8;
  unsigned char *row = malloc(w);
  printf("P4\n%ld %ld\n", n, n);
  for (long y = 0; y < n; y++) {
    double ci = 2.0 * (double)y / (double)n - 1.0;
    __m256d vci = _mm256_set1_pd(ci);
    for (long bx = 0; bx < w; bx++) {
      double c[8];
      for (int k = 0; k < 8; k++) c[k] = (2.0 * (double)(bx * 8 + k)) / (double)n - 1.5;
      int lo = inside4(_mm256_loadu_pd(c), vci), hi = inside4(_mm256_loadu_pd(c + 4), vci);
      int acc = 0;
      for (int k = 0; k < 8; k++) {
        long x = bx * 8 + k;
        int bit = x < n && (k < 4 ? (lo >> k) & 1 : (hi >> (k - 4)) & 1);
        acc = (acc << 1) | bit;
      }
      row[bx] = (unsigned char)acc;
    }
    fwrite(row, 1, w, stdout);
  }
  return 0;
}
