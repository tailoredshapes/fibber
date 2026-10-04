/* binary-trees, the Computer Language Benchmarks Game: allocate and walk complete binary trees, no pooling (malloc/free).
   usage: binary-trees N     gcc -O3 -march=native */
#include <stdio.h>
#include <stdlib.h>

typedef struct Node { struct Node *l, *r; } Node;

static Node *make(int d) {
    Node *n = malloc(sizeof(Node));
    if (d == 0) { n->l = n->r = NULL; } else { n->l = make(d - 1); n->r = make(d - 1); }
    return n;
}
static long check(const Node *t) { return t->l ? 1 + check(t->l) + check(t->r) : 1; }
static void drop(Node *t) { if (t->l) { drop(t->l); drop(t->r); } free(t); }

int main(int argc, char **argv) {
    int n = argc > 1 ? atoi(argv[1]) : 10;
    int min_depth = 4, max_depth = n > min_depth + 2 ? n : min_depth + 2, stretch = max_depth + 1;
    Node *t = make(stretch);
    printf("stretch tree of depth %d\t check: %ld\n", stretch, check(t));
    drop(t);
    Node *long_lived = make(max_depth);
    for (int d = min_depth; d <= max_depth; d += 2) {
        long iters = 1L << (max_depth - d + min_depth), sum = 0;
        for (long i = 0; i < iters; i++) { Node *x = make(d); sum += check(x); drop(x); }
        printf("%ld\t trees of depth %d\t check: %ld\n", iters, d, sum);
    }
    printf("long lived tree of depth %d\t check: %ld\n", max_depth, check(long_lived));
    drop(long_lived);
    return 0;
}
