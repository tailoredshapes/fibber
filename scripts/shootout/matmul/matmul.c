/* matmul: C = A*B for n x n matrices of doubles, loop order i k j (the inner loop runs along a row of B and of C).
   A[i][k] = (i + k) mod 7, B[k][j] = (k + 2j) mod 5: integers, so every sum is exact in any order. Output: sum of C, %.1f */
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
  long n = atol(argv[1]);
  double *a = malloc(n * n * sizeof(double)), *b = malloc(n * n * sizeof(double)), *c = calloc(n * n, sizeof(double));
  for (long i = 0; i < n; i++)
    for (long j = 0; j < n; j++) { a[i * n + j] = (double)((i + j) % 7); b[i * n + j] = (double)((i + 2 * j) % 5); }
  for (long i = 0; i < n; i++)
    for (long k = 0; k < n; k++) {
      double aik = a[i * n + k];
      for (long j = 0; j < n; j++) c[i * n + j] += aik * b[k * n + j];
    }
  double s = 0.0;
  for (long i = 0; i < n * n; i++) s += c[i];
  printf("%.1f\n", s);
  return 0;
}
