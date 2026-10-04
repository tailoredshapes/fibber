/* fannkuch-redux, single-threaded: the same rotate-and-count algorithm as fannkuch-redux.fib, .java and .clj */
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
  int n = argc > 1 ? atoi(argv[1]) : 7;
  int p[32], p1[32], cnt[32];
  long checksum = 0, maxflips = 0, pc = 0;
  int r = n;
  for (int i = 0; i < n; i++) p1[i] = i;
  for (;;) {
    while (r != 1) { cnt[r - 1] = r; r--; }
    for (int i = 0; i < n; i++) p[i] = p1[i];
    long flips = 0;
    int k;
    while ((k = p[0]) != 0) {
      for (int i = 0, j = k; i < j; i++, j--) { int t = p[i]; p[i] = p[j]; p[j] = t; }
      flips++;
    }
    if (flips > maxflips) maxflips = flips;
    checksum += (pc % 2 == 0) ? flips : -flips;
    for (;;) {
      if (r == n) { printf("%ld\nPfannkuchen(%d) = %ld\n", checksum, n, maxflips); return 0; }
      int p0 = p1[0];
      for (int i = 0; i < r; i++) p1[i] = p1[i + 1];
      p1[r] = p0;
      if (--cnt[r] > 0) break;
      r++;
    }
    pc++;
  }
}
