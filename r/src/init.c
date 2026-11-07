
// clang-format sorts includes unless SortIncludes: Never. However, the ordering
// does matter here. So, we need to disable clang-format for safety.

// clang-format off
#include <stdint.h>
#include <Rinternals.h>
#include <R_ext/Parse.h>
// clang-format on

#include "rust/api.h"

static uintptr_t TAGGED_POINTER_MASK = (uintptr_t)1;

SEXP handle_result(SEXP res_) {
    uintptr_t res = (uintptr_t)res_;

    // An error is indicated by tag.
    if ((res & TAGGED_POINTER_MASK) == 1) {
        // Remove tag
        SEXP res_aligned = (SEXP)(res & ~TAGGED_POINTER_MASK);

        // Currently, there are two types of error cases:
        //
        //   1. Error from Rust code
        //   2. Error from R's C API, which is caught by R_UnwindProtect()
        //
        if (TYPEOF(res_aligned) == CHARSXP) {
            // In case 1, the result is an error message that can be passed to
            // Rf_errorcall() directly.
            Rf_errorcall(R_NilValue, "%s", CHAR(res_aligned));
        } else {
            // In case 2, the result is the token to restart the
            // cleanup process on R's side.
            R_ContinueUnwind(res_aligned);
        }
    }

    return (SEXP)res;
}

SEXP savvy_ffi_measure__impl(SEXP c_arg__plot, SEXP c_arg__scale, SEXP c_arg__names, SEXP c_arg__specs) {
    SEXP res = savvy_ffi_measure__ffi(c_arg__plot, c_arg__scale, c_arg__names, c_arg__specs);
    return handle_result(res);
}

SEXP savvy_ffi_option_names__impl(void) {
    SEXP res = savvy_ffi_option_names__ffi();
    return handle_result(res);
}

SEXP savvy_ffi_read_raw__impl(SEXP c_arg__path) {
    SEXP res = savvy_ffi_read_raw__ffi(c_arg__path);
    return handle_result(res);
}

SEXP savvy_ffi_run_deck__impl(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values, SEXP c_arg__tmax_divisor) {
    SEXP res = savvy_ffi_run_deck__ffi(c_arg__deck, c_arg__is_path, c_arg__include_names, c_arg__include_texts, c_arg__opt_names, c_arg__opt_values, c_arg__tmax_divisor);
    return handle_result(res);
}

SEXP savvy_ffi_sweep_deck__impl(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values, SEXP c_arg__tmax_divisor, SEXP c_arg__params, SEXP c_arg__values, SEXP c_arg__threads) {
    SEXP res = savvy_ffi_sweep_deck__ffi(c_arg__deck, c_arg__is_path, c_arg__include_names, c_arg__include_texts, c_arg__opt_names, c_arg__opt_values, c_arg__tmax_divisor, c_arg__params, c_arg__values, c_arg__threads);
    return handle_result(res);
}

SEXP savvy_ffi_write_raw__impl(SEXP c_arg__path, SEXP c_arg__plots, SEXP c_arg__binary) {
    SEXP res = savvy_ffi_write_raw__ffi(c_arg__path, c_arg__plots, c_arg__binary);
    return handle_result(res);
}

SEXP savvy_SpiceSession_alter__impl(SEXP self__, SEXP c_arg__refdes, SEXP c_arg__param, SEXP c_arg__value, SEXP c_arg__is_list) {
    SEXP res = savvy_SpiceSession_alter__ffi(self__, c_arg__refdes, c_arg__param, c_arg__value, c_arg__is_list);
    return handle_result(res);
}

SEXP savvy_SpiceSession_alterable__impl(SEXP self__, SEXP c_arg__refdes) {
    SEXP res = savvy_SpiceSession_alterable__ffi(self__, c_arg__refdes);
    return handle_result(res);
}

SEXP savvy_SpiceSession_open__impl(SEXP c_arg__deck, SEXP c_arg__is_path, SEXP c_arg__include_names, SEXP c_arg__include_texts, SEXP c_arg__opt_names, SEXP c_arg__opt_values, SEXP c_arg__tmax_divisor) {
    SEXP res = savvy_SpiceSession_open__ffi(c_arg__deck, c_arg__is_path, c_arg__include_names, c_arg__include_texts, c_arg__opt_names, c_arg__opt_values, c_arg__tmax_divisor);
    return handle_result(res);
}

SEXP savvy_SpiceSession_param__impl(SEXP self__, SEXP c_arg__refdes, SEXP c_arg__param) {
    SEXP res = savvy_SpiceSession_param__ffi(self__, c_arg__refdes, c_arg__param);
    return handle_result(res);
}

SEXP savvy_SpiceSession_run__impl(SEXP self__) {
    SEXP res = savvy_SpiceSession_run__ffi(self__);
    return handle_result(res);
}

SEXP savvy_SpiceSession_set_temp__impl(SEXP self__, SEXP c_arg__celsius) {
    SEXP res = savvy_SpiceSession_set_temp__ffi(self__, c_arg__celsius);
    return handle_result(res);
}

SEXP savvy_SpiceSession_set_tmax_divisor__impl(SEXP self__, SEXP c_arg__d) {
    SEXP res = savvy_SpiceSession_set_tmax_divisor__ffi(self__, c_arg__d);
    return handle_result(res);
}

SEXP savvy_SpiceSession_set_warm_start__impl(SEXP self__, SEXP c_arg__on) {
    SEXP res = savvy_SpiceSession_set_warm_start__ffi(self__, c_arg__on);
    return handle_result(res);
}

SEXP savvy_SpiceSession_status__impl(SEXP self__) {
    SEXP res = savvy_SpiceSession_status__ffi(self__);
    return handle_result(res);
}

SEXP savvy_SpiceSession_temp__impl(SEXP self__) {
    SEXP res = savvy_SpiceSession_temp__ffi(self__);
    return handle_result(res);
}

SEXP savvy_SpiceSession_title__impl(SEXP self__) {
    SEXP res = savvy_SpiceSession_title__ffi(self__);
    return handle_result(res);
}

SEXP savvy_SpiceSession_tmax_divisor__impl(SEXP self__) {
    SEXP res = savvy_SpiceSession_tmax_divisor__ffi(self__);
    return handle_result(res);
}

SEXP savvy_SpiceSession_warm_start__impl(SEXP self__) {
    SEXP res = savvy_SpiceSession_warm_start__ffi(self__);
    return handle_result(res);
}

SEXP savvy_SpiceSession_warm_stats__impl(SEXP self__) {
    SEXP res = savvy_SpiceSession_warm_stats__ffi(self__);
    return handle_result(res);
}


static const R_CallMethodDef CallEntries[] = {
    {"savvy_ffi_measure__impl", (DL_FUNC) &savvy_ffi_measure__impl, 4},
    {"savvy_ffi_option_names__impl", (DL_FUNC) &savvy_ffi_option_names__impl, 0},
    {"savvy_ffi_read_raw__impl", (DL_FUNC) &savvy_ffi_read_raw__impl, 1},
    {"savvy_ffi_run_deck__impl", (DL_FUNC) &savvy_ffi_run_deck__impl, 7},
    {"savvy_ffi_sweep_deck__impl", (DL_FUNC) &savvy_ffi_sweep_deck__impl, 10},
    {"savvy_ffi_write_raw__impl", (DL_FUNC) &savvy_ffi_write_raw__impl, 3},
    {"savvy_SpiceSession_alter__impl", (DL_FUNC) &savvy_SpiceSession_alter__impl, 5},
    {"savvy_SpiceSession_alterable__impl", (DL_FUNC) &savvy_SpiceSession_alterable__impl, 2},
    {"savvy_SpiceSession_open__impl", (DL_FUNC) &savvy_SpiceSession_open__impl, 7},
    {"savvy_SpiceSession_param__impl", (DL_FUNC) &savvy_SpiceSession_param__impl, 3},
    {"savvy_SpiceSession_run__impl", (DL_FUNC) &savvy_SpiceSession_run__impl, 1},
    {"savvy_SpiceSession_set_temp__impl", (DL_FUNC) &savvy_SpiceSession_set_temp__impl, 2},
    {"savvy_SpiceSession_set_tmax_divisor__impl", (DL_FUNC) &savvy_SpiceSession_set_tmax_divisor__impl, 2},
    {"savvy_SpiceSession_set_warm_start__impl", (DL_FUNC) &savvy_SpiceSession_set_warm_start__impl, 2},
    {"savvy_SpiceSession_status__impl", (DL_FUNC) &savvy_SpiceSession_status__impl, 1},
    {"savvy_SpiceSession_temp__impl", (DL_FUNC) &savvy_SpiceSession_temp__impl, 1},
    {"savvy_SpiceSession_title__impl", (DL_FUNC) &savvy_SpiceSession_title__impl, 1},
    {"savvy_SpiceSession_tmax_divisor__impl", (DL_FUNC) &savvy_SpiceSession_tmax_divisor__impl, 1},
    {"savvy_SpiceSession_warm_start__impl", (DL_FUNC) &savvy_SpiceSession_warm_start__impl, 1},
    {"savvy_SpiceSession_warm_stats__impl", (DL_FUNC) &savvy_SpiceSession_warm_stats__impl, 1},
    {NULL, NULL, 0}
};

void R_init_spice(DllInfo *dll) {
    R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
    R_useDynamicSymbols(dll, FALSE);

    // Functions for initialization, if any.

}
