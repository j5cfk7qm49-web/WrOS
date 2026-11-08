/* Restore Debian's prompt even when the graphical renderer crashed. No input
 * is read and no password, command, path or environment is accepted. MIT. */
#include <fcntl.h>
#include <linux/kd.h>
#include <linux/vt.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(void) {
    int fd = open("/dev/console", O_RDWR | O_NOCTTY | O_CLOEXEC);
    if (fd < 0) return 1;
    struct vt_mode mode = { .mode = VT_AUTO };
    int result = ioctl(fd, VT_SETMODE, &mode);
    result |= ioctl(fd, KDSETMODE, KD_TEXT);
    result |= ioctl(fd, KDSKBMODE, K_UNICODE);
    close(fd);
    return result < 0;
}
