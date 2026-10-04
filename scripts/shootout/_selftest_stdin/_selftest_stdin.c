#include <stdio.h>
int main(void) {
    static char buf[65536];
    long total = 0, n;
    while ((n = (long)fread(buf, 1, sizeof buf, stdin)) > 0) total += n;
    printf("%ld\ndone\n", total);
    return 0;
}
