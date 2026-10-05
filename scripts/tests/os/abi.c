#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <poll.h>
#include <stddef.h>
#include <stdio.h>
#include <sys/socket.h>
#include <sys/utsname.h>
#include <time.h>
#include <unistd.h>

int main(void) {
    const long values[] = {
        O_CREAT, O_EXCL, O_TRUNC, O_APPEND, O_NONBLOCK, O_CLOEXEC,
        AT_FDCWD, CLOCK_MONOTONIC, EAGAIN, ETIMEDOUT, ECONNABORTED,
        ECONNRESET, ECONNREFUSED, ENOTSUP, EADDRINUSE, SOL_SOCKET,
        SO_REUSEADDR, MSG_NOSIGNAL, AF_INET6,
#ifdef __APPLE__
        1,
#else
        0,
#endif
        sizeof(((struct utsname *)0)->sysname), offsetof(struct dirent, d_type),
        offsetof(struct dirent, d_name), _SC_NPROCESSORS_ONLN, EINPROGRESS, SO_ERROR,
        sizeof(long), sizeof(void *), sizeof(struct timespec), sizeof(struct pollfd),
        sizeof(struct sockaddr_in), sizeof(struct sockaddr_in6),
        offsetof(struct sockaddr_in, sin_port), offsetof(struct sockaddr_in, sin_addr),
        offsetof(struct sockaddr_in6, sin6_addr)
    };
    for (size_t i = 0; i < sizeof(values) / sizeof(values[0]); ++i) {
        printf("%ld\n", values[i]);
    }
    return 0;
}
