/* dot-product: sum of x[i]*y[i] over arrays of N doubles, repeated REPS times (default 1). x[i] = i mod 7, y[i] = i mod 3 + 0.5:
   every product and partial sum is a multiple of 0.5 below 2^53, so the result is exact in any order of summation. Output: %.1f */
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
  long n = atol(argv[1]);
  long reps = argc > 2 ? atol(argv[2]) : 1;
  double *x = malloc(n * sizeof(double)), *y = malloc(n * sizeof(double));
  for (long i = 0; i < n; i++) { x[i] = (double)(i % 7); y[i] = (double)(i % 3) + 0.5; }
  double total = 0.0;
  for (long r = 0; r < reps; r++) {
    double s = 0.0;
    for (long i = 0; i < n; i++) s += x[i] * y[i];
    total += s;
  }
  printf("%.1f\n", total);
  return 0;
}
