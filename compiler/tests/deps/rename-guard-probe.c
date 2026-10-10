/* Prove that the guard intercepts this process and does not follow exec. */
#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

static int loaded(void) {
    return dlsym(RTLD_DEFAULT, "fibber_deps_rename_guard_loaded") != NULL;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "child") == 0) {
        if (!loaded()) return 0;
        fputs("rename guard leaked into an exec child\n", stderr);
        return 1;
    }
    if (argc != 2 || !loaded()) {
        fputs("rename guard was not loaded into the parent\n", stderr);
        return 1;
    }
    if (chdir(argv[1]) || mkdir("guard-probe-root", 0700) ||
        chmod("guard-probe-root", 0500)) { perror("guard probe setup"); return 1; }
    errno = 0;
    if (rename("guard-probe-root", "guard-probe-moved") != -1 || errno != EACCES) {
        fputs("rename guard did not reject a read-only directory\n", stderr);
        return 1;
    }
    pid_t child = fork();
    if (child == -1) { perror("fork"); return 1; }
    if (child == 0) {
        execl(argv[0], argv[0], "child", (char *)NULL);
        perror("exec guard probe");
        _exit(1);
    }
    int verdict;
    if (waitpid(child, &verdict, 0) != child || !WIFEXITED(verdict) ||
        WEXITSTATUS(verdict) != 0) return 1;
    puts("deps: rename guard intercepts parent; exec child is unmodified");
    return 0;
}
