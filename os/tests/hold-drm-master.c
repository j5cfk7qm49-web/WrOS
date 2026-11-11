/* Disposable VM regression only: simulate the colour helper's brief ownership. */
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <stdio.h>
#include <time.h>
#include <unistd.h>
#include <xf86drm.h>

int main(void) {
    char path[64];
    int fd = -1;
    for (int n = 0; n < 16; ++n) {
        snprintf(path, sizeof path, "/dev/dri/card%d", n);
        fd = open(path, O_RDWR | O_CLOEXEC);
        if (fd < 0) continue;
        if (drmIsMaster(fd) || drmSetMaster(fd) == 0) break;
        close(fd);
        fd = -1;
    }
    if (fd < 0) return 1;
    puts("holding master");
    fflush(stdout);
    struct timespec delay = {.tv_sec = 1, .tv_nsec = 500000000};
    nanosleep(&delay, NULL);
    int result = drmDropMaster(fd);
    close(fd);
    puts("released master");
    return result ? 1 : 0;
}
