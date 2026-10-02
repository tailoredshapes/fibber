/*
 * A C program using liblair.so through lair.h and nothing else
 * (spec/compiler.md section 9). tests/c_consumer.rs compiles it, runs it
 * and compares its output with what the Rust API says; it also runs
 * under AddressSanitizer, so every handle and error it takes is freed
 * here or the leak is reported.
 *
 * usage: consumer SRC_DIR TMP_DIR   (SRC_DIR holds math.lir and hooked.lir)
 */
#define _POSIX_C_SOURCE 200809L
#include "lair.h"

#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>

#define CHECK(c)                                                              \
    do {                                                                      \
        if (!(c)) {                                                           \
            fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__, #c); \
            exit(1);                                                          \
        }                                                                     \
    } while (0)

/* Take an error: its text as a malloc'd copy (the caller frees it), or NULL
 * for success; the error itself is freed here. */
static char *take(lair_error *e) {
    if (e == NULL) {
        return NULL;
    }
    size_t len = 12345;
    const char *t = lair_error_text(e, &len);
    CHECK(t != NULL && t[len] == '\0' && strlen(t) == len);
    char *copy = malloc(len + 1);
    CHECK(copy != NULL);
    memcpy(copy, t, len + 1);
    lair_error_free(e);
    return copy;
}

/* An operation that must succeed. */
#define OK(expr)                                                              \
    do {                                                                      \
        char *m_ = take(expr);                                                \
        if (m_ != NULL) {                                                     \
            fprintf(stderr, "%s:%d: %s failed: %s\n", __FILE__, __LINE__, #expr, m_); \
            exit(1);                                                          \
        }                                                                     \
    } while (0)

/* An operation that must fail: its text, printed under `label`. */
static void fails(const char *label, lair_error *e) {
    char *m = take(e);
    CHECK(m != NULL);
    printf("%s: %s\n", label, m);
    free(m);
}

static char *read_file(const char *dir, const char *name) {
    char path[1024];
    snprintf(path, sizeof path, "%s/%s", dir, name);
    FILE *f = fopen(path, "rb");
    CHECK(f != NULL);
    CHECK(fseek(f, 0, SEEK_END) == 0);
    long n = ftell(f);
    CHECK(n >= 0 && fseek(f, 0, SEEK_SET) == 0);
    char *buf = malloc((size_t)n + 1);
    CHECK(buf != NULL && fread(buf, 1, (size_t)n, f) == (size_t)n);
    buf[n] = '\0';
    fclose(f);
    return buf;
}

#define STR(s) (s), strlen(s)

static lair_jit *new_jit(int opt) {
    lair_jit *j = NULL;
    OK(lair_jit_new(opt, &j));
    CHECK(j != NULL);
    return j;
}

static void add(lair_jit *j, const char *name, const char *src) {
    OK(lair_jit_add_source(j, STR(name), STR(src)));
}

static size_t address(lair_jit *j, const char *name) {
    size_t a = 0;
    OK(lair_jit_address(j, STR(name), &a));
    CHECK(a != 0);
    return a;
}

static size_t c_entry(lair_jit *j, const char *name) {
    size_t a = 0;
    OK(lair_jit_c_entry(j, STR(name), &a));
    return a;
}

/* ---- a session, calling addresses ---------------------------------- */

static void session(const char *dir) {
    char *math = read_file(dir, "math.lir");
    lair_jit *j = new_jit(0);
    printf("session: new ok\n");
    add(j, "math", math);

    int64_t arg = -12;
    printf("square(-12) = %lld\n", (long long)lair_call_i64(address(j, "square"), &arg, 1));
    printf("c_entry(square) == address: %s\n",
           c_entry(j, "square") == address(j, "square") ? "yes" : "no");
    size_t raw = address(j, "count"), entry = c_entry(j, "count");
    printf("c_entry(count) != address: %s\n", entry != raw ? "yes" : "no");
    CHECK(c_entry(j, "count") == entry);
    int64_t count_args[2] = {1000000, 0};
    printf("count(1000000, 0) = %lld\n", (long long)lair_call_i64(entry, count_args, 2));

    size_t out = 99;
    char *m = take(lair_jit_address(j, STR("nope"), &out));
    CHECK(m != NULL && out == 99);
    printf("address(nope): %s\n", m);
    free(m);

    /* Errors: the module is refused whole and the session lives on. */
    fails("add invalid", lair_jit_add_source(j, STR("invalid"),
              STR("(define (h i32) () (block entry (ret (i64 1))))")));
    fails("add duplicate", lair_jit_add_source(j, STR("dup"),
              STR("(define (square i64) ((i64 x)) (block entry (ret x)))")));
    fails("add syntax", lair_jit_add_source(j, STR("syntax"), STR("(define (broken i64")));
    arg = 9;
    printf("session still usable: square(9) = %lld\n",
           (long long)lair_call_i64(address(j, "square"), &arg, 1));
    lair_jit_free(j);
    free(math);
}

static void arity(void) {
    char mod[16384];
    size_t used = 0;
    for (int k = 0; k <= 8; k++) {
        char params[256] = "", w[1024];
        for (int i = 0; i < k; i++) {
            char p[32];
            snprintf(p, sizeof p, "(i64 a%d) ", i);
            strcat(params, p);
        }
        if (k == 0) {
            snprintf(w, sizeof w, "(i64 42)");
        } else {
            snprintf(w, sizeof w, "(i64 0)");
            long long ten = 1;
            for (int i = 0; i < k - 1; i++) {
                ten *= 10;
            }
            for (int i = 0; i < k; i++) {
                char next[1200];
                snprintf(next, sizeof next, "(add %s (mul a%d (i64 %lld)))", w, i, ten);
                strcpy(w, next);
                ten /= 10;
            }
        }
        used += (size_t)snprintf(mod + used, sizeof mod - used,
            "(define (f%d i64) (%s) (block entry (ret %s)))\n"
            "(define (g%d double) (%s) (block entry (ret (fdiv (sitofp double %s) (double 4.0)))))\n",
            k, params, w, k, params, w);
        CHECK(used < sizeof mod);
    }
    snprintf(mod + used, sizeof mod - used,
        "(define (deref i64) ((ptr p) (i64 k)) (block entry (ret (add (load i64 p) k))))\n");
    lair_jit *j = new_jit(3);
    add(j, "arity", mod);
    for (int k = 0; k <= 8; k++) {
        int64_t args[8];
        for (int i = 0; i < k; i++) {
            args[i] = (i % 2 == 0) ? i + 1 : -(i + 1);
        }
        char name[16];
        snprintf(name, sizeof name, "f%d", k);
        printf("f%d = %lld\n", k, (long long)lair_call_i64(address(j, name), args, (size_t)k));
        snprintf(name, sizeof name, "g%d", k);
        printf("g%d = %.2f\n", k, lair_call_f64(address(j, name), args, (size_t)k));
    }
    int64_t cell = 1000;
    int64_t dargs[2] = {(int64_t)(intptr_t)&cell, 7};
    printf("deref = %lld\n", (long long)lair_call_i64(address(j, "deref"), dargs, 2));
    int64_t nine[9] = {1, 1, 1, 1, 1, 1, 1, 1, 1}; /* f3(1, 1, 1) would be 111 */
    printf("refused calls: %lld %lld %lld %.1f\n",
           (long long)lair_call_i64(address(j, "f3"), nine, 9),
           (long long)lair_call_i64(address(j, "f3"), NULL, 3),
           (long long)lair_call_i64(0, nine, 3), lair_call_f64(0, nine, 3));
    lair_jit_free(j);
}

/* ---- check, executables, null arguments ----------------------------- */

static void checks_and_executables(const char *dir, const char *tmp) {
    char *math = read_file(dir, "math.lir");
    OK(lair_check_source(STR(math)));
    printf("check valid: ok\n");
    fails("check invalid",
          lair_check_source(STR("(define (h i32) () (block entry (ret (i64 1))))")));
    free(math);

    const char *prog = "(declare printf i32 (ptr ...))\n"
        "(define (main i32) () (block entry (call @printf (string \"hello from lair\\n\")) (ret (i32 37))))";
    char exe[1024], cmd[1100];
    snprintf(exe, sizeof exe, "%s/hello", tmp);
    const char *libs[2] = {"m", "c"};
    size_t lens[2] = {1, 1};
    OK(lair_build_executable(STR(prog), STR(exe), 2, libs, lens, 2));
    FILE *p = popen(exe, "r");
    CHECK(p != NULL);
    char line[128] = "";
    CHECK(fgets(line, sizeof line, p) != NULL);
    int status = pclose(p);
    CHECK(WIFEXITED(status));
    printf("exe: status %d, said %s", WEXITSTATUS(status), line);
    snprintf(cmd, sizeof cmd, "%s/hello", tmp);
    remove(cmd);

    fails("exe without main", lair_build_executable(
              STR("(define (square i64) ((i64 x)) (block entry (ret (mul x x))))"),
              STR(exe), 0, NULL, NULL, 0));
    fails("exe bad level", lair_build_executable(STR(prog), STR(exe), 9, NULL, NULL, 0));
    const char *dirs[1] = {"/no/such/lair/library/directory"};
    size_t dir_lens[1] = {strlen(dirs[0])};
    fails("exe with a missing library directory",
          lair_build_executable_with(STR(prog), STR(exe), 0, NULL, NULL, 0, dirs, dir_lens, 1));
    OK(lair_build_executable_with(STR(prog), STR(exe), 0, NULL, NULL, 0, NULL, NULL, 0));
    remove(exe);

    lair_jit *j = NULL;
    lair_error *e = lair_jit_new(7, &j);
    CHECK(j == NULL);
    fails("bad opt level", e);
    fails("null jit", lair_jit_add_source(NULL, STR("m"), STR("")));
    fails("null src", lair_check_source(NULL, 3));
    fails("null out", lair_jit_new(0, NULL));
    lair_jit_free(NULL);
    lair_error_free(NULL);
    size_t len = 5;
    CHECK(strcmp(lair_error_text(NULL, &len), "") == 0 && len == 0);
    printf("null arguments: ok\n");
}

/* ---- the mailbox ---------------------------------------------------- */

typedef struct {
    lair_jit *jit;
    size_t install;
} hooked;

static hooked hooked_new(const char *dir, int opt) {
    char *src = read_file(dir, "hooked.lir");
    hooked h;
    h.jit = new_jit(opt);
    add(h.jit, "hooked", src);
    h.install = address(h.jit, "install");
    free(src);
    return h;
}

static void hooked_install(const hooked *h, lair_call *c) {
    int64_t args[3] = {(int64_t)lair_hook1_address(), (int64_t)lair_hook2_address(),
                       (int64_t)(intptr_t)c};
    lair_call_i64(h->install, args, 3);
}

/* Run fn(arg) on a worker and answer every hook call: arity 1 with
 * 3a+1, arity 2 with 100a+b. Returns the call's result; counts requests. */
static int64_t serve(lair_call *c, int verbose, size_t *requests) {
    for (;;) {
        int k = lair_call_wait(c);
        if (k == 0) {
            return lair_call_result(c);
        }
        CHECK(k == 1 || k == 2);
        int64_t a = lair_call_hook_arg(c, 0);
        int64_t answer;
        if (k == 1) {
            answer = 3 * a + 1;
            if (verbose) printf("  hook1(%lld) -> %lld\n", (long long)a, (long long)answer);
        } else {
            int64_t b = lair_call_hook_arg(c, 1);
            answer = 100 * a + b;
            if (verbose) printf("  hook2(%lld, %lld) -> %lld\n", (long long)a, (long long)b, (long long)answer);
        }
        (*requests)++;
        lair_call_hook_reply(c, answer);
    }
}

static int64_t run(hooked *h, lair_call *c, const char *fn, int64_t arg, int verbose, size_t *n) {
    lair_call_start(c, address(h->jit, fn), &arg, 1);
    return serve(c, verbose, n);
}

static void mailbox(const char *dir) {
    hooked h = hooked_new(dir, 0);
    lair_call *c = lair_call_new();
    CHECK(c != NULL);
    hooked_install(&h, c);
    size_t n = 0;
    printf("mailbox one(20):\n");
    printf("  result %lld\n", (long long)run(&h, c, "one", 20, 1, &n));
    int64_t two_args[2] = {5, 6};
    lair_call_start(c, address(h.jit, "two"), two_args, 2);
    printf("mailbox two(5, 6):\n");
    printf("  result %lld\n", (long long)serve(c, 1, &n));
    printf("mailbox both(4):\n");
    printf("  result %lld\n", (long long)run(&h, c, "both", 4, 1, &n));
    n = 0;
    int64_t r = run(&h, c, "many", 1000, 0, &n);
    printf("mailbox many(1000): %zu requests, result %lld\n", n, (long long)r);
    CHECK(lair_call_wait(c) == 0 && lair_call_result(c) == r);

    /* Two calls at once, one mailbox each; answered in the other order. */
    lair_call *a = lair_call_new(), *b = lair_call_new();
    size_t ask = address(h.jit, "ask1cx");
    int64_t aa[3] = {(int64_t)lair_hook1_address(), (int64_t)(intptr_t)a, 10};
    int64_t bb[3] = {(int64_t)lair_hook1_address(), (int64_t)(intptr_t)b, 20};
    lair_call_start(a, ask, aa, 3);
    lair_call_start(b, ask, bb, 3);
    CHECK(lair_call_wait(a) == 1 && lair_call_wait(b) == 1);
    CHECK(lair_call_hook_arg(a, 0) == 10 && lair_call_hook_arg(b, 0) == 20);
    lair_call_hook_reply(b, 2000);
    lair_call_hook_reply(a, 1000);
    CHECK(lair_call_wait(a) == 0 && lair_call_wait(b) == 0);
    printf("mailbox two at once: %lld %lld\n", (long long)lair_call_result(a),
           (long long)lair_call_result(b));
    lair_call_free(a);
    lair_call_free(b);

    /* Out of order: a fault, said once by wait, and the mailbox recovers. */
    lair_call *m = lair_call_new();
    lair_call_hook_reply(m, 5);
    size_t len = 0;
    const char *f = lair_call_fault(m, &len);
    CHECK(f != NULL && f[len] == '\0');
    printf("mailbox misuse: %s; wait gives %d\n", f, lair_call_wait(m));
    CHECK(lair_call_fault(NULL, &len) != NULL);
    hooked_install(&h, m);
    n = 0;
    r = run(&h, m, "one", 1, 0, &n);
    CHECK(lair_call_fault(m, NULL) == NULL);
    printf("mailbox after the fault: result %lld\n", (long long)r);
    lair_call_free(m);
    lair_call_free(c);
    lair_jit_free(h.jit);
}

typedef struct {
    const char *dir;
    int id;
    int64_t result;
    size_t requests;
} job;

static void *thread_main(void *arg) {
    job *jb = arg;
    hooked h = hooked_new(jb->dir, jb->id);
    lair_call *c = lair_call_new();
    hooked_install(&h, c);
    int64_t total = 0;
    for (int round = 0; round < 3; round++) {
        total += run(&h, c, "many", 300, 0, &jb->requests);
    }
    jb->result = total;
    lair_call_free(c);
    lair_jit_free(h.jit);
    return NULL;
}

static void threads(const char *dir) {
    pthread_t t[2];
    job jobs[2] = {{dir, 0, 0, 0}, {dir, 1, 0, 0}};
    for (int i = 0; i < 2; i++) {
        CHECK(pthread_create(&t[i], NULL, thread_main, &jobs[i]) == 0);
    }
    for (int i = 0; i < 2; i++) {
        CHECK(pthread_join(t[i], NULL) == 0);
    }
    printf("threads: %zu + %zu requests, results %lld %lld\n", jobs[0].requests,
           jobs[1].requests, (long long)jobs[0].result, (long long)jobs[1].result);
}

int main(int argc, char **argv) {
    CHECK(argc == 3);
    setvbuf(stdout, NULL, _IOLBF, 0);
    session(argv[1]);
    arity();
    checks_and_executables(argv[1], argv[2]);
    mailbox(argv[1]);
    threads(argv[1]);
    printf("done\n");
    return 0;
}
