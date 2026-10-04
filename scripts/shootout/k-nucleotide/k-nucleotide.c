/* k-nucleotide, C twin: the same algorithm as k-nucleotide.fib (stdin in 64 KB blocks, the THREE sequence as 2-bit codes, every
   k-mer of length 1, 2, 3, 4, 6, 12, 18 counted in a hash table keyed by the k-mer as an integer). C has no hash table: this is an
   open-addressing one (linear probing, multiplicative hash, doubling at half load). gcc -O3 -march=native. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <unistd.h>

#define EMPTY (~(uint64_t)0)
typedef struct { uint64_t *keys; uint32_t *vals; uint64_t cap, used; } table;

static void tinit(table *t, uint64_t cap) {
  t->cap = cap; t->used = 0;
  t->keys = malloc(cap * sizeof(uint64_t)); t->vals = calloc(cap, sizeof(uint32_t));
  memset(t->keys, 0xff, cap * sizeof(uint64_t));
}
static inline uint64_t hash(uint64_t k) { k *= 0x9E3779B97F4A7C15ULL; return k ^ (k >> 32); }
static void grow(table *t) {
  table n; tinit(&n, t->cap * 2);
  for (uint64_t i = 0; i < t->cap; i++) if (t->keys[i] != EMPTY) {
    uint64_t j = hash(t->keys[i]) & (n.cap - 1);
    while (n.keys[j] != EMPTY) j = (j + 1) & (n.cap - 1);
    n.keys[j] = t->keys[i]; n.vals[j] = t->vals[i];
  }
  n.used = t->used; free(t->keys); free(t->vals); *t = n;
}
static inline void incr(table *t, uint64_t key) {
  uint64_t j = hash(key) & (t->cap - 1);
  while (t->keys[j] != EMPTY) { if (t->keys[j] == key) { t->vals[j]++; return; } j = (j + 1) & (t->cap - 1); }
  t->keys[j] = key; t->vals[j] = 1;
  if (++t->used * 2 > t->cap) grow(t);
}
static uint32_t lookup(table *t, uint64_t key) {
  uint64_t j = hash(key) & (t->cap - 1);
  while (t->keys[j] != EMPTY) { if (t->keys[j] == key) return t->vals[j]; j = (j + 1) & (t->cap - 1); }
  return 0;
}

static void count(table *t, const unsigned char *seq, int64_t n, int k) {
  uint64_t mask = (1ULL << (2 * k)) - 1, key = 0;
  tinit(t, 16);
  for (int64_t i = 0; i < n; i++) {
    key = ((key << 2) | seq[i]) & mask;
    if (i >= k - 1) incr(t, key);
  }
}
static void decode(uint64_t key, int k, char *out) {
  for (int i = 0; i < k; i++) out[k - 1 - i] = "ACGT"[(key >> (2 * i)) & 3];
  out[k] = 0;
}
typedef struct { uint64_t key; uint32_t val; } entry;
static int cmp(const void *a, const void *b) {
  const entry *x = a, *y = b;
  if (x->val != y->val) return x->val < y->val ? 1 : -1;
  return x->key < y->key ? -1 : x->key > y->key;
}
static void frequencies(const unsigned char *seq, int64_t n, int k) {
  table t; count(&t, seq, n, k);
  entry *es = malloc(t.used * sizeof(entry)); uint64_t m = 0;
  for (uint64_t i = 0; i < t.cap; i++) if (t.keys[i] != EMPTY) { es[m].key = t.keys[i]; es[m].val = t.vals[i]; m++; }
  qsort(es, m, sizeof(entry), cmp);
  int64_t total = n - k + 1; /* percent: exact integer arithmetic, rounded half up (%.3f of Java and C disagree on exact ties) */
  char buf[32];
  for (uint64_t i = 0; i < m; i++) { decode(es[i].key, k, buf); { int64_t v = ((int64_t)es[i].val * 200000 + (int64_t)total) / (2 * (int64_t)total), fp = v % 1000; printf("%s %lld.%03lld\n", buf, (long long)(v / 1000), (long long)fp); } }
  printf("\n");
  free(es); free(t.keys); free(t.vals);
}
static void count_of(const unsigned char *seq, int64_t n, const char *pat) {
  int k = (int)strlen(pat); uint64_t key = 0;
  for (int i = 0; i < k; i++) key = key * 4 + (strchr("ACGT", pat[i]) - "ACGT");
  table t; count(&t, seq, n, k);
  printf("%u\t%s\n", lookup(&t, key), pat);
  free(t.keys); free(t.vals);
}

int main(void) {
  signed char tbl[256]; memset(tbl, -1, sizeof tbl);
  for (int i = 0; i < 4; i++) { tbl[(int)"ACGT"[i]] = i; tbl[(int)"acgt"[i]] = i; }
  const char *want = ">THREE";
  unsigned char b[65536];
  unsigned char *seq = NULL; int64_t total = 0, cap = 0;
  int mode = 0, hp = 0, ok = 0; ssize_t len;
  while (mode < 3 && (len = read(0, b, sizeof b)) > 0) {
    if (total + len > cap) { cap = cap ? cap * 2 : 1 << 20; if (cap < total + len) cap = total + len; seq = realloc(seq, cap); }
    for (ssize_t i = 0; i < len && mode < 3; i++) {
      int c = b[i];
      if (mode == 1) {
        if (c == '\n') { mode = (ok && hp >= 6) ? 2 : 0; hp = 0; }
        else { if (hp < 6 && c != want[hp]) ok = 0; hp++; }
      } else if (c == '>') { if (mode == 2) mode = 3; else { mode = 1; hp = 1; ok = 1; } }
      else if (mode == 2 && tbl[c] >= 0) seq[total++] = (unsigned char)tbl[c];
    }
  }
  frequencies(seq, total, 1);
  frequencies(seq, total, 2);
  count_of(seq, total, "GGT");
  count_of(seq, total, "GGTA");
  count_of(seq, total, "GGTATT");
  count_of(seq, total, "GGTATTTTAATT");
  count_of(seq, total, "GGTATTTTAATTTATAGT");
  return 0;
}
