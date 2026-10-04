/* spectral-norm (Benchmarks Game algorithm), single-threaded. Output: %.9f */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

static double a(int i, int j) { return 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1); }

static void mul_av(int n, const double *v, double *av) {
  for (int i = 0; i < n; i++) {
    double s = 0.0;
    for (int j = 0; j < n; j++) s += a(i, j) * v[j];
    av[i] = s;
  }
}

static void mul_atv(int n, const double *v, double *atv) {
  for (int i = 0; i < n; i++) {
    double s = 0.0;
    for (int j = 0; j < n; j++) s += a(j, i) * v[j];
    atv[i] = s;
  }
}

int main(int argc, char **argv) {
  int n = atoi(argv[1]);
  double *u = malloc(n * sizeof(double)), *v = malloc(n * sizeof(double)), *t = malloc(n * sizeof(double));
  for (int i = 0; i < n; i++) u[i] = 1.0;
  for (int i = 0; i < 10; i++) {
    mul_av(n, u, t); mul_atv(n, t, v);
    mul_av(n, v, t); mul_atv(n, t, u);
  }
  double vbv = 0, vv = 0;
  for (int i = 0; i < n; i++) { vbv += u[i] * v[i]; vv += v[i] * v[i]; }
  printf("%0.9f\n", sqrt(vbv / vv));
  return 0;
}
