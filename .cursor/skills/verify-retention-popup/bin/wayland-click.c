#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <linux/input-event-codes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>

static const struct wl_message pointer_requests[] = {
    {"motion", "uff", NULL},
    {"motion_absolute", "uuuuu", NULL},
    {"button", "uuu", NULL},
    {"axis", "uuf", NULL},
    {"frame", "", NULL},
    {"axis_source", "u", NULL},
    {"axis_stop", "uu", NULL},
    {"axis_discrete", "uufi", NULL},
    {"destroy", "", NULL},
};

static const struct wl_interface zwlr_virtual_pointer_v1_interface = {
    "zwlr_virtual_pointer_v1", 2, 9, pointer_requests, 0, NULL,
};

static const struct wl_interface *manager_types[] = {
    &wl_seat_interface,
    &zwlr_virtual_pointer_v1_interface,
    &wl_seat_interface,
    &wl_output_interface,
    &zwlr_virtual_pointer_v1_interface,
};

static const struct wl_message manager_requests[] = {
    {"create_virtual_pointer", "?on", manager_types + 0},
    {"destroy", "", NULL},
    {"create_virtual_pointer_with_output", "?o?on", manager_types + 2},
};

static const struct wl_interface zwlr_virtual_pointer_manager_v1_interface = {
    "zwlr_virtual_pointer_manager_v1", 2, 3, manager_requests, 0, NULL,
};

struct output_node {
    struct wl_output *output;
    char name[128];
    struct output_node *next;
};

struct client {
    struct wl_seat *seat;
    struct output_node *outputs;
    struct wl_proxy *manager;
    uint32_t manager_version;
};

static void seat_capabilities(void *data, struct wl_seat *seat, uint32_t capabilities) {
    (void)data;
    (void)seat;
    (void)capabilities;
}

static void seat_name(void *data, struct wl_seat *seat, const char *name) {
    (void)data;
    (void)seat;
    (void)name;
}

static const struct wl_seat_listener seat_listener = {seat_capabilities, seat_name};

static void output_geometry(void *data, struct wl_output *output, int32_t x, int32_t y,
                            int32_t physical_width, int32_t physical_height, int32_t subpixel,
                            const char *make, const char *model, int32_t transform) {
    (void)data;
    (void)output;
    (void)x;
    (void)y;
    (void)physical_width;
    (void)physical_height;
    (void)subpixel;
    (void)make;
    (void)model;
    (void)transform;
}

static void output_mode(void *data, struct wl_output *output, uint32_t flags, int32_t width,
                        int32_t height, int32_t refresh) {
    (void)data;
    (void)output;
    (void)flags;
    (void)width;
    (void)height;
    (void)refresh;
}

static void output_done(void *data, struct wl_output *output) {
    (void)data;
    (void)output;
}

static void output_scale(void *data, struct wl_output *output, int32_t factor) {
    (void)data;
    (void)output;
    (void)factor;
}

static void output_name(void *data, struct wl_output *output, const char *name) {
    struct output_node *node = data;
    (void)output;
    snprintf(node->name, sizeof node->name, "%s", name);
}

static void output_description(void *data, struct wl_output *output, const char *description) {
    (void)data;
    (void)output;
    (void)description;
}

static const struct wl_output_listener output_listener = {
    output_geometry, output_mode, output_done, output_scale, output_name, output_description,
};

static void pause_ms(unsigned ms) {
    struct timespec req = {.tv_sec = ms / 1000, .tv_nsec = (long)(ms % 1000) * 1000000L};
    nanosleep(&req, NULL);
}

static void registry_global(void *data, struct wl_registry *registry, uint32_t name,
                            const char *interface, uint32_t version) {
    struct client *client = data;
    if (strcmp(interface, "wl_seat") == 0 && client->seat == NULL) {
        client->seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
        wl_seat_add_listener(client->seat, &seat_listener, NULL);
    } else if (strcmp(interface, "wl_output") == 0) {
        uint32_t bound = version < 4 ? version : 4;
        struct output_node *node = calloc(1, sizeof *node);
        if (node == NULL) {
            return;
        }
        node->output = wl_registry_bind(registry, name, &wl_output_interface, bound);
        node->next = client->outputs;
        client->outputs = node;
        wl_output_add_listener(node->output, &output_listener, node);
    } else if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0 &&
               client->manager == NULL) {
        uint32_t bound = version < 2 ? version : 2;
        client->manager_version = bound;
        client->manager = wl_registry_bind(registry, name, &zwlr_virtual_pointer_manager_v1_interface, bound);
    }
}

static void registry_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {registry_global, registry_remove};

static void pointer_request(struct wl_proxy *pointer, uint32_t opcode, uint32_t a, uint32_t b,
                            uint32_t c, uint32_t d, uint32_t e) {
    if (opcode == 1) {
        wl_proxy_marshal(pointer, opcode, a, b, c, d, e);
    } else if (opcode == 2) {
        wl_proxy_marshal(pointer, opcode, a, b, c);
    } else if (opcode == 4) {
        wl_proxy_marshal(pointer, opcode);
    }
}

int main(int argc, char **argv) {
    if (argc != 7) {
        fprintf(stderr, "usage: wayland-click move|click x y width height output\n");
        return 2;
    }
    int clicking = strcmp(argv[1], "click") == 0;
    if (!clicking && strcmp(argv[1], "move") != 0) {
        fprintf(stderr, "usage: wayland-click move|click x y width height output\n");
        return 2;
    }
    uint32_t x = (uint32_t)strtoul(argv[2], NULL, 10);
    uint32_t y = (uint32_t)strtoul(argv[3], NULL, 10);
    uint32_t width = (uint32_t)strtoul(argv[4], NULL, 10);
    uint32_t height = (uint32_t)strtoul(argv[5], NULL, 10);
    const char *output_name = argv[6];
    if (width == 0 || height == 0 || x >= width || y >= height) {
        fprintf(stderr, "point %u,%u is outside 0,0 %ux%u\n", x, y, width, height);
        return 2;
    }

    struct wl_display *display = wl_display_connect(NULL);
    if (display == NULL) {
        fprintf(stderr, "wl_display_connect failed: %s\n", strerror(errno));
        return 1;
    }
    struct client client = {0};
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, &client);
    wl_display_roundtrip(display);
    wl_display_roundtrip(display);
    struct output_node *chosen = NULL;
    for (struct output_node *node = client.outputs; node != NULL; node = node->next) {
        if (strcmp(node->name, output_name) == 0) {
            chosen = node;
        }
    }
    if (client.manager == NULL || client.seat == NULL || chosen == NULL) {
        fprintf(stderr, "missing seat, zwlr_virtual_pointer_manager_v1, or output %s\n", output_name);
        for (struct output_node *node = client.outputs; node != NULL; node = node->next) {
            fprintf(stderr, "output %s\n", node->name[0] ? node->name : "(unnamed)");
        }
        return 1;
    }

    struct wl_proxy *pointer = NULL;
    if (client.manager_version >= 2) {
        pointer = wl_proxy_marshal_constructor(client.manager, 2, &zwlr_virtual_pointer_v1_interface,
                                               client.seat, chosen->output, NULL);
    } else {
        pointer = wl_proxy_marshal_constructor(client.manager, 0, &zwlr_virtual_pointer_v1_interface,
                                               client.seat, NULL);
    }
    if (pointer == NULL) {
        fprintf(stderr, "create_virtual_pointer failed\n");
        return 1;
    }
    wl_display_roundtrip(display);

    uint32_t now = (uint32_t)(clock() / (CLOCKS_PER_SEC / 1000));
    pointer_request(pointer, 1, now, x, y, width, height);
    pointer_request(pointer, 4, 0, 0, 0, 0, 0);
    wl_display_flush(display);
    if (clicking) {
        pause_ms(30);
        pointer_request(pointer, 2, now, BTN_LEFT, 1, 0, 0);
        pointer_request(pointer, 4, 0, 0, 0, 0, 0);
        wl_display_flush(display);
        pause_ms(30);
        pointer_request(pointer, 2, now + 1, BTN_LEFT, 0, 0, 0);
        pointer_request(pointer, 4, 0, 0, 0, 0, 0);
        wl_display_flush(display);
    }
    pause_ms(50);
    wl_proxy_marshal(pointer, 8);
    wl_display_flush(display);
    wl_display_disconnect(display);
    printf("ok %s %u %u\n", clicking ? "click" : "move", x, y);
    return 0;
}
