/*
 * C smoke test of the dune-ffi ABI (compiled and run by tests/c_smoke.rs).
 *
 *   smoke <path/to/rc.cir>
 *
 * Simulates the 3f5 RC example from disk, prints v(2) at the last
 * time point and checks it is 1 - e^-7 within the integration error;
 * then exercises the in-memory path, a parse error and NULL handling.
 * Exits 0 on success.
 */
#include <math.h>
#include <stdio.h>
#include <string.h>

#include "dune.h"

#define CHECK(cond)                                                        \
    do {                                                                   \
        if (!(cond)) {                                                     \
            fprintf(stderr, "smoke.c:%d: check failed: %s\n", __LINE__, #cond); \
            return 1;                                                      \
        }                                                                  \
    } while (0)

static long find_var(const DuneResult *r, size_t plot, const char *name) {
    size_t n = dune_plot_variable_count(r, plot);
    for (size_t v = 0; v < n; v++) {
        if (strcmp(dune_plot_variable_name(r, plot, v), name) == 0) return (long)v;
    }
    return -1;
}

int main(int argc, char **argv) {
    CHECK(argc == 2);
    printf("dune %s\n", dune_version());

    /* 1. A deck from disk. */
    DuneResult *r = dune_simulate_file(argv[1]);
    if (!dune_result_ok(r)) {
        fprintf(stderr, "error (%d): %s\n", (int)dune_result_error_kind(r),
                dune_result_error_message(r));
        dune_result_free(r);
        return 1;
    }
    CHECK(dune_result_plot_count(r) == 1);
    printf("plot 0: %s / %s, %zu variables, %zu points\n", dune_plot_title(r, 0),
           dune_plot_name(r, 0), dune_plot_variable_count(r, 0), dune_plot_npoints(r, 0));
    ptrdiff_t scale = dune_plot_scale_index(r, 0);
    CHECK(scale >= 0);
    long v2 = find_var(r, 0, "v(2)");
    CHECK(v2 >= 0);
    CHECK(strcmp(dune_plot_variable_type(r, 0, (size_t)v2), "voltage") == 0);

    const double *t = NULL, *v = NULL;
    size_t nt = 0, nv = 0;
    CHECK(dune_plot_column_real(r, 0, (size_t)scale, &t, &nt));
    CHECK(dune_plot_column_real(r, 0, (size_t)v2, &v, &nv));
    CHECK(nt == nv && nv == dune_plot_npoints(r, 0) && nv > 0);
    double last = v[nv - 1];
    printf("v(2) at t=%g: %.6f\n", t[nt - 1], last);
    CHECK(fabs(t[nt - 1] - 7.0) < 1e-9);
    CHECK(fabs(last - (1.0 - exp(-7.0))) < 2e-3);
    dune_result_free(r);

    /* 2. Text deck, AC, complex column as interleaved re,im. */
    r = dune_simulate("lp\nv1 1 0 ac 1\nr1 1 2 1\nc1 2 0 1\n.ac lin 1 1 1\n.end\n");
    CHECK(dune_result_ok(r));
    CHECK(dune_plot_is_complex(r, 0));
    v2 = find_var(r, 0, "v(2)");
    CHECK(v2 >= 0);
    CHECK(dune_plot_column_complex(r, 0, (size_t)v2, &v, &nv));
    CHECK(nv == 2);
    double w = 2.0 * 3.14159265358979323846, den = 1.0 + w * w;
    printf("ac v(2) at 1 Hz: %.6f %+.6fj\n", v[0], v[1]);
    CHECK(fabs(v[0] - 1.0 / den) < 1e-9 && fabs(v[1] + w / den) < 1e-9);
    dune_result_free(r);

    /* 3. A parse error with its position. */
    r = dune_simulate("t\nv1 1 0 dc 1\nq1 1 1 0\n.op\n.end\n");
    CHECK(!dune_result_ok(r));
    CHECK(dune_result_error_kind(r) == DUNE_ERROR_KIND_PARSE);
    printf("parse error: %s\n", dune_result_error_message(r));
    for (size_t i = 0; i < dune_result_diagnostic_count(r); i++) {
        printf("  diag %zu: %s:%u:%u %s\n", i, dune_result_diagnostic_file(r, i),
               dune_result_diagnostic_line(r, i), dune_result_diagnostic_column(r, i),
               dune_result_diagnostic_message(r, i));
    }
    dune_result_free(r);

    /* 4. NULL everywhere. */
    r = dune_simulate(NULL);
    CHECK(dune_result_error_kind(r) == DUNE_ERROR_KIND_INPUT);
    dune_result_free(r);
    CHECK(!dune_result_ok(NULL));
    CHECK(dune_plot_name(NULL, 0) == NULL);
    CHECK(!dune_plot_column_real(NULL, 0, 0, &v, &nv) && v == NULL && nv == 0);
    dune_result_free(NULL);

    printf("ok\n");
    return 0;
}
