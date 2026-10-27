/* Test-only LD_PRELOAD driver. Never packaged or used by the root manager. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <unistd.h>
#include <xf86drm.h>
#include <xf86drmMode.h>

static int devices[4096], opens, writes, failure_used, readback_used, initialized;
static uint16_t live[2][12], initial[2][12];
static int scenario(const char *name) {
    const char *value = getenv("WROS_TEST_DRM_SCENARIO");
    return value && !strcmp(value, name);
}
static void initialize(void) {
    if (initialized) return;
    initialized = 1;
    for (int output = 0; output < 2; ++output)
        for (int channel = 0; channel < 3; ++channel)
            for (int point = 0; point < 4; ++point)
                initial[output][channel * 4 + point] = (uint16_t)(point * (19000 - channel * 400 - output * 100));
    memcpy(live, initial, sizeof live);
}
__attribute__((destructor)) static void report(void) {
    initialize();
    fprintf(stderr, "fake-drm-opens=%d fake-writes=%d fake-restored=%d\n", opens, writes, !memcmp(live, initial, sizeof live));
}
uid_t geteuid(void) { return 0; } /* Kernel privileges are never changed. */
int open(const char *path, int flags, ...) {
    int (*original)(const char *, int, ...) = dlsym(RTLD_NEXT, "open");
    if (!strncmp(path, "/dev/dri/card", 13)) {
        ++opens;
        if (strcmp(path, "/dev/dri/card0")) { errno = ENOENT; return -1; }
        int fd = original("/dev/null", O_RDWR | O_CLOEXEC);
        if (fd >= 0 && fd < 4096) devices[fd] = 1;
        return fd;
    }
    mode_t mode = 0;
    if (flags & O_CREAT) { va_list arguments; va_start(arguments, flags); mode = va_arg(arguments, mode_t); va_end(arguments); }
    return original(path, flags, mode);
}
int __open_2(const char *path, int flags) { return open(path, flags); }
int fstat(int fd, struct stat *info) {
    int (*original)(int, struct stat *) = dlsym(RTLD_NEXT, "fstat");
    int result = original(fd, info);
    if (!result && fd >= 0 && fd < 4096 && devices[fd]) {
        info->st_mode = S_IFCHR | 0600; info->st_rdev = makedev(226, 0);
    }
    return result;
}
int drmIsMaster(int fd) { (void)fd; return 1; }
int drmSetMaster(int fd) { (void)fd; return 0; }
drmModeRes *drmModeGetResources(int fd) {
    (void)fd;
    drmModeRes *resources = calloc(1, sizeof *resources);
    resources->count_crtcs = 2; resources->count_connectors = 2;
    resources->crtcs = calloc(2, sizeof(uint32_t));
    resources->connectors = calloc(2, sizeof(uint32_t));
    for (int n = 0; n < 2; ++n) { resources->crtcs[n] = 100 + n; resources->connectors[n] = 200 + n; }
    return resources;
}
void drmModeFreeResources(drmModeRes *resources) { free(resources->crtcs); free(resources->connectors); free(resources); }
drmModeCrtc *drmModeGetCrtc(int fd, uint32_t id) {
    (void)fd;
    drmModeCrtc *crtc = calloc(1, sizeof *crtc);
    crtc->crtc_id = id; crtc->mode_valid = 1; crtc->width = 640; crtc->height = 480;
    crtc->gamma_size = scenario("unsupported") && id == 101 ? 0 : 4;
    return crtc;
}
void drmModeFreeCrtc(drmModeCrtc *crtc) { free(crtc); }
drmModeConnector *drmModeGetConnector(int fd, uint32_t id) {
    (void)fd;
    drmModeConnector *connector = calloc(1, sizeof *connector);
    connector->connector_id = id + ((scenario("topology") || (scenario("hotplug-after-set") && writes)) ? 5 : 0);
    connector->encoder_id = id + 100;
    connector->connection = scenario("disconnected") ? DRM_MODE_DISCONNECTED : DRM_MODE_CONNECTED;
    connector->connector_type = DRM_MODE_CONNECTOR_eDP;
    connector->connector_type_id = id - 199;
    connector->mmWidth = 280; connector->mmHeight = 170;
    return connector;
}
void drmModeFreeConnector(drmModeConnector *connector) { free(connector); }
drmModeEncoder *drmModeGetEncoder(int fd, uint32_t id) {
    (void)fd;
    drmModeEncoder *encoder = calloc(1, sizeof *encoder);
    encoder->encoder_id = id; encoder->crtc_id = id - 200;
    return encoder;
}
void drmModeFreeEncoder(drmModeEncoder *encoder) { free(encoder); }
int drmModeCrtcGetGamma(int fd, uint32_t crtc, uint32_t size, uint16_t *red, uint16_t *green, uint16_t *blue) {
    (void)fd; initialize();
    if (size != 4 || crtc < 100 || crtc > 101) return -1;
    memcpy(red, live[crtc - 100], 8); memcpy(green, live[crtc - 100] + 4, 8); memcpy(blue, live[crtc - 100] + 8, 8);
    if (scenario("readback") && writes && !readback_used) { ++readback_used; blue[3] ^= 1; }
    return 0;
}
int drmModeCrtcSetGamma(int fd, uint32_t crtc, uint32_t size, const uint16_t *red, const uint16_t *green, const uint16_t *blue) {
    (void)fd; initialize(); ++writes;
    if (size != 4 || crtc < 100 || crtc > 101) return -1;
    if (scenario("rollback-failure") && writes > 2 && crtc == 100) return -1;
    memcpy(live[crtc - 100], red, 8); memcpy(live[crtc - 100] + 4, green, 8);
    if ((scenario("set-failure") || scenario("rollback-failure")) && crtc == 101 && !failure_used) {
        ++failure_used; live[1][8] = blue[0]; live[1][9] = blue[1]; return -1;
    }
    memcpy(live[crtc - 100] + 8, blue, 8);
    return 0;
}
