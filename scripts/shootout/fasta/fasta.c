/* fasta (Computer Language Benchmarks Game), the C twin: gcc -O3 -march=native fasta.c -o fasta */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define IM 139968
#define IA 3877
#define IC 29573
#define CAP 65536

static const char ALU[] =
    "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG"
    "GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA"
    "CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT"
    "ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA"
    "GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG"
    "AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC"
    "AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA";

static char buf[CAP];
static int pos = 0;

static void flush(void) {
    int off = 0;
    while (off < pos) {
        ssize_t w = write(1, buf + off, pos - off);
        if (w < 0) exit(1);
        off += (int)w;
    }
    pos = 0;
}

static void put_str(const char *s) {
    size_t n = strlen(s);
    memcpy(buf + pos, s, n);
    pos += (int)n;
}

static void repeat(long n) {
    int m = (int)strlen(ALU), k = 0;
    for (long left = n; left > 0;) {
        int w = left < 60 ? (int)left : 60;
        for (int j = 0; j < w; j++) buf[pos + j] = ALU[(k + j) % m];
        buf[pos + w] = '\n';
        pos += w + 1;
        if (pos > CAP - 61) flush();
        left -= w;
        k = (k + w) % m;
    }
}

static void cumulative(const double *p, double *c, int n) {
    double acc = 0.0;
    for (int i = 0; i < n; i++) { acc += p[i]; c[i] = acc; }
}

static int random_fasta(const double *cum, const char *syms, int cnt, long n, int seed) {
    int last = seed, m = cnt - 1;
    for (long left = n; left > 0;) {
        int w = left < 60 ? (int)left : 60;
        for (int j = 0; j < w; j++) {
            last = (last * IA + IC) % IM;
            double r = (double)last / 139968.0;
            int i = 0;
            while (i < m && r >= cum[i]) i++;
            buf[pos + j] = syms[i];
        }
        buf[pos + w] = '\n';
        pos += w + 1;
        if (pos > CAP - 61) flush();
        left -= w;
    }
    return last;
}

int main(int argc, char **argv) {
    long n = argc > 1 ? atol(argv[1]) : 1000;
    double ip[] = {0.27, 0.12, 0.12, 0.27, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02};
    double hp[] = {0.3029549426680, 0.1979883004921, 0.1975473066391, 0.3015094502008};
    double ic[15], hc[4];
    cumulative(ip, ic, 15);
    cumulative(hp, hc, 4);
    put_str(">ONE Homo sapiens alu\n");
    repeat(n * 2);
    put_str(">TWO IUB ambiguity codes\n");
    int s = random_fasta(ic, "acgtBDHKMNRSVWY", 15, n * 3, 42);
    put_str(">THREE Homo sapiens frequency\n");
    random_fasta(hc, "acgt", 4, n * 5, s);
    flush();
    return 0;
}
