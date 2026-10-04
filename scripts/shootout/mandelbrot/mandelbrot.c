/* mandelbrot: the Benchmarks Game's, N x N pixels, 50 iterations, P4 bitmap on stdout. */
#include <stdio.h>
#include <stdlib.h>

static int inside(double cr, double ci) {
  double zr = 0, zi = 0, tr = 0, ti = 0;
  for (int i = 0; i < 50 && tr + ti <= 4.0; i++) {
    zi = 2.0 * zr * zi + ci;
    zr = tr - ti + cr;
    tr = zr * zr;
    ti = zi * zi;
  }
  return tr + ti <= 4.0;
}

int main(int argc, char **argv) {
  long n = atol(argv[1]);
  long w = (n + 7) / 8;
  unsigned char *row = malloc(w);
  printf("P4\n%ld %ld\n", n, n);
  for (long y = 0; y < n; y++) {
    double ci = 2.0 * (double)y / (double)n - 1.0;
    for (long bx = 0; bx < w; bx++) {
      int acc = 0;
      for (int k = 0; k < 8; k++) {
        long x = bx * 8 + k;
        int bit = x < n && inside(2.0 * (double)x / (double)n - 1.5, ci);
        acc = (acc << 1) | bit;
      }
      row[bx] = (unsigned char)acc;
    }
    fwrite(row, 1, w, stdout);
  }
  return 0;
}
