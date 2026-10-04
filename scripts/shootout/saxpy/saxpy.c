/* saxpy: y = a*x + y with a = 0.5 over arrays of N doubles, REPS times (default 1); then the sum of y. x[i] = i mod 7,
   y[i] = i mod 5: every value is a multiple of 0.5 below 2^53, so the result is exact (with or without fma). Output: %.1f */
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
  long n = atol(argv[1]);
  long reps = argc > 2 ? atol(argv[2]) : 1;
  double *x = malloc(n * sizeof(double)), *y = malloc(n * sizeof(double));
  for (long i = 0; i < n; i++) { x[i] = (double)(i % 7); y[i] = (double)(i % 5); }
  for (long r = 0; r < reps; r++)
    for (long i = 0; i < n; i++) y[i] = 0.5 * x[i] + y[i];
  double s = 0.0;
  for (long i = 0; i < n; i++) s += y[i];
  printf("%.1f\n", s);
  return 0;
}
