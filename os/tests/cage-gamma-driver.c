/* Test-only wlroots headless gamma capability. Never installed in WrOS. */
#define _GNU_SOURCE
#define WLR_USE_UNSTABLE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <wlr/types/wlr_output.h>

size_t wlr_output_get_gamma_size(struct wlr_output *output) {
    (void)output;
    return 256;
}

bool wlr_output_test_state(struct wlr_output *output, const struct wlr_output_state *state) {
    static bool (*test)(struct wlr_output *, const struct wlr_output_state *);
    if (!test) test = dlsym(RTLD_NEXT, "wlr_output_test_state");
    struct wlr_output_state filtered = *state;
    filtered.committed &= ~WLR_OUTPUT_STATE_GAMMA_LUT;
    return test(output, &filtered);
}

bool wlr_output_commit_state(struct wlr_output *output, const struct wlr_output_state *state) {
    static bool (*commit)(struct wlr_output *, const struct wlr_output_state *);
    if (!commit) commit = dlsym(RTLD_NEXT, "wlr_output_commit_state");
    struct wlr_output_state filtered = *state;
    if (state->committed & WLR_OUTPUT_STATE_GAMMA_LUT) {
        FILE *trace = fopen(getenv("WROS_CAGE_GAMMA_TRACE"), "a");
        if (!trace) abort();
        size_t n = state->gamma_lut_size;
        fprintf(trace, "%s %zu", output->name, n);
        if (n) fprintf(trace, " %u %u %u", state->gamma_lut[n - 1],
                       state->gamma_lut[n * 2 - 1], state->gamma_lut[n * 3 - 1]);
        fputc('\n', trace);
        fclose(trace);
        if (n && getenv("WROS_CAGE_GAMMA_REJECT")) return false;
        filtered.committed &= ~WLR_OUTPUT_STATE_GAMMA_LUT;
    }
    return commit(output, &filtered);
}
