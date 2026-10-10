/* Model BSD's refusal to rename a directory whose root is not writable.
 * Run the real package tests through this guard, including concurrent fetches.
 * Files and writable directories retain the host's ordinary rename behaviour. */
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>

const int fibber_deps_rename_guard_loaded = 1;

/* Interpose only the compiler, not its git/clang/chmod children. System tools
 * can use a different Mach-O subtype (arm64e) from this arm64 test library. */
__attribute__((constructor)) static void scope_to_process(void) {
#ifdef __APPLE__
    unsetenv("DYLD_INSERT_LIBRARIES");
#else
    unsetenv("LD_PRELOAD");
#endif
}

static int readonly_directory(const char *path) {
    struct stat st;
    return stat(path, &st) == 0 && S_ISDIR(st.st_mode) && !(st.st_mode & S_IWUSR);
}

#ifdef __APPLE__
static int guarded_rename(const char *from, const char *to) {
    if (readonly_directory(from)) { errno = EACCES; return -1; }
    return rename(from, to);
}
__attribute__((used, section("__DATA,__interpose")))
static const struct { const void *replacement; const void *original; } guard = {
    (const void *)guarded_rename, (const void *)rename
};
#else
#include <dlfcn.h>
int rename(const char *from, const char *to) {
    if (readonly_directory(from)) { errno = EACCES; return -1; }
    int (*real_rename)(const char *, const char *) = dlsym(RTLD_NEXT, "rename");
    return real_rename(from, to);
}
#endif
