#include <stdio.h>
#include <stdlib.h>
int main(int c, char **v) {
    long n = atol(v[1]), acc = 0;
    for (long i = 0; i < n; i++) acc += (i * i) % 1000003;
    printf("%ld\ndone\n", acc);
    return 0;
}
