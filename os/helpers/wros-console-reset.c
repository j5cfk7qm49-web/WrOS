/* Restore Debian's prompt even when the graphical renderer crashed. No input
 * is read and no password, command, path or environment is accepted. MIT. */
#include <fcntl.h>
#include <linux/kd.h>
#include <linux/vt.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

int main(void) {
    int fd = open("/dev/console", O_RDWR | O_NOCTTY | O_CLOEXEC);
    if (fd < 0) return 1;
    struct vt_mode mode = { .mode = VT_AUTO };
    int result = ioctl(fd, VT_SETMODE, &mode);
    result |= ioctl(fd, KDSETMODE, KD_TEXT);
    result |= ioctl(fd, KDSKBMODE, K_UNICODE);
    /* A killed renderer cannot restore cfmakeraw's input flags. Debian askpass
     * enables canonical input itself, but expects CR-to-newline conversion to
     * be present already. Keep echo off throughout the handoff. */
    struct termios terminal;
    if (tcgetattr(fd, &terminal) < 0) {
        result = -1;
    } else {
        terminal.c_iflag &= ~(IGNCR | INLCR | ISTRIP);
        terminal.c_iflag |= ICRNL | IUTF8;
        terminal.c_oflag |= OPOST | ONLCR;
        terminal.c_lflag &= ~(ECHO | ECHONL);
        terminal.c_lflag |= ICANON | ISIG | IEXTEN;
        terminal.c_cc[VMIN] = 1;
        terminal.c_cc[VTIME] = 0;
        result |= tcsetattr(fd, TCSAFLUSH, &terminal);
    }
    close(fd);
    return result < 0;
}
