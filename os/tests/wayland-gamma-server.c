/* Test-only colour server. No real display, input, book or network access.
 * SPDX-License-Identifier: MIT
 */
#define _GNU_SOURCE
#include <wayland-server.h>
#include "gamma-server.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>

struct screen { int id; struct wl_global *global; };
struct control { struct screen *screen; struct wl_resource *resource; };
static struct wl_display *display;
static FILE *logfile;
static const char *mode;
static struct screen screens[2];

static void destroy_resource(struct wl_client *client, struct wl_resource *resource) {
    (void)client;
    wl_resource_destroy(resource);
}

static void control_destroyed(struct wl_resource *resource) {
    struct control *control = wl_resource_get_user_data(resource);
    fprintf(logfile, "restore %d\n", control->screen->id);
    fflush(logfile);
    free(control);
}

static void set_gamma(struct wl_client *client, struct wl_resource *resource, int32_t fd) {
    struct control *control = wl_resource_get_user_data(resource);
    struct stat info;
    uint16_t values[3 * 256];
    (void)client;
    if (fstat(fd, &info) || info.st_size != (off_t)sizeof(values) ||
        pread(fd, values, sizeof(values), 0) != (ssize_t)sizeof(values)) {
        close(fd);
        wl_resource_post_error(resource, ZWLR_GAMMA_CONTROL_V1_ERROR_INVALID_GAMMA, "bad ramp");
        return;
    }
    close(fd);
    fprintf(logfile, "set %d %u %u %u\n", control->screen->id, values[255], values[511], values[767]);
    fflush(logfile);
    if (control->screen->id == 1) {
        if (!strcmp(mode, "partial")) zwlr_gamma_control_v1_send_failed(resource);
        if (!strcmp(mode, "remove")) wl_global_remove(screens[0].global);
        if (!strcmp(mode, "disconnect")) wl_display_terminate(display);
    }
}

static const struct zwlr_gamma_control_v1_interface control_impl = {
    .set_gamma = set_gamma, .destroy = destroy_resource
};

static void get_control(struct wl_client *client, struct wl_resource *manager,
                        uint32_t id, struct wl_resource *output) {
    (void)manager;
    struct control *control = calloc(1, sizeof(*control));
    if (!control) { wl_client_post_no_memory(client); return; }
    control->screen = wl_resource_get_user_data(output);
    control->resource = wl_resource_create(client, &zwlr_gamma_control_v1_interface, 1, id);
    if (!control->resource) { free(control); wl_client_post_no_memory(client); return; }
    wl_resource_set_implementation(control->resource, &control_impl, control, control_destroyed);
    if (!strcmp(mode, "unsupported")) zwlr_gamma_control_v1_send_failed(control->resource);
    else if (!strcmp(mode, "oversized")) zwlr_gamma_control_v1_send_gamma_size(control->resource, 4097);
    else if (strcmp(mode, "missing-size")) zwlr_gamma_control_v1_send_gamma_size(control->resource, 256);
}

static const struct zwlr_gamma_control_manager_v1_interface manager_impl = {
    .get_gamma_control = get_control, .destroy = destroy_resource
};

static void bind_manager(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
    (void)data; (void)version;
    struct wl_resource *resource = wl_resource_create(client, &zwlr_gamma_control_manager_v1_interface, 1, id);
    wl_resource_set_implementation(resource, &manager_impl, NULL, NULL);
}

static const struct wl_output_interface output_impl = { .release = destroy_resource };
static void bind_output(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
    (void)version;
    struct wl_resource *resource = wl_resource_create(client, &wl_output_interface, 1, id);
    wl_resource_set_implementation(resource, &output_impl, data, NULL);
}

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    mode = argv[1];
    logfile = fopen(argv[2], "w");
    if (!logfile) return 3;
    display = wl_display_create();
    if (!display || wl_display_add_socket(display, "gamma-test")) return 4;
    for (int i = 0; i < 2; i++) {
        screens[i].id = i;
        screens[i].global = wl_global_create(display, &wl_output_interface, 1, &screens[i], bind_output);
    }
    if (strcmp(mode, "absent"))
        wl_global_create(display, &zwlr_gamma_control_manager_v1_interface, 1, NULL, bind_manager);
    puts("ready"); fflush(stdout);
    wl_display_run(display);
    wl_display_destroy_clients(display);
    wl_display_destroy(display);
    fclose(logfile);
    return 0;
}
