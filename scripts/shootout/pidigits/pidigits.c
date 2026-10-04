/* pidigits: the streaming spigot of the Benchmarks Game with GMP (mpz), as its C entries write it.
   gcc -O3 -march=native pidigits.c -lgmp -o pidigits */
#include <gmp.h>
#include <stdio.h>
#include <stdlib.h>

static mpz_t numer, accum, denom, tmp1, tmp2;

static int extract_digit(unsigned nth) {
    mpz_mul_ui(tmp1, numer, nth);
    mpz_add(tmp2, tmp1, accum);
    mpz_tdiv_q(tmp1, tmp2, denom);
    return (int) mpz_get_ui(tmp1);
}

static void next_term(unsigned k) {
    unsigned y2 = k * 2 + 1;
    mpz_mul_2exp(tmp1, numer, 1);
    mpz_add(accum, accum, tmp1);
    mpz_mul_ui(accum, accum, y2);
    mpz_mul_ui(numer, numer, k);
    mpz_mul_ui(denom, denom, y2);
}

static void eliminate_digit(unsigned d) {
    mpz_submul_ui(accum, denom, d);
    mpz_mul_ui(accum, accum, 10);
    mpz_mul_ui(numer, numer, 10);
}

int main(int argc, char **argv) {
    int n = atoi(argv[1]), i = 0, k = 0, len = 0;
    char line[16];
    mpz_init(tmp1);
    mpz_init(tmp2);
    mpz_init_set_ui(numer, 1);
    mpz_init_set_ui(accum, 0);
    mpz_init_set_ui(denom, 1);
    while (i < n) {
        k++;
        next_term(k);
        if (mpz_cmp(numer, accum) > 0) continue;
        int d = extract_digit(3);
        if (d != extract_digit(4)) continue;
        line[len++] = (char) ('0' + d);
        i++;
        if (i % 10 == 0) {
            line[len] = 0;
            printf("%s\t:%d\n", line, i);
            len = 0;
        }
        eliminate_digit(d);
    }
    if (len > 0) {
        while (len < 10) line[len++] = ' ';
        line[len] = 0;
        printf("%s\t:%d\n", line, n);
    }
    return 0;
}
