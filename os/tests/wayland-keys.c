/* Test-only physical keys, through cage's virtual keyboard and an XKB Colemak map.
 * Compile with wayland-scanner's bindings for protocols/virtual-keyboard-unstable-v1.xml.
 * Nothing from this helper is installed in WrOS. */
#define _GNU_SOURCE
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include "virtual-keyboard.h"

static struct wl_seat *seat;
static struct zwp_virtual_keyboard_manager_v1 *manager;

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)data; (void)version;
    if (!strcmp(interface, "wl_seat"))
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
    if (!strcmp(interface, "zwp_virtual_keyboard_manager_v1"))
        manager = wl_registry_bind(registry, name, &zwp_virtual_keyboard_manager_v1_interface, 1);
}
static void removed(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data; (void)registry; (void)name;
}
static const struct wl_registry_listener listener = {global, removed};

int main(void) {
    struct wl_display *display = wl_display_connect(NULL);
    assert(display);
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &listener, NULL);
    assert(wl_display_roundtrip(display) >= 0 && seat && manager);
    struct xkb_context *context = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
    struct xkb_rule_names names = {.layout = "us", .variant = "colemak", .options = "compose:ralt"};
    struct xkb_keymap *map = xkb_keymap_new_from_names(context, &names, XKB_KEYMAP_COMPILE_NO_FLAGS);
    assert(map);
    char *text = xkb_keymap_get_as_string(map, XKB_KEYMAP_FORMAT_TEXT_V1);
    size_t size = strlen(text) + 1;
    int fd = memfd_create("wros-test-keymap", MFD_CLOEXEC);
    assert(fd >= 0 && write(fd, text, size) == (ssize_t)size);
    struct zwp_virtual_keyboard_v1 *keyboard = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(manager, seat);
    zwp_virtual_keyboard_v1_keymap(keyboard, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, size);
    /* Headless cage creates its keyboard seat on the first key. Prime it with
     * Shift so the window receives focus before the first writing character. */
    zwp_virtual_keyboard_v1_key(keyboard, 0, 42, WL_KEYBOARD_KEY_STATE_PRESSED);
    zwp_virtual_keyboard_v1_key(keyboard, 1, 42, WL_KEYBOARD_KEY_STATE_RELEASED);
    assert(wl_display_roundtrip(display) >= 0);
    usleep(200000);
    close(fd); free(text);
    puts("ready"); fflush(stdout);
    unsigned key, control, time = 2;
    while (scanf("%u %u", &key, &control) == 2) {
        assert(key <= 767 && control <= 1);
        uint32_t mods = control ? 1u << xkb_keymap_mod_get_index(map, XKB_MOD_NAME_CTRL) : 0;
        zwp_virtual_keyboard_v1_modifiers(keyboard, mods, 0, 0, 0);
        zwp_virtual_keyboard_v1_key(keyboard, time++, key, WL_KEYBOARD_KEY_STATE_PRESSED);
        zwp_virtual_keyboard_v1_key(keyboard, time++, key, WL_KEYBOARD_KEY_STATE_RELEASED);
        zwp_virtual_keyboard_v1_modifiers(keyboard, 0, 0, 0, 0);
        assert(wl_display_roundtrip(display) >= 0);
        puts("sent"); fflush(stdout);
    }
    zwp_virtual_keyboard_v1_destroy(keyboard);
    wl_display_flush(display);
    wl_display_disconnect(display);
    xkb_keymap_unref(map); xkb_context_unref(context);
    return 0;
}
