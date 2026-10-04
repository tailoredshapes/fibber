/* reverse-complement, C twin: the same algorithm as reverse-complement.fib (read(2) in 64 KB blocks, complemented blocks kept in
   a list, written back to front in 60-column lines through a 64 KB buffer). gcc -O3 -march=native. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static unsigned char out[65536];
static int pos = 0;

static void flush_out(void) {
  int off = 0;
  while (off < pos) { ssize_t k = write(1, out + off, pos - off); if (k <= 0) exit(1); off += (int)k; }
  pos = 0;
}
static inline void put(int b) { if (pos == 65536) flush_out(); out[pos++] = (unsigned char)b; }

typedef struct { unsigned char *p; int n; } chunk;
static chunk *chunks = 0; static int nchunks = 0, cap = 0;
static void add(unsigned char *tmp, int j) {
  if (nchunks == cap) { cap = cap ? cap * 2 : 1024; chunks = realloc(chunks, cap * sizeof(chunk)); }
  chunks[nchunks].p = malloc(j); memcpy(chunks[nchunks].p, tmp, j); chunks[nchunks].n = j; nchunks++;
}
static void emit(void) {
  int col = 0;
  for (int k = nchunks - 1; k >= 0; k--) {
    unsigned char *c = chunks[k].p;
    for (int i = chunks[k].n - 1; i >= 0; i--) { put(c[i]); if (++col == 60) { put('\n'); col = 0; } }
    free(c);
  }
  if (col > 0) put('\n');
  nchunks = 0;
}

int main(void) {
  unsigned char tbl[256]; memset(tbl, 0, sizeof tbl);
  const char *from = "ACGTUMRWSYKVHDBN", *to = "TGCAAKYWSRMBDHVN";
  for (int i = 0; i < 16; i++) { tbl[(int)from[i]] = to[i]; tbl[from[i] + 32] = to[i]; }
  unsigned char b[65536], tmp[65536];
  int mode = 0; ssize_t n;
  while ((n = read(0, b, sizeof b)) > 0) {
    int j = 0;
    for (ssize_t i = 0; i < n; i++) {
      int c = b[i];
      if (mode == 1) { put(c); if (c == '\n') mode = 0; }
      else if (c == '\n') { }
      else if (c == '>') { if (j > 0) { add(tmp, j); j = 0; } emit(); mode = 1; put(c); }
      else tmp[j++] = tbl[c];
    }
    if (j > 0) add(tmp, j);
  }
  emit();
  flush_out();
  return 0;
}
