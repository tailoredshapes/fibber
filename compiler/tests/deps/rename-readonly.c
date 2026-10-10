/* Model BSD's refusal to rename a directory whose root is not writable.
 * Run the real package tests through this guard, including concurrent fetches.
 * Files and writable directories retain the host's ordinary rename behaviour. */
#include <errno.h>
#include <stdio.h>
#include <sys/stat.h>

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
