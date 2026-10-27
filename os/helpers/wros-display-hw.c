/* WrOS narrow DRM bridge, MIT. No modesetting, paths or commands from clients.
 * Root manager alone owns snapshots. Capture all active outputs or fail; apply
 * only to the same topology, verify readback and roll back a partial failure.
 * This executable is not setuid and is not the polkit authorization endpoint.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <unistd.h>
#include <xf86drm.h>
#include <xf86drmMode.h>

#define MAX_OUTPUTS 32
#define MAX_POINTS 4096
#define MAX_BYTES (1024 * 1024)
static const unsigned char MAGIC[8] = {'W','r','O','S','G','M','1',0};
static volatile sig_atomic_t interrupted;
typedef struct {
    uint32_t card, crtc, width, height, points;
    uint64_t topology;
    uint16_t *ramps;
    int fd;
} Output;
typedef struct { uint32_t count; Output outputs[MAX_OUTPUTS]; } Snapshot;
static void on_signal(int number) { (void)number; interrupted = 1; }
static int fail(const char *message) { fprintf(stderr, "%s\n", message); return -1; }
static void release(Snapshot *snapshot) {
    for (uint32_t n = 0; n < snapshot->count; ++n) {
        free(snapshot->outputs[n].ramps);
        if (snapshot->outputs[n].fd >= 0) close(snapshot->outputs[n].fd);
    }
    snapshot->count = 0;
}
static uint64_t hash_bytes(uint64_t hash, const void *bytes, size_t count) {
    const unsigned char *data = bytes;
    for (size_t n = 0; n < count; ++n) { hash ^= data[n]; hash *= UINT64_C(1099511628211); }
    return hash;
}
/* A hardware-change detector, not a cryptographic authentication claim. */
static int identity(int fd, drmModeRes *resources, uint32_t crtc, uint64_t *result) {
    uint64_t hash = UINT64_C(14695981039346656037);
    int found = 0;
    if (resources->count_connectors < 0 || resources->count_connectors > 256)
        return fail("Unbounded DRM connector inventory");
    for (int n = 0; n < resources->count_connectors; ++n) {
        drmModeConnector *connector = drmModeGetConnector(fd, resources->connectors[n]);
        if (!connector) return fail("Unable to inspect DRM connector");
        drmModeEncoder *encoder = connector->encoder_id ? drmModeGetEncoder(fd, connector->encoder_id) : NULL;
        if (connector->connection == DRM_MODE_CONNECTED && encoder && encoder->crtc_id == crtc) {
            uint32_t tags[] = {connector->connector_id, connector->connector_type,
                connector->connector_type_id, connector->mmWidth, connector->mmHeight};
            hash = hash_bytes(hash, tags, sizeof tags);
            ++found;
            if (connector->count_props < 0 || connector->count_props > 256) {
                drmModeFreeEncoder(encoder); drmModeFreeConnector(connector);
                return fail("Unbounded DRM connector properties");
            }
            for (int p = 0; p < connector->count_props; ++p) {
                drmModePropertyRes *property = drmModeGetProperty(fd, connector->props[p]);
                if (!property) { drmModeFreeEncoder(encoder); drmModeFreeConnector(connector); return fail("Unreadable DRM property"); }
                if (!strcmp(property->name, "EDID") && connector->prop_values[p]) {
                    drmModePropertyBlobRes *blob = drmModeGetPropertyBlob(fd, (uint32_t)connector->prop_values[p]);
                    if (!blob || blob->length > 65536) {
                        if (blob) drmModeFreePropertyBlob(blob);
                        drmModeFreeProperty(property); drmModeFreeEncoder(encoder); drmModeFreeConnector(connector);
                        return fail("Unreadable display identity");
                    }
                    hash = hash_bytes(hash, blob->data, blob->length);
                    drmModeFreePropertyBlob(blob);
                }
                drmModeFreeProperty(property);
            }
        }
        if (encoder) drmModeFreeEncoder(encoder);
        drmModeFreeConnector(connector);
    }
    if (!found) return fail("Active CRTC has no verifiable connected output");
    *result = hash;
    return 0;
}
static int get_gamma(Output *output, uint16_t *ramps) {
    return drmModeCrtcGetGamma(output->fd, output->crtc, output->points,
        ramps, ramps + output->points, ramps + output->points * 2);
}
static int set_gamma(Output *output, uint16_t *ramps) {
    return drmModeCrtcSetGamma(output->fd, output->crtc, output->points,
        ramps, ramps + output->points, ramps + output->points * 2);
}
static int capture(Snapshot *snapshot) {
    for (uint32_t card = 0; card < 64; ++card) {
        char path[64];
        snprintf(path, sizeof path, "/dev/dri/card%" PRIu32, card);
        int fd = open(path, O_RDWR | O_CLOEXEC | O_NOFOLLOW);
        if (fd < 0) { if (errno == ENOENT) continue; return fail("DRM device access unavailable"); }
        struct stat info;
        if (fstat(fd, &info) || !S_ISCHR(info.st_mode) || major(info.st_rdev) != 226) {
            close(fd); return fail("Unexpected DRM device identity");
        }
        if (!drmIsMaster(fd) && drmSetMaster(fd)) {
            close(fd); return fail("Display is owned by another graphics session");
        }
        drmModeRes *resources = drmModeGetResources(fd);
        if (!resources || resources->count_crtcs < 0 || resources->count_crtcs > 256) {
            if (resources) drmModeFreeResources(resources);
            close(fd); return fail("DRM mode resources unavailable");
        }
        for (int n = 0; n < resources->count_crtcs; ++n) {
            drmModeCrtc *crtc = drmModeGetCrtc(fd, resources->crtcs[n]);
            if (!crtc) { drmModeFreeResources(resources); close(fd); return fail("Unreadable CRTC"); }
            if (!crtc->mode_valid || !crtc->width || !crtc->height) { drmModeFreeCrtc(crtc); continue; }
            if (snapshot->count == MAX_OUTPUTS || crtc->gamma_size < 2 || crtc->gamma_size > MAX_POINTS) {
                drmModeFreeCrtc(crtc); drmModeFreeResources(resources); close(fd);
                return fail("An active output lacks supported gamma controls");
            }
            Output *output = &snapshot->outputs[snapshot->count++];
            output->fd = -1;
            output->card = card; output->crtc = crtc->crtc_id;
            output->width = crtc->width; output->height = crtc->height;
            output->points = (uint32_t)crtc->gamma_size;
            drmModeFreeCrtc(crtc);
            output->fd = dup(fd);
            output->ramps = calloc((size_t)output->points * 3, sizeof(uint16_t));
            if (output->fd < 0 || !output->ramps || identity(fd, resources, output->crtc, &output->topology) || get_gamma(output, output->ramps)) {
                drmModeFreeResources(resources); close(fd); return fail("Unable to capture every active output");
            }
        }
        drmModeFreeResources(resources);
        close(fd);
    }
    return snapshot->count ? 0 : fail("No active supported DRM display is available");
}
static int write_u32(uint32_t value) {
    unsigned char bytes[4] = {(unsigned char)value, (unsigned char)(value >> 8),
        (unsigned char)(value >> 16), (unsigned char)(value >> 24)};
    return fwrite(bytes, 1, 4, stdout) == 4 ? 0 : -1;
}
static int emit(const Snapshot *snapshot) {
    if (fwrite(MAGIC, 1, sizeof MAGIC, stdout) != sizeof MAGIC || write_u32(snapshot->count)) return -1;
    for (uint32_t n = 0; n < snapshot->count; ++n) {
        const Output *output = &snapshot->outputs[n];
        uint32_t fields[] = {output->card, output->crtc, output->width, output->height, output->points,
            (uint32_t)output->topology, (uint32_t)(output->topology >> 32)};
        for (size_t p = 0; p < sizeof fields / sizeof fields[0]; ++p) if (write_u32(fields[p])) return -1;
        for (uint32_t p = 0; p < output->points * 3; ++p) {
            unsigned char bytes[2] = {(unsigned char)output->ramps[p], (unsigned char)(output->ramps[p] >> 8)};
            if (fwrite(bytes, 1, 2, stdout) != 2) return -1;
        }
    }
    return fflush(stdout);
}
static uint32_t read_u32(const unsigned char *data) {
    return (uint32_t)data[0] | (uint32_t)data[1] << 8 | (uint32_t)data[2] << 16 | (uint32_t)data[3] << 24;
}
static int parse(Snapshot *snapshot) {
    unsigned char *data = malloc(MAX_BYTES + 1);
    if (!data) return fail("Snapshot allocation failed");
    size_t size = fread(data, 1, MAX_BYTES + 1, stdin), at = 12;
    int result = -1;
    if (ferror(stdin) || size < at || size > MAX_BYTES || memcmp(data, MAGIC, sizeof MAGIC)) goto done;
    uint32_t count = read_u32(data + 8);
    if (!count || count > MAX_OUTPUTS) goto done;
    for (uint32_t n = 0; n < count; ++n) {
        if (size - at < 28) goto done;
        Output *output = &snapshot->outputs[snapshot->count++];
        output->fd = -1;
        output->card = read_u32(data + at); output->crtc = read_u32(data + at + 4);
        output->width = read_u32(data + at + 8); output->height = read_u32(data + at + 12);
        output->points = read_u32(data + at + 16);
        output->topology = (uint64_t)read_u32(data + at + 20) | (uint64_t)read_u32(data + at + 24) << 32;
        at += 28;
        if (output->card > 63 || !output->crtc || !output->width || output->width > 32768 ||
            !output->height || output->height > 32768 || output->points < 2 || output->points > MAX_POINTS ||
            size - at < (size_t)output->points * 6) goto done;
        for (uint32_t p = 0; p < n; ++p)
            if (snapshot->outputs[p].card == output->card && snapshot->outputs[p].crtc == output->crtc) goto done;
        output->ramps = calloc((size_t)output->points * 3, sizeof(uint16_t));
        if (!output->ramps) goto done;
        for (uint32_t p = 0; p < output->points * 3; ++p, at += 2)
            output->ramps[p] = (uint16_t)((uint16_t)data[at] | (uint16_t)data[at + 1] << 8);
    }
    if (at == size) result = 0;
done:
    free(data);
    return result ? fail("Invalid or oversized display snapshot") : 0;
}
static Output *matching(Snapshot *snapshot, const Output *output) {
    for (uint32_t n = 0; n < snapshot->count; ++n) {
        Output *candidate = &snapshot->outputs[n];
        if (candidate->card == output->card && candidate->crtc == output->crtc &&
            candidate->width == output->width && candidate->height == output->height &&
            candidate->points == output->points && candidate->topology == output->topology) return candidate;
    }
    return NULL;
}
static int still_matching(Output *output) {
    drmModeRes *resources = drmModeGetResources(output->fd);
    drmModeCrtc *crtc = drmModeGetCrtc(output->fd, output->crtc);
    uint64_t topology = 0;
    int same = resources && crtc && crtc->mode_valid && crtc->width == output->width &&
        crtc->height == output->height && crtc->gamma_size == (int)output->points &&
        !identity(output->fd, resources, output->crtc, &topology) && topology == output->topology;
    if (resources) drmModeFreeResources(resources);
    if (crtc) drmModeFreeCrtc(crtc);
    return same;
}
static int apply(Snapshot *wanted, Snapshot *before) {
    if (wanted->count != before->count) return fail("Display topology changed; no colors applied");
    for (uint32_t n = 0; n < wanted->count; ++n)
        if (!matching(before, &wanted->outputs[n])) return fail("Display identity changed; no colors applied");
    int success = 1;
    uint16_t readback[MAX_POINTS * 3];
    for (uint32_t n = 0; n < wanted->count; ++n) {
        Output *desired = &wanted->outputs[n], *current = matching(before, desired);
        if (interrupted || set_gamma(current, desired->ramps) || get_gamma(current, readback) ||
            memcmp(readback, desired->ramps, (size_t)desired->points * 6)) { success = 0; break; }
    }
    for (uint32_t n = 0; n < before->count; ++n)
        if (!still_matching(&before->outputs[n])) success = 0;
    if (success && !interrupted) return 0;
    int restored = 1;
    for (uint32_t n = 0; n < before->count; ++n) {
        Output *output = &before->outputs[n];
        /* Never restore a previous monitor's calibration onto a changed output. */
        if (!still_matching(output)) { restored = 0; continue; }
        if (set_gamma(output, output->ramps) || get_gamma(output, readback) ||
            memcmp(readback, output->ramps, (size_t)output->points * 6)) restored = 0;
    }
    return fail(restored ? "Display adjustment failed; original ramps restored" :
        "Display adjustment failed; restoration incomplete, recovery timer required");
}
int main(int argc, char **argv) {
    if (geteuid() != 0) { fail("Display hardware access requires the authorized root manager"); return 1; }
    if (argc != 2 || (strcmp(argv[1], "capture") && strcmp(argv[1], "apply"))) {
        fail("Expected capture or apply, without device/path arguments"); return 2;
    }
    signal(SIGTERM, on_signal); signal(SIGINT, on_signal);
    Snapshot before = {0}, wanted = {0};
    int result;
    if (!strcmp(argv[1], "capture")) {
        result = capture(&before);
        if (!result) result = emit(&before);
    } else {
        result = parse(&wanted);
        if (!result) result = capture(&before);
        if (!result) result = apply(&wanted, &before);
        if (!result) puts("applied-and-readback-verified");
    }
    release(&wanted); release(&before);
    return result ? 1 : 0;
}
