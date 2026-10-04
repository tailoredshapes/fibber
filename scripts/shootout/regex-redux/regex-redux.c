/* regex-redux, the Computer Language Benchmarks Game, in C on PCRE2 with its JIT, as the published C entry is.
   The container has libpcre2-8.so.0 and no pcre2.h, so the few declarations used are written out here (they are the
   ones of pcre2.h) and the library is opened with dlopen: the program builds with the plain
     gcc -O3 -march=native -o regex-redux regex-redux.c -lm */
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef struct pcre2_real_code_8 pcre2_code;
typedef struct pcre2_real_match_data_8 pcre2_match_data;
static pcre2_code *(*pcre2_compile_8)(const unsigned char *, size_t, uint32_t, int *, size_t *, void *);
static int (*pcre2_jit_compile_8)(pcre2_code *, uint32_t);
static pcre2_match_data *(*pcre2_match_data_create_from_pattern_8)(const pcre2_code *, void *);
static int (*pcre2_jit_match_8)(const pcre2_code *, const unsigned char *, size_t, size_t, uint32_t, pcre2_match_data *, void *);
static size_t *(*pcre2_get_ovector_pointer_8)(pcre2_match_data *);
#define PCRE2_JIT_COMPLETE 1u

static void load_pcre2(void) {
    void *h = dlopen("libpcre2-8.so.0", RTLD_NOW);
    if (!h) { fprintf(stderr, "no libpcre2-8.so.0\n"); exit(2); }
    *(void **)&pcre2_compile_8 = dlsym(h, "pcre2_compile_8");
    *(void **)&pcre2_jit_compile_8 = dlsym(h, "pcre2_jit_compile_8");
    *(void **)&pcre2_match_data_create_from_pattern_8 = dlsym(h, "pcre2_match_data_create_from_pattern_8");
    *(void **)&pcre2_jit_match_8 = dlsym(h, "pcre2_jit_match_8");
    *(void **)&pcre2_get_ovector_pointer_8 = dlsym(h, "pcre2_get_ovector_pointer_8");
    if (!pcre2_compile_8 || !pcre2_jit_compile_8 || !pcre2_match_data_create_from_pattern_8 || !pcre2_jit_match_8 ||
        !pcre2_get_ovector_pointer_8) { fprintf(stderr, "pcre2 symbols missing\n"); exit(2); }
}

static const char *variants[] = {
    "agggtaaa|tttaccct", "[cgt]gggtaaa|tttaccc[acg]", "a[act]ggtaaa|tttacc[agt]t",
    "ag[act]gtaaa|tttac[agt]ct", "agg[act]taaa|ttta[agt]cct", "aggg[acg]aaa|ttt[cgt]ccct",
    "agggt[cgt]aa|tt[acg]accct", "agggta[cgt]a|t[acg]taccct", "agggtaa[cgt]|[acg]ttaccct"};
static const char *subst[][2] = {{"tHa[Nt]", "<4>"}, {"aND|caN|Ha[DS]|WaS", "<3>"}, {"a[NSt]|BY", "<2>"},
                                 {"<[^>]*>", "|"}, {"\\|[^|][^|]*\\|", "-"}};

typedef struct { pcre2_code *re; pcre2_match_data *md; } Re;

static Re compile(const char *p) {
    int err; size_t off;
    Re r;
    r.re = pcre2_compile_8((const unsigned char *)p, strlen(p), 0, &err, &off, NULL);
    if (!r.re) { fprintf(stderr, "bad pattern %s\n", p); exit(2); }
    pcre2_jit_compile_8(r.re, PCRE2_JIT_COMPLETE);
    r.md = pcre2_match_data_create_from_pattern_8(r.re, NULL);
    return r;
}

/* the next match at or after from: 1 and its span in *s, *e, or 0 */
static int find(Re *r, const unsigned char *t, size_t n, size_t from, size_t *s, size_t *e) {
    if (pcre2_jit_match_8(r->re, t, n, from, 0, r->md, NULL) < 0) return 0;
    size_t *ov = pcre2_get_ovector_pointer_8(r->md);
    *s = ov[0]; *e = ov[1];
    return 1;
}

static size_t count(Re *r, const unsigned char *t, size_t n) {
    size_t c = 0, from = 0, s, e;
    while (from <= n && find(r, t, n, from, &s, &e)) { c++; from = e > s ? e : e + 1; }
    return c;
}

/* every match replaced by repl (no group references in these templates); the new text in a malloc'd buffer */
static unsigned char *replace(Re *r, const unsigned char *t, size_t n, const char *repl, size_t *outn) {
    size_t rl = strlen(repl), cap = n + 1024, o = 0, last = 0, from = 0, s, e;
    unsigned char *out = malloc(cap);
    while (from <= n && find(r, t, n, from, &s, &e)) {
        if (o + (s - last) + rl + 1 > cap) { cap = cap * 2 + s - last + rl; out = realloc(out, cap); }
        memcpy(out + o, t + last, s - last); o += s - last;
        memcpy(out + o, repl, rl); o += rl;
        last = e; from = e > s ? e : e + 1;
    }
    if (o + (n - last) + 1 > cap) { cap = o + (n - last) + 1; out = realloc(out, cap); }
    memcpy(out + o, t + last, n - last); o += n - last;
    *outn = o;
    return out;
}

int main(void) {
    load_pcre2();
    size_t cap = 1 << 20, n = 0;
    unsigned char *in = malloc(cap);
    ssize_t k;
    while ((k = read(0, in + n, cap - n)) > 0) { n += (size_t)k; if (n == cap) in = realloc(in, cap *= 2); }
    size_t ilen = n, clen;
    Re strip = compile(">.*\n|\n");
    unsigned char *seq = replace(&strip, in, n, "", &clen);
    free(in);
    for (int i = 0; i < 9; i++) { Re r = compile(variants[i]); printf("%s %zu\n", variants[i], count(&r, seq, clen)); }
    size_t len = clen;
    for (int i = 0; i < 5; i++) {
        Re r = compile(subst[i][0]);
        size_t nl;
        unsigned char *next = replace(&r, seq, len, subst[i][1], &nl);
        free(seq); seq = next; len = nl;
    }
    printf("\n%zu\n%zu\n%zu\n", ilen, clen, len);
    return 0;
}
