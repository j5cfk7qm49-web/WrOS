/* A colour-only protocol client for the actual Cage integration regression. */
#define _GNU_SOURCE
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>

static const struct wl_interface control_interface;
static const struct wl_interface *get_types[] = {&control_interface, &wl_output_interface};
static const struct wl_message manager_requests[] = {{"get_gamma_control", "no", get_types}, {"destroy", "", NULL}};
static const struct wl_interface manager_interface = {"zwlr_gamma_control_manager_v1", 1, 2, manager_requests, 0, NULL};
static const struct wl_message control_requests[] = {{"set_gamma", "h", NULL}, {"destroy", "", NULL}};
static const struct wl_message control_events[] = {{"gamma_size", "u", NULL}, {"failed", "", NULL}};
static const struct wl_interface control_interface = {"zwlr_gamma_control_v1", 1, 2, control_requests, 2, control_events};

struct state { struct wl_proxy *manager; struct wl_output *output; uint32_t size; bool failed; };
static void global(void *data, struct wl_registry *registry, uint32_t name, const char *interface, uint32_t version) {
    (void)version;
    struct state *state = data;
    if (!strcmp(interface, manager_interface.name)) state->manager = wl_registry_bind(registry, name, &manager_interface, 1);
    if (!strcmp(interface, "wl_output") && !state->output) state->output = wl_registry_bind(registry, name, &wl_output_interface, 1);
}
static void removed(void *data, struct wl_registry *registry, uint32_t name) { (void)data; (void)registry; (void)name; }
static const struct wl_registry_listener registry_listener = {global, removed};
static void gamma_size(void *data, struct wl_proxy *control, uint32_t size) { (void)control; ((struct state *)data)->size = size; }
static void failed(void *data, struct wl_proxy *control) { (void)control; ((struct state *)data)->failed = true; }
static void (*control_listener[])(void) = {(void (*)(void))gamma_size, (void (*)(void))failed};
static void frame_wait(struct wl_display *display) {
    if (wl_display_roundtrip(display) < 0) exit(8);
    struct timespec delay = {.tv_nsec = 250000000};
    nanosleep(&delay, NULL);
    if (wl_display_roundtrip(display) < 0) exit(9);
}
int main(int argc, char **argv) {
    struct state state = {0};
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 2;
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, &state);
    if (wl_display_roundtrip(display) < 0 || !state.output || !state.manager) return 3;
    struct wl_proxy *control = wl_proxy_marshal_flags(state.manager, 0, &control_interface, 1, 0, NULL, state.output);
    wl_proxy_add_listener(control, control_listener, &state);
    if (wl_display_roundtrip(display) < 0 || state.failed || state.size != 256) return 4;
    int fd = memfd_create("cage-gamma-test", MFD_CLOEXEC);
    size_t bytes = state.size * 3 * sizeof(uint16_t);
    if (fd < 0 || ftruncate(fd, (off_t)bytes)) return 5;
    uint16_t *ramps = mmap(NULL, bytes, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (ramps == MAP_FAILED) return 6;
    for (uint32_t i = 0; i < state.size; ++i) {
        ramps[i] = i * 257;
        ramps[state.size + i] = i * 200;
        ramps[state.size * 2 + i] = i * 100;
    }
    wl_proxy_marshal_flags(control, 0, NULL, 1, 0, fd);
    munmap(ramps, bytes);
    close(fd);
    frame_wait(display);
    bool rejected = argc == 2 && !strcmp(argv[1], "reject");
    if (state.failed != rejected) return 7;
    if (state.failed) wl_proxy_destroy(control);
    else wl_proxy_marshal_flags(control, 1, NULL, 1, WL_MARSHAL_FLAG_DESTROY);
    frame_wait(display);
    wl_display_disconnect(display);
    return 0;
}
